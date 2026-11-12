"""Invented offline stubs only. No test opens a socket or calls Hindsight.

Request references: HC U1/001 and U3/010 retain; U4/016/025 documents/facts;
U8/030-033 lists; U14/018/116 + 519/520 operations; 470/475 model envelope;
U11/107 stats; Profile/480/517/538 version/config/traces. Transport refusal cases
are invented U9/UQ4 cases, not a claim that real redirects have been measured.
"""
import contextlib
import copy
import errno
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('publisher', ROOT / 'scripts/publish-memory.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
CONFIG = json.loads((ROOT / 'memory/engineering.json').read_bytes())


def envelope(items, offset=0, limit=100, key='items'):
    return {key: items[offset:offset+limit], 'total': len(items), 'offset': offset, 'limit': limit}


def pin():
    # Only invented content; production projection/tag logic is imported, never duplicated.
    record = {'header': {'schema_version': 2, 'record_id': 'invented', 'title': 'Example',
              'subject': {'type': 'memory-map', 'id': 'engineering'}, 'abstract': 'Example',
              'kind': 'incident', 'horizon': 'mid-term', 'recall_terms': ['ExampleID'],
              'review_on': '2026-12-01'}, 'body': b'ExampleID applies when the cache is closed.\n'}
    nodes = {('memory-map', 'engineering'): {'parent': None}}
    return {'commit': 'a' * 40, 'config': CONFIG, 'config_raw': b'config', 'subjects_raw': b'subjects',
            'expected': {'invented': {'bytes': P.CHECK.publication_bytes(record),
              'hash': P.CHECK.publication_hash(record), 'tags': P.CHECK.derived_tags(record['header'], nodes),
              'terms': ['ExampleID'], 'recall': P.CHECK.recall_tags(record['header']['subject'])}}}


class Stub:
    """Shapes: HC U4/016/025, U8/030/032, U14/018/116/519, Profile/480/517/538, 470/107."""
    def __init__(self):
        self.pin = pin()
        self.calls = []
        self.mutate = lambda route, value: value
        want = self.pin['expected']['invented']
        self.doc = {'id': 'invented', 'original_text': want['bytes'].decode(), 'tags': want['tags']}
        self.fact = {'id': 'fact-1', 'document_id': 'invented', 'text': 'ExampleID applies when closed.',
                     'entities': '', 'tags': want['tags'], 'fact_type': 'world'}  # Empty string is unmeasured.
        self.trace = {'id': 'trace-1', 'scope': 'retain_extract_facts', 'status': 'success',
                      'provider': 'invented-provider', 'model': 'invented-model',
                      'metadata': {'document_id': 'invented'}}
        self.operations = []
        self.details = {}

    def request(self, method, path, body=None):
        self.calls.append((method, path, body))
        assert method == 'GET', 'verify or blocked publish attempted a mutation'
        url = urllib.parse.urlsplit(path)
        route = url.path.removeprefix(P.B)
        query = urllib.parse.parse_qs(url.query)
        offset, limit = int(query.get('offset', [0])[0]), int(query.get('limit', [100])[0])
        if route == '/version':
            answer = {'api_version': P.API_VERSION}
        elif route == '/config':
            answer = {'config': {'enable_observations': False, 'enable_auto_consolidation': False,
                 'retain_extraction_mode': 'custom', 'retain_custom_instructions': CONFIG['retain_custom_instructions']}}
        elif route == '/stats':
            answer = dict.fromkeys(['total_observations', 'pending_operations', 'pending_consolidation',
                                   'failed_operations', 'failed_consolidation'], 0)
        elif route == '/documents/invented':
            answer = copy.deepcopy(self.doc)
        elif route.startswith('/operations/'):
            answer = self.details[route.rsplit('/', 1)[1]]
        else:
            items = {'/documents': [{'id': 'invented'}], '/memories/list': [self.fact],
                     '/mental-models': [], '/llm-requests': [self.trace], '/operations': self.operations}[route]
            answer = envelope(copy.deepcopy(items), offset, limit, 'operations' if route == '/operations' else 'items')
        return self.mutate(route, answer)


class PublisherTests(unittest.TestCase):
    def setUp(self):
        self.network = patch.object(P.socket, 'create_connection', side_effect=AssertionError('offline_only'))
        self.blocked_socket = self.network.start()
        self.addCleanup(self.network.stop)

    def tearDown(self):
        self.blocked_socket.assert_not_called()

    def test_exact_instruction_pin(self):
        self.assertEqual(hashlib.sha256(CONFIG['retain_custom_instructions'].encode()).hexdigest(),
                         '67ea2fbd3ad16718fb41c3c5a879cd96f8a1f61db83e4cb4fd67998f695883e0')
        self.assertEqual(set(CONFIG), {'schema_version', 'bank_id', 'retain_custom_instructions', 'mcp_enabled_tools'})

    def test_verify_read_only_and_complete(self):
        stub = Stub()
        with patch.object(P, 'unchanged'):
            result = P.verify(stub, stub.pin)
        self.assertEqual((result['documents'], result['facts'], result['acceptance']), (1, 1, 'not_evaluated'))
        self.assertTrue(all(method == 'GET' for method, _, _ in stub.calls))
        self.assertTrue(any('exclude_knowledge_pages=false' in path for _, path, _ in stub.calls))

    def test_rejects_bytes_tags_terms_profile_trace_and_extra_ids(self):
        changes = [('/documents/invented', lambda a: dict(a, original_text='altered')),
                   ('/documents/invented', lambda a: dict(a, tags=[])),
                   ('/memories/list', lambda a: dict(a, items=[dict(a['items'][0], text='omitted', entities='')])),
                   ('/memories/list', lambda a: dict(a, items=[dict(a['items'][0], tags=[])])),
                   ('/memories/list', lambda a: dict(a, items=[dict(a['items'][0], document_id='foreign')])),
                   ('/version', lambda a: dict(a, api_version='unmeasured')),
                   ('/config', lambda a: {'config': dict(a['config'], retain_custom_instructions='drift')}),
                   ('/llm-requests', lambda a: dict(a, items=[], total=0)),
                   ('/llm-requests', lambda a: dict(a, items=[dict(a['items'][0], metadata={})])),
                   ('/documents', lambda a: dict(a, items=a['items'] + [{'id': 'extra'}], total=2)),
                   ('/mental-models', lambda a: dict(a, items=[{'id': 'model'}], total=1)),
                   ('/stats', lambda a: dict(a, total_observations=1))]
        for route, change in changes:
            with self.subTest(route=route):
                stub = Stub()
                stub.mutate = lambda r, a: change(a) if r == route else a
                with patch.object(P, 'unchanged'), self.assertRaises(P.Stop):
                    P.verify(stub, stub.pin)

    def test_entities_preserve_term_without_date_blanket_refusal(self):
        stub = Stub()
        stub.fact.update(text='Condition applies.', entities='Other entity, ExampleID', occurred_start='2026-01-01')
        with patch.object(P, 'unchanged'):
            P.verify(stub, stub.pin)

    def test_non_ascii_term_preserved_only_in_entities(self):
        stub = Stub()
        stub.pin['expected']['invented']['terms'] = ['\uac00']
        stub.fact.update(text='Condition applies.', entities='Other entity, \uac00')
        with patch.object(P, 'unchanged'):
            P.verify(stub, stub.pin)

    def test_entity_character_does_not_match_json_escape_fragment(self):
        stub = Stub()
        stub.pin['expected']['invented']['terms'] = ['uac00']
        stub.fact.update(text='Condition applies.', entities='Other entity, \uac00')
        with patch.object(P, 'unchanged'), self.assertRaisesRegex(P.Stop, '^recall_term_missing$'):
            P.verify(stub, stub.pin)

    def test_pagination_and_uncertain_lists(self):
        for broken in [dict(items=[], total=1, offset=0, limit=100),
                       dict(items=[], total=0, offset=0, limit=100, total_is_estimate=True),
                       dict(items=[], total=0, offset=1, limit=100)]:
            with self.subTest(broken=broken), self.assertRaises(P.Stop):
                P.pages(type('List', (), {'request': lambda *a: broken})(), '/documents')
        calls = []
        def read(method, path):
            offset = int(urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)['offset'][0])
            calls.append(offset)
            return envelope([{'id': 'one'}, {'id': 'two'}], offset, 1)
        self.assertEqual(len(P.pages(type('List', (), {'request': staticmethod(read)})(), '/documents', limit=1)), 2)
        self.assertEqual(calls, [0, 1])

    def test_child_missing_or_active_blocks(self):
        stub = Stub()
        stub.operations = [{'id': 'parent', 'status': 'completed'}]
        stub.details['parent'] = {'id': 'parent', 'status': 'completed', 'error_message': None,
            'result_metadata': {'is_parent': True, 'num_sub_batches': 1},
            'child_operations': [{'operation_id': 'child', 'status': 'completed', 'error_message': None}]}
        with self.assertRaisesRegex(P.Stop, 'child'):
            P.quiescent(stub)
        stub.operations.append({'id': 'child', 'status': 'completed'})
        stub.details['child'] = {'id': 'child', 'status': 'completed', 'result_metadata': {'parent_operation_id': 'parent'}}
        self.assertEqual(len(P.quiescent(stub)), 2)
        stub.details['child']['status'] = 'pending'
        with self.assertRaises(P.Stop):
            P.quiescent(stub)

    def test_lost_ack_never_resends(self):
        stub = Stub()
        original = stub.request
        def dispatch(method, path, body=None):
            if method != 'GET':
                stub.calls.append((method, path, body))
                raise P.Stop('mutation_uncertain')
            return original(method, path, body)
        stub.request = dispatch
        with self.assertRaisesRegex(P.Stop, 'keep_consumers_stopped'):
            P.retain_once(stub, 'invented', stub.pin['expected']['invented'])
        writes = [x for x in stub.calls if x[0] == 'POST']
        self.assertEqual(len(writes), 1)
        self.assertEqual(writes[0][2]['items'][0]['update_mode'], 'replace')
        self.assertEqual(len(writes[0][2]['operation_id']), 36)
        self.assertTrue(any('/operations?' in path for _, path, _ in stub.calls))
        calls = []
        def acknowledged_then_partial(method, path, body=None):
            calls.append(method)
            if method == 'POST':
                return dict(success=True, operation_id=body['operation_id'],
                            bank_id=P.BANK, items_count=1, **{'async': True})
            raise P.Stop('list_incomplete')
        client = type('Partial', (), {'request': staticmethod(acknowledged_then_partial)})()
        with self.assertRaisesRegex(P.Stop, 'keep_consumers_stopped'):
            P.retain_once(client, 'invented', stub.pin['expected']['invented'])
        self.assertEqual(calls, ['POST', 'GET'])

    def test_publish_refuses_unmeasured_lifecycle_even_when_existing_or_absent(self):
        for absent in (False, True):
            stub = Stub()
            if absent:
                def missing(route, answer):
                    raise P.Stop('read_404')
                stub.mutate = missing
            with patch.object(P, 'unchanged'), self.assertRaisesRegex(P.Stop, 'UQ1'):
                P.publish(stub, stub.pin)
            self.assertTrue(stub.calls)
            self.assertTrue(all(x[0] == 'GET' for x in stub.calls))

    def test_retain_uses_fresh_ids_and_verifies_acknowledged_parent(self):
        class Dispatch:
            def __init__(self):
                self.bodies = []
            def request(self, method, path, body):
                self.bodies.append(body)
                return {'success': True, 'operation_id': body['operation_id'],
                        'bank_id': P.BANK, 'items_count': 1, 'async': True}
        client = Dispatch()
        def operations(_):
            return [{'id': client.bodies[-1]['operation_id']}]
        with patch.object(P, 'quiescent', side_effect=operations) as checked:
            for _ in range(2):
                P.retain_once(client, 'invented', pin()['expected']['invented'])
            self.assertEqual(checked.call_count, 2)
        self.assertNotEqual(client.bodies[0]['operation_id'], client.bodies[1]['operation_id'])

    def test_mid_verification_drift_and_estimated_or_changed_lists(self):
        stub = Stub()
        with patch.object(P, 'unchanged', side_effect=P.Stop('stale_commit')), self.assertRaises(P.Stop):
            P.verify(stub, stub.pin)
        self.assertTrue(stub.calls)
        def changed(method, path):
            offset = int(urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)['offset'][0])
            return dict(items=[{'id': str(offset)}], total=2 + offset, offset=offset, limit=1)
        with self.assertRaisesRegex(P.Stop, 'list_changed'):
            P.pages(type('List', (), {'request': staticmethod(changed)})(), '/documents', limit=1)

    def test_stale_commit_blocks_before_requests(self):
        stub = Stub()
        with patch.object(P.CHECK, 'git', return_value=b'b' * 40 + b'\n'), self.assertRaisesRegex(P.Stop, 'stale_commit'):
            P.publish(stub, stub.pin)
        self.assertEqual(stub.calls, [])

    def test_fd_sentinel_redaction_redirects_proxy_and_target(self):
        sentinel = 'invented-secret-do-not-output'
        read_fd, write_fd = os.pipe()
        try:
            os.write(write_fd, sentinel.encode())
            os.close(write_fd)
            self.assertEqual(P.credential(read_fd), sentinel)
        finally:
            os.close(read_fd)
        output = io.StringIO()
        for failure in [TimeoutError(sentinel), urllib.error.HTTPError(P.ORIGIN, 302, sentinel, {}, None),
                        urllib.error.HTTPError(P.ORIGIN, 401, sentinel, {}, None)]:
            client = P.Client(sentinel)
            with patch.object(client.opener, 'open', side_effect=failure) as opened:
                with patch.object(P, 'locked', contextlib.nullcontext), patch.object(P, 'pinned', return_value=pin()), \
                     patch.object(P, 'credential', return_value=sentinel), patch.object(P, 'Client', return_value=client), \
                     contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
                    self.assertEqual(P.main(['verify', '--credential-fd', '3']), 1)
                self.assertEqual(opened.call_count, 1)
        self.assertNotIn(sentinel, output.getvalue())
        self.assertIsNone(P.NoRedirect().redirect_request(None, None, 302, '', {}, 'http://invalid.example/'))
        client = P.Client(sentinel)
        for target in ['https://invalid.example/', '/v1/default/banks/other/config', P.B + '/../config']:
            with patch.object(client.opener, 'open') as opened, self.assertRaises(P.Stop):
                client.request('GET', target)
            opened.assert_not_called()
        with patch.object(P.urllib.request, 'build_opener') as build:
            P.Client(sentinel)
            self.assertEqual(build.call_args.args[0].proxies, {})

    def test_shared_nonblocking_lock(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(P.CHECK, 'git', return_value=directory.encode()):
            with P.locked():
                with self.assertRaisesRegex(P.Stop, 'publisher_busy'), P.locked():
                    self.fail('concurrent lock')
            self.assertEqual(Path(directory, 'publish-memory.lock').read_bytes(), b'')

    def test_help_has_no_target_override(self):
        result = subprocess.run([sys.executable, '-I', '-B', str(ROOT / 'scripts/publish-memory.py'), '--help'],
                                capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0)
        self.assertIn(b'publish,verify', result.stdout)
        self.assertIn(b'--credential-fd', result.stdout)
        self.assertNotIn(b'--bank', result.stdout)
        self.assertNotIn(b'--url', result.stdout)


class RetractStub(Stub):
    """HC U5/124-127, U10/062,143; invented content, never a native call."""
    def __init__(self):
        super().__init__()
        self.deleted = False
        self.lost = False
        self.surviving = False

    def request(self, method, path, body=None):
        if method == 'DELETE':
            self.calls.append((method, path, body))
            self.deleted = True
            if self.lost:
                raise P.Stop('mutation_uncertain')
            return {'success': True, 'document_id': 'invented', 'memory_units_deleted': 1}
        if method == 'POST':
            self.calls.append((method, path, body))
            return {'results': [self.fact] if self.surviving else [], 'chunks': None}
        if self.deleted and '/documents/invented' in path:
            self.calls.append((method, path, body))
            raise P.Stop('read_404')
        if self.deleted and '/memories/list?' in path:
            self.calls.append((method, path, body))
            return envelope([])
        return super().request(method, path, body)


class RetractionTests(unittest.TestCase):
    setUp = PublisherTests.setUp
    tearDown = PublisherTests.tearDown

    def run_retract(self, stub, override=False):
        with patch.object(P, 'unchanged'), contextlib.redirect_stdout(io.StringIO()):
            return P.retract(stub, stub.pin, 'invented', override)

    def test_retract_order_and_listed_reads(self):
        stub = RetractStub()
        stub.pin['expected']['invented']['terms'].append('OtherID')
        result = self.run_retract(stub)
        self.assertEqual(result['result'], 'retracted_on_listed_reads')
        self.assertEqual(result['route'], 'unavailable')
        self.assertEqual(result['listed_reads'], 2)
        calls = stub.calls
        deletion = next(i for i, x in enumerate(calls) if x[0] == 'DELETE')
        self.assertEqual(calls[deletion][1:], (P.B + '/documents/invented', None))
        self.assertTrue(any('/documents/invented' in x[1] for x in calls[:deletion]))
        self.assertTrue(any('/operations?' in x[1] for x in calls[:deletion]))
        self.assertTrue(any('/mental-models?' in x[1] for x in calls[:deletion]))
        self.assertEqual([x[0] for x in calls[deletion:deletion+4]], ['DELETE', 'GET', 'GET', 'GET'])
        recalls = [x for x in calls if x[0] == 'POST']
        self.assertEqual([x[2]['query'] for x in recalls], ['ExampleID', 'OtherID'])
        for _, path, body in recalls:
            self.assertEqual(path, P.B + '/memories/recall')
            self.assertEqual(body['tags'], ['subject:memory-map:engineering'])
            self.assertEqual(body['tags_match'], 'all_strict')
            self.assertEqual(body['include'], {'entities': None, 'chunks': {'max_tokens': 512}})
        self.assertEqual(sum(x[0] == 'DELETE' for x in calls), 1)

    def test_recovery_pin_mismatch_and_missing_source(self):
        for missing in (False, True):
            stub = RetractStub()
            if missing:
                stub.pin['expected'].clear()
            else:
                stub.doc['original_text'] = 'different invented bytes'
            with self.assertRaisesRegex(P.Stop, '^bank_copy_not_in_source$'):
                self.run_retract(stub)
            self.assertTrue(stub.calls)
            self.assertTrue(all(x[0] == 'GET' for x in stub.calls))
            if missing:
                with self.assertRaisesRegex(P.Stop, '^deleted_unproven$'):
                    self.run_retract(stub, True)
                self.assertFalse(any(x[0] == 'POST' for x in stub.calls))
            else:
                self.assertEqual(self.run_retract(stub, True)['result'], 'retracted_on_listed_reads')

    def test_source_drift_immediately_before_delete_stops(self):
        stub, checks = RetractStub(), []
        def source_check(pin):
            checks.append(list(stub.calls))
            if len(checks) == 2:
                raise P.Stop('stale_commit')
        with patch.object(P, 'unchanged', side_effect=source_check), \
             contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(P.Stop, '^stale_commit$'):
            P.retract(stub, stub.pin, 'invented')
        self.assertEqual(len(checks), 2)
        self.assertEqual(checks[1][-1][:2], ('GET', P.B + '/documents/invented'))
        self.assertTrue(stub.calls)
        self.assertTrue(all(call[0] == 'GET' for call in stub.calls))

    def test_final_rerun_catches_drift_after_recall(self):
        for drift, code in [('operations', 'operation_not_completed'),
                            ('models', 'models_present'),
                            ('profile', 'profile_drift'), ('source', 'stale_commit')]:
            with self.subTest(drift=drift):
                stub = RetractStub()
                original = stub.request
                recalled = False
                def read(method, path, body=None):
                    nonlocal recalled
                    answer = original(method, path, body)
                    if method == 'POST':
                        recalled = True
                    elif recalled:
                        if drift == 'operations' and '/operations?' in path:
                            return envelope([{'id': 'late', 'status': 'pending'}], key='operations')
                        if drift == 'models' and '/mental-models?' in path:
                            return envelope([{'id': 'late-model'}])
                        if drift == 'profile' and path.endswith('/config'):
                            answer['config']['retain_custom_instructions'] = 'invented drift'
                    return answer
                def source_check(_):
                    if recalled and drift == 'source':
                        raise P.Stop('stale_commit')
                stub.request = read
                with patch.object(P, 'unchanged', side_effect=source_check), \
                     contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(P.Stop, '^' + code + '$'):
                    P.retract(stub, stub.pin, 'invented')
                self.assertTrue(recalled)

    def test_retract_success_emits_no_fact_or_original_text(self):
        stub = RetractStub()
        stub.fact['text'] = 'ExampleID applies when the cache is closed.'
        stdout, stderr = io.StringIO(), io.StringIO()
        bodies = [stub.fact['text'], stub.doc['original_text']]
        stopped, result = None, None
        with patch.object(P, 'unchanged'), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                result = P.retract(stub, stub.pin, 'invented')
            except Exception as error:
                stopped = error
        output = stdout.getvalue() + stderr.getvalue() + str(stopped) + json.dumps(result)
        for body in bodies:
            self.assertNotIn(body, output, 'retract_body_disclosed')
        self.assertIsNone(stopped)
        self.assertEqual(result['result'], 'retracted_on_listed_reads')

    def test_failed_preconditions_have_nonvacuous_no_mutation_assertions(self):
        for command in ('publish', 'retract', 'restore'):
            for route, failure in [('/version', 'read_404'), ('/config', 'read_uncertain'),
                                   ('/operations', 'operation_not_completed'),
                                   ('/mental-models', 'list_incomplete')]:
                with self.subTest(command=command, route=route), tempfile.TemporaryDirectory() as directory:
                    stub = RetractStub()
                    def fail(r, answer):
                        if r == route:
                            raise P.Stop(failure)
                        return answer
                    stub.mutate = fail
                    archive = Path(directory, 'recovery.zip')
                    archive.write_bytes(b'invented archive')
                    with patch.object(P, 'unchanged'), self.assertRaises(P.Stop):
                        if command == 'retract':
                            P.retract(stub, stub.pin, 'invented')
                        elif command == 'restore':
                            P.restore(stub, stub.pin, archive, P.digest(archive.read_bytes()))
                        else:
                            P.publish(stub, stub.pin)
                    self.assertTrue(stub.calls)
                    self.assertFalse(any(x[0] in ('DELETE', 'PUT', 'PATCH', 'POST') for x in stub.calls))

    def test_delete_lost_answer_reads_target_and_facts_never_succeeds(self):
        stub = RetractStub()
        stub.lost = True
        output = io.StringIO()
        with patch.object(P, 'unchanged'), contextlib.redirect_stdout(output), \
             self.assertRaisesRegex(P.Stop, '^delete_unresolved_route_unavailable_no_resend$'):
            P.retract(stub, stub.pin, 'invented')
        self.assertEqual(sum(x[0] == 'DELETE' for x in stub.calls), 1)
        self.assertFalse(any(x[0] == 'POST' for x in stub.calls))
        deletion = next(i for i, call in enumerate(stub.calls) if call[0] == 'DELETE')
        self.assertEqual([(method, path) for method, path, _ in stub.calls[deletion + 1:]], [
            ('GET', P.B + '/documents/invented'),
            ('GET', P.B + '/memories/list?document_id=invented&limit=100&offset=0'),
            ('GET', P.B + '/documents/invented/chunks')])
        reads = json.loads(output.getvalue())
        self.assertIs(reads['document_absent'], True)
        self.assertIs(reads['facts_absent'], True)
        self.assertIs(reads['chunks_absent'], True)

        # Each changed readback must independently change its printed evidence.
        for field, suffix, answer in [
                ('document_absent', '/documents/invented', stub.doc),
                ('facts_absent', '/memories/list?document_id=invented&limit=100&offset=0', envelope([stub.fact])),
                ('chunks_absent', '/documents/invented/chunks', {'chunks': []})]:
            stub = RetractStub()
            stub.lost = True
            original = stub.request
            def surviving_read(method, path, body=None):
                if stub.deleted and method == 'GET' and path == P.B + suffix:
                    stub.calls.append((method, path, body))
                    return answer
                return original(method, path, body)
            stub.request = surviving_read
            with patch.object(P, 'unchanged'), contextlib.redirect_stdout(io.StringIO()) as changed, \
                 self.assertRaisesRegex(P.Stop, '^delete_unresolved_route_unavailable_no_resend$'):
                P.retract(stub, stub.pin, 'invented')
            self.assertIs(json.loads(changed.getvalue())[field], False)

    def test_readback_residue_and_profile_drift_stop(self):
        for residue in ('document', 'facts', 'chunks', 'recall', 'models', 'profile'):
            stub = RetractStub()
            original = stub.request
            def read(method, path, body=None):
                if stub.deleted and method == 'GET':
                    if residue == 'document' and path.endswith('/documents/invented'):
                        return stub.doc
                    if residue == 'chunks' and path.endswith('/chunks'):
                        return {'chunks': []}
                    if residue == 'facts' and '/memories/list?' in path:
                        return envelope([stub.fact])
                    if residue == 'models' and '/mental-models?' in path:
                        return envelope([{'id': 'derived'}])
                    if residue == 'profile' and path.endswith('/config'):
                        return {'config': {}}
                return original(method, path, body)
            stub.request = read
            stub.surviving = residue == 'recall'
            code = {'document': 'deleted_unproven', 'facts': 'deleted_unproven',
                    'chunks': 'deleted_unproven', 'recall': 'listed_read_contains_document',
                    'models': 'models_present', 'profile': 'observations_enabled'}[residue]
            with self.subTest(residue=residue), self.assertRaisesRegex(P.Stop, '^' + code + '$'):
                self.run_retract(stub)
            self.assertEqual(sum(call[0] == 'DELETE' for call in stub.calls), 1)

    def test_real_client_no_resend_for_post_and_delete(self):
        for method in ('DELETE', 'POST'):
            for lost_at in ('open', 'read'):
                client = P.Client('invented-private-sentinel')
                with contextlib.ExitStack() as stack:
                    opened = stack.enter_context(patch.object(client.opener, 'open'))
                    if lost_at == 'open':
                        opened.side_effect = TimeoutError('invented')
                    else:
                        response = opened.return_value.__enter__.return_value
                        response.status = 200
                        response.read.side_effect = TimeoutError('invented')
                    with self.assertRaisesRegex(P.Stop, '^mutation_uncertain$'):
                        client.request(method, P.B + '/documents/invented')
                    self.assertEqual(opened.call_count, 1)

    def test_all_commands_share_lock_before_pinning_or_transport(self):
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(P.CHECK, 'git', return_value=directory.encode()), P.locked():
            for command in ('publish', 'verify', 'retract', 'restore'):
                argv = [command, '--credential-fd', '3']
                if command == 'retract':
                    argv += ['--record-id', 'invented']
                if command == 'restore':
                    argv += ['--recovery', 'unused.zip', '--recovery-sha256', '0' * 64]
                output = io.StringIO()
                with patch.object(P, 'pinned') as pinned, patch.object(P, 'Client') as client, \
                     contextlib.redirect_stdout(output):
                    self.assertEqual(P.main(argv), 1)
                self.assertIn('publisher_busy', output.getvalue())
                pinned.assert_not_called()
                client.assert_not_called()

    def test_restore_local_pins_before_requests_and_unmeasured_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory, 'recovery.zip')
            archive.write_bytes(b'invented archive')
            link = Path(directory, 'link.zip')
            link.symlink_to(archive)
            cases = [(archive, 'bad', P.Stop, 'recovery_sha256_invalid'),
                     (archive, '0' * 64, P.Stop, 'recovery_hash_mismatch'),
                     (link, P.digest(archive.read_bytes()), OSError, errno.ELOOP),
                     (Path(directory), '0' * 64, P.Stop, 'recovery_not_regular')]
            for path, sha, exception, code in cases:
                stub = Stub()
                with self.subTest(path=path), patch.object(P, 'unchanged'), self.assertRaises(exception) as stopped:
                    P.restore(stub, stub.pin, path, sha)
                self.assertEqual(stopped.exception.errno if exception is OSError else str(stopped.exception), code)
                self.assertEqual(stub.calls, [])
            stub = Stub()
            with patch.object(P, 'unchanged'), self.assertRaisesRegex(P.Stop, '^uq1_restore_unmeasured$'):
                P.restore(stub, stub.pin, archive, P.digest(archive.read_bytes()))
            self.assertTrue(stub.calls)
            self.assertTrue(all(x[0] == 'GET' for x in stub.calls))

    def test_success_output_redaction_each_command(self):
        sentinel = 'invented-private-sentinel'
        for command in ('publish', 'verify', 'retract', 'restore'):
            stub = RetractStub() if command == 'retract' else Stub()
            output = io.StringIO()
            argv = [command, '--credential-fd', '3']
            if command == 'retract':
                argv += ['--record-id', 'invented']
            if command == 'restore':
                argv += ['--recovery', 'unused.zip', '--recovery-sha256', '0' * 64]
            # publish/restore cannot succeed in this packet: exercise their common
            # success-output boundary with an invented return, never an import.
            with contextlib.ExitStack() as stack:
                for name in ('locked', 'run_budget'):
                    stack.enter_context(patch.object(P, name, contextlib.nullcontext))
                stack.enter_context(patch.object(P, 'pinned', return_value=stub.pin))
                stack.enter_context(patch.object(P, 'credential', return_value=sentinel))
                def transport(request, timeout):
                    url = urllib.parse.urlsplit(request.full_url)
                    body = None if request.data is None else json.loads(request.data)
                    try:
                        answer = stub.request(request.get_method(), url.path + ('?' + url.query if url.query else ''), body)
                    except P.Stop as error:
                        if str(error) == 'read_404':
                            raise urllib.error.HTTPError(request.full_url, 404, 'missing', {}, None) from None
                        raise
                    response = type('Response', (), {'status': 200,
                        'read': lambda self, limit: json.dumps(answer).encode()})()
                    return contextlib.nullcontext(response)
                opener = type('Transport', (), {'open': staticmethod(transport)})()
                stack.enter_context(patch.object(P.urllib.request, 'build_opener', return_value=opener))
                stack.enter_context(patch.object(P.time, 'sleep'))
                stack.enter_context(patch.object(P, 'unchanged'))
                if command in ('publish', 'restore'):
                    stack.enter_context(patch.object(P, command, return_value={'result': 'invented'}))
                stack.enter_context(contextlib.redirect_stdout(output))
                stack.enter_context(contextlib.redirect_stderr(output))
                with tempfile.TemporaryFile() as descriptors:
                    saved = [os.dup(fd) for fd in (1, 2)]
                    try:
                        for fd in (1, 2):
                            os.dup2(descriptors.fileno(), fd)
                        try:
                            status = P.main(argv)
                        except Exception as error:
                            self.assertNotIn(sentinel, str(error))
                            raise
                    finally:
                        for fd, original in zip((1, 2), saved):
                            os.dup2(original, fd)
                            os.close(original)
                    descriptors.seek(0)
                    descriptor_output = descriptors.read()
            self.assertEqual(status, 0)
            self.assertNotIn(sentinel, output.getvalue())
            self.assertNotIn(sentinel.encode(), descriptor_output)
