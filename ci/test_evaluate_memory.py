"""Invented offline questions and in-process stubs; no service or credential files."""
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import socket
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    value = importlib.util.module_from_spec(spec); spec.loader.exec_module(value)
    return value
E = module('evaluator', 'scripts/evaluate-memory.py')
T = module('publisher_fixtures', 'ci/test_publish_memory.py')
T.P = E.P


def invented():
    return dict(schema='d2-question-input/1', canary=E.CANARY, questions=[dict(
        id=rid, forms=dict.fromkeys(E.FORMS, 'Invented marker question'), status='verified',
        **{'class': 'engineering'}, sources=[dict(kind='command', command='invented not executable')],
        expectation=dict(literal=['marker'], subject='oracle-document-id', condition='invented')) for rid in E.IDS])


class Stub(T.Stub):
    def __init__(self):
        super().__init__()
        self.pin['expected']['oracle-document-id'] = copy.deepcopy(self.pin['expected']['invented'])
        self.other_doc = dict(self.doc, id='oracle-document-id', original_text='Invented second document.')
        self.pin['expected']['oracle-document-id'].update(bytes=self.other_doc['original_text'].encode(),
            hash=E.P.digest(self.other_doc['original_text'].encode()))
    def request(self, method, path, body=None):
        if method == 'POST':
            self.calls.append((method, path, body))
            return dict(results=[dict(self.fact, text='marker')])
        if method == 'GET' and path.startswith(E.P.B + '/documents?'):
            self.calls.append((method, path, body))
            query = T.urllib.parse.parse_qs(T.urllib.parse.urlsplit(path).query)
            return T.envelope([dict(id=rid) for rid in self.pin['expected']],
                              int(query['offset'][0]), int(query['limit'][0]))
        if method == 'GET' and path == E.P.B + '/documents/oracle-document-id':
            self.calls.append((method, path, body))
            return copy.deepcopy(self.other_doc)
        return super().request(method, path, body)


class EvaluationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.network = patch.object(socket, 'create_connection', side_effect=AssertionError('offline_only')).start()
        self.addCleanup(patch.stopall)
        self.stub, self.data = Stub(), invented()
        self.selector = E.P.CHECK.recall_tags(dict(type='memory-map', id='engineering'), descendants=True)
        records = {'invented': dict(header=dict(schema_version=2, kind='incident', horizon='mid-term', review_on='2000-01-01',
                                               subject=dict(type='memory-map', id='engineering')))}
        patch.object(E.P.CHECK, 'load', return_value=(1, records, {}, [])).start()
        patch.object(E.P, 'unchanged').start()
        self.raw = E.encode(self.data)
        self.evidence = E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [])
        self.evidence_raw = E.encode(self.evidence); self.hash = E.P.digest(self.evidence_raw)
        self.results = dict(target_reached=True, slots=[dict(id=s['id'], form=s['form'], route=s['route'],
            verdict='supported', current_count=1, expectation_reverified=True, evidence_sha256=self.hash)
            for s in self.evidence['slots']])
    def tearDown(self):
        self.network.assert_not_called()
    def check(self):
        return E.check_results(self.results, self.evidence, self.hash, self.data)
    def test_complete_and_missing_slot(self):
        self.assertEqual(len(self.check()['slots']), 270)
        self.results['slots'].pop()
        with self.assertRaisesRegex(E.P.Stop, '^verdict_slots_incomplete$'): self.check()
    def test_false_success(self):
        for verdict in ('stale', 'missing', 'partial'):
            self.results['slots'][0]['verdict'] = verdict
            with self.assertRaisesRegex(E.P.Stop, '^false_success$'): self.check()
        self.results['target_reached'] = False
        self.assertFalse(self.check()['target_reached'])
    def test_document_candidates_not_oracle(self):
        self.assertEqual(set(self.stub.pin['expected']), {'invented', 'oracle-document-id'})
        self.stub.calls.clear()
        E.gather(self.stub, self.data['questions'][:1], self.stub.pin, self.selector, ['invented'])
        reads = [p for method, p, _ in self.stub.calls if method == 'GET']
        self.assertEqual(reads, [E.P.B + '/documents/invented']*3)
        self.assertTrue(all(b['tags'] == ['within:memory-map:engineering'] and b['tags_match'] == 'all_strict'
                            and b['budget'] == 'low' and b['max_tokens'] == 512
                            for method, _, b in self.stub.calls if method == 'POST'))
    def test_changed_and_volatile_need_reverified_binding(self):
        p = self.directory / 'source'; p.write_bytes(b'invented')
        for source, expected in [(dict(kind='file', path=str(p), anchor='invented', sha256='0'*64), 'source_changed'),
                                 (dict(kind='file', path=str(p), anchor='invented', volatile=True), 'reverify')]:
            self.evidence['source_pins']['R01'] = E.source_pins(dict(sources=[source]))
            self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], expected)
            for reverified, digest in [(False, self.hash), (True, '0'*64)]:
                self.results['slots'][0].update(expectation_reverified=reverified, evidence_sha256=digest)
                code = 'expectation_not_reverified' if not reverified else 'evidence_hash_mismatch'
                with self.assertRaisesRegex(E.P.Stop, '^' + code + '$'): self.check()
    def test_held_out_private_channel_and_no_missing_channel_read(self):
        question, oracle = 'invented-held-question-body', 'invented-held-oracle-body'
        held = dict(questions=[dict(self.data['questions'][0], forms=dict.fromkeys(E.FORMS, question), oracle=oracle)])
        with tempfile.TemporaryFile() as incoming, tempfile.TemporaryFile() as private:
            incoming.write(E.encode(held)); incoming.seek(0); self.stub.calls.clear()
            with self.assertRaisesRegex(E.P.Stop, '^held_out_evidence_fd_required$'):
                E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [], incoming.fileno())
            self.assertEqual(self.stub.calls, []); self.assertEqual(incoming.tell(), 0)
            stdout, stderr = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [], incoming.fileno(), private.fileno())
            public = self.directory / 'public.json'; public.write_bytes(E.encode(result))
            for secret in (question, oracle):
                self.assertNotIn(secret, public.read_text()+stdout.getvalue()+stderr.getvalue())
            private.seek(0); private_raw = private.read(); self.assertIn(oracle.encode(), private_raw)
            sealed = E.load(private_raw)
            verdicts = dict(target_reached=True, slots=[dict(id=s['id'], form=s['form'], route=s['route'],
                verdict='supported', current_count=1, expectation_reverified=True,
                evidence_sha256=E.P.digest(private_raw)) for s in sealed['slots']])
            self.assertEqual(E.check_results(verdicts, sealed, E.P.digest(private_raw), sealed)['total']['supported'], 6)
            self.assertEqual(result['held_out']['count'], 1)
    def test_canary_missing_read_is_unknown(self):
        with patch.object(self.stub, 'request', side_effect=E.P.Stop('read_uncertain')):
            result = E.canary(self.stub, self.stub.pin, self.data, self.selector, None)
        self.assertEqual([q['literal'] for q in result['questions']], ['unknown']*6)
        baseline = dict(profile=E.profile(self.stub, self.stub.pin)); self.stub.calls.clear()
        self.assertEqual(E.canary(self.stub, self.stub.pin, self.data, self.selector, baseline)['route'], 'no_readable_drift')
        self.assertEqual(sum(m == 'POST' for m, _, _ in self.stub.calls), 6)
    def test_scope_exclusion_aliases_omission_and_replacement(self):
        for label in ('sibling', 'same-prefix', 'untagged', 'lesson-only', 'retracted'):
            with self.subTest(label=label), patch.object(self.stub, 'request', return_value=dict(
                    results=[dict(document_id=label, text='marker')])):
                with self.assertRaisesRegex(E.P.Stop, '^wrong_scope_or_private$'):
                    E.recall(self.stub, 'invented alias', self.selector, self.stub.pin)
        exact = E.P.CHECK.recall_tags(dict(type='module', id='invented'), horizon='mid-term')
        with self.assertRaisesRegex(E.P.Stop, '^wrong_scope_or_private$'):
            E.recall(self.stub, 'identifier', exact, self.stub.pin)
        self.stub.fact['text'] = 'identifier omitted'
        with patch.object(E, 'recall', return_value=([self.stub.fact], {})):
            absent = E.canary(self.stub, self.stub.pin, self.data, self.selector, None)
            self.assertEqual([x['literal'] for x in absent['questions']], ['literal_absent']*6)
        self.stub.doc['original_text'] = 'invented replacement'
        with self.assertRaisesRegex(E.P.Stop, '^document_bytes$'):
            E.gather(self.stub, self.data['questions'][:1], self.stub.pin, self.selector, [])
    def test_offline_cli_checks_and_measurement_files(self):
        paths = {n: self.directory / (n+'.json') for n in ('corpus', 'evidence', 'results')}
        for name, value in [('corpus', self.data), ('evidence', self.evidence), ('results', self.results)]:
            paths[name].write_bytes(E.encode(value))
        out = self.directory / 'output.json'
        args = ['--corpus', str(paths['corpus']), '--evidence', str(paths['evidence']), '--output', str(out)]
        self.assertEqual(E.main(['--check-corpus', str(paths['corpus']), '--output', str(out)]), 0)
        self.assertEqual(E.main(['--check-results', str(paths['results'])]+args), 0)
        oid = lambda kind: dict(object_type=kind, namespace=dict(instance='invented'), native_key='invented')
        request = dict(kpi=oid('kpi'), value='invented supported=270', observed_at='2026-11-12T00:00:00.123456+00:00',
                       evidence=str(out), request_id='invented')
        params = self.directory / 'parameters.json'; params.write_bytes(E.encode(request))
        self.assertEqual(E.main(['--measurement-request', str(params), '--output', str(self.directory)]), 0)
        self.assertEqual(json.loads((self.directory/'apply.json').read_bytes())['parameters']['kpi'], oid('kpi'))
        self.assertEqual(json.loads((self.directory/'kpi_get.json').read_bytes()), dict(id=oid('kpi')))
        self.assertEqual(json.loads((self.directory/'apply.json').read_bytes())['parameters']['observed_at'], '2026-11-12T00:00:00Z')
        self.assertFalse((self.directory/'measurement_get.json').exists())

    def cli_results(self, evidence=None, results=None, descriptors=()):
        evidence = self.evidence if evidence is None else evidence
        results = self.results if results is None else results
        paths = {n: self.directory / (n+'.json') for n in ('corpus', 'evidence', 'results', 'checked')}
        for name, value in [('corpus', self.data), ('evidence', evidence), ('results', results)]:
            paths[name].write_bytes(E.encode(value))
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            rc = E.main(['--check-results', str(paths['results']), '--corpus', str(paths['corpus']),
                         '--evidence', str(paths['evidence']), '--output', str(paths['checked'])] + list(descriptors))
        return rc, E.load(paths['checked'].read_bytes()) if rc == 0 else stdout.getvalue()

    def test_full_target_requires_held_out(self):
        rc, result = self.cli_results()
        self.assertEqual(rc, 0)
        self.assertFalse(result['target_reached'])
        self.assertEqual(result['held_out'], 'not_evaluated')

    def test_nonfile_pins_are_recomputed(self):
        for kind in ('decision', 'command', 'github', 'git'):
            with self.subTest(kind=kind):
                source = dict(kind=kind, topic='invented', command='never execute', commit='a'*40)
                self.assertEqual(E.source_pins(dict(sources=[source]))[0]['pin'], 'reverify')
                self.data['questions'][0]['sources'] = [source]
                self.evidence['source_pins']['R01'] = [dict(source, pin='equal')]
                self.results['slots'][0]['expectation_reverified'] = False
                with self.assertRaisesRegex(E.P.Stop, '^expectation_not_reverified$'): self.check()
                self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], 'reverify')

    def test_equal_file_slots_bind_evidence(self):
        self.data['questions'][0]['sources'] = [dict(kind='file', path='invented', anchor='invented', sha256='a'*64)]
        self.evidence['source_pins']['R01'] = [dict(pin='equal')]
        self.results['slots'][0]['evidence_sha256'] = '0'*64
        with self.assertRaisesRegex(E.P.Stop, '^evidence_hash_mismatch$'): self.check()

    def test_main_held_out_guard_and_target_conjunction(self):
        sealed, held, evidence, fixed, private = self.held_fixture()
        rc, error = self.cli_results(evidence, fixed)
        self.assertEqual(rc, 1)
        self.assertIn('held_out_judgments_required', error)
        for fixed_pass, held_pass in [(True, True), (True, False), (False, True)]:
            with self.subTest(fixed=fixed_pass, held=held_pass):
                sealed, held, evidence, fixed, private = self.held_fixture()
                fixed['target_reached'] = fixed_pass
                if not fixed_pass: fixed['slots'][0]['verdict'] = 'partial'
                private['target_reached'] = held_pass
                if not held_pass: private['slots'][0]['verdict'] = 'partial'
                rc, result = self.check_held(sealed, held, evidence, fixed, private)
                self.assertEqual(rc, 0)
                self.assertIs(result['target_reached'], fixed_pass and held_pass)
                self.assertEqual(result['held_out']['supported'], 6 if held_pass else 5)

    def test_canary_reports_changed_profile(self):
        baseline = dict(profile=E.profile(self.stub, self.stub.pin))
        baseline['profile']['bank_instruction_sha256'] = 'changed'
        result = E.canary(self.stub, self.stub.pin, self.data, self.selector, baseline)
        self.assertEqual(result['profile_comparison'], 'drift')
        self.assertEqual(result['route'], 'unavailable')

    def test_root_default_and_checker_due_function(self):
        output = self.directory / 'collected.json'
        nodes = {('memory-map', 'invented-root'): dict(parent=None)}
        with contextlib.ExitStack() as stack:
            for name, value in [('locked', contextlib.nullcontext), ('run_budget', contextlib.nullcontext),
                                ('pinned', lambda: self.stub.pin), ('credential', lambda _: 'invented'),
                                ('Client', lambda _: self.stub)]: stack.enter_context(patch.object(E.P, name, value))
            stack.enter_context(patch.object(E.P.CHECK, 'load', return_value=(0, {}, nodes, [])))
            gathered = stack.enter_context(patch.object(E, 'collect', return_value={}))
            self.assertEqual(E.main(['--collect', '--credential-fd', '3', '--output', str(output)]), 0)
            self.assertEqual(gathered.call_args.args[4]['tags'], ['within:memory-map:invented-root'])
        argv = E.sys.argv
        with patch.object(E.P.CHECK, 'main', side_effect=lambda: (print('[{},{}]') or 0)) as due:
            collected = E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [])
            self.assertEqual(collected['due_count'], 2)
        due.assert_called_once_with()
        self.assertIs(E.sys.argv, argv)
        self.assertRegex(self.evidence['observed_at'], r'^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$')

    def test_empty_held_out_refused_before_evidence_write(self):
        with tempfile.TemporaryFile() as incoming, tempfile.TemporaryFile() as private:
            incoming.write(E.encode(dict(questions=[]))); incoming.seek(0)
            with patch.object(E, 'write_fd') as written, self.assertRaisesRegex(E.P.Stop, '^held_out_empty$'):
                E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [], incoming.fileno(), private.fileno())
            written.assert_not_called()
        evidence = dict(self.evidence, held_out=dict(count=0, sha256='a'*64))
        fixed = copy.deepcopy(self.results)
        for slot in fixed['slots']: slot['evidence_sha256'] = E.P.digest(E.encode(evidence))
        with tempfile.TemporaryFile() as ef, tempfile.TemporaryFile() as rf:
            ef.write(E.encode(dict(questions=[], slots=[], source_pins={}, input_sha256='a'*64))); ef.seek(0)
            rf.write(E.encode(dict(target_reached=True, slots=[]))); rf.seek(0)
            rc, error = self.cli_results(evidence, fixed, ['--held-out-evidence-fd', str(ef.fileno()),
                                                          '--held-out-results-fd', str(rf.fileno())])
        self.assertEqual(rc, 1)
        self.assertIn('held_out_empty', error)

    def test_file_pins_are_recomputed(self):
        path = self.directory / 'source'; path.write_bytes(b'invented')
        digest = E.P.digest(path.read_bytes())
        for source, supplied, expected in [
                (dict(volatile=True), dict(pin='equal'), 'reverify'),
                (dict(sha256='a'*64), dict(pin='equal', sha256_now='a'*64), 'source_changed'),
                (dict(sha256=digest), dict(pin='missing'), 'equal'),
                (dict(volatile=True), dict(pin='missing'), 'missing'),
                (dict(sha256=digest), dict(pin='equal', sha256_now='b'*64), 'equal')]:
            with self.subTest(expected=expected):
                source.update(kind='file', path=str(path), anchor='invented')
                self.data['questions'][0]['sources'] = [source]
                self.evidence['source_pins']['R01'] = [supplied]
                self.results['slots'][0]['expectation_reverified'] = False
                if expected == 'equal':
                    self.assertTrue(self.check()['target_reached'])
                else:
                    with self.assertRaisesRegex(E.P.Stop, '^expectation_not_reverified$'): self.check()
                self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], expected)

    def test_held_out_binding_guard(self):
        for changed in ('hash', 'count'):
            with self.subTest(changed=changed):
                sealed, private, evidence, fixed, judged = self.held_fixture()
                if changed == 'hash': private['input_sha256'] = 'b'*64
                else: evidence['held_out']['count'] = 2
                rc, error = self.check_held(sealed, private, evidence, fixed, judged)
                self.assertEqual(rc, 1)
                self.assertIn('held_out_binding', error)

    def test_evidence_slots_guard(self):
        self.evidence['slots'].pop()
        with self.assertRaisesRegex(E.P.Stop, '^evidence_slots_incomplete$'): self.check()

    def test_corpus_changed_guard(self):
        self.evidence['corpus_sha256'] = '0'*64
        for slot in self.results['slots']: slot['evidence_sha256'] = E.P.digest(E.encode(self.evidence))
        rc, error = self.cli_results()
        self.assertEqual(rc, 1)
        self.assertIn('corpus_changed', error)

    def test_current_count_guard(self):
        self.results['slots'][0]['current_count'] = -1
        with self.assertRaisesRegex(E.P.Stop, '^current_count_invalid$'): self.check()

    def test_main_held_out_channel_guard_precedes_corpus_read(self):
        output = io.StringIO()
        with patch.object(E.Path, 'read_bytes') as read, contextlib.redirect_stdout(output):
            self.assertEqual(E.main(['--collect', '--held-out-fd', '3']), 1)
        self.assertIn('held_out_evidence_fd_required', output.getvalue())
        read.assert_not_called()

    def test_sixth_recall_item_never_feeds_documents(self):
        items = [dict(self.stub.fact, id='invented-fact-'+str(i),
                      document_id='invented' if i < 5 else 'oracle-document-id') for i in range(6)]
        original = self.stub.request
        def read(method, path, body=None):
            return dict(results=items) if method == 'POST' else original(method, path, body)
        self.stub.calls.clear()
        with patch.object(self.stub, 'request', side_effect=read):
            slots = E.gather(self.stub, self.data['questions'][:1], self.stub.pin, self.selector, [])
        self.assertEqual([p for m, p, _ in self.stub.calls if m == 'GET'], [E.P.B+'/documents/invented']*3)
        self.assertEqual([s['returned_count'] for s in slots if s['route'] == 'recall'], [5]*3)
        self.assertEqual([s['native_returned_count'] for s in slots if s['route'] == 'recall'], [6]*3)

    def test_checker_due_arguments_and_system_exit(self):
        class Clock(E.datetime.datetime):
            @classmethod
            def now(cls, tz=None): return cls(2026, 11, 12, 23, 59, 59, tzinfo=E.datetime.timezone.utc)
        seen, argv = [], E.sys.argv
        def checker():
            seen.append(list(E.sys.argv)); print('[]'); return 0
        with patch.object(E.datetime, 'datetime', Clock), patch.object(E.P.CHECK, 'main', side_effect=checker):
            self.assertEqual(E.due_count('b'*40), 0)
        self.assertEqual(seen, [['check-memory-records.py', '--commit', 'b'*40, '--due', '2026-11-12']])
        self.assertIs(E.sys.argv, argv)
        output = io.StringIO()
        with contextlib.ExitStack() as stack:
            for name, value in [('locked', contextlib.nullcontext), ('run_budget', contextlib.nullcontext),
                                ('pinned', lambda: self.stub.pin), ('credential', lambda _: 'invented'),
                                ('Client', lambda _: self.stub)]: stack.enter_context(patch.object(E.P, name, value))
            stack.enter_context(patch.object(E.P.CHECK, 'load', return_value=(0, {}, {('memory-map', 'engineering'): dict(parent=None)}, [])))
            stack.enter_context(patch.object(E.P.CHECK, 'main', side_effect=SystemExit(2)))
            stack.enter_context(contextlib.redirect_stdout(output))
            self.assertEqual(E.main(['--collect', '--credential-fd', '3']), 1)
        self.assertIn('checker_argument_failure', output.getvalue())
        self.assertIs(E.sys.argv, argv)

    def held_fixture(self, questions=None):
        questions = copy.deepcopy(questions if questions is not None else self.data['questions'][:1])
        sealed = E.encode(dict(questions=questions))
        private = dict(questions=questions, input_sha256=E.P.digest(sealed),
                       source_pins={q['id']: E.source_pins(q) for q in questions},
                       slots=[copy.deepcopy(s) for q in questions for s in self.evidence['slots'] if s['id'] == q['id']])
        evidence = dict(copy.deepcopy(self.evidence), held_out=dict(count=len(questions), sha256=E.P.digest(sealed)))
        fixed = copy.deepcopy(self.results)
        judged = dict(target_reached=True, slots=[dict(id=s['id'], form=s['form'], route=s['route'],
            verdict='supported', current_count=1, expectation_reverified=True) for s in private['slots']])
        for slot in fixed['slots']: slot['evidence_sha256'] = E.P.digest(E.encode(evidence))
        for slot in judged['slots']: slot['evidence_sha256'] = E.P.digest(E.encode(private))
        return sealed, private, evidence, fixed, judged

    def check_held(self, sealed, private, evidence, fixed, judged, include_input=True):
        for slot in fixed['slots']: slot['evidence_sha256'] = E.P.digest(E.encode(evidence))
        for slot in judged['slots']: slot['evidence_sha256'] = E.P.digest(E.encode(private))
        with tempfile.TemporaryFile() as hf, tempfile.TemporaryFile() as ef, tempfile.TemporaryFile() as rf:
            for stream, raw in [(hf, sealed), (ef, E.encode(private)), (rf, E.encode(judged))]:
                stream.write(raw); stream.seek(0)
            args = ['--held-out-evidence-fd', str(ef.fileno()), '--held-out-results-fd', str(rf.fileno())]
            if include_input: args += ['--held-out-fd', str(hf.fileno())]
            return self.cli_results(evidence, fixed, args)

    def test_duplicate_held_out_refused_before_reads_or_evidence(self):
        questions = [copy.deepcopy(self.data['questions'][0]) for _ in range(2)]
        questions[1]['forms']['en'] = 'Invented second question with the same id'
        self.stub.calls.clear()
        with tempfile.TemporaryFile() as incoming, tempfile.TemporaryFile() as private:
            incoming.write(E.encode(dict(questions=questions))); incoming.seek(0)
            with self.assertRaisesRegex(E.P.Stop, '^held_out_duplicate_id$'):
                E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [], incoming.fileno(), private.fileno())
            self.assertEqual(private.tell(), 0)
        self.assertEqual(self.stub.calls, [])

    def test_duplicate_question_ids_at_check(self):
        for data in (self.data, dict(questions=self.data['questions'][:1])):
            with self.subTest(count=len(data['questions'])):
                data = copy.deepcopy(data); data['questions'].append(copy.deepcopy(data['questions'][0]))
                code = 'duplicate_id' if 'schema' in data else 'held_out_duplicate_id'
                with self.assertRaisesRegex(E.P.Stop, '^' + code + '$'):
                    E.check_results(self.results, self.evidence, self.hash, data)

    def test_duplicate_evidence_keys_fixed_and_held_out(self):
        self.evidence['slots'].append(copy.deepcopy(self.evidence['slots'][0]))
        with self.assertRaisesRegex(E.P.Stop, '^evidence_slots_duplicate$'): self.check()
        self.evidence['slots'].pop()
        sealed, private, evidence, fixed, judged = self.held_fixture()
        private['slots'].append(copy.deepcopy(private['slots'][0]))
        rc, error = self.check_held(sealed, private, evidence, fixed, judged)
        self.assertEqual(rc, 1); self.assertIn('evidence_slots_duplicate', error)

    def test_changed_file_since_collect_requires_reverification(self):
        path = self.directory / 'source'; path.write_bytes(b'invented old source')
        source = dict(kind='file', path=str(path), anchor='invented', sha256=E.P.digest(path.read_bytes()))
        self.data['questions'][0]['sources'] = [source]
        collected = E.source_pins(dict(sources=[source]))
        self.evidence['source_pins']['R01'] = copy.deepcopy(collected)
        self.assertEqual(self.check()['pins_changed_since_collect'], 0)
        path.write_bytes(b'invented new source')
        for slot in self.results['slots'][:6]: slot['expectation_reverified'] = False
        with self.assertRaisesRegex(E.P.Stop, '^expectation_not_reverified$'): self.check()
        for slot in self.results['slots'][:6]: slot['expectation_reverified'] = True
        self.evidence['source_pins']['R01'] = copy.deepcopy(collected)
        checked = self.check()
        self.assertEqual(checked['pins_changed_since_collect'], 1)
        self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], 'source_changed')
        path.unlink(); self.evidence['source_pins']['R01'] = copy.deepcopy(collected)
        for slot in self.results['slots'][:6]: slot['expectation_reverified'] = False
        with self.assertRaisesRegex(E.P.Stop, '^expectation_not_reverified$'): self.check()
        self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], 'missing')

    def test_empty_source_pins_against_volatile_source(self):
        self.data['questions'][0]['sources'] = [dict(kind='file', path='invented', anchor='invented', volatile=True)]
        self.evidence['source_pins']['R01'] = []
        for slot in self.results['slots'][:6]: slot['expectation_reverified'] = False
        with self.assertRaisesRegex(E.P.Stop, '^source_pins_incomplete$'): self.check()

    def test_held_out_check_requires_sealed_input(self):
        args = self.held_fixture()
        rc, error = self.check_held(*args, include_input=False)
        self.assertEqual(rc, 1); self.assertIn('held_out_input_required', error)

    def test_held_out_input_content_and_hash_binding(self):
        for changed in ('id', 'forms', 'sources', 'expectation', 'oracle', 'order', 'sealed_bytes', 'public_hash'):
            with self.subTest(changed=changed):
                args = list(self.held_fixture(self.data['questions'][:2]))
                sealed, private, evidence, fixed, judged = args
                if changed == 'order': private['questions'].reverse()
                elif changed == 'sealed_bytes': args[0] = sealed + b' '
                elif changed == 'public_hash': evidence['held_out']['sha256'] = 'b'*64
                else: private['questions'][0][changed] = 'invented substitution'
                rc, error = self.check_held(*args)
                self.assertEqual(rc, 1); self.assertIn('held_out_binding', error)

    def test_valid_two_question_held_out_and_private_output(self):
        questions = copy.deepcopy(self.data['questions'][:2])
        for q in questions: q['oracle'] = 'invented-private-oracle'
        rc, checked = self.check_held(*self.held_fixture(questions))
        self.assertEqual(rc, 0); self.assertTrue(checked['target_reached'])
        self.assertEqual(checked['held_out']['supported'], 12)
        self.assertNotIn('invented-private-oracle', E.encode(checked).decode())
        self.assertEqual(checked['pins_changed_since_collect'], 0)

    def test_result_fields_cannot_override_collected_fields(self):
        for name in ('items', 'returned_count', 'latency_ms', 'oracle', 'usage', 'native_returned_count', 'response_bytes'):
            with self.subTest(name=name):
                self.results['slots'][0][name] = 'invented override'
                with self.assertRaisesRegex(E.P.Stop, '^result_field_not_allowed$'): self.check()
                del self.results['slots'][0][name]
        sealed, private, evidence, fixed, judged = self.held_fixture()
        judged['slots'][0]['items'] = []
        rc, error = self.check_held(sealed, private, evidence, fixed, judged)
        self.assertEqual(rc, 1); self.assertIn('result_field_not_allowed', error)

    def test_supported_requires_positive_returned_and_current_counts(self):
        for returned, current in ((0, 0), (1, 0)):
            with self.subTest(returned=returned, current=current):
                self.evidence['slots'][0]['returned_count'] = returned
                self.evidence['slots'][0]['items'] = [self.stub.fact] * returned
                self.results['slots'][0]['current_count'] = current
                with self.assertRaisesRegex(E.P.Stop, '^supported_without_items$'): self.check()
                self.results['slots'][0]['verdict'] = 'missing'; self.results['target_reached'] = False
                self.assertFalse(self.check()['target_reached'])
                self.results['slots'][0]['verdict'] = 'supported'; self.results['target_reached'] = True
        self.evidence['slots'][0]['returned_count'] = 100
        self.evidence['slots'][0]['items'] = [self.stub.fact] * 100
        self.results['slots'][0]['current_count'] = 1
        self.assertEqual(self.check()['slots'][0]['current_item_share'], 0.01)


    def test_held_out_sources_required_before_reads(self):
        question = copy.deepcopy(self.data['questions'][0]); question['sources'] = []
        self.stub.calls.clear()
        with tempfile.TemporaryFile() as incoming, tempfile.TemporaryFile() as private:
            incoming.write(E.encode(dict(questions=[question]))); incoming.seek(0)
            with self.assertRaisesRegex(E.P.Stop, '^held_out_sources_required$'):
                E.collect(self.stub, self.stub.pin, self.data, self.raw, self.selector, [], incoming.fileno(), private.fileno())
            self.assertEqual(private.tell(), 0)
        self.assertEqual(self.stub.calls, [])

    def test_returned_count_must_match_collected_items(self):
        for returned in (0, 2, True):
            with self.subTest(returned=returned):
                self.evidence['slots'][0]['items'] = [self.stub.fact]
                self.evidence['slots'][0]['returned_count'] = returned
                with self.assertRaisesRegex(E.P.Stop, '^returned_count_mismatch$'): self.check()
        self.evidence['slots'][0]['returned_count'] = 1
        sealed, private, evidence, fixed, judged = self.held_fixture()
        private['slots'][0]['returned_count'] = 2
        rc, error = self.check_held(sealed, private, evidence, fixed, judged)
        self.assertEqual(rc, 1); self.assertIn('returned_count_mismatch', error)

    def test_held_out_changed_pin_reaches_public_count(self):
        path = self.directory / 'held-source'; path.write_bytes(b'invented old source')
        question = copy.deepcopy(self.data['questions'][0])
        question['sources'] = [dict(kind='file', path=str(path), anchor='invented', sha256=E.P.digest(path.read_bytes()))]
        args = self.held_fixture([question])
        path.write_bytes(b'invented new source')
        rc, checked = self.check_held(*args)
        self.assertEqual(rc, 0); self.assertTrue(checked['target_reached'])
        self.assertEqual(checked['pins_changed_since_collect'], 1)

    def test_pin_change_count_compares_values_not_metadata(self):
        path = self.directory / 'source'; path.write_bytes(b'invented old source')
        source = dict(kind='file', path=str(path), anchor='invented', sha256='0'*64)
        self.data['questions'][0]['sources'] = [source]
        self.evidence['source_pins']['R01'] = E.source_pins(dict(sources=[source]))
        self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], 'source_changed')
        path.write_bytes(b'invented other source')
        checked = self.check()
        self.assertEqual(checked['pins_changed_since_collect'], 0)
        self.assertEqual(self.evidence['source_pins']['R01'][0]['pin'], 'source_changed')

    def test_duplicate_fixed_corpus_code(self):
        self.data['questions'][1]['id'] = self.data['questions'][0]['id']
        with self.assertRaisesRegex(E.P.Stop, '^duplicate_id$'): E.corpus(E.encode(self.data))
