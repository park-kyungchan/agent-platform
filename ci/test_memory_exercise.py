"""Invented offline HTTP/SSE/ZIP server; sockets are forbidden, including in controls."""
import contextlib
import copy
import importlib.util
import io
import inspect
import textwrap
import types
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
        if name not in self.catalog or (self.filter_call and enabled is not None and name not in enabled):
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
        self.assertEqual(self.exercise.retains, 23)  # Includes denied retain/sync_retain probes.
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

    def test_k1_foreign_schema_and_all_changing_bodies(self):
        self.server.catalog['delete_bank'] = {'name': 'delete_bank', 'inputSchema': {'type': 'object',
            'properties': {'bank': {'type': 'string', 'enum': [FOREIGN]}}, 'required': ['bank']}}
        self.assertEqual(self.run_driver(), 3)
        self.assertIn({'tool': 'delete_bank', 'verdict': 'refused_unmeasured'},
                      self.exercise.evidence.get('tool_probes', []), 'unsafe_sampler_not_refused')
        self.assertFalse(any(FOREIGN.encode() in (b if isinstance(b, bytes) else X.encode(b))
                             for m, _, b, _ in self.server.calls if m != 'GET'), 'foreign_id_sent')
        for route in ('', '/config', '/memories', '/transfer/export', '/transfer/import', '@mcp'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep); exercise.preflight()
            with self.assertRaisesRegex(X.P.Stop, '^foreign_bank_in_body$'):
                exercise.change('POST', route, {'payload': FOREIGN})
            self.assertFalse(any(m != 'GET' for m, _, _, _ in server.calls), 'foreign_body_dispatched')
        for schema in [
            {'type': 'object', 'properties': {'other_bank': {'type': 'string'}}},
            {'type': 'object', 'properties': {'content': {'type': 'string', 'enum': [FOREIGN]}}, 'required': ['content']},
        ]:
            with self.assertRaisesRegex(X.P.Stop, '^tool_arguments_unmeasured$'):
                self.exercise.arguments(schema)
        sampled = self.exercise.arguments({'type': 'object', 'properties': {
            'content': {'type': 'string', 'default': FOREIGN, 'example': FOREIGN}}, 'required': ['content']})
        self.assertEqual(sampled['content'], self.exercise.marker, 'server_value_sampled')

    def test_k2_executor_error_and_nonmatching_rpc_errors_are_not_denial(self):
        for mode in ('executed', 'code', 'shape', 'message'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.mcp
            def reply(body, sid):
                status, answer, returned = original(body, sid)
                if body['method'] == 'tools/call':
                    name = body['params']['name']
                    if name in server.catalog and name not in (server.config.get('mcp_enabled_tools') or []):
                        if mode == 'executed':
                            answer = {'jsonrpc': '2.0', 'id': 1, 'result': {'isError': True,
                                'content': [{'type': 'text', 'text': 'Error executing tool ' + name + ': Document not found'}]}}
                        elif mode == 'code':
                            answer['error']['code'] = -999
                        elif mode == 'shape':
                            answer['error']['extra'] = 'invented'
                        else:
                            answer['error']['message'] = 'Other denial: ' + name
                return status, answer, returned
            server.mcp = reply
            self.assertEqual(exercise.run(), 3, 'executed_or_unmatched_error_accepted')
            self.assertEqual(exercise.evidence['steps']['E2']['verdict'], 'failed')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')

    def test_k3_last_cleanup_bank_list_cannot_reappear(self):
        original = self.server.answer
        def reappear(method, path, body=None, **kwargs):
            if self.exercise.cleaning and path.startswith(X.BANKS + '?') and len(self.server.calls) > 1:
                if '/knowledge-base/search?' in self.server.calls[-2][1]:
                    self.server.present = True
            return original(method, path, body, **kwargs)
        self.server.answer = reappear
        self.assertEqual(self.run_driver(), 4, 'final_reappearance_hidden')
        self.assertEqual(self.exercise.evidence['cleanup_failure'], 'absence_unproven')
        self.assertTrue(self.server.calls[-1][1].startswith(X.BANKS + '?'), 'last_read_not_bank_list')

    def test_k4_signals_at_finally_and_cleanup_first_statement(self):
        for point in ('finally', 'cleanup-entry', 'cleanup-running'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            marks = []
            def action():
                exercise.create()
                marks.append(exercise.evidence['cleanup'])
                self.assertEqual(exercise.evidence['cleanup'], 'incomplete', 'create_not_marked_incomplete')
            exercise.e1 = action
            exercise.e4 = exercise.e5 = exercise.e6 = lambda: None
            fired = False
            def trace(frame, event, arg):
                nonlocal fired
                if frame.f_code.co_filename == str(ROOT / 'scripts/exercise-memory.py') and event == 'line' and not fired:
                    line = inspect.getsource(frame.f_code).splitlines()[frame.f_lineno - frame.f_code.co_firstlineno]
                    hit = ((point == 'finally' and frame.f_code.co_name == 'run' and line.strip() == 'if self.attempted:')
                        or (point == 'cleanup-entry' and frame.f_code.co_name == 'cleanup' and not exercise.cleaning)
                        or (point == 'cleanup-running' and frame.f_code.co_name == 'cleanup' and exercise.cleaning))
                    if hit:
                        fired = True
                        os.kill(os.getpid(), signal.SIGTERM)
                return trace
            with tempfile.TemporaryDirectory() as tmp, patch.object(X.P, 'credential', return_value=SECRET), \
                 patch.object(X, 'Wire', return_value=server), patch.object(X, 'Exercise', return_value=exercise), \
                 patch.object(X.P, 'locked', contextlib.nullcontext), contextlib.redirect_stdout(io.StringIO()):
                path = Path(tmp, 'evidence')
                old_trace = X.sys.gettrace()
                try:
                    X.sys.settrace(trace)
                    code = X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)])
                finally:
                    X.sys.settrace(old_trace)
                saved = X.load(path.read_bytes())
            self.assertEqual(marks, ['incomplete'], 'create_not_marked_incomplete')
            self.assertTrue(fired, 'signal_point_not_reached')
            self.assertEqual(code, 3, 'signal_skipped_cleanup')
            self.assertEqual(saved['cleanup'], 'proven', 'signal_left_bank_unproven')
            self.assertFalse(server.present)

    def test_k5_full_catalog_forecast_and_probe_coverage(self):
        for name in X.SUPPORTED_TOOLS:
            self.server.catalog.setdefault(name, {'name': name, 'inputSchema': {'type': 'object', 'properties': {}}})
        self.assertEqual(self.run_driver(), 0)
        self.assertLessEqual(len(self.server.calls), 300, 'request_budget_exceeded')
        self.assertLessEqual(self.clock(), 240, 'spacing_budget_exceeded')
        self.assertEqual(self.exercise.evidence['E2_classes'], {'retain': 'retain', 'delete': 'delete_document',
                         'reflect': 'reflect', 'page': 'create_knowledge_page'})
        probes = [r for r in self.exercise.evidence['requests'] if r.get('rpc_method') == 'tools/call']
        # Nonrepresentative tools only appear on the two full new-session policy passes.
        self.assertEqual(sum(r.get('tool') == 'update_memory' for r in probes), 2, 'full_sweep_repeated_or_lost')
        for name in ('retain', 'delete_document', 'reflect', 'create_knowledge_page'):
            self.assertEqual(sum(r.get('tool') == name for r in probes), 12, 'class_probe_lost')
        grouped = {}
        for row in self.exercise.evidence['requests']:
            step = grouped.setdefault(row['step'], {'requests': 0, 'writes': 0})
            step['requests'] += 1
            step['writes'] += row['method'] != 'GET' and '/memories/recall' not in row['path']
        print('full_catalog_forecast=' + json.dumps({'requests': len(self.server.calls), 'retains': self.exercise.retains,
                                                  'spacing_s': self.clock(), 'steps': grouped}, sort_keys=True))

    def test_k6_id_rejects_suffix_and_sampler_forces_bank_id(self):
        for invalid in (BANK + '\n', BANK + '/x', BANK + '0'):
            with self.assertRaisesRegex(X.P.Stop, '^bank_id_rejected$'):
                X.Exercise(invalid, self.server, self.clock, self.clock.sleep)
        self.server.catalog['delete_bank'] = {'name': 'delete_bank', 'inputSchema': {'type': 'object',
            'properties': {'bank_id': {'type': 'string'}}, 'required': ['bank_id']}}
        self.assertEqual(self.run_driver(), 0)
        calls = [b['params']['arguments']['bank_id'] for m, p, b, _ in self.server.calls
                 if isinstance(b, dict) and b.get('method') == 'tools/call' and b['params']['name'] == 'delete_bank']
        self.assertTrue(calls)
        self.assertEqual(set(calls), {BANK}, 'bank_id_not_forced')

    def test_k6_download_target_cannot_escape(self):
        self.exercise.preflight(); self.exercise.create()
        text = self.exercise.marker + ' invented download subject.'
        self.exercise.retain('invented-a', text)
        original = self.server.answer
        def outside(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            if '/transfer/export' in path:
                meta = self.server.ops[answer['operation_id']]['result_metadata']
                raw = self.server.downloads[meta['download_url']]
                meta['download_url'] = '/outside/download'
                self.server.downloads['/outside/download'] = raw
            return status, answer, sid
        self.server.answer = outside
        with self.assertRaisesRegex(X.P.Stop, '^download_target_rejected$'):
            self.exercise.export({'invented-a': text})
        self.assertFalse(any(p == '/outside/download' for _, p, _, _ in self.server.calls), 'outside_download_sent')

    def test_k6_restore_config_history_and_webhook_guards(self):
        for kind, failure in [('config', 'restore_config_changed'), ('history', 'restored_history_missing'),
                              ('webhooks', 'restored_webhooks_changed')]:
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.answer
            restored = False
            def changed(method, path, body=None, **kwargs):
                nonlocal restored
                status, answer, sid = original(method, path, body, **kwargs)
                if '/transfer/import?' in path and 'mode=restore' in path:
                    restored = True
                    if kind == 'config':
                        server.config['invented_config'] = 'changed'
                    if kind == 'history':
                        old = next(k for k in server.ops if len(k) == 36)
                        del server.ops[old]
                if restored and kind == 'webhooks' and path.endswith('/webhooks'):
                    answer = {'webhooks': [{'id': 'invented-hook'}]}
                return status, answer, sid
            server.answer = changed
            self.assertEqual(exercise.run(), 3, 'restore_difference_accepted_' + kind)
            self.assertEqual(exercise.evidence['steps']['E1']['failure'], failure)
            self.assertEqual(exercise.evidence['cleanup'], 'proven')

    def test_k6_document_absence_delete_status_and_uncertainty(self):
        self.exercise.preflight(); self.exercise.create()
        original = self.server.answer
        def retained(method, path, body=None, **kwargs):
            if not self.server.present and '/documents/invented-a' in path:
                return 200, {'original_text': 'invented residue'}, None
            return original(method, path, body, **kwargs)
        self.server.answer = retained
        self.server.present = False
        with self.assertRaisesRegex(X.P.Stop, '^absence_unproven$'):
            self.exercise.absence()
        self.server.answer = original
        self.server.present = True
        def bad_delete(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            return (503, answer, sid) if method == 'DELETE' else (status, answer, sid)
        self.server.answer = bad_delete
        with self.assertRaisesRegex(X.P.Stop, '^delete_failed$'):
            self.exercise.delete()
        self.exercise.uncertain = True
        before = len(self.server.calls)
        with self.assertRaisesRegex(X.P.Stop, '^unresolved_mutation$'):
            self.exercise.change('PATCH', '/config', {'updates': {}})
        self.assertEqual(len(self.server.calls), before, 'mutation_after_uncertainty')

    def test_k7_failed_rows_and_hashed_count_directions(self):
        for outcome in ('5xx', 'timeout', 'unreadable'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            def failure(*args, **kwargs):
                if outcome == 'timeout':
                    raise TimeoutError(SECRET)
                return 503 if outcome == '5xx' else 200, b'not-json' if outcome == 'unreadable' else b'{}', None
            server.exchange = failure
            self.assertEqual(exercise.run(), 2)
            self.assertEqual(exercise.evidence['requests'][0]['verdict'], 'failed', 'failed_row_measured')
        clock = Clock(); server = FakeServer(clock)
        exercise = X.Exercise(BANK, server, clock, clock.sleep)
        original = exercise.counts
        changed = False
        def counts():
            counts = original()
            if exercise.cleaning and counts.get(BANK) is None:
                counts.update({'grew-bank': 3, 'shrank-bank': 1, 'new-bank': 5})
                counts.pop('missing-bank', None)
            return counts
        exercise.counts = counts
        exercise.preflight()
        exercise.before.update({'grew-bank': 2, 'shrank-bank': 2, 'missing-bank': 2})
        exercise.create(); exercise.cleanup()
        comparison = exercise.evidence['other_bank_comparison']
        self.assertEqual(set(comparison.values()), {'unchanged', 'grew', 'shrank', 'missing', 'new'}, 'directions_missing')
        self.assertTrue(all(X.re.fullmatch('[0-9a-f]{64}', k) for k in comparison), 'bank_id_not_hashed')
        self.assertEqual(exercise.evidence['cleanup'], 'incomplete')
        self.assertNotIn(FOREIGN, json.dumps(exercise.evidence))

    def test_l1_deflated_entries_of_every_kind_reject_foreign_ids(self):
        for filename, escaped in [('data.json', False), ('opaque.bin', False), ('opaque.bin', True), ('opaque.bin', 'unicode')]:
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.answer
            foreign = '\uac00bank' if escaped == 'unicode' else 'foreign"bank' if escaped else FOREIGN
            def archive_reply(method, path, body=None, **kwargs):
                status, answer, sid = original(method, path, body, **kwargs)
                if path.startswith(X.BANKS + '?') and escaped:
                    for bank in answer['banks']:
                        if bank['bank_id'] == FOREIGN:
                            bank['bank_id'] = foreign
                if '/transfer/export?' in path:
                    key = server.ops[answer['operation_id']]['result_metadata']['download_url']
                    output = io.BytesIO()
                    with zipfile.ZipFile(io.BytesIO(server.downloads[key])) as old, \
                         zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as new:
                        data = X.load(old.read('data.json'))
                        if filename == 'data.json':
                            data['owner'] = foreign
                        new.writestr('data.json', X.encode(data))
                        if filename != 'data.json':
                            new.writestr(filename, json.dumps({'owner': foreign}, ensure_ascii=escaped == 'unicode').encode())
                    server.downloads[key] = output.getvalue()
                    self.assertNotIn(foreign.encode(), output.getvalue())
                return status, answer, sid
            server.answer = archive_reply
            self.assertEqual(exercise.run(), 3, 'compressed_foreign_archive_accepted')
            self.assertEqual(exercise.evidence['steps']['E1']['failure'], 'archive_foreign_bank')
            self.assertFalse(any('/transfer/import?' in p for _, p, _, _ in server.calls), 'unsafe_archive_imported')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')

    def test_l1_unreadable_or_oversized_entry_is_never_imported(self):
        for kind in ('oversized', 'unreadable'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.answer
            def bad_archive(method, path, body=None, **kwargs):
                status, answer, sid = original(method, path, body, **kwargs)
                if '/transfer/export?' in path:
                    key = server.ops[answer['operation_id']]['result_metadata']['download_url']
                    if kind == 'oversized':
                        output = io.BytesIO()
                        with zipfile.ZipFile(io.BytesIO(server.downloads[key])) as old, \
                             zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as new:
                            new.writestr('data.json', old.read('data.json'))
                            new.writestr('opaque.bin', b'x' * (X.MAX_BYTES + 1))
                        server.downloads[key] = output.getvalue()
                    else:
                        raw = bytearray(server.downloads[key])
                        central = raw.index(b'PK\x01\x02')
                        raw[central + 16] ^= 0xff  # Valid container, unreadable member CRC.
                        server.downloads[key] = bytes(raw)
                return status, answer, sid
            server.answer = bad_archive
            self.assertEqual(exercise.run(), 3, 'bad_entry_accepted')
            self.assertFalse(any('/transfer/import?' in p for _, p, _, _ in server.calls))
            self.assertEqual(exercise.evidence['cleanup'], 'proven')

    def test_l2_answered_write_interrupt_and_step_deadline_cleanup(self):
        for kind in ('interrupt', 'step-deadline'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.exchange
            def answered(method, path, body=None, **kwargs):
                result = original(method, path, body, **kwargs)
                if method == 'PUT':
                    if kind == 'interrupt':
                        exercise.interrupt()
                    else:
                        clock.now += .75
                return result
            server.exchange = answered
            e1 = exercise.e1
            def at_boundary():
                if kind == 'step-deadline':
                    clock.now = 779.5
                e1()
            exercise.e1 = at_boundary
            self.assertEqual(exercise.run(), 3, 'answered_write_left_bank')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')
            put = next(r for r in exercise.evidence['requests'] if r['method'] == 'PUT')
            self.assertTrue(put['answered'], 'received_answer_lost')
            self.assertEqual(put['status'], 200)
            self.assertFalse(exercise.uncertain, 'completed_write_marked_uncertain')
            self.assertFalse(server.present)

    def test_l3_hup_and_quit_use_deferred_interrupt_handler(self):
        for sig in (signal.SIGHUP, signal.SIGQUIT):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.exchange
            def answered(method, path, body=None, **kwargs):
                result = original(method, path, body, **kwargs)
                if method == 'PUT':
                    os.kill(os.getpid(), sig)
                return result
            server.exchange = answered
            # A missing registration fails this test without killing the test runner.
            def previous_handler(*_):
                raise KeyboardInterrupt
            previous = signal.signal(sig, previous_handler)
            try:
                with tempfile.TemporaryDirectory() as tmp, patch.object(X.P, 'credential', return_value=SECRET), \
                     patch.object(X, 'Wire', return_value=server), patch.object(X, 'Exercise', return_value=exercise), \
                     patch.object(X.P, 'locked', contextlib.nullcontext), contextlib.redirect_stdout(io.StringIO()):
                    path = Path(tmp, 'evidence')
                    code = X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)])
                    self.assertEqual(code, 3, 'signal_not_deferred')
                    self.assertEqual(X.load(path.read_bytes())['cleanup'], 'proven')
                self.assertIs(signal.getsignal(sig), previous_handler, 'signal_handler_not_restored')
            finally:
                signal.signal(sig, previous)
            self.assertFalse(server.present)

    def test_l4_old_session_catalog_and_session_specific_denials(self):
        for kind in ('old-list', 'session-denial'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.mcp
            baselines = []
            def sessions(body, sid):
                status, answer, returned = original(body, sid)
                if kind == 'old-list' and body['method'] == 'tools/list' and sid == 'session-1':
                    answer['result']['tools'] = list(server.catalog.values())
                if kind == 'session-denial' and 'error' in answer:
                    answer['error']['code'] -= int(sid.rsplit('-', 1)[1])
                    if body['params']['name'] == 'zz_nonexistent_exercise_probe':
                        baselines.append(sid)
                return status, answer, returned
            server.mcp = sessions
            self.assertEqual(exercise.run(), 3 if kind == 'old-list' else 0, 'session_qualification_lost')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')
            if kind == 'session-denial':
                self.assertEqual(set(baselines), set(exercise.sessions), 'session_baseline_missing')

    def test_l4_json_escaped_body_and_cleanup_discovery(self):
        original = self.server.answer
        foreign = 'foreign"bank'
        def renamed(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            if path.startswith(X.BANKS + '?'):
                for bank in answer['banks']:
                    if bank['bank_id'] == FOREIGN:
                        bank['bank_id'] = foreign
            return status, answer, sid
        self.server.answer = renamed
        self.exercise.preflight(); self.exercise.create()
        body = {'updates': {'invented': foreign}}
        self.assertNotIn(foreign.encode(), X.encode(body))
        with self.assertRaisesRegex(X.P.Stop, '^foreign_bank_in_body$'):
            self.exercise.change('PATCH', '/config', body)
        self.server.pending = True
        self.server.operation('cleanup-pending')
        self.exercise.active.add('cleanup-pending')
        self.exercise.uncertain = True
        self.exercise.cleanup()
        self.assertTrue(any('/operations?' in p for _, p, _, _ in self.server.calls), 'cleanup_discovery_skipped')
        self.assertEqual(self.exercise.evidence['cleanup'], 'incomplete')
        self.assertFalse(any(m == 'DELETE' for m, _, _, _ in self.server.calls))

    def test_l4_recall_subject_and_fixed_catalog_order(self):
        for kind in ('subject', 'order'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            if kind == 'order':
                for name in ('delete_bank', 'delete_knowledge_node', 'create_knowledge_page'):
                    server.catalog[name] = {'name': name, 'inputSchema': {'type': 'object', 'properties': {}}}
                # Deliberately reverse tuple preferences, with a literal sequence independent of hash seed.
                first = ('sync_retain', 'retain', 'delete_bank', 'delete_document', 'reflect',
                         'delete_knowledge_node', 'create_knowledge_page')
                server.catalog = {n: server.catalog[n] for n in first + tuple(n for n in server.catalog if n not in first)}
            original = server.mcp
            def wrong_subject(body, sid):
                status, answer, returned = original(body, sid)
                if kind == 'subject' and body['method'] == 'tools/call' and body['params']['name'] == 'recall':
                    for fact in answer.get('result', {}).get('structuredContent', {}).get('results', []):
                        fact['tags'] = ['subject:task:sibling']
                return status, answer, returned
            server.mcp = wrong_subject
            self.assertEqual(exercise.run(), 3 if kind == 'subject' else 0, 'recall_subject_scope_lost')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')
            if kind == 'order':
                self.assertEqual(exercise.evidence['E2_classes'], {'retain': 'retain', 'delete': 'delete_document',
                    'reflect': 'reflect', 'page': 'create_knowledge_page'}, 'catalog_order_used_for_class')

    def test_l5_forecast_latency_headroom(self):
        for latency in (0, .5, 1.5):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            for name in X.SUPPORTED_TOOLS:
                server.catalog.setdefault(name, {'name': name, 'inputSchema': {'type': 'object', 'properties': {}}})
            original = server.exchange
            cleanup_started = []
            cleanup = exercise.cleanup
            def start_cleanup():
                cleanup_started.append(clock())
                cleanup()
            exercise.cleanup = start_cleanup
            def delayed(*args, **kwargs):
                answer = original(*args, **kwargs)
                clock.now += latency
                return answer
            server.exchange = delayed
            self.assertEqual(exercise.run(), 0)
            self.assertEqual(len(server.calls), 297)
            self.assertEqual(300 - len(server.calls), 3, 'request_headroom_changed')
            if latency:
                # Conservative: charge every request AND all spacing against steps; no overlap credit.
                safe_extra_gets = int((780 - 180 - len(server.calls) * latency) // latency)
                self.assertEqual(safe_extra_gets, 903 if latency == .5 else 103)
                with_poll_sleep = int((780 - 180 - len(server.calls) * latency) // (latency + 1))
                self.assertEqual(with_poll_sleep, 301 if latency == .5 else 61)
                print('latency_forecast=' + json.dumps({'request_s': latency, 'step_s': cleanup_started[0],
                     'elapsed_s': clock(), 'conservative_extra_poll_gets': safe_extra_gets, 'with_1s_sleep_per_get': with_poll_sleep}, sort_keys=True))

    def test_r2_guard_removals_fail_in_process(self):
        original = X.P.require
        def edited(name, before, after):
            owner = X if name == 'main' else X.Exercise
            source = textwrap.dedent(inspect.getsource(getattr(owner, name)))
            self.assertIn(before, source)
            namespace = dict(vars(X))
            exec(compile(source.replace(before, after), '<memory-exercise-mutant>', 'exec'), namespace)
            function = namespace[name]
            function = types.FunctionType(function.__code__, vars(X), name, function.__defaults__)
            return patch.object(owner, name, function)
        def drop_archive_guard(condition, code):
            return original(condition or code == 'archive_foreign_bank', code)
        cases = [
            ('L1', 'test_l1_deflated_entries_of_every_kind_reject_foreign_ids',
             patch.object(X.P, 'require', side_effect=drop_archive_guard)),
            ('L2', 'test_l2_answered_write_interrupt_and_step_deadline_cleanup',
             edited('call', "row['answer_shape'] = keys(data)", "self.bound()\n        row['answer_shape'] = keys(data)")),
            ('L3', 'test_l3_hup_and_quit_use_deferred_interrupt_handler',
             edited('main', 'signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGQUIT',
                    'signal.SIGINT, signal.SIGTERM')),
            ('L4-old-list', 'test_l4_old_session_catalog_and_session_specific_denials',
             edited('filtering', 'set(listed) == set(allowed)', 'not fresh or set(listed) == set(allowed)')),
            ('L4-session-denial', 'test_l4_old_session_catalog_and_session_specific_denials',
             edited('filtering', "self.denials[sid] = self.denial(baseline, sentinel)",
                    "self.denials[sid] = next(iter(self.denials.values()), self.denial(baseline, sentinel))")),
            ('L4-escaped', 'test_l4_json_escaped_body_and_cleanup_discovery',
             edited('change', ' or encode(bank)[1:-1] in raw_body', '')),
            ('L4-cleanup-poll', 'test_l4_json_escaped_body_and_cleanup_discovery',
             edited('cleanup', 'self.poll()  # Read-only discovery', 'pass  # Read-only discovery')),
            ('L4-subject', 'test_l4_recall_subject_and_fixed_catalog_order',
             edited('filtering', " and 'subject:task:invented' in r.get('tags', [])", '')),
            ('L5', 'test_l5_forecast_latency_headroom',
             edited('filtering', 'full = fresh and tuple(allowed) not in self.full_policies', 'full = fresh')),
            ('L4-order', 'test_l4_recall_subject_and_fixed_catalog_order',
             edited('representatives', 'n for n in names if n in self.catalog', 'n for n in self.catalog if n in names')),
        ]
        for name, test, mutation in cases:
            with self.subTest(repair=name), mutation:
                result = unittest.TestResult()
                ExerciseTests(test).run(result)
            failures = result.failures + result.errors
            self.assertTrue(failures, 'r2_removal_survived_' + name)
            trace = failures[0][1]
            locations = X.re.findall(r'test_memory_exercise.py\", line ([0-9]+)', trace)
            print('r2_removed=' + name + ' failing_line=ci/test_memory_exercise.py:' + locations[-1]
                  + ' message=' + trace.splitlines()[-1])

    def test_r1_guard_removals_fail_in_process(self):
        original = X.P.require
        bypass = lambda code: patch.object(X.P, 'require', side_effect=lambda condition, actual:
            original(condition or actual == code, actual))
        def edited(name, before, after):
            source = textwrap.dedent(inspect.getsource(getattr(X.Exercise, name)))
            self.assertIn(before, source)
            namespace = dict(vars(X))
            exec(compile(source.replace(before, after), '<memory-exercise-mutant>', 'exec'), namespace)
            return patch.object(X.Exercise, name, namespace[name])
        cases = [
            ('K1-body', 'test_k1_foreign_schema_and_all_changing_bodies', bypass('foreign_bank_in_body')),
            ('K1-sampler', 'test_k1_foreign_schema_and_all_changing_bodies',
             patch.object(X.Exercise, 'arguments', lambda exercise, _: {'bank': FOREIGN})),
            ('K2', 'test_k2_executor_error_and_nonmatching_rpc_errors_are_not_denial',
             patch.object(X.Exercise, 'denied', lambda exercise, answer, name, sid:
                 bool(answer.get('result', {}).get('isError')) or 'error' in answer)),
            ('K3', 'test_k3_last_cleanup_bank_list_cannot_reappear', bypass('absence_unproven')),
            ('K4-signal', 'test_k4_signals_at_finally_and_cleanup_first_statement',
             patch.object(X.Exercise, 'interrupt', side_effect=KeyboardInterrupt)),
            ('K4-mark', 'test_k4_signals_at_finally_and_cleanup_first_statement',
             edited('change', "self.evidence['cleanup'] = 'incomplete'", 'pass')),
            ('K5', 'test_k5_full_catalog_forecast_and_probe_coverage',
             edited('filtering', 'full = fresh and tuple(allowed) not in self.full_policies', 'full = fresh')),
            ('K6-match', 'test_k6_id_rejects_suffix_and_sampler_forces_bank_id',
             patch.object(X, 'ID', types.SimpleNamespace(fullmatch=X.ID.match))),
            ('K6-bank', 'test_k6_id_rejects_suffix_and_sampler_forces_bank_id',
             edited('arguments', "if name in ('bank_id', 'target_bank_id'):", 'if False:')),
            ('K6-download', 'test_k6_download_target_cannot_escape', bypass('download_target_rejected')),
            ('K6-config', 'test_k6_restore_config_history_and_webhook_guards', bypass('restore_config_changed')),
            ('K6-history', 'test_k6_restore_config_history_and_webhook_guards', bypass('restored_history_missing')),
            ('K6-webhooks', 'test_k6_restore_config_history_and_webhook_guards', bypass('restored_webhooks_changed')),
            ('K6-document404', 'test_k6_document_absence_delete_status_and_uncertainty',
             edited('absence', 'absent and status == 404 and search == 404', 'absent and search == 404')),
            ('K6-delete', 'test_k6_document_absence_delete_status_and_uncertainty', bypass('delete_failed')),
            ('K6-unresolved', 'test_k6_document_absence_delete_status_and_uncertainty', bypass('unresolved_mutation')),
            ('K7-status', 'test_k7_failed_rows_and_hashed_count_directions',
             edited('call', "'failed' if status >= 500 else 'measured'", "'measured'")),
            ('K7-hash', 'test_k7_failed_rows_and_hashed_count_directions',
             edited('cleanup', 'P.digest(k.encode())', 'k')),
        ]
        for name, test, mutation in cases:
            with self.subTest(guard=name), mutation:
                result = unittest.TestResult()
                ExerciseTests(test).run(result)
            failures = result.failures + result.errors
            self.assertTrue(failures, 'repair_removal_survived_' + name)
            trace = failures[0][1]
            assertion = trace.splitlines()[-1]
            locations = X.re.findall(r'test_memory_exercise.py\", line ([0-9]+)', trace)
            print('repair_removed=' + name + ' failing_line=ci/test_memory_exercise.py:' + locations[-1]
                  + ' message=' + assertion)

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
