"""Short, flock-serialized transitions on the existing task-owned Jev ledger.

This module never creates/replaces a ledger, dispatches inference, or retries a
request. Native callers reserve once, release the lock during inference, then
reconcile the exact original receipt. All writers of this ledger must use this
owner; the historical serial invoker is not a concurrent writer.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import datetime as dt
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import secrets
import stat
import sys
import time

DEFAULT_LEDGER = Path('/home/palantirkc/orchestrator/.recovery-20261019/jev-accounting.actual.json')
MODEL = 'typesafe-ai/jev-1.14.0'
DIRECT_POLICY = 'jev.direct-selected-answer.v2'
HEX = re.compile(r'[0-9a-f]{64}\Z')
LABEL = re.compile(r'[A-Za-z0-9_.:-]{1,160}\Z')
ACTIVE = {'admitted-semantic-dispatch-pending', 'dispatch-outcome-unknown-do-not-retry'}
OWNER = 'ExistingLedger flock owner; inference outside lock; legacy serialized helpers retired'


class AccountingHeld(Exception):
    """A safe, non-secret error code; never an automatic retry instruction."""


def need(condition, code):
    if not condition:
        raise AccountingHeld(code)


def sha(body):
    return hashlib.sha256(body).hexdigest()


def utc():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def contract_metadata(value):
    """Explicit v2 admission only; absent metadata always means legacy v1.

    These technical binding fields are supplied by the existing native host
    owner before dispatch. No schema, policy or canonical request digest is
    inferred from an old ledger row or a later provider result.
    """
    if value is None:
        return None
    need(isinstance(value, dict) and set(value) == {
        'requestSchema', 'resultSchema', 'consumptionPolicy', 'requestDigest'},
        'v2_reservation_metadata_required')
    need(value['requestSchema'] == 'jev.semantic-request.v2'
         and value['resultSchema'] == 'jev.semantic-result.v2'
         and value['consumptionPolicy'] == DIRECT_POLICY
         and isinstance(value['requestDigest'], str) and HEX.fullmatch(value['requestDigest']),
         'v2_reservation_metadata_invalid')
    return dict(value)


def numeric(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


class ExistingLedger:
    def __init__(self, path=DEFAULT_LEDGER, *, lock_timeout=2.0, max_in_flight=4):
        self.path = Path(path)
        need(self.path.is_absolute(), 'absolute_existing_ledger_required')
        self.lock_path = self.path.with_name('.' + self.path.name + '.lock')
        need(numeric(lock_timeout) and 0 < lock_timeout <= 10, 'lock_deadline')
        need(type(max_in_flight) is int and 1 <= max_in_flight <= 4, 'concurrency_limit')
        self.lock_timeout, self.max_in_flight = lock_timeout, max_in_flight

    @contextmanager
    def _locked(self):
        need(self.path.resolve() == self.path and not self.path.is_symlink(), 'exact_existing_ledger_required')
        original = self.path.lstat()
        need(stat.S_ISREG(original.st_mode) and original.st_nlink == 1, 'regular_existing_ledger_required')
        lock_descriptor = os.open(self.lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        lock_meta = os.fstat(lock_descriptor)
        locked = False
        descriptor = None
        try:
            need(stat.S_ISREG(lock_meta.st_mode) and lock_meta.st_nlink == 1
                 and lock_meta.st_uid == original.st_uid and stat.S_IMODE(lock_meta.st_mode) == 0o600,
                 'task_lock_metadata_changed')
            deadline = time.monotonic() + self.lock_timeout
            while True:
                try:
                    fcntl.flock(lock_descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    locked = True
                    break
                except BlockingIOError:
                    need(time.monotonic() < deadline, 'ledger_owner_busy_no_dispatch')
                    time.sleep(0.01)
            need((self.lock_path.lstat().st_dev, self.lock_path.lstat().st_ino) ==
                 (lock_meta.st_dev, lock_meta.st_ino), 'task_lock_identity_changed')
            descriptor = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW)
            meta = os.fstat(descriptor)
            current = self.path.lstat()
            need(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1 and meta.st_size <= 1_048_576,
                 'regular_bounded_existing_ledger_required')
            need((meta.st_dev, meta.st_ino) == (current.st_dev, current.st_ino), 'ledger_identity_changed')
            raw = os.read(descriptor, 1_048_577)
            ledger = json.loads(raw)
            self._validate(ledger)
            yield descriptor, raw, ledger
        finally:
            if locked:
                fcntl.flock(lock_descriptor, fcntl.LOCK_UN)
            if descriptor is not None:
                os.close(descriptor)
            os.close(lock_descriptor)

    @staticmethod
    def _validate(ledger):
        need(isinstance(ledger, dict) and ledger.get('schema') == 'host-orchestrator.jev-task-accounting.v1',
             'existing_ledger_schema')
        need(ledger.get('capUsd') == 14 and ledger.get('currency') == 'USD', 'existing_cap_owner_required')
        for key in ('knownEstimatedSpendUsd', 'unreconciledReserveUsd'):
            need(numeric(ledger.get(key)) and ledger[key] >= 0, 'accounting_unknown')
        need(type(ledger.get('inferenceRequestsDispatched')) is int and ledger['inferenceRequestsDispatched'] >= 0,
             'request_count_unknown')
        need(isinstance(ledger.get('calls'), list) and all(isinstance(row, dict) for row in ledger['calls']),
             'existing_call_records_required')
        need(all(str(row.get('state', '')).startswith('reconciled-') or
                 row.get('state') in ACTIVE for row in ledger['calls']), 'legacy_call_requires_reconciliation')
        for row in ledger['calls']:
            if 'semanticContract' in row:
                need(row['semanticContract'] is not None, 'v2_reservation_metadata_required')
                contract_metadata(row['semanticContract'])

    def _persist(self, descriptor, previous, ledger):
        current = self.path.lstat()
        held = os.fstat(descriptor)
        need((current.st_dev, current.st_ino) == (held.st_dev, held.st_ino), 'ledger_identity_changed')
        # Detect an old non-cooperating serial writer; all active writers must
        # still be admitted to this narrow shared owner before activation.
        os.lseek(descriptor, 0, os.SEEK_SET)
        need(os.read(descriptor, 1_048_577) == previous, 'ledger_changed_do_not_dispatch')
        raw = (json.dumps(ledger, indent=2, allow_nan=False) + '\n').encode()
        # Stable sibling flock protects one existing task ledger. Atomic replace
        # prevents torn JSON after process death; this is not a runtime/session
        # store or a general transaction framework. Uncertain candidates remain.
        temporary = self.path.with_name('.' + self.path.name + '.pending-' + secrets.token_hex(12))
        writer = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                         stat.S_IMODE(held.st_mode))
        try:
            os.fchmod(writer, stat.S_IMODE(held.st_mode))
            os.fchown(writer, held.st_uid, held.st_gid)
            view = memoryview(raw)
            while view:
                count = os.write(writer, view)
                need(count > 0, 'ledger_write_unknown')
                view = view[count:]
            os.fsync(writer)
        finally:
            os.close(writer)
        current = self.path.lstat()
        need((current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns, current.st_ctime_ns,
              current.st_uid, current.st_gid, current.st_mode, current.st_nlink) ==
             (held.st_dev, held.st_ino, held.st_size, held.st_mtime_ns, held.st_ctime_ns,
              held.st_uid, held.st_gid, held.st_mode, held.st_nlink), 'ledger_preimage_changed')
        os.replace(temporary, self.path)
        parent = os.open(self.path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            os.fsync(parent)
        finally:
            os.close(parent)
        return sha(raw)

    @staticmethod
    def _binding(scope):
        need(isinstance(scope, dict) and isinstance(scope.get('sessionId'), str)
             and LABEL.fullmatch(scope['sessionId']), 'native_session_required')
        for key in ('agentId', 'agentType'):
            need(scope.get(key) is None or isinstance(scope[key], str) and LABEL.fullmatch(scope[key]),
                 'native_identity_invalid')
        return {key: scope.get(key) for key in ('sessionId', 'agentId', 'agentType')}

    def reserve(self, *, request_id, request_sha256, state_digest, purpose, scope, receipt_ref,
                pricing, reserve_usd=0.01, max_input_bytes=49152, decision_digest=None,
                semantic_contract=None):
        need(isinstance(request_id, str) and LABEL.fullmatch(request_id), 'request_id')
        need(isinstance(request_sha256, str) and HEX.fullmatch(request_sha256), 'request_digest')
        need(isinstance(state_digest, str) and HEX.fullmatch(state_digest), 'state_digest')
        need(purpose == 'semantic_bundle', 'purpose')
        need(isinstance(receipt_ref, str) and 1 <= len(receipt_ref) <= 2048, 'receipt_reference')
        need(numeric(reserve_usd) and 0 < reserve_usd <= 0.10, 'reserve_bound')
        need(type(max_input_bytes) is int and max_input_bytes == 49152, 'input_byte_bound')
        need(isinstance(pricing, dict) and set(pricing) == {
            'model', 'inputUsdPerToken', 'outputUsdPerToken', 'evidenceSha256'}, 'pricing_evidence_required')
        need(pricing['model'] == MODEL and isinstance(pricing['evidenceSha256'], str)
             and HEX.fullmatch(pricing['evidenceSha256']), 'pricing_model_or_evidence')
        need(all(numeric(pricing[key]) and pricing[key] >= 0
                 for key in ('inputUsdPerToken', 'outputUsdPerToken'))
             and pricing['inputUsdPerToken'] > 0, 'known_finite_model_pricing_required')
        # One token per input/output byte is a deliberately conservative local
        # admission estimate, not a provider invoice or universal tokenizer law.
        minimum = max_input_bytes * pricing['inputUsdPerToken'] + 65536 * pricing['outputUsdPerToken']
        need(reserve_usd >= minimum, 'reserve_below_pricing_byte_bound')
        need(decision_digest is None or isinstance(decision_digest, str) and HEX.fullmatch(decision_digest),
             'decision_digest')
        semantic_contract = contract_metadata(semantic_contract)
        binding = self._binding(scope)
        with self._locked() as (descriptor, previous, ledger):
            for row in ledger['calls']:
                if row.get('localCorrelationId') == request_id:
                    need(row.get('requestSha256') == request_sha256, 'request_id_rebound_do_not_retry')
                    need(row.get('semanticContract') == semantic_contract, 'request_contract_rebound_do_not_retry')
                    return {'admitted': False, 'reason': 'existing_request_do_not_dispatch',
                            'existing': row, 'ledgerSha256': sha(previous)}
                if (decision_digest is not None and row.get('decisionDigest') == decision_digest
                        and row.get('purpose') == purpose and row.get('stateDigest') == state_digest
                        and row.get('nativeBinding') == binding):
                    # A schema successor cannot reopen an equivalent completed
                    # or unknown effect. Returning the original row does not
                    # upgrade its version or infer missing admission metadata.
                    return {'admitted': False, 'reason': 'equivalent_state_do_not_dispatch',
                            'existing': row, 'ledgerSha256': sha(previous)}
            need(sum(row.get('state') in ACTIVE for row in ledger['calls']) < self.max_in_flight,
                 'in_flight_limit_no_dispatch')
            accounted = ledger['knownEstimatedSpendUsd'] + ledger['unreconciledReserveUsd']
            need(accounted + reserve_usd <= 14, 'cumulative_cap_exceeded')
            entry = {'localCorrelationId': request_id, 'operation': 'semantic_bundle', 'purpose': purpose,
                     'nativeTaskOrRunId': None, 'nativeBinding': binding, 'stateDigest': state_digest,
                     'decisionDigest': decision_digest,
                     'state': 'admitted-semantic-dispatch-pending', 'requestSha256': request_sha256,
                     'reservedUsd': reserve_usd, 'previousAccountingSha256': sha(previous),
                     'receipt': receipt_ref, 'admittedAtUtc': utc(), 'accountedTransportCount': 0,
                     'pricingEvidenceSha256': pricing['evidenceSha256'],
                     'minimumByteBoundReserveUsd': minimum, 'maximumWireInputBytes': max_input_bytes}
            if semantic_contract is not None:
                entry['semanticContract'] = semantic_contract
            if ledger.get('admissionOwner') != OWNER:
                ledger.setdefault('accountingOwnerHistory', []).append({
                    'admissionOwner': ledger.get('admissionOwner'),
                    'retiredAtUtc': utc(), 'successor': OWNER,
                    'meaning': 'Attributed historical owner; all active host writers must be retired or migrated before use.'})
                ledger['admissionOwner'] = OWNER
            ledger['calls'].append(entry)
            ledger['unreconciledReserveUsd'] = round(ledger['unreconciledReserveUsd'] + reserve_usd, 12)
            current_sha = self._persist(descriptor, previous, ledger)
            return {'admitted': True, 'admission': entry, 'ledgerSha256': current_sha,
                    'priorAccountedUsd': accounted, 'cumulativeCapUsd': 14}

    def mark_unknown(self, *, request_id, request_sha256):
        with self._locked() as (descriptor, previous, ledger):
            row = self._find(ledger, request_id, request_sha256)
            need(not str(row.get('state', '')).startswith('reconciled-'), 'already_reconciled')
            row.update(state='dispatch-outcome-unknown-do-not-retry', uncertainRequestsMaximum=1)
            return {'state': row['state'], 'ledgerSha256': self._persist(descriptor, previous, ledger),
                    'retryAllowed': False}

    def pending(self):
        """Original unknown/pending inference receipts only; never state or a queue."""
        with self._locked() as (_, previous, ledger):
            rows = [row for row in ledger['calls'] if row.get('purpose') == 'semantic_bundle'
                    and row.get('state') in ACTIVE]
            need(len(rows) <= 4, 'pending_owner_state_unknown')
            return {'schema': 'jev.semantic-pending.v1', 'ledgerSha256': sha(previous),
                    'pending': [{'requestId': row['localCorrelationId'],
                                 'requestSha256': row['requestSha256'],
                                 'stateDigest': row['stateDigest'], 'state': row['state'],
                                 'receipt': row['receipt'], 'nativeBinding': row['nativeBinding'],
                                 **({'semanticContract': dict(row['semanticContract'])}
                                    if 'semanticContract' in row else {})}
                                for row in rows]}

    @staticmethod
    def _find(ledger, request_id, request_sha256):
        matches = [row for row in ledger['calls'] if row.get('localCorrelationId') == request_id]
        need(len(matches) == 1 and matches[0].get('requestSha256') == request_sha256, 'original_admission_required')
        return matches[0]

    def reconcile(self, *, request_id, request_sha256, result, estimate_usd=None):
        need(isinstance(result, dict) and result.get('schema') in (
            'jev.semantic-result.v1', 'jev.semantic-result.v2'), 'response_schema')
        need(result.get('requestId') == request_id and result.get('submittedRequestSha256') == request_sha256,
             'response_correlation_unknown')
        count = result.get('transportInvocations')
        need(type(count) is int and count in (0, 1), 'transport_count_unknown')
        need(result.get('status') in ('ok', 'error', 'held'), 'response_status')
        need(result.get('requestedModel') == MODEL, 'requested_model_mismatch')
        if result['schema'] == 'jev.semantic-result.v2':
            need(result.get('consumptionPolicy') == DIRECT_POLICY
                 and isinstance(result.get('requestDigest'), str) and HEX.fullmatch(result['requestDigest'])
                 and result.get('purpose') == 'semantic_bundle', 'v2_response_contract_invalid')
            leases = result.get('connectionLeases')
            need(type(leases) is int and 0 <= leases <= count, 'v2_connection_count_unknown')
            need(result.get('executionAuthorityGranted') is False and result.get('automaticWrite') is False,
                 'v2_response_authority_invalid')
            need(result['status'] != 'held' or count == 0, 'v2_held_transport_count')
            need(result['status'] != 'ok' or count == leases == 1, 'v2_success_transport_count')
        if result['status'] == 'ok':
            need(result.get('returnedModel') in (MODEL, 'jev-1.14.0')
                 and result.get('sourcePostconditionVerified') is True, 'successful_response_unverified')
        need(estimate_usd is None or numeric(estimate_usd) and estimate_usd >= 0, 'estimate_invalid')
        response_sha = sha(json.dumps(result, sort_keys=True, separators=(',', ':'), allow_nan=False).encode())
        with self._locked() as (descriptor, previous, ledger):
            row = self._find(ledger, request_id, request_sha256)
            need(result.get('stateDigest') == row.get('stateDigest'), 'state_binding_mismatch')
            contract = row.get('semanticContract')
            if contract is None:
                need(result['schema'] == 'jev.semantic-result.v1', 'response_contract_binding_mismatch')
            else:
                need(result['schema'] == contract['resultSchema']
                     and result.get('consumptionPolicy') == contract['consumptionPolicy']
                     and result.get('requestDigest') == contract['requestDigest']
                     and result.get('purpose') == row['purpose'], 'response_contract_binding_mismatch')
            if str(row.get('state', '')).startswith('reconciled-'):
                need(row.get('semanticResponseSha256') == response_sha, 'original_response_changed')
                return {'state': row['state'], 'alreadyReconciled': True, 'ledgerSha256': sha(previous)}
            need(row.get('accountedTransportCount') == 0, 'transport_already_accounted')
            ledger['inferenceRequestsDispatched'] += count
            if count == 0:
                ledger['unreconciledReserveUsd'] = round(ledger['unreconciledReserveUsd'] - row['reservedUsd'], 12)
            if estimate_usd is not None and count:
                ledger['knownEstimatedSpendUsd'] += estimate_usd
                row['catalogBasedEstimateUsd'] = estimate_usd
            row.update(state='reconciled-semantic-native-response-billing-reserve-retained' if count else
                       'reconciled-semantic-zero-http', transportInvocations=count,
                       accountedTransportCount=count, responseStatus=result['status'],
                       returnedModel=result.get('returnedModel'), semanticResponseSha256=response_sha,
                       sourcePostconditionVerified=result.get('sourcePostconditionVerified') is True,
                       providerReportedCostUsd=None, reconciledAtUtc=utc())
            ledger['lastReconciledAtUtc'] = row['reconciledAtUtc']
            return {'state': row['state'], 'ledgerSha256': self._persist(descriptor, previous, ledger),
                    'inferenceRequestsDispatched': ledger['inferenceRequestsDispatched'],
                    'unreconciledReserveUsd': ledger['unreconciledReserveUsd'],
                    'knownEstimatedSpendUsd': ledger['knownEstimatedSpendUsd']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('operation', choices=('reserve', 'reconcile', 'mark-unknown', 'pending'))
    args = parser.parse_args()
    raw = sys.stdin.buffer.read(131073)
    need(0 < len(raw) <= 131072, 'accounting_input_bound')
    payload = json.loads(raw)
    owner = ExistingLedger()
    method = {'reserve': owner.reserve, 'reconcile': owner.reconcile,
              'mark-unknown': owner.mark_unknown, 'pending': owner.pending}[args.operation]
    print(json.dumps(method(**payload), allow_nan=False))


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        code = error.args[0] if isinstance(error, AccountingHeld) else 'accounting_precondition_or_outcome_unknown'
        print(json.dumps({'status': 'held', 'errorCode': code, 'retryAllowed': False}))
        raise SystemExit(1)
