"""Atomic immutable result recovery on fake roots only, with zero model calls."""
import copy
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch

from semantic_caller import CALLER_SHA, MANIFEST_SHA, MODEL, Held, NativeResultReceipts, run
from test_semantic_caller import request


def result_for(raw, request_id='synthetic-1'):
    return {'schema': 'jev.semantic-result.v1', 'requestId': request_id,
            'purpose': 'semantic_bundle', 'stateDigest': 'a' * 64,
            'submittedRequestSha256': hashlib.sha256(raw).hexdigest(),
            'requestedModel': MODEL, 'returnedModel': MODEL, 'status': 'ok', 'errorCode': None,
            'transportInvocations': 1, 'connectionLeases': 1, 'answers': {}, 'usage': {'input_tokens': 50},
            'sourcePostconditionVerified': True, 'executionAuthorityGranted': False,
            'automaticWrite': False, 'workerCompletionVerified': False}


def competing_publication(root, uid, gid, value, barrier, results):
    try:
        owner = NativeResultReceipts(root, uid=uid, gid=gid)
        barrier.wait(timeout=5)
        results.put(owner.publish(value)['submittedRequestSha256'])
    except Exception as error:
        results.put(type(error).__name__)


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.owner = NativeResultReceipts(self.root, uid=os.getuid(), gid=os.getgid())
        self.raw = json.dumps(request()).encode()
        self.result = result_for(self.raw)

    def tearDown(self):
        self.temp.cleanup()

    def test_publish_regular_readonly_single_link_and_identical_reuse(self):
        got = self.owner.publish(self.result)
        target = self.root / 'semantic-results/synthetic-1.json'
        meta = target.stat()
        self.assertEqual(got, self.result)
        self.assertEqual(stat.S_IMODE(meta.st_mode), 0o444)
        self.assertEqual(meta.st_nlink, 1)
        self.assertEqual(stat.S_IMODE(target.parent.stat().st_mode), 0o700)
        inode = meta.st_ino
        self.assertEqual(self.owner.publish(copy.deepcopy(self.result)), self.result)
        self.assertEqual(target.stat().st_ino, inode)

    def test_conflicting_same_request_never_overwrites(self):
        self.owner.publish(self.result)
        target = self.root / 'semantic-results/synthetic-1.json'
        original = target.read_bytes()
        changed = {**self.result, 'submittedRequestSha256': 'b' * 64}
        with self.assertRaisesRegex(Held, 'native_receipt_conflict'):
            self.owner.publish(changed)
        with self.assertRaisesRegex(Held, 'native_receipt_conflict'):
            self.owner.read('synthetic-1', 'b' * 64, 'a' * 64)
        self.assertEqual(target.read_bytes(), original)

    def test_read_existing_absent_directory_has_no_creation_or_sync_effect(self):
        before = self.root.stat()
        with patch('semantic_caller.os.mkdir', side_effect=AssertionError('read must not create')), \
                patch('semantic_caller.os.fsync', side_effect=AssertionError('read must not sync')):
            self.assertIsNone(self.owner.read_existing('synthetic-1',
                              self.result['submittedRequestSha256'], 'a' * 64))
        after = self.root.stat()
        self.assertEqual(list(self.root.iterdir()), [])
        self.assertEqual((before.st_ino, before.st_mtime_ns, before.st_ctime_ns),
                         (after.st_ino, after.st_mtime_ns, after.st_ctime_ns))

    def test_read_existing_preserves_original_guards_and_correlation_without_writes(self):
        self.owner.publish(self.result)
        target = self.root / 'semantic-results/synthetic-1.json'
        original = target.read_bytes()
        with patch('semantic_caller.os.mkdir', side_effect=AssertionError('read must not create')), \
                patch('semantic_caller.os.fsync', side_effect=AssertionError('read must not sync')):
            self.assertEqual(self.owner.read_existing('synthetic-1',
                             self.result['submittedRequestSha256'], 'a' * 64), self.result)
            self.assertIsNone(self.owner.read_existing('synthetic-absent', 'b' * 64, 'a' * 64))
            with self.assertRaisesRegex(Held, 'native_receipt_conflict'):
                self.owner.read_existing('synthetic-1', 'b' * 64, 'a' * 64)
            target.chmod(0o644)
            with self.assertRaisesRegex(Held, 'native_receipt_metadata_changed'):
                self.owner.read_existing('synthetic-1', self.result['submittedRequestSha256'], 'a' * 64)
        self.assertEqual(target.read_bytes(), original)

    def test_rename_failure_preserves_complete_pending_and_no_partial_final(self):
        with patch.object(self.owner, 'rename', return_value=-1):
            with self.assertRaisesRegex(Held, 'native_receipt_publication_unknown'):
                self.owner.publish(self.result)
        folder = self.root / 'semantic-results'
        self.assertFalse((folder / 'synthetic-1.json').exists())
        pending = list(folder.glob('.pending-synthetic-1-*'))
        self.assertEqual(len(pending), 1)
        self.assertEqual(json.loads(pending[0].read_bytes()), self.result)

    def test_symlink_hardlink_wrong_mode_and_rawstate_refuse(self):
        self.owner.ready()
        target = self.root / 'semantic-results/synthetic-1.json'
        outside = self.root / 'outside.json'
        outside.write_text(json.dumps(self.result))
        target.symlink_to(outside)
        with self.assertRaises(OSError):
            self.owner.read('synthetic-1', self.result['submittedRequestSha256'], 'a' * 64)
        target.unlink()
        self.owner.publish(self.result)
        alias = self.root / 'alias.json'
        alias.hardlink_to(target)
        with self.assertRaisesRegex(Held, 'native_receipt_metadata_changed'):
            self.owner.read('synthetic-1', self.result['submittedRequestSha256'], 'a' * 64)
        alias.unlink()
        target.chmod(0o644)
        with self.assertRaisesRegex(Held, 'native_receipt_metadata_changed'):
            self.owner.read('synthetic-1', self.result['submittedRequestSha256'], 'a' * 64)
        with self.assertRaisesRegex(Held, 'technical_result_only'):
            self.owner.publish({**self.result, 'state': {'raw': 'SYNTHETIC_NOT_ALLOWED'}})

    def test_same_original_result_returns_without_native_import_or_http(self):
        self.owner.publish(self.result)
        checks = []

        def verifier():
            checks.append('verified')
            return {'status': 'successor-source-verified', 'callerSha256': CALLER_SHA,
                    'manifestSha256': MANIFEST_SHA}

        with patch('semantic_caller.NativeResultReceipts', return_value=self.owner), \
                patch('semantic_caller.os.getuid', return_value=10000), \
                patch('semantic_caller.os.getgid', return_value=10000), \
                patch.dict(os.environ, {'HOME': '/opt/data', 'HERMES_HOME': '/opt/data'}), \
                patch('semantic_caller.importlib.util.spec_from_file_location') as imports:
            got = run(self.raw, descriptor_verify=verifier)
        self.assertEqual(got, self.result)
        self.assertEqual(checks, ['verified', 'verified'])
        imports.assert_not_called()

    def test_two_processes_publish_one_identical_immutable_result(self):
        self.owner.ready()
        context = multiprocessing.get_context('fork')
        barrier, outputs = context.Barrier(2), context.Queue()
        workers = [context.Process(target=competing_publication,
                   args=(str(self.root), os.getuid(), os.getgid(), self.result, barrier, outputs))
                   for _ in range(2)]
        for worker in workers:
            worker.start()
        observed = [outputs.get(timeout=10) for _ in workers]
        for worker in workers:
            worker.join(timeout=10)
            self.assertEqual(worker.exitcode, 0)
        self.assertEqual(observed, [self.result['submittedRequestSha256']] * 2)
        self.assertEqual(self.owner.read('synthetic-1', self.result['submittedRequestSha256'], 'a' * 64), self.result)
        self.assertEqual((self.root / 'semantic-results/synthetic-1.json').stat().st_nlink, 1)


if __name__ == '__main__':
    unittest.main()
