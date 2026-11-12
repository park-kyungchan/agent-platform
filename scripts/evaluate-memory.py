#!/usr/bin/env python3
"""Collect fixed-bank evidence for human judgment; never assign answer verdicts."""
import argparse
import datetime
import contextlib
import io
import importlib.util
import json
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('publisher', ROOT / 'scripts/publish-memory.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
FORMS, ROUTES = ('en', 'ko', 'formed'), ('recall', 'document')
IDS = [f'R{i:02}' for i in range(1, 36)] + [f'N{i:02}' for i in range(1, 11)]
CANARY = ['R05', 'R09', 'R23', 'R27', 'R30', 'N07']
VERDICTS = ('supported', 'partial', 'stale', 'missing')
RESULT_FIELDS = {'id', 'form', 'route', 'verdict', 'current_count',
                 'evidence_sha256', 'expectation_reverified'}
load = lambda raw: json.loads(raw, object_pairs_hook=P.CHECK.unique_pairs)
encode = lambda value: (json.dumps(value, ensure_ascii=False, sort_keys=True) + '\n').encode()


def corpus(raw):
    data = load(raw)
    questions = data['questions']
    P.require(data['schema'] == 'd2-question-input/1' and data['canary'] == CANARY
              and [q['id'] for q in questions] == IDS, 'corpus_identities')
    for q in questions:
        P.require(set(q['forms']) == set(FORMS) and all(P.CHECK.nonempty(v) for v in q['forms'].values())
                  and q['class'] == 'engineering' and q['status'] in ('verified', 'answer_changed', 'source_missing')
                  and set(q['expectation']) == {'literal', 'subject', 'condition'}
                  and isinstance(q['expectation']['literal'], list)
                  and all(isinstance(v, str) for v in q['expectation']['literal'])
                  and all(P.CHECK.nonempty(q['expectation'][k]) for k in ('subject', 'condition'))
                  and bool(q['sources']), 'corpus_question')
        for s in q['sources']:
            P.require(s['kind'] in ('file', 'decision', 'command', 'git', 'github'), 'corpus_reference')
            if s['kind'] == 'file':
                P.require(P.CHECK.nonempty(s.get('path')) and P.CHECK.nonempty(s.get('anchor'))
                          and ((s.get('volatile') is True and 'sha256' not in s)
                               or (not s.get('volatile') and isinstance(s.get('sha256'), str)
                                   and bool(P.re.fullmatch('[0-9a-f]{64}', s['sha256'])))), 'corpus_file')
    return data


def source_pins(question):
    pins = []
    for source in question['sources']:
        item = dict(source, pin='reverify', present='unknown', sha256_now='unknown')
        if source['kind'] == 'file':
            try:
                raw = P.CHECK.regular_bytes(Path(source['path']))
                item.update(present=True, sha256_now=P.digest(raw))
                item['pin'] = ('reverify' if source.get('volatile') else
                               'equal' if item['sha256_now'] == source['sha256'] else 'source_changed')
            except (OSError, ValueError):
                item.update(present=False, pin='missing')
        pins.append(item)
    return pins


def recall(client, query, selector, pin):
    start = time.monotonic()
    # Publisher retract: HC U10/062,143; ancestor tags 489,492-515.
    answer = client.request('POST', P.B + '/memories/recall', dict(
        query=query, types=['world', 'experience'], budget='low', max_tokens=512,
        include={'entities': None, 'chunks': {'max_tokens': 512}}, **selector))
    items = answer.get('results')
    P.require(isinstance(items, list), 'recall_shape')
    for item in items[:5]:
        want = pin['expected'].get(item.get('document_id'))
        P.require(want is not None and all(t in want['tags'] for t in selector['tags']), 'wrong_scope_or_private')
        P.require(isinstance(item.get('text'), str), 'recall_shape')
    return items[:5], dict(native_returned_count=len(items), latency_ms=(time.monotonic()-start)*1000,
                           response_bytes=len(encode(answer)), usage=answer.get('usage', 'unknown'))


def gather(client, questions, pin, selector, document_ids):
    slots = []
    for question in questions:
        for form, query in question['forms'].items():
            items, metrics = recall(client, query, selector, pin)
            slots.append(dict(id=question['id'], form=form, route='recall', items=items,
                              returned_count=len(items), verdict=None, current_count=None, **metrics))
            # Only returned candidates and caller-named ids; expectations never feed this list.
            ids = list(dict.fromkeys([x['document_id'] for x in items] + document_ids))
            documents, start = [], time.monotonic()
            for rid in ids:
                P.require(rid in pin['expected'], 'document_not_admitted')
                doc = client.request('GET', P.B + '/documents/' + P.segment(rid))
                P.require(doc.get('id') == rid and isinstance(doc.get('original_text'), str)
                          and P.digest(doc['original_text'].encode()) == pin['expected'][rid]['hash'], 'document_bytes')
                documents.append(doc)
            slots.append(dict(id=question['id'], form=form, route='document', items=documents,
                              returned_count=len(documents), verdict=None, current_count=None, latency_ms=(time.monotonic()-start)*1000,
                              response_bytes=len(encode(documents)), usage='unknown'))
    return slots


def profile(client, pin):
    instruction = P.profile(client, pin['config'])
    return dict(api_version=P.API_VERSION, extraction_mode='custom', bank_instruction_sha256=instruction,
                publisher_instruction_sha256=P.digest(pin['config']['retain_custom_instructions'].encode()),
                subjects_sha256=P.digest(pin['subjects_raw']), config_sha256=P.digest(pin['config_raw']))


def due_count(commit):
    """Call the checker's on-demand CLI function; preserve the caller's argv/output."""
    argv, output = sys.argv, io.StringIO()
    try:
        sys.argv = ['check-memory-records.py', '--commit', commit, '--due',
                    datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d')]
        with contextlib.redirect_stdout(output):
            P.require(P.CHECK.main() == 0, 'source_invalid')
        return len(load(output.getvalue()))
    except SystemExit:
        raise P.Stop('checker_argument_failure') from None
    finally:
        sys.argv = argv


def collect(client, pin, data, raw, selector, document_ids, held_fd=None, evidence_fd=None):
    P.require(held_fd is None or evidence_fd is not None, 'held_out_evidence_fd_required')
    held = b'' if held_fd is None else read_fd(held_fd)
    questions = None if held_fd is None else load(held)['questions']
    P.require(questions is None or bool(questions), 'held_out_empty')
    if questions is not None:
        unique_questions(questions)
    result = dict(C=pin['commit'], observed_at=datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'), corpus_sha256=P.digest(raw), profile=profile(client, pin),
                  payload_sha256={rid: w['hash'] for rid, w in pin['expected'].items()},
                  source_pins={q['id']: source_pins(q) for q in data['questions']})
    P.quiescent(client)
    P.empty_derived(client)
    _, records, _, errors = P.CHECK.load(Path('memory/records'), Path('memory/subjects.json'),
                                        commit=pin['commit'], publication=True)
    P.require(not errors, 'source_invalid')
    facts, traces = P.pages(client, '/memories/list'), P.pages(client, '/llm-requests', limit=500)
    P.require(all(f.get('document_id') in pin['expected'] for f in facts), 'foreign_fact')
    result['counts'] = {k: {v: sum(r['header'][k] == v for r in records.values())
                           for v in (P.CHECK.SECTIONS if k == 'kind' else ('mid-term', 'long-term'))} for k in ('kind', 'horizon')}
    result['counts']['subject'] = {t + ':' + i: sum(r['header']['subject'] == dict(type=t, id=i)
                    for r in records.values()) for t, i in {(r['header']['subject']['type'],
                    r['header']['subject']['id']) for r in records.values()}}
    result['due_count'] = due_count(pin['commit'])
    result['fact_counts'] = {rid: sum(f.get('document_id') == rid for f in facts) for rid in records}
    extraction = [t for t in traces if t.get('scope') == 'retain_extract_facts']
    result.update(extraction_profiles=[{k: t.get(k) or 'unknown' for k in ('provider', 'model', 'usage')} for t in extraction] or 'unknown',
                  trace_coverage='unknown', embedding_reranker_runtime='unknown', observations=0,
                  slots=gather(client, data['questions'], pin, selector, document_ids))
    if held_fd is not None:
        private = dict(questions=questions, input_sha256=P.digest(held),
                       source_pins={q['id']: source_pins(q) for q in questions},
                       slots=gather(client, questions, pin, selector, document_ids))
        write_fd(evidence_fd, encode(private))
        result['held_out'] = dict(count=len(questions), sha256=P.digest(held))
    P.quiescent(client)
    P.empty_derived(client)
    P.unchanged(pin)
    P.require(profile(client, pin) == result['profile'], 'profile_drift')
    return result


def read_fd(fd):
    P.require(fd >= 3, 'private_fd_required')
    with os.fdopen(os.dup(fd), 'rb') as stream:
        raw = stream.read(8*1024*1024+1)
    P.require(len(raw) <= 8*1024*1024, 'fd_input_too_large')
    return raw


def write_fd(fd, raw):
    P.require(fd >= 3, 'private_fd_required')
    with os.fdopen(os.dup(fd), 'wb') as stream:
        stream.write(raw)


def unique_questions(questions):
    ids = [q['id'] for q in questions]
    P.require(len(ids) == len(set(ids)), 'held_out_duplicate_id')


def reference_pin(source, pin):
    if source['kind'] == 'file' and not source.get('volatile'):
        return source_pins(dict(sources=[source]))[0]
    return dict(pin, pin='missing' if source['kind'] == 'file' and pin.get('pin') == 'missing' else 'reverify')


def check_results(results, evidence, evidence_hash, data):
    unique_questions(data['questions'])
    required = {(q['id'], f, r) for q in data['questions'] for f in FORMS for r in ROUTES}
    slots = [dict(s) for s in results['slots']]
    keys = [(s['id'], s['form'], s['route']) for s in slots]
    P.require(len(keys) == len(required) and set(keys) == required, 'verdict_slots_incomplete')
    observed = {(s['id'], s['form'], s['route']): s for s in evidence['slots']}
    P.require(len(observed) == len(evidence['slots']), 'evidence_slots_duplicate')
    P.require(set(observed) == required, 'evidence_slots_incomplete')
    current_pins, changed = {}, 0
    for question in data['questions']:
        pins = evidence['source_pins'][question['id']]
        P.require(len(pins) == len(question['sources']), 'source_pins_incomplete')
        fresh = [reference_pin(s, p) for s, p in zip(question['sources'], pins)]
        changed += sum(a != b for a, b in zip(fresh, pins))
        current_pins[question['id']] = fresh
    evidence['source_pins'] = current_pins
    counts, by_status = {}, {}
    for slot, key in zip(slots, keys):
        P.require(set(slot) <= RESULT_FIELDS, 'result_field_not_allowed')
        verdict = slot['verdict']
        P.require(verdict in VERDICTS, 'verdict_required')
        pins = current_pins[slot['id']]
        P.require(slot.get('evidence_sha256') == evidence_hash, 'evidence_hash_mismatch')
        if any(p['pin'] != 'equal' for p in pins):
            P.require(slot.get('expectation_reverified') is True, 'expectation_not_reverified')
        n, current = observed[key]['returned_count'], slot['current_count']
        P.require(type(current) is int and 0 <= current <= n, 'current_count_invalid')
        P.require(verdict != 'supported' or (n > 0 and current > 0), 'supported_without_items')
        slot.update({k: v for k, v in observed[key].items() if k not in slot})
        slot['returned_count'], slot['current_item_share'] = n, current/n if n else None
        group = slot['form'] + '/' + slot['route']
        counts.setdefault(group, dict.fromkeys(VERDICTS, 0))[verdict] += 1
        status = next(q['status'] for q in data['questions'] if q['id'] == slot['id'])
        by_status.setdefault(status, dict.fromkeys(VERDICTS, 0))[verdict] += 1
    reached = all(s['verdict'] == 'supported' for s in slots)
    P.require(results.get('target_reached') is reached, 'false_success')
    return dict(target_reached=reached, pins_changed_since_collect=changed, by_route_form=counts, by_status=by_status,
                total={v: sum(s['verdict'] == v for s in slots) for v in VERDICTS}, slots=slots,
                targets={status: sum(q['status'] == status for q in data['questions'])*6 for status in by_status})


def canary(client, pin, data, selector, baseline):
    try:
        current = profile(client, pin)
    except P.Stop:
        current = None
    comparison = ('unknown' if current is None or not baseline else
                  'equal' if current == baseline.get('profile') else 'drift')
    evidence = []
    for q in data['questions']:
        if q['id'] not in data['canary']:
            continue
        status = 'unknown'
        try:
            if current is not None:
                items, _ = recall(client, q['forms']['en'], selector, pin)
                text = '\n'.join(i['text'] for i in items)
                status = 'literal_present' if all(t in text for t in q['expectation']['literal']) else 'literal_absent'
        except P.Stop:
            pass
        evidence.append(dict(id=q['id'], literal=status))
    P.unchanged(pin)
    return dict(profile_comparison=comparison, questions=evidence, route='unavailable' if comparison != 'equal'
                or any(q['literal'] != 'literal_present' for q in evidence) else 'no_readable_drift')


def measurement(kpi, value, observed_at, evidence, request_id):
    P.require(P.CHECK.object_id(kpi) and kpi['object_type'] == 'kpi' and P.CHECK.nonempty(request_id)
              and P.CHECK.nonempty(value) and P.CHECK.nonempty(evidence) and len(value.encode()) <= 200 and len(evidence.encode()) <= 300, 'measurement_request_invalid')
    P.require(datetime.datetime.fromisoformat(observed_at.replace('Z', '+00:00')).utcoffset()
              == datetime.timedelta(0), 'measurement_time_invalid')
    observed_at = datetime.datetime.fromisoformat(observed_at.replace('Z', '+00:00')).strftime('%Y-%m-%dT%H:%M:%SZ')
    return dict(apply=dict(action_type='record_measurement', request_id=request_id,
                          parameters=dict(kpi=kpi, value=value, observed_at=observed_at, evidence=evidence)),
                kpi_get=dict(id=kpi))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_mutually_exclusive_group(required=True)
    for flag in ('check-corpus', 'check-results', 'measurement-request'):
        commands.add_argument('--' + flag, type=Path)
    commands.add_argument('--collect', action='store_true')
    commands.add_argument('--canary', action='store_true')
    parser.add_argument('--corpus', type=Path, default=ROOT / 'memory/evaluation/questions.json')
    for flag in ('evidence', 'output', 'baseline'):
        parser.add_argument('--' + flag, type=Path)
    for flag in ('credential-fd', 'held-out-fd', 'held-out-evidence-fd', 'held-out-results-fd'):
        parser.add_argument('--' + flag, type=int)
    parser.add_argument('--subject')
    parser.add_argument('--document-id', action='append', default=[])
    args = parser.parse_args(argv)
    try:
        P.require(args.held_out_fd is None or args.held_out_evidence_fd is not None, 'held_out_evidence_fd_required')
        raw = (args.check_corpus or args.corpus).read_bytes()
        data = corpus(raw)
        if args.check_corpus:
            result = dict(identities=45, forms=3, routes=2, verdict_slots=270)
        elif args.check_results:
            evidence_raw = args.evidence.read_bytes()
            evidence = load(evidence_raw)
            P.require(evidence['corpus_sha256'] == P.digest(raw), 'corpus_changed')
            result = dict(evidence, **check_results(load(args.check_results.read_bytes()), evidence, P.digest(evidence_raw), data))
            if 'held_out' in evidence:
                P.require(evidence['held_out']['count'] > 0, 'held_out_empty')
                P.require(args.held_out_results_fd is not None and args.held_out_evidence_fd is not None,
                          'held_out_judgments_required')
                P.require(args.held_out_fd is not None, 'held_out_input_required')
                sealed_raw = read_fd(args.held_out_fd)
                private_raw = read_fd(args.held_out_evidence_fd)
                private = load(private_raw)
                P.require(P.digest(sealed_raw) == private['input_sha256'] == evidence['held_out']['sha256']
                          and private['questions'] == load(sealed_raw)['questions']
                          and len(private['questions']) == evidence['held_out']['count'], 'held_out_binding')
                judged = check_results(load(read_fd(args.held_out_results_fd)), private,
                                       P.digest(private_raw), private)
                result['held_out'] = judged['total']
                result['target_reached'] &= judged['target_reached']
                result['pins_changed_since_collect'] += judged['pins_changed_since_collect']
            else:
                result.update(held_out='not_evaluated', target_reached=False)
        elif args.measurement_request:
            result = measurement(**load(args.measurement_request.read_bytes()))
            P.require(args.output is not None and args.output.is_dir(), 'request_output_directory_required')
            for name, request in result.items():
                (args.output / (name + '.json')).write_bytes(encode(request))
            return 0
        else:
            P.require(args.credential_fd is not None, 'credential_fd_required')
            os.chdir(ROOT)
            with P.run_budget(), P.locked():
                pin = P.pinned()
                _, _, nodes, errors = P.CHECK.load(Path('memory/records'), Path('memory/subjects.json'),
                                                  commit=pin['commit'], publication=True)
                matches = [n for n in nodes if n[1] == args.subject] if args.subject is not None else [
                    n for n in nodes if nodes[n]['parent'] is None]
                P.require(not errors and len(matches) == 1, 'subject_unknown_or_ambiguous')
                selector = P.CHECK.recall_tags(dict(zip(('type', 'id'), matches[0])), descendants=True)
                client = P.Client(P.credential(args.credential_fd))
                result = (canary(client, pin, data, selector, load(args.baseline.read_bytes()) if args.baseline else None)
                          if args.canary else collect(client, pin, data, raw, selector, args.document_id,
                                                    args.held_out_fd, args.held_out_evidence_fd))
        if args.output:
            args.output.write_bytes(encode(result))
        else:
            sys.stdout.buffer.write(encode(result))
        return 0
    except P.Stop as error:
        print('evaluator_stop@scripts/evaluate-memory.py:1: ' + str(error))
    except Exception:
        print('evaluator_stop@scripts/evaluate-memory.py:1: invalid_input_or_read_failure')
    return 1


if __name__ == '__main__':
    sys.exit(main())
