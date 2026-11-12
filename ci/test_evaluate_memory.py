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
    def request(self, method, path, body=None):
        if method == 'POST':
            self.calls.append((method, path, body))
            return dict(results=[dict(self.fact, text='marker')])
        return super().request(method, path, body)


class EvaluationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.network = patch.object(socket, 'create_connection', side_effect=AssertionError('offline_only')).start()
        self.addCleanup(patch.stopall)
        self.stub, self.data = Stub(), invented()
        self.selector = E.P.CHECK.recall_tags(dict(type='memory-map', id='engineering'), descendants=True)
        records = {'invented': dict(header=dict(kind='incident', horizon='mid-term', review_on='2000-01-01',
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
                with self.assertRaisesRegex(E.P.Stop, '^expectation_not_reverified$'): self.check()
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
        request = dict(kpi=oid('kpi'), value='invented supported=270', observed_at='2026-11-12T00:00:00Z',
                       evidence=str(out), request_id='invented', measurement_id=oid('measurement'))
        params = self.directory / 'parameters.json'; params.write_bytes(E.encode(request))
        self.assertEqual(E.main(['--measurement-request', str(params), '--output', str(self.directory)]), 0)
        self.assertEqual(json.loads((self.directory/'apply.json').read_bytes())['parameters']['kpi'], oid('kpi'))
        self.assertEqual(json.loads((self.directory/'measurement_get.json').read_bytes()), dict(id=oid('measurement')))
