#!/usr/bin/env python3
"""Fixed-bank engineering publisher, verifier and recovery safety routes; no retries."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import http.client
import importlib.util
import json
import os
import re
from pathlib import Path
import select
import signal
import socket
import stat
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
ORIGIN = 'http://127.0.0.1:8888'
BANK = 'coding-agent::engineering'
B = '/v1/default/banks/' + urllib.parse.quote(BANK, safe='')
API_VERSION = '1.2.0'  # HC measured-20261112.md; another API requires X5.
BUDGET = 900
SPEC = importlib.util.spec_from_file_location('memory_records', ROOT / 'ci/check-memory-records.py')
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class Stop(Exception):
    """Only fixed, body-free codes may cross the command boundary."""


def require(condition, code):
    if not condition:
        raise Stop(code)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def segment(value):
    return urllib.parse.quote(value, safe='')


@contextmanager
def run_budget():
    def expired(*_):
        raise Stop('run_budget_exhausted_read_target_and_operations_no_resend')
    previous = signal.signal(signal.SIGALRM, expired)
    timer = signal.setitimer(signal.ITIMER_REAL, BUDGET)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, *timer)
        signal.signal(signal.SIGALRM, previous)


@contextmanager
def locked():
    # Shared by every subcommand across linked worktrees; no progress.
    common = CHECK.git(ROOT, 'rev-parse', '--git-common-dir').decode().strip()
    path = (ROOT / common).resolve() / 'publish-memory.lock'
    fd = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                and info.st_mode & 0o077 == 0, 'unsafe_lock')
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise Stop('publisher_busy') from None
        yield
    finally:
        os.close(fd)


def credential(fd):
    # Caller opens an anonymous pipe/private FD in process, never a token file.
    require(fd >= 3, 'private_fd_required')
    info = os.fstat(fd)
    require(stat.S_ISFIFO(info.st_mode) or (stat.S_ISREG(info.st_mode)
            and info.st_mode & 0o077 == 0), 'private_fd_required')
    require(bool(select.select([fd], [], [], 15)[0]), 'credential_fd_timeout')
    raw = os.read(fd, 8193)
    require(0 < len(raw) <= 8192, 'credential_length')
    try:
        value = raw.decode('ascii')
    except UnicodeError:
        raise Stop('credential_encoding') from None
    require(value.strip() == value and all(32 < ord(c) < 127 for c in value), 'credential_encoding')
    return value


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Client:
    def __init__(self, secret):
        self._secret = secret
        self.deadline = time.monotonic() + BUDGET
        self.last_write = None
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())

    def request(self, method, path, body=None):
        require(path.startswith(B + '/') or path == '/version', 'target_rejected')
        require(not any(c in path for c in ('\r', '\n', '#', '\\'))
                and not any(x in ('.', '..') for x in path.split('/')), 'target_rejected')
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, 'run_budget_exhausted')
        if method != 'GET':
            if self.last_write is not None:
                time.sleep(max(0, 1 - (time.monotonic() - self.last_write)))
            self.last_write = time.monotonic()
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, 'run_budget_exhausted')
        request = urllib.request.Request(ORIGIN + path, method=method,
                    data=None if body is None else json.dumps(body).encode(),
                    headers={'Authorization': 'Bearer ' + self._secret, 'Content-Type': 'application/json'})
        try:
            with self.opener.open(request, timeout=min(15, remaining)) as response:
                require(response.status == 200, 'unexpected_http_status')
                raw = response.read(8 * 1024 * 1024 + 1)
                require(len(raw) <= 8 * 1024 * 1024, 'response_too_large')
                return json.loads(raw, object_pairs_hook=CHECK.unique_pairs)
        except urllib.error.HTTPError as error:
            # Never inspect/log the response body, URL, headers or exception text.
            if method == 'GET' and error.code == 404:
                raise Stop('read_404') from None
            raise Stop('http_rejected_read' if method == 'GET' else 'mutation_uncertain') from None
        except (OSError, socket.timeout, http.client.HTTPException, ValueError, RecursionError):
            raise Stop('read_uncertain' if method == 'GET' else 'mutation_uncertain') from None


def pages(client, route, key='items', query=None, limit=100):
    """HC U8/030-033, U14/018, Mental-model list, Profile/538."""
    offset, total, result, ids = 0, None, [], set()
    while True:
        args = dict(query or {}, limit=limit, offset=offset)
        page = client.request('GET', B + route + '?' + urllib.parse.urlencode(args))
        require(isinstance(page, dict) and isinstance(page.get(key), list), 'list_shape')
        n = page.get('total')
        require(type(n) is int and n >= 0 and type(page.get('offset')) is int and page['offset'] == offset
                and type(page.get('limit')) is int and page['limit'] > 0
                and page['limit'] <= limit and page.get('total_is_estimate', False) is False, 'list_incomplete')
        require(total is None or total == n, 'list_changed')
        total = n
        batch = page[key]
        require(len(batch) <= page['limit'] and offset + len(batch) <= total, 'list_incomplete')
        for item in batch:
            require(isinstance(item, dict) and isinstance(item.get('id'), str)
                    and item['id'] not in ids, 'list_identity')
            ids.add(item['id'])
        result.extend(batch)
        offset += len(batch)
        if offset == total:
            return result
        require(bool(batch), 'list_incomplete')


def pinned():
    """W0: Lead owns holds/admission/stop proof; source pins come only from Git."""
    commit = CHECK.git(ROOT, 'rev-parse', 'refs/heads/main^{commit}').decode().strip()
    # CHECK.load resolves its repository using cwd; the CLI fixes cwd to ROOT.
    _, records, nodes, errors = CHECK.load(Path('memory/records'), Path('memory/subjects.json'),
                                          commit=commit, publication=True)
    require(not errors, 'source_invalid')
    raw = CHECK.git(ROOT, 'show', commit + ':memory/engineering.json')
    config = json.loads(raw, object_pairs_hook=CHECK.unique_pairs)
    require(set(config) == {'schema_version', 'bank_id', 'retain_custom_instructions', 'mcp_enabled_tools'}
            and type(config['schema_version']) is int and config['schema_version'] == 1
            and config['bank_id'] == BANK and config['mcp_enabled_tools'] == ['recall', 'get_document']
            and isinstance(config['retain_custom_instructions'], str)
            and bool(config['retain_custom_instructions']), 'config_invalid')
    subjects = CHECK.git(ROOT, 'show', commit + ':memory/subjects.json')
    expected = {rid: {'bytes': CHECK.publication_bytes(r), 'hash': CHECK.publication_hash(r),
                     'tags': CHECK.derived_tags(r['header'], nodes), 'terms': r['header']['recall_terms'],
                     'recall': CHECK.recall_tags(r['header']['subject'])}
                for rid, r in records.items()}
    return {'commit': commit, 'config_raw': raw, 'subjects_raw': subjects,
            'config': config, 'expected': expected}


def unchanged(pin):
    """W8 source drift barrier; never substitutes for acceptance or W9."""
    require(CHECK.git(ROOT, 'rev-parse', 'refs/heads/main^{commit}').decode().strip() == pin['commit'], 'stale_commit')
    for name, raw in [('engineering', pin['config_raw']), ('subjects', pin['subjects_raw'])]:
        require(CHECK.git(ROOT, 'show', 'refs/heads/main:memory/' + name + '.json') == raw, 'stale_config')


def quiescent(client):
    """W1/W6: enumerate parents AND children, not just one acknowledgement (U14)."""
    operations = pages(client, '/operations', key='operations')
    for operation in operations:
        require(operation.get('status') == 'completed', 'operation_not_completed')
        detail = client.request('GET', B + '/operations/' + segment(operation['id']) + '?include_payload=true')
        require(detail.get('id') == operation['id'] and detail.get('status') == 'completed'
                and detail.get('error_message') is None, 'operation_not_completed')
        metadata = detail.get('result_metadata') or {}
        require(metadata.get('extraction_errors_count', 0) == 0, 'extraction_failed')
        parent = metadata.get('parent_operation_id')
        require(parent is None or parent in {x['id'] for x in operations}, 'operation_parent_missing')
        children = detail.get('child_operations') or []  # 519-h-q2-retain-poll1, HC U14
        require(isinstance(children, list) and all(isinstance(x, dict)
                and x.get('operation_id') in {o['id'] for o in operations}
                and x.get('status') == 'completed' and x.get('error_message') is None
                for x in children), 'operation_child_missing_or_active')
        if metadata.get('is_parent'):
            require(metadata.get('num_sub_batches') == len(children), 'operation_child_missing_or_active')
    return operations


def profile(client, config):
    """HC Profile/480,517: readable fields only; no invented embedding profile."""
    require(client.request('GET', '/version').get('api_version') == API_VERSION, 'api_drift')
    actual = client.request('GET', B + '/config').get('config', {})
    require(actual.get('enable_observations') is False and actual.get('enable_auto_consolidation') is False,
            'observations_enabled')
    require(actual.get('retain_extraction_mode') == 'custom'
            and actual.get('retain_custom_instructions') == config['retain_custom_instructions'], 'profile_drift')
    return digest(actual['retain_custom_instructions'].encode())


def empty_derived(client):
    # HC 470/475: include pages, no tag filter; empty items alone is insufficient.
    require(not pages(client, '/mental-models', query={'exclude_knowledge_pages': 'false'}), 'models_present')
    stats = client.request('GET', B + '/stats')  # HC U11/107
    require(stats.get('total_observations') == 0 and stats.get('pending_operations') == 0
            and stats.get('pending_consolidation') == 0 and stats.get('failed_operations') == 0
            and stats.get('failed_consolidation') == 0, 'derived_or_pending_work')


def verify(client, pin):
    """W7: exhaustive ID/byte/full-tag/term/profile/trace checks, read-only."""
    instruction_hash = profile(client, pin['config'])
    quiescent(client)
    empty_derived(client)
    expected = pin['expected']
    documents = pages(client, '/documents')
    require({x['id'] for x in documents} == set(expected), 'document_set_mismatch')
    all_facts = pages(client, '/memories/list')
    require(all(f.get('document_id') in expected for f in all_facts), 'foreign_fact')
    fact_ids = set()
    for rid, want in expected.items():
        doc = client.request('GET', B + '/documents/' + segment(rid))  # HC U4/016
        text = doc.get('original_text')
        require(doc.get('id') == rid and isinstance(text, str)
                and text.encode() == want['bytes'] and digest(text.encode()) == want['hash'], 'document_bytes')
        require(doc.get('tags') == want['tags'], 'document_tags')
        facts = pages(client, '/memories/list', query={'document_id': rid})  # HC U4/025
        for fact in facts:
            require(fact.get('document_id') == rid and fact.get('tags') == want['tags']
                    and fact.get('fact_type') in ('world', 'experience'), 'fact_scope_or_tags')
            require(isinstance(fact.get('text'), str) and isinstance(fact.get('entities'), str), 'fact_shape')
            fact_ids.add(fact['id'])
        for term in want['terms']:
            # Entities are plain names separated by ", ": HC 025-facts-tags-readback / 032-facts-page1.
            require(any(term in f['text'] or term in f['entities'] for f in facts), 'recall_term_missing')
    require(fact_ids == {x['id'] for x in all_facts}, 'fact_set_mismatch')
    traces = pages(client, '/llm-requests', limit=500)  # HC Profile/538
    extraction = [x for x in traces if x.get('scope') == 'retain_extract_facts']
    require(all(x.get('status') == 'success' and x.get('metadata', {}).get('document_id') in expected
                and x.get('provider') and x.get('model') for x in extraction), 'trace_attribution')
    require({x.get('metadata', {}).get('document_id') for x in extraction} == set(expected), 'trace_missing')
    profiles = {(x['provider'], x['model']) for x in extraction}
    require(len(profiles) <= 1, 'extraction_profile_drift')
    require(profile(client, pin['config']) == instruction_hash, 'profile_drift')
    quiescent(client)
    empty_derived(client)
    unchanged(pin)
    return {'commit': pin['commit'], 'documents': len(documents), 'facts': len(fact_ids),
            'api_version': API_VERSION, 'instruction_sha256': instruction_hash,
            'extraction_profile_sha256': [digest(json.dumps(x).encode()) for x in sorted(profiles)],
            'acceptance': 'not_evaluated'}


def retain_once(client, rid, want):
    """W6 measured dispatch helper; unreachable from publish until UQ1/UQ2 qualify."""
    operation = str(uuid.uuid4())
    try:
        result = client.request('POST', B + '/memories', {
            'items': [{'content': want['bytes'].decode(), 'document_id': rid,
                       'tags': want['tags'], 'update_mode': 'replace'}],
            'async': True, 'operation_id': operation})  # HC U3/010 + U1/001 optional timestamp; no retry.
        require(isinstance(result, dict) and result.get('success') is True
                and result.get('operation_id') == operation and result.get('bank_id') == BANK
                and result.get('items_count') == 1 and result.get('async') is True,
                'mutation_uncertain')
    except Stop:
        # Native read-only reconciliation, never an overlapping mutation or resend.
        try:
            client.request('GET', B + '/documents/' + segment(rid))
            quiescent(client)
        except Stop:
            pass
        raise Stop('retain_unresolved_read_document_and_parent_child_operations_keep_consumers_stopped') from None
    try:
        operations = quiescent(client)
        require(any(x['id'] == operation for x in operations), 'operation_missing')
    except Stop:
        raise Stop('retain_acknowledged_read_document_and_parent_child_operations_keep_consumers_stopped') from None


def publish(client, pin):
    """W0-W5 barriers: deliberately no speculative bank lifecycle request."""
    require(bool(pin['expected']), 'no_admitted_corpus')
    unchanged(pin)
    try:
        profile(client, pin['config'])
        quiescent(client)
        empty_derived(client)
        pages(client, '/documents')
        pages(client, '/memories/list')
    except Stop as error:
        if str(error) == 'read_404':
            raise Stop('UQ1_read_404_requires_bank_absence_proof_V4_5_no_mutation') from None
        raise
    # W2/W3 export/restore qualification and W4 delete/absence are UQ1.
    # W5 denial and W9 enablement are UQ2; W9 sessions are UQ3.
    # W6 cannot precede W5. W7 is verify(); W8 judgments belong to the Lead.
    raise Stop('UQ1_export_restore_delete_and_UQ2_denial_require_V4_5_no_mutation')


def absent_document(client, rid, chunks=False):
    """HC U5/125,127: only exact GET404 establishes absence on this read."""
    try:
        client.request('GET', B + '/documents/' + segment(rid) + ('/chunks' if chunks else ''))
    except Stop as error:
        if str(error) == 'read_404':
            return True
        raise
    return False


def retract(client, pin, rid, no_recovery_copy=False):
    """Design 6: Lead owns consumer stop and pending source-removal hold."""
    require(bool(CHECK.ID.fullmatch(rid)), 'record_id_invalid')
    unchanged(pin)
    instruction_hash = profile(client, pin['config'])
    quiescent(client)
    empty_derived(client)
    doc = client.request('GET', B + '/documents/' + segment(rid))  # HC U4/016
    require(doc.get('id') == rid and isinstance(doc.get('original_text'), str), 'document_shape')
    want = pin['expected'].get(rid)
    require(no_recovery_copy or (want is not None
            and digest(doc['original_text'].encode()) == want['hash']), 'bank_copy_not_in_source')
    unchanged(pin)
    uncertain = False
    try:
        answer = client.request('DELETE', B + '/documents/' + segment(rid))  # HC U5/124
        uncertain = not (isinstance(answer, dict) and answer.get('success') is True and answer.get('document_id') == rid)
    except Stop:
        uncertain = True
    # Read each independently even after a lost answer. Never retry DELETE.
    reads = {}
    for name, read in [
            ('document_absent', lambda: absent_document(client, rid)),
            ('facts_absent', lambda: not pages(client, '/memories/list', query={'document_id': rid})),
            ('chunks_absent', lambda: absent_document(client, rid, chunks=True))]:
        try:
            reads[name] = read()
        except Stop:
            reads[name] = 'unknown'
    # Only booleans/fixed words leave this boundary, never returned text/errors.
    print(json.dumps(dict(reads, consumers='keep_stopped', source_removal='Lead_hold_required'), sort_keys=True))
    require(not uncertain, 'delete_unresolved_route_unavailable_no_resend')
    require(all(value is True for value in reads.values()), 'deleted_unproven')
    quiescent(client)
    empty_derived(client)
    require(profile(client, pin['config']) == instruction_hash, 'profile_drift')
    if want is None:
        print('listed_reads=not_evaluated')
        raise Stop('deleted_unproven')
    listed = 0
    for term in want['terms']:
        # HC U10/062,143: one term query with the exact derived subject selector.
        answer = client.request('POST', B + '/memories/recall', dict(
            query=term, types=['world', 'experience'], budget='low', max_tokens=512,
            include={'entities': None, 'chunks': {'max_tokens': 512}}, **want['recall']))
        results = answer.get('results')
        require(isinstance(results, list) and all(isinstance(x, dict)
                and isinstance(x.get('document_id'), str) for x in results), 'recall_shape')
        require(all(x['document_id'] != rid for x in results), 'listed_read_contains_document')
        listed += 1
    quiescent(client)
    empty_derived(client)
    require(profile(client, pin['config']) == instruction_hash, 'profile_drift')
    unchanged(pin)
    return {'result': 'retracted_on_listed_reads', 'listed_reads': listed,
            'consumers': 'keep_stopped', 'source_removal': 'Lead_hold_required',
            'route': 'unavailable', 'commit': pin['commit']}


def recovery_file(path, expected_hash):
    """Local bytes only; native archive inner format is still UQ1."""
    require(bool(re.fullmatch('[0-9a-fA-F]{64}', expected_hash)), 'recovery_sha256_invalid')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        require(stat.S_ISREG(os.fstat(fd).st_mode), 'recovery_not_regular')
        hasher = hashlib.sha256()
        while True:
            raw = os.read(fd, 1024 * 1024)
            if not raw:
                break
            hasher.update(raw)
        require(hasher.hexdigest() == expected_hash.lower(), 'recovery_hash_mismatch')
    finally:
        os.close(fd)


def restore(client, pin, recovery, recovery_sha256):
    """Design 8: no speculative import or bank lifecycle shape, even in tests."""
    recovery_file(recovery, recovery_sha256)
    unchanged(pin)
    # Existing-bank preconditions use W1 reads. A 404 is not absence proof (UQ1).
    try:
        profile(client, pin['config'])
        quiescent(client)
        empty_derived(client)
    except Stop as error:
        if str(error) != 'read_404':
            raise
    raise Stop('uq1_restore_unmeasured')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
             epilog='All subcommands require --credential-fd FD (an inherited private descriptor).')
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('publish', 'verify', 'retract', 'restore'):
        sub = commands.add_parser(name)
        sub.add_argument('--credential-fd', required=True, type=int, help='inherited private FD, never a secret value')
        if name == 'retract':
            sub.add_argument('--record-id', required=True)
            sub.add_argument('--no-recovery-copy', action='store_true')
        if name == 'restore':
            sub.add_argument('--recovery', required=True, type=Path)
            sub.add_argument('--recovery-sha256', required=True)
    args = parser.parse_args(argv)
    os.chdir(ROOT)
    try:
        with run_budget(), locked():
            pin = pinned()
            client = Client(credential(args.credential_fd))
            if args.command == 'retract':
                result = retract(client, pin, args.record_id, args.no_recovery_copy)
            elif args.command == 'restore':
                result = restore(client, pin, args.recovery, args.recovery_sha256)
            else:
                result = verify(client, pin) if args.command == 'verify' else publish(client, pin)
            print(json.dumps(dict(result, budget_seconds=BUDGET), sort_keys=True))
        return 0
    except Stop as error:
        print('publisher_stop@scripts/publish-memory.py:1: ' + str(error))
    except Exception:
        print('publisher_stop@scripts/publish-memory.py:1: input_or_transport_failure_no_resend')
    return 1


if __name__ == '__main__':
    sys.exit(main())
