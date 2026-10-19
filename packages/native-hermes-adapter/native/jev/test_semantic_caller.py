"""Finite wire validation without native imports, credentials, or inference."""
import copy
import json
import os
import unittest
from unittest.mock import patch

from semantic_caller import CALLER_SHA, MANIFEST_SHA, Held, MODEL, parse_request, run


def request():
    context = {'currentAuthority': {'text': 'Synthetic test', 'sourceRefs': ['synthetic']},
               'wholeObjective': 'Whole synthetic purpose', 'taskGoal': 'Read source',
               'acceptedWhy': [{'text': 'Preserve meaning', 'attribution': 'current-user', 'sourceRefs': ['synthetic']}],
               'acceptedDecisions': [], 'owners': [{'role': 'native', 'id': 'synthetic'}],
               'dependencies': [], 'completedEffectRefs': [], 'uncertainEffectRefs': [],
               'remainingAcceptance': ['Read exact original source']}
    return {'schema': 'jev.semantic-request.v1', 'requestId': 'synthetic-1', 'model': MODEL,
            'purpose': 'semantic_bundle', 'stateDigest': 'a' * 64,
            'state': {'requiredContext': context, 'binding': {'nativeTaskId': None}, 'facts': {'progress': 0.5}},
            'questions': {'sufficient': {'type': 'noul', 'instructions': 'Are required outcomes supported?'},
                          'next': {'type': 'choice', 'instructions': 'Which offered read?',
                                   'criteria': {'read': 'Read exact source', '__abstain': 'No supported candidate'}}},
            'scope': {'sessionId': 'SYNTHETIC_NOT_NATIVE_SESSION', 'agentId': None,
                      'agentType': None, 'taskGoal': 'Read source', 'wholeObjective': 'Whole purpose',
                      'why': 'Preserve meaning', 'owners': ['native:synthetic']}}


class SemanticWireTests(unittest.TestCase):
    def test_accepts_fractional_state_and_opaque_host_digest(self):
        value = request()
        self.assertEqual(parse_request(json.dumps(value).encode()), value)

    def test_missing_whole_lens_and_unknown_fields_hold(self):
        for change in ('why', 'capsule', 'extra', 'model', 'purpose'):
            value = request()
            if change == 'why':
                value['scope']['why'] = ''
            elif change == 'capsule':
                del value['state']['requiredContext']['uncertainEffectRefs']
            elif change == 'extra':
                value['arbitraryExecution'] = True
            else:
                value[change] = 'other'
            with self.assertRaises(Held):
                parse_request(json.dumps(value).encode())

    def test_duplicate_fields_nonfinite_and_bounds_hold(self):
        value = request()
        raw = json.dumps(value).encode()
        with self.assertRaises(Held):
            parse_request(raw[:-1] + b',"purpose":"semantic_bundle"}')
        with self.assertRaises((Held, ValueError)):
            parse_request(raw.replace(b'0.5', b'1e999'))
        with self.assertRaises(Held):
            parse_request(b' ' * 65537)
        value['questions'] = {str(i): copy.deepcopy(value['questions']['next']) for i in range(25)}
        with self.assertRaises(Held):
            parse_request(json.dumps(value).encode())
        value = request()
        value['questions']['next']['criteria'] = {str(i): 'candidate' for i in range(33)}
        with self.assertRaises(Held):
            parse_request(json.dumps(value).encode())

    def test_native_precondition_failure_returns_zero_and_no_action(self):
        # Standalone transport has no admitted descriptor verifier and must hold
        # before any native import, credential lookup or HTTP dispatch.
        result = run(json.dumps(request()).encode())
        self.assertEqual(result['status'], 'held')
        self.assertEqual(result['errorCode'], 'descriptor_verifier_required')
        self.assertEqual(result['transportInvocations'], 0)
        self.assertFalse(result['executionAuthorityGranted'])
        self.assertFalse(result['automaticWrite'])
        self.assertEqual(result['answers'], {})

    def test_descriptor_failure_precedes_native_source_import(self):
        calls = []

        def verifier():
            calls.append('descriptor')
            raise Held('synthetic_descriptor_refused')

        with patch('semantic_caller.os.getuid', return_value=10000), \
                patch('semantic_caller.os.getgid', return_value=10000), \
                patch.dict(os.environ, {'HOME': '/opt/data', 'HERMES_HOME': '/opt/data'}):
            result = run(json.dumps(request()).encode(), descriptor_verify=verifier)
        self.assertEqual(calls, ['descriptor'])
        self.assertEqual(result['transportInvocations'], 0)
        self.assertEqual(result['errorCode'], 'synthetic_descriptor_refused')
        self.assertFalse(result['sourcePostconditionVerified'])

    def test_fresh_descriptor_postflight_runs_after_later_preflight_error(self):
        calls = []
        evidence = {'status': 'successor-source-verified', 'callerSha256': CALLER_SHA,
                    'manifestSha256': MANIFEST_SHA}

        def verifier():
            calls.append('descriptor')
            return evidence

        # The technical receipt owner is mocked before any native path access.
        # The admitted callback still runs again after that later preflight error.
        with patch('semantic_caller.os.getuid', return_value=10000), \
                patch('semantic_caller.os.getgid', return_value=10000), \
                patch('semantic_caller.NativeResultReceipts', side_effect=Held('synthetic_receipt_owner_held')), \
                patch.dict(os.environ, {'HOME': '/opt/data', 'HERMES_HOME': '/opt/data'}):
            result = run(json.dumps(request()).encode(), descriptor_verify=verifier)
        self.assertEqual(calls, ['descriptor', 'descriptor'])
        self.assertEqual(result['transportInvocations'], 0)
        self.assertFalse(result['sourcePostconditionVerified'])
        self.assertEqual(result['descriptorSourceAfter'], evidence)


if __name__ == '__main__':
    unittest.main()
