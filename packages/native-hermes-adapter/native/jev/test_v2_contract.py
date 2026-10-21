"""Imported native source fixtures: synthetic owner, bytes and zero network."""
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

import semantic_caller as caller
from semantic_caller import Held, NativeResultReceipts, parse_request, run
from test_semantic_caller import request as v1_request
from v2_contract import POLICY, admit_existing_client, canonical_json, digest, validate_answers


def request():
    r = v1_request()
    r.update(schema='jev.semantic-request.v2', consumptionPolicy=POLICY, imageOptIn=False)
    r['state']['directBoundary'] = {
        'opportunity': {'role': 'child', 'unit': 'step', 'stableItemId': 'synthetic-item',
                        'revision': 'r1', 'nativeHookId': 'synthetic-hook', 'consumerId': 'synthetic-consumer'},
        'actions': [], 'boxes': {}, 'imageEvidence': {}}
    return seal(r)


def seal(r):
    r['stateDigest'] = digest(r['state'])
    return r


def raw(r):
    return json.dumps(seal(r), ensure_ascii=False, allow_nan=False).encode()


def answers(r):
    result = {}
    for k, q in r['questions'].items():
        if q['type'] == 'noul':
            result[k] = {'type': 'noul', 'noul': 0.5}
        elif q['type'] == 'choice':
            keys = list(q['criteria'])
            result[k] = {'type': 'choice', 'choice': keys[-1], 'confidence': 0,
                         'probabilities': {key: 1 / len(keys) for key in keys}}
        elif q['type'] == 'score':
            keys = list(map(str, range(len(q['criteria']))))
            result[k] = {'type': 'score', 'score': (len(keys) - 1) / 2, 'confidence': 0,
                         'legend': dict(zip(keys, q['criteria'])),
                         'probabilities': {key: 1 / len(keys) for key in keys}}
        else:
            result[k] = {'type': 'box', 'box': {'top': 0, 'left': 0, 'bottom': 1, 'right': 1},
                         'confidence': None}
    return result


def image_request(*, multipart=False):
    r = request()
    r['imageOptIn'] = True
    # Deliberately not a decoded image. Owner declarations cannot admit this.
    image = b'SYNTHETIC_NOT_IMAGE_BYTES'
    sha = hashlib.sha256(image).hexdigest()
    r['assets'] = {'[picture]': ({'type': 'image', 'part': 'picture-file'} if multipart else {
        'type': 'image', 'format': 'png', 'data_base64': base64.b64encode(image).decode()})}
    r['state']['facts']['image'] = '[picture]'
    r['questions']['region'] = {'type': 'box', 'instructions': 'Locate target in [picture].'}
    r['state']['directBoundary']['boxes'] = {'region': {
        'assetRef': '[picture]', 'presenceQuestionId': 'sufficient', 'minimumPresence': 0.75}}
    r['state']['directBoundary']['imageEvidence'] = {'[picture]': {
        'source': {'path': 'synthetic.png', 'sha256': sha}, 'sha256': sha,
        'width': 280, 'height': 280, 'byteLength': len(image), 'processedByteLength': 1000,
        'format': 'png', 'still': True, 'decoded': True, 'evidenceRefs': ['synthetic-evidence']}}
    return seal(r)


class ContractTests(unittest.TestCase):
    def test_text_v2_and_v1_are_explicitly_distinct(self):
        r = request()
        r['questions']['degree'] = {'type': 'score', 'instructions': {'ask': 'Rate degree'},
                                    'criteria': [{'meaning': 'Low'}, ['High']]}
        self.assertEqual(parse_request(raw(r)), r)
        admit_existing_client(r)
        self.assertEqual(validate_answers(r, answers(r)), answers(r))
        for schema in ('jev.semantic-request.v1', 'jev.semantic-request.v3'):
            r['schema'] = schema
            with self.assertRaises(Held):
                parse_request(raw(r))
        self.assertEqual(parse_request(json.dumps(v1_request()).encode()), v1_request())

    def test_low_confidence_and_tied_selected_choice_is_unchanged(self):
        r = request()
        a = answers(r)
        self.assertEqual(a['next']['choice'], '__abstain')
        self.assertIs(validate_answers(r, a), a)
        self.assertEqual(a['next']['confidence'], 0)
        a['next']['probabilities'] = {'read': 0.1, '__abstain': 0.9}
        a['next']['choice'] = 'read'
        with self.assertRaisesRegex(Held, 'v2_choice_membership'):
            validate_answers(r, a)

    def test_exact_result_types_coverage_and_distributions(self):
        r = request()
        for mutate in (
            lambda a: a.pop('next'),
            lambda a: a.update(extra={'type': 'noul', 'noul': 1}),
            lambda a: a['sufficient'].update(noul=True),
            lambda a: a['sufficient'].update(noul=float('nan')),
            lambda a: a['next'].update(type='score'),
            lambda a: a['next'].update(confidence=None),
            lambda a: a['next'].update(choice='invented'),
            lambda a: a['next'].update(probabilities={'read': 1}),
            lambda a: a['next'].update(probabilities={'read': 1, '__abstain': 1}),
            lambda a: a['next'].update(probabilities={'read': -0.001, '__abstain': 1.001}),
        ):
            with self.subTest(mutation=mutate):
                a = answers(r)
                mutate(a)
                with self.assertRaises(Held):
                    validate_answers(r, a)

    def test_score_uses_actual_fractional_value_structured_legend_and_no_default(self):
        r = request()
        r['questions'] = {'score': {'type': 'score', 'instructions': 'Rate the degree.',
                                   'criteria': [{'degree': 'low'}, ['high']]}}
        a = answers(r)
        self.assertEqual(validate_answers(r, a)['score']['score'], 0.5)
        for key, value in [('score', True), ('score', 1.1), ('score', 0.9),
                           ('legend', {'0': 'low', '1': 'high'}), ('probabilities', {'0': 1})]:
            b = copy.deepcopy(a)
            b['score'][key] = value
            with self.assertRaises(Held):
                validate_answers(r, b)
        for key in ('confidence', 'legend', 'probabilities'):
            b = copy.deepcopy(a)
            del b['score'][key]
            with self.assertRaises(Held):
                validate_answers(r, b)

    def test_valid_common_contract_but_existing_client_subset_holds(self):
        cases = []
        for kind, criteria in [('choice', {'only': None}), ('score', ['only'])]:
            r = request()
            r['questions'] = {'single': {'type': kind, 'instructions': 'Choose or rate.', 'criteria': criteria}}
            cases.append((r, 'v2_single_criterion_client_extension_required'))
        for instructions in (None, '', {}, []):
            r = request()
            r['questions'] = {'nullable': {'type': 'noul', 'instructions': instructions}}
            cases.append((r, 'v2_question_content_client_extension_required'))
        r = request()
        r['questions']['sufficient']['criteria'] = {}
        cases.append((r, 'v2_noul_content_client_extension_required'))
        for r, code in cases:
            with self.subTest(code=code, questions=r['questions']):
                parse_request(raw(r))
                with self.assertRaisesRegex(Held, code):
                    admit_existing_client(r)

    def test_v2_policy_digest_and_question_shape_holds(self):
        for mutate in (
            lambda r: r.update(consumptionPolicy='other'),
            lambda r: r.update(imageOptIn=1),
            lambda r: r['questions']['next'].update(type='unknown'),
            lambda r: r['questions']['next'].update(execute='forbidden'),
            lambda r: r['questions']['sufficient'].update(criteria={'other': 'unsupported'}),
            lambda r: r['state'].pop('directBoundary'),
        ):
            r = request()
            mutate(r)
            with self.assertRaises(Held):
                parse_request(raw(r))
        r = request()
        r['state']['facts']['changed'] = True
        with self.assertRaisesRegex(Held, 'v2_state_digest_mismatch'):
            parse_request(json.dumps(r).encode())

    def test_images_are_structurally_valid_but_not_decoder_or_native_support(self):
        for multipart, code in [(False, 'v2_image_decoder_owner_extension_required'),
                                (True, 'v2_multipart_owner_extension_required')]:
            r = image_request(multipart=multipart)
            self.assertEqual(parse_request(raw(r)), r)
            with self.assertRaisesRegex(Held, code):
                admit_existing_client(r)
            # Nullable Box confidence is part of the offline typed contract.
            validate_answers(r, answers(r))

    def test_asset_content_opt_in_bounds_bytes_and_box_presence_binding(self):
        for mutate in (
            lambda r: r.update(imageOptIn=False),
            lambda r: r['state']['facts'].pop('image'),
            lambda r: r['assets']['[picture]'].update(data_base64='not-base64'),
            lambda r: r['assets']['[picture]'].update(part='file'),
            lambda r: r['state']['directBoundary']['imageEvidence']['[picture]'].update(width=279),
            lambda r: r['state']['directBoundary']['imageEvidence']['[picture]'].update(processedByteLength=33554433),
            lambda r: r['state']['directBoundary']['imageEvidence']['[picture]'].update(still=False),
            lambda r: r['state']['directBoundary']['boxes']['region'].update(presenceQuestionId='next'),
            lambda r: r['state']['directBoundary']['boxes']['region'].update(minimumPresence=0.5),
        ):
            r = image_request()
            # Remove the question's reference as well for the metadata-only case.
            r['questions']['region']['instructions'] = 'Locate the target.'
            mutate(r)
            with self.assertRaises(Held):
                parse_request(raw(r))

    def test_canonical_digest_matches_actual_common_source(self):
        repo = Path(__file__).resolve().parents[4]
        values = [request(), {'10': 0.1, '2': 1.0, 'a': [1e-7, 1e-6, 1e20, 1e21, -0.0,
                  1000000000000000128, '한글', '\U0001f600', '\ud800'], '\ue000': 0, '\U00010000': 1}]
        code = ('import {canonicalJson,semanticDigest} from "./packages/work-continuity/src/jev/policy.ts";'
                'const rows=JSON.parse(await Bun.stdin.text());'
                'console.log(JSON.stringify(rows.map(v=>[canonicalJson(v),semanticDigest(v)])));')
        result = subprocess.run(['bun', '-e', code], input=json.dumps(values).encode(),
                                cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        self.assertEqual(json.loads(result.stdout), [[canonical_json(v), digest(v)] for v in values])

    def test_actual_common_preparation_native_result_and_common_consumption_round_trip(self):
        r = request()
        r['questions']['degree'] = {'type': 'score', 'instructions': 'Rate source degree.',
                                    'criteria': [{'low': 'minimal'}, ['high']]}
        binding = {'runtime': 'native-hermes', 'sessionId': 'SYNTHETIC_NOT_NATIVE', 'agentId': None,
                   'agentType': None, 'parentSessionId': None, 'rootSessionId': None,
                   'lineageSessionIds': [], 'profileId': None, 'nativeTaskId': None,
                   'nativeRunId': None, 'logical': None, 'admissionRef': 'synthetic-admission'}
        input_value = {'requestId': 'synthetic-roundtrip', 'event': {
            'eventId': 'synthetic-event', 'kind': 'engineering_result', 'binding': binding,
            'sourceRevision': 'synthetic-source', 'materialRevision': '1', 'material': True,
            'privacy': 'admitted', 'externalEvidence': False, 'facts': {'source': 'synthetic'}},
            'lens': r['state']['requiredContext'], 'opportunity': r['state']['directBoundary']['opportunity'],
            'questions': r['questions'], 'actions': []}
        repo = Path(__file__).resolve().parents[4]
        source = './packages/work-continuity/src/jev/index.ts'
        prepare_code = (f'import {{prepareDirectDecision}} from "{source}";'
                        'console.log(JSON.stringify(prepareDirectDecision(JSON.parse(await Bun.stdin.text()))));')
        prepared = json.loads(subprocess.run(['bun', '-e', prepare_code], cwd=repo,
            input=json.dumps(input_value).encode(), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            check=True).stdout)
        self.assertEqual(prepared['status'], 'prepared-direct')
        self.assertEqual(parse_request(json.dumps(prepared['request']).encode()), prepared['request'])
        with tempfile.TemporaryDirectory() as folder:
            result = OfflineOwner(Path(folder), prepared['request']).run()
        self.assertEqual(result['status'], 'ok', result)
        consume_code = (f'import {{normalizeDirectReply}} from "{source}";'
                        'const x=JSON.parse(await Bun.stdin.text());'
                        'console.log(JSON.stringify(normalizeDirectReply(x.prepared,x.result)));')
        selection = json.loads(subprocess.run(['bun', '-e', consume_code], cwd=repo,
            input=json.dumps({'prepared': prepared, 'result': result}).encode(),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout)
        self.assertEqual(selection['schema'], 'jev.direct-selection.v2')
        self.assertEqual(selection['answers'], result['answers'])
        self.assertFalse(selection['executionAuthorityGranted'])


class OfflineOwner:
    """Synthetic native-route injection, never the installed runtime or provider."""
    def __init__(self, root, r, *, payload=None, drift=False, second_borrow=False, wire_drift=False):
        self.root, self.r, self.calls = root, r, []
        self.payload = payload or {'model': 'jev-1.14.0', 'answers': answers(r),
                                   'usage': {'input_tokens': 11, 'output_tokens': 7}}
        self.drift, self.second_borrow, self.wire_drift = drift, second_borrow, wire_drift
        (root / 'upstream/jevkit').mkdir(parents=True)
        files = {'application-manifest.json': json.dumps({'deploymentRoot': str(root)}).encode(),
                 'native-caller.py': b'SYNTHETIC_CALLER', 'upstream/jevkit/client.py': b'SYNTHETIC_CLIENT'}
        for name, body in files.items():
            (root / name).write_bytes(body)
        self.caller_sha = hashlib.sha256(files['native-caller.py']).hexdigest()
        self.manifest_sha = hashlib.sha256(files['application-manifest.json']).hexdigest()
        self.client_sha = hashlib.sha256(files['upstream/jevkit/client.py']).hexdigest()
        self.receipts = NativeResultReceipts(root, uid=os.getuid(), gid=os.getgid())
        self.adapter = types.SimpleNamespace(verify_sources=self.verify_sources,
            verify_loaded_origins=lambda *args: ['synthetic-origin'],
            BASE='https://synthetic.invalid', ENDPOINT='https://synthetic.invalid/v1/systemone',
            SingleRequestPool=self.pool_type(), usage_only=lambda u: dict(u) if isinstance(u, dict) else {})
        self.client = types.SimpleNamespace(check_questions=lambda q: copy.deepcopy(q), ask=self.ask,
                                            _http_transport=self.http, _POOL=object(), JevError=Held)

    def pool_type(self):
        class Pool:
            def __init__(self, original, error):
                self.leases = 0
            def borrow(self):
                if self.leases:
                    raise Held('http_budget_exhausted')
                self.leases += 1
        return Pool

    def verify_sources(self, manifest):
        self.calls.append('source')
        if self.drift and 'http' in self.calls:
            raise Held('synthetic_source_drift')

    def descriptor(self):
        self.calls.append('descriptor')
        return {'status': 'successor-source-verified', 'callerSha256': self.caller_sha,
                'manifestSha256': self.manifest_sha}

    def getter(self, key):
        self.calls.append('synthetic-credential')
        return 'SYNTHETIC_NOT_A_CREDENTIAL'

    def ask(self, state, questions, **kwargs):
        self.calls.append('ask')
        assert kwargs['retries'] == 0 and kwargs['model'] == caller.MODEL
        assert kwargs['provider'] == 'typesafe'
        body = {'model': caller.MODEL, 'state': state, 'questions': questions}
        if self.wire_drift:
            body['state'] = {'changed': True}
        kwargs['transport'](json.dumps(body).encode(), {}, 1)
        # Deliberately transformed client result must not become v2 provenance.
        return {'answers': {'client-projection': 'SYNTHETIC_NOT_RAW'}}

    def http(self, *args):
        self.client._POOL.borrow()
        self.calls.append('http')
        if self.second_borrow:
            self.client._POOL.borrow()
        return json.dumps(self.payload).encode()

    def run(self):
        kit = types.ModuleType('jevkit')
        kit.client, kit.privacy, kit.rerank = self.client, types.SimpleNamespace(is_sensitive=lambda _: False), object()
        config = types.ModuleType('hermes_cli.config')
        config.get_env_value_prefer_dotenv = self.getter
        spec = types.SimpleNamespace(loader=types.SimpleNamespace(exec_module=lambda _: None))
        with patch.multiple(caller, ROOT=self.root, CALLER_SHA=self.caller_sha,
                            MANIFEST_SHA=self.manifest_sha, CLIENT_SHA=self.client_sha), \
                patch.object(caller, 'NativeResultReceipts', return_value=self.receipts), \
                patch.object(caller.os, 'getuid', return_value=10000), \
                patch.object(caller.os, 'getgid', return_value=10000), \
                patch.dict(os.environ, {'HOME': '/opt/data', 'HERMES_HOME': '/opt/data'}), \
                patch.dict(sys.modules, {'jevkit': kit, 'hermes_cli.config': config}), \
                patch.object(sys, 'path', list(sys.path)), \
                patch.object(caller.importlib.util, 'spec_from_file_location', return_value=spec), \
                patch.object(caller.importlib.util, 'module_from_spec', return_value=self.adapter):
            return run(raw(self.r), descriptor_verify=self.descriptor)


class OfflineTransportTests(unittest.TestCase):
    def test_one_existing_route_preserves_original_raw_answers_usage_and_receipt(self):
        r = request()
        r['questions']['degree'] = {'type': 'score', 'instructions': 'Rate degree.',
                                    'criteria': [{'low': 'minimal'}, ['high']]}
        with tempfile.TemporaryDirectory() as folder:
            owner = OfflineOwner(Path(folder), r)
            got = owner.run()
            self.assertEqual(got['status'], 'ok', got)
            self.assertEqual(got['schema'], 'jev.semantic-result.v2')
            self.assertEqual(got['consumptionPolicy'], POLICY)
            self.assertEqual(got['requestDigest'], digest(r))
            self.assertEqual(got['submittedRequestSha256'], hashlib.sha256(raw(r)).hexdigest())
            self.assertEqual(got['answers'], answers(r))
            self.assertEqual(got['usage'], {'input_tokens': 11, 'output_tokens': 7})
            self.assertEqual((got['transportInvocations'], got['connectionLeases']), (1, 1))
            self.assertTrue(got['sourcePostconditionVerified'])
            self.assertFalse(got['executionAuthorityGranted'])
            calls = owner.calls.copy()
            self.assertEqual(owner.run(), got)
            self.assertEqual(owner.calls, calls + ['descriptor', 'descriptor'])

    def test_postflight_drift_and_malformed_answers_never_discard_original_effect(self):
        for mode in ('source', 'answers', 'model', 'missing-score-confidence'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as folder:
                r = request()
                r['questions']['degree'] = {'type': 'score', 'instructions': 'Rate degree.',
                                           'criteria': ['low', 'high']}
                payload = {'model': 'jev-1.14.0', 'answers': answers(r),
                           'usage': {'input_tokens': 11, 'output_tokens': 7}}
                if mode == 'answers':
                    payload['answers']['sufficient']['noul'] = True
                elif mode == 'model':
                    payload['model'] = 'jev-latest'
                elif mode == 'missing-score-confidence':
                    del payload['answers']['degree']['confidence']
                owner = OfflineOwner(Path(folder), r, payload=payload, drift=mode == 'source')
                got = owner.run()
                self.assertEqual(got['status'], 'error', got)
                self.assertEqual(got['answers'], {})
                self.assertEqual(got['transportInvocations'], 1)
                self.assertEqual(got['usage']['input_tokens'], 11)
                self.assertEqual(owner.run(), got)
                self.assertEqual(owner.calls.count('http'), 1)

    def test_images_and_client_subset_hold_before_import_credentials_and_http(self):
        for r, code in [(image_request(), 'v2_image_decoder_owner_extension_required'),
                        (image_request(multipart=True), 'v2_multipart_owner_extension_required')]:
            with self.subTest(code=code), tempfile.TemporaryDirectory() as folder:
                owner = OfflineOwner(Path(folder), r)
                got = owner.run()
                self.assertEqual(got['status'], 'held')
                self.assertEqual(got['errorCode'], code)
                self.assertEqual(got['transportInvocations'], 0)
                self.assertEqual(owner.calls, ['descriptor', 'descriptor'])

    def test_wire_projection_drift_and_second_lease_cannot_send_again(self):
        for mode in ('wire', 'lease'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as folder:
                owner = OfflineOwner(Path(folder), request(), wire_drift=mode == 'wire', second_borrow=mode == 'lease')
                got = owner.run()
                self.assertEqual(got['errorCode'], 'v2_client_wire_projection_changed' if mode == 'wire'
                                 else 'http_budget_exhausted')
                self.assertEqual(got['transportInvocations'], 0 if mode == 'wire' else 1)
                self.assertEqual(owner.calls.count('http'), 0 if mode == 'wire' else 1)


if __name__ == '__main__':
    unittest.main()
