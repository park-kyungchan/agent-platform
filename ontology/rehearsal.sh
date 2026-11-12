#!/usr/bin/env bash
set -euo pipefail
[[ $# == 1 ]] || { echo 'usage: rehearsal.sh /path/to/ont' >&2; exit 2; }
ont_binary=$(realpath -- "$1")
repo=$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
rehearsal_root=$(mktemp -d)
trap 'python3 -c '\''import shutil,sys; shutil.rmtree(sys.argv[1])'\'' "$rehearsal_root"' EXIT
python3 - "$ont_binary" "$repo" "$rehearsal_root/store.sqlite" <<'PY'
import datetime, hashlib, json, pathlib, subprocess, sys
binary, repo, store = sys.argv[1:]
now = int(datetime.datetime(2026, 11, 11, 2, tzinfo=datetime.timezone.utc).timestamp())
def call(args, request=None, code=0):
    p = subprocess.run([binary, '--store', store, '--now', str(now), *args], input=json.dumps(request) if request is not None else None, text=True, capture_output=True, timeout=60)
    assert p.returncode == code, (args, p.returncode, p.stdout, p.stderr)
    if args == ['describe']: print('ont_describe_stdout_bytes=' + str(len(p.stdout.encode())), file=sys.stderr)
    if args == ['action', 'validate']: print('validate_parameters=' + json.dumps(request['parameters']) + '\nvalidate=' + p.stdout.rstrip(), file=sys.stderr)
    if args == ['action', 'apply'] and code == 2 and 'future_observation' in p.stdout: print('future_refusal=' + p.stdout.rstrip(), file=sys.stderr)
    data = json.loads(p.stdout); assert data['status'] == ('refused' if code else 'ok'), data
    return data.get('result', data)
def oid(kind, key): return dict(object_type=kind, namespace=dict(instance='vps://palantirkc', account='root'), native_key=key)
def obj(i): return call(['get'], dict(id=i))['objects'][0]
def fields(i): return {k: v.get('value') for k, v in obj(i)['observations'][-1]['fields'].items()}
def created(r, kind): return next(e['id'] for e in r['edits'] if e['id']['object_type'] == kind and e['change'] == 'initial')
def apply(action, rid, parameters, failure=None, outcome='applied'):
    r = call(['action', 'apply'], dict(action_type=action, request_id=rid, parameters=parameters), 2 if failure else 0)
    if failure:
        expected = {failure} if isinstance(failure, str) else set(failure)
        actual = set(r['failed']) if r['code'] == 'submission_criteria' else {r['code']}
        assert actual == expected, (expected, r)
    else: assert r['outcome'] == outcome and r['actor_trust'] == 'self_reported', r
    return r
def render(view, key=None): return call(['render'], dict(view=view, args=dict(task=key or task['native_key'])))
def fingerprint(): return [(p.name, hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None) for p in [pathlib.Path(store), pathlib.Path(store).with_name('edits.jsonl')]]
task = oid('task', 'workspace-strategy-reassessment-20261109'); packet = oid('packet', 'ontology-operational-base/TASK-O13.md'); report = oid('report', 'ontology-operational-base/REPORT-O13.md')
v1 = subprocess.check_output(['git', '-C', repo, 'show', 'f2d1b030bd1a0a66928ef57d9365027526e664da:ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v1).hexdigest() == '4573ab180f63889029ae98c892c996e810f9d8140cf26726ee6d5fffbfc10605'
first = pathlib.Path(store).with_name('v1.json'); first.write_bytes(v1)
call(['schema', 'install', str(first)]); call(['index', '--source', 'task_states', '--source', 'packets', '--source', 'reports'])
revision = obj(task)['observations'][-1]['revision']; obj(packet); obj(report)
v2 = subprocess.check_output(['git', '-C', repo, 'show', '2b9417c8bc96b0cbeb4ab7913aa44cb49ad8d604:ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v2).hexdigest() == '27de5fd02c20299897720ab34543e04e6db685438c3dd7409691cb8ddae11f24'
second = pathlib.Path(store).with_name('v2.json'); second.write_bytes(v2)
call(['schema', 'install', str(second)])
probe = created(apply('set_hold', 'v2-probe', dict(task=task, effect='upgrade probe', until='schema v3')), 'hold')
apply('lift_hold', 'v2-probe-lift', dict(hold=probe))
probe_revision = obj(probe)['head']
call(['schema', 'install', str(pathlib.Path(repo, 'ontology/work.schema.json'))])
assert obj(probe)['head'] == probe_revision
assert obj(task)['observations'][-1]['revision'] == revision
call(['describe'], {})
hp = dict(task=task, effect='publish', until='review accepted')
dp = dict(task=task, topic='route', text='use actions', decided_by='lead', undo='supersede')
kp = dict(task=task, name='checks', definition='passing checks from rehearsal output')
tp = dict(task=task, key='rehearsal', consumer='Academy daily-record route', consumer_kind='domain_route', first_use='Save a daily record and observe its readback')
thread = created(apply('open_thread', 'thread', tp), 'thread')
pp = dict(thread=thread, first_lines=100, limit_lines=150, current_lines=100, amended_by_family='family-a')
plan = created(apply('open_plan', 'plan', pp), 'plan')
wp = dict(task=task, name='O13', tier='bounded', runtime='codex', packet=packet, thread=thread, plan=plan, layer='domain')
unchanged = fingerprint()
for action, params in [('set_hold', hp), ('record_decision', dp), ('define_kpi', kp), ('start_worker_run', wp)]:
    expected = ('unknown_task', 'thread_of_other_task') if action == 'start_worker_run' else 'unknown_task'
    apply(action, 'unknown-' + action, dict(params, task=oid('task', 'invented-o13-task')), expected)
assert fingerprint() == unchanged
h = created(apply('set_hold', 'hold-1', hp), 'hold'); first_time = fields(h)['set_at']; now += 1
apply('set_hold', 'hold-1', hp, outcome='already_applied'); assert fields(h)['set_at'] == first_time == '2026-11-11T02:00:00Z'
apply('set_hold', 'hold-1', dict(hp, until='changed'), 'request_id_reused')
apply('set_hold', 'hold-duplicate', hp, 'duplicate_active_hold')
apply('lift_hold', 'lift', dict(hold=h, reason='accepted')); apply('lift_hold', 'lift-again', dict(hold=h), 'hold_not_active')
apply('set_hold', 'hold-2', hp)
d = created(apply('record_decision', 'decision-1', dp), 'decision'); apply('record_decision', 'decision-duplicate', dp, 'decision_already_current')
sp = dict(decision=d, text='use | rendered\nactions', decided_by='lead', undo='supersede')
successor = created(apply('supersede_decision', 'decision-2', sp), 'decision'); apply('supersede_decision', 'decision-again', sp, 'decision_not_current')
assert fields(successor)['task'] == task and fields(successor)['topic'] == 'route' and fields(successor)['supersedes'] == d and fields(d)['status'] == 'superseded'
k = created(apply('define_kpi', 'kpi', kp), 'kpi'); apply('define_kpi', 'kpi-duplicate', kp, 'duplicate_kpi')
assert '| checks | (missing) | (missing) | (missing) | (missing) |' in render('kpi_board')['markdown']
mp = dict(kpi=k, value='10', observed_at='2026-11-11T00:00:00Z', evidence='ontology/rehearsal.sh')
apply('record_measurement', 'measure-1', mp)
newer = dict(mp, value='11', observed_at='2026-11-11T01:00:00Z')
assert call(['action', 'validate'], dict(action_type='record_measurement', parameters=newer))['valid']
apply('record_measurement', 'measure-2', newer); before = render('kpi_board')['markdown']
older = dict(mp, observed_at='2026-11-11T00:30:00Z')
assert not call(['action', 'validate'], dict(action_type='record_measurement', parameters=older))['valid']
unchanged = fingerprint(); apply('record_measurement', 'measure-old', older, 'older_measurement'); assert fingerprint() == unchanged
for i, invalid_time in enumerate(['not-a-time', '2026-13-01T00:00:00Z', '2026-02-31T00:00:00Z', '2026-11-11T24:00:00Z']):
    unchanged = fingerprint(); board = render('kpi_board')['markdown']
    apply('record_measurement', 'measure-bad-' + str(i), dict(mp, observed_at=invalid_time), 'invalid_parameter')
    assert fingerprint() == unchanged and render('kpi_board')['markdown'] == board
apply('record_measurement', 'measure-correction', dict(newer, value='12')); corrected = render('kpi_board')['markdown']
apply('record_measurement', 'measure-new-id', dict(newer, value='12')); assert render('kpi_board')['markdown'] == corrected
measurements = call(['search'], dict(object_type='measurement', where={'eq': ['kpi', k]}))['objects']; assert len(measurements) == 4 and fields(k)['value'] == '12'
for i, future in enumerate(['2026-11-11T03:00:01Z', '9999-12-31T23:59:59Z']):
    unchanged = fingerprint(); board = render('kpi_board')['markdown']; params = dict(mp, observed_at=future)
    assert not call(['action', 'validate'], dict(action_type='record_measurement', parameters=params))['valid']
    apply('record_measurement', 'measure-future-' + str(i), params, 'future_observation')
    assert fingerprint() == unchanged and render('kpi_board')['markdown'] == board
boundary = dict(mp, value='13', observed_at='2026-11-11T02:00:01Z')
assert call(['action', 'validate'], dict(action_type='record_measurement', parameters=boundary))['valid']
apply('record_measurement', 'measure-boundary', boundary); now += 1
later = dict(mp, value='14', observed_at='2026-11-11T02:00:02Z'); apply('record_measurement', 'measure-after-future', later)
assert fields(k)['value'] == '14' and fields(k)['observed_at'] == later['observed_at'] and render('kpi_board')['markdown'] != board
assert len(call(['search'], dict(object_type='measurement', where={'eq': ['kpi', k]}))['objects']) == 6
apply('set_kpi_target', 'target-absent', dict(kpi=k), 'invalid_parameter'); apply('set_kpi_target', 'target', dict(kpi=k, target='14'))
other_task = oid('task', 'ontology-operational-base'); apply('define_kpi', 'other-kpi', dict(kp, task=other_task))
w = created(apply('start_worker_run', 'worker', wp), 'worker_run'); apply('start_worker_run', 'worker-duplicate', wp, 'duplicate_running_worker')
apply('note_worker_run', 'note-absent', dict(worker_run=w), 'invalid_parameter')
apply('note_worker_run', 'note', dict(worker_run=w, note='progress', report=report)); assert 'result' not in fields(w) and fields(w)['last_note'] == 'progress'
apply('close_worker_run', 'close-absent', dict(worker_run=w), 'invalid_parameter')
now += 59; cp = dict(worker_run=w, result='passed'); apply('close_worker_run', 'close', cp)
apply('note_worker_run', 'note-late', dict(worker_run=w, note='late'), 'worker_run_not_running'); apply('close_worker_run', 'close-again', cp, 'worker_run_not_running')
assert fields(w)['report'] == report and fields(w)['result'] == 'passed' and fields(w)['last_note'] == 'progress'
pending_packet = oid('packet', 'ontology-operational-base/TASK-O13-unindexed.md'); pending_report = oid('report', 'ontology-operational-base/REPORT-O13-unindexed.md')
call(['get'], dict(id=pending_packet), 2); call(['get'], dict(id=pending_report), 2)
pending = created(apply('start_worker_run', 'pending', dict(wp, name='pending', packet=pending_packet)), 'worker_run')
now += 1; apply('close_worker_run', 'pending-close', dict(worker_run=pending, result='pending passed', report=pending_report))
live = created(apply('start_worker_run', 'live', dict(wp, name='live')), 'worker_run')
apply('note_worker_run', 'live-note', dict(worker_run=live, note='check | next\nline'))
apply('start_worker_run', 'live-pending', dict(wp, name='live-pending', packet=pending_packet))
assert fields(w)['started_at'] == '2026-11-11T02:00:02Z' and fields(w)['closed_at'] == '2026-11-11T02:01:01Z'
assert fields(successor)['decided_at'] == '2026-11-11T02:00:01Z' and all(fields(m['id'])['recorded_at'] == '2026-11-11T02:00:01Z' for m in measurements)
# Render/installation fixture; transition coverage belongs to the exhaustive walk.
record = oid('memory_record', 'rehearsal-render')
apply('mark_memory_record_pending', 'memory-render-pending', dict(record=record, document_id='rehearsal-render', body_hash='a' * 64))
apply('mark_memory_record_ready', 'memory-render-ready', dict(record=record))
# Hash values are abstracted by the walk; prove actual replacement separately.
apply('mark_memory_record_pending', 'memory-render-correction', dict(record=record, document_id='rehearsal-render', body_hash='b' * 64))
assert fields(record)['status'] == 'pending' and fields(record)['body_hash'] == 'b' * 64
apply('mark_memory_record_ready', 'memory-render-corrected-ready', dict(record=record))
memory_block = call(['render'], dict(view='current_memory_records', args={}))
assert memory_block['rows'] == 1 and not memory_block['truncated']
assert '| Record id | Document id | Body hash | Status |' in memory_block['markdown']
assert '| rehearsal-render | rehearsal-render | ' + 'b' * 64 + ' | ready |' in memory_block['markdown']
print(memory_block['markdown'])
aggregation = call(['aggregate'], dict(object_type='worker_run', group_by=['status'], metrics=['count']))
assert aggregation['kind'] == 'aggregation' and aggregation['groups'] == [dict(key=dict(status='running'), count=2), dict(key=dict(status='closed'), count=2)], aggregation
print('worker_run_aggregate=' + json.dumps(aggregation, sort_keys=True))
refused = call(['aggregate'], dict(object_type='memory_record', metrics=['count']), 2)
assert refused['code'] == 'derive_forbidden', refused
print('memory_record_aggregate=' + json.dumps(refused, sort_keys=True))
requests = [dict(jsonrpc='2.0', id=i, method='tools/call', params=dict(name=n, arguments={})) for i, n in enumerate(['ont_describe', 'ont_action_list'], 1)]
wire = subprocess.run([binary, '--store', store, 'mcp'], input='\n'.join(map(json.dumps, requests))+'\n', text=True, capture_output=True, timeout=60); assert wire.returncode == 0, wire.stderr
for request, line in zip(requests, wire.stdout.splitlines(), strict=True):
    answer = json.loads(line)['result']; assert not answer['isError'], answer
    if request['params']['name'] == 'ont_action_list': assert answer['content'][0]['text'].count('Actions take the full Id from search; view arguments take the native key.') == 15
    size = len(answer['content'][0]['text'].encode()); assert size <= 262144; print(request['params']['name'] + '_bytes=' + str(size), file=sys.stderr)
expected = {'kpi_board': (1, '| checks | 14 | 2026-11-11T02:00:02Z | 14 | ontology/rehearsal.sh |'), 'active_holds': (1, '| publish | review accepted |'), 'current_decisions': (1, '| route | use \\| rendered actions | lead | supersede |'), 'live_workers': (2, 'check \\| next line'), 'closed_workers': (2, 'pending passed')}
for view, (count, text) in expected.items():
    r = render(view); assert r['rows'] == count and not r['truncated'] and text in r['markdown'], r
    assert render(view, 'absent-o13-task')['rows'] == 0
    if view in ['live_workers', 'closed_workers']: assert packet['native_key'] in r['markdown'] and pending_packet['native_key'] in r['markdown']
    if view == 'live_workers': assert '(missing)' in r['markdown']
    if view == 'closed_workers': assert report['native_key'] in r['markdown'] and pending_report['native_key'] in r['markdown'] and r['markdown'].index('pending passed') < r['markdown'].index('| passed |')
    print(r['markdown'])
# v5 gate rows run after the frozen view/aggregate expectations above.
def gate(action, rid, params, failure=None):
    before = fingerprint(); answer = apply(action, 'gate-' + action + '-' + rid, params, failure)
    if failure: assert fingerprint() == before
    print('gate_row=' + rid + ' expected=' + (','.join(sorted(failure)) if failure and not isinstance(failure, str) else failure or 'admitted'))
    return answer
def dispatch(rid, failure=None, **changes):
    answer = gate('start_worker_run', rid, dict(wp, name='gate-' + rid, **changes), failure)
    if not failure: apply('close_worker_run', 'gate-close-' + rid, dict(worker_run=created(answer, 'worker_run'), result='gate passed'))
gate('open_thread', 'duplicate-thread', tp, 'duplicate_thread')
gate('open_thread', 'new-thread', dict(tp, key='other'))
missing_thread = oid('thread', 'absent'); missing_plan = oid('plan', 'absent')
gate('amend_thread', 'unknown-thread', dict(thread=missing_thread, consumer='x', consumer_kind='none', first_use='', reason='fixture'), 'unknown_thread')
gate('amend_thread', 'reason-required', dict(thread=thread, consumer='x', consumer_kind='none', first_use=''), 'invalid_parameter')
gate('open_plan', 'duplicate-plan', pp, 'duplicate_plan')
gate('open_plan', 'unknown-plan-thread', dict(pp, thread=missing_thread), 'unknown_thread')
gate('amend_plan', 'unknown-plan', dict(plan=missing_plan, current_lines=101, amended_by_family='family-a'), 'unknown_plan')
gate('amend_plan', 'first-immutable', dict(plan=plan, current_lines=101, first_lines=1, amended_by_family='family-a'), 'invalid_parameter')
# A missing required plan parameter uses the native parameter refusal.
gate('start_worker_run', 'plan-missing', {k: v for k, v in wp.items() if k != 'plan'}, 'invalid_parameter')
dispatch('thread-unknown', ('unknown_thread', 'thread_of_other_task', 'engine_without_domain_consumer', 'plan_of_other_thread', 'missing_first_use'), thread=missing_thread)
restarted = created(gate('start_worker_run', 'closed-name', wp), 'worker_run')
apply('close_worker_run', 'gate-close-restarted', dict(worker_run=restarted, result='gate passed'))
dispatch('plan-unknown', ('plan_of_other_thread', 'plan_over_limit'), plan=missing_plan)
other = created(gate('open_thread', 'plan-other-thread', dict(tp, key='plan-other')), 'thread')
other_plan = created(gate('open_plan', 'plan-other', dict(pp, thread=other)), 'plan')
dispatch('plan-mismatch', 'plan_of_other_thread', plan=other_plan); dispatch('plan-matches')
foreign = created(gate('open_thread', 'foreign-thread', dict(tp, task=other_task, key='foreign')), 'thread')
foreign_plan = created(gate('open_plan', 'foreign-plan', dict(pp, thread=foreign)), 'plan')
dispatch('foreign-task', 'thread_of_other_task', thread=foreign, plan=foreign_plan)
for kind in ['harness', 'none']:
    gate('amend_thread', 'consumer-' + kind, dict(thread=thread, consumer='Program board', consumer_kind=kind, first_use=tp['first_use'], reason='gate fixture'))
    expected = ('engine_without_domain_consumer', 'missing_first_use') if kind == 'none' else 'engine_without_domain_consumer'
    dispatch('engine-' + kind, expected, layer='engine')
    if kind == 'harness':
        dispatch('harness-harness', layer='harness')
        dispatch('harness-use-present', layer='domain')
    if kind == 'none': dispatch('no-kind', 'missing_first_use')
gate('amend_thread', 'domain-consumer', dict(thread=thread, consumer='', consumer_kind='domain_route', first_use=tp['first_use'], reason='kind decides'))
dispatch('engine-domain', layer='engine')
gate('amend_thread', 'empty-first-use', dict(thread=thread, consumer='Academy', consumer_kind='domain_route', first_use='', reason='gate fixture'))
dispatch('empty-use', 'missing_first_use')
gate('amend_thread', 'restore-first-use', dict(thread=thread, consumer='Academy', consumer_kind='domain_route', first_use=tp['first_use'], reason='gate fixture'))
dispatch('has-use')
assert any(o['fields']['consumer_kind'].get('value') == 'harness' for o in obj(thread)['observations'])
review = dict(plan=plan, reviewer_family='family-a', review_report='fixture/REVIEW.md')
gate('amend_plan', 'at-limit', dict(plan=plan, current_lines=150, amended_by_family='family-a')); dispatch('at-limit')
gate('amend_plan', 'over-limit', dict(plan=plan, current_lines=151, amended_by_family='family-a')); dispatch('over-limit', 'plan_over_limit')
gate('record_scope_review', 'unknown-plan', dict(review, plan=missing_plan), ('unknown_plan', 'scope_review_same_family'))
gate('record_scope_review', 'same-family', review, 'scope_review_same_family')
gate('record_scope_review', 'different-family', dict(review, reviewer_family='family-b')); dispatch('reviewed-over-limit')
assert fields(plan)['scope_reviewed'] and fields(plan)['review_report'] == review['review_report']
gate('amend_plan', 'invalidate-review', dict(plan=plan, current_lines=152, amended_by_family='family-b'))
assert not fields(plan)['scope_reviewed'] and fields(plan)['first_lines'] == 100 and fields(plan)['limit_lines'] == 150
dispatch('amended-after-review', 'plan_over_limit')
gate('record_scope_review', 'latest-family', dict(review, reviewer_family='family-b'), 'scope_review_same_family')
for action, params in [('record_decision', dict(dp, topic='gate-user')), ('supersede_decision', dict(sp, decision=successor))]:
    gate(action, action + '-no-evidence', dict(params, decided_by='user'), 'user_decision_evidence_required')
    gate(action, action + '-form-no-words', dict(params, decided_by='user', user_form='decision'), 'user_decision_evidence_required')
    gate(action, action + '-empty-words', dict(params, decided_by='user', user_words='', user_form='decision'), 'user_decision_evidence_required')
    gate(action, action + '-words-no-form', dict(params, decided_by='user', user_words='"Use the domain route."'), 'user_decision_evidence_required')
    user = dict(params, decided_by='user', user_words='"Use the domain route."', user_form='question')
    gate(action, action + '-question', user, 'user_question_is_not_a_decision')
    answer = gate(action, action + '-decision', dict(user, user_form='decision'))
    assert fields(created(answer, 'decision'))['user_words'] == user['user_words']
gate('record_decision', 'lead-question', dict(dp, topic='gate-lead-question', user_form='question'))
print('v5_gate_rows_complete')
PY

python3 -I -B "$repo/ontology/memory-index-walk.py" "$ont_binary" "$rehearsal_root/memory-walk" --records 2
