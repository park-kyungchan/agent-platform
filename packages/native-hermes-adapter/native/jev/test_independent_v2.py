"""Independent imported-caller checks. All native I/O is replaced before run().

The fake client is an offline transport fixture, not a claimed provider SDK.
No installed code is imported, credentials resolved, sockets opened or receipts
written. Assertions target semantic_caller's actual parser and run control flow.
"""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import types
import tempfile
import unittest
from contextlib import ExitStack
from unittest.mock import patch

import semantic_caller as caller


def request(version=2):
    value = {
        'schema': 'jev.semantic-request.v' + str(version),
        'requestId': 'independent-synthetic-1', 'model': caller.MODEL,
        'purpose': 'semantic_bundle', 'stateDigest': 'a' * 64,
        'state': {'requiredContext': {
            'currentAuthority': {'text': 'Synthetic offline source verification', 'sourceRefs': ['synthetic']},
            'wholeObjective': 'Preserve source contracts', 'taskGoal': 'Verify caller',
            'acceptedWhy': [{'text': 'Verify exact source', 'attribution': 'current-user', 'sourceRefs': ['synthetic']}],
            'acceptedDecisions': [], 'owners': [{'role': 'native', 'id': 'synthetic'}],
            'dependencies': [], 'completedEffectRefs': [], 'uncertainEffectRefs': [],
            'remainingAcceptance': ['Actual native integration remains unverified']},
            'binding': {'nativeTaskId': None}, 'facts': {}},
        'questions': {'pick': {'type': 'choice', 'instructions': 'Which source read is appropriate?',
                                'criteria': {'read': 'Read exact source', 'hold': 'Hold'}}},
        'scope': {'sessionId': 'SYNTHETIC_NOT_NATIVE', 'agentId': None, 'agentType': None,
                  'taskGoal': 'Verify caller', 'wholeObjective': 'Preserve source contracts',
                  'why': 'Verify exact source', 'owners': ['native:synthetic']},
        **({'consumptionPolicy': 'jev.direct-selected-answer.v2', 'imageOptIn': False} if version == 2 else {})}
    if version == 2:
        value['state']['directBoundary'] = {
            'opportunity': {'role': 'child', 'unit': 'item', 'stableItemId': 'item:synthetic',
                            'revision': '1', 'nativeHookId': 'owner:synthetic-hook',
                            'consumerId': 'owner:synthetic-consumer'},
            'actions': [], 'boxes': {}, 'imageEvidence': {}}
        value['stateDigest'] = caller.common_digest(value['state'])
    return value


def raw(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(',', ':')).encode()


def answer_payload():
    return {'model': 'jev-1.14.0', 'answers': {'pick': {
        'type': 'choice', 'choice': 'read', 'confidence': 0.01,
        'probabilities': {'read': 0.5, 'hold': 0.5}}},
        'usage': {'input_tokens': 7, 'output_tokens': 3, 'total_tokens': 10}}


class MemoryPath:
    def __init__(self, fixture, suffix=''):
        self.fixture, self.suffix = fixture, suffix

    def __truediv__(self, name):
        return MemoryPath(self.fixture, self.suffix + '/' + name)

    def __str__(self):
        return '/synthetic-jev-owner' + self.suffix

    def read_bytes(self):
        if self.suffix == '/application-manifest.json':
            return self.fixture.manifest
        if self.suffix == '/native-caller.py':
            return self.fixture.adapter_bytes
        if self.suffix == '/upstream/jevkit/client.py':
            return self.fixture.client_bytes
        raise AssertionError('Unexpected owner file access')


class OfflineOwner:
    """Inject only synthetic source metadata, receipt owner and one offline client."""
    def __init__(self, payload=None):
        self.payload = answer_payload() if payload is None else payload
        self.calls, self.asks, self.published, self.sources = [], [], [], 0
        self.existing = None
        self.drift = False
        self.parsed_reply = None
        self.transport_error = None
        self.double_transport = False
        self.manifest = raw({'deploymentRoot': '/synthetic-jev-owner'})
        self.adapter_bytes = b'SYNTHETIC_NOT_INSTALLED_SOURCE'
        self.client_bytes = b'SYNTHETIC_NOT_INSTALLED_CLIENT_SOURCE'
        self.caller_sha = hashlib.sha256(self.adapter_bytes).hexdigest()
        self.manifest_sha = hashlib.sha256(self.manifest).hexdigest()
        self.client = types.ModuleType('jevkit.client')
        self.client.JevError = type('JevError', (Exception,), {})
        self.client._POOL = object()
        self.client.check_questions = copy.deepcopy
        self.client.ask = self.ask
        self.actual_client = None
        self.client._http_transport = self.transport
        self.adapter = types.ModuleType('synthetic_owner_adapter')
        self.adapter.BASE = 'https://synthetic.invalid/typesafe'
        self.adapter.ENDPOINT = self.adapter.BASE + '/v1/systemone'
        self.adapter.verify_sources = self.verify_sources
        self.adapter.verify_loaded_origins = lambda *args: [{'source': 'synthetic'}]
        self.adapter.usage_only = lambda value: {k: v for k, v in (value or {}).items()
            if k in ('input_tokens', 'output_tokens', 'total_tokens')
            and type(v) is int and v >= 0}
        fixture = self

        class Pool:
            def __init__(self, original, error):
                self.leases = 0
            def borrow(self, *args):
                if self.leases:
                    raise caller.Held('http_budget_exhausted')
                self.leases += 1
        self.adapter.SingleRequestPool = Pool

        class Receipts:
            def __init__(self, *args):
                pass
            def ready(self):
                pass
            def read(self, *args):
                return fixture.existing
            def publish(self, result):
                fixture.published.append(copy.deepcopy(result))
                return result
        self.receipts = Receipts

    def descriptor(self):
        return {'status': 'successor-source-verified', 'callerSha256': self.caller_sha,
                'manifestSha256': self.manifest_sha}

    def verify_sources(self, manifest):
        self.sources += 1
        if self.drift and self.sources > 1:
            raise caller.Held('synthetic_source_drift')

    def transport(self, body, headers, timeout, endpoint, limit):
        self.calls.append({'body': json.loads(body), 'endpoint': endpoint})
        self.client._POOL.borrow(None, timeout)
        if self.transport_error:
            raise self.transport_error
        return self.payload if isinstance(self.payload, bytes) else raw(self.payload)

    def ask(self, state, questions, **kwargs):
        self.asks.append({k: v for k, v in kwargs.items() if k != 'transport'})
        body = raw({'state': state, 'questions': questions, 'model': kwargs['model'],
                    **({'assets': kwargs['assets']} if kwargs.get('assets') else {})})
        response = kwargs['transport'](body, {}, 20)
        if self.double_transport:
            kwargs['transport'](body, {}, 20)
        parsed = json.loads(response)
        return {'answers': self.parsed_reply if self.parsed_reply is not None else parsed['answers']}

    def run(self, value, descriptor=None):
        kit = types.ModuleType('jevkit')
        kit.client = self.actual_client or self.client
        kit.privacy = types.SimpleNamespace(is_sensitive=lambda value: False)
        kit.rerank = types.ModuleType('jevkit.rerank')
        config = types.ModuleType('hermes_cli.config')
        config.get_env_value_prefer_dotenv = lambda name: 'SYNTHETIC_OFFLINE_CREDENTIAL'
        loader = types.SimpleNamespace(exec_module=lambda module: None)
        spec = types.SimpleNamespace(loader=loader)
        with ExitStack() as stack:
            if self.actual_client is not None:
                self.client = self.actual_client
                stack.enter_context(patch.object(self.client, '_POOL', object()))
                stack.enter_context(patch.object(self.client, '_http_transport', self.transport))
            stack.enter_context(patch.object(caller, 'ROOT', MemoryPath(self)))
            stack.enter_context(patch.object(caller, 'CALLER_SHA', self.caller_sha))
            stack.enter_context(patch.object(caller, 'MANIFEST_SHA', self.manifest_sha))
            stack.enter_context(patch.object(caller, 'CLIENT_SHA', hashlib.sha256(self.client_bytes).hexdigest()))
            stack.enter_context(patch.object(caller, 'NativeResultReceipts', self.receipts))
            stack.enter_context(patch.object(caller.os, 'getuid', return_value=10000))
            stack.enter_context(patch.object(caller.os, 'getgid', return_value=10000))
            stack.enter_context(patch.object(caller.importlib.util, 'spec_from_file_location', return_value=spec))
            stack.enter_context(patch.object(caller.importlib.util, 'module_from_spec', return_value=self.adapter))
            stack.enter_context(patch.dict(sys.modules, {'jevkit': kit, 'jevkit.client': kit.client,
                'jevkit.privacy': kit.privacy, 'jevkit.rerank': kit.rerank,
                'hermes_cli': types.ModuleType('hermes_cli'), 'hermes_cli.config': config}))
            stack.enter_context(patch.dict(os.environ, {'HOME': '/opt/data', 'HERMES_HOME': '/opt/data'}))
            original_path = list(sys.path)
            try:
                return caller.run(raw(value), descriptor_verify=descriptor or self.descriptor)
            finally:
                sys.path[:] = original_path


class IndependentCallerTests(unittest.TestCase):
    def assert_no_authority(self, result):
        for field in ('executionAuthorityGranted', 'automaticWrite', 'workerCompletionVerified'):
            self.assertIs(result[field], False)

    def test_v1_compatibility_request_and_single_offline_dispatch(self):
        value = request(1)
        self.assertEqual(caller.parse_request(raw(value)), value)
        owner = OfflineOwner()
        result = owner.run(value)
        self.assertEqual(result['status'], 'ok')
        self.assertEqual(result['schema'], 'jev.semantic-result.v1')
        self.assertEqual(result['transportInvocations'], 1)
        self.assertEqual(result['connectionLeases'], 1)
        self.assertEqual(owner.asks[0]['retries'], 0)
        self.assert_no_authority(result)

    def test_v2_low_confidence_tie_preserves_selected_answer_and_binding(self):
        value = request()
        owner = OfflineOwner()
        result = owner.run(value)
        self.assertEqual(result['status'], 'ok')
        self.assertEqual(result['schema'], 'jev.semantic-result.v2')
        self.assertEqual(result['consumptionPolicy'], value['consumptionPolicy'])
        self.assertEqual(result['answers']['pick'], owner.payload['answers']['pick'])
        self.assertEqual(result['requestId'], value['requestId'])
        self.assertEqual(result['stateDigest'], value['stateDigest'])
        self.assertEqual(result['requestDigest'], caller.common_digest(value))
        self.assertEqual(result['submittedRequestSha256'], hashlib.sha256(raw(value)).hexdigest())
        self.assertEqual(result['usage'], owner.payload['usage'])
        self.assertEqual(result['returnedModel'], 'jev-1.14.0')
        self.assertEqual(owner.calls[0]['body']['state']['scope'], value['scope'])
        self.assertEqual(owner.asks[0]['retries'], 0)
        self.assert_no_authority(result)

    def test_v2_noul_and_score_raw_structured_legend_are_not_sdk_rewrites(self):
        value = request()
        value['questions'] = {'present': {'type': 'noul', 'instructions': 'Is the source present?'},
            'grade': {'type': 'score', 'instructions': 'Degree of source coverage?',
                      'criteria': [{'description': 'Absent'}, {'description': 'Complete'}]}}
        payload = answer_payload()
        payload['answers'] = {'present': {'type': 'noul', 'noul': 0.55},
            'grade': {'type': 'score', 'score': 0.5, 'confidence': 0.02,
                      'legend': {'0': {'description': 'Absent'}, '1': {'description': 'Complete'}},
                      'probabilities': {'0': 0.5, '1': 0.5}}}
        owner = OfflineOwner(payload)
        owner.parsed_reply = {'present': {'type': 'noul', 'noul': 0.55},
            'grade': {'type': 'score', 'score': 0.5, 'confidence': 1,
                      'probabilities': {0: 0.5, 1: 0.5}, 'spread_reported': True}}
        result = owner.run(value)
        self.assertEqual(result['status'], 'ok')
        self.assertEqual(result['answers'], payload['answers'])

    def test_v2_malformed_answers_never_become_semantics(self):
        broken = [None, {}, {'pick': None}, {'pick': {'type': 'noul', 'noul': 0.7}},
                  {'pick': {'type': 'choice', 'choice': 'foreign', 'confidence': 0.1,
                            'probabilities': {'read': 0.5, 'hold': 0.5}}},
                  {'pick': {'type': 'choice', 'choice': 'read', 'confidence': None,
                            'probabilities': {'read': 1, 'hold': 0}}},
                  {'pick': {'type': 'choice', 'choice': 'read', 'confidence': 0.1,
                            'probabilities': {'read': 0.1, 'hold': 0.9}}}]
        for answers in broken:
            with self.subTest(answers=answers):
                payload = answer_payload()
                payload['answers'] = answers
                owner = OfflineOwner(payload)
                result = owner.run(request())
                self.assertNotEqual(result['status'], 'ok')
                self.assertEqual(result['answers'], {})
                self.assertLessEqual(len(owner.calls), 1)
                self.assert_no_authority(result)

    def test_v2_wrong_model_and_null_payload_do_not_pass(self):
        payload = answer_payload()
        payload['model'] = 'jev-latest'
        for response in (payload, b'null', b'{invalid', b'{"model":"jev-1.14.0","model":"jev-1.14.0"}'):
            with self.subTest(response=response):
                owner = OfflineOwner(response)
                result = owner.run(request())
                self.assertNotEqual(result['status'], 'ok')
                self.assertEqual(result['answers'], {})
                self.assertLessEqual(len(owner.calls), 1)

    def test_v2_unknown_usage_is_not_fabricated(self):
        payload = answer_payload()
        payload['usage'] = None
        result = OfflineOwner(payload).run(request())
        self.assertEqual(result['usage'], {})

    def test_descriptor_rejection_precedes_import_transport_and_receipts(self):
        owner = OfflineOwner()
        result = owner.run(request(1), descriptor=lambda: {'status': 'stale'})
        self.assertEqual(result['status'], 'held')
        self.assertEqual(result['transportInvocations'], 0)
        self.assertEqual(owner.asks, [])
        self.assertEqual(owner.published, [])

    def test_descriptor_post_drift_clears_answers_without_retry(self):
        owner = OfflineOwner()
        calls = []
        def descriptor():
            calls.append(1)
            return owner.descriptor() if len(calls) == 1 else {'status': 'stale'}
        result = owner.run(request(1), descriptor)
        self.assertNotEqual(result['status'], 'ok')
        self.assertEqual(result['answers'], {})
        self.assertFalse(result['sourcePostconditionVerified'])
        self.assertEqual(len(owner.calls), 1)

    def test_source_post_drift_clears_answers_without_retry(self):
        owner = OfflineOwner()
        owner.drift = True
        result = owner.run(request(1))
        self.assertNotEqual(result['status'], 'ok')
        self.assertEqual(result['answers'], {})
        self.assertEqual(len(owner.calls), 1)

    def test_unknown_transport_and_second_dispatch_do_not_retry(self):
        for mode in ('unknown', 'second'):
            with self.subTest(mode=mode):
                owner = OfflineOwner()
                owner.transport_error = TimeoutError() if mode == 'unknown' else None
                owner.double_transport = mode == 'second'
                result = owner.run(request(1))
                self.assertNotEqual(result['status'], 'ok')
                self.assertEqual(result['transportInvocations'], 1)
                self.assertEqual(len(owner.calls), 1)
                self.assertEqual(result['answers'], {})

    def test_completed_and_unknown_original_receipts_are_returned_without_dispatch(self):
        for status in ('ok', 'error'):
            owner = OfflineOwner()
            owner.existing = {'schema': 'jev.semantic-result.v1', 'status': status,
                              'submittedRequestSha256': hashlib.sha256(raw(request(1))).hexdigest(),
                              'stateDigest': request(1)['stateDigest'], 'originalEffect': 'synthetic-' + status}
            self.assertEqual(owner.run(request(1)), owner.existing)
            self.assertEqual(owner.asks, [])
            self.assertEqual(owner.calls, [])
            self.assertEqual(owner.published, [])

    def test_request_invalid_version_null_nonfinite_duplicate_and_model(self):
        value = request()
        candidates = [b'null', b'{}', raw(value)[:-1] + b',"model":"foreign"}',
                      raw({**value, 'model': 'jev-latest'}), raw({**value, 'schema': 'jev.semantic-request.v3'}),
                      raw({**value, 'imageOptIn': 'true'}), raw({**value, 'consumptionPolicy': None}),
                      raw(value).replace(b'"imageOptIn":false', b'"imageOptIn":NaN')]
        for body in candidates:
            with self.subTest(body=body[:50]):
                with self.assertRaises((caller.Held, ValueError, TypeError)):
                    caller.parse_request(body)

    def test_images_and_box_hold_before_owner_io_until_decoder_is_qualified(self):
        # Tiny synthetic header-like bytes are deliberately not a decoded image.
        # No image acceptance is inferred from metadata, a reference, or opt-in.
        for asset in ({'type': 'image', 'format': 'png', 'data_base64': 'aW52YWxpZA=='},
                      {'type': 'image', 'part': 'image1'},
                      {'type': 'image', 'format': 'png', 'data_base64': '!!!'},
                      {'type': 'image', 'format': 'gif', 'data_base64': 'aW52YWxpZA=='},
                      {'type': 'image', 'format': 'png', 'data_base64': 'aW52YWxpZA==', 'part': 'image1'}):
            with self.subTest(asset=asset):
                value = request()
                value['imageOptIn'] = True
                value['assets'] = {'SYNTHETIC_IMAGE_REF': asset}
                value['state']['facts']['image'] = 'SYNTHETIC_IMAGE_REF'
                value['questions'] = {'present': {'type': 'noul', 'instructions': 'Is target in SYNTHETIC_IMAGE_REF?'},
                                      'region': {'type': 'box', 'instructions': 'Find target in SYNTHETIC_IMAGE_REF'}}
                value['stateDigest'] = caller.common_digest(value['state'])
                owner = OfflineOwner()
                result = owner.run(value)
                self.assertEqual(result['status'], 'held')
                self.assertEqual(result['transportInvocations'], 0)
                self.assertEqual(owner.calls, [])
                self.assertEqual(result['answers'], {})


class ImageContractSubsetTests(unittest.TestCase):
    def image_request(self, multipart=False):
        value = request()
        value['imageOptIn'] = True
        value['state']['facts']['image'] = 'SYNTHETIC_IMAGE_REF'
        body = b'SYNTHETIC_NOT_A_DECODED_IMAGE'
        digest = hashlib.sha256(body).hexdigest()
        import base64
        value['assets'] = {'SYNTHETIC_IMAGE_REF': {'type': 'image', 'part': 'image1'} if multipart else
            {'type': 'image', 'format': 'png', 'data_base64': base64.b64encode(body).decode()}}
        value['state']['directBoundary']['imageEvidence'] = {'SYNTHETIC_IMAGE_REF': {
            'decoded': True, 'still': True, 'format': 'png', 'width': 280, 'height': 280,
            'byteLength': len(body), 'processedByteLength': 100, 'sha256': digest,
            'source': {'path': 'synthetic:image-fixture', 'sha256': digest},
            'evidenceRefs': ['synthetic:metadata-not-decoder-proof']}}
        value['stateDigest'] = caller.common_digest(value['state'])
        return value

    def assert_parse_held(self, value, code):
        value['stateDigest'] = caller.common_digest(value['state'])
        with self.assertRaises(caller.Held) as caught:
            caller.parse_request(raw(value))
        self.assertEqual(str(caught.exception), code)

    def test_structural_image_metadata_never_qualifies_decoder_or_multipart(self):
        for multipart, code in ((False, 'v2_image_decoder_owner_extension_required'),
                                (True, 'v2_multipart_owner_extension_required')):
            value = self.image_request(multipart)
            self.assertEqual(caller.parse_request(raw(value)), value)
            owner = OfflineOwner()
            result = owner.run(value)
            self.assertEqual(result['status'], 'held')
            self.assertEqual(result['errorCode'], code)
            self.assertEqual(result['transportInvocations'], 0)
            self.assertEqual(owner.asks, [])

    def test_image_reference_requires_actual_content_not_keys_or_metadata(self):
        for location in ('key', 'boundary-only'):
            value = self.image_request()
            value['state']['facts'] = {'SYNTHETIC_IMAGE_REF': 'unreferenced'} if location == 'key' else {}
            self.assert_parse_held(value, 'v2_image_not_represented')

    def test_image_opt_in_geometry_and_processed_expanded_limits(self):
        value = self.image_request()
        value['imageOptIn'] = False
        self.assert_parse_held(value, 'v2_experimental_images_opt_in_required')
        for width in (279, 1569):
            value = self.image_request()
            value['state']['directBoundary']['imageEvidence']['SYNTHETIC_IMAGE_REF']['width'] = width
            self.assert_parse_held(value, 'v2_image_owner_geometry_or_bytes')
        for processed, occurrences in ((33554433, 1), (20000000, 2)):
            value = self.image_request()
            value['state']['facts']['image'] = 'SYNTHETIC_IMAGE_REF ' * occurrences
            value['state']['directBoundary']['imageEvidence']['SYNTHETIC_IMAGE_REF']['processedByteLength'] = processed
            self.assert_parse_held(value, 'v2_image_request_bound')

    def test_image_content_hash_mismatch_and_missing_box_presence_binding_hold(self):
        value = self.image_request()
        value['assets']['SYNTHETIC_IMAGE_REF']['data_base64'] = 'aW52YWxpZA=='
        self.assert_parse_held(value, 'v2_asset_bytes_mismatch')
        value = self.image_request()
        value['questions']['region'] = {'type': 'box', 'instructions': 'Find target in SYNTHETIC_IMAGE_REF'}
        self.assert_parse_held(value, 'v2_box_binding_required')


OWNER_CLIENT_ROOT = Path('/docker/hermes-agent-2q3y/data/runtime/harness/jev-1.14/upstream/jevkit')
# These are public source digests, kept separate from credential-like labels.
OWNER_CLIENT_PINS = dict(zip(
    ('images.py', 'keystore.py', 'client.py'),
    ('4a711bb42e9d303a6c65427f4cacf192792afa291ef4cf9ebc73ab17cf563331',
     'a3fdc653a473b4c24cb6cf8b77fcfb24b599e38026650cb4fd28fdc7795d39dc',
     '63915a3925e074b3d314c65bd2eaf4146523c7b3e3692dd0814440c739493f5c'),
))
EVIDENCE_ROOT = Path('/home/palantirkc/jev-1.14-interview-20261021/execution/w03-native-v2-source-20261021/output')


@unittest.skipUnless((OWNER_CLIENT_ROOT / 'client.py').is_file(), 'Exact mounted owner source unavailable')
class ActualPinnedClientOfflineTests(unittest.TestCase):
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.dict(os.environ, {}, clear=True))
        package = types.ModuleType('independent_pinned_jevkit')
        package.__path__ = []
        modules = {'independent_pinned_jevkit': package}
        self.stack.enter_context(patch.dict(sys.modules, modules))
        self.sources = {}
        for filename, expected in OWNER_CLIENT_PINS.items():
            path = OWNER_CLIENT_ROOT / filename
            body = path.read_bytes()
            self.assertEqual(hashlib.sha256(body).hexdigest(), expected)
            name = 'independent_pinned_jevkit.' + path.stem
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            self.stack.enter_context(patch.dict(sys.modules, {name: module}))
            spec.loader.exec_module(module)
            setattr(package, path.stem, module)
            self.sources[path.stem] = module
        self.client = self.sources['client']
        self.keystore = self.sources['keystore']
        for method in ('resolve', 'provider', 'credentials_file', 'credentials_file_for'):
            self.stack.enter_context(patch.object(self.keystore, method,
                side_effect=AssertionError('Real credential lookup forbidden')))
        self.stack.enter_context(patch.object(socket, 'create_connection',
            side_effect=AssertionError('Real socket forbidden')))
        self.stack.enter_context(patch.object(socket.socket, 'connect',
            side_effect=AssertionError('Real socket forbidden')))
        self.stack.enter_context(patch.object(socket.socket, 'connect_ex',
            side_effect=AssertionError('Real socket forbidden')))
        self.stack.enter_context(patch.object(subprocess, 'Popen',
            side_effect=AssertionError('External process forbidden')))
        self.stack.enter_context(patch.object(self.client, '_http_transport',
            side_effect=AssertionError('Uninjected native transport forbidden')))
        self.calls = []

    def ask(self, questions, payload, **kwargs):
        def send(body, headers, timeout):
            self.calls.append({'body': json.loads(body),
                               'imageOptIn': headers.get('TypeSafe-Danger-Zone-Images')})
            if isinstance(payload, Exception):
                raise payload
            return raw(payload)
        return self.client.ask({'facts': 'Synthetic source; SYNTHETIC_IMAGE_REF'}, questions,
            api_key='SYNTHETIC_OFFLINE_KEY_ONLY', provider='typesafe',
            model=caller.MODEL, transport=send, retries=0, **kwargs)

    def score_question(self, structured=True):
        return {'grade': {'type': 'score', 'instructions': 'How complete is the source?',
            'criteria': [{'description': 'Absent'}, {'description': 'Complete'}]
                        if structured else ['Absent', 'Complete']}}

    def score_payload(self, structured=True):
        criteria = self.score_question(structured)['grade']['criteria']
        return {'model': 'jev-1.14.0', 'usage': {'input_tokens': 7, 'output_tokens': 3},
            'answers': {'grade': {'type': 'score', 'score': 0.5, 'confidence': 0.02,
                'probabilities': {'0': 0.5, '1': 0.5},
                'legend': {str(i): value for i, value in enumerate(criteria)}}}}

    def test_actual_client_choice_low_confidence_tie_and_explicit_key(self):
        result = self.ask(request()['questions'], answer_payload())
        self.assertEqual(result['answers']['pick']['choice'], 'read')
        self.assertEqual(result['answers']['pick']['confidence'], 0.01)
        self.assertEqual(len(self.calls), 1)

    def test_actual_client_score_transforms_keys_drops_structured_legend(self):
        result = self.ask(self.score_question(), self.score_payload())
        answer = result['answers']['grade']
        self.assertEqual(answer['probabilities'], {0: 0.5, 1: 0.5})
        self.assertTrue(answer['spread_reported'])
        self.assertNotIn('legend', answer)
        self.assertEqual(answer['confidence'], 0.02)

    def test_actual_client_missing_score_confidence_defaults_but_wrapper_refuses(self):
        payload = self.score_payload()
        del payload['answers']['grade']['confidence']
        parsed = self.ask(self.score_question(), payload)
        self.assertEqual(parsed['answers']['grade']['confidence'], 1.0)
        value = request()
        value['questions'] = self.score_question()
        owner = OfflineOwner(payload)
        owner.actual_client = self.client
        owner.client_bytes = (OWNER_CLIENT_ROOT / 'client.py').read_bytes()
        result = owner.run(value)
        self.assertEqual(result['status'], 'error')
        self.assertEqual(result['answers'], {})
        self.assertEqual(result['transportInvocations'], 1)
        self.assertEqual(result['errorCode'], 'v2_score_fields')

    def test_actual_client_wrapper_retains_raw_structured_score(self):
        value = request()
        value['questions'] = self.score_question()
        payload = self.score_payload()
        owner = OfflineOwner(payload)
        owner.actual_client = self.client
        owner.client_bytes = (OWNER_CLIENT_ROOT / 'client.py').read_bytes()
        result = owner.run(value)
        self.assertEqual(result['status'], 'ok', result.get('errorCode'))
        self.assertEqual(result['answers'], payload['answers'])
        self.assertEqual(result['answers']['grade']['probabilities'], {'0': 0.5, '1': 0.5})
        self.assertEqual(result['answers']['grade']['legend']['0'], {'description': 'Absent'})
        self.assertNotIn('spread_reported', result['answers']['grade'])
        self.assertEqual(result['usage'], payload['usage'])

    @unittest.skipUnless((EVIDENCE_ROOT / 'independent.common-fixtures.json').is_file(),
                         'Generated common request evidence unavailable')
    def test_actual_client_and_caller_accept_common_generated_request(self):
        generated = json.loads((EVIDENCE_ROOT / 'independent.common-fixtures.json').read_bytes())['generated']
        value = generated['request']
        payload = self.score_payload()
        payload['answers']['pick'] = answer_payload()['answers']['pick']
        for name, q in value['questions'].items():
            if q['type'] == 'noul':
                payload['answers'][name] = {'type': 'noul', 'noul': 0.75}
        owner = OfflineOwner(payload)
        owner.actual_client = self.client
        owner.client_bytes = (OWNER_CLIENT_ROOT / 'client.py').read_bytes()
        result = owner.run(value)
        self.assertEqual(result['status'], 'ok', result.get('errorCode'))
        self.assertEqual(result['answers'], payload['answers'])
        self.assertEqual(result['requestDigest'], generated['requestDigest'])
        self.assertEqual(result['transportInvocations'], 1)
        self.assertEqual(result['connectionLeases'], 1)

    def test_actual_client_string_score_legend_uses_integer_keys(self):
        parsed = self.ask(self.score_question(False), self.score_payload(False))
        self.assertEqual(parsed['answers']['grade']['legend'], {0: 'Absent', 1: 'Complete'})

    def test_actual_client_box_null_confidence_is_unsupported(self):
        questions = {'region': {'type': 'box', 'instructions': 'Find source region in SYNTHETIC_IMAGE_REF'}}
        payload = {'model': 'jev-1.14.0', 'answers': {'region': {'type': 'box',
            'box': {'left': 0.1, 'right': 0.5, 'top': 0.1, 'bottom': 0.5}, 'confidence': None}}}
        with self.assertRaises(self.client.JevError):
            self.ask(questions, payload)
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(self.calls[0]['imageOptIn'], 'true')

    def test_actual_client_base64_plumbing_does_not_decode_content_and_multipart_refuses(self):
        payload = answer_payload()
        asset = {'SYNTHETIC_IMAGE_REF': {'type': 'image', 'format': 'png', 'data_base64': '!!!'}}
        self.ask(request()['questions'], payload, assets=asset)
        self.assertEqual(self.calls[0]['body']['assets'], asset)
        self.assertEqual(self.calls[0]['imageOptIn'], 'true')
        self.calls.clear()
        with self.assertRaises(ValueError):
            self.ask(request()['questions'], payload,
                assets={'SYNTHETIC_IMAGE_REF': {'type': 'image', 'part': 'image1'}})
        self.assertEqual(self.calls, [])

    def test_actual_client_retry_zero_keeps_unknown_transport_one_attempt(self):
        with self.assertRaises(self.client.JevError):
            self.ask(request()['questions'], self.client.JevError('network'))
        self.assertEqual(len(self.calls), 1)

    def test_actual_client_single_level_score_and_choice_refuse_pretransport(self):
        for question in ({'type': 'score', 'instructions': 'How complete?', 'criteria': ['Complete']},
                         {'type': 'choice', 'instructions': 'Which source?', 'criteria': {'read': 'Read'}}):
            with self.subTest(question=question):
                with self.assertRaises(ValueError):
                    self.ask({'q': question}, answer_payload())
                self.assertEqual(self.calls, [])


@unittest.skipUnless((EVIDENCE_ROOT / 'independent.common-fixtures.json').is_file(),
                     'Generated exact common-source fixture evidence unavailable')
class CommonGeneratedFixtureTests(unittest.TestCase):
    def test_generated_prepare_direct_request_and_digests_match_actual_python(self):
        data = json.loads((EVIDENCE_ROOT / 'independent.common-fixtures.json').read_bytes())
        generated = data['generated']
        value = generated['request']
        self.assertEqual(caller.parse_request(raw(value)), value)
        self.assertEqual(caller.common_digest(value['state']), value['stateDigest'])
        self.assertEqual(caller.common_digest(value), generated['requestDigest'])
        self.assertIn('directBoundary', value['state'])

    @unittest.skipUnless((EVIDENCE_ROOT / 'independent.final-roundtrip.ts').is_file(),
                         'Final common-normalizer harness unavailable')
    def test_generated_common_request_actual_client_python_result_common_normalizer_roundtrip(self):
        generated = json.loads((EVIDENCE_ROOT / 'independent.common-fixtures.json').read_bytes())['generated']
        value = generated['request']
        actual = ActualPinnedClientOfflineTests('test_actual_client_and_caller_accept_common_generated_request')
        actual.setUp()
        try:
            payload = actual.score_payload()
            payload['answers']['pick'] = answer_payload()['answers']['pick']
            for name, q in value['questions'].items():
                if q['type'] == 'noul':
                    payload['answers'][name] = {'type': 'noul', 'noul': 0.75}
            owner = OfflineOwner(payload)
            owner.actual_client = actual.client
            owner.client_bytes = (OWNER_CLIENT_ROOT / 'client.py').read_bytes()
            result = owner.run(value)
            self.assertEqual(result['status'], 'ok', result.get('errorCode'))
            self.assertEqual(result['answers'], payload['answers'])
        finally:
            actual.doCleanups()
        completed = subprocess.run(['bun', str(EVIDENCE_ROOT / 'independent.final-roundtrip.ts')],
            input=raw({'request': value, 'result': result}), capture_output=True, check=False)
        self.assertEqual(completed.returncode, 0, completed.stderr.decode()[:1000])
        normalized = json.loads(completed.stdout)
        self.assertTrue(normalized['actualPreparedRequestEqual'])
        self.assertEqual(normalized['requestDigest'], generated['requestDigest'])
        self.assertEqual(normalized['normalized']['answers'], result['answers'])
        self.assertIs(normalized['executionAuthorityGranted'], False)
        type(self).final_roundtrip_observation = normalized

    def test_javascript_python_canonical_numeric_unicode_and_integer_key_corpus(self):
        import v2_contract
        data = json.loads((EVIDENCE_ROOT / 'independent.common-fixtures.json').read_bytes())
        self.assertEqual(len(data['fixtures']), 280)
        for case in data['fixtures']:
            with self.subTest(case=case['id']):
                self.assertEqual(v2_contract.canonical_json(case['value']), case['canonicalJson'])
                self.assertEqual(v2_contract.digest(case['value']), case['semanticDigest'])


class IndependentAccountingContinuationTests(unittest.TestCase):
    """Actual ExistingLedger, exclusively synthetic temporary existing ledgers."""
    def setUp(self):
        import shared_accounting
        self.accounting = shared_accounting
        self.temp = tempfile.TemporaryDirectory(prefix='independent-accounting-synthetic-')
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'synthetic-existing-ledger.json'
        self.legacy = [{'localCorrelationId': 'synthetic-historical-1',
            'state': 'reconciled-historical', 'requestSha256': '1' * 64, 'reservedUsd': 0.5},
            {'localCorrelationId': 'synthetic-historical-2', 'state': 'reconciled-historical',
             'requestSha256': '2' * 64, 'reservedUsd': 0.5}]
        self.initial = {'schema': 'host-orchestrator.jev-task-accounting.v1',
            'capUsd': 14, 'currency': 'USD', 'knownEstimatedSpendUsd': 0.2,
            'unreconciledReserveUsd': 1.0, 'inferenceRequestsDispatched': 2,
            'providerReportedCostUsd': None, 'calls': self.legacy,
            'admissionOwner': 'synthetic-historical-owner'}
        self.path.write_bytes(raw(self.initial))
        self.owner = shared_accounting.ExistingLedger(self.path)
        self.value = request()
        self.sha = hashlib.sha256(raw(self.value)).hexdigest()
        self.metadata = {'requestSchema': 'jev.semantic-request.v2',
            'resultSchema': 'jev.semantic-result.v2', 'consumptionPolicy': self.value['consumptionPolicy'],
            'requestDigest': caller.common_digest(self.value)}

    def ledger(self):
        return json.loads(self.path.read_bytes())

    def reserve_args(self, version=2):
        value = self.value if version == 2 else request(1)
        return {'request_id': value['requestId'], 'request_sha256': hashlib.sha256(raw(value)).hexdigest(),
            'state_digest': value['stateDigest'], 'purpose': value['purpose'], 'scope': value['scope'],
            'receipt_ref': 'synthetic:immutable-result', 'pricing': {'model': caller.MODEL,
                'inputUsdPerToken': 0.000000042, 'outputUsdPerToken': 0, 'evidenceSha256': 'a' * 64},
            **({'semantic_contract': copy.deepcopy(self.metadata)} if version == 2 else {})}

    def reconcile(self, result):
        return self.owner.reconcile(request_id=self.value['requestId'], request_sha256=self.sha, result=result)

    def test_v2_actual_imported_result_same_owner_pending_binding_idempotence(self):
        admitted = self.owner.reserve(**self.reserve_args())
        self.assertTrue(admitted['admitted'])
        pending = self.owner.pending()['pending'][0]
        self.assertEqual(pending['semanticContract'], self.metadata)
        self.assertEqual(pending['requestSha256'], self.sha)
        self.assertEqual(pending['stateDigest'], self.value['stateDigest'])
        fixture = OfflineOwner()
        result = fixture.run(self.value)
        self.assertEqual(result['status'], 'ok')
        self.reconcile(result)
        after = self.path.read_bytes()
        self.assertTrue(self.reconcile(result)['alreadyReconciled'])
        self.assertFalse(self.owner.reserve(**self.reserve_args())['admitted'])
        self.assertEqual(self.path.read_bytes(), after)
        current = self.ledger()
        self.assertEqual(current['calls'][:2], self.legacy)
        self.assertEqual(current['inferenceRequestsDispatched'], 3)
        self.assertEqual(current['unreconciledReserveUsd'], 1.01)
        self.assertEqual(current['capUsd'], 14)
        self.assertEqual(len(fixture.calls), 1)

    def test_v1_default_no_upgrade_and_v2_no_downgrade(self):
        self.owner.reserve(**self.reserve_args(1))
        original = self.path.read_bytes()
        self.assertNotIn('semanticContract', self.ledger()['calls'][-1])
        v2 = OfflineOwner().run(self.value)
        with self.assertRaises(self.accounting.AccountingHeld):
            self.owner.reconcile(request_id=request(1)['requestId'],
                request_sha256=hashlib.sha256(raw(request(1))).hexdigest(),
                result={**v2, 'submittedRequestSha256': hashlib.sha256(raw(request(1))).hexdigest(),
                        'stateDigest': request(1)['stateDigest']})
        self.assertEqual(self.path.read_bytes(), original)
        with self.assertRaisesRegex(self.accounting.AccountingHeld, 'request_contract_rebound_do_not_retry'):
            self.owner.reserve(**{**self.reserve_args(1), 'semantic_contract': self.metadata})
        self.assertEqual(self.path.read_bytes(), original)
        self.path.write_bytes(raw(self.initial))
        self.owner.reserve(**self.reserve_args())
        original = self.path.read_bytes()
        with self.assertRaises(self.accounting.AccountingHeld):
            self.reconcile({**v2, 'schema': 'jev.semantic-result.v1'})
        self.assertEqual(self.path.read_bytes(), original)

    def test_original_raw_sha_state_canonical_policy_version_and_authority_mismatch_hold(self):
        self.owner.reserve(**self.reserve_args())
        valid = OfflineOwner().run(self.value)
        original = self.path.read_bytes()
        for update in ({'submittedRequestSha256': 'e' * 64}, {'stateDigest': 'e' * 64},
                       {'requestDigest': 'e' * 64}, {'consumptionPolicy': 'other'},
                       {'schema': 'jev.semantic-result.v1'}, {'requestId': 'other'},
                       {'purpose': 'other'}, {'requestedModel': 'jev-latest'},
                       {'returnedModel': 'jev-latest'}, {'sourcePostconditionVerified': False},
                       {'transportInvocations': True}, {'connectionLeases': True},
                       {'transportInvocations': 2, 'connectionLeases': 2},
                       {'transportInvocations': 0, 'connectionLeases': 0},
                       {'executionAuthorityGranted': True}, {'automaticWrite': True}):
            with self.subTest(update=update), self.assertRaises(self.accounting.AccountingHeld):
                self.reconcile({**valid, **update})
            self.assertEqual(self.path.read_bytes(), original)
        # Equivalent canonical JSON never substitutes for the exact submitted raw SHA.
        alternative = json.dumps(self.value, indent=2).encode()
        self.assertNotEqual(hashlib.sha256(alternative).hexdigest(), self.sha)
        self.assertEqual(caller.common_digest(json.loads(alternative)), self.metadata['requestDigest'])
        with self.assertRaises(self.accounting.AccountingHeld):
            self.reconcile({**valid, 'submittedRequestSha256': hashlib.sha256(alternative).hexdigest()})
        self.assertEqual(self.path.read_bytes(), original)

    def test_unknown_keeps_reserve_zero_http_releases_only_original_and_no_repeat(self):
        self.owner.reserve(**self.reserve_args())
        self.owner.mark_unknown(request_id=self.value['requestId'], request_sha256=self.sha)
        original = self.path.read_bytes()
        self.assertFalse(self.owner.reserve(**self.reserve_args())['admitted'])
        self.assertEqual(self.path.read_bytes(), original)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.01)
        fixture = OfflineOwner()
        fixture.transport_error = TimeoutError()
        result = fixture.run(self.value)
        self.assertEqual(result['status'], 'error')
        self.reconcile(result)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.01)
        self.assertEqual(self.ledger()['inferenceRequestsDispatched'], 3)
        self.assertTrue(self.reconcile(result)['alreadyReconciled'])
        self.assertFalse(self.owner.reserve(**self.reserve_args())['admitted'])
        self.assertEqual(len(fixture.calls), 1)
        self.path.write_bytes(raw(self.initial))
        self.owner.reserve(**self.reserve_args())
        fixture = OfflineOwner()
        zero = fixture.run(self.value, descriptor=lambda: {'status': 'stale'})
        self.assertEqual(zero['transportInvocations'], 0)
        self.reconcile(zero)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1)
        self.assertEqual(self.ledger()['inferenceRequestsDispatched'], 2)
        after = self.path.read_bytes()
        self.assertTrue(self.reconcile(zero)['alreadyReconciled'])
        self.assertFalse(self.owner.reserve(**self.reserve_args())['admitted'])
        self.assertEqual(self.path.read_bytes(), after)
        self.assertEqual(fixture.calls, [])

    def test_same_usd14_flock_pricing_and_inflight_limits_preserved(self):
        original = self.path.read_bytes()
        with self.owner._locked():
            bounded = self.accounting.ExistingLedger(self.path, lock_timeout=0.02)
            self.assertEqual(bounded.lock_path, self.owner.lock_path)
            with self.assertRaisesRegex(self.accounting.AccountingHeld, 'ledger_owner_busy_no_dispatch'):
                bounded.reserve(**self.reserve_args())
        self.assertEqual(self.path.read_bytes(), original)
        high = {**self.initial, 'unreconciledReserveUsd': 13.799}
        self.path.write_bytes(raw(high))
        original = self.path.read_bytes()
        with self.assertRaisesRegex(self.accounting.AccountingHeld, 'cumulative_cap_exceeded'):
            self.owner.reserve(**self.reserve_args())
        self.assertEqual(self.path.read_bytes(), original)
        self.path.write_bytes(raw(self.initial))
        original = self.path.read_bytes()
        with self.assertRaisesRegex(self.accounting.AccountingHeld, 'reserve_below_pricing_byte_bound'):
            self.owner.reserve(**{**self.reserve_args(), 'reserve_usd': 0.001})
        self.assertEqual(self.path.read_bytes(), original)
        for index in range(4):
            self.owner.reserve(**{**self.reserve_args(), 'request_id': 'synthetic-inflight-' + str(index),
                'request_sha256': hashlib.sha256(str(index).encode()).hexdigest()})
        original = self.path.read_bytes()
        with self.assertRaisesRegex(self.accounting.AccountingHeld, 'in_flight_limit_no_dispatch'):
            self.owner.reserve(**self.reserve_args())
        self.assertEqual(self.path.read_bytes(), original)

    def test_semantic_contract_metadata_invalid_never_inferred_or_persisted(self):
        original = self.path.read_bytes()
        for metadata in ({}, {**self.metadata, 'requestDigest': None},
                         {**self.metadata, 'requestSchema': 'jev.semantic-request.v1'},
                         {**self.metadata, 'consumptionPolicy': 'other'},
                         {**self.metadata, 'extra': True}):
            with self.subTest(metadata=metadata), self.assertRaises(self.accounting.AccountingHeld):
                self.owner.reserve(**{**self.reserve_args(), 'semantic_contract': metadata})
            self.assertEqual(self.path.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
