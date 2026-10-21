"""Isolated fake-ledger races and failure boundaries; never touch host accounting."""
import copy
import fcntl
import hashlib
import json
import multiprocessing
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from shared_accounting import AccountingHeld, DIRECT_POLICY, ExistingLedger, MODEL, OWNER

PRICING = {'model': MODEL, 'inputUsdPerToken': 0.000000042,
           'outputUsdPerToken': 0, 'evidenceSha256': 'a' * 64}
SCOPE = {'sessionId': 'SYNTHETIC_SESSION', 'agentId': None, 'agentType': 'worker'}


def arguments(index=1, **updates):
    result = {'request_id': f'synthetic-{index}', 'request_sha256': f'{index:064x}',
              'state_digest': 'b' * 64, 'purpose': 'semantic_bundle', 'scope': SCOPE,
              'receipt_ref': f'synthetic-{index}.receipt.json', 'pricing': PRICING}
    result.update(updates)
    return result


def response(index=1, count=1):
    return {'schema': 'jev.semantic-result.v1', 'requestId': f'synthetic-{index}',
            'submittedRequestSha256': f'{index:064x}', 'stateDigest': 'b' * 64,
            'requestedModel': MODEL, 'returnedModel': MODEL, 'status': 'ok',
            'sourcePostconditionVerified': True, 'transportInvocations': count}


def v2_contract(**updates):
    return {'requestSchema': 'jev.semantic-request.v2', 'resultSchema': 'jev.semantic-result.v2',
            'consumptionPolicy': DIRECT_POLICY, 'requestDigest': 'c' * 64, **updates}


def v2_response(index=1, count=1, **updates):
    return {**response(index, count), 'schema': 'jev.semantic-result.v2',
            'consumptionPolicy': DIRECT_POLICY, 'requestDigest': 'c' * 64,
            'purpose': 'semantic_bundle', 'connectionLeases': count,
            'executionAuthorityGranted': False, 'automaticWrite': False, **updates}


def concurrent_reserve(path, index, barrier, output):
    try:
        barrier.wait(timeout=5)
        output.put(ExistingLedger(path).reserve(**arguments(index))['admitted'])
    except Exception as error:
        output.put(type(error).__name__)


def concurrent_reconcile(path, index, barrier, output):
    try:
        barrier.wait(timeout=5)
        output.put(ExistingLedger(path).reconcile(request_id=f'synthetic-{index}',
                   request_sha256=f'{index:064x}', result=response(index))['state'])
    except Exception as error:
        output.put(type(error).__name__)


class AccountingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / 'existing.json'
        self.legacy = [{'localCorrelationId': f'historical-{i}', 'state': 'reconciled-historical',
                        'requestSha256': f'{100+i:064x}', 'reservedUsd': 0.1} for i in range(10)]
        self.initial = {'schema': 'host-orchestrator.jev-task-accounting.v1', 'capUsd': 14,
                        'currency': 'USD', 'providerReportedCostUsd': None,
                        'knownEstimatedSpendUsd': 0.000370776, 'unreconciledReserveUsd': 1,
                        'inferenceRequestsDispatched': 10, 'calls': self.legacy,
                        'admissionOwner': 'historical serialized Root'}
        self.path.write_text(json.dumps(self.initial))
        self.owner = ExistingLedger(self.path)

    def tearDown(self):
        self.temp.cleanup()

    def ledger(self):
        return json.loads(self.path.read_bytes())

    def test_existing_owner_and_ten_legacy_calls_preserved(self):
        self.assertTrue(self.owner.reserve(**arguments())['admitted'])
        current = self.ledger()
        self.assertEqual(current['calls'][:10], self.legacy)
        self.assertEqual(current['admissionOwner'], OWNER)
        self.assertEqual(current['accountingOwnerHistory'][0]['admissionOwner'], 'historical serialized Root')
        self.assertEqual(current['unreconciledReserveUsd'], 1.01)

    def test_missing_ledger_never_created(self):
        missing = self.path.parent / 'missing.json'
        with self.assertRaises(FileNotFoundError):
            ExistingLedger(missing).reserve(**arguments())
        self.assertFalse(missing.exists())

    def test_pricing_unknown_model_or_too_small_reserve_holds_unchanged(self):
        original = self.path.read_bytes()
        for change in ({'reserve_usd': 0.001}, {'pricing': None},
                       {'pricing': {**PRICING, 'model': 'other'}},
                       {'pricing': {**PRICING, 'inputUsdPerToken': 0}}):
            with self.assertRaises(AccountingHeld):
                self.owner.reserve(**arguments(**change))
            self.assertEqual(self.path.read_bytes(), original)

    def test_duplicate_id_and_rebound_id(self):
        self.owner.reserve(**arguments())
        self.assertFalse(self.owner.reserve(**arguments())['admitted'])
        with self.assertRaisesRegex(AccountingHeld, 'request_id_rebound'):
            self.owner.reserve(**arguments(request_sha256='c' * 64))

    def test_full_decision_digest_required_for_cross_id_reuse(self):
        self.owner.reserve(**arguments(decision_digest='c' * 64))
        self.assertFalse(self.owner.reserve(**arguments(2, decision_digest='c' * 64))['admitted'])
        self.assertTrue(self.owner.reserve(**arguments(3, decision_digest='d' * 64))['admitted'])
        self.assertTrue(self.owner.reserve(**arguments(4, decision_digest='c' * 64,
                         scope={**SCOPE, 'sessionId': 'OTHER_SYNTHETIC'}))['admitted'])

    def test_unknown_stays_reserved_and_cannot_redispatch(self):
        self.owner.reserve(**arguments())
        self.owner.mark_unknown(request_id='synthetic-1', request_sha256=f'{1:064x}')
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.01)
        self.assertFalse(self.owner.reserve(**arguments())['admitted'])
        self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}', result=response())
        self.assertEqual(self.ledger()['inferenceRequestsDispatched'], 11)

    def test_pending_lists_only_original_metadata_without_releasing_reserves(self):
        self.owner.reserve(**arguments())
        self.owner.reserve(**arguments(2))
        self.owner.mark_unknown(request_id='synthetic-2', request_sha256=f'{2:064x}')
        before = self.path.read_bytes()
        pending = self.owner.pending()
        self.assertEqual(len(pending['pending']), 2)
        self.assertEqual({row['requestId'] for row in pending['pending']}, {'synthetic-1', 'synthetic-2'})
        self.assertTrue(all(set(row) == {'requestId', 'requestSha256', 'stateDigest', 'state',
                                      'receipt', 'nativeBinding'} for row in pending['pending']))
        self.assertEqual(self.path.read_bytes(), before)
        self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}', result=response())
        self.assertEqual([row['requestId'] for row in self.owner.pending()['pending']], ['synthetic-2'])

    def test_reconcile_idempotence_and_zero_http_refund(self):
        self.owner.reserve(**arguments())
        kw = dict(request_id='synthetic-1', request_sha256=f'{1:064x}', result=response(count=0))
        self.owner.reconcile(**kw)
        after = self.path.read_bytes()
        self.assertTrue(self.owner.reconcile(**kw)['alreadyReconciled'])
        self.assertEqual(self.path.read_bytes(), after)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1)
        self.assertEqual(self.ledger()['inferenceRequestsDispatched'], 10)
        with self.assertRaisesRegex(AccountingHeld, 'original_response_changed'):
            self.owner.reconcile(**{**kw, 'result': response(count=1)})

    def test_corrupt_or_failed_success_cannot_refund(self):
        self.owner.reserve(**arguments())
        original = self.path.read_bytes()
        for result in ({**response(count=0), 'stateDigest': 'd' * 64},
                       {**response(count=0), 'sourcePostconditionVerified': False},
                       {**response(count=0), 'transportInvocations': True}):
            with self.assertRaises(AccountingHeld):
                self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}', result=result)
            self.assertEqual(self.path.read_bytes(), original)

    def test_cap_and_concurrency_limit(self):
        for index in range(1, 5):
            self.owner.reserve(**arguments(index))
        with self.assertRaisesRegex(AccountingHeld, 'in_flight_limit'):
            self.owner.reserve(**arguments(5))
        ledger = copy.deepcopy(self.initial)
        ledger['unreconciledReserveUsd'] = 13.999
        self.path.write_text(json.dumps(ledger))
        with self.assertRaisesRegex(AccountingHeld, 'cumulative_cap_exceeded'):
            self.owner.reserve(**arguments())

    def test_legacy_pending_symlink_and_hardlink_refuse(self):
        current = copy.deepcopy(self.initial)
        current['calls'][0]['state'] = 'admitted-old-unreconciled'
        self.path.write_text(json.dumps(current))
        with self.assertRaisesRegex(AccountingHeld, 'legacy_call_requires_reconciliation'):
            self.owner.reserve(**arguments())
        self.path.write_text(json.dumps(self.initial))
        alias = self.path.parent / 'alias.json'
        alias.symlink_to(self.path)
        with self.assertRaises(AccountingHeld):
            ExistingLedger(alias).reserve(**arguments())
        alias.unlink()
        alias.hardlink_to(self.path)
        with self.assertRaises(AccountingHeld):
            self.owner.reserve(**arguments())

    def test_flock_deadline_does_not_dispatch(self):
        with self.owner._locked():
            with self.assertRaisesRegex(AccountingHeld, 'ledger_owner_busy'):
                ExistingLedger(self.path, lock_timeout=0.03).reserve(**arguments())

    def test_interrupted_atomic_commit_preserves_original_and_candidate(self):
        original = self.path.read_bytes()
        with patch('shared_accounting.os.replace', side_effect=OSError('synthetic replace failure')):
            with self.assertRaises(OSError):
                self.owner.reserve(**arguments())
        self.assertEqual(self.path.read_bytes(), original)
        pending = list(self.path.parent.glob('.existing.json.pending-*'))
        self.assertEqual(len(pending), 1)
        candidate = json.loads(pending[0].read_bytes())
        self.assertEqual(len(candidate['calls']), 11)
        self.assertEqual(candidate['calls'][:10], self.legacy)

    def test_four_processes_preserve_all_reservations_and_reconciliations(self):
        context = multiprocessing.get_context('fork')
        for function in (concurrent_reserve, concurrent_reconcile):
            barrier, output = context.Barrier(4), context.Queue()
            workers = [context.Process(target=function, args=(str(self.path), i, barrier, output))
                       for i in range(1, 5)]
            for worker in workers:
                worker.start()
            observed = [output.get(timeout=10) for _ in workers]
            for worker in workers:
                worker.join(timeout=10)
                self.assertEqual(worker.exitcode, 0)
            if function is concurrent_reserve:
                self.assertEqual(observed, [True] * 4)
            else:
                self.assertTrue(all(str(item).startswith('reconciled-') for item in observed))
        current = self.ledger()
        self.assertEqual(len(current['calls']), 14)
        self.assertEqual(current['calls'][:10], self.legacy)
        self.assertEqual(current['inferenceRequestsDispatched'], 14)
        self.assertEqual(current['unreconciledReserveUsd'], 1.04)

    def test_v1_admission_and_historical_rows_do_not_gain_v2_metadata(self):
        admitted = self.owner.reserve(**arguments())
        self.assertNotIn('semanticContract', admitted['admission'])
        self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}', result=response())
        ledger = self.ledger()
        self.assertEqual(ledger['calls'][:10], self.legacy)
        self.assertNotIn('semanticContract', ledger['calls'][-1])

    def test_v2_explicit_reservation_metadata_pending_and_idempotent_result(self):
        admission = self.owner.reserve(**arguments(semantic_contract=v2_contract()))
        self.assertEqual(admission['admission']['semanticContract'], v2_contract())
        self.assertEqual(self.owner.pending()['pending'][0]['semanticContract'], v2_contract())
        args = {'request_id': 'synthetic-1', 'request_sha256': f'{1:064x}', 'result': v2_response()}
        self.owner.reconcile(**args)
        after = self.path.read_bytes()
        self.assertTrue(self.owner.reconcile(**args)['alreadyReconciled'])
        self.assertEqual(self.path.read_bytes(), after)
        self.assertEqual(self.ledger()['calls'][:10], self.legacy)
        self.assertEqual(self.ledger()['inferenceRequestsDispatched'], 11)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.01)
        self.assertFalse(self.owner.reserve(**arguments(semantic_contract=v2_contract()))['admitted'])

    def test_v2_bad_or_incomplete_reservation_metadata_never_writes(self):
        original = self.path.read_bytes()
        invalid = [{}, {'requestSchema': 'jev.semantic-request.v2'},
                   v2_contract(requestSchema='jev.semantic-request.v1'),
                   v2_contract(resultSchema='jev.semantic-result.v1'),
                   v2_contract(consumptionPolicy='other'), v2_contract(requestDigest='not-a-hash'),
                   {**v2_contract(), 'unowned': True}]
        for metadata in invalid:
            with self.subTest(metadata=metadata), self.assertRaises(AccountingHeld):
                self.owner.reserve(**arguments(semantic_contract=metadata))
            self.assertEqual(self.path.read_bytes(), original)

    def test_v2_result_never_upgrades_legacy_admission_or_downgrades_v2(self):
        self.owner.reserve(**arguments())
        self.owner.reserve(**arguments(2, semantic_contract=v2_contract()))
        original = self.path.read_bytes()
        for index, result in [(1, v2_response()), (2, response(2))]:
            with self.subTest(index=index), self.assertRaisesRegex(AccountingHeld, 'response_contract_binding_mismatch'):
                self.owner.reconcile(request_id=f'synthetic-{index}', request_sha256=f'{index:064x}', result=result)
            self.assertEqual(self.path.read_bytes(), original)

    def test_v2_correlation_policy_and_canonical_digest_mismatch_preserve_reservation(self):
        self.owner.reserve(**arguments(semantic_contract=v2_contract()))
        original = self.path.read_bytes()
        for update in ({'consumptionPolicy': 'other'}, {'requestDigest': 'd' * 64},
                       {'submittedRequestSha256': 'd' * 64}, {'stateDigest': 'd' * 64},
                       {'purpose': 'other'}, {'requestId': 'other'}, {'requestDigest': None}):
            with self.subTest(update=update), self.assertRaises(AccountingHeld):
                self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}',
                                     result=v2_response(**update))
            self.assertEqual(self.path.read_bytes(), original)

    def test_v2_transport_count_authority_and_success_proofs_cannot_refund(self):
        self.owner.reserve(**arguments(semantic_contract=v2_contract()))
        original = self.path.read_bytes()
        for update in ({'transportInvocations': True}, {'connectionLeases': True},
                       {'transportInvocations': 0, 'connectionLeases': 0},
                       {'connectionLeases': 0}, {'status': 'held'},
                       {'executionAuthorityGranted': True}, {'automaticWrite': True},
                       {'sourcePostconditionVerified': False}, {'returnedModel': 'jev-latest'}):
            with self.subTest(update=update), self.assertRaises(AccountingHeld):
                self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}',
                                     result=v2_response(**update))
            self.assertEqual(self.path.read_bytes(), original)

    def test_v2_zero_http_and_unknown_original_effects_stay_no_repeat(self):
        for index in (1, 2):
            self.owner.reserve(**arguments(index, semantic_contract=v2_contract()))
        self.owner.mark_unknown(request_id='synthetic-2', request_sha256=f'{2:064x}')
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.02)
        self.assertFalse(self.owner.reserve(**arguments(2, semantic_contract=v2_contract()))['admitted'])
        zero = v2_response(count=0, status='held', returnedModel=None, sourcePostconditionVerified=False)
        self.owner.reconcile(request_id='synthetic-1', request_sha256=f'{1:064x}', result=zero)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.01)
        unknown = v2_response(2, status='error', returnedModel=None, sourcePostconditionVerified=False)
        self.owner.reconcile(request_id='synthetic-2', request_sha256=f'{2:064x}', result=unknown)
        self.assertEqual(self.ledger()['unreconciledReserveUsd'], 1.01)
        self.assertEqual(self.ledger()['inferenceRequestsDispatched'], 11)
        after = self.path.read_bytes()
        self.assertTrue(self.owner.reconcile(request_id='synthetic-2', request_sha256=f'{2:064x}',
                                            result=unknown)['alreadyReconciled'])
        self.assertFalse(self.owner.reserve(**arguments(2, semantic_contract=v2_contract()))['admitted'])
        self.assertEqual(self.path.read_bytes(), after)

    def test_contract_rebinding_and_cross_version_equivalence_cannot_reopen_effect(self):
        self.owner.reserve(**arguments(decision_digest='e' * 64))
        self.owner.mark_unknown(request_id='synthetic-1', request_sha256=f'{1:064x}')
        original = self.path.read_bytes()
        with self.assertRaisesRegex(AccountingHeld, 'request_contract_rebound_do_not_retry'):
            self.owner.reserve(**arguments(semantic_contract=v2_contract()))
        held = self.owner.reserve(**arguments(2, decision_digest='e' * 64, semantic_contract=v2_contract()))
        self.assertFalse(held['admitted'])
        self.assertNotIn('semanticContract', held['existing'])
        self.assertEqual(self.path.read_bytes(), original)

    def test_v2_actual_imported_caller_result_reconciles_same_existing_owner(self):
        from test_v2_contract import OfflineOwner, request, raw
        from v2_contract import digest
        r = request()
        request_sha = hashlib.sha256(raw(r)).hexdigest()
        metadata = v2_contract(requestDigest=digest(r))
        self.owner.reserve(**arguments(request_id=r['requestId'], request_sha256=request_sha,
            state_digest=r['stateDigest'], scope=r['scope'], semantic_contract=metadata))
        with tempfile.TemporaryDirectory() as folder:
            result = OfflineOwner(Path(folder), r).run()
        self.assertEqual(result['status'], 'ok')
        outcome = self.owner.reconcile(request_id=r['requestId'], request_sha256=request_sha, result=result)
        self.assertEqual(outcome['inferenceRequestsDispatched'], 11)
        self.assertEqual(self.ledger()['calls'][-1]['semanticContract'], metadata)
        self.assertEqual(self.ledger()['calls'][:10], self.legacy)


if __name__ == '__main__':
    unittest.main()
