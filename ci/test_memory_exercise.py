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
import warnings
import subprocess
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


def edited(name, before, after):
    owner = X if name in ('main', 'credential_matches', 'private_fallback') else X.Exercise
    source = textwrap.dedent(inspect.getsource(getattr(owner, name)))
    assert before in source, 'mutation_target_missing_' + name
    namespace = dict(vars(X))
    exec(compile(source.replace(before, after, 1), '<memory-exercise-mutant>', 'exec'), namespace)
    function = namespace[name]
    function = types.FunctionType(function.__code__, vars(X), name, function.__defaults__)
    return patch.object(owner, name, function)


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
    program_runs = 0

    @classmethod
    def tearDownClass(cls):
        print('program_output_runs=' + str(cls.program_runs))

    def program(self, action, secret=SECRET, notes=()):
        """Read both Python streams and OS descriptors for every program run."""
        ExerciseTests.program_runs += 1
        parent = X.sys.stdout
        out, err = io.StringIO(), io.StringIO()
        exercise = getattr(action, '__self__', None) or getattr(X.Exercise, 'return_value', self.exercise)
        with tempfile.TemporaryFile() as native_out, tempfile.TemporaryFile() as native_err:
            saved = [os.dup(1), os.dup(2)]
            os.dup2(native_out.fileno(), 1)
            os.dup2(native_err.fileno(), 2)
            try:
                with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err), X.output_policy():
                    result = action()
                    if isinstance(result, subprocess.CompletedProcess):
                        out.write(result.stdout.decode('utf-8', errors='replace'))
                        err.write(result.stderr.decode('utf-8', errors='replace'))
                        return result.returncode
                    return result
            finally:
                os.dup2(saved[0], 1)
                os.dup2(saved[1], 2)
                for fd in saved:
                    os.close(fd)
                native_out.seek(0); native_err.seek(0)
                text = out.getvalue() + native_out.read().decode('utf-8', errors='replace')
                self.assertFalse(bool(err.getvalue() or native_err.read()), 'library_stderr_escaped')
                self.assertTrue(not text or text.endswith('\n'), 'partial_program_output')
                self.assertLessEqual(len(text.splitlines()), 1, 'extra_program_output')
                for line in text.splitlines():
                    if line in (X.REFUSAL, X.SHORT_REFUSAL):
                        continue
                    try:
                        value = json.loads(line)
                    except ValueError:
                        self.fail('undefined_program_output')
                    self.assertTrue(isinstance(value, dict) and set(value) == {'exit', 'cleanup', 'credential_prefix_matches'}, 'undefined_program_output')
                    self.assertTrue(type(value['exit']) is int and value['exit'] in (0, 2, 3, 4), 'undefined_program_exit')
                    self.assertTrue(value['cleanup'] in ('not_run', 'proven', 'incomplete'), 'undefined_program_cleanup')
                    self.assertTrue(type(value['credential_prefix_matches']) is int and value['credential_prefix_matches'] >= 0, 'undefined_program_count')
                    self.assertTrue(line == X.encode(value).decode(), 'undefined_program_output')
                for n in range(min(8, len(secret)), len(secret) + 1):
                    self.assertFalse(secret[:n] in text, 'output_credential_disclosed')
                retained = list(notes)
                for _, _, body, _ in getattr(getattr(exercise, 'wire', None), 'calls', []):
                    if isinstance(body, dict):
                        retained.extend(item.get('content') for item in body.get('items', []))
                for note in retained:
                    if isinstance(note, str) and note:
                        self.assertFalse(note in text, 'output_note_disclosed')
                if isinstance(parent, io.StringIO):
                    parent.write(text)

    def setUp(self):
        self.socket = patch.object(X.P.socket, 'create_connection', side_effect=AssertionError('offline_only'))
        self.blocked = self.socket.start()
        self.addCleanup(self.socket.stop)
        self.addCleanup(self.blocked.assert_not_called)
        self.clock = Clock()
        self.server = FakeServer(self.clock)
        self.exercise = X.Exercise(BANK, self.server, self.clock, self.clock.sleep)

    def check_removals(self, prefix, cases):
        for name, test, mutation in cases:
            with self.subTest(guard=name), mutation:
                result = unittest.TestResult()
                ExerciseTests(test).run(result)
            failures = result.failures + result.errors
            self.assertTrue(failures, prefix + '_removal_survived_' + name)
            trace = failures[0][1]
            locations = X.re.findall(r'test_memory_exercise.py", line ([0-9]+)', trace)
            cause = next((line for line in trace.splitlines() if line.startswith('AssertionError:')), trace.splitlines()[-1])
            message = '' if prefix == 'control' else ' message=' + cause
            print(prefix + '_removed=' + name + ' failing_line=ci/test_memory_exercise.py:' + locations[-1] + message)

    def run_driver(self, action=None):
        if action:
            self.exercise.e1 = action
        return self.program(self.exercise.run)

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
                self.assertEqual(self.program(exercise.run), 3, 'exception_not_cleaned')
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
            self.assertEqual(self.program(exercise.run), 4, 'cleanup_failure_hidden')
            self.assertEqual(exercise.evidence['cleanup'], 'incomplete')

    def test_c5_foreign_count_shrink_is_exit4(self):
        def changed():
            self.exercise.create()
            self.server.foreign_count -= 1
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
                    code = self.program(lambda: X.main(['--bank', BANK, '--credential-fd', str(read_fd), '--evidence', str(path)]))
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
            self.assertEqual(self.program(exercise.run), 3, 'tool_filter_failure_accepted')
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
                self.assertEqual(self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)])), 2)
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
                self.assertEqual(self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)])), 3)
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
            self.assertEqual(self.program(exercise.run), 3, 'executed_or_unmatched_error_accepted')
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
                    code = self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]))
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
            self.assertEqual(self.program(exercise.run), 3, 'restore_difference_accepted_' + kind)
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
            self.assertEqual(self.program(exercise.run), 2)
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
            self.assertEqual(self.program(exercise.run), 3, 'compressed_foreign_archive_accepted')
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
            self.assertEqual(self.program(exercise.run), 3, 'bad_entry_accepted')
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
            self.assertEqual(self.program(exercise.run), 3, 'answered_write_left_bank')
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
                    code = self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]))
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
            self.assertEqual(self.program(exercise.run), 3 if kind == 'old-list' else 0, 'session_qualification_lost')
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
            self.assertEqual(self.program(exercise.run), 3 if kind == 'subject' else 0, 'recall_subject_scope_lost')
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
            self.assertEqual(self.program(exercise.run), 0)
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

    def main_result(self, exercise, server, secret=SECRET):
        with tempfile.TemporaryDirectory() as tmp, patch.object(X.P, 'credential', return_value=secret), \
             patch.object(X, 'Wire', return_value=server), patch.object(X, 'Exercise', return_value=exercise), \
             patch.object(X.P, 'locked', contextlib.nullcontext), contextlib.redirect_stdout(io.StringIO()):
            path = Path(tmp, 'evidence')
            code = self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]), secret=secret)
            raw = path.read_bytes()
            self.assertTrue(raw, 'evidence_file_empty')
            return code, X.load(raw)

    def test_m1_decoded_archive_keys_values_names_and_unmeasured_entries(self):
        cases = ('partial-escape', 'all-escape-key', 'jsonl-unknown', 'name', 'slash', 'upper',
                 'nested-zip', 'gzip', 'bytes', 'binary', 'empty')
        for kind in cases:
            with self.subTest(kind=kind):
                clock = Clock(); server = FakeServer(clock)
                exercise = X.Exercise(BANK, server, clock, clock.sleep)
                original = server.answer
                foreign = 'foreign/x' if kind == 'slash' else FOREIGN
                escaped = ''.join('\\u%04x' % ord(c) for c in foreign)
                def hostile(method, path, body=None, **kwargs):
                    status, answer, sid = original(method, path, body, **kwargs)
                    if path.startswith(X.BANKS + '?') and kind == 'slash':
                        answer['banks'][0]['bank_id'] = foreign
                    if '/transfer/export?' in path:
                        key = server.ops[answer['operation_id']]['result_metadata']['download_url']
                        output = io.BytesIO()
                        with zipfile.ZipFile(io.BytesIO(server.downloads[key])) as old, \
                             zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as new:
                            data = old.read('data.json')
                            if kind == 'partial-escape':
                                data = data[:-1] + b',"owner":"\\u0066oreign-bank"}'
                            elif kind == 'upper':
                                data = data[:-1] + b',' + X.encode({'owner': FOREIGN.upper()})[1:]
                            new.writestr('data.json', data)
                            name = foreign + '.json' if kind == 'name' else 'opaque.bin'
                            payload = b'{}'
                            if kind == 'all-escape-key':
                                payload = ('{"nested":[{"' + escaped + '":1}]}').encode()
                            elif kind == 'jsonl-unknown':
                                payload = ('{"safe":true}\n{"nested":["' + escaped + '"]}\n').encode()
                            elif kind == 'slash':
                                payload = b'{"owner":"foreign\\/x"}'
                            elif kind == 'nested-zip':
                                nested = io.BytesIO()
                                with zipfile.ZipFile(nested, 'w', compression=zipfile.ZIP_DEFLATED) as z:
                                    z.writestr('nested.json', X.encode({'owner': foreign}))
                                payload = nested.getvalue()
                            elif kind == 'gzip':
                                payload = b'\x1f\x8bnot-json'
                            elif kind in ('bytes', 'binary', 'empty'):
                                payload = b'unmeasured' if kind == 'bytes' else b'\xff\x00' if kind == 'binary' else b''
                            new.writestr(name, payload)
                        server.downloads[key] = output.getvalue()
                    return status, answer, sid
                server.answer = hostile
                self.assertEqual(self.program(exercise.run), 3, 'decoded_archive_accepted')
                expected = 'archive_unmeasured' if kind in ('nested-zip', 'gzip', 'bytes', 'binary', 'empty') else 'archive_foreign_bank'
                self.assertEqual(exercise.evidence['steps']['E1']['failure'], expected, 'archive_refusal_unmeasured')
                self.assertFalse(any('/transfer/import?' in p for _, p, _, _ in server.calls), 'archive_dispatched')
                self.assertEqual(exercise.evidence['cleanup'], 'proven')
                self.assertTrue(exercise.evidence.get('archives'), 'archive_diagnostic_missing')
                record = exercise.evidence['archives'][-1]
                index = 0 if kind in ('partial-escape', 'upper') else 1
                self.assertEqual(record.get('failed_entry_index'), index, 'archive_failure_index_missing')
                self.assertEqual(len(record['entries']), index + 1)
                expected_class = {'nested-zip': 'zip', 'gzip': 'gzip', 'bytes': 'text', 'binary': 'binary',
                                  'empty': 'empty', 'jsonl-unknown': 'jsonl'}.get(kind, 'json')
                self.assertEqual(record['entries'][-1]['class'], expected_class, 'archive_class_wrong')
                for entry in record['entries']:
                    self.assertEqual(set(entry), {'name_sha256', 'bytes', 'class'})
                    self.assertRegex(entry['name_sha256'], '^[0-9a-f]{64}$')

    def test_m2_schema_names_are_hashed_before_evidence(self):
        for kind in ('note', 'credential'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            secret = 'q"invented-credential-only'
            payload = exercise.marker + ' subject A uses ExampleID-A when its gate is closed.' if kind == 'note' else secret
            server.catalog['retain']['inputSchema'] = {'type': 'object', 'properties': {payload: {'type': 'string'}}, 'required': [payload]}
            code, evidence = self.main_result(exercise, server, secret)
            self.assertEqual(code, 0, 'schema_key_was_not_sanitized')
            self.assertFalse(any(payload in s for s in X.strings(evidence)), 'schema_payload_disclosed')
            shapes = [r['request_keys'] for r in evidence['requests'] if r.get('tool') == 'retain']
            self.assertTrue(shapes)
            self.assertIn('sha256:' + X.P.digest(payload.encode()), shapes[0]['params']['arguments'])
            self.assertEqual(evidence['credential_prefix_matches'], 0)

    def test_m2_decoded_credential_check(self):
        for secret in ('q"invented-secret-only',):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            e1 = exercise.e1
            def inject():
                e1()
                exercise.evidence['invented_injection'] = {'nested': [{secret: secret[:min(8, len(secret))]}]}
            exercise.e1 = inject
            code, evidence = self.main_result(exercise, server, secret)
            self.assertEqual(code, 3, 'decoded_credential_disclosure_missed')
            self.assertEqual(evidence.get('failure'), 'evidence_redaction_failed')
            self.assertGreater(evidence['credential_prefix_matches'], 0)
            self.assertFalse(any(secret in s for s in X.strings(evidence)), 'credential_disclosed')

    def test_m2_credential_colliding_with_diagnostics(self):
        for secret in ('credential', 'prefix_matches'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            code, evidence = self.main_result(exercise, server, secret)
            self.assertEqual(code, 3, 'diagnostic_collision_not_refused')
            self.assertEqual(X.credential_matches(X.encode(evidence), secret), 0, 'fallback_disclosed_credential')

    def test_m3_successful_initialize_and_exclusive_list_result(self):
        for kind in ('initialize-error', 'initialize-both', 'initialize-null', 'list-both'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.mcp
            def malformed(body, sid):
                status, answer, returned = original(body, sid)
                if body['method'] == ('tools/list' if kind == 'list-both' else 'initialize'):
                    if kind == 'initialize-null':
                        answer['result'] = None
                    else:
                        answer['error'] = {'code': -32602, 'message': 'invented rejection'}
                        if kind == 'initialize-error':
                            answer.pop('result')
                return status, answer, returned
            server.mcp = malformed
            self.assertEqual(self.program(exercise.run), 3, 'rejected_mcp_success_accepted')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')
            self.assertTrue(any(r['verdict'] == 'failed' and r.get('rpc_method') in ('initialize', 'tools/list')
                                for r in exercise.evidence['requests']))
            if kind != 'list-both':
                self.assertFalse(exercise.sessions)
                self.assertFalse(any(isinstance(b, dict) and b.get('method') == 'notifications/initialized'
                                     for _, _, b, _ in server.calls), 'rejected_session_used')

    def test_m4_foreign_session_id_refused_before_use(self):
        for sid in (FOREIGN, 'prefix-' + FOREIGN + '-suffix', FOREIGN.upper()):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.mcp
            used = []
            def named(body, session):
                if session is not None:
                    used.append(session)
                status, answer, returned = original(body, session)
                return status, answer, sid if body['method'] == 'initialize' else returned
            server.mcp = named
            self.assertEqual(self.program(exercise.run), 3, 'foreign_session_header_used')
            self.assertEqual(exercise.evidence['steps']['E1']['failure'], 'mcp_session_scope')
            self.assertFalse(used, 'foreign_session_dispatched')
            self.assertEqual(exercise.evidence['cleanup'], 'proven')

    def test_m5_answered_post_memories_tool_and_import(self):
        for target in ('memories', 'tool', 'import'):
            for stop in ('interrupt', 'deadline'):
                clock = Clock(); server = FakeServer(clock)
                exercise = X.Exercise(BANK, server, clock, clock.sleep)
                original = server.exchange
                fired = []
                def delayed(method, path, body=None, **kwargs):
                    result = original(method, path, body, **kwargs)
                    match = method == 'POST' and (path == exercise.base + '/memories' if target == 'memories'
                        else isinstance(body, dict) and body.get('method') == 'tools/call' if target == 'tool'
                        else '/transfer/import?' in path)
                    if match and not fired:
                        fired.append(True)
                        if stop == 'interrupt':
                            exercise.interrupt()
                        else:
                            clock.now = 780.25
                    return result
                server.exchange = delayed
                self.assertEqual(self.program(exercise.run), 3, 'answered_post_left_bank')
                self.assertTrue(fired, 'write_case_not_exercised')
                self.assertEqual(exercise.evidence['cleanup'], 'proven')
                self.assertFalse(exercise.uncertain, 'answered_post_uncertain')
                self.assertFalse(server.present)
                rows = [r for r in exercise.evidence['requests'] if r['method'] == 'POST']
                self.assertTrue(all(r['answered'] for r in rows), 'received_post_answer_lost')

    def test_m5_signal_before_dispatch_does_not_invent_work(self):
        original = self.exercise.call
        def between_checks(method, path, body=None, **kwargs):
            if method == 'POST' and path == self.exercise.base + '/memories':
                self.exercise.interrupt()
            return original(method, path, body, **kwargs)
        self.exercise.call = between_checks
        def attempt():
            self.exercise.create()
            self.exercise.retain('invented-a', self.exercise.marker)
        self.assertEqual(self.run_driver(attempt), 3, 'unsent_write_marked_uncertain')
        self.assertEqual(self.exercise.evidence['cleanup'], 'proven')
        self.assertFalse(self.exercise.uncertain)
        self.assertFalse(self.exercise.active, 'unsent_operation_left_active')
        self.assertFalse(any(m == 'POST' and p == self.exercise.base + '/memories' for m, p, _, _ in self.server.calls))

    def test_m6_signal_during_evidence_write_keeps_result(self):
        previous = signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
        fdopen = os.fdopen
        @contextlib.contextmanager
        def during_write(fd, mode):
            with fdopen(fd, mode) as target:
                class Sink:
                    def write(self, raw):
                        os.kill(os.getpid(), signal.SIGTERM)
                        return target.write(raw)
                yield Sink()
        try:
            with patch.object(X.os, 'fdopen', during_write):
                code, evidence = self.main_result(self.exercise, self.server)
            self.assertEqual(code, 0, 'evidence_signal_changed_exit')
            self.assertEqual(evidence['cleanup'], 'proven')
            self.assertEqual(evidence['credential_prefix_matches'], 0)
            self.assertFalse(self.server.present)
        finally:
            signal.signal(signal.SIGTERM, previous)

    def test_m7_changing_401_and_303_are_failed(self):
        for status in (401, 303):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.answer
            def rejected(method, path, body=None, **kwargs):
                result = original(method, path, body, **kwargs)
                return (status, {}, None) if method == 'PUT' else result
            server.answer = rejected
            self.assertEqual(self.program(exercise.run), 3 if status == 401 else 4)
            put = next(r for r in exercise.evidence['requests'] if r['method'] == 'PUT')
            self.assertTrue(put['answered'])
            self.assertEqual(put['verdict'], 'failed', 'rejected_write_measured')
            self.assertEqual(exercise.uncertain, status == 303, 'redirect_not_uncertain')

    def test_m8_growth_is_unattributed_shrink_and_missing_fail(self):
        for kind in ('grew', 'new', 'shrank', 'missing'):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.answer
            def drift(method, path, body=None, **kwargs):
                status, answer, sid = original(method, path, body, **kwargs)
                if exercise.cleaning and path.startswith(X.BANKS + '?'):
                    if kind == 'new':
                        answer['banks'].append({'bank_id': 'new-bank', 'fact_count': 1})
                        answer['total'] += 1
                    elif kind == 'missing':
                        answer['banks'] = [b for b in answer['banks'] if b['bank_id'] != FOREIGN]
                        answer['total'] -= 1
                    else:
                        answer['banks'][0]['fact_count'] += 1 if kind == 'grew' else -1
                return status, answer, sid
            server.answer = drift
            self.assertEqual(self.program(exercise.run), 0 if kind in ('grew', 'new') else 4, 'drift_policy_wrong')
            comparison = exercise.evidence['other_bank_comparison']
            self.assertIn(kind, comparison.values())
            self.assertTrue(all(X.re.fullmatch('[0-9a-f]{64}', k) for k in comparison), 'drift_id_disclosed')
            self.assertFalse(server.present)
            if kind in ('grew', 'new'):
                self.assertEqual(exercise.evidence['cleanup'], 'proven')
                self.assertEqual(exercise.evidence['other_bank_drift'], 'unattributed', 'growth_attributed')
                self.assertFalse(exercise.evidence['cleanup_proofs']['other_counts_equal'])

    def test_q1_rebuilds_every_import_and_discards_all_container_metadata(self):
        original = self.server.answer
        expected = []
        payload = b'untrusted ' + FOREIGN.encode()
        def metadata(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            if '/transfer/export?' in path:
                key = self.server.ops[answer['operation_id']]['result_metadata']['download_url']
                output = io.BytesIO()
                with zipfile.ZipFile(io.BytesIO(self.server.downloads[key])) as old, zipfile.ZipFile(output, 'w') as new:
                    info = zipfile.ZipInfo('data.json', (2026, 11, 12, 12, 34, 56))
                    info.comment = payload
                    info.extra = b'\xfe\xca' + len(payload).to_bytes(2, 'little') + payload
                    new.writestr(info, old.read('data.json'))
                    info = zipfile.ZipInfo('second.opaque', (2025, 1, 2, 3, 4, 6))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    new.writestr(info, b'{"safe":1}\n{"safe":2}\n')
                    new.comment = payload
                raw = output.getvalue()
                with zipfile.ZipFile(io.BytesIO(raw)) as z:
                    expected.append([(e.filename, z.read(e)) for e in z.infolist()])
                self.server.downloads[key] = raw + b'trailing ' + payload
            return status, answer, sid
        self.server.answer = metadata
        self.assertEqual(self.program(self.exercise.run), 0, 'clean_entries_not_rebuilt')
        imports = [b[b.index(b'PK'):].rsplit(b'\r\n--', 1)[0] for _, p, b, _ in self.server.calls if '/transfer/import?' in p]
        self.assertEqual(len(imports), 2)
        for raw, wanted in zip(imports, (expected[0], expected[2])):
            self.assertNotIn(payload, raw, 'container_bytes_forwarded')
            with zipfile.ZipFile(io.BytesIO(raw)) as z:
                self.assertEqual(z.comment, b'')
                self.assertEqual([(e.filename, z.read(e)) for e in z.infolist()], wanted, 'entry_bytes_changed')
                for entry in z.infolist():
                    self.assertEqual((entry.comment, entry.extra), (b'', b''), 'entry_metadata_forwarded')
                    self.assertEqual(entry.compress_type, zipfile.ZIP_STORED)
                    self.assertEqual(entry.date_time, (1980, 1, 1, 0, 0, 0))
        for record in self.exercise.evidence['archives']:
            self.assertIs(record['archive_rebuilt'], True)
            self.assertNotIn('failed_entry_index', record)
            self.assertEqual(record['container_removed'], {'archive_comment_bytes': len(payload),
                'entry_comments': 1, 'entry_extra_fields': 1, 'compression_methods': [0, 8]})
        self.assertNotIn(payload.decode(), json.dumps(self.exercise.evidence))

    def test_q1_raw_guard_and_q4_casefold_before_dispatch(self):
        self.exercise.preflight()
        for body in (FOREIGN.encode(), FOREIGN.upper().encode(), b'STRASSE'):
            if body == b'STRASSE':
                self.exercise.before = {'Stra\u00dfe': 1}
            with self.subTest(body_hash=X.P.digest(body)), \
                 patch.object(self.server, 'exchange', return_value=(200, b'{}', None)) as dispatch:
                with self.assertRaisesRegex(X.P.Stop, '^foreign_bank_in_body$'):
                    self.exercise.change('PATCH', '/config', body)
                dispatch.assert_not_called()

    def test_q2_oversized_entry_keeps_diagnostic(self):
        self.exercise.preflight(); self.exercise.create()
        self.exercise.retain('invented-a', self.exercise.marker)
        original = self.server.answer
        def oversized(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            if '/transfer/export?' in path:
                key = self.server.ops[answer['operation_id']]['result_metadata']['download_url']
                output = io.BytesIO()
                with zipfile.ZipFile(io.BytesIO(self.server.downloads[key])) as old, zipfile.ZipFile(output, 'w') as new:
                    new.writestr('data.json', old.read('data.json'))
                    new.writestr('oversized', b'x' * (X.MAX_BYTES + 1))
                self.server.downloads[key] = output.getvalue()
            return status, answer, sid
        self.server.answer = oversized
        with self.assertRaisesRegex(X.P.Stop, '^archive_too_large$'):
            self.exercise.export({'invented-a': self.exercise.marker})
        record = self.exercise.evidence['archives'][-1]
        self.assertEqual(record['failed_entry_index'], 1)
        self.assertEqual(record['entries'][1]['class'], 'text')
        self.assertEqual(record['entries'][1]['bytes'], X.MAX_BYTES + 1)
        self.assertNotIn('oversized', list(X.strings(record)))

    def test_q3_short_credential_refuses_before_any_request(self):
        for secret in ('a', 'z"7', '1234567'):
            with tempfile.TemporaryDirectory() as tmp, patch.object(X.P, 'credential', return_value=secret), \
                 patch.object(X, 'Wire', return_value=self.server) as wire, patch.object(X, 'Exercise', return_value=self.exercise), \
                 patch.object(X.P, 'locked', contextlib.nullcontext), contextlib.redirect_stdout(io.StringIO()) as output:
                path = Path(tmp, 'evidence')
                code = self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]), secret=secret)
                self.assertEqual(code, 2, 'short_credential_admitted')
                self.assertEqual(output.getvalue(), '' if secret in X.SHORT_REFUSAL else X.SHORT_REFUSAL + '\n')
                wire.assert_not_called()
                self.assertFalse(path.exists(), 'short_credential_created_evidence')
                self.assertFalse(self.server.calls)

    def test_q5_unchanged_banks_have_no_unattributed_drift(self):
        self.assertEqual(self.program(self.exercise.run), 0)
        self.assertFalse('other_bank_drift' in self.exercise.evidence, 'drift_without_growth')

    def test_q5_summary_and_message_guards_have_admitted_credentials(self):
        for mode, secret in (('summary', 'credential'), ('message', 'exercise')):
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            with tempfile.TemporaryDirectory() as tmp, patch.object(X.P, 'credential', return_value=secret), \
                 patch.object(X, 'Wire', side_effect=RuntimeError if mode == 'message' else None, return_value=server), \
                 patch.object(X, 'Exercise', return_value=exercise), patch.object(X.P, 'locked', contextlib.nullcontext), \
                 contextlib.redirect_stdout(io.StringIO()) as output:
                path = Path(tmp, 'evidence')
                code = self.program(lambda: X.main(['--bank', BANK, '--credential-fd', '3', '--evidence', str(path)]), secret=secret)
                self.assertEqual(code, 3 if mode == 'summary' else 2)
                self.assertFalse(secret in output.getvalue(), mode + '_credential_disclosed')
                self.assertEqual(X.credential_matches(path.read_bytes(), secret), 0)

    def test_s1_duplicate_note_names_refuse_before_any_rebuild_write(self):
        note = self.exercise.marker + ' subject A uses ExampleID-A when its gate is closed.'
        original = self.server.answer
        def duplicates(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            if '/transfer/export?' in path:
                key = self.server.ops[answer['operation_id']]['result_metadata']['download_url']
                output = io.BytesIO()
                with warnings.catch_warnings(), zipfile.ZipFile(io.BytesIO(self.server.downloads[key])) as old:
                    warnings.simplefilter('ignore', UserWarning)  # Only construction of the hostile test fixture.
                    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as z:
                        z.writestr('data.json', old.read('data.json'))
                        z.writestr(note + '.json', b'{}')
                        z.writestr(note + '.json', b'{}')
                self.server.downloads[key] = output.getvalue()
            return status, answer, sid
        self.server.answer = duplicates
        def action():
            self.exercise.create()
            self.exercise.retain('invented-a', note)
            self.exercise.export({'invented-a': note})
        self.exercise.e1 = action
        writestr = zipfile.ZipFile.writestr
        writes = []
        def watched(z, info, data, *args, **kwargs):
            if isinstance(info, zipfile.ZipInfo) and info.date_time == (1980, 1, 1, 0, 0, 0):
                writes.append(True)
            return writestr(z, info, data, *args, **kwargs)
        with patch.object(zipfile.ZipFile, 'writestr', watched):
            self.assertEqual(self.program(self.exercise.run), 3, 'duplicate_archive_accepted')
        self.assertEqual(self.exercise.evidence['cleanup'], 'proven')
        self.assertEqual(self.exercise.evidence['steps']['E1']['failure'], 'archive_unmeasured')
        self.assertFalse(writes, 'duplicate_archive_written')
        record = self.exercise.evidence['archives'][-1]
        self.assertEqual(record['failed_entry_index'], 2)
        self.assertEqual([e['class'] for e in record['entries']], ['json'] * 3)
        self.assertEqual(record['entries'][1]['name_sha256'], record['entries'][2]['name_sha256'])

    def test_s2_warning_inside_export_fails_step_without_output(self):
        note = self.exercise.marker + ' subject A uses ExampleID-A when its gate is closed.'
        original = self.exercise.export
        def noisy(*args, **kwargs):
            warnings.warn(note, UserWarning)
            return original(*args, **kwargs)
        self.exercise.export = noisy
        self.assertEqual(self.program(self.exercise.run), 3, 'library_warning_not_a_failure')
        self.assertEqual(self.exercise.evidence['steps']['E1']['failure'], 'library_warning')
        self.assertEqual(self.exercise.evidence['cleanup'], 'proven')

    def test_s2_process_import_exception_native_output_and_shutdown(self):
        note = 'InventedPrivateNoteForProcessOutput'
        source = (ROOT / 'scripts/exercise-memory.py').read_text()
        cases = [
            ("raise ValueError(" + repr(note) + ")", 2),
            ("warnings.warn(" + repr(note) + ", UserWarning)", 2),
            ("os.write(1, " + repr(note.encode()) + "); os.write(2, " + repr(note.encode()) + "); print(" + repr(note) + ")", 0),
            ("import atexit; atexit.register(lambda: warnings.warn(" + repr(note) + ", ResourceWarning))", 0),
            ("import atexit; atexit.register(lambda: (_ for _ in ()).throw(ValueError(" + repr(note) + ")))", 0),
            ("unclosed = open(os.devnull)", 0),
            ("sys.argv = ['exercise', " + repr(note) + "]", 2),
        ]
        for injection, expected in cases:
            modified = source.replace('ROOT = Path(__file__).resolve().parents[1]', injection + '\nROOT = Path(__file__).resolve().parents[1]', 1)
            launcher = ("import socket, sys\n"
                        "socket.create_connection = lambda *a, **k: (_ for _ in ()).throw(AssertionError('offline_only'))\n"
                        "sys.argv = ['exercise', '--help']\n"
                        "exec(compile(" + repr(modified) + ", 'exercise-process', 'exec'), {'__name__':'__main__', '__file__':" + repr(str(ROOT / 'scripts/exercise-memory.py')) + "})")
            code = self.program(lambda: subprocess.run([X.sys.executable, '-I', '-B', '-c', launcher], capture_output=True, timeout=15), notes=(note,))
            self.assertEqual(code, expected, 'process_output_policy_failed')

    def test_s4_container_refusals_have_no_entry_index_and_keep_body_class(self):
        for kind, raw in [('json', b'{"not":"zip"}'), ('jsonl', b'{}\n{}'), ('gzip', b'\x1f\x8bnot-json'),
                          ('text', b'not-an-archive'), ('binary', b'\xff\x00'), ('empty', b''), ('zip', b'PKbroken')]:
            clock = Clock(); server = FakeServer(clock)
            exercise = X.Exercise(BANK, server, clock, clock.sleep)
            original = server.answer
            def nonzip(method, path, body=None, **kwargs):
                status, answer, sid = original(method, path, body, **kwargs)
                if '/transfer/export?' in path:
                    key = server.ops[answer['operation_id']]['result_metadata']['download_url']
                    server.downloads[key] = raw
                return status, answer, sid
            server.answer = nonzip
            self.assertEqual(self.program(exercise.run), 3)
            self.assertEqual(exercise.evidence['steps']['E1']['failure'], 'archive_unmeasured', 'nonzip_not_unmeasured')
            record = exercise.evidence['archives'][-1]
            self.assertEqual(record['entries'], [])
            self.assertEqual(record.get('body_class'), kind, 'download_body_class_missing')
            self.assertNotIn('failed_entry_index', record)
        original = self.server.answer
        def incomplete(method, path, body=None, **kwargs):
            status, answer, sid = original(method, path, body, **kwargs)
            if '/transfer/export?' in path:
                key = self.server.ops[answer['operation_id']]['result_metadata']['download_url']
                output = io.BytesIO()
                with zipfile.ZipFile(output, 'w') as z:
                    z.writestr('empty-data.json', b'{}')
                self.server.downloads[key] = output.getvalue()
            return status, answer, sid
        self.server.answer = incomplete
        self.assertEqual(self.program(self.exercise.run), 3)
        record = self.exercise.evidence['archives'][-1]
        self.assertEqual(self.exercise.evidence['steps']['E1']['failure'], 'archive_documents_unmeasured')
        self.assertFalse('failed_entry_index' in record, 'container_refusal_has_entry_index')

    def test_r5_guard_removals_fail_in_process(self):
        self.check_removals('r5', [
            ('S1', 'test_s1_duplicate_note_names_refuse_before_any_rebuild_write',
             edited('export', "P.require(entry.filename not in seen, 'archive_unmeasured')", 'pass')),
            ('S2-warning', 'test_s2_warning_inside_export_fails_step_without_output',
             patch.object(warnings, 'simplefilter', lambda *_a, **_k: None)),
            ('S3-output', 'test_s2_warning_inside_export_fails_step_without_output',
             patch.object(X, 'output_policy', contextlib.nullcontext)),
            ('S4-class', 'test_s4_container_refusals_have_no_entry_index_and_keep_body_class',
             edited('export', "record['body_class'] = archive_content(raw)[0]", 'pass')),
            ('S4-index', 'test_s4_container_refusals_have_no_entry_index_and_keep_body_class',
             edited('export', "record.pop('failed_entry_index', None)", 'pass')),
        ])

    def test_r4_guard_removals_fail_in_process(self):
        self.check_removals('r4', [
            ('Q1-rebuild', 'test_q1_rebuilds_every_import_and_discards_all_container_metadata',
             edited('export', 'raw = output.getvalue()', 'pass')),
            ('Q1-raw', 'test_q1_raw_guard_and_q4_casefold_before_dispatch',
             edited('change', "body.decode('utf-8', errors='replace') if isinstance(body, bytes) else body", 'None if isinstance(body, bytes) else body')),
            ('Q2', 'test_m1_decoded_archive_keys_values_names_and_unmeasured_entries',
             edited('export', "self.evidence.setdefault('archives', []).append(record)", 'pass')),
            ('Q3', 'test_q3_short_credential_refuses_before_any_request',
             edited('main', 'if len(secret) < 8:', 'if False:')),
            ('Q4-raw', 'test_q1_raw_guard_and_q4_casefold_before_dispatch',
             edited('no_foreign', 'bank.casefold() in text.casefold()', 'bank in text')),
            ('Q4-archive', 'test_m1_decoded_archive_keys_values_names_and_unmeasured_entries',
             edited('no_foreign', 'bank.casefold() in text.casefold()', 'bank in text')),
            ('Q4-session', 'test_m4_foreign_session_id_refused_before_use',
             edited('no_foreign', 'bank.casefold() in text.casefold()', 'bank in text')),
            ('Q5-drift', 'test_q5_unchanged_banks_have_no_unattributed_drift',
             edited('cleanup', "if any(v in ('grew', 'new') for v in comparison.values()):", 'if True:')),
            ('Q5-summary', 'test_q5_summary_and_message_guards_have_admitted_credentials',
             edited('main', 'if not credential_matches(summary, secret):', 'if True:')),
            ('Q5-message', 'test_q5_summary_and_message_guards_have_admitted_credentials',
             edited('main', 'if not secret or not credential_matches(encode(message), secret):', 'if True:')),
        ])

    def test_r3_guard_removals_fail_in_process(self):
        original = X.P.require
        bypass = lambda code: patch.object(X.P, 'require', side_effect=lambda condition, actual:
            original(condition or actual == code, actual))
        self.check_removals('r3', [
            ('M1-decoded', 'test_m1_decoded_archive_keys_values_names_and_unmeasured_entries',
             edited('export', "self.no_foreign(value, 'archive_foreign_bank')", 'pass')),
            ('M1-parse', 'test_m1_decoded_archive_keys_values_names_and_unmeasured_entries',
             edited('export', "P.require(bool(values), 'archive_unmeasured')", 'pass')),
            ('M1-name', 'test_m1_decoded_archive_keys_values_names_and_unmeasured_entries',
             edited('export', "self.no_foreign(entry.filename, 'archive_foreign_bank')", 'pass')),
            ('M2-schema', 'test_m2_schema_names_are_hashed_before_evidence',
             edited('call', "'request_keys': keys(body)", "'request_keys': body")),
            ('M2-decoded', 'test_m2_decoded_credential_check',
             edited('credential_matches', 'decoded = list(strings(load(raw)))', 'decoded = []')),
            ('M2-fallback', 'test_m2_credential_colliding_with_diagnostics',
             edited('private_fallback', 'if secret and credential_matches(raw, secret):', 'if False:')),
            ('M2-final-field', 'test_m2_credential_colliding_with_diagnostics',
             edited('main', "exercise.evidence['credential_prefix_matches'] = 0", 'pass')),
            ('M3-init', 'test_m3_successful_initialize_and_exclusive_list_result',
             edited('session', "isinstance(answer.get('result'), dict) and 'error' not in answer", 'True')),
            ('M3-list', 'test_m3_successful_initialize_and_exclusive_list_result',
             edited('tools', "isinstance(answer.get('result'), dict) and 'error' not in answer", 'True')),
            ('M4', 'test_m4_foreign_session_id_refused_before_use', bypass('mcp_session_scope')),
            ('M5-post', 'test_m5_answered_post_memories_tool_and_import',
             edited('call', "row['answer_shape'] = keys(data)", "if method != 'PUT': self.bound()\n        row['answer_shape'] = keys(data)")),
            ('M5-unsent', 'test_m5_signal_before_dispatch_does_not_invent_work',
             edited('change', "if len(self.evidence['requests']) > requests_before and self.evidence['requests'][-1]['dispatched']:", 'if True:')),
            ('M6', 'test_m6_signal_during_evidence_write_keeps_result',
             edited('main', 'raw = encode(exercise.evidence)', 'stack.close()\n            raw = encode(exercise.evidence)')),
            ('M7-status', 'test_m7_changing_401_and_303_are_failed',
             edited('call', "status >= 500 or (method != 'GET' and not 200 <= status < 300)", 'status >= 500')),
            ('M7-303', 'test_m7_changing_401_and_303_are_failed',
             edited('change', '(301, 302, 303, 307, 308)', '(301, 302, 307, 308)')),
            ('M8', 'test_m8_growth_is_unattributed_shrink_and_missing_fail',
             edited('cleanup', "not any(v in ('shrank', 'missing') for v in comparison.values())", "proofs['other_counts_equal']")),
            ('M9', 'test_r2_guard_removals_fail_in_process',
             patch.dict(globals(), {'edited': lambda *_: contextlib.nullcontext()})),
        ])

    def test_r2_guard_removals_fail_in_process(self):
        original = X.P.require
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
             edited('change', "self.no_foreign(body.decode('utf-8', errors='replace') if isinstance(body, bytes) else body, 'foreign_bank_in_body')", 'pass')),
            ('L4-cleanup-poll', 'test_l4_json_escaped_body_and_cleanup_discovery',
             edited('cleanup', 'self.poll()  # Read-only discovery', 'pass  # Read-only discovery')),
            ('L4-subject', 'test_l4_recall_subject_and_fixed_catalog_order',
             edited('filtering', " and 'subject:task:invented' in r.get('tags', [])", '')),
            ('L4-order', 'test_l4_recall_subject_and_fixed_catalog_order',
             edited('representatives', 'n for n in names if n in self.catalog', 'n for n in self.catalog if n in names')),
        ]
        self.check_removals('r2', cases)

    def test_r1_guard_removals_fail_in_process(self):
        original = X.P.require
        bypass = lambda code: patch.object(X.P, 'require', side_effect=lambda condition, actual:
            original(condition or actual == code, actual))
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
             edited('call', "'failed' if status >= 500 or (method != 'GET' and not 200 <= status < 300) else 'measured'", "'measured'")),
            ('K7-hash', 'test_k7_failed_rows_and_hashed_count_directions',
             edited('cleanup', 'P.digest(k.encode())', 'k')),
        ]
        self.check_removals('repair', cases)

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
            ('c5', 'test_c5_foreign_count_shrink_is_exit4',
             patch.object(X.P, 'require', side_effect=bypass({'other_bank_counts_changed'}))),
            ('c6', 'test_c6_server_reflected_key_names_and_exception_redacted',
             patch.object(X, 'keys', lambda value: value if isinstance(value, dict) else {})),
            ('c7-list', 'test_c7_list_or_call_bypass_fails_e2',
             patch.object(X.P, 'require', side_effect=bypass({'tool_filter_list_failed'}))),
            ('c7-call', 'test_c7_list_or_call_bypass_fails_e2',
             patch.object(X.P, 'require', side_effect=bypass({'tool_filter_call_failed'}))),
        ]
        self.check_removals('control', cases)

if __name__ == '__main__':
    unittest.main()
