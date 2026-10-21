"""Actual common-produced requests, current parser/SDK and temporary ledger only.

Invoked by the common TypeScript test. No installed host execution or real I/O.
The existing pinned-client fixture denies sockets, real keystore operations and
external processes, and injects synthetic config/transport before caller.run().
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
import tempfile
import types
from contextlib import ExitStack, contextmanager
from unittest.mock import patch


def deny_mounted_source_reads(event, args):
    if event == 'open' and isinstance(args[0], (str, bytes)):
        path = args[0].decode() if isinstance(args[0], bytes) else args[0]
        if path.startswith('/docker/'):
            raise AssertionError('Default finite CI must not read mounted SDK source')


if __name__ == '__main__' and sys.argv[1:] != ['--verify-sdk-origin']:
    sys.addaudithook(deny_mounted_source_reads)

import semantic_caller as caller
from shared_accounting import ExistingLedger
from test_independent_v2 import OfflineOwner, OWNER_CLIENT_ROOT, OWNER_CLIENT_PINS, raw
from test_shared_accounting import PRICING

SDK_FIXTURE_ROOT = Path(__file__).resolve().parent / 'fixtures/pinned_jevkit'
ADAPTER_FIXTURE = SDK_FIXTURE_ROOT.parent / 'native-caller.py'
ADAPTER_ORIGINAL = OWNER_CLIENT_ROOT.parent.parent / 'native-caller.py'


def verify_native_adapter_source(*, original=False):
    manifest = json.loads(ADAPTER_FIXTURE.with_suffix('.pin.json').read_bytes())
    body = ADAPTER_FIXTURE.read_bytes()
    expected = 'dd72953016d12fdc730235dece88e635b58573042544b77e890b5d64566af71d'
    assert manifest['sha256'] == hashlib.sha256(body).hexdigest() == expected
    assert manifest['bytes'] == len(body)
    assert manifest['originalSource'] == str(ADAPTER_ORIGINAL)
    if original:
        assert ADAPTER_ORIGINAL.read_bytes() == body, 'Native adapter original/copy mismatch'
    return {'file': ADAPTER_FIXTURE.name, 'sha256': expected, 'bytes': len(body),
            'originalSource': str(ADAPTER_ORIGINAL)}


def verify_sdk_sources(*, original=False):
    manifest = json.loads((SDK_FIXTURE_ROOT / 'source-pins.json').read_bytes())
    pins = {row['path']: row['sha256'] for row in manifest['files']}
    assert pins == OWNER_CLIENT_PINS, 'SDK fixture pin denominator changed'
    rows = []
    for name, expected in OWNER_CLIENT_PINS.items():
        body = (SDK_FIXTURE_ROOT / name).read_bytes()
        assert hashlib.sha256(body).hexdigest() == expected, 'SDK fixture changed: ' + name
        if original:
            # Explicit required original closure command, never a conditional skip.
            assert (OWNER_CLIENT_ROOT / name).read_bytes() == body, 'SDK original/copy mismatch'
        rows.append({'file': name, 'sha256': expected, 'bytes': len(body)})
    return rows


@contextmanager
def pinned_sdk():
    """Load exact immutable SDK copies, denying every real transport/credential route."""
    verify_sdk_sources()
    with ExitStack() as stack:
        stack.enter_context(patch.dict(os.environ, {}, clear=True))
        package = types.ModuleType('w05_pinned_jevkit')
        package.__path__ = []
        stack.enter_context(patch.dict(sys.modules, {'w05_pinned_jevkit': package}))
        for name in OWNER_CLIENT_PINS:
            source = SDK_FIXTURE_ROOT / name
            module_name = 'w05_pinned_jevkit.' + source.stem
            spec = importlib.util.spec_from_file_location(module_name, source)
            module = importlib.util.module_from_spec(spec)
            stack.enter_context(patch.dict(sys.modules, {module_name: module}))
            spec.loader.exec_module(module)
            setattr(package, source.stem, module)
        for method in ('resolve', 'provider', 'credentials_file', 'credentials_file_for'):
            stack.enter_context(patch.object(package.keystore, method,
                side_effect=AssertionError('Real credential lookup forbidden')))
        for target, name in ((socket, 'create_connection'), (socket.socket, 'connect'),
                             (socket.socket, 'connect_ex'), (subprocess, 'Popen')):
            stack.enter_context(patch.object(target, name,
                side_effect=AssertionError('Real socket/external process forbidden')))
        stack.enter_context(patch.object(package.client, '_http_transport',
            side_effect=AssertionError('Uninjected native transport forbidden')))
        yield package.client


def parse(values):
    rows = []
    for value in values:
        try:
            parsed = caller.parse_request(raw(value))
            rows.append({'status': 'parsed', 'requestDigest': caller.common_digest(parsed)})
        except caller.Held as error:
            rows.append({'status': 'held', 'errorCode': str(error)})
    return rows


def roundtrip(values):
    # Portable mandatory path: real byte-exact SDK sources in finite source QA.
    verify_sdk_sources()
    verify_native_adapter_source()
    rows = []
    with tempfile.TemporaryDirectory(prefix='w05-common-offline-') as folder:
        path = Path(folder) / 'synthetic-existing-ledger.json'
        historical = {'localCorrelationId': 'synthetic-historical',
                      'state': 'reconciled-historical', 'requestSha256': '1' * 64}
        initial = {'schema': 'host-orchestrator.jev-task-accounting.v1',
                   'capUsd': 14, 'currency': 'USD', 'providerReportedCostUsd': None,
                   'knownEstimatedSpendUsd': 0.2, 'unreconciledReserveUsd': 1,
                   'inferenceRequestsDispatched': 1, 'calls': [historical],
                   'admissionOwner': 'synthetic-historical-owner'}
        path.write_bytes(raw(initial))
        ledger = ExistingLedger(path)
        for value in values:
            assert caller.parse_request(raw(value)) == value
            arguments = {
                'request_id': value['requestId'],
                'request_sha256': hashlib.sha256(raw(value)).hexdigest(),
                'state_digest': value['stateDigest'], 'purpose': value['purpose'],
                'scope': value['scope'], 'receipt_ref': 'synthetic:immutable-result',
                'pricing': PRICING,
            }
            if value['schema'] == 'jev.semantic-request.v2':
                arguments['semantic_contract'] = {
                    'requestSchema': value['schema'], 'resultSchema': 'jev.semantic-result.v2',
                    'consumptionPolicy': value['consumptionPolicy'],
                    'requestDigest': caller.common_digest(value),
                }
            assert ledger.reserve(**arguments)['admitted']
            payload = {'model': 'jev-1.14.0', 'usage': {'input_tokens': 7, 'output_tokens': 3},
                       'answers': {}}
            for name, question in value['questions'].items():
                if question['type'] == 'noul':
                    payload['answers'][name] = {'type': 'noul', 'noul': 0.4}
                else:
                    keys = list(question['criteria'])
                    payload['answers'][name] = {
                        'type': 'choice', 'choice': 'review' if 'review' in keys else keys[0], 'confidence': 0.01,
                        'probabilities': {key: 1 / len(keys) for key in keys},
                    }
            with pinned_sdk() as client:
                owner = OfflineOwner(payload)
                owner.actual_client = client
                owner.client_bytes = (SDK_FIXTURE_ROOT / 'client.py').read_bytes()
                result = owner.run(value)
                assert result['status'] == 'ok', result
                assert (result['transportInvocations'], result['connectionLeases']) == (1, 1)
                assert result['answers'] == payload['answers']
                assert len(owner.calls) == 1
                assert owner.calls[0]['body']['questions'] == value['questions']
                assert owner.calls[0]['body']['state']['scope'] == value['scope']
                # Recover the exact original result; do not issue a second request.
                owner.existing = copy.deepcopy(result)
                assert owner.run(value) == result
                assert len(owner.calls) == 1
            reconcile = {'request_id': value['requestId'],
                         'request_sha256': arguments['request_sha256'], 'result': result}
            ledger.reconcile(**reconcile)
            settled = path.read_bytes()
            assert ledger.reconcile(**reconcile)['alreadyReconciled']
            assert not ledger.reserve(**arguments)['admitted']
            assert path.read_bytes() == settled
            current = json.loads(settled)
            assert current['calls'][0] == historical
            assert current['calls'][-1].get('semanticContract') == arguments.get('semantic_contract')
            assert current['unreconciledReserveUsd'] == 1 + 0.01 * (len(rows) + 1)
            assert current['inferenceRequestsDispatched'] == len(rows) + 2
            assert current['capUsd'] == 14
            rows.append(result)
    return rows


if __name__ == '__main__' and len(sys.argv) == 2:
    if sys.argv[1] == '--verify-sdk-origin':
        print(json.dumps({'schema': 'jev.w05.actual-sdk-origin-proof.v1',
                          'originalRoot': str(OWNER_CLIENT_ROOT),
                          'fixtureRoot': str(SDK_FIXTURE_ROOT),
                          'files': verify_sdk_sources(original=True),
                          'nativeAdapter': verify_native_adapter_source(original=True),
                          'status': 'exact-full-source-closure-match',
                          'providerRequests': 0}))
    else:
        operation = {'--parse': parse, '--roundtrip': roundtrip}[sys.argv[1]]
        print(json.dumps(operation(json.load(sys.stdin)), allow_nan=False))
