"""Invented offline HTTP/SSE/ZIP server; sockets are forbidden, including in controls."""
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import stat
import tempfile
import unittest
from unittest.mock import patch
import urllib.parse
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('exercise', ROOT / 'scripts/exercise-memory.py')
X = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(X)
BANK = 'zz-conformance-20261112-0123abcd'
SECRET = 'private-exercise-sentinel-never-recorded'
FOREIGN = 'foreign-bank'


class Clock:
    def __init__(self):
        self.now = 0
    def __call__(self):
        return self.now
    def sleep(self, seconds):
        self.now += seconds


def envelope(items, query, key='items'):
    offset, limit = int(query.get('offset', [0])[0]), int(query.get('limit', [100])[0])
    return {key: items[offset:offset + limit], 'total': len(items), 'offset': offset, 'limit': limit}


class FakeServer:
    """Models request shapes, NOT native guarantees. Every answer here is invented."""
    def __init__(self, clock, bank=BANK):
        self.clock, self.bank = clock, bank
        self.base = X.BANKS + '/' + X.P.segment(bank)
        self.present = False
        self.config = {}
        self.docs = {}
        self.ops = {}
        self.calls = []
        self.downloads = {}
        self.foreign_count = 7
        self.session_count = 0
        self.filter_list = self.filter_call = True
        self.pending = False
        self.leave_marker = self.fail_delete = False
        self.fail = None
        self.catalog = {name: {'name': name, 'inputSchema': {'type': 'object',
            'properties': {arg: {'type': 'string'} for arg in args}, 'required': args}}
            for name, args in {'recall': ['query'], 'get_document': ['document_id'], 'retain': ['content'],
                              'sync_retain': ['content'], 'reflect': ['query'],
                              'delete_document': ['document_id'], 'create_mental_model': ['name', 'source_query'],
                              'list_mental_models': []}.items()}

    def fact(self, rid):
        doc = self.docs[rid]
        return {'id': 'fact-' + rid, 'document_id': rid, 'text': doc['original_text'],
                'entities': 'Garam, ExampleID-A', 'tags': doc['tags'], 'fact_type': 'world',
                'occurred_start': '2026-01-02' if 'dated' in rid else None, 'occurred_end': None}

    def operation(self, oid=None, meta=None):
        oid = oid or 'operation-' + str(len(self.ops))
        self.ops[oid] = {'id': oid, 'status': 'pending' if self.pending else 'completed',
                         'result_metadata': meta or {}, 'error_message': None, 'child_operations': []}
        return {'operation_id': oid, 'status': self.ops[oid]['status']}

    def exchange(self, method, path, body=None, **kwargs):
        self.calls.append((method, path, copy.deepcopy(body), self.clock()))
        if self.fail:
            self.fail(method, path, body)
        status, answer, sid = self.answer(method, path, body, **kwargs)
        return status, answer if isinstance(answer, bytes) else X.encode(answer), sid

    def answer(self, method, path, body=None, **kwargs):
        url = urllib.parse.urlsplit(path)
        q = urllib.parse.parse_qs(url.query)
        route = url.path.removeprefix(self.base)
        if kwargs.get('invalid'):
            return 401, {}, None
        if url.path == '/version':
            return 200, {'api_version': X.P.API_VERSION}, None
        if url.path == '/openapi.json':
            return 200, {'paths': {X.BANKS + '/{bank_id}' + r: {'post': {}}
                                   for r in ('/transfer/import', '/transfer/export')}}, None
        if url.path == X.BANKS:
            items = [{'bank_id': FOREIGN, 'fact_count': self.foreign_count}]
            if self.present:
                items.append({'bank_id': self.bank, 'fact_count': len(self.docs)})
            return 200, envelope(items, q, 'banks'), None
        if url.path in self.downloads:
            return 200, self.downloads[url.path], None
        if url.path == '/mcp/' + X.P.segment(self.bank) + '/':
            return self.mcp(body, kwargs.get('session'))
        if route == '' and method == 'PUT':
            self.present = True
            return 200, {'bank_id': FOREIGN}, None  # Deliberately hostile answer target.
        if route == '' and method == 'DELETE':
            if self.fail_delete:
                return 503, {}, None
            self.present = False
            self.docs, self.ops = {}, {}
            return 200, {'success': True, 'bank_id': FOREIGN}, None
        if route == '/transfer/import' and method == 'POST':
            raw = body[body.index(b'PK'):].rsplit(b'\r\n--', 1)[0]
            with zipfile.ZipFile(io.BytesIO(raw)) as archive:
                saved = X.load(archive.read('data.json'))
            if q['mode'] == ['restore']:
                assert not self.present
                self.docs = {}
                self.config = saved['config']
                self.ops = saved['operations']
            self.present = True
            self.docs.update(saved['documents'])
            return 202, self.operation(), None
        if not self.present:
            if self.leave_marker and route.startswith('/knowledge-base/search'):
                return 200, {'items': [{'text': 'invented marker residue'}]}, None
            return 404, {}, None
        if route == '/webhooks':
            return 200, {'webhooks': []}, None
        if route == '/config':
            if method == 'PATCH':
                self.config.update(body['updates'])
            return 200, {'config': self.config}, None
        if route == '/memories' and method == 'POST':
            for item in body.get('items', []):
                if item.get('timestamp') == 'not-a-date':
                    return 422, {}, None
                rid = item['document_id']
                self.docs[rid] = {'id': rid, 'original_text': item['content'], 'tags': item['tags']}
            return 200, dict(self.operation(body['operation_id']), bank_id=FOREIGN, success=True), None
        if route == '/operations':
            return 200, envelope(list(self.ops.values()), q, 'operations'), None
        if route.startswith('/operations/'):
            return 200, self.ops[urllib.parse.unquote(route.rsplit('/', 1)[1])], None
        if route == '/documents':
            return 200, envelope(list(self.docs.values()), q), None
        if route.startswith('/documents/'):
            rid = urllib.parse.unquote(route.split('/')[2])
            return (200, self.docs[rid], None) if rid in self.docs else (404, {}, None)
        if route == '/memories/list':
            ids = q.get('document_id', list(self.docs))
            return 200, envelope([self.fact(rid) for rid in ids if rid in self.docs], q), None
        if route == '/transfer/export':
            selected = q.get('document_id', list(self.docs))
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, 'w') as z:
                z.writestr('data.json', X.encode({'bank_id': self.bank,
                    'documents': {rid: self.docs[rid] for rid in selected},
                    'config': self.config, 'operations': self.ops}))
            key = '/v1/default/files/download/invented-' + str(len(self.downloads))
            self.downloads[key] = archive.getvalue()
            return 202, self.operation(meta={'download_url': key}), None
        if route == '/memories/recall':
            return 200, {'results': [self.fact(r) for r in self.docs][:2]}, None
        if route == '/llm-requests':
            return 200, envelope([{'id': 'trace', 'scope': 'retain_extract_facts',
                'input_tokens': 12, 'output_tokens': 3, 'provider': 'invented', 'model': 'invented'}], q), None
        raise AssertionError('unhandled_fake_route')

    def mcp(self, body, sid):
        method = body['method']
        if method == 'initialize':
            self.session_count += 1
            return 200, {'jsonrpc': '2.0', 'id': 1, 'result': {}}, 'session-' + str(self.session_count)
        if method == 'notifications/initialized':
            return 202, {}, sid
        enabled = self.config.get('mcp_enabled_tools')
        if method == 'tools/list':
            names = self.catalog if enabled is None or not self.filter_list else enabled
            return 200, {'jsonrpc': '2.0', 'id': 1, 'result': {'tools': [self.catalog[n] for n in names]}}, sid
        name = body['params']['name']
        if self.filter_call and enabled is not None and name not in enabled:
            return 200, {'jsonrpc': '2.0', 'id': 1, 'error': {'code': -32602, 'message': 'Unknown tool: ' + name}}, sid
        # Executed forbidden tools return success, never get mistaken for denial.
        if name == 'get_document' and not self.docs:
            result = {'isError': False, 'structuredContent': {'error': 'Document not found'}}
        elif name == 'get_document':
            result = {'isError': False, 'structuredContent': self.docs['invented-a']}
        else:
            result = {'isError': False, 'structuredContent': {'results': [self.fact(r) for r in self.docs]}}
        return 200, {'jsonrpc': '2.0', 'id': 1, 'result': result}, sid


class ExerciseTests(unittest.TestCase):
    def setUp(self):
        self.socket = patch.object(X.P.socket, 'create_connection', side_effect=AssertionError('offline_only'))
        self.blocked = self.socket.start()
        self.addCleanup(self.socket.stop)
        self.addCleanup(self.blocked.assert_not_called)
        self.clock = Clock()
        self.server = FakeServer(self.clock)
        self.exercise = X.Exercise(BANK, self.server, self.clock, self.clock.sleep)

    def run_driver(self, action=None):
        if action:
            self.exercise.e1 = action
        return self.exercise.run()

    def assert_cleanup(self, code, expected=3):
        self.assertEqual(code, expected)
        self.assertEqual(self.exercise.evidence['cleanup'], 'proven' if expected != 4 else 'incomplete')
        self.assertTrue(any(m == 'DELETE' for m, _, _, _ in self.server.calls))

    def test_full_fake_run_and_ownership_c2(self):
        self.assert_cleanup(self.run_driver(), 0)
        self.assertEqual(set(self.exercise.evidence['steps']), set(X.STEPS))
        for step in X.STEPS:
            self.assertEqual(self.exercise.evidence['steps'][step]['verdict'], 'measured')
        writes = [x for x in self.server.calls if x[0] != 'GET' and '/memories/recall' not in x[1]]
        self.assertTrue(writes)
        for method, path, body, started in writes:
            self.assertTrue(path.startswith(self.exercise.base) or path == self.exercise.mcp, 'foreign_mutation')
            q = urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)
            if '/transfer/import' in path:
                self.assertEqual(q['target_bank_id'], [BANK], 'foreign_import')
        times = [x[3] for x in writes]
        self.assertTrue(all(b - a >= 1 for a, b in zip(times, times[1:])))
        self.assertEqual(self.exercise.retains, 33)  # Includes denied retain/sync_retain probes.
        self.assertLess(self.clock(), 900)
        self.assertFalse(self.server.present)
        self.assertEqual(len(self.exercise.evidence['archives']), 3)

    def test_c1_invalid_and_initially_present_no_mutations(self):
        with self.assertRaises(X.P.Stop):
            X.Exercise('other-bank', self.server, self.clock, self.clock.sleep)
        self.assertEqual(self.server.calls, [])
        self.server.present = True
        self.assertEqual(self.run_driver(), 2)
        self.assertTrue(self.server.calls)
        self.assertFalse(any(m != 'GET' for m, _, _, _ in self.server.calls), 'present_bank_mutated')

    def test_c2_rejects_foreign_route_and_import_query(self):
        self.exercise.preflight()
        for route in (X.BANKS + '/' + FOREIGN, '/../' + FOREIGN, '@foreign'):
            with self.assertRaises(X.P.Stop):
                self.exercise.change('DELETE', route)
        with self.assertRaises(X.P.Stop):
            self.exercise.change('PATCH', '/config', {}, query={'bank_id': FOREIGN})
        self.assertFalse(any(m != 'GET' for m, _, _, _ in self.server.calls))

    def test_c3_41st_retain_reaches_cleanup(self):
        def forty_one():
            self.exercise.create()
            for i in range(41):
                self.exercise.retain('invented-a', self.exercise.marker + ' note ' + str(i))
        self.assert_cleanup(self.run_driver(forty_one))
        retained = [x for x in self.server.calls if x[0] == 'POST' and x[1].endswith('/memories')]
        self.assertEqual(len(retained), 40, 'retain_41_sent')

    def test_c3_spacing_clock_failure_reaches_cleanup(self):
        self.exercise.sleep = lambda _: None
        self.assertEqual(self.run_driver(), 4)
        self.assertEqual(self.exercise.evidence['cleanup'], 'incomplete')
        writes = [x for x in self.server.calls if x[0] != 'GET' and '/memories/recall' not in x[1]]
        self.assertEqual(len(writes), 1, 'early_write_sent')

    def test_c3_three_write_errors_reach_cleanup(self):
        def errors():
            self.exercise.create()
            for i in range(4):
                self.exercise.change('POST', '/memories', {'items': [{'timestamp': 'not-a-date'}]})
        self.assert_cleanup(self.run_driver(errors))
        rejected = [x for x in self.server.calls if x[0] == 'POST']
        self.assertEqual(len(rejected), 3, 'fourth_error_write_sent')

    def test_c3_poll_limit_and_total_limit(self):
        def pending():
            self.exercise.create()
            self.server.pending = True
            self.exercise.submitted(self.exercise.change('POST', '/memories',
                {'items': [], 'operation_id': 'pending'}))
        self.assertEqual(self.run_driver(pending), 4)
        self.assertLessEqual(self.clock(), 245, 'poll_exceeded_bound')
        self.assertEqual(self.exercise.evidence['cleanup'], 'incomplete')
        self.setUp()
        def over_total():
            self.exercise.create()
            self.clock.now = 780
            self.exercise.change('PATCH', '/config', {'updates': {}})
        self.assert_cleanup(self.run_driver(over_total))
        self.assertLess(self.clock(), 900)
        self.assertFalse(any(t >= 780 and m == 'PATCH' for m, _, _, t in self.server.calls), 'late_mutation')

    def test_c4_exception_in_e1_e2_e5_and_interrupt(self):
        for stage in ('E1', 'E2', 'E5', 'interrupt'):
            with self.subTest(stage=stage):
                clock = Clock()
                server = FakeServer(clock)
                exercise = X.Exercise(BANK, server, clock, clock.sleep)
                if stage in ('E1', 'interrupt'):
                    def e1():
                        exercise.create()
                        raise KeyboardInterrupt if stage == 'interrupt' else ValueError(SECRET)
                    exercise.e1 = e1
                else:
                    def failure(*_):
                        raise ValueError(SECRET)
                    setattr(exercise, 'filtering' if stage == 'E2' else 'e5', failure)
                self.assertEqual(exercise.run(), 3, 'exception_not_cleaned')
                self.assertEqual(exercise.evidence['cleanup'], 'proven')
                self.assertFalse(server.present)
                self.assertNotIn(SECRET, json.dumps(exercise.evidence))

    def test_c4_failed_delete_and_residue_are_exit4(self):
        for mode in ('delete', 'marker'):
            clock = Clock()
            server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            def fail():
                exercise.create()
                server.fail_delete = mode == 'delete'
                server.leave_marker = mode == 'marker'
                raise ValueError('invented')
            exercise.e1 = fail
            self.assertEqual(exercise.run(), 4, 'cleanup_failure_hidden')
            self.assertEqual(exercise.evidence['cleanup'], 'incomplete')

    def test_c5_foreign_count_change_is_exit4(self):
        def changed():
            self.exercise.create()
            self.server.foreign_count += 1
            raise ValueError('invented')
        self.assert_cleanup(self.run_driver(changed), 4)

    def test_c6_fd_evidence_stdout_stderr_redacted_and_private(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp, 'evidence.json')
            read_fd, write_fd = os.pipe()
            os.write(write_fd, SECRET.encode())
            os.close(write_fd)
            output = io.StringIO()
            try:
                with patch.object(X, 'Wire', return_value=self.server), \
                     patch.object(X, 'Exercise', return_value=self.exercise), \
                     patch.object(X.P, 'locked', contextlib.nullcontext), \
                     contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
                    code = X.main(['--bank', BANK, '--credential-fd', str(read_fd), '--evidence', str(path)])
            finally:
                os.close(read_fd)
            self.assertEqual(code, 0)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            text = path.read_text() + output.getvalue()
            for n in range(8, len(SECRET) + 1):
                self.assertNotIn(SECRET[:n], text, 'credential_disclosed')
            for _, _, body, _ in self.server.calls:
                if isinstance(body, dict):
                    for item in body.get('items', []):
                        if 'content' in item and item['content'] != self.exercise.marker:
                            self.assertFalse(item['content'] in text, 'note_disclosed')
            self.assertEqual(json.loads(path.read_text())['credential_prefix_matches'], 0)

    def test_c6_server_reflected_key_names_and_exception_redacted(self):
        self.server.present = True
        original = self.server.exchange
        def reflected(*args, **kwargs):
            status, raw, sid = original(*args, **kwargs)
            value = X.load(raw)
            value[SECRET] = SECRET
            return status, X.encode(value), sid
        self.server.exchange = reflected
        self.assertEqual(self.run_driver(), 2)
        self.assertNotIn(SECRET, json.dumps(self.exercise.evidence))

    def test_c7_list_or_call_bypass_fails_e2(self):
        for guard in ('filter_list', 'filter_call'):
            clock = Clock()
            server = FakeServer(clock)
            setattr(server, guard, False)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            self.assertEqual(exercise.run(), 3, 'tool_filter_failure_accepted')
            self.assertEqual(exercise.evidence['steps']['E2']['verdict'], 'failed')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')

    def test_transport_reuses_publisher_no_redirects_and_sse(self):
        wire = X.Wire(SECRET)
        self.assertIsInstance(wire.client, X.P.Client)
        response = type('Response', (), {'status': 200, 'headers': {},
                    'read': lambda _, n: b'data: {"jsonrpc":"2.0","id":1,"result":{}}\n\n'})()
        with patch.object(wire.client.opener, 'open', return_value=contextlib.nullcontext(response)) as opened:
            exercise = X.Exercise(BANK, wire, self.clock, self.clock.sleep)
            self.assertEqual(exercise.rpc('initialize', {})[0]['result'], {})
            self.assertTrue(opened.call_args.args[0].full_url.startswith(X.P.ORIGIN))
        self.assertIsNone(X.P.NoRedirect().redirect_request(None, None, 302, '', {}, 'https://example.invalid/'))

    def test_uncertain_write_never_deletes_or_resends(self):
        def lost(method, path, body):
            if method == 'PUT':
                self.server.present = True
                raise TimeoutError(SECRET)
        self.server.fail = lost
        self.assertEqual(self.run_driver(), 4)
        self.assertEqual(sum(m == 'PUT' for m, _, _, _ in self.server.calls), 1)
        self.assertFalse(any(m == 'DELETE' for m, _, _, _ in self.server.calls))
        self.assertEqual(self.exercise.evidence['cleanup'], 'incomplete')

    def test_existing_evidence_refuses_before_requests(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp, 'exists')
            path.write_text('retained')
            with patch.object(X.P, 'credential', return_value=SECRET), patch.object(X, 'Wire') as wire, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]), 2)
            wire.assert_not_called()
            self.assertEqual(path.read_text(), 'retained')

    def test_signal_handler_cleanup(self):
        with tempfile.TemporaryDirectory() as tmp:
            def interrupted():
                self.exercise.create()
                os.kill(os.getpid(), signal.SIGTERM)
            self.exercise.e1 = interrupted
            path = Path(tmp, 'evidence')
            with patch.object(X.P, 'credential', return_value=SECRET), patch.object(X, 'Wire', return_value=self.server), \
                 patch.object(X, 'Exercise', return_value=self.exercise), patch.object(X.P, 'locked', contextlib.nullcontext), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]), 3)
            self.assertEqual(json.loads(path.read_text())['cleanup'], 'proven')

    def test_whole_http_response_budget_expires_and_restores_handler(self):
        previous = signal.getsignal(signal.SIGALRM)
        with self.assertRaisesRegex(X.P.Stop, '^transport_uncertain$'):
            with X.http_budget(.01):
                signal.pause()
        self.assertIs(signal.getsignal(signal.SIGALRM), previous)
        self.assertEqual(signal.getitimer(signal.ITIMER_REAL), (0, 0))

    def test_control_guard_removals_are_detected_in_process(self):
        # Run the real assertion controls with ONE named guard disabled in memory.
        # No mutant source file, subprocess, socket or real target is used.
        original = X.P.require
        def bypass(codes):
            return lambda condition, code: original(condition or code in codes, code)
        cases = [
            ('c1', 'test_c1_invalid_and_initially_present_no_mutations',
             patch.object(X.P, 'require', side_effect=bypass({'bank_id_rejected'}))),
            ('c1-present', 'test_c1_invalid_and_initially_present_no_mutations',
             patch.object(X.P, 'require', side_effect=bypass({'bank_already_present'}))),
            ('c2', 'test_c2_rejects_foreign_route_and_import_query',
             patch.object(X.P, 'require', side_effect=bypass({'mutation_route_rejected'}))),
            ('c3-retains', 'test_c3_41st_retain_reaches_cleanup',
             patch.object(X.P, 'require', side_effect=bypass({'retain_bound'}))),
            ('c3-gap', 'test_c3_spacing_clock_failure_reaches_cleanup',
             patch.object(X.P, 'require', side_effect=bypass({'write_gap_bound'}))),
            ('c3-errors', 'test_c3_three_write_errors_reach_cleanup',
             patch.object(X.P, 'require', side_effect=bypass({'write_error_bound'}))),
            ('c3-poll', 'test_c3_poll_limit_and_total_limit',
             patch.object(X.Exercise, 'poll_work', lambda exercise, _: copy.deepcopy(exercise.wire.ops))),
            ('c3-total', 'test_c3_poll_limit_and_total_limit',
             patch.object(X.P, 'require', side_effect=bypass({'total_time_bound'}))),
            ('c4', 'test_c4_exception_in_e1_e2_e5_and_interrupt',
             patch.object(X.Exercise, 'cleanup', lambda _: None)),
            ('c4-residue', 'test_c4_failed_delete_and_residue_are_exit4',
             patch.object(X.P, 'require', side_effect=bypass({'absence_unproven'}))),
            ('c5', 'test_c5_foreign_count_change_is_exit4',
             patch.object(X.P, 'require', side_effect=bypass({'other_bank_counts_changed'}))),
            ('c6', 'test_c6_server_reflected_key_names_and_exception_redacted',
             patch.object(X, 'keys', lambda value: value if isinstance(value, dict) else {})),
            ('c7-list', 'test_c7_list_or_call_bypass_fails_e2',
             patch.object(X.P, 'require', side_effect=bypass({'tool_filter_list_failed'}))),
            ('c7-call', 'test_c7_list_or_call_bypass_fails_e2',
             patch.object(X.P, 'require', side_effect=bypass({'tool_filter_call_failed'}))),
        ]
        for name, test, mutation in cases:
            with self.subTest(control=name), mutation:
                result = unittest.TestResult()
                ExerciseTests(test).run(result)
            self.assertGreater(len(result.failures) + len(result.errors), 0, 'guard_removal_survived_' + name)
            trace = (result.failures + result.errors)[0][1]
            locations = X.re.findall(r'test_memory_exercise.py\", line ([0-9]+)', trace)
            print('control_removed=' + name + ' failing_line=ci/test_memory_exercise.py:' + locations[-1])



if __name__ == '__main__':
    unittest.main()
