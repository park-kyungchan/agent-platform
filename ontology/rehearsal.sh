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
def shape(r, kind, changes):
    edits = r['edits']
    assert [e['change'] for e in edits] == changes, r
    first = edits[0]['id']
    assert set(first) == {'object_type', 'namespace', 'native_key'} and first['object_type'] == kind, r
    assert set(first['namespace']) == {'instance', 'account'} and all(isinstance(v, str) and v for v in [first['native_key'], *first['namespace'].values()]), r
    assert all(e['id'] == first for e in edits), r
    print('shape=' + kind + ' changes=' + json.dumps(changes) + ' ids=one')
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
# Preserve a real v5 flag-only review across the upgrade to v6.
legacy = pathlib.Path(store).with_name('v5.json')
legacy.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', 'f46cdf49ba74e117ddaaf387416a0eac1d53f2af:ontology/work.schema.json'], timeout=10))
call(['schema', 'install', str(legacy)])
legacy_thread = created(apply('open_thread', 'legacy-thread', dict(task=task, key='legacy-v5', consumer='fixture', consumer_kind='harness', first_use='upgrade')), 'thread')
legacy_plan = created(apply('open_plan', 'legacy-plan', dict(thread=legacy_thread, first_lines=100, limit_lines=150, current_lines=151, amended_by_family='family-a')), 'plan')
apply('record_scope_review', 'legacy-review', dict(plan=legacy_plan, reviewer_family='family-b', review_report='fixture/v5.md'))
v6 = pathlib.Path(store).with_name('v6.json')
v6.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', '8ad36bf3cf10e19659965cc2d458a921533cdfae:ontology/work.schema.json'], timeout=10))
call(['schema', 'install', str(v6)])
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
# v6 gate rows run after the frozen view/aggregate expectations above.
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
dispatch('plan-unknown', ('plan_missing', 'plan_of_other_thread', 'plan_over_limit'), plan=missing_plan)
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
assert fields(legacy_plan)['scope_reviewed'] and 'reviewed_lines' not in fields(legacy_plan)
dispatch('v5-flag-only', 'plan_over_limit', thread=legacy_thread, plan=legacy_plan)
review = dict(plan=plan, reviewed_lines=152, reviewer_family='family-a', review_report='fixture/REVIEW.md')
gate('amend_plan', 'at-limit', dict(plan=plan, current_lines=150, amended_by_family='family-a')); dispatch('at-limit')
gate('amend_plan', 'over-limit', dict(plan=plan, current_lines=151, amended_by_family='family-a')); dispatch('over-limit', 'plan_over_limit')
gate('record_scope_review', 'unknown-plan', dict(review, plan=missing_plan), ('unknown_plan', 'reviewed_lines_below_current', 'scope_review_same_family'))
dispatch('review-over-limit', tier='review')
dispatch('review-other-refusals', 'plan_of_other_thread', tier='review', plan=other_plan)
gate('record_scope_review', 'size-required', {k: v for k, v in review.items() if k != 'reviewed_lines'}, 'invalid_parameter')
gate('record_scope_review', 'below-current', dict(review, reviewer_family='family-b', reviewed_lines=150), 'reviewed_lines_below_current')
gate('record_scope_review', 'same-family', review, 'scope_review_same_family')
gate('record_scope_review', 'different-family', dict(review, reviewer_family='family-b')); dispatch('reviewed-over-limit')
assert fields(plan)['scope_reviewed'] and fields(plan)['reviewed_lines'] == 152 and fields(plan)['review_report'] == review['review_report']
gate('amend_plan', 'invalidate-review', dict(plan=plan, current_lines=152, amended_by_family='family-b'))
assert not fields(plan)['scope_reviewed'] and fields(plan)['first_lines'] == 100 and fields(plan)['limit_lines'] == 150
dispatch('amended-within-review')
assert fields(plan)['current_lines'] == fields(plan)['reviewed_lines']
gate('amend_plan', 'above-reviewed-size', dict(plan=plan, current_lines=153, amended_by_family='family-b'))
dispatch('above-reviewed-size', 'plan_over_limit')
gate('record_scope_review', 'latest-family', dict(review, reviewer_family='family-b', reviewed_lines=153), 'scope_review_same_family')
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
print('v6_gate_rows_complete')
v6_running = created(gate('start_worker_run', 'v6-still-running', dict(wp, name='v6-still-running', tier='review')), 'worker_run')
old_plan = fields(plan); old_run = fields(w)
old_views = {view: call(['render'], dict(view=view, args={} if view == 'current_memory_records' else dict(task=task['native_key']))) for view in json.loads(v6.read_text())['views']}
# Version 7 is pinned in Git: its rows run on it, then version 8 is installed over the store they leave.
v7 = pathlib.Path(store).with_name('v7.json')
v7.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', 'ccb15142dbe19f2e04ad1c5d445048336db60d08:ontology/work.schema.json'], timeout=10))
assert hashlib.sha256(v7.read_bytes()).hexdigest() == '6f6d2acc4a45ccc7e2f25268d458f9b6fabc5ab10fb85231d0fa81eaf68373f4'
call(['schema', 'install', str(v7)])
assert fields(plan) == old_plan and fields(w) == old_run
for kind, retained in [('plan', plan), ('worker_run', w)]:
    assert retained in [o['id'] for o in call(['search'], dict(object_type=kind))['objects']]
    print('gate_row=v6-' + kind + '-get-search expected=admitted')
for view in json.loads(v7.read_text())['views']:
    answer = call(['render'], dict(view=view, args={} if view == 'current_memory_records' else dict(task=task['native_key'])))
    assert not answer['truncated']
    if view in old_views:
        assert answer['rows'] == old_views[view]['rows']
        assert answer['markdown'] == old_views[view]['markdown'].replace('schema=work@6', 'schema=work@7')
    print('gate_row=v7-view-' + view + ' expected=admitted rows=' + str(answer['rows']))
same_name = gate('start_worker_run', 'v6-running-same-name', dict(wp, name='v6-still-running', tier='review'))
print('v6_running_same_name=' + same_name['outcome'])
gate('note_worker_run', 'v6-running-note', dict(worker_run=v6_running, note='upgrade note'))
gate('close_worker_run', 'v6-running-close', dict(worker_run=v6_running, result='upgrade close'))
assert fields(v6_running).get('running_in') is None
gate('close_worker_run', 'v6-same-name-close', dict(worker_run=created(same_name, 'worker_run'), result='done'))
gate('amend_plan', 'v6-plan-amend', dict(plan=plan, current_lines=100, amended_by_family='family-b'))
rp = dict(task=task, kind='packet', head='a' * 40, model='fixture-model', effort='medium', prompt='/home/palantirkc/work/fixture/PROMPT.md', prompt_sha256='b' * 64, runs=[w])
review_started = gate('start_review', 'review-start', rp)
# crates/ap/src/review.rs start parses this shape; change both together.
shape(review_started, 'review', ['initial'])
first_review = created(review_started, 'review')
assert fields(first_review)['runs'] == [w]
assert fields(first_review)['running_in'] == task
gate('start_review', 'review-duplicate', rp, 'duplicate_running_review')
another_kind = created(gate('start_review', 'review-another-kind', dict(rp, kind='design')), 'review')
gate('start_review', 'review-space-prompt', dict(rp, prompt='/home/palantirkc/work/fixture/PROMPT space.md'), 'invalid_parameter')
gate('close_review', 'review-space-report', dict(review=first_review, verdict='accept', report='/home/palantirkc/work/fixture/REPORT space.md', report_sha256='c' * 64), 'invalid_parameter')
gate('close_review', 'review-report-required', dict(review=first_review, verdict='accept'), 'report_required')
gate('close_review', 'review-hash-required', dict(review=first_review, verdict='accept', report='/home/palantirkc/work/fixture/REPORT.md'), 'report_required')
gate('close_review', 'review-empty-report', dict(review=first_review, verdict='accept', report='', report_sha256='c' * 64), 'invalid_parameter')
for label, prompt in [('empty', ''), ('relative', 'PROMPT.md')]:
    gate('start_review', 'review-' + label + '-prompt', dict(rp, prompt=prompt), 'invalid_parameter')
review_closed = gate('close_review', 'review-abandoned', dict(review=first_review, verdict='abandoned'))
# No ap command parses close_review edits today; ontology/README.md states this shape.
shape(review_closed, 'review', ['update', 'update'])
assert fields(first_review).get('running_in') is None
gate('close_review', 'review-second-close', dict(review=first_review, verdict='abandoned'), 'review_not_running')
next_review = created(gate('start_review', 'review-after-close', rp), 'review')
assert fields(first_review)['status'] == 'closed' and fields(first_review)['verdict'] == 'abandoned'
assert fields(next_review)['status'] == 'running'
assert render('reviews')['rows'] == 3
another_head = created(gate('start_review', 'review-another-head', dict(rp, head='e' * 40)), 'review')
gate('close_review', 'review-another-head-close', dict(review=another_head, verdict='abandoned'))
review = dict(review, reviewed_lines=153, reviewer_family='family-b')
gate('record_scope_review', 'v7-same-family', review)
gate('record_scope_review', 'v7-running-review', dict(review, review=next_review), 'review_not_closed')
gate('record_scope_review', 'v7-closed-review', dict(review, review=first_review))
assert fields(plan)['review'] == first_review
gate('close_review', 'review-report', dict(review=next_review, verdict='accept', report='/home/palantirkc/work/fixture/REPORT.md', report_sha256='c' * 64, account='fixture', session='fixture-session'))
assert fields(next_review)['report_sha256'] == 'c' * 64
for rid, extra in [('write-set', dict(write_set=['crates/ap/src/work.rs', 'ontology/**'])), ('no-write-set', {})]:
    run = created(gate('start_worker_run', 'v7-' + rid, dict(wp, name='v7-' + rid, **extra)), 'worker_run')
    assert fields(run)['running_in'] == task
    if rid == 'write-set':
        worker_started = gate('start_worker_run', 'v7-another-name', dict(wp, name='v7-another-name'))
        # crates/ap/src/work.rs applied_run parses this shape; change both together.
        shape(worker_started, 'worker_run', ['initial'])
        other = created(worker_started, 'worker_run')
        worker_closed = gate('close_worker_run', 'v7-another-name-close', dict(worker_run=other, result='done'))
        # crates/ap/src/work.rs applied_run parses this shape; change both together.
        shape(worker_closed, 'worker_run', ['update', 'update'])
    gate('start_worker_run', 'v7-duplicate-' + rid, dict(wp, name='v7-' + rid, **extra), 'duplicate_running_worker')
    if extra: assert fields(run)['write_set'] == extra['write_set']
    else: assert 'write_set' not in fields(run)
    gate('close_worker_run', 'v7-close-' + rid, dict(worker_run=run, result='done'))
    assert fields(run).get('running_in') is None
# The schema itself bounds members, even when ap is bypassed.
for label, member in [('plus', '+a'), ('301', 'a' * 301), ('wildcard', 'src/*.rs'), ('absolute', '/etc/passwd'), ('unicode', 'src/\u00e9.rs')]:
    gate('start_worker_run', 'direct-write-' + label, dict(wp, name='direct-' + label, write_set=[member]), 'invalid_parameter')
valid_run = created(gate('start_worker_run', 'direct-write-valid', dict(wp, name='direct-valid', write_set=['crates/ap/src/**', 'ontology/work.schema.json'])), 'worker_run')
gate('close_worker_run', 'direct-write-valid-close', dict(worker_run=valid_run, result='done'))
# The same path-part boundary must hold without the ap caller.
for label, member, failure in [
    ('297-plain', 'a' * 297, None),
    ('298-plain', 'a' * 298, 'invalid_parameter'),
    ('297-tree', 'a' * 297 + '/**', None),
    ('298-tree', 'a' * 298 + '/**', 'invalid_parameter'),
]:
    answer = gate('start_worker_run', 'direct-boundary-' + label, dict(wp, name='boundary-' + label, write_set=[member]), failure)
    if not failure:
        gate('close_worker_run', 'direct-boundary-close-' + label, dict(worker_run=created(answer, 'worker_run'), result='done'))
# Five closed objects must not enter the sixth start's basis on the G4 engine.
closed = {}
for kind in ['review', 'worker_run']:
    closed[kind] = []
    params = dict(rp, head='d' * 40) if kind == 'review' else dict(wp, name='basis-fixture')
    for index in range(5):
        item = created(gate('start_' + kind, 'basis-' + kind + '-' + str(index), params), kind)
        close = dict(review=item, verdict='abandoned') if kind == 'review' else dict(worker_run=item, result='done')
        gate('close_' + kind, 'basis-close-' + kind + '-' + str(index), close)
        assert fields(item).get('running_in') is None
        closed[kind].append(item)
    answer = call(['action', 'validate'], dict(action_type='start_' + kind, parameters=params))
    assert answer['valid'], answer
    heads = answer['basis']
    print('start_' + kind + '_basis_heads=' + str(len(heads)))
    version = subprocess.check_output([binary, '--version'], text=True, timeout=10).strip()
    print(version)
    # Requires an engine reading reverse links from postings (ontology-platform 2654024 or later).
    expected = [task, another_kind] if kind == 'review' else [task, thread, plan]
    assert len(heads) == len(expected), (kind, heads, expected)
    for item in closed[kind]:
        assert not any(h['id'] == item for h in heads), (kind, item, heads)
    print('basis_closed_absent=' + kind)
print('v7_gate_rows_complete')
# Version 8: install attempts and revoked reviews. A review started under version 7 stays running.
v7_running = created(gate('start_review', 'v7-still-running', dict(rp, head='f' * 40)), 'review')
# rp8 is a packet review of head8 in repo8. What the host holds at its upgrade: reviews closed with accept under version 7.
repo8, head8 = 'owner/repo', '1' * 40
rp8 = dict(rp, repository=repo8, head=head8)
closed8 = dict(verdict='accept', report='/home/palantirkc/work/fixture/REPORT.md', report_sha256='c' * 64)
v7_closed = {}
for label, head in [('admitted', '8' * 40), ('revoked', '9' * 40)]:
    item = created(apply('start_review', 'v7-closed-' + label, dict(rp8, head=head)), 'review')
    apply('close_review', 'v7-closed-close-' + label, dict(closed8, review=item))
    v7_closed[label] = item
call(['schema', 'install', str(pathlib.Path(repo, 'ontology/work.schema.json'))])
assert fields(next_review)['status'] == 'closed' and fields(next_review)['verdict'] == 'accept'
gate('close_review', 'v8-upgrade-running-review-closes', dict(review=v7_running, verdict='abandoned'))
assert fields(v7_running)['status'] == 'closed' and fields(v7_running).get('running_in') is None
# The search of ap unit (crates/ap/src/unit.rs reviewed) requires basis.qualified. On this engine a review
# that lacks a filtered optional property (repository, verdict, report_sha256) is counted in
# unresolved_excluded and the answer is not qualified, for a review of any schema version. That basis is
# printed as evidence. The row below is the read a caller can rely on: a search by properties that every
# review has (kind, head, status) is qualified; verdict, repository and the report's digest are then
# compared on the objects of the answer.
query = [['kind', 'packet'], ['repository', repo8], ['head', '8' * 40], ['status', 'closed'], ['verdict', 'accept'], ['report_sha256', 'c' * 64], ['task', task]]
found = call(['search'], dict(object_type='review', where={'all': [{'eq': q} for q in query]}, limit=1))
assert [o['id'] for o in found['objects']] == [v7_closed['admitted']], found
print('v8_upgrade_search_basis=' + json.dumps(found['basis'], sort_keys=True))
plain = call(['search'], dict(object_type='review', where={'all': [{'eq': q} for q in [['kind', 'packet'], ['head', '8' * 40], ['status', 'closed']]]}))
assert [o['id'] for o in plain['objects']] == [v7_closed['admitted']] and plain['basis']['qualified'] is True, plain
assert fields(v7_closed['admitted'])['verdict'] == 'accept' and fields(v7_closed['admitted'])['repository'] == repo8
print('gate_row=v8-upgrade-closed-review-qualified expected=admitted')
gate('revoke_review', 'v8-upgrade-closed-review-revoked', dict(review=v7_closed['revoked'], reason='revoked after the upgrade'))
assert fields(v7_closed['revoked'])['status'] == 'revoked'
at_now = datetime.datetime.fromtimestamp(now, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
def review8(rid, verdict='accept', **changes):
    item = created(apply('start_review', 'v8-review-' + rid, dict(rp8, **changes)), 'review')
    if verdict: apply('close_review', 'v8-review-close-' + rid, dict(closed8, review=item, verdict=verdict))
    return item
accepting = review8('accepting')
running8 = review8('running', None, head='5' * 40)
repair8 = review8('repair', 'repair', head='6' * 40)
integrated8 = review8('integrated', kind='integrated')
to_revoke = review8('to-revoke', head='3' * 40)
gate('revoke_review', 'v8-revoke', dict(review=to_revoke, reason='report missed a path'))
f = fields(to_revoke)
assert f['status'] == 'revoked' and f['revoked_at'] == at_now and f['revoke_reason'] == 'report missed a path', f
assert f['verdict'] == 'accept' and f['closed_at'] and f.get('running_in') is None
gate('revoke_review', 'v8-revoke-twice', dict(review=to_revoke, reason='again'), 'review_not_closed')
gate('revoke_review', 'v8-revoke-running', dict(review=running8, reason='not closed'), 'review_not_closed')
gate('revoke_review', 'v8-revoke-long-reason', dict(review=accepting, reason='x' * 101), 'invalid_parameter')
# A revoked review is not a closed one for any other reader of the store.
gate('record_scope_review', 'v8-scope-revoked-review', dict(review, review=to_revoke), 'review_not_closed')
assert to_revoke not in [o['id'] for o in call(['search'], dict(object_type='review', where={'eq': ['status', 'closed']}))['objects']]
# installed_artifact is owned by a source of four paths under /usr/local/bin that need not exist where the
# rehearsal runs. The rows run on a copy that differs only in those paths (two files of this store's
# directory) and in its version.
schema8 = json.loads(pathlib.Path(repo, 'ontology/work.schema.json').read_text())
binaries = [pathlib.Path(store).with_name(n) for n in ('installed-a', 'installed-b')]
for b in binaries: b.write_bytes(b'fixture\n')
copy = json.loads(json.dumps(schema8)); copy['version'] = 9
copy['sources']['installed_binaries']['paths'] = [str(b) for b in binaries]
assert {k: v for k, v in copy.items() if k not in ('version', 'sources')} == {k: v for k, v in schema8.items() if k not in ('version', 'sources')}
assert {k: v for k, v in copy['sources'].items() if k != 'installed_binaries'} == {k: v for k, v in schema8['sources'].items() if k != 'installed_binaries'}
assert {k: v for k, v in copy['sources']['installed_binaries'].items() if k != 'paths'} == {k: v for k, v in schema8['sources']['installed_binaries'].items() if k != 'paths'}
fixture = pathlib.Path(store).with_name('v8-installed-paths.json'); fixture.write_text(json.dumps(copy))
call(['schema', 'install', str(fixture)]); call(['index', '--source', 'installed_binaries'])
ta, tb = (oid('installed_artifact', str(b)) for b in binaries)
obj(ta); obj(tb)
ip = {'target': ta, 'kind': 'forward', 'repository': repo8, 'commit': '2' * 40, 'attempt': 1, 'from': '4' * 40, 'by': 'rehearsal-timer', 'head': head8, 'review': accepting, 'reviews': [accepting]}
started = gate('start_install', 'v8-install-start', ip)
# The command ap deliver parses this shape; change both together.
shape(started, 'install', ['initial'])
install = created(started, 'install'); f = fields(install)
assert f['status'] == 'running' and f['running_in'] == ta and f['target'] == ta and f['started_at'] == at_now, f
assert all(f[k] == v for k, v in ip.items()), f
gate('start_install', 'v8-install-second-running', dict(ip, attempt=2), 'install_running')
other_target = created(gate('start_install', 'v8-install-other-target', dict(ip, target=tb)), 'install')
apply('close_install', 'v8-close-other-target', dict(install=other_target, outcome='not_started', to='4' * 40))
gate('start_install', 'v8-install-unknown-target', dict(ip, target=oid('installed_artifact', '/usr/local/bin/absent-rehearsal')), 'unknown_target')
before = fingerprint()
apply('start_install', 'gate-start_install-v8-install-start', ip, outcome='already_applied')
assert fingerprint() == before
assert [o['id'] for o in call(['search'], dict(object_type='install', where={'eq': ['target', ta]}))['objects']] == [install]
print('gate_row=v8-install-same-request expected=already_applied')
closed = gate('close_install', 'v8-install-close', dict(install=install, outcome='installed', to='2' * 40, code='ok'))
# The command ap deliver parses this shape; change both together.
shape(closed, 'install', ['update', 'update'])
f = fields(install)
assert f['status'] == 'closed' and f['outcome'] == 'installed' and f['to'] == '2' * 40 and f.get('running_in') is None and f['closed_at'], f
assert f['code'] == 'ok' and f['target'] == ta and 'rollback_binary' not in f and 'rollback_content' not in f
gate('close_install', 'v8-install-close-twice', dict(install=install, outcome='failed', to='2' * 40), 'install_not_running')
second = created(gate('start_install', 'v8-install-second-attempt', dict(ip, attempt=2, from_sha256='d' * 64)), 'install')
assert fields(second)['from_sha256'] == 'd' * 64
handles = dict(rollback_binary='/home/palantirkc/work/fixture/ap.previous', rollback_content='/home/palantirkc/work/fixture/content.tar')
gate('close_install', 'v8-install-close-handles', dict(install=second, outcome='failed_restored', to='4' * 40, **handles))
f = fields(second)
assert all(f[k] == v for k, v in handles.items()) and f['status'] == 'closed' and f['outcome'] == 'failed_restored', f
without = {k: v for k, v in ip.items() if k not in ('review', 'reviews')}
for rid, params in [
    ('review-running', dict(ip, review=running8, reviews=[running8], head='5' * 40)),
    ('review-repair', dict(ip, review=repair8, reviews=[repair8], head='6' * 40)),
    ('review-other-head', dict(ip, head='7' * 40)),
    ('review-other-repository', dict(ip, repository='owner/other')),
    ('review-other-kind', dict(ip, review=integrated8, reviews=[integrated8])),
    ('forward-without-review', without),
    ('review-revoked', dict(ip, review=to_revoke, reviews=[to_revoke], head='3' * 40)),
    ('review-unknown', dict(ip, review=oid('review', 'absent-rehearsal-review'))),
]:
    gate('start_install', 'v8-install-' + rid, params, 'review_not_accepting')
upgraded = created(gate('start_install', 'v8-upgrade-closed-review-admitted', dict(ip, head='8' * 40, review=v7_closed['admitted'], reviews=[v7_closed['admitted']])), 'install')
apply('close_install', 'v8-close-upgraded', dict(install=upgraded, outcome='installed', to='2' * 40))
back = {k: v for k, v in without.items() if k != 'head'} | {'kind': 'rollback', 'commit': '4' * 40, 'from': '2' * 40}
gate('start_install', 'v8-install-rolls-back-unknown', dict(back, rolls_back=oid('install', 'absent-rehearsal-install')), 'unknown_rolled_back')
now += 1
rollback = created(gate('start_install', 'v8-install-rollback', dict(back, rolls_back=install)), 'install')
assert 'review' not in fields(rollback) and fields(rollback)['kind'] == 'rollback' and fields(rollback)['rolls_back'] == install
apply('close_install', 'v8-close-rollback', dict(install=rollback, outcome='installed', to='4' * 40))
installs = call(['search'], dict(object_type='install', limit=1000))['objects']
view = call(['render'], dict(view='installs', args={}))
assert view['rows'] == len(installs) == 5 and not view['truncated'], view
lines = [l for l in view['markdown'].splitlines() if l.startswith('| ')]
assert lines[0] == '| Started | Kind | Commit | Attempt | Status | Outcome | To | By |', lines
assert lines[2].startswith('| ' + datetime.datetime.fromtimestamp(now, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ') + ' | rollback | ' + '4' * 40 + ' | 1 | closed | installed | '), lines
print(view['markdown'])
print('gate_row=v8-view-installs expected=admitted')
assert call(['render'], dict(view='reviews', args=dict(task=task['native_key'])))['rows'] > 0
print('v8_gate_rows_complete')
PY

python3 -I -B "$repo/ontology/memory-index-walk.py" "$ont_binary" "$rehearsal_root/memory-walk" --records 2
