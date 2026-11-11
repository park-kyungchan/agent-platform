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
    if failure: assert failure in json.dumps(r) and r['code'] in [failure, 'submission_criteria'], r
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
wp = dict(task=task, name='O13', tier='bounded', runtime='codex', packet=packet)
unchanged = fingerprint()
for action, params in [('set_hold', hp), ('record_decision', dp), ('define_kpi', kp), ('start_worker_run', wp)]:
    apply(action, 'unknown-' + action, dict(params, task=oid('task', 'invented-o13-task')), 'unknown_task')
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
# Cross-task memory index: only invented ids and hashes, never a server call.
mr = oid('memory_record', 'rehearsal-original'); ms = oid('memory_record', 'rehearsal-successor')
mpending, mready, msup = 'mark_memory_record_pending', 'mark_memory_record_ready', 'mark_memory_record_superseded'
def pending(record, digest, **extra): return dict(record=record, document_id=record['native_key'], body_hash=digest, **extra)
def memory_view(name): return call(['render'], dict(view=name, args={}))
apply(mpending, 'memory-start', pending(mr, 'a' * 64)); assert fields(mr)['status'] == 'pending'
assert memory_view('pending_memory_records')['rows'] == 1 and memory_view('ready_memory_records')['rows'] == 0
unchanged = fingerprint(); apply(mready, 'memory-wrong-hash', dict(record=mr, body_hash='b' * 64), 'memory_body_hash_mismatch'); assert fingerprint() == unchanged
apply(mready, 'memory-ready', dict(record=mr, body_hash='a' * 64))
unchanged = fingerprint(); apply(mready, 'memory-not-pending', dict(record=mr, body_hash='a' * 64), 'memory_record_not_pending'); assert fingerprint() == unchanged
apply(mpending, 'memory-correct', pending(mr, 'b' * 64)); assert fields(mr)['body_hash'] == 'b' * 64
apply(mready, 'memory-correct-ready', dict(record=mr, body_hash='b' * 64))
# Regression: a prematurely linked successor cannot become ready, and its
# predecessor can still enter pending to recover the intended batch order.
apply(mpending, 'memory-successor', pending(ms, 'c' * 64, supersedes=mr))
unchanged = fingerprint(); apply(mready, 'memory-predecessor-ready', dict(record=ms, body_hash='c' * 64), 'memory_predecessor_not_pending'); assert fingerprint() == unchanged
assert memory_view('ready_memory_records')['rows'] == 1 and memory_view('pending_memory_records')['rows'] == 1
apply(mpending, 'memory-replace', pending(mr, 'b' * 64))
unchanged = fingerprint(); apply(mready, 'memory-predecessor-reopen', dict(record=mr, body_hash='b' * 64), 'memory_record_has_successor'); assert fingerprint() == unchanged
unchanged = fingerprint(); apply(msup, 'memory-not-ready-successor', dict(record=mr, successor=ms), 'memory_successor_not_ready'); assert fingerprint() == unchanged
unchanged = fingerprint(); apply(mpending, 'memory-bad-status', pending(ms, 'c' * 64, status='unknown'), 'invalid_parameter'); assert fingerprint() == unchanged
apply(mready, 'memory-successor-ready', dict(record=ms, body_hash='c' * 64))
apply(msup, 'memory-supersede', dict(record=mr, successor=ms))
assert fields(mr)['status'] == 'superseded' and fields(ms)['supersedes'] == mr
unchanged = fingerprint(); apply(mpending, 'memory-reopen', pending(mr, 'd' * 64), 'memory_record_has_ready_successor'); assert fingerprint() == unchanged
assert memory_view('pending_memory_records')['rows'] == 0
memory_rows = memory_view('ready_memory_records')
assert memory_rows['rows'] == 1 and not memory_rows['truncated'] and ms['native_key'] in memory_rows['markdown'] and 'c' * 64 in memory_rows['markdown']
print(memory_rows['markdown'])
requests = [dict(jsonrpc='2.0', id=i, method='tools/call', params=dict(name=n, arguments={})) for i, n in enumerate(['ont_describe', 'ont_action_list'], 1)]
wire = subprocess.run([binary, '--store', store, 'mcp'], input='\n'.join(map(json.dumps, requests))+'\n', text=True, capture_output=True, timeout=60); assert wire.returncode == 0, wire.stderr
for request, line in zip(requests, wire.stdout.splitlines(), strict=True):
    answer = json.loads(line)['result']; assert not answer['isError'], answer
    if request['params']['name'] == 'ont_action_list': assert answer['content'][0]['text'].count('Actions take the full Id from search; view arguments take the native key.') == 10
    size = len(answer['content'][0]['text'].encode()); assert size <= 262144; print(request['params']['name'] + '_bytes=' + str(size), file=sys.stderr)
expected = {'kpi_board': (1, '| checks | 14 | 2026-11-11T02:00:02Z | 14 | ontology/rehearsal.sh |'), 'active_holds': (1, '| publish | review accepted |'), 'current_decisions': (1, '| route | use \\| rendered actions | lead | supersede |'), 'live_workers': (2, 'check \\| next line'), 'closed_workers': (2, 'pending passed')}
for view, (count, text) in expected.items():
    r = render(view); assert r['rows'] == count and not r['truncated'] and text in r['markdown'], r
    assert render(view, 'absent-o13-task')['rows'] == 0
    if view in ['live_workers', 'closed_workers']: assert packet['native_key'] in r['markdown'] and pending_packet['native_key'] in r['markdown']
    if view == 'live_workers': assert '(missing)' in r['markdown']
    if view == 'closed_workers': assert report['native_key'] in r['markdown'] and pending_report['native_key'] in r['markdown'] and r['markdown'].index('pending passed') < r['markdown'].index('| passed |')
    print(r['markdown'])
PY
