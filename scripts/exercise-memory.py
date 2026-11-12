#!/usr/bin/env python3
"""Opt-in, bounded invented-bank measurement. Never run as part of CI."""
import argparse
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import signal
import sys
import time
import urllib.error
import urllib.parse
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('exercise_publisher', ROOT / 'scripts/publish-memory.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
BANKS = '/v1/default/banks'
ID = re.compile(r'zz-conformance-[0-9]{8}-[0-9a-f]{8}')
MAX_BYTES = 8 * 1024 * 1024
FAILURE_CODES = frozenset(('absence_unproven allowed_document_failed allowed_read_failed allowed_scope_failed '
    'allowed_shape_unmeasured allowed_tool_denied archive_corrupt archive_document_mismatch '
    'archive_documents_unmeasured archive_foreign_bank archive_too_large archive_unmeasured '
    'bank_already_present bank_id_rejected canary_missed_drift canary_read_failed config_failed '
    'config_readback_failed counts_unmeasured create_failed default_catalog_unmeasured delete_failed '
    'denial_shape_unmeasured document_bytes_or_tags document_set_changed download_target_rejected '
    'fact_readback_failed fact_set_changed fact_shape foreign_bank_in_body invalid_auth_accepted '
    'invalid_timestamp_not_rejected list_identity list_incomplete list_shape mcp_envelope '
    'mcp_http_failure mcp_notification_failed mcp_session_missing mcp_session_scope merge_facts_changed '
    'missing_delete_not_404 missing_read_not_404 mutation_route_rejected mutation_target_rejected '
    'operation_child_missing operation_children_shape operation_error operation_failed '
    'operation_identity operation_not_discovered operation_parent_missing other_bank_counts_changed '
    'poll_time_bound profile_drift read_status read_target_rejected response_too_large '
    'restore_config_changed restore_facts_changed restored_history_missing restored_webhooks_changed '
    'retain_bound sse_response_unmeasured submit_unmeasured target_rejected term_missing '
    'tool_arguments_unmeasured tool_catalog_incomplete tool_catalog_shape tool_filter_call_failed '
    'tool_filter_list_failed tool_names_unmeasured total_time_bound transfer_route_unmeasured '
    'transport_uncertain uncertain_work_keep_bank unresolved_mutation write_error_bound '
    'write_gap_bound').split())
SUPPORTED_TOOLS = frozenset('retain sync_retain recall reflect list_mental_models get_mental_model '
    'create_mental_model update_mental_model delete_mental_model refresh_mental_model clear_mental_model '
    'list_directives create_directive delete_directive list_memories get_memory update_memory invalidate_memory '
    'list_documents get_document delete_document list_operations get_operation cancel_operation list_tags '
    'get_bank update_bank delete_bank clear_memories get_knowledge_base_tree search_knowledge_base '
    'get_knowledge_page create_knowledge_folder create_knowledge_page update_knowledge_node delete_knowledge_node'.split())
OBSERVED_KEYS = frozenset('api_version paths items banks total limit offset bank_id id name '
    'config overrides enable_observations enable_auto_consolidation retain_extraction_mode retain_custom_instructions '
    'mcp_enabled_tools operations status result_metadata child_operations operation_id error_message error success '
    'document_id original_text text entities tags fact_type occurred_start occurred_end provider model input_tokens '
    'output_tokens total_tokens cached_tokens thoughts_tokens scope jsonrpc result content structuredContent isError '
    'tools inputSchema properties required type additionalProperties annotations description download_url storage_key '
    'byte_size filename webhooks params arguments method updates async query'.split())
STEPS = ('E1', 'E2', 'E4', 'E5', 'E6')
CONFIG = json.loads((ROOT / 'memory/engineering.json').read_bytes())
encode = lambda value: json.dumps(value, ensure_ascii=False, sort_keys=True).encode()
load = lambda raw: json.loads(raw, object_pairs_hook=P.CHECK.unique_pairs)


def keys(value):
    """Known protocol keys stay readable; dynamic server keys become hashes."""
    if isinstance(value, dict):
        return {k if k in OBSERVED_KEYS else 'sha256:' + P.digest(str(k).encode()): keys(v) for k, v in value.items()}
    if isinstance(value, list):
        return [keys(value[0])] if value else []
    return type(value).__name__


def strings(value):
    """Walk decoded JSON keys and string values, never serialized escape sequences."""
    if isinstance(value, dict):
        for k, v in value.items():
            yield k
            yield from strings(v)
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, str):
        yield value


def credential_matches(raw, secret):
    prefixes = [secret[:n] for n in range(min(8, len(secret)), len(secret) + 1)]
    decoded = list(strings(load(raw)))
    return sum(raw.count(p.encode()) + sum(s.count(p) for s in decoded) for p in prefixes)


def private_fallback(value, secret):
    raw = encode(value)
    # A very short credential can collide even with fixed diagnostic keys.
    if secret and credential_matches(raw, secret):
        return b'1' if secret == '0' else b'0'
    return raw


@contextlib.contextmanager
def http_budget(seconds):
    """Bound the whole response, including a peer that trickles bytes forever."""
    def expired(*_):
        raise P.Stop('transport_uncertain')
    previous = signal.signal(signal.SIGALRM, expired)
    started = time.monotonic()
    prior = signal.getitimer(signal.ITIMER_REAL)
    signal.setitimer(signal.ITIMER_REAL, min(seconds, prior[0]) if prior[0] else seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        if prior[0]:
            signal.setitimer(signal.ITIMER_REAL, max(.000001, prior[0] - (time.monotonic() - started)), prior[1])


class Wire:
    """Reuse publisher opener (proxy-free, NoRedirect), credential and error boundary.

    Its fixed-bank request() cannot address bank-list/MCP/download/ZIP routes.
    This small transport extension handles those formats and retains HTTP status.
    No URL/credential ever enters exception messages or evidence.
    """
    def __init__(self, secret):
        self.client = P.Client(secret)

    def exchange(self, method, path, body=None, session=None, invalid=False, content_type=None, timeout=15):
        headers = {'Authorization': 'Bearer ' + ('invalid-exercise-auth' if invalid else self.client._secret),
                   'Content-Type': content_type or 'application/json',
                   'Accept': 'application/json, text/event-stream'}
        if session:
            headers['mcp-session-id'] = session
        raw = body if isinstance(body, bytes) else None if body is None else encode(body)
        request = P.urllib.request.Request(P.ORIGIN + path, method=method, data=raw, headers=headers)
        try:
            with http_budget(timeout), self.client.opener.open(request, timeout=timeout) as response:
                status, raw = response.status, response.read(MAX_BYTES + 1)
                sid = response.headers.get('mcp-session-id')
        except urllib.error.HTTPError as error:
            # Error bodies may contain private text; do not read them.
            status = error.code
            error.close()
            return status, b'', None
        except (OSError, P.http.client.HTTPException, ValueError):
            raise P.Stop('transport_uncertain') from None
        P.require(len(raw) <= MAX_BYTES, 'response_too_large')
        return status, raw, sid


class Exercise:
    def __init__(self, bank, wire, clock=time.monotonic, sleep=time.sleep):
        P.require(bool(ID.fullmatch(bank)), 'bank_id_rejected')
        self.bank, self.wire, self.clock, self.sleep = bank, wire, clock, sleep
        self.base, self.mcp = BANKS + '/' + P.segment(bank), '/mcp/' + P.segment(bank) + '/'
        self.start = clock()
        self.last_write = None
        self.retains = self.write_errors = 0
        self.attempted = self.cleaning = self.uncertain = False
        self.before = None
        self.active = set()
        self.catalog, self.denials, self.completed = {}, {}, {}
        self.interrupted = False
        self.full_policies = set()
        self.tool_policy = None
        self.sessions = []
        self.marker = 'InventedConformance' + bank[-8:]
        self.docs = {}
        self.step = 'preflight'
        self.evidence = {'schema': 'memory-exercise/1', 'bank': bank, 'steps': {}, 'requests': [],
                         'cleanup': 'not_run', 'limits': {'retains': 40, 'write_gap_s': 1,
                         'poll_s': 120, 'total_s': 900, 'consecutive_write_errors': 3},
                         'known_limits': ['host_death', 'unobservable_delayed_finality',
                         'embedding_weights_and_caps', 'unobserved_webhook_effects', 'E3_separate']}

    def observed(self, condition, code):
        if not condition and self.evidence['requests']:
            self.evidence['requests'][-1]['verdict'] = 'failed'
        P.require(condition, code)

    def interrupt(self):
        self.interrupted = True  # Signal handlers never interrupt the cleanup boundary.

    def no_foreign(self, value, code):
        P.require(not any(bank in text for text in strings(value) for bank in self.before or {}), code)

    def bound(self, retaining=False):
        if self.interrupted and not self.cleaning:
            raise KeyboardInterrupt
        P.require(self.clock() < getattr(self, 'poll_until', float('inf')), 'poll_time_bound')
        # Reserve the last 120 seconds for resolution/deletion/absence proof.
        P.require(self.clock() - self.start < (900 if self.cleaning else 780), 'total_time_bound')
        if not self.cleaning:
            P.require(self.write_errors < 3, 'write_error_bound')
            P.require(not retaining or self.retains < 40, 'retain_bound')

    def change(self, method, route='', body=None, query=None, **kwargs):
        """The sole builder of ALL changing requests, including scoped MCP calls."""
        P.require(route == '@mcp' or route in ('', '/config', '/memories', '/transfer/export', '/transfer/import', '/documents/missing-invented'),
                  'mutation_route_rejected')
        self.bound(retaining=route == '/memories' or kwargs.get('retaining', False))
        P.require(not self.uncertain, 'unresolved_mutation')
        if not isinstance(body, bytes):
            self.no_foreign(body, 'foreign_bank_in_body')
        path = self.mcp if route == '@mcp' else self.base + route
        query = dict(query or {})
        if route == '/transfer/import':
            query['target_bank_id'] = self.bank  # Never take a target from an archive/answer.
        for name in ('bank_id', 'target_bank_id'):
            P.require(name not in query or query[name] == self.bank, 'mutation_target_rejected')
        if query:
            path += '?' + urllib.parse.urlencode(query, doseq=True)
        if self.last_write is not None:
            self.sleep(max(0, 1 - (self.clock() - self.last_write)))
            P.require(self.clock() - self.last_write >= 1, 'write_gap_bound')
        self.bound(retaining=route == '/memories' or kwargs.get('retaining', False))
        self.last_write = self.clock()
        if route == '/memories' or kwargs.pop('retaining', False):
            self.retains += 1
        if method == 'PUT' or route == '/transfer/import':
            self.evidence['cleanup'] = 'incomplete'
            self.attempted = True  # Before dispatch: an exception may hide successful creation.
        dispatched_operation = body.get('operation_id') if route == '/memories' and isinstance(body, dict) else None
        if isinstance(dispatched_operation, str):
            self.active.add(dispatched_operation)
        requests_before = len(self.evidence['requests'])
        try:
            result = self.call(method, path, body, **kwargs)
        except BaseException:
            if len(self.evidence['requests']) > requests_before and self.evidence['requests'][-1]['dispatched']:
                self.uncertain = True
            else:
                self.active.discard(dispatched_operation)
            raise
        status = result[0]
        if 400 <= status < 500 and isinstance(dispatched_operation, str):
            self.active.discard(dispatched_operation)
        self.write_errors = self.write_errors + 1 if status >= 400 else 0
        if status >= 500 or status in (301, 302, 303, 307, 308):
            self.uncertain = True
        return result

    def call(self, method, path, body=None, **kwargs):
        self.bound()
        # GETs are constructed locally too; only one owned-operation download is admitted below.
        allowed = path.split('?', 1)[0]
        P.require(allowed in ('/version', '/openapi.json', BANKS, self.base, self.mcp)
                  or allowed.startswith(self.base + '/')
                  or (method == 'GET' and allowed == getattr(self, 'download', None)), 'read_target_rejected')
        P.require(not any(x in path for x in ('\r', '\n', '#', '\\'))
                  and not any(x in ('.', '..') for x in allowed.split('/')), 'target_rejected')
        started = self.clock()
        row = {'step': self.step, 'method': method, 'path': path.replace(self.bank, '<bank>'),
               'request_keys': keys(body), 'basis': self.basis(method, allowed),
               'status': None, 'dispatched': False, 'answered': False, 'verdict': 'failed'}
        if allowed == self.mcp and isinstance(body, dict):
            row['rpc_method'] = body.get('method')
            if body.get('method') == 'tools/call':
                row['tool'] = body['params']['name']  # Already checked against SUPPORTED_TOOLS.
        if kwargs.get('content_type', '').startswith('multipart/form-data;'):
            row['request_keys'] = {'file': 'bytes'}
        # Queries contain only invented ids or fixed options. Download tokens never leave memory.
        if allowed.startswith(self.base + '/operations/'):
            row['path'] = BANKS + '/<bank>/operations/<operation>' + ('?include_payload=true' if '?' in path else '')
        if allowed == getattr(self, 'download', None):
            row['path'] = '/v1/default/files/download/<key>'
        self.evidence['requests'].append(row)
        try:
            row['dispatched'] = True
            status, raw, sid = self.wire.exchange(method, path, body,
                 timeout=min(15, max(.001, self.start + 900 - self.clock()),
                             max(.001, getattr(self, 'poll_until', float('inf')) - self.clock())), **kwargs)
            row.update(answered=True, status=status, bytes=len(raw), sha256=P.digest(raw), seconds=self.clock() - started)
            if raw.startswith(b'PK'):
                data = raw
            elif raw:
                try:
                    data = load(raw)
                except (ValueError, UnicodeError):
                    # SSE: match response id; multiline data events are joined.
                    messages = []
                    for event in raw.decode().replace('\r\n', '\n').split('\n\n'):
                        payload = '\n'.join(x[5:].lstrip() for x in event.splitlines() if x.startswith('data:'))
                        if payload:
                            messages.append(load(payload))
                    matches = [x for x in messages if isinstance(x, dict) and x.get('id') == (body or {}).get('id')]
                    P.require(len(matches) == 1, 'sse_response_unmeasured')
                    data = matches[0]
            else:
                data = {}
            row['answer_shape'] = keys(data)
            row['verdict'] = 'failed' if status >= 500 or (method != 'GET' and not 200 <= status < 300) else 'measured'
            return status, data, sid
        finally:
            row['seconds'] = self.clock() - started

    def basis(self, method, path):
        if '/transfer/' in path or '/files/download/' in path or path == '/openapi.json' or path.endswith('/webhooks'):
            return 'UQ1_E1_measurement'
        if path == self.base:
            return 'UQ1_E1_lifecycle_measurement'
        if path == self.mcp:
            return 'U1_initialize_SSE;UQ2_E2_filter_measurement'
        if path.endswith('/config'):
            return 'Profile_480_517;UQ2_E2_allowlist_measurement;UQ6_E6_drift'
        if '/operations' in path:
            return 'U14_018_116_parent_child'
        if '/memories/recall' in path:
            return 'U10_062_143;ancestor_tags_489_515'
        if path.endswith('/memories'):
            return 'U3_010;F4_429_if_invalid_timestamp;UQ5_E5_inputs'
        if '/documents' in path or '/memories/list' in path:
            return 'U4_016_025;U8_030_033;U5_125_127_if_absent'
        if '/llm-requests' in path:
            return 'Cost_Profile_538'
        if '/knowledge-base/search' in path or path == BANKS:
            return 'U8;cleanup_153_157_455_464'
        return 'Profile_480_version'

    def get(self, route, query=None):
        path = route if route.startswith('/v1/') or route in ('/version', '/openapi.json') else self.base + route
        if query:
            path += '?' + urllib.parse.urlencode(query, doseq=True)
        status, data, _ = self.call('GET', path)
        P.require(status == 200, 'read_status')
        return data

    def pages(self, route, key='items', query=None):
        offset, total, items, seen = 0, None, [], set()
        while True:
            page = self.get(route, dict(query or {}, offset=offset, limit=100))
            P.require(isinstance(page, dict) and isinstance(page.get(key), list), 'list_shape')
            n, batch = page.get('total'), page[key]
            P.require(type(n) is int and n >= 0 and page.get('offset') == offset
                      and type(page.get('limit')) is int and 0 < page['limit'] <= 100
                      and page.get('total_is_estimate', False) is False
                      and (total is None or total == n) and len(batch) <= page['limit']
                      and offset + len(batch) <= n, 'list_incomplete')
            total = n
            for item in batch:
                identity = item.get('bank_id' if key == 'banks' else 'id')
                P.require(isinstance(identity, str) and identity not in seen, 'list_identity')
                seen.add(identity)
            items.extend(batch)
            offset += len(batch)
            if offset == total:
                return items
            P.require(bool(batch), 'list_incomplete')

    def counts(self):
        banks = self.pages(BANKS, 'banks')
        P.require(all(type(x.get('fact_count')) is int and x['fact_count'] >= 0 for x in banks), 'counts_unmeasured')
        return {x['bank_id']: x['fact_count'] for x in banks}

    def preflight(self):
        self.before = self.counts()
        P.require(self.bank not in self.before, 'bank_already_present')
        self.evidence['other_banks'] = {'count': len(self.before), 'sha256': P.digest(encode(self.before))}
        version = self.get('/version')
        self.evidence['api_version_sha256'] = P.digest(encode(version))
        self.evidence['api_version'] = version.get('api_version') if version.get('api_version') == P.API_VERSION else 'unmeasured'
        schema = self.get('/openapi.json')
        self.evidence['openapi_sha256'] = P.digest(encode(schema))
        for suffix in ('/transfer/export', '/transfer/import'):
            P.require('post' in schema.get('paths', {}).get(BANKS + '/{bank_id}' + suffix, {}),
                      'transfer_route_unmeasured')

    def poll(self, refresh=False):
        if refresh:
            self.completed.clear()
        self.poll_until = min(self.clock() + 120, self.start + (900 if self.cleaning else 780))
        try:
            return self.poll_work(self.poll_until)
        finally:
            del self.poll_until

    def poll_work(self, deadline):
        while True:
            P.require(self.clock() < deadline, 'poll_time_bound')
            operations = self.pages('/operations', 'operations')
            details = {}
            for item in operations:
                oid = item['id']
                detail = self.completed.get(oid) if item.get('status') == 'completed' else None
                if detail is None or oid in self.active:
                    detail = self.get('/operations/' + P.segment(oid), {'include_payload': 'true'})
                P.require(detail.get('id') == oid, 'operation_identity')
                details[oid] = detail
            ids = set(details)
            busy = not self.active <= ids  # A late acknowledgement can precede discovery; wait, never resend.
            for detail in details.values():
                status = detail.get('status')
                P.require(status not in ('failed', 'cancelled'), 'operation_failed')
                busy |= status != 'completed'
                meta = detail.get('result_metadata') or {}
                P.require(not detail.get('error_message') and not meta.get('extraction_errors_count'), 'operation_error')
                parent = meta.get('parent_operation_id')
                P.require(parent is None or parent in ids, 'operation_parent_missing')
                children = detail.get('child_operations') or []
                P.require(isinstance(children, list), 'operation_children_shape')
                for child in children:
                    P.require(child.get('operation_id') in ids and not child.get('error_message'), 'operation_child_missing')
                    busy |= child.get('status') != 'completed'
                if meta.get('is_parent'):
                    P.require(meta.get('num_sub_batches') == len(children), 'operation_child_missing')
                if status == 'completed' and all(c.get('status') == 'completed' for c in children):
                    self.completed[detail['id']] = detail
            if not busy:
                self.active.clear()
                return details
            self.sleep(min(1, max(0, deadline - self.clock())))

    def submitted(self, answer):
        status, data, _ = answer
        if not (status in (200, 202) and isinstance(data.get('operation_id'), str)):
            self.uncertain = True
            raise P.Stop('submit_unmeasured')
        oid = data['operation_id']
        self.active.add(oid)
        return self.poll()[oid]

    def create(self, default=False):
        self.completed.clear()
        self.docs.clear()
        self.tool_policy = None
        status, _, _ = self.change('PUT', body={'enable_observations': False})
        P.require(status == 200, 'create_failed')
        if not default:
            self.configure([])

    def configure(self, tools, instruction=None):
        desired = {'enable_observations': False, 'enable_auto_consolidation': False,
                   'retain_extraction_mode': 'custom', 'retain_custom_instructions': instruction or CONFIG['retain_custom_instructions'],
                   'mcp_enabled_tools': tools}
        status, _, _ = self.change('PATCH', '/config', {'updates': desired})
        P.require(status == 200, 'config_failed')
        actual = self.get('/config').get('config', {})
        P.require(all(actual.get(k) == v for k, v in desired.items()), 'config_readback_failed')
        self.tool_policy = list(tools) if tools is not None else None
        return actual

    def rpc(self, method, params=None, session=None, retaining=False):
        payload = {'jsonrpc': '2.0', 'id': 1, 'method': method}
        if params is not None:
            payload['params'] = params
        status, answer, sid = self.change('POST', '@mcp', payload, session=session, retaining=retaining)
        P.require(status == 200, 'mcp_http_failure')
        P.require(answer.get('id') == 1 and answer.get('jsonrpc') == '2.0', 'mcp_envelope')
        return answer, sid

    def session(self):
        answer, sid = self.rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                      'clientInfo': {'name': 'memory-exercise', 'version': '1'}})
        self.observed(isinstance(answer.get('result'), dict) and 'error' not in answer, 'mcp_envelope')
        P.require(isinstance(sid, str) and bool(sid), 'mcp_session_missing')
        self.no_foreign(sid, 'mcp_session_scope')
        # Notification has no response id (202 is normal); still uses the sole mutation builder.
        status, _, _ = self.change('POST', '@mcp', {'jsonrpc': '2.0', 'method': 'notifications/initialized'}, session=sid)
        P.require(status in (200, 202, 204), 'mcp_notification_failed')
        self.sessions.append(sid)
        return sid

    def tools(self, session):
        answer, _ = self.rpc('tools/list', {}, session)
        self.observed(isinstance(answer.get('result'), dict) and 'error' not in answer, 'mcp_envelope')
        result = answer.get('result', {})
        P.require(isinstance(result.get('tools'), list) and not result.get('nextCursor'), 'tool_catalog_incomplete')
        tools = result['tools']
        P.require(all(isinstance(t.get('name'), str) for t in tools)
                  and len({t['name'] for t in tools}) == len(tools), 'tool_catalog_shape')
        P.require({t['name'] for t in tools} <= SUPPORTED_TOOLS, 'tool_names_unmeasured')
        return {t['name']: t for t in tools}

    def arguments(self, schema):
        def validate(node):
            if isinstance(node, dict):
                P.require(all('bank' not in k.lower() or k in ('bank_id', 'target_bank_id')
                              for k in node.get('properties', {})), 'tool_arguments_unmeasured')
                for value in node.values():
                    validate(value)
            elif isinstance(node, list):
                for value in node:
                    validate(value)
        validate(schema)
        def sample(s, name=''):
            if name in ('bank_id', 'target_bank_id'):
                return self.bank
            if name in ('document_id', 'mental_model_id', 'page_id', 'id'):
                return 'invented-a'
            if name == 'timestamp':
                return '2026-01-02T00:00:00Z'
            P.require('enum' not in s, 'tool_arguments_unmeasured')
            if 'anyOf' in s:
                return sample(next(x for x in s['anyOf'] if x.get('type') != 'null'), name)
            kind = s.get('type')
            if kind == 'object':
                return {k: sample(s['properties'][k], k) for k in s.get('required', [])}
            if kind == 'array':
                return [sample(s.get('items', {'type': 'string'}))]
            if kind == 'string':
                return self.marker
            if kind in ('integer', 'number'):
                return 1
            if kind == 'boolean':
                return False
            raise P.Stop('tool_arguments_unmeasured')
        return sample(schema)

    @staticmethod
    def denial(answer, name):
        # Every result is execution, including isError and an executor's missing-resource error.
        error = answer.get('error')
        if 'result' in answer or not isinstance(error, dict) or type(error.get('code')) is not int or not isinstance(error.get('message'), str):
            return None
        normalized = dict(error, message=error['message'].replace(name, '<tool>'))
        return error['code'], keys(error), P.digest(encode(normalized))

    def denied(self, answer, name, sid):
        return self.denial(answer, name) is not None and self.denial(answer, name) == self.denials[sid]

    def representatives(self):
        # First catalog member of each fixed tuple; never choose by server order or description.
        classes = {'retain': ('retain', 'sync_retain'),
                   'delete': ('delete_document', 'delete_bank', 'clear_memories'),
                   'reflect': ('reflect',),
                   'page': ('create_knowledge_page', 'delete_knowledge_node', 'update_knowledge_node')}
        chosen = {kind: next((n for n in names if n in self.catalog), None) for kind, names in classes.items()}
        self.evidence['E2_classes'] = {k: v or 'class_absent' for k, v in chosen.items()}
        return [name for name in chosen.values() if name]

    def filtering(self, old=None):
        started = self.clock()
        self.step = 'E2'
        visible_ids = set(self.docs)  # Owned originals already compared by E1; a fresh create clears them.
        document_status = 200 if 'invented-a' in self.docs else 404
        document = {'original_text': self.docs.get('invented-a')}
        if old is None:
            old = self.session()
            self.catalog.update(self.tools(old))
        representatives = self.representatives()
        for allowed in ([], ['recall', 'get_document']):
            if self.tool_policy != allowed:
                self.configure(allowed)
            for sid, fresh in ((old, False), (self.session(), True)):
                listed = self.tools(sid)
                self.observed(set(listed) == set(allowed), 'tool_filter_list_failed')
                if sid not in self.denials:
                    sentinel = 'zz_nonexistent_exercise_probe'
                    baseline, _ = self.rpc('tools/call', {'name': sentinel, 'arguments': {}}, sid)
                    self.denials[sid] = self.denial(baseline, sentinel)
                    self.observed(self.denials[sid] is not None, 'denial_shape_unmeasured')
                full = fresh and tuple(allowed) not in self.full_policies
                names = list(self.catalog) if full else representatives
                # Preserve successful scoped reads after restore, alongside the four denial classes.
                if allowed and fresh and not full:
                    names = names + list(allowed)
                for name in names:
                    try:
                        args = self.arguments(self.catalog[name]['inputSchema'])
                    except P.Stop:
                        self.evidence.setdefault('tool_probes', []).append({'tool': name, 'verdict': 'refused_unmeasured'})
                        raise
                    if name == 'recall':
                        args.update(query=self.marker, tags=['subject:task:invented'], tags_match='all_strict',
                                    types=['world', 'experience'], budget='low', max_tokens=128)
                    answer, _ = self.rpc('tools/call', {'name': name, 'arguments': args}, sid,
                                         retaining=name in ('retain', 'sync_retain'))
                    if name not in allowed:
                        self.observed(self.denied(answer, name, sid), 'tool_filter_call_failed')
                    else:
                        self.observed(not ('error' in answer or answer.get('result', {}).get('isError')), 'allowed_read_failed')
                        structured = answer.get('result', {}).get('structuredContent')
                        self.observed(isinstance(structured, dict), 'allowed_shape_unmeasured')
                        if name == 'recall':
                            results = structured.get('results')
                            self.observed(isinstance(results, list) and all(
                                r.get('document_id') in visible_ids and 'subject:task:invented' in r.get('tags', [])
                                for r in results) and (not visible_ids or bool(results)), 'allowed_scope_failed')
                        elif document_status == 200:
                            self.observed(structured.get('id') == 'invented-a' and
                                structured.get('original_text') == document.get('original_text')
                                and not structured.get('error'), 'allowed_document_failed')
                        else:
                            self.observed(document_status == 404 and bool(structured.get('error')), 'allowed_document_failed')
                if full:
                    self.full_policies.add(tuple(allowed))
        self.configure([])
        self.evidence['steps']['E2'] = {'verdict': 'measured', 'sessions': len(self.sessions),
                                       'catalog_count': len(self.catalog),
                                       'seconds': self.clock() - started + self.evidence['steps'].get('E2', {}).get('seconds', 0)}

    def retain(self, rid, text):
        want = {'content': text, 'document_id': rid, 'tags': ['subject:task:invented'], 'update_mode': 'replace'}
        oid = str(uuid.uuid4())
        result = self.change('POST', '/memories', {'items': [want], 'async': True, 'operation_id': oid})
        if result[0] not in (200, 202) or result[1].get('operation_id') != oid:
            self.uncertain = True
            raise P.Stop('submit_unmeasured')
        self.submitted(result)
        self.docs[rid] = text
        self.compare({rid: text}, exact=False)

    def compare(self, expected, exact=True):
        if exact:
            P.require({d['id'] for d in self.pages('/documents')} == set(expected), 'document_set_changed')
        snapshots, fact_ids = {}, set()
        for rid, text in expected.items():
            doc = self.get('/documents/' + P.segment(rid))
            P.require(doc.get('original_text') == text and doc.get('tags') == ['subject:task:invented'], 'document_bytes_or_tags')
            facts = self.pages('/memories/list', query={'document_id': rid})
            P.require(bool(facts) and all(f.get('document_id') == rid and f.get('tags') == doc['tags']
                       and f.get('fact_type') in ('world', 'experience') for f in facts), 'fact_readback_failed')
            P.require(all(isinstance(f.get('text'), str) and isinstance(f.get('entities'), str) for f in facts), 'fact_shape')
            self.last_facts = facts
            fact_ids.update(f['id'] for f in facts)
            canonical = [{k: f.get(k) for k in ('text', 'entities', 'tags', 'fact_type', 'occurred_start', 'occurred_end')} for f in facts]
            snapshots[rid] = P.digest(encode(sorted(canonical, key=lambda x: encode(x))))
        if exact:
            self.observed({f['id'] for f in self.pages('/memories/list')} == fact_ids, 'fact_set_changed')
        return snapshots

    def export(self, expected, subset=False):
        query = {'document_id': list(expected)} if subset else {
                 'include_data': 'true', 'include_bank_config': 'true', 'include_history': 'true'}
        detail = self.submitted(self.change('POST', '/transfer/export', query=query))
        meta = detail.get('result_metadata') or {}
        url = urllib.parse.urlsplit(meta.get('download_url', ''))
        P.require(not url.scheme and not url.netloc and not url.query and not url.fragment
                  and url.path.startswith('/v1/default/files/download/'), 'download_target_rejected')
        self.download = url.path
        status, raw, _ = self.call('GET', self.download)
        P.require(status == 200 and isinstance(raw, bytes), 'archive_unmeasured')
        found, entries = set(), []
        def inspect(value):
            self.no_foreign(value, 'archive_foreign_bank')
            if isinstance(value, dict):
                for k in ('bank_id', 'source_bank_id', 'target_bank_id'):
                    P.require(k not in value or value[k] == self.bank, 'archive_foreign_bank')
                text = value.get('original_text')
                rid = value.get('document_id', value.get('id'))
                if text is not None:
                    P.require(rid in expected and text == expected[rid], 'archive_document_mismatch')
                    found.add(rid)
                for v in value.values():
                    inspect(v)
            elif isinstance(value, list):
                for v in value:
                    inspect(v)
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            P.require(sum(x.file_size for x in archive.infolist()) <= MAX_BYTES, 'archive_too_large')
            for entry in archive.infolist():
                self.no_foreign(entry.filename, 'archive_foreign_bank')
                entries.append({'name_sha256': P.digest(entry.filename.encode()), 'bytes': entry.file_size})
                with archive.open(entry) as source:
                    value = source.read(MAX_BYTES + 1)  # zipfile checks CRC at EOF; the read is capped.
                P.require(len(value) <= MAX_BYTES, 'archive_too_large')
                try:
                    values = [load(value)]
                except (ValueError, UnicodeError):
                    try:
                        values = [load(line) for line in value.splitlines() if line.strip()]
                    except (ValueError, UnicodeError):
                        raise P.Stop('archive_unmeasured') from None
                P.require(bool(values), 'archive_unmeasured')
                for value in values:
                    inspect(value)
        P.require(found == set(expected), 'archive_documents_unmeasured')
        self.evidence.setdefault('archives', []).append({'subset': subset, 'sha256': P.digest(raw),
                 'bytes': len(raw), 'entries': entries, 'documents': len(found),
                 'flags': sorted(query), 'verdict': 'measured', 'answer_shape': keys(meta)})
        return raw

    def restore(self, raw, merge=False):
        if not merge:
            self.completed.clear()  # Restored operation ids must be read again, never accepted from the cache.
        boundary = 'exercise-' + uuid.uuid4().hex
        body = ('--' + boundary + '\r\nContent-Disposition: form-data; name="file"; filename="exercise.zip"\r\n'
                'Content-Type: application/zip\r\n\r\n').encode() + raw + ('\r\n--' + boundary + '--\r\n').encode()
        result = self.change('POST', '/transfer/import', body,
                  query={'mode': 'merge' if merge else 'restore', 'document_conflict': 'replace',
                         'include_data': 'true', 'include_bank_config': 'true', 'include_history': 'true'},
                  content_type='multipart/form-data; boundary=' + boundary)
        self.submitted(result)

    def absence(self):
        absent = self.bank not in self.counts()
        status, _, _ = self.call('GET', self.base + '/documents/invented-a')
        search, _, _ = self.call('GET', self.base + '/knowledge-base/search?' + urllib.parse.urlencode({'q': self.marker}))
        P.require(absent and status == 404 and search == 404, 'absence_unproven')

    def delete(self):
        self.poll()
        status, _, _ = self.change('DELETE')
        P.require(status == 200, 'delete_failed')
        self.absence()

    def e1(self):
        self.create(default=True)
        # Capture defaults before the first policy update and before any record.
        old = self.session()
        self.catalog = self.tools(old)
        P.require({'recall', 'get_document'} <= set(self.catalog), 'default_catalog_unmeasured')
        self.filtering(old)
        self.step = 'E1'
        originals = {'invented-a': self.marker + ' subject A uses ExampleID-A when its gate is closed.',
                     'invented-b': self.marker + ' subject B uses ExampleID-B when its gate is open.'}
        for rid, text in originals.items():
            self.retain(rid, text)
        before = self.compare(originals)
        profile = self.get('/config')
        original_history = set(self.poll())
        hooks = self.get('/webhooks')  # UQ1: observe only; never configure an external receiver.
        full = self.export(originals)
        self.export({'invented-a': originals['invented-a']}, True)
        changed = originals['invented-a'].replace('closed', 'sealed')
        self.retain('invented-a', changed)
        changed_facts = self.compare({'invented-a': changed}, False)
        subset = self.export({'invented-a': changed}, True)
        self.delete()
        self.create()
        self.filtering(old)  # Old session survives the same-id lifecycle, or E2 fails.
        self.step = 'E1'
        self.delete()  # Native restore requires the target to be absent.
        self.restore(full)
        self.docs = dict(originals)
        P.require(self.compare(originals) == before, 'restore_facts_changed')
        P.require(self.get('/config') == profile, 'restore_config_changed')
        self.restore(subset, merge=True)
        self.docs['invented-a'] = changed
        P.require(self.compare({'invented-a': changed}, False) == changed_facts, 'merge_facts_changed')
        self.compare({'invented-a': changed, 'invented-b': originals['invented-b']})
        history = self.poll(refresh=True)
        missing_history = len(original_history - set(history))
        self.observed(missing_history == 0, 'restored_history_missing')
        self.observed(self.get('/webhooks') == hooks, 'restored_webhooks_changed')
        self.evidence['steps']['E1'] = {'verdict': 'measured', 'history_count': len(history), 'missing_original_history': missing_history,
                                       'webhooks_shape': keys(hooks), 'webhook_effects': 'not_observed',
                                       'history_shape': keys(list(history.values()))}
        self.filtering(old)

    def e4(self):
        status, _, _ = self.call('GET', self.base + '/config', invalid=True)
        P.require(status == 401, 'invalid_auth_accepted')
        cases = []
        for route in ('/documents/missing-invented', '/documents/missing-invented/chunks'):
            code, _, _ = self.call('GET', self.base + route)
            P.require(code == 404, 'missing_read_not_404')
            cases.append(code)
        code, _, _ = self.change('DELETE', '/documents/missing-invented')
        self.observed(code == 404, 'missing_delete_not_404')
        cases.append(code)
        code, _, _ = self.change('POST', '/memories', {'items': [{'content': self.marker,
                    'timestamp': 'not-a-date'}], 'async': True, 'operation_id': str(uuid.uuid4())})
        P.require(code == 422, 'invalid_timestamp_not_rejected')
        self.configure([])  # Successful measured write; do not manufacture a three-error stop here.
        self.evidence['steps']['E4'] = {'verdict': 'measured', 'errors': [status] + cases + [code],
                                       'redirect': 'offline_only', 'corrupt_pdf': 'not_run_unbounded_retry'}

    def e5(self):
        cases = [('identifier', 'ExampleID.C5-A'), ('alias', '\uac00\ub78c Garam'),
                 ('dated', '2026-01-02'), ('boundary-4095', 'BoundaryID'), ('boundary-4096', 'BoundaryID')]
        results = []
        for label, term in cases:
            text = self.marker + ' subject ' + label + ' uses ' + term + ' only when the invented gate is closed.'
            if label.startswith('boundary-'):
                n = int(label.split('-')[1])
                text += ' ' + ('x' * (n - len(text.encode()) - 1))
            rid = 'invented-' + label
            self.retain(rid, text)
            facts = self.last_facts  # The exhaustive read just performed by retain/compare.
            terms = term.split() if label == 'alias' else [term]
            P.require(all(any(t in str(f.get('text', '')) or t in str(f.get('entities', '')) for f in facts)
                          for t in terms), 'term_missing')
            results.append({'case': label, 'bytes': len(text.encode()), 'sha256': P.digest(text.encode()),
                            'facts': len(facts), 'terms': len(terms),
                            'dated_facts': sum(f.get('occurred_start') is not None for f in facts), 'fact_entity_sha256': P.digest(encode(facts))})
        self.evidence['steps']['E5'] = {'verdict': 'measured', 'cases': results,
             'subject_condition_judgment': 'not_run', 'administrative_assertion_judgment': 'not_run'}
        # Meaning is judged by the Lead; literal survival cannot certify it.
        self.evidence['steps']['E5']['semantic_judgment'] = 'Lead_required_before_publication'

    def e6(self):
        current = self.get('/config').get('config', {})
        expected = CONFIG['retain_custom_instructions']
        P.require(current.get('retain_custom_instructions') == expected and current.get('retain_extraction_mode') == 'custom', 'profile_drift')
        baseline = P.digest(expected.encode())
        self.configure([], expected + ' Invented deliberate canary drift.')
        drift = self.get('/config').get('config', {})
        P.require(P.digest(str(drift.get('retain_custom_instructions')).encode()) != baseline, 'canary_missed_drift')
        self.configure([])
        questions = []
        for question in ('ExampleID-A', 'ExampleID-B', self.marker):
            started = self.clock()
            status, answer, _ = self.call(
                'POST', self.base + '/memories/recall', {'query': question, 'types': ['world', 'experience'],
                'budget': 'low', 'max_tokens': 128, 'tags': ['subject:task:invented'], 'tags_match': 'all_strict'})
            P.require(status == 200 and isinstance(answer.get('results'), list), 'canary_read_failed')
            questions.append({'query_sha256': P.digest(question.encode()), 'seconds': self.clock() - started,
                              'bytes': len(encode(answer)), 'count': len(answer['results'])})
        traces = self.pages('/llm-requests')
        usage = []
        for trace in traces:
            usage.append({k: trace[k] for k in ('input_tokens', 'output_tokens', 'total_tokens', 'cached_tokens', 'thoughts_tokens')
                          if type(trace.get(k)) is int and trace[k] >= 0})
        self.evidence['steps']['E6'] = {'verdict': 'measured', 'instruction_sha256': baseline,
             'mode_sha256': P.digest(encode(current.get('retain_extraction_mode'))), 'questions': questions,
             'reported_usage': usage, 'profile_sha256': sorted({P.digest(encode([t.get('provider'), t.get('model')])) for t in traces}),
             'usage_coverage': 'reported_only', 'canary_drift_detected': True}

    def cleanup(self):
        self.cleaning, self.step = True, 'cleanup'
        self.evidence['cleanup'] = 'incomplete'
        proofs = {'owned_bank_absent': 'unknown', 'other_counts_equal': 'unknown'}
        self.evidence['cleanup_proofs'] = proofs
        try:
            # No competing mutation after a lost acknowledgement; elapsed time is not finality.
            if self.uncertain:
                self.poll()  # Read-only discovery, even when a delayed effect cannot be proven final.
            P.require(not self.uncertain, 'uncertain_work_keep_bank')
            if self.bank in self.counts():
                self.delete()
            else:
                self.absence()
            after = self.counts()  # The LAST cleanup read; no owned-bank reappearance is dropped.
            proofs['owned_bank_absent'] = self.bank not in after
            P.require(proofs['owned_bank_absent'], 'absence_unproven')
            other = {k: v for k, v in after.items() if k != self.bank}
            proofs['other_counts_equal'] = other == self.before
            comparison = {P.digest(k.encode()): ('new' if k not in self.before else 'missing' if k not in other
                          else 'grew' if other[k] > self.before[k] else 'shrank' if other[k] < self.before[k]
                          else 'unchanged') for k in set(other) | set(self.before)}
            self.evidence['other_bank_comparison'] = comparison
            proofs['other_count_differences'] = sum(v != 'unchanged' for v in comparison.values())
            if any(v in ('grew', 'new') for v in comparison.values()):
                self.evidence['other_bank_drift'] = 'unattributed'
            P.require(not any(v in ('shrank', 'missing') for v in comparison.values()), 'other_bank_counts_changed')
            self.evidence['cleanup'] = 'proven'
        except BaseException as error:
            self.evidence['cleanup_failure'] = self.failure(error)
            self.evidence['cleanup_remaining'] = {'owned_bank': 'absent' if proofs['owned_bank_absent'] is True else 'unproven',
                                                   'other_counts_equal': proofs['other_counts_equal'], 'active_operations': len(self.active),
                                                   'uncertain_mutation': self.uncertain}

    @staticmethod
    def failure(error):
        # Only exact exception class + a closed lexical code may be emitted.
        if type(error) is P.Stop and str(error) in FAILURE_CODES:
            return str(error)
        return 'interrupted' if isinstance(error, (KeyboardInterrupt, SystemExit)) else 'local_or_transport_failure'

    def run(self):
        code = 3
        try:
            self.preflight()
            for name, action in [('E1', self.e1), ('E4', self.e4), ('E5', self.e5), ('E6', self.e6)]:
                self.step = name
                started = self.clock()
                try:
                    action()
                finally:
                    self.evidence['steps'].setdefault(name, {'verdict': 'failed'})['seconds'] = self.clock() - started
            code = 0 if all(self.evidence['steps'].get(s, {}).get('verdict') == 'measured' for s in STEPS) else 3
        except BaseException as error:
            # Never serialize exception text: even fake transports can include credentials/bodies.
            self.evidence['steps'][self.step] = {'verdict': 'refused_unmeasured' if self.failure(error).endswith('unmeasured') else 'failed',
                                                'failure': self.failure(error),
                                                'interrupted': isinstance(error, (KeyboardInterrupt, SystemExit))}
            if not self.attempted:
                code = 2 if self.step == 'preflight' else 3
        finally:
            if self.attempted:
                self.cleanup()
            for step in STEPS:
                self.evidence['steps'].setdefault(step, {'verdict': 'not_run'})
            self.evidence['elapsed_s'] = self.clock() - self.start
            self.evidence['retain_attempts'] = self.retains
        self.evidence['interrupted'] = self.interrupted
        return 4 if self.attempted and self.evidence['cleanup'] != 'proven' else 3 if self.interrupted else code


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bank', required=True)
    parser.add_argument('--credential-fd', required=True, type=int)
    parser.add_argument('--evidence', required=True, type=Path)
    args = parser.parse_args(argv)
    fd, exercise, secret = None, None, None
    try:
        P.require(bool(ID.fullmatch(args.bank)), 'bank_id_rejected')
        secret = P.credential(args.credential_fd)
        fd = os.open(args.evidence, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        os.fchmod(fd, 0o600)
        exercise = Exercise(args.bank, Wire(secret))
        def interrupted(*_):
            exercise.interrupt()
        with contextlib.ExitStack() as stack:
            for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGQUIT):
                previous = signal.signal(sig, interrupted)
                stack.callback(signal.signal, sig, previous)
            with P.locked():
                code = exercise.run()
            exercise.evidence['credential_prefix_matches'] = 0
            raw = encode(exercise.evidence)
            matches = credential_matches(raw, secret)
            # On a collision do not write the unsafe evidence, even when a server reflected a secret.
            if matches:
                raw = private_fallback({'cleanup': exercise.evidence['cleanup'], 'failure': 'evidence_redaction_failed',
                                        'credential_prefix_matches': matches}, secret)
                code = 4 if exercise.evidence['cleanup'] != 'proven' else 3
            with os.fdopen(fd, 'wb') as target:
                fd = None
                target.write(raw + b'\n')
            summary = encode({'exit': code, 'cleanup': exercise.evidence['cleanup'], 'credential_prefix_matches': matches})
            if not credential_matches(summary, secret):
                print(summary.decode())
            return code
    except BaseException:
        if fd is not None:
            try:
                os.write(fd, private_fallback({'cleanup': exercise.evidence['cleanup'] if exercise else 'not_run',
                                              'failure': 'input_or_local_precondition'}, secret) + b'\n')
            except OSError:
                pass
        message = 'exercise_refused@scripts/exercise-memory.py:1: input_or_local_precondition'
        if not secret or not credential_matches(encode(message), secret):
            print(message)
        return (4 if exercise.evidence['cleanup'] != 'proven' else 3) if exercise and exercise.attempted else 2
    finally:
        if fd is not None:
            os.close(fd)


if __name__ == '__main__':
    sys.exit(main())
