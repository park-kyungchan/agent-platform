"""Finite semantic transport using the existing source-pinned native Jev owner.

The host reserves the shared budget and verifies fixed Docker/source metadata.
This process owns neither the host ledger nor native sessions. It emits closed
typed judgments; the same native Lead retains writes and completion authority.
"""
from __future__ import annotations

import contextlib
import ctypes
import errno
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import secrets
import stat
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path('/opt/data/runtime/harness/jev-1.14')
CALLER_SHA = 'dd72953016d12fdc730235dece88e635b58573042544b77e890b5d64566af71d'
MANIFEST_SHA = '72294eb10b15de0e64d5551da668a15173cfe66ba7314b9c5356d92703a2d39f'
MODEL = 'typesafe-ai/jev-1.14.0'
LABEL = re.compile(r'[A-Za-z0-9_.:-]{1,160}\Z')
HEX = re.compile(r'[0-9a-f]{64}\Z')
MAX_WIRE_BYTES = 49152
RESULT_FIELDS = {'schema', 'requestId', 'purpose', 'stateDigest', 'submittedRequestSha256',
                 'requestedModel', 'returnedModel', 'status', 'errorCode', 'answers', 'usage',
                 'transportInvocations', 'connectionLeases', 'callerLatencyMs',
                 'sourcePostconditionVerified', 'descriptorSourceBefore', 'descriptorSourceAfter',
                 'sourcePins', 'wire', 'executionAuthorityGranted', 'automaticWrite',
                 'workerCompletionVerified'}


class NativeResultReceipts:
    """Immutable technical inference results at one native-owned fixed root.

    No state/prompt, credential, session replica, dispatch loop, or retry record is
    stored. Linux renameat2(NOREPLACE) publishes a complete fsynced regular file
    without overwriting a concurrent/third-state receipt or exposing partial JSON.
    """
    def __init__(self, root=ROOT, *, uid=10000, gid=10000):
        self.root, self.uid, self.gid = Path(root), uid, gid
        self.rename = getattr(ctypes.CDLL(None, use_errno=True), 'renameat2', None)
        need(self.rename is not None, 'atomic_receipt_publication_unsupported')
        self.rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        self.rename.restype = ctypes.c_int

    @contextlib.contextmanager
    def directory(self, *, create=True):
        need(self.root.is_absolute() and self.root.resolve() == self.root, 'exact_receipt_root_required')
        parent = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        child = None
        try:
            meta = os.fstat(parent)
            current = self.root.lstat()
            need((meta.st_dev, meta.st_ino) == (current.st_dev, current.st_ino)
                 and meta.st_uid == self.uid and meta.st_gid == self.gid, 'receipt_root_owner_changed')
            if create:
                try:
                    os.mkdir('semantic-results', mode=0o700, dir_fd=parent)
                    os.fsync(parent)
                except FileExistsError:
                    pass
            try:
                child = os.open('semantic-results', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                dir_fd=parent)
            except FileNotFoundError:
                if create:
                    raise
                yield None
                return
            owned = os.fstat(child)
            need(owned.st_uid == self.uid and owned.st_gid == self.gid
                 and stat.S_IMODE(owned.st_mode) == 0o700, 'receipt_directory_owner_or_mode')
            yield child
        finally:
            if child is not None:
                os.close(child)
            os.close(parent)

    def ready(self):
        with self.directory():
            pass

    @staticmethod
    def _name(request_id):
        need(isinstance(request_id, str) and LABEL.fullmatch(request_id), 'receipt_request_id')
        return request_id + '.json'

    def _read(self, directory, request_id):
        try:
            descriptor = os.open(self._name(request_id), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                                 dir_fd=directory)
        except FileNotFoundError:
            return None
        try:
            before = os.fstat(descriptor)
            need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                 and before.st_uid == self.uid and before.st_gid == self.gid
                 and stat.S_IMODE(before.st_mode) == 0o444 and before.st_size <= 131072,
                 'native_receipt_metadata_changed')
            body = bytearray()
            while len(body) <= 131072:
                chunk = os.read(descriptor, min(65536, 131073 - len(body)))
                if not chunk:
                    break
                body.extend(chunk)
            after = os.fstat(descriptor)
            need(len(body) == before.st_size and (before.st_dev, before.st_ino, before.st_size,
                 before.st_mtime_ns, before.st_ctime_ns) == (after.st_dev, after.st_ino, after.st_size,
                 after.st_mtime_ns, after.st_ctime_ns), 'native_receipt_changed_during_read')
            result = json.loads(body, object_pairs_hook=strict_pairs,
                                parse_constant=lambda _: (_ for _ in ()).throw(Held('nonfinite_receipt')))
            need(isinstance(result, dict) and result.get('schema') == 'jev.semantic-result.v1'
                 and result.get('requestId') == request_id and set(result) <= RESULT_FIELDS,
                 'native_receipt_shape')
            return result
        finally:
            os.close(descriptor)

    def read(self, request_id, request_sha256, state_digest):
        with self.directory() as directory:
            result = self._read(directory, request_id)
        if result is not None:
            need(result.get('submittedRequestSha256') == request_sha256
                 and result.get('stateDigest') == state_digest, 'native_receipt_conflict')
        return result

    def read_existing(self, request_id, request_sha256, state_digest):
        """Recover one original result with no directory/file creation or fsync."""
        self._name(request_id)
        with self.directory(create=False) as directory:
            result = None if directory is None else self._read(directory, request_id)
        if result is not None:
            need(result.get('submittedRequestSha256') == request_sha256
                 and result.get('stateDigest') == state_digest, 'native_receipt_conflict')
        return result

    def publish(self, result):
        need(isinstance(result, dict) and set(result) <= RESULT_FIELDS
             and result.get('schema') == 'jev.semantic-result.v1', 'technical_result_only')
        request_id = result['requestId']
        raw = (json.dumps(result, sort_keys=True, ensure_ascii=False, allow_nan=False,
                          separators=(',', ':')) + '\n').encode()
        need(len(raw) <= 131072, 'native_result_receipt_bound')
        with self.directory() as directory:
            prior = self._read(directory, request_id)
            if prior is not None:
                need(prior == result, 'native_receipt_conflict')
                return prior
            temporary = '.pending-' + request_id + '-' + secrets.token_hex(8)
            descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                 0o444, dir_fd=directory)
            try:
                os.fchmod(descriptor, 0o444)
                view = memoryview(raw)
                while view:
                    count = os.write(descriptor, view)
                    need(count > 0, 'native_receipt_write_unknown')
                    view = view[count:]
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
            status = self.rename(directory, temporary.encode(), directory,
                                 self._name(request_id).encode(), 1)
            if status != 0:
                code = ctypes.get_errno()
                if code == errno.EEXIST:
                    prior = self._read(directory, request_id)
                    need(prior == result, 'native_receipt_conflict')
                    return prior
                raise Held('native_receipt_publication_unknown')
            os.fsync(directory)
            observed = self._read(directory, request_id)
            need(observed == result, 'native_receipt_readback_unknown')
            return observed


class Held(Exception):
    pass


def need(condition, code):
    if not condition:
        raise Held(code)


def sha(body):
    return hashlib.sha256(body).hexdigest()


def strict_pairs(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, 'duplicate_json_field')
        result[key] = value
    return result


def parse_request(raw):
    need(0 < len(raw) <= 65536, 'request_size')
    request = json.loads(raw, object_pairs_hook=strict_pairs,
                         parse_constant=lambda _: (_ for _ in ()).throw(Held('nonfinite_json')))
    json.dumps(request, allow_nan=False)
    need(isinstance(request, dict) and set(request) == {
        'schema', 'requestId', 'model', 'purpose', 'stateDigest', 'state', 'questions', 'scope'}, 'request_fields')
    need(request['schema'] == 'jev.semantic-request.v1' and request['model'] == MODEL, 'request_model_or_schema')
    need(isinstance(request['requestId'], str) and LABEL.fullmatch(request['requestId']), 'request_id')
    need(request['purpose'] == 'semantic_bundle', 'purpose')
    need(isinstance(request['stateDigest'], str) and HEX.fullmatch(request['stateDigest']), 'state_digest')
    need(isinstance(request['state'], dict), 'state_object_required')
    need(all(isinstance(request['state'].get(key), dict) for key in ('requiredContext', 'binding', 'facts')),
         'whole_lens_state_required')
    context = request['state']['requiredContext']
    need(set(context) >= {'currentAuthority', 'wholeObjective', 'taskGoal', 'acceptedWhy',
                          'acceptedDecisions', 'owners', 'dependencies', 'completedEffectRefs',
                          'uncertainEffectRefs', 'remainingAcceptance'}, 'whole_lens_capsule_incomplete')
    scope = request['scope']
    need(isinstance(scope, dict) and set(scope) == {
        'sessionId', 'agentId', 'agentType', 'taskGoal', 'wholeObjective', 'why', 'owners'}, 'scope_fields')
    need(isinstance(scope['sessionId'], str) and LABEL.fullmatch(scope['sessionId']), 'native_session_required')
    for key in ('agentId', 'agentType'):
        need(scope[key] is None or isinstance(scope[key], str) and LABEL.fullmatch(scope[key]), 'native_identity')
    for key in ('taskGoal', 'wholeObjective', 'why'):
        need(isinstance(scope[key], str) and 1 <= len(scope[key]) <= 12000, 'whole_lens_scope_missing')
    need(isinstance(scope['owners'], list) and 1 <= len(scope['owners']) <= 32
         and all(isinstance(owner, str) and 1 <= len(owner) <= 1000 for owner in scope['owners']), 'scope_owners')
    questions = request['questions']
    need(isinstance(questions, dict) and 1 <= len(questions) <= 24, 'question_count')
    for name, question in questions.items():
        need(isinstance(name, str) and LABEL.fullmatch(name) and isinstance(question, dict), 'question_shape')
        need(question.get('type') in ('noul', 'choice'), 'question_type')
        if question['type'] == 'choice':
            need(isinstance(question.get('criteria'), dict) and 2 <= len(question['criteria']) <= 32,
                 'candidate_count')
    return request


def run(raw, *, descriptor_verify=None):
    started = time.monotonic()
    request, transports, leases = None, 0, 0
    adapter = manifest = origins = getter = pool = None
    answers, usage, wire = {}, {}, {}
    status, error, source_verified = 'held', None, False
    returned_model = None
    descriptor_before = descriptor_after = None
    receipt_owner = None
    try:
        request = parse_request(raw)
        need(callable(descriptor_verify), 'descriptor_verifier_required')
        need(os.getuid() == 10000 and os.getgid() == 10000, 'native_uid')
        need(os.environ.get('HOME') == '/opt/data' and os.environ.get('HERMES_HOME') == '/opt/data', 'native_home')
        descriptor_before = descriptor_verify()
        need(isinstance(descriptor_before, dict)
             and descriptor_before.get('status') == 'successor-source-verified'
             and descriptor_before.get('callerSha256') == CALLER_SHA
             and descriptor_before.get('manifestSha256') == MANIFEST_SHA,
             'descriptor_precondition_failed')
        receipt_owner = NativeResultReceipts(ROOT)
        receipt_owner.ready()
        existing = receipt_owner.read(request['requestId'], sha(raw), request['stateDigest'])
        if existing is not None:
            fresh = descriptor_verify()
            need(isinstance(fresh, dict) and fresh.get('status') == 'successor-source-verified'
                 and fresh.get('callerSha256') == CALLER_SHA and fresh.get('manifestSha256') == MANIFEST_SHA,
                 'descriptor_postcondition_failed')
            # This is the original exact result, including its original count and
            # usage. It reconciles the original admission, never a fresh model call.
            return existing
        manifest_raw = (ROOT / 'application-manifest.json').read_bytes()
        need(sha(manifest_raw) == MANIFEST_SHA and sha((ROOT / 'native-caller.py').read_bytes()) == CALLER_SHA,
             'native_source_preflight')
        spec = importlib.util.spec_from_file_location('existing_pinned_jev_caller', ROOT / 'native-caller.py')
        adapter = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(adapter)
        manifest = json.loads(manifest_raw)
        need(manifest.get('deploymentRoot') == str(ROOT), 'native_deployment_root')
        adapter.verify_sources(manifest)
        sys.path.insert(0, str(ROOT / 'upstream'))
        from jevkit import client, privacy, rerank  # noqa: F401: required origin closure
        adapter.verify_loaded_origins(manifest)
        need(not privacy.is_sensitive(json.dumps(request['state'], ensure_ascii=False))
             and not privacy.is_sensitive(json.dumps(request['questions'], ensure_ascii=False))
             and not privacy.is_sensitive(json.dumps(request['scope'], ensure_ascii=False)), 'sensitive_input')
        # The full scope is sent as state, not merely stored in receipts. This
        # ensures every question can see purpose/WHY/owners as well as local facts.
        state = {**request['state'], 'scope': request['scope']}
        questions = client.check_questions(request['questions'])
        sys.path.insert(0, '/opt/hermes')
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            from hermes_cli.config import get_env_value_prefer_dotenv
            getter = get_env_value_prefer_dotenv
            origins = adapter.verify_loaded_origins(manifest, getter)
            credential = getter('AI_GATEWAY_API_KEY')
        need(credential and not str(credential).lstrip().startswith('op://'), 'native_credential_unavailable')
        os.environ['JEV_PROXY_API_KEY'] = credential
        os.environ['TYPESAFE_BASE_URL'] = adapter.BASE
        os.environ['TYPESAFE_MODEL'] = MODEL
        del credential
        pool = adapter.SingleRequestPool(client._POOL, client.JevError)
        client._POOL = pool

        def transport(body, headers, timeout):
            nonlocal transports, returned_model, usage
            need(transports == 0, 'http_budget_exhausted')
            need(len(body) <= MAX_WIRE_BYTES, 'serialized_wire_budget_exceeded')
            transports += 1
            wire.update(startedMonotonicNs=time.monotonic_ns(), nativeProcessId=os.getpid(),
                        requestBodySha256=sha(body))
            try:
                reply = client._http_transport(body, headers, timeout, adapter.ENDPOINT, 65536)
            finally:
                wire['completedMonotonicNs'] = time.monotonic_ns()
            wire['responseBodySha256'] = sha(reply)
            payload = json.loads(reply)
            returned_model = payload.get('model')
            usage = adapter.usage_only(payload.get('usage'))
            return reply

        reply = client.ask(state, questions, timeout=20, retries=0, model=MODEL,
                           provider='typesafe', transport=transport)
        need(returned_model in (MODEL, 'jev-1.14.0'), 'returned_model_mismatch')
        need(transports == 1 and pool.leases == 1, 'http_count_unverified')
        answers, status = reply['answers'], 'ok'
    except Exception as exception:
        # Never print upstream exception details: they may include source/tool
        # state or credential transport context. Known codes only are retained.
        code = exception.args[0] if isinstance(exception, Held) else getattr(exception, 'code', None)
        error = code if isinstance(code, str) and re.fullmatch(r'[a-z0-9_]{1,96}', code) else 'native_semantic_precondition_or_transport_error'
        status = 'error' if transports else 'held'
    finally:
        os.environ.pop('JEV_PROXY_API_KEY', None)
    if pool is not None:
        leases = pool.leases
    if adapter is not None and manifest is not None and origins is not None:
        try:
            adapter.verify_sources(manifest)
            need(adapter.verify_loaded_origins(manifest, getter) == origins, 'loaded_origins_changed')
            need(sha((ROOT / 'application-manifest.json').read_bytes()) == MANIFEST_SHA, 'manifest_changed')
            source_verified = True
        except Exception:
            status, error = 'error', error or 'source_postcondition_failed'
            answers = {}
    if descriptor_before is not None:
        try:
            descriptor_after = descriptor_verify()
            need(isinstance(descriptor_after, dict)
                 and descriptor_after.get('status') == 'successor-source-verified'
                 and descriptor_after.get('callerSha256') == CALLER_SHA
                 and descriptor_after.get('manifestSha256') == MANIFEST_SHA,
                 'descriptor_postcondition_failed')
        except Exception:
            source_verified = False
            status, error = 'error' if transports else 'held', error or 'descriptor_postcondition_failed'
            answers = {}
    if status != 'ok':
        answers = {}
    result = {'schema': 'jev.semantic-result.v1', 'requestId': request.get('requestId') if request else None,
            'purpose': request.get('purpose') if request else None,
            'stateDigest': request.get('stateDigest') if request else None,
            'submittedRequestSha256': sha(raw), 'requestedModel': MODEL, 'returnedModel': returned_model,
            'status': status, 'errorCode': error, 'answers': answers, 'usage': usage,
            'transportInvocations': transports, 'connectionLeases': leases,
            'callerLatencyMs': round((time.monotonic() - started) * 1000),
            'sourcePostconditionVerified': source_verified,
            'descriptorSourceBefore': descriptor_before,
            'descriptorSourceAfter': descriptor_after,
            'sourcePins': {'callerSha256': CALLER_SHA, 'manifestSha256': MANIFEST_SHA}, 'wire': wire,
            'executionAuthorityGranted': False, 'automaticWrite': False, 'workerCompletionVerified': False}
    if receipt_owner is not None and request is not None:
        try:
            return receipt_owner.publish(result)
        except Exception:
            result['status'] = 'error' if transports else 'held'
            result['errorCode'] = result['errorCode'] or 'native_result_receipt_publication_unknown'
            result['answers'] = {}
            result['durableReceiptStatus'] = 'publication-unknown-preserve-original'
    return result


if __name__ == '__main__':
    payload = sys.stdin.buffer.read(65537)
    print(json.dumps(run(payload), ensure_ascii=False, allow_nan=False), flush=True)
