"""Invented offline stubs only. No test opens a socket or calls Hindsight.

Request references: HC U1/001 and U3/010 retain; U4/016/025 documents/facts;
U8/030-033 lists; U14/018/116 + 519/520 operations; 470/475 model envelope;
U11/107 stats; Profile/480/517/538 version/config/traces. Transport refusal cases
are invented U9/UQ4 cases, not a claim that real redirects have been measured.
"""
import contextlib
import copy
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
              'terms': ['ExampleID']}}}


class Stub:
    """Shapes: HC U4/016/025, U8/030/032, U14/018/116/519, Profile/480/517/538, 470/107."""
    def __init__(self):
        self.pin = pin()
        self.calls = []
        self.mutate = lambda route, value: value
        want = self.pin['expected']['invented']
        self.doc = {'id': 'invented', 'original_text': want['bytes'].decode(), 'tags': want['tags']}
        self.fact = {'id': 'fact-1', 'document_id': 'invented', 'text': 'ExampleID applies when closed.',
                     'entities': '[]', 'tags': want['tags'], 'fact_type': 'world'}
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
                   ('/memories/list', lambda a: dict(a, items=[dict(a['items'][0], text='omitted', entities='[]')])),
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
        stub.fact.update(text='Condition applies.', entities='["ExampleID"]', occurred_start='2026-01-01')
        with patch.object(P, 'unchanged'):
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

    def test_publish_refuses_unmeasured_lifecycle_even_when_existing_or_absent(self):
        for absent in (False, True):
            stub = Stub()
            if absent:
                stub.request = lambda *args: (_ for _ in ()).throw(P.Stop('read_404'))
            with patch.object(P, 'unchanged'), self.assertRaisesRegex(P.Stop, 'UQ1'):
                P.publish(stub, stub.pin)
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
