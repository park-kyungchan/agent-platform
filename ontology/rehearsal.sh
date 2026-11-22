#!/usr/bin/env bash
set -euo pipefail
[[ $# == 1 ]] || { echo 'usage: rehearsal.sh /path/to/ont' >&2; exit 2; }
ont_binary=$(realpath -- "$1")
# ontology/migrate runs the engine that $ONT_BIN names.
export ONT_BIN="$ont_binary"
repo=$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
rehearsal_root=$(mktemp -d)
# The engine refuses a root reached through a symbolic link: the one root of every copy is a real path.
rehearsal_root=$(realpath -- "$rehearsal_root")
trap 'python3 -c '\''import shutil,sys; shutil.rmtree(sys.argv[1])'\'' "$rehearsal_root"' EXIT
# The index sources of the schema name the host's work directory. The rehearsal indexes the fixture files of the
# repository, copied once under its own root, and installs every schema file as a copy whose strings under the
# host's work directory (the paths of `sources`, and the arguments of the transforms of the keys and properties of
# the object types) begin with that root instead. The copies of the two Python steps share one directory.
work_root="$rehearsal_root/work"
mkdir -- "$work_root" "$rehearsal_root/copies" "$rehearsal_root/lib"
cp -R -- "$repo/ontology/fixtures/work/." "$work_root/"
cat > "$rehearsal_root/lib/redirect.py" <<'PY'
import copy as _copy, json, pathlib, sys
HOST = '/home/palantirkc/work/'
def convert(value, change):
    if isinstance(value, str): return change(value)
    if isinstance(value, list): return [convert(v, change) for v in value]
    if isinstance(value, dict): return {k: convert(v, change) for k, v in value.items()}
    return value
def groups(schema, change):
    # Returns a copy of the schema in which `change` is applied to the strings of the two groups: those of `sources`
    # (no description among them), and those of the `transform` of a primary_key or of a property of an object type.
    out = _copy.deepcopy(schema)
    out['sources'] = convert(out['sources'], change)
    for kind in out['object_types'].values():
        for holder in [kind.get('primary_key', {}), *kind.get('properties', {}).values()]:
            if isinstance(holder.get('transform'), dict): holder['transform'] = convert(holder['transform'], change)
    return out
def strings(schema, begin):
    found = {'sources': [], 'transforms': []}
    def count(where):
        def inner(s):
            if s.startswith(begin): found[where].append(s)
            return s
        return inner
    probe = _copy.deepcopy(schema)
    convert(probe['sources'], count('sources'))
    for kind in probe['object_types'].values():
        for holder in [kind.get('primary_key', {}), *kind.get('properties', {}).values()]:
            if isinstance(holder.get('transform'), dict): convert(holder['transform'], count('transforms'))
    return len(found['sources']), len(found['transforms'])
def make(path, root, copies, label=None, binaries=None):
    # Writes the copy of one schema file, whose strings of both groups under the host's work directory begin with
    # `root` (ending in a slash) instead; `binaries` replaces the paths of installed_binaries. Returns (path, copy).
    original = json.loads(pathlib.Path(path).read_text())
    assert root.endswith('/') and not root.startswith(HOST)
    expected = strings(original, HOST)
    copy = groups(original, lambda s: root + s[len(HOST):] if s.startswith(HOST) else s)
    assert strings(copy, HOST) == (0, 0) and strings(copy, root) == expected, (path, expected)
    restored = groups(copy, lambda s: HOST + s[len(root):] if s.startswith(root) else s)
    if binaries is not None:
        copy['sources']['installed_binaries']['paths'] = binaries
        restored['sources']['installed_binaries']['paths'] = original['sources']['installed_binaries']['paths']
    assert restored == original, path
    out = pathlib.Path(copies, '%02d-%s' % (len(list(pathlib.Path(copies).iterdir())), label or pathlib.Path(path).name))
    out.write_text(json.dumps(copy))
    print('redirect_copy=' + out.name + ' version=' + str(copy['version']) + ' source_paths=' + str(expected[0]) + ' transform_arguments=' + str(expected[1]), file=sys.stderr)
    return out, copy
def trace(root, copies):
    total = 0
    for file in sorted(pathlib.Path(copies).iterdir()):
        schema = json.loads(file.read_text())
        assert strings(schema, HOST) == (0, 0), file.name
        total += sum(strings(schema, root))
    assert total > 0
    print("rehearsal_sources=" + str(total) + " redirected, 0 under the host's work directory", file=sys.stderr)
PY
python3 - "$ont_binary" "$repo" "$rehearsal_root/store.sqlite" "$work_root/" "$rehearsal_root/copies" "$rehearsal_root/lib" <<'PY'
import datetime, hashlib, json, pathlib, subprocess, sys
binary, repo, store, work, copies, lib = sys.argv[1:]
sys.path.insert(0, lib)
import redirect
def redirected(path, label=None): return redirect.make(path, work, copies, label)[0]
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
call(['schema', 'install', str(redirected(first))]); call(['index', '--source', 'task_states', '--source', 'packets', '--source', 'reports'])
revision = obj(task)['observations'][-1]['revision']; obj(packet); obj(report)
v2 = subprocess.check_output(['git', '-C', repo, 'show', '2b9417c8bc96b0cbeb4ab7913aa44cb49ad8d604:ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v2).hexdigest() == '27de5fd02c20299897720ab34543e04e6db685438c3dd7409691cb8ddae11f24'
second = pathlib.Path(store).with_name('v2.json'); second.write_bytes(v2)
call(['schema', 'install', str(redirected(second))])
probe = created(apply('set_hold', 'v2-probe', dict(task=task, effect='upgrade probe', until='schema v3')), 'hold')
apply('lift_hold', 'v2-probe-lift', dict(hold=probe))
# Versions 2 to 4 took a row of the user without words; such a row stays current and is triaged after version 9.
early_user = created(apply('record_decision', 'v2-user-row-without-words', dict(task=oid('task', 'ontology-operational-base'), topic='v2-user-approval', text='an approval', decided_by='user', undo='supersede')), 'decision')
assert 'user_words' not in fields(early_user) and fields(early_user)['decided_by'] == 'user' and fields(early_user)['status'] == 'current'
probe_revision = obj(probe)['head']
# Preserve a real v5 flag-only review across the upgrade to v6.
legacy = pathlib.Path(store).with_name('v5.json')
legacy.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', 'f46cdf49ba74e117ddaaf387416a0eac1d53f2af:ontology/work.schema.json'], timeout=10))
call(['schema', 'install', str(redirected(legacy))])
legacy_thread = created(apply('open_thread', 'legacy-thread', dict(task=task, key='legacy-v5', consumer='fixture', consumer_kind='harness', first_use='upgrade')), 'thread')
legacy_plan = created(apply('open_plan', 'legacy-plan', dict(thread=legacy_thread, first_lines=100, limit_lines=150, current_lines=151, amended_by_family='family-a')), 'plan')
apply('record_scope_review', 'legacy-review', dict(plan=legacy_plan, reviewer_family='family-b', review_report='fixture/v5.md'))
v6 = pathlib.Path(store).with_name('v6.json')
v6.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', '8ad36bf3cf10e19659965cc2d458a921533cdfae:ontology/work.schema.json'], timeout=10))
call(['schema', 'install', str(redirected(v6))])
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
call(['schema', 'install', str(redirected(v7))])
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
# Version 8 is pinned in Git: its rows run on it, then version 9 is installed over the store they leave.
v8 = pathlib.Path(store).with_name('v8.json')
v8.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', '64fb1f59c6ff426807886d5a10397f5be3e496fa:ontology/work.schema.json'], timeout=10))
assert hashlib.sha256(v8.read_bytes()).hexdigest() == '8b4b4236d9e5cff8ce3286211682cc977f4cdf1fdbfb36b42aefa08021e04d23'
call(['schema', 'install', str(redirected(v8))])
assert fields(next_review)['status'] == 'closed' and fields(next_review)['verdict'] == 'accept'
gate('close_review', 'v8-upgrade-running-review-closes', dict(review=v7_running, verdict='abandoned'))
assert fields(v7_running)['status'] == 'closed' and fields(v7_running).get('running_in') is None
# The search ap unit made before it read by required properties: every reader requires basis.qualified.
# On this engine a filter on an optional property (repository, verdict, report_sha256) answers
# qualified: false as soon as one scanned review lacks it (a running or abandoned review, of any schema
# version), while unresolved_excluded stays 0. That basis is printed as evidence. The row below is the read
# a caller can rely on, and the one ap unit and ap deliver make (accepting in crates/ap/src/review.rs): a
# search by properties that every review has (kind, head, status) is qualified; verdict, repository and the
# report's digest are then compared on the objects of the answer.
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
# Version 9: every object written under version 8 so far reads the same after the install, and the actions of version 8 still apply.
v8_objects = [task, packet, report, h, d, successor, early_user, k, w, thread, plan, first_review, next_review, running8, accepting, to_revoke, v7_running, v7_closed['admitted'], v7_closed['revoked']]
v8_before = [obj(i) for i in v8_objects]
v8_views = {view: call(['render'], dict(view=view, args={} if view in ('current_memory_records', 'installs') else dict(task=task['native_key']))) for view in json.loads(v8.read_text())['views']}
# Version 9 is pinned in Git: its rows run on it, then the repository's file is installed over the store they leave.
v9 = pathlib.Path(store).with_name('v9.json')
v9.write_bytes(subprocess.check_output(['git', '-C', repo, 'show', '8379ae651929853d6fb2df79559ece4a98c5f551:ontology/work.schema.json'], timeout=10))
assert hashlib.sha256(v9.read_bytes()).hexdigest() == 'faba52d93401384cdc6146a7f505c61dafc4a87662deb284dfc689f19a58d298'
call(['schema', 'install', str(redirected(v9))])
assert [obj(i) for i in v8_objects] == v8_before
for view, before in v8_views.items():
    answer = call(['render'], dict(view=view, args={} if view in ('current_memory_records', 'installs') else dict(task=task['native_key'])))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@8', 'schema=work@9'), view
print('gate_row=v9-v8-objects-and-views-unchanged expected=admitted objects=' + str(len(v8_objects)) + ' views=' + str(len(v8_views)))
after_v9 = created(gate('record_decision', 'v9-record-decision', dict(dp, topic='after-v9')), 'decision')
gate('start_review', 'v9-duplicate-of-v8-review', dict(rp8, head='5' * 40), 'duplicate_running_review')
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('gate_row=v9-mcp-sizes expected=admitted bytes=' + ','.join(map(str, sizes)))
# installed_artifact is owned by a source of four paths under /usr/local/bin that need not exist where the
# rehearsal runs. The rows from here on run on the repository's file, as the one copy that also moves those paths
# (two files of this store's directory) and the host's work directory (see the top of this file); it keeps the
# file's own version, installed over version 9.
repository_schema = json.loads(pathlib.Path(repo, 'ontology/work.schema.json').read_text())
binaries = [pathlib.Path(store).with_name(n) for n in ('installed-a', 'installed-b')]
for b in binaries: b.write_bytes(b'fixture\n')
fixture, copy = redirect.make(pathlib.Path(repo, 'ontology/work.schema.json'), work, copies, 'repository-installed-paths.json', [str(b) for b in binaries])
assert copy['version'] > 9
# Every object and view asserted under version 9 reads the same after the install of the repository's file.
v9_objects = v8_objects + [after_v9]
v9_before = [obj(i) for i in v9_objects]
v9_views = {view: call(['render'], dict(view=view, args={} if view in ('current_memory_records', 'installs') else dict(task=task['native_key']))) for view in json.loads(v9.read_text())['views']}
call(['schema', 'install', str(fixture)])
assert [obj(i) for i in v9_objects] == v9_before
for view, before in v9_views.items():
    answer = call(['render'], dict(view=view, args={} if view in ('current_memory_records', 'installs') else dict(task=task['native_key'])))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@9', 'schema=work@' + str(copy['version'])), view
print('gate_row=v10-v9-objects-and-views-unchanged expected=admitted objects=' + str(len(v9_objects)) + ' views=' + str(len(v9_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('gate_row=v10-mcp-sizes expected=admitted bytes=' + ','.join(map(str, sizes)))
print('v10_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
call(['index', '--source', 'installed_binaries'])
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
# Version 17 on (the repository's file is installed here): an install names its host, a reference no criterion reads.
with_host = created(gate('start_install', 'v17-install-host', dict(ip, attempt=3, host=oid('host', 'fixture-host'))), 'install')
assert fields(with_host)['host'] == oid('host', 'fixture-host') and fields(with_host)['status'] == 'running'
apply('close_install', 'v17-close-install-host', dict(install=with_host, outcome='installed', to='2' * 40))

# Version 9: the objective, the backlog item, the order of a task and the triage of the user's rows. Each refusal row is before the action it guards.
def stamp(): return datetime.datetime.fromtimestamp(now, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
def tick():
    global now
    now += 1
def row(topic, owner=None, who='user'):
    tick()
    words = dict(user_words='"v9 ' + topic + '."', user_form='decision') if who == 'user' else {}
    return created(gate('record_decision', 'v9-row-' + topic, dict(task=owner or task, topic='v9-' + topic, text='v9 ' + topic, decided_by=who, undo='supersede', **words)), 'decision')
invented = oid('task', 'invented-v9-task')
lead_row, u_early, u_src, u_old, uo = row('lead', who='lead'), row('early'), row('source'), row('old'), row('other', other_task)
gate('supersede_decision', 'v9-row-old-superseded', dict(decision=u_old, text='v9 old, replaced', decided_by='user', user_words='"v9 old, replaced."', user_form='decision', undo='supersede'))
assert fields(u_old)['status'] == 'superseded'
tick()
op = dict(task=task, name='ship-board', statement='the board orders the work', check='true', rank=1, source=u_src)
gate('state_objective', 'unknown-task', dict(op, task=invented), ('unknown_task', 'source_not_user_words'))
for label, source in [('lead-row', lead_row), ('other-task-row', uo), ('superseded-row', u_old)]:
    gate('state_objective', label, dict(op, source=source), 'source_not_user_words')
o1 = created(gate('state_objective', 'o1', op), 'objective')
f = fields(o1)
assert f['status'] == 'stated' and f['source'] == u_src and f['rank'] == 1 and f['stated_at'] == stamp() and f['statement'] == op['statement'], f
gate('state_objective', 'name-used', dict(op, statement='again'), 'name_used')
# confirm_objective
tick(); lead_late = row('lead-late', who='lead'); uo_late = row('other-late', other_task); u_late = row('late')
for label, r, code in [('lead-row', lead_late, 'row_not_user_words'), ('other-task-row', uo_late, 'row_not_user_words'), ('source', u_src, 'row_is_source_or_earlier'), ('earlier-row', u_early, 'row_is_source_or_earlier')]:
    gate('confirm_objective', label, dict(objective=o1, row=r), code)
tick()
gate('confirm_objective', 'o1', dict(objective=o1, row=u_late))
f = fields(o1); assert f['status'] == 'confirmed' and f['confirmed_by'] == u_late and f['confirmed_at'] == stamp(), f
gate('confirm_objective', 'not-stated', dict(objective=o1, row=u_late), 'objective_not_stated')
# restate_objective
tick(); lead_after = row('lead-after-confirm', who='lead'); uo_after = row('other-after-confirm', other_task)
rp9 = dict(objective=o1, statement='the board orders and refuses')
gate('restate_objective', 'lead-row', dict(rp9, source=lead_after), 'source_not_user_words')
gate('restate_objective', 'other-task-row', dict(rp9, source=uo_after), 'source_not_user_words')
gate('restate_objective', 'row-before-confirmation', dict(rp9, source=u_src), 'confirmed_needs_later_row')
gate('restate_objective', 'row-of-the-confirmation', dict(rp9, source=u_late), 'confirmed_needs_later_row')
u_re = row('restate')
gate('restate_objective', 'o1', dict(rp9, source=u_re))
f = fields(o1)
assert f['status'] == 'stated' and f['statement'] == rp9['statement'] and f['source'] == u_re and f.get('confirmed_by') is None and f['stated_at'] == stamp(), f
seen = [o['fields']['statement']['value'] for o in obj(o1)['observations']]
statements = [x for n, x in enumerate(seen) if n == 0 or x != seen[n - 1]]
assert statements == [op['statement'], rp9['statement']], statements
print('gate_row=v9-earlier-statement-read-back expected=admitted statements=' + json.dumps(statements))
# An objective that is not confirmed is restated with any current row of the user, also one earlier than a kept confirmed_at.
gate('restate_objective', 'stated-with-earlier-row', dict(rp9, statement='the board orders, refuses and shows', source=u_early))
assert fields(o1)['status'] == 'stated' and fields(o1)['source'] == u_early
gate('restate_objective', 'again-with-later-row', dict(rp9, statement='the board orders, refuses and shows it', source=u_re))
o3 = created(gate('state_objective', 'o3', {k: v for k, v in dict(op, name='never-confirmed', rank=3).items() if k != 'check'}), 'objective')
assert 'confirmed_at' not in fields(o3) and 'check' not in fields(o3)
gate('restate_objective', 'never-confirmed-earlier-row', dict(objective=o3, statement='never confirmed, restated', source=u_early))
assert fields(o3)['source'] == u_early and fields(o3)['status'] == 'stated'
gate('record_check', 'no-check-command', dict(objective=o3, exit=0, line='x'), 'objective_without_check')
assert 'last_exit' not in fields(o3)
# amend_objective
am = dict(objective=o1, rank=2, decision=lead_row)
gate('amend_objective', 'nothing', dict(objective=o1, decision=lead_row), 'nothing_to_amend')
gate('amend_objective', 'superseded-row', am | dict(decision=d), 'row_not_current')
gate('amend_objective', 'other-task-row', am | dict(decision=uo), 'row_not_current')
gate('amend_objective', 'rank', am)
assert fields(o1)['rank'] == 2 and fields(o1)['check'] == 'true' and fields(o1)['status'] == 'stated'
gate('amend_objective', 'check', dict(objective=o1, check='test -f ok', decision=lead_row))
assert fields(o1)['rank'] == 2 and fields(o1)['check'] == 'test -f ok'
# record_check, and retire_objective of a confirmed objective
gate('record_check', 'o1', dict(objective=o1, exit=1, line='not yet'))
f = fields(o1); assert f['last_exit'] == 1 and f['last_line'] == 'not yet' and f['last_checked_at'] == stamp(), f
tick(); u_conf = row('confirm-again'); tick()
gate('confirm_objective', 'o1-again', dict(objective=o1, row=u_conf))
tick(); lead_ret = row('lead-retire', who='lead'); uo_ret = row('other-retire', other_task)
rt = dict(objective=o1, reason='replaced by a later objective')
gate('retire_objective', 'confirmed-no-row', rt, 'confirmed_needs_later_row')
gate('retire_objective', 'confirmed-earlier-row', dict(rt, row=u_late), 'confirmed_needs_later_row')
gate('retire_objective', 'confirmed-row-of-the-confirmation', dict(rt, row=u_conf), 'confirmed_needs_later_row')
gate('retire_objective', 'confirmed-lead-row', dict(rt, row=lead_ret), 'confirmed_needs_later_row')
gate('retire_objective', 'confirmed-other-task-row', dict(rt, row=uo_ret), 'confirmed_needs_later_row')
u_ret = row('retire')
gate('retire_objective', 'o1', dict(rt, row=u_ret))
f = fields(o1); assert f['status'] == 'retired' and f['retire_reason'] == rt['reason'] and f['retired_at'] == stamp(), f
gate('retire_objective', 'again', rt, 'objective_not_live')
gate('restate_objective', 'retired', dict(rp9, source=u_ret), 'objective_not_live')
gate('amend_objective', 'retired', dict(objective=o1, rank=5, decision=lead_row), 'objective_not_live')
gate('record_check', 'retired', dict(objective=o1, exit=0, line='x'), 'objective_not_live')
gate('state_objective', 'name-of-a-retired-objective', op, 'name_used')
o2 = created(gate('state_objective', 'o2', dict(op, name='second', rank=4)), 'objective')
gate('retire_objective', 'stated-no-row', dict(objective=o2, reason='not wanted'))
assert fields(o2)['status'] == 'retired'
o_other = created(gate('state_objective', 'o-other', dict(op, task=other_task, name='other', source=uo)), 'objective')
print('v9_objective_rows_complete')
# item
item_params = dict(task=task, key='k-queued', title='a queued item', statement='the state it wants', objective=o3, origin=u_src, first_use='the Lead, in the run of the next message')
def add(key, **changes): return created(gate('add_item', 'add-' + key, dict(item_params, key=key, **changes)), 'item')
gate('add_item', 'unknown-task', dict(item_params, task=invented), ('unknown_task', 'origin_not_current', 'objective_not_live'))
gate('add_item', 'origin-superseded', dict(item_params, origin=d), 'origin_not_current')
gate('add_item', 'origin-superseded-user-row', dict(item_params, origin=u_old), 'origin_not_current')
gate('add_item', 'origin-other-task', dict(item_params, origin=uo), 'origin_not_current')
gate('add_item', 'objective-retired', dict(item_params, objective=o2), 'objective_not_live')
gate('add_item', 'objective-other-task', dict(item_params, objective=o_other), 'objective_not_live')
i1 = add('k-unit'); i2 = add('k-evidence', origin=lead_row); i3 = add('k-drop'); i4 = add('k-unordered'); i5 = add('k-rework'); i6 = add('k-drop-in-work')
f = fields(i1); assert f['state'] == 'queued' and f['objective'] == o3 and f['origin'] == u_src and f['added_at'] == stamp() and 'repository' not in f, f
gate('add_item', 'key-used', dict(item_params, key='k-unit'), 'key_used')
other_item = created(gate('add_item', 'add-other', dict(item_params, task=other_task, key='k-other', objective=o_other, origin=uo)), 'item')
other_item2 = created(gate('add_item', 'add-other-2', dict(item_params, task=other_task, key='k-other-2', objective=o_other, origin=uo)), 'item')
gate('amend_item', 'objective-retired', dict(item=i4, objective=o2), 'objective_not_live')
gate('amend_item', 'objective-other-task', dict(item=i4, objective=o_other), 'objective_not_live')
gate('amend_item', 'i4', dict(item=i4, title='an unordered item', first_use='the user, in a progress message', objective=o3))
f = fields(i4); assert f['title'] == 'an unordered item' and f['first_use'] == 'the user, in a progress message' and f['statement'] == item_params['statement'], f
# start_item before the task has a board
gate('start_item', 'no-board', dict(item=i1, repository=repo8, branch='unit/one'), 'item_not_ordered')
# set_order
board = oid('board', task['native_key']); other_board = oid('board', other_task['native_key'])
so = dict(board=board, task=task, order=[i2, i1, i3, i5, i6], reason='the order of the next work')
gate('set_order', 'unknown-task', dict(so, task=invented), 'unknown_task')
gate('set_order', 'first', so)
f = fields(board); assert f['task'] == task and f['order'] == so['order'] and f['reason'] == so['reason'] and set(f) == {'task', 'order', 'reason', 'set_at'} and f['set_at'] == stamp(), f
gate('set_order', 'other-board', dict(so, board=other_board, order=[other_item], task=other_task))
gate('set_order', 'second-board-of-the-task', dict(so, board=oid('board', 'second-board-of-a-task')), 'board_of_another_task')
gate('set_order', 'board-of-another-task', dict(so, board=other_board), 'board_of_another_task')
# The store does not check the elements of the list: an item of another task, a dangling element and an empty list are stored.
unchecked = [i6, other_item2, oid('item', 'absent-v9-item'), i1]
tick(); gate('set_order', 'unchecked-elements', dict(so, order=unchecked))
assert fields(board)['order'] == unchecked
gate('set_order', 'empty', dict(so, order=[])); assert fields(board)['order'] == []
gate('set_order', 'ordered', so); assert fields(board)['order'] == so['order']
gate('set_order', 'reordered', dict(so, order=[i2, i1, i5, i6, i3, other_item2])); assert fields(board)['order'] == [i2, i1, i5, i6, i3, other_item2]
assert len(call(['search'], dict(object_type='board'))['objects']) == 2
# start_item, release_item
gate('start_item', 'unordered', dict(item=i4), 'item_not_ordered')
gate('start_item', 'ordered-by-another-task-only', dict(item=other_item2), 'item_not_ordered')
gate('start_item', 'repository-only', dict(item=i1, repository=repo8), 'repository_and_branch')
gate('start_item', 'branch-only', dict(item=i1, branch='unit/one'), 'repository_and_branch')
gate('release_item', 'queued', dict(item=i1, reason='not started'), 'item_not_in_work')
tick(); gate('start_item', 'i1', dict(item=i1, repository=repo8, branch='unit/one'))
f = fields(i1); assert f['state'] == 'in_work' and f['repository'] == repo8 and f['branch'] == 'unit/one' and f['started_at'] == stamp(), f
gate('start_item', 'in-work', dict(item=i1, repository=repo8, branch='unit/one'), 'item_not_queued')
gate('start_item', 'i2', dict(item=i2)); f = fields(i2); assert f['state'] == 'in_work' and 'repository' not in f and 'branch' not in f, f
gate('start_item', 'i5', dict(item=i5, repository=repo8, branch='unit/rework'))
tick(); gate('release_item', 'i5', dict(item=i5, reason='closed without a merge'))
f = fields(i5); assert f['state'] == 'queued' and f['release_reason'] == 'closed without a merge' and f['repository'] == repo8, f
gate('start_item', 'released-without-repository', dict(item=i5), 'repository_and_branch')
gate('start_item', 'released-restart', dict(item=i5, repository=repo8, branch='unit/rework-2')); assert fields(i5)['branch'] == 'unit/rework-2'
gate('start_item', 'i6', dict(item=i6))
# close_item
def review9(rid, head, verdict='accept', **changes):
    r = created(gate('start_review', 'v9-review-' + rid, dict(rp8, head=head, **changes)), 'review')
    if verdict: gate('close_review', 'v9-review-close-' + rid, dict(closed8, review=r, verdict=verdict))
    return r
r_ok, r_ok2 = review9('ok', 'a1' * 20), review9('ok-2', 'a2' * 20)
r_other_repo, r_abandoned, r_running, r_revoked = review9('other-repo', 'a3' * 20, repository='owner/other'), review9('abandoned', 'a4' * 20, 'abandoned'), review9('running', 'a5' * 20, None), review9('revoked', 'a6' * 20)
gate('revoke_review', 'v9-review-revoke', dict(review=r_revoked, reason='revoked before the close'))
r_task = created(gate('start_review', 'v9-review-other-task', dict(rp8, task=other_task, head='a7' * 20)), 'review')
gate('close_review', 'v9-review-close-other-task', dict(closed8, review=r_task))
for label, extra in [('evidence-only', dict(evidence='cargo test: ok')), ('nothing', {}), ('review-and-evidence', dict(review=r_ok, evidence='cargo test: ok')),
                     ('review-of-another-repository', dict(review=r_other_repo)), ('review-abandoned', dict(review=r_abandoned)), ('review-running', dict(review=r_running)),
                     ('review-revoked', dict(review=r_revoked)), ('review-of-another-task', dict(review=r_task))]:
    gate('close_item', 'unit-' + label, dict(item=i1, **extra), 'review_or_evidence')
for label, extra in [('review', dict(review=r_ok)), ('nothing', {}), ('review-and-evidence', dict(review=r_ok, evidence='ran it: ok'))]:
    gate('close_item', 'no-repository-' + label, dict(item=i2, **extra), 'review_or_evidence')
gate('close_item', 'queued', dict(item=i4, evidence='x'), 'item_not_in_work')
tick(); gate('close_item', 'i1', dict(item=i1, review=r_ok))
f = fields(i1); assert f['state'] == 'done' and f['review'] == r_ok and f['closed_at'] == stamp() and 'evidence' not in f, f
gate('close_item', 'i2', dict(item=i2, evidence='ran it: ok'))
f = fields(i2); assert f['state'] == 'done' and f['evidence'] == 'ran it: ok' and 'review' not in f, f
gate('close_item', 'review-closes-another-item', dict(item=i5, review=r_ok), 'review_closes_another_item')
gate('close_item', 'i5', dict(item=i5, review=r_ok2)); assert fields(i5)['state'] == 'done'
# drop_item, and no action reopens a done or dropped item
gate('release_item', 'queued-drop', dict(item=i3, reason='x'), 'item_not_in_work')
tick(); gate('drop_item', 'i3', dict(item=i3, reason='not wanted'))
f = fields(i3); assert f['state'] == 'dropped' and f['drop_reason'] == 'not wanted' and f['dropped_at'] == stamp(), f
gate('drop_item', 'i6-in-work', dict(item=i6, reason='not wanted either')); assert fields(i6)['state'] == 'dropped'
r_spare = review9('spare', 'a8' * 20)
for label, closed_item, closing in [('done', i1, dict(review=r_spare)), ('dropped', i3, dict(evidence='x'))]:
    gate('drop_item', 'closed-' + label, dict(item=closed_item, reason='x'), 'item_done_or_dropped')
    gate('start_item', 'closed-' + label, dict(item=closed_item, repository=repo8, branch='unit/again'), 'item_not_queued')
    gate('release_item', 'closed-' + label, dict(item=closed_item, reason='x'), 'item_not_in_work')
    gate('close_item', 'closed-' + label, dict(item=closed_item, **closing), 'item_not_in_work')
    gate('amend_item', 'closed-' + label, dict(item=closed_item, title='x'), 'item_closed')
gate('add_item', 'key-of-a-dropped-item', dict(item_params, key='k-drop'), 'key_used')
print('v9_item_rows_complete')
# triage_decision
t_item, t_intent, t_na, t_none = row('t-item'), row('t-intent'), row('t-not-adopted'), row('t-none')
tick()
td = dict(decision=t_item, triage='item', item=i4)
gate('triage_decision', 'lead-row', dict(decision=lead_row, triage='none', reason='a limit'), 'not_a_user_row')
gate('triage_decision', 'superseded-row', dict(decision=u_old, triage='none', reason='a limit'), 'not_a_user_row')
gate('triage_decision', 'item-missing', dict(decision=t_item, triage='item'), 'item_required')
gate('triage_decision', 'item-of-another-task', dict(td, item=other_item), 'item_required')
gate('triage_decision', 'intent-missing', dict(decision=t_intent, triage='intent'), 'objective_required')
gate('triage_decision', 'intent-other-task', dict(decision=t_intent, triage='intent', objective=o_other), 'objective_required')
gate('triage_decision', 'not-adopted-no-reason', dict(decision=t_na, triage='not_adopted'), 'reason_required')
gate('triage_decision', 'none-no-reason', dict(decision=t_none, triage='none'), 'reason_required')
for label, params in [('item-with-none', dict(decision=t_none, triage='none', reason='r', item=i4)), ('item-with-intent', dict(decision=t_intent, triage='intent', objective=o3, item=i4)),
                      ('item-with-not-adopted', dict(decision=t_na, triage='not_adopted', reason='r', item=i4)), ('objective-with-item', dict(td, objective=o3)),
                      ('objective-with-none', dict(decision=t_none, triage='none', reason='r', objective=o3)), ('objective-with-not-adopted', dict(decision=t_na, triage='not_adopted', reason='r', objective=o3))]:
    gate('triage_decision', label, params, 'reference_not_for_triage')
gate('triage_decision', 'item', td)
f = fields(t_item); assert f['triage'] == 'item' and f['triage_item'] == i4 and f['triaged_at'] == stamp() and 'triage_objective' not in f, f
gate('triage_decision', 'intent', dict(decision=t_intent, triage='intent', objective=o3)); assert fields(t_intent)['triage_objective'] == o3
gate('triage_decision', 'not-adopted', dict(decision=t_na, triage='not_adopted', reason='a design that was refused')); assert fields(t_na)['triage_reason'] == 'a design that was refused'
gate('triage_decision', 'none', dict(decision=t_none, triage='none', reason='a question, answered')); assert fields(t_none)['triage'] == 'none'
gate('triage_decision', 'again', dict(decision=t_item, triage='none', reason='again'), 'already_triaged')
gate('triage_decision', 'again-same', td, 'already_triaged')
assert 'triage' not in fields(d) and 'triage' not in fields(successor)
# A row of the user recorded under version 2 has no words and is triaged like any other current row of the user.
gate('triage_decision', 'row-without-words', dict(decision=early_user, triage='none', reason='an approval, before the board'))
f = fields(early_user); assert f['triage'] == 'none' and f['triage_reason'] == 'an approval, before the board' and 'user_words' not in f, f
# The empty string is no value: each string parameter of the fourteen actions but `line` of record_check refuses "" by its type.
i7 = add('k-empty-strings'); t_empty = row('t-empty'); tick()
gate('set_order', 'with-i7', dict(so, order=[i2, i1, i5, i6, i3, other_item2, i7]))
gate('start_item', 'i7', dict(item=i7)); assert fields(i7)['state'] == 'in_work' and 'repository' not in fields(i7)
empty_rows = [('state_objective', dict(op, name='empty-parameters', source=u_src), ['name', 'statement', 'check']),
              ('restate_objective', dict(objective=o3, statement='s', source=u_re), ['statement']),
              ('amend_objective', dict(objective=o3, check='true', decision=lead_row), ['check']),
              ('retire_objective', dict(objective=o3, reason='r'), ['reason']),
              ('add_item', dict(item_params, key='k-empty-parameters'), ['key', 'title', 'statement', 'first_use']),
              ('amend_item', dict(item=i4, title='t', statement='s', first_use='u'), ['title', 'statement', 'first_use']),
              ('start_item', dict(item=i4, repository=repo8, branch='unit/empty'), ['repository', 'branch']),
              ('release_item', dict(item=i7, reason='r'), ['reason']),
              ('close_item', dict(item=i7, evidence='e'), ['evidence']),
              ('drop_item', dict(item=i7, reason='r'), ['reason']),
              ('set_order', dict(so, order=[i7]), ['reason']),
              ('triage_decision', dict(decision=t_empty, triage='none', reason='r'), ['reason'])]
for action, base, names in empty_rows:
    for name in names: gate(action, 'empty-' + name, dict(base, **{name: ''}), 'invalid_parameter')
gate('triage_decision', 'empty-reason-not-adopted', dict(decision=t_empty, triage='not_adopted', reason=''), 'invalid_parameter')
assert 'check' not in fields(o3) and 'triage' not in fields(t_empty) and fields(i7)['state'] == 'in_work' and 'evidence' not in fields(i7)
# A value that applied before still applies: spaces, a line break and bytes outside ASCII; and a check that prints nothing.
o4 = created(gate('state_objective', 'o4', dict(op, name='with-check', rank=5)), 'objective')
gate('record_check', 'empty-line', dict(objective=o4, exit=0, line=''))
assert fields(o4)['last_exit'] == 0 and fields(o4)['last_line'] == ''
gate('close_item', 'multiline-evidence', dict(item=i7, evidence='ran it\nthen: ok \u00e9'))
assert fields(i7)['state'] == 'done' and fields(i7)['evidence'] == 'ran it\nthen: ok \u00e9'
gate('add_item', 'multiline-title', dict(item_params, key='k-multiline', title='two words\nand \u00e9'))
print('v9_gate_rows_complete')
PY

python3 -I -B "$repo/ontology/memory-index-walk.py" "$ont_binary" "$rehearsal_root/memory-walk" --records 2

# Version 10 on a second store: the user's rows of version 9 carried by ontology/migrate, then the walk of the
# requests, intents and criteria, on version 10 pinned in Git. Then version 11, pinned in Git, over the objects
# made under version 10, and the walk of the units, runs and findings. Then version 12, pinned in Git, over the
# objects made under version 11, and the walk of the page digest. Then version 13, pinned in Git, over the objects
# made under version 12, and the walk of the principles. Then version 14, pinned in Git, over the objects made under
# version 13, and the walk of the two record sources of the routes. Then version 15, pinned in Git, over the objects
# made under version 14: the function horizon. Then version 16, pinned in Git, and its walk of the clauses; then
# version 17, the repository's file, and its walk of the hosts, accounts, samples and checks.
mkdir -- "$rehearsal_root/v10"
python3 - "$ont_binary" "$repo" "$rehearsal_root/v10/store.sqlite" "$work_root/" "$rehearsal_root/copies" "$rehearsal_root/lib" <<'PY'
import datetime, hashlib, json, os, pathlib, subprocess, sys
binary, repo, store, work, copies, lib = sys.argv[1:]
sys.path.insert(0, lib)
import redirect
def redirected(path, label=None): return redirect.make(path, work, copies, label)[0]
assert os.environ['ONT_BIN'] == binary
now = int(datetime.datetime(2026, 11, 12, tzinfo=datetime.timezone.utc).timestamp())
env = {k: v for k, v in os.environ.items() if k not in ('ONT_AGENT', 'ONT_ROLE')}
def call(args, request=None, code=0, client=None, role=None):
    p = subprocess.run([binary, '--store', store, '--now', str(now), *(['--client', client] if client else []), *args], input=json.dumps(request) if request is not None else None,
                       text=True, capture_output=True, timeout=60, env=dict(env, ONT_ROLE=role) if role else env)
    assert p.returncode == code, (args, request, p.returncode, p.stdout, p.stderr)
    data = json.loads(p.stdout); assert data['status'] == ('refused' if code else 'ok'), data
    return data.get('result', data)
def oid(kind, key): return dict(object_type=kind, namespace=dict(instance='vps://palantirkc', account='root'), native_key=key)
def obj(i): return call(['get'], dict(id=i))['objects'][0]
def fields(i): return {k: v.get('value') for k, v in obj(i)['observations'][-1]['fields'].items()}
def created(r, kind): return next(e['id'] for e in r['edits'] if e['id']['object_type'] == kind and e['change'] == 'initial')
def stamp(at=None): return datetime.datetime.fromtimestamp(now if at is None else at, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
def tick():
    global now
    now += 1
def fingerprint(): return [(p.name, hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None) for p in [pathlib.Path(store), pathlib.Path(store).with_name('edits.jsonl')]]
phase = 'v10'
def apply(action, rid, parameters, failure=None, role=None):
    before = fingerprint()
    r = call(['action', 'apply'], dict(action_type=action, request_id=phase + '-' + action + '-' + rid, parameters=parameters), 2 if failure else 0, role=role)
    if failure:
        expected = {failure} if isinstance(failure, str) else set(failure)
        actual = set(r['failed']) if r['code'] == 'submission_criteria' else {r['code']}
        assert actual == expected, (action, rid, expected, r)
        assert fingerprint() == before
    else: assert r['outcome'] == 'applied', r
    print('gate_row=' + phase + '-' + action + '-' + rid + ' expected=' + (','.join(sorted(expected)) if failure else 'admitted'))
    return r
def install(commit, digest):
    pinned = subprocess.check_output(['git', '-C', repo, 'show', commit + ':ontology/work.schema.json'], timeout=10)
    assert hashlib.sha256(pinned).hexdigest() == digest
    path = pathlib.Path(store).with_name('pinned-' + commit + '.json'); path.write_bytes(pinned)
    call(['schema', 'install', str(redirected(path))])
    return json.loads(pinned)
# The store starts at version 2 with a row of the user without words, and is carried through the pinned versions to 9.
install('2b9417c8bc96b0cbeb4ab7913aa44cb49ad8d604', '27de5fd02c20299897720ab34543e04e6db685438c3dd7409691cb8ddae11f24')
call(['index', '--source', 'task_states'])
task = oid('task', 'ontology-operational-base')
no_words = created(apply('record_decision', 'v2-without-words', dict(task=task, topic='v10-approval', text='an approval', decided_by='user', undo='supersede')), 'decision')
for commit, digest in [('f46cdf49ba74e117ddaaf387416a0eac1d53f2af', 'f1573b514c1bb2591e525b502dfb1c37e22cea0f4588c67281afdc41df10adc5'),
                       ('8ad36bf3cf10e19659965cc2d458a921533cdfae', '8467bd2984f77e5d81fe4efeca5d583a98cf754e100f7829aac2645a146bb414'),
                       ('ccb15142dbe19f2e04ad1c5d445048336db60d08', '6f6d2acc4a45ccc7e2f25268d458f9b6fabc5ab10fb85231d0fa81eaf68373f4'),
                       ('64fb1f59c6ff426807886d5a10397f5be3e496fa', '8b4b4236d9e5cff8ce3286211682cc977f4cdf1fdbfb36b42aefa08021e04d23'),
                       ('8379ae651929853d6fb2df79559ece4a98c5f551', 'faba52d93401384cdc6146a7f505c61dafc4a87662deb284dfc689f19a58d298')]:
    install(commit, digest)
call(['index', '--source', 'task_states'])
def row(rid, topic, words=None, who='user'):
    tick()
    extra = dict(user_words=words, user_form='decision') if words else {}
    return created(apply('record_decision', rid, dict(task=task, topic=topic, text='v10 ' + topic, decided_by=who, undo='supersede', **extra)), 'decision')
with_words = row('v9-words', 'v10-widget-colour', '"Make the invented widgets green."')
with_triage = row('v9-triaged', 'v10-widget-size', '"Keep the invented widgets small."')
apply('triage_decision', 'v9-triaged', dict(decision=with_triage, triage='none', reason='a preference, kept'))
lead_row = row('v9-lead', 'v10-lead-order', who='lead')
decisions = [no_words, with_words, with_triage, lead_row]
v9_before = [obj(i) for i in [task, *decisions]]
# Version 10 is pinned in Git by the commit of main that holds it; the repository's file comes after its walk.
install('e8e0286d81a5020aed14d9232156cc8c1de0a5e4', 'd10e4f5dec9aadcf3dbc9d4be63655196743c09e17adf4e3b3041d3d847af388')
assert [obj(i) for i in [task, *decisions]] == v9_before
print('gate_row=v10-v9-rows-unchanged expected=admitted objects=' + str(len(v9_before)))
# The migration, run as itself: delivery skips a file that is not executable.
assert subprocess.check_output(['git', '-C', repo, 'ls-files', '-s', 'ontology/migrate'], text=True, timeout=10).startswith('100755')
def migrate(before, after, code=0, extra=None):
    p = subprocess.run([str(pathlib.Path(repo, 'ontology/migrate')), store, str(before), str(after)], text=True, capture_output=True, timeout=600, env=dict(env, **(extra or {})))
    assert p.returncode == code, (before, after, p.returncode, p.stdout, p.stderr)
    print('migrate ' + str(before) + ' ' + str(after) + ' -> ' + p.stdout.strip())
    return json.loads(p.stdout)
def carried(d): return [o['id'] for o in call(['search'], dict(object_type='request', where={'eq': ['legacy_decision', d]}))['objects']]
decided = [obj(i) for i in decisions]
assert migrate(9, 10) == dict(carried=2, already=0)
for d, reason in [(with_words, 'carried from version 9'), (with_triage, 'carried from version 9, triage none')]:
    [request] = carried(d); f, df = fields(request), fields(d)
    assert f['words'] == df['user_words'] and f['words_verified'] is False and f['reading'] == df['text'] and f['kind'] == 'decision' and f['topic'] == df['topic'], f
    assert f['said_at'] == df['decided_at'] and f['status'] == 'current' and f['route'] == 'aside' and f['route_reason'] == reason and f.get('intent') is None and f['legacy_decision'] == d, f
assert carried(no_words) == [] and carried(lead_row) == [] and [obj(i) for i in decisions] == decided
unchanged = fingerprint()
assert migrate(9, 10, extra=dict(ONT_AGENT='another-agent', ONT_ROLE='lead')) == dict(carried=0, already=2)
assert fingerprint() == unchanged
# A record that an interrupted run left without its aside: the run sets it aside.
third = row('v10-old-verb', 'v10-widget-name', '"Call it the invented widget board."'); tf = fields(third)
call(['action', 'apply'], dict(action_type='record_request', request_id='migrate-10-' + third['native_key'], parameters=dict(
    words=tf['user_words'], words_verified=False, reading=tf['text'], kind='decision', topic=tf['topic'], said_at=tf['decided_at'], legacy_decision=third)), client='migrate')
[third_request] = carried(third); assert fields(third_request)['route'] == 'unrouted'
assert migrate(9, 10) == dict(carried=1, already=2)
assert fields(third_request)['route'] == 'aside' and fields(third_request)['route_reason'] == 'carried from version 9'
# A carried row triaged with the old verb between two runs: the second run's aside would carry another reason
# under the same request id (request_id_reused); its record answered already_applied, so the row is already done.
apply('triage_decision', 'v9-after-carry', dict(decision=third, triage='none', reason='a name, kept'))
unchanged = fingerprint()
assert migrate(9, 10) == dict(carried=0, already=3)
assert fingerprint() == unchanged and fields(third_request)['route_reason'] == 'carried from version 9'
for before, after in [(9, 19), (10, 10), (10, 9)]:
    assert migrate(before, after, 2) == dict(status='unknown_pair')
assert fingerprint() == unchanged
assert migrate(9, 10) == dict(carried=0, already=3)
assert [obj(i) for i in decisions] == decided
print('v10_migration_rows_complete')
# The walk. Each refusal row is before the action it guards; a refusal writes nothing.
def request(rid, kind, said=None, topic='v10-walk'):
    return created(apply('record_request', rid, dict(words='Invented words of ' + rid + '.', words_verified=True, record='fixture/session.jsonl', reading='The user says ' + rid + '.',
                                                     kind=kind, topic=topic, said_at=said or stamp(), session='fixture-session', runtime='fixture-cli')), 'request')
tick()
rp = dict(words='Build the invented widget board.', words_verified=True, reading='The user orders a widget board.', kind='order', topic='v10-widget-board', said_at=stamp())
apply('record_request', 'words-empty', dict(rp, words=''), 'words_empty')
apply('record_request', 'future', dict(rp, said_at=stamp(now + 1)), 'said_in_the_future')
apply('record_request', 'legacy-lead-row', dict(rp, legacy_decision=lead_row), 'legacy_not_a_user_row')
apply('record_request', 'legacy-absent', dict(rp, legacy_decision=oid('decision', 'absent-v10-decision')), 'legacy_not_a_user_row')
apply('record_request', 'legacy-carried', dict(rp, legacy_decision=with_words), 'legacy_already_carried')
r0 = created(apply('record_request', 'r0', rp), 'request')
f = fields(r0); assert f['status'] == 'current' and f['route'] == 'unrouted' and f['recorded_at'] == stamp() and f['words'] == rp['words'] and f.get('intent') is None, f
unrouted = call(['render'], dict(view='requests_unrouted', args={})); assert unrouted['rows'] == 1 and rp['reading'] in unrouted['markdown'], unrouted
intent = oid('intent', 'widget-board')
dp = dict(intent=intent, outcome='The invented widget board lists every widget.', first_user='The fixture reader, in its first run.', origin=r0,
          questions=[dict(question='Which colour?', default='green', answer='')], directory='fixture/widget-board')
apply('draft_intent', 'origin-absent', dict(dp, origin=oid('request', 'absent-v10-request')), 'origin_not_current')
apply('draft_intent', 'outcome-empty', dict(dp, outcome=''), 'outcome_empty')
apply('draft_intent', 'first-user-empty', dict(dp, first_user=''), 'first_user_empty')
apply('draft_intent', 'draft', dp)
f = fields(intent); assert f['status'] == 'draft' and f['amended_at'] == stamp() and f['origin'] == r0 and f['questions'] == dp['questions'] and f.get('confirmed_by') is None, f
f = fields(r0); assert f['route'] == 'intent' and f['intent'] == intent and f['routed_at'] == stamp(), f
apply('draft_intent', 'same-key', dict(dp, outcome='Another outcome.'), 'revision_conflict')
tick(); r1 = request('r1', 'answer')
apply('route_request', 'r1', dict(request=r1, intent=intent)); assert fields(r1)['intent'] == intent
apply('confirm_intent', 'no-criterion', dict(intent=intent, by=r1), 'no_criterion')
cp = dict(intent=intent, statement='The board lists 3 widgets.', check='fixture-check --count 3', baseline='0 widgets, fixture, 2026-11-12')
apply('add_criterion', 'statement-empty', dict(cp, statement=''), 'statement_empty')
apply('add_criterion', 'check-empty', dict(cp, check=''), 'check_empty')
c1 = created(apply('add_criterion', 'c1', cp), 'criterion')
f = fields(c1); assert f['state'] == 'open' and f['intent'] == intent and f['added_at'] == stamp() and f['baseline'] == cp['baseline'], f
c2 = created(apply('add_criterion', 'c2', dict(intent=intent, statement='The board is green.', check='look at it')), 'criterion')
tick(); r2 = request('r2', 'decision')
apply('confirm_intent', 'unrouted', dict(intent=intent, by=r2), 'by_not_routed_here')
preference = request('preference', 'preference'); apply('route_request', 'preference', dict(request=preference, intent=intent))
apply('confirm_intent', 'preference', dict(intent=intent, by=preference), 'by_not_the_users_word')
tick(); apply('amend_intent', 'why', dict(intent=intent, why='The fixture needs a board.'))
apply('confirm_intent', 'said-before-amendment', dict(intent=intent, by=r1), 'by_older_than_amendment')
tick(); r3 = request('r3', 'decision'); apply('route_request', 'r3', dict(request=r3, intent=intent))
apply('confirm_intent', 'r3', dict(intent=intent, by=r3))
f = fields(intent); assert f['status'] == 'confirmed' and f['confirmed_by'] == r3 and f['confirmed_at'] == stamp() and f['confirmed_delegated'] is False and f['why'] == 'The fixture needs a board.', f
apply('confirm_intent', 'confirmed', dict(intent=intent, by=r3), 'intent_not_open')
apply('route_request', 'confirming', dict(request=r3, intent=intent), 'request_confirms_an_intent')
apply('set_request_aside', 'confirming', dict(request=r3, reason='a remark'), 'request_confirms_an_intent')
apply('supersede_request', 'confirming', dict(rp, request=r3), 'request_confirms_an_intent')
apply('draft_intent', 'origin-confirms', dict(dp, intent=oid('intent', 'widget-other'), origin=r3), 'origin_confirms_an_intent')
apply('amend_intent', 'outcome-empty', dict(intent=intent, outcome=''), 'outcome_empty')
apply('amend_intent', 'first-user-empty', dict(intent=intent, first_user=''), 'first_user_empty')
tick(); apply('amend_intent', 'outcome', dict(intent=intent, outcome='The invented widget board lists every widget, in order.'))
f = fields(intent)
assert f['status'] == 'draft' and f.get('confirmed_by') is None and f['confirmed_delegated'] is False and f['amended_at'] == stamp() and f['first_user'] == dp['first_user'] and f['why'] == 'The fixture needs a board.' and f['questions'] == dp['questions'], f
apply('route_request', 'after-amendment', dict(request=r3, intent=intent))
apply('name_lead', 'unrouted', dict(intent=intent, session='fixture-lead', by=r2), 'by_not_routed_here')
apply('name_lead', 'preference', dict(intent=intent, session='fixture-lead', by=preference), 'by_not_the_users_word')
apply('name_lead', 'r0', dict(intent=intent, session='fixture-lead', by=r0)); f = fields(intent); assert f['lead_session'] == 'fixture-lead' and f['lead_named_by'] == r0, f
apply('note_intent', 'notes', dict(intent=intent, notes='Next: run the count.')); assert fields(intent)['notes'] == 'Next: run the count.'
apply('waive_criterion', 'unrouted', dict(criterion=c2, by=r2, reason='not needed'), 'by_not_routed_here')
apply('waive_criterion', 'preference', dict(criterion=c2, by=preference, reason='not needed'), 'by_not_the_users_word')
apply('waive_criterion', 'reason-empty', dict(criterion=c2, by=r3, reason=''), 'reason_empty')
apply('waive_criterion', 'c2', dict(criterion=c2, by=r3, reason='The user does not need it.'))
f = fields(c2); assert f['state'] == 'waived' and f['waived_by'] == r3 and f['evidence'] == 'The user does not need it.', f
apply('check_criterion', 'waived', dict(criterion=c2, state='pass', evidence='seen'), 'criterion_waived')
apply('close_intent', 'draft', dict(intent=intent, reason='fixture-check passed'), ('intent_not_confirmed', 'criterion_not_passed'))
tick(); r4 = request('r4', 'answer'); apply('route_request', 'r4', dict(request=r4, intent=intent)); apply('confirm_intent', 'r4', dict(intent=intent, by=r4))
apply('close_intent', 'open-criterion', dict(intent=intent, reason='fixture-check passed'), 'criterion_not_passed')
for state in ['open', 'waived']: apply('check_criterion', 'state-' + state, dict(criterion=c1, state=state, evidence='seen'), 'state_not_a_result')
apply('check_criterion', 'evidence-empty', dict(criterion=c1, state='pass', evidence=''), 'evidence_empty')
apply('check_criterion', 'fail', dict(criterion=c1, state='fail', evidence='fixture-check: 2 widgets'))
apply('close_intent', 'failed-criterion', dict(intent=intent, reason='fixture-check passed'), 'criterion_not_passed')
tick(); apply('check_criterion', 'pass', dict(criterion=c1, state='pass', evidence='fixture-check: 3 widgets'))
f = fields(c1); assert f['state'] == 'pass' and f['evidence'] == 'fixture-check: 3 widgets' and f['checked_at'] == stamp(), f
apply('close_intent', 'met', dict(intent=intent, reason='fixture-check passed'))
f = fields(intent); assert f['status'] == 'met' and f['closed_at'] == stamp() and f['closed_reason'] == 'fixture-check passed', f
for action, params in [('amend_intent', dict(intent=intent, why='later')), ('note_intent', dict(intent=intent, notes='later')), ('name_lead', dict(intent=intent, session='fixture-lead', by=r0)),
                       ('add_criterion', cp), ('retire_intent', dict(intent=intent, reason='later'))]:
    apply(action, 'met', params, 'intent_closed')
apply('route_request', 'met', dict(request=r2, intent=intent), 'intent_closed')
apply('route_request', 'unknown-intent', dict(request=r2, intent=oid('intent', 'absent-v10-intent')), 'unknown_intent')
apply('close_intent', 'closed', dict(intent=intent, reason='again'), 'intent_not_confirmed')
apply('confirm_intent', 'met', dict(intent=intent, by=r4), 'intent_not_open')
tick(); retired = oid('intent', 'widget-retire'); apply('draft_intent', 'retire', dict(dp, intent=retired, origin=request('o-retire', 'order')))
apply('retire_intent', 'reason-empty', dict(intent=retired, reason=''), 'reason_empty')
apply('retire_intent', 'retired', dict(intent=retired, reason='replaced by widget-board'))
f = fields(retired); assert f['status'] == 'retired' and f['closed_reason'] == 'replaced by widget-board' and f['closed_at'] == stamp(), f
apply('retire_intent', 'again', dict(intent=retired, reason='again'), 'intent_closed')
tick(); sp = dict(rp, request=r2, words='Corrected invented words.', reading='The user corrects r2.', kind='correction', said_at=stamp())
apply('supersede_request', 'words-empty', dict(sp, words=''), 'words_empty')
apply('supersede_request', 'future', dict(sp, said_at=stamp(now + 1)), 'said_in_the_future')
words2 = fields(r2)['words']
successor = created(apply('supersede_request', 'r2', sp), 'request')
f = fields(r2); assert f['status'] == 'superseded' and f['words'] == words2, f
f = fields(successor); assert f['status'] == 'current' and f['route'] == 'unrouted' and f['supersedes'] == r2 and f['words'] == sp['words'] and f['kind'] == 'correction' and f['said_at'] == stamp(), f
apply('supersede_request', 'again', sp, 'request_not_current')
apply('route_request', 'superseded', dict(request=r2, intent=oid('intent', 'absent-v10-intent')), ('request_not_current', 'unknown_intent'))
apply('set_request_aside', 'superseded', dict(request=r2, reason='a remark'), 'request_not_current')
apply('set_request_aside', 'reason-empty', dict(request=r1, reason=''), 'reason_empty')
tick(); apply('set_request_aside', 'r1', dict(request=r1, reason='a question answered at once'))
f = fields(r1); assert f['route'] == 'aside' and f['route_reason'] == 'a question answered at once' and f.get('intent') is None and f['routed_at'] == stamp() and f['words'] == 'Invented words of r1.', f
print('v10_walk_rows_complete')
# The role of the actor: the engine of ontology/ENGINE-PIN knows it and ships the verb log.
tick(); assumed = oid('intent', 'widget-assume'); apply('draft_intent', 'assume', dict(dp, intent=assumed, origin=request('o-assume', 'order')))
apply('add_criterion', 'assume', dict(cp, intent=assumed))
apply('assume_intent', 'no-role', dict(intent=assumed, reason='the user is away'), 'not_the_lead')
apply('assume_intent', 'reason-empty', dict(intent=assumed, reason=''), 'reason_empty', role='lead')
apply('assume_intent', 'lead', dict(intent=assumed, reason='the user is away'), role='lead')
f = fields(assumed); assert f['status'] == 'assumed' and f['assumed_reason'] == 'the user is away', f
apply('assume_intent', 'again', dict(intent=assumed, reason='again'), 'intent_not_draft', role='lead')
tick(); order, delegation = request('not-a-delegation', 'order'), request('delegation', 'delegation')
apply('set_request_aside', 'delegation', dict(request=delegation, reason='it covers the widget intents'))
covered = oid('intent', 'widget-delegated'); apply('draft_intent', 'delegated', dict(dp, intent=covered, origin=request('o-delegated', 'order')))
dlp = dict(intent=assumed, delegation=delegation, reason='the delegation covers widget intents')
apply('confirm_intent_delegated', 'order', dict(dlp, delegation=order), 'not_a_delegation', role='lead')
apply('confirm_intent_delegated', 'no-role', dlp, 'not_the_lead')
apply('confirm_intent_delegated', 'reason-empty', dict(dlp, reason=''), 'reason_empty', role='lead')
apply('confirm_intent_delegated', 'no-criterion', dict(dlp, intent=covered), 'no_criterion', role='lead')
apply('confirm_intent_delegated', 'assumed', dlp, role='lead')
f = fields(assumed); assert f['status'] == 'confirmed' and f['confirmed_by'] == delegation and f['confirmed_delegated'] is True and f['confirmed_at'] == stamp(), f
apply('confirm_intent_delegated', 'confirmed', dlp, 'intent_not_open', role='lead')
apply('route_request', 'delegation-used', dict(request=delegation, intent=covered), 'request_confirms_an_intent')
apply('add_criterion', 'delegated', dict(cp, intent=covered))
early_stop = request('early-stop', 'stop', said=stamp(now - 10))
tick(); stop, later_preference = request('stop', 'stop'), request('later-preference', 'preference')
apply('end_delegation', 'not-a-delegation', dict(request=order, stopped_by=stop), 'not_a_delegation')
apply('end_delegation', 'preference', dict(request=delegation, stopped_by=later_preference), 'by_not_the_users_word')
apply('end_delegation', 'earlier', dict(request=delegation, stopped_by=early_stop), 'by_not_later')
ended = apply('end_delegation', 'ended', dict(request=delegation, stopped_by=stop))
# The action changed the delegation and nothing else: the user's later word stays current.
assert [(e['id'], e['change']) for e in ended['edits']] == [(delegation, 'update')], ended
assert fields(delegation)['status'] == 'superseded' and fields(stop)['status'] == 'current' and fields(assumed)['confirmed_by'] == delegation
apply('confirm_intent_delegated', 'ended', dict(dlp, intent=covered), 'not_a_delegation', role='lead')
apply('end_delegation', 'again', dict(request=delegation, stopped_by=stop), 'not_a_delegation')
refused = call(['log'], dict(outcome='refused', action_type='confirm_intent', parameters=True))['rows']
assert any(r['code'] == 'submission_criteria' and r.get('failed') == ['by_not_routed_here'] and r['parameters']['by'] == r2 for r in refused), refused
led = call(['log'], dict(action_type='assume_intent', actor=dict(role='lead')))['rows']
assert len(led) == 1 and led[0]['actor']['role'] == 'lead', led
print('v10_role_rows_complete')
# The four views.
open_keys = sorted(o['id']['native_key'] for o in call(['search'], dict(object_type='intent', where={'in': ['status', ['draft', 'assumed', 'confirmed']]}))['objects'])
view = call(['render'], dict(view='intents_open', args={}))
assert view['rows'] == len(open_keys) > 0 and all('| ' + k + ' |' in view['markdown'] for k in open_keys) and '| widget-board |' not in view['markdown'], view
view = call(['render'], dict(view='criteria_of_intent', args=dict(intent='widget-board')))
assert view['rows'] == 2 and '| pass |' in view['markdown'] and '| waived |' in view['markdown'], view
view = call(['render'], dict(view='requests_by_topic', args=dict(topic='v10-widget-colour')))
assert view['rows'] == 1 and '| aside |' in view['markdown'], view
unrouted = [o['id'] for o in call(['search'], dict(object_type='request', where={'all': [{'eq': ['status', 'current']}, {'eq': ['route', 'unrouted']}]}))['objects']]
view = call(['render'], dict(view='requests_unrouted', args={}))
assert view['rows'] == len(unrouted) > 0 and 'v10 v10-widget-colour' not in view['markdown'] and 'The user corrects r2.' in view['markdown'], view
print(view['markdown'])
print('v10_gate_rows_complete')
# Version 11, pinned in Git by the commit of main that holds it, over the objects made under version 10; the
# repository's file comes after its walk.
v10_objects = [task, *decisions, third, *[o['id'] for t in ('request', 'intent', 'criterion') for o in call(['search'], dict(object_type=t, limit=1000))['objects']]]
v10_before = [obj(i) for i in v10_objects]
v10_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args in [('intents_open', {}), ('criteria_of_intent', dict(intent='widget-board')),
             ('requests_by_topic', dict(topic='v10-widget-colour')), ('requests_unrouted', {})]]
V11_PIN = ('eb4963d5fa0ff697a9f6743df38a03bbd20b233d', '62062852a76a173dc7a51625b1ccfdd806432d086a7350b9b3d2a12b6aeb1d34')
v11_schema = install(*V11_PIN)
assert v11_schema['version'] == 11
redirect.trace(work, copies)
assert [obj(i) for i in v10_objects] == v10_before
for view, args, before in v10_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@10', 'schema=work@11'), view
print('gate_row=v11-v10-objects-and-views-unchanged expected=admitted objects=' + str(len(v10_objects)) + ' views=' + str(len(v10_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v11_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
unchanged = fingerprint()
assert migrate(10, 11) == dict(carried=0, already=0)
assert migrate(9, 11) == dict(carried=0, already=3)
assert fingerprint() == unchanged and [obj(i) for i in v10_objects] == v10_before
print('v11_migration_rows_complete')
phase = 'v11'
# Version 10's actions on objects made under version 10.
tick(); covered_word = request('covered-word', 'decision'); apply('route_request', 'covered', dict(request=covered_word, intent=covered))
apply('confirm_intent', 'covered', dict(intent=covered, by=covered_word)); assert fields(covered)['status'] == 'confirmed'
apply('route_request', 'v10-confirming', dict(request=r4, intent=covered), 'request_confirms_an_intent')
[covered_criterion] = [o['id'] for o in call(['search'], dict(object_type='criterion', where={'eq': ['intent', covered]}))['objects']]
apply('close_intent', 'covered-open', dict(intent=covered, reason='seen'), 'criterion_not_passed')
apply('check_criterion', 'covered', dict(criterion=covered_criterion, state='pass', evidence='fixture-check: 3 widgets'))
apply('close_intent', 'covered', dict(intent=covered, reason='fixture-check passed')); assert fields(covered)['status'] == 'met'
print('v11_v10_actions_complete')

# The walk of units. Each refusal row is before the action it guards; a refusal writes nothing.
def digest(text): return hashlib.sha256(text.encode()).hexdigest()
H1, H2, H3, MERGED = '1' * 40, '2' * 40, '3' * 40, 'e' * 40
PROMPT = dict(prompt='/home/palantirkc/work/fixture/PROMPT.md', prompt_sha256='b' * 64)
REPORT = dict(report='/home/palantirkc/work/fixture/REPORT.md', report_sha256='c' * 64)
REPO = 'fixture-owner/widgets'
page = dict(objective='The invented widget board lists every widget.', acceptance=[dict(check='fixture-check --count 3', kind='fails_on_base')], write_set=['src/**'],
            implementer=dict(runtime='fixture-cli', model='fixture-model', effort='high'), verifier=dict(runtime='fixture-cli', model='fixture-model', effort='xhigh'), repository=REPO)
def unit(key): return oid('unit', key)
def review(rid, kind, head, verdict='accept', session='fixture-verifier', close=True, **subject):
    tick()  # a review counts only when it started later than the unit's last amendment
    started = created(apply('start_unit_review', rid, dict(dict(task=task, kind=kind, head=head, model='fixture-model', effort='xhigh', **PROMPT), **subject)), 'review')
    if close:
        given = dict(review=started, verdict=verdict, session=session, **REPORT)
        if 'unit' in subject: given['unit'] = subject['unit']
        apply('close_unit_review', rid, given)
    return started
def hold(rid, **on): return created(apply('set_hold_on', rid, dict(task=task, effect='fixture effect ' + rid, until='the fixture says so', scope='the fixture', **on)), 'hold')
def lift(rid, h): apply('lift_hold', rid, dict(hold=h, reason='the fixture said so'))
# The lane small.
tick(); origin = request('small-origin', 'order', topic='v11-small'); sp = dict(page, unit=unit('fixture-small'), lane='small', origin=origin, **{'class': 'plain'})
apply('add_unit', 'small-no-origin', {k: v for k, v in sp.items() if k != 'origin'}, 'origin_required')
apply('add_unit', 'small-superseded', dict(sp, origin=r2), 'origin_not_current')
apply('add_unit', 'small-contract', dict(sp, **{'class': 'contract'}), 'small_must_be_plain')
apply('add_unit', 'small-no-repository', dict(sp, repository=''), 'small_needs_one_repository_and_a_write_set')
apply('add_unit', 'small-no-write-set', dict(sp, write_set=[]), 'small_needs_one_repository_and_a_write_set')
apply('add_unit', 'small-objective-empty', dict(sp, objective=''), 'objective_empty')
apply('add_unit', 'small-itself', dict(sp, depends_on=[unit('fixture-small')]), 'depends_on_itself')
small = unit('fixture-small'); apply('add_unit', 'small', sp)
f = fields(small)
assert f['state'] == 'draft' and f['rounds'] == 0 and f['writers'] == [] and f['added_at'] == f['amended_at'] == stamp() and f['origin'] == origin and f['lane'] == 'small', f
apply('add_unit', 'small-same-key', dict(sp, origin=request('small-origin-2', 'order', topic='v11-small')), 'revision_conflict')
apply('add_unit', 'small-second', dict(sp, unit=unit('fixture-small-2')), 'request_has_a_live_unit')
apply('amend_unit', 'small-contract', dict(unit=small, **{'class': 'contract'}), 'small_must_be_plain')
for member, value in [('origin', origin), ('intent', intent), ('lane', 'objective')]:
    apply('amend_unit', 'small-' + member, dict(unit=small, **{member: value}), 'invalid_parameter')
apply('ready_unit', 'small', dict(unit=small)); apply('start_unit', 'small', dict(unit=small))
assert fields(small)['state'] == 'running' and fields(small)['started_at'] == stamp()
# The lane objective: an intent that is still a draft holds no unit.
tick(); gadget = oid('intent', 'gadget-board'); gadget_origin = request('gadget-origin', 'order', topic='v11-gadget')
apply('draft_intent', 'gadget', dict(dp, intent=gadget, origin=gadget_origin)); gadget_criterion = created(apply('add_criterion', 'gadget', dict(cp, intent=gadget)), 'criterion')
op = dict(page, lane='objective', intent=gadget, **{'class': 'plain'})
apply('add_unit', 'no-intent', {k: v for k, v in op.items() if k != 'intent'} | dict(unit=unit('fixture-dep')), 'intent_required')
apply('add_unit', 'draft-intent', dict(op, unit=unit('fixture-dep')), 'intent_not_open_for_units')
def confirm(rid, i):
    tick(); word = request(rid, 'decision', topic='v11-gadget'); apply('route_request', rid, dict(request=word, intent=i)); apply('confirm_intent', rid, dict(intent=i, by=word))
confirm('gadget-yes', gadget)
dep, contract = unit('fixture-dep'), unit('fixture-contract')
apply('add_unit', 'dep', dict(op, unit=dep))
apply('add_unit', 'contract', dict(op, unit=contract, depends_on=[dep], **{'class': 'contract'}))
apply('ready_unit', 'dep-unreviewed-plan', dict(unit=dep, intent=gadget), 'plan_not_reviewed')
apply('start_unit_review', 'plan-no-subject', dict(task=task, kind='plan', head=digest('the plan'), model='fixture-model', effort='xhigh', **PROMPT), 'subject_required')
apply('start_unit_review', 'plan-unknown-intent', dict(task=task, kind='plan', head=digest('the plan'), model='fixture-model', effort='xhigh', intent=oid('intent', 'absent-v11-intent'), **PROMPT), 'unknown_intent')
review('plan', 'plan', digest('the plan'), intent=gadget)
apply('ready_unit', 'contract-unreviewed-page', dict(unit=contract, intent=gadget), 'page_not_reviewed')
apply('start_unit_review', 'page-no-subject', dict(task=task, kind='page', head=digest('page 1'), model='fixture-model', effort='xhigh', **PROMPT), 'subject_required')
apply('start_unit_review', 'page-unknown-unit', dict(task=task, kind='page', head=digest('page 1'), model='fixture-model', effort='xhigh', unit=unit('absent-v11-unit'), **PROMPT), 'unknown_unit')
tick(); review('page-1', 'page', digest('page 1'), unit=contract)
apply('ready_unit', 'contract-no-intent', dict(unit=contract), 'intent_parameter_required')
apply('ready_unit', 'contract-other-intent', dict(unit=contract, intent=covered), ('intent_differs', 'intent_not_open_for_units', 'plan_not_reviewed'))
apply('ready_unit', 'contract', dict(unit=contract, intent=gadget)); assert fields(contract)['state'] == 'ready' and fields(contract)['ready_at'] == stamp()
apply('ready_unit', 'contract-again', dict(unit=contract, intent=gadget), 'unit_not_draft')
# An amendment returns the unit to draft; a page review counts only when it started after the last amendment.
tick(); apply('amend_unit', 'contract-objective-empty', dict(unit=contract, objective=''), 'objective_empty')
apply('amend_unit', 'contract-itself', dict(unit=contract, depends_on=[contract]), 'depends_on_itself')
# depends_on (an array of units) sorts before unit; the rule changes the unit named by unit, and only it.
r = apply('amend_unit', 'contract-1', dict(unit=contract, invariants='The board keeps its order.', depends_on=[dep]))
assert r['edits'] and all(e['id'] == contract for e in r['edits']) and fields(dep)['state'] == 'draft' and fields(contract)['depends_on'] == [dep], r
f = fields(contract); assert f['state'] == 'draft' and f['amended_at'] == stamp() and f['invariants'] == 'The board keeps its order.' and f['objective'] == page['objective'], f
apply('ready_unit', 'contract-page-older', dict(unit=contract, intent=gadget), 'page_not_reviewed')
tick(); early = review('page-2', 'page', digest('page 2'), unit=contract, close=False)
apply('start_unit_review', 'page-2-twice', dict(task=task, kind='page', head=digest('page 2'), model='fixture-model', effort='xhigh', unit=contract, **PROMPT), 'duplicate_running_review')
apply('start_unit_review', 'unknown-task', dict(task=oid('task', 'absent-v11-task'), kind='page', head=digest('page 2'), model='fixture-model', effort='xhigh', unit=contract, **PROMPT), 'unknown_task')
tick(); apply('amend_unit', 'contract-2', dict(unit=contract, notes='Read the board first.'))
tick(); apply('close_unit_review', 'page-2', dict(review=early, verdict='accept', session='fixture-verifier', unit=contract, **REPORT))
apply('ready_unit', 'contract-page-started-before', dict(unit=contract, intent=gadget), 'page_not_reviewed')
tick(); review('page-3', 'page', digest('page 3'), unit=contract)
apply('ready_unit', 'contract-2', dict(unit=contract, intent=gadget))
# start_unit: dependencies merged or done, no active hold.
apply('start_unit', 'contract-dep', dict(unit=contract, intent=gadget), 'dependency_not_merged')
apply('start_unit', 'dep-draft', dict(unit=dep, intent=gadget), 'unit_not_ready')
apply('ready_unit', 'dep', dict(unit=dep, intent=gadget)); apply('start_unit', 'dep', dict(unit=dep, intent=gadget))
apply('record_head', 'contract-not-running', dict(unit=contract, head=H1, writers=['fixture-implementer']), 'unit_not_running')
apply('record_head', 'dep', dict(unit=dep, head=H1, writers=['fixture-implementer'], pull_request=6, rounds=0))
apply('accept_unit', 'dep-unreviewed', dict(unit=dep, head=H1), 'head_not_reviewed')
review('dep', 'packet', H1, unit=dep, repository=REPO); apply('accept_unit', 'dep', dict(unit=dep, head=H1))
apply('merge_unit', 'dep-no-intent', dict(unit=dep, head=H1, merged_commit=MERGED), 'intent_parameter_required')
apply('merge_unit', 'dep-other-head', dict(unit=dep, head=H2, merged_commit=MERGED, intent=gadget), ('head_differs', 'head_not_reviewed'))
apply('merge_unit', 'dep', dict(unit=dep, head=H1, merged_commit=MERGED, intent=gadget))
f = fields(dep); assert f['state'] == 'merged' and f['merged_commit'] == MERGED and f['ended_at'] == stamp(), f
held_contract = hold('contract', unit=contract)
apply('set_hold_on', 'unknown-unit', dict(task=task, effect='fixture other', until='x', unit=unit('absent-v11-unit')), 'unknown_unit')
apply('set_hold_on', 'unknown-intent', dict(task=task, effect='fixture other', until='x', intent=oid('intent', 'absent-v11-intent')), 'unknown_intent')
apply('set_hold_on', 'unknown-task', dict(task=oid('task', 'absent-v11-task'), effect='fixture other', until='x', unit=contract), 'unknown_task')
apply('set_hold_on', 'duplicate', dict(task=task, effect='fixture effect contract', until='x', unit=contract), 'duplicate_active_hold')
apply('start_unit', 'contract-held', dict(unit=contract, intent=gadget), 'held'); lift('contract', held_contract)
held_gadget = hold('gadget', intent=gadget); apply('start_unit', 'contract-intent-held', dict(unit=contract, intent=gadget), 'held'); lift('gadget', held_gadget)
assert fields(held_contract)['unit'] == contract and fields(held_gadget)['intent'] == gadget and fields(held_gadget)['scope'] == 'the fixture'
apply('start_unit', 'contract', dict(unit=contract, intent=gadget)); assert fields(contract)['state'] == 'running'
apply('record_head', 'contract', dict(unit=contract, head=H1, writers=['fixture-implementer'], pull_request=7, rounds=0))
f = fields(contract); assert f['head'] == H1 and f['writers'] == ['fixture-implementer'] and f['pull_request'] == 7, f
# A review of a unit is closed by close_unit_review, and never by a session that wrote to the unit.
r1 = review('contract-h1', 'packet', H1, close=False, unit=contract, repository=REPO, pull_request=7, round=0)
apply('accept_unit', 'contract-running-review', dict(unit=contract, head=H1), 'head_not_reviewed')
apply('close_review', 'contract-h1-old-verb', dict(review=r1, verdict='accept', session='fixture-verifier', **REPORT), 'review_names_a_unit')
apply('close_unit_review', 'contract-h1-no-report', dict(review=r1, verdict='accept', session='fixture-verifier', unit=contract), 'report_required')
apply('close_unit_review', 'contract-h1-writer', dict(review=r1, verdict='accept', session='fixture-implementer', unit=contract, **REPORT), 'writer_cannot_review')
apply('close_unit_review', 'contract-h1-no-session', dict(review=r1, verdict='accept', unit=contract, **REPORT), 'session_required')
apply('close_unit_review', 'contract-h1-no-unit', dict(review=r1, verdict='accept', session='fixture-verifier', **REPORT), 'unit_differs')
apply('close_unit_review', 'contract-h1-other-unit', dict(review=r1, verdict='accept', session='fixture-verifier', unit=dep, **REPORT), 'unit_differs')
apply('close_unit_review', 'contract-h1', dict(review=r1, verdict='accept', session='fixture-verifier', unit=contract, **REPORT))
f = fields(r1); assert f['status'] == 'closed' and f['verdict'] == 'accept' and f['unit'] == contract and f['round'] == 0 and f.get('running_in') is None and f['closed_at'] == stamp(), f
apply('close_unit_review', 'contract-h1-again', dict(review=r1, verdict='accept', session='fixture-verifier', unit=contract, **REPORT), 'review_not_running')
apply('accept_unit', 'contract-other-head', dict(unit=contract, head=H2), ('head_differs', 'head_not_reviewed'))
apply('accept_unit', 'contract', dict(unit=contract, head=H1)); assert fields(contract)['state'] == 'accepted'
apply('merge_unit', 'contract-unapproved', dict(unit=contract, head=H1, merged_commit=MERGED, intent=gadget), 'not_approved')
# The come-back of an accepted unit: reopen, a new head, the carry of a review or a verifier's round, a second accept.
apply('reopen_unit', 'contract-cause-empty', dict(unit=contract, cause='', intent=gadget), 'cause_empty')
apply('reopen_unit', 'contract', dict(unit=contract, cause='main_moved', intent=gadget)); assert fields(contract)['state'] == 'running'
apply('reopen_unit', 'contract-again', dict(unit=contract, cause='main_moved', intent=gadget), 'unit_not_accepted')
apply('record_head', 'contract-h2', dict(unit=contract, head=H2, writers=['fixture-implementer', 'fixture-updater'], pull_request=7, rounds=1))
no_repository = review('contract-h1-no-repository', 'packet', H1, unit=contract)
apply('carry_review', 'no-repository', dict(review=no_repository, unit=contract, head=H2), 'review_incomplete')
running = review('contract-h2-running', 'packet', H2, close=False, unit=contract, repository=REPO)
apply('carry_review', 'running', dict(review=running, unit=contract, head=H3), ('review_not_accepting', 'review_incomplete'))
apply('close_unit_review', 'contract-h2-running', dict(review=running, verdict='abandoned', session='fixture-verifier', unit=contract))
apply('carry_review', 'other-unit', dict(review=r1, unit=dep, head=H2), 'unit_differs')
apply('carry_review', 'same-head', dict(review=r1, unit=contract, head=H1), 'head_unchanged')
apply('stop_unit', 'contract-cause-empty', dict(unit=contract, cause=''), 'cause_empty')
apply('stop_unit', 'contract', dict(unit=contract, cause='page_changes', detail='The Lead amends the page.'))
f = fields(contract); assert f['state'] == 'stopped' and f['stop_cause'] == 'page_changes' and f['ended_at'] == stamp(), f
tick(); apply('amend_unit', 'contract-3', dict(unit=contract, invariants='The board keeps its order and its colour.'))
tick(); review('page-4', 'page', digest('page 4'), unit=contract)
apply('ready_unit', 'contract-3', dict(unit=contract, intent=gadget)); apply('start_unit', 'contract-3', dict(unit=contract, intent=gadget))
apply('carry_review', 'older-than-amendment', dict(review=r1, unit=contract, head=H2), 'review_older_than_amendment')
r3 = review('contract-h2', 'packet', H2, unit=contract, repository=REPO, round=1); tick()
apply('record_head', 'contract-h3', dict(unit=contract, head=H3, writers=['fixture-implementer', 'fixture-updater'], pull_request=7, rounds=1))
carried = created(apply('carry_review', 'contract-h3', dict(review=r3, unit=contract, head=H3)), 'review')
f, g = fields(carried), fields(r3)
assert f['kind'] == 'packet' and f['head'] == H3 and f['status'] == 'closed' and f['verdict'] == 'accept' and f['carried_from'] == r3 and f['started_at'] == f['closed_at'] == stamp(), f
assert all(f[k] == g[k] for k in ('task', 'model', 'effort', 'prompt', 'prompt_sha256', 'repository', 'unit', 'report', 'report_sha256')) and 'session' not in f and 'round' not in f, f
apply('accept_unit', 'contract-h3', dict(unit=contract, head=H3)); assert fields(contract)['state'] == 'accepted' and fields(contract)['head'] == H3
# The search delivery makes (accepting in crates/ap/src/review.rs): by kind, head and status, then the fields it reads.
for reviewed, head in [(r1, H1), (r3, H2), (carried, H3)]:
    found = call(['search'], dict(object_type='review', where={'all': [{'eq': ['kind', 'packet']}, {'eq': ['head', head]}, {'eq': ['status', 'closed']}]}))
    assert reviewed in [o['id'] for o in found['objects']] and found['basis']['qualified'] is True, found
    f = fields(reviewed); assert f['verdict'] == 'accept' and f['repository'] == REPO and f['report'] == REPORT['report'] and f['report_sha256'] == REPORT['report_sha256'] and f['closed_at'], f
print('gate_row=v11-delivery-search expected=admitted reviews=3')
# The Lead's approval: the role lead, known to an engine with the verb log.
skipped = []
knows_role = 'log' in [c['command'] for c in json.loads(subprocess.check_output([binary, '--help'], text=True, timeout=10))['commands']]
if knows_role:
    apply('approve_unit', 'contract-no-role', dict(unit=contract, head=H3, reason='The Lead read the diff.'), 'not_the_lead')
    apply('approve_unit', 'contract-old-head', dict(unit=contract, head=H1, reason='The Lead read the diff.'), 'head_differs', role='lead')
    apply('approve_unit', 'contract-reason-empty', dict(unit=contract, head=H3, reason=''), 'reason_empty', role='lead')
    apply('approve_unit', 'dep-merged', dict(unit=dep, head=H1, reason='The Lead read the diff.'), 'unit_not_accepted', role='lead')
    apply('approve_unit', 'contract', dict(unit=contract, head=H3, reason='The Lead read the diff.'), role='lead'); assert fields(contract)['approved_head'] == H3
    held_contract = hold('contract-merge', unit=contract)
    apply('merge_unit', 'contract-held', dict(unit=contract, head=H3, merged_commit=MERGED, intent=gadget), 'held'); lift('contract-merge', held_contract)
    apply('merge_unit', 'contract', dict(unit=contract, head=H3, merged_commit=MERGED, intent=gadget)); assert fields(contract)['state'] == 'merged'
    apply('drop_unit', 'contract-merged', dict(unit=contract, reason='late'), 'unit_ended')
else:
    skipped.append('approve_unit and merge_unit of fixture-contract')
    apply('drop_unit', 'contract', dict(unit=contract, reason='the engine has no role'))
# A hold stops the merge of an accepted plain unit, and the way back into running.
plain = unit('fixture-plain'); apply('add_unit', 'plain', dict(op, unit=plain)); apply('ready_unit', 'plain', dict(unit=plain, intent=gadget))
apply('note_unit_wait', 'plain', dict(unit=plain, waits_on='a free slot')); assert fields(plain)['waits_on'] == 'a free slot'
apply('start_unit', 'plain-no-intent', dict(unit=plain), 'intent_parameter_required')
apply('start_unit', 'plain-other-intent', dict(unit=plain, intent=covered), ('intent_differs', 'intent_not_open_for_units'))
apply('start_unit', 'plain', dict(unit=plain, intent=gadget)); apply('note_unit_wait', 'plain-running', dict(unit=plain, waits_on='x'), 'unit_not_ready')
apply('record_head', 'plain', dict(unit=plain, head=H1, writers=['fixture-implementer'])); review('plain', 'packet', H1, unit=plain, repository=REPO)
apply('accept_unit', 'plain', dict(unit=plain, head=H1))
apply('accept_unit', 'plain-again', dict(unit=plain, head=H1), 'unit_not_running')
apply('merge_unit', 'dep-again', dict(unit=dep, head=H1, merged_commit=MERGED, intent=gadget), 'unit_not_accepted')
apply('merge_unit', 'plain-other-intent', dict(unit=plain, head=H1, merged_commit=MERGED, intent=covered), 'intent_differs')
apply('reopen_unit', 'plain-no-intent', dict(unit=plain, cause='merge_failed'), 'intent_parameter_required')
apply('reopen_unit', 'plain-other-intent', dict(unit=plain, cause='merge_failed', intent=covered), 'intent_differs')
held_plain = hold('plain', unit=plain)
apply('merge_unit', 'plain-held', dict(unit=plain, head=H1, merged_commit=MERGED, intent=gadget), 'held')
apply('reopen_unit', 'plain-held', dict(unit=plain, cause='merge_failed', intent=gadget), 'held')
lift('plain', held_plain); apply('merge_unit', 'plain', dict(unit=plain, head=H1, merged_commit=MERGED, intent=gadget)); assert fields(plain)['state'] == 'merged'
stopped = unit('fixture-stopped'); apply('add_unit', 'stopped', dict(op, unit=stopped)); apply('ready_unit', 'stopped', dict(unit=stopped, intent=gadget))
apply('resume_unit', 'stopped-ready', dict(unit=stopped, intent=gadget), 'unit_not_stopped')
apply('start_unit', 'stopped', dict(unit=stopped, intent=gadget)); apply('stop_unit', 'stopped', dict(unit=stopped, cause='limit'))
apply('stop_unit', 'stopped-again', dict(unit=stopped, cause='limit'), 'unit_not_running')
apply('record_head', 'stopped', dict(unit=stopped, head=H2, writers=['fixture-implementer'])); assert fields(stopped)['head'] == H2
held_stopped = hold('stopped', unit=stopped); apply('resume_unit', 'stopped-held', dict(unit=stopped, intent=gadget), 'held'); lift('stopped', held_stopped)
apply('resume_unit', 'stopped-other-intent', dict(unit=stopped, intent=covered), 'intent_differs')
apply('resume_unit', 'stopped-no-intent', dict(unit=stopped), 'intent_parameter_required')
apply('resume_unit', 'stopped', dict(unit=stopped, intent=gadget)); assert fields(stopped)['state'] == 'running'
# A unit of class contract or effect enters running only under a confirmed intent.
effect = unit('fixture-effect'); apply('add_unit', 'effect', dict(op, unit=effect, **{'class': 'effect'})); tick(); review('page-effect', 'page', digest('page effect'), unit=effect)
apply('ready_unit', 'effect', dict(unit=effect, intent=gadget))
tick(); apply('amend_intent', 'gadget-why', dict(intent=gadget, why='The fixture needs gadgets.')); assert fields(gadget)['status'] == 'draft'
apply('start_unit', 'effect-draft-intent', dict(unit=effect, intent=gadget), ('intent_not_open_for_units', 'intent_not_confirmed'))
apply('ready_unit', 'second-draft-intent', dict(unit=unit('fixture-stopped'), intent=gadget), ('unit_not_draft', 'intent_not_open_for_units'))
apply('add_unit', 'gadget-draft', dict(op, unit=unit('fixture-late')), 'intent_not_open_for_units')
confirm('gadget-yes-again', gadget)
apply('start_unit', 'effect', dict(unit=effect, intent=gadget)); apply('stop_unit', 'effect', dict(unit=effect, cause='limit'))
tick(); apply('amend_intent', 'gadget-why-2', dict(intent=gadget, why='The fixture needs more gadgets.'))
apply('resume_unit', 'effect-draft-intent', dict(unit=effect, intent=gadget), 'intent_not_confirmed')
confirm('gadget-yes-third', gadget)
apply('resume_unit', 'effect', dict(unit=effect, intent=gadget)); assert fields(effect)['state'] == 'running'
# The page complete before ready.
bare = unit('fixture-bare'); apply('add_unit', 'bare', dict(unit=bare, lane='objective', intent=gadget, objective='A bare page.', **{'class': 'plain'}))
apply('ready_unit', 'bare', dict(unit=bare, intent=gadget), ('write_set_missing', 'acceptance_missing', 'tiers_missing'))
apply('amend_unit', 'bare-write-set', dict(unit=bare, write_set=[], acceptance=[], implementer=page['implementer'], verifier=page['verifier']))
apply('ready_unit', 'bare-empty', dict(unit=bare, intent=gadget), ('write_set_missing', 'acceptance_missing'))
apply('amend_unit', 'bare-acceptance', dict(unit=bare, write_set=['docs/**']))
apply('ready_unit', 'bare-acceptance', dict(unit=bare, intent=gadget), 'acceptance_missing')
first, second = unit('fixture-first'), unit('fixture-second')
apply('add_unit', 'first', dict(op, unit=first)); apply('add_unit', 'second', dict(op, unit=second, depends_on=[first]))
apply('drop_unit', 'first-reason-empty', dict(unit=first, reason=''), 'reason_empty')
apply('drop_unit', 'first', dict(unit=first, reason='replaced by fixture-second')); f = fields(first); assert f['state'] == 'dropped' and f['drop_reason'] == 'replaced by fixture-second', f
apply('drop_unit', 'first-again', dict(unit=first, reason='again'), 'unit_ended')
apply('ready_unit', 'second', dict(unit=second, intent=gadget), 'dependency_dropped')
apply('amend_unit', 'first-dropped', dict(unit=first, notes='late'), 'unit_not_amendable')
# A unit without a repository: its head is the digest of its evidence, reviewed like any head.
nonrepo = {k: v for k, v in op.items() if k != 'repository'}
evidence = 'The measurement shows 3 widgets.'
def no_repository_unit(key, **extra):
    u = unit(key); apply('add_unit', key, dict(nonrepo, unit=u, **extra)); apply('ready_unit', key, dict(unit=u, intent=gadget)); apply('start_unit', key, dict(unit=u, intent=gadget))
    apply('record_head', key, dict(unit=u, head=digest(evidence), writers=['fixture-implementer']))
    accepting = review(key, 'packet', digest(evidence), unit=u); apply('accept_unit', key, dict(unit=u, head=digest(evidence)))
    return u, accepting
done, _ = no_repository_unit('fixture-done')
apply('merge_unit', 'done', dict(unit=done, head=digest(evidence), merged_commit=MERGED, intent=gadget), 'no_repository')
apply('close_unit', 'repository', dict(unit=plain, evidence=evidence, intent=gadget), ('unit_has_a_repository', 'unit_not_accepted'))
apply('close_unit', 'done-evidence-empty', dict(unit=done, evidence='', intent=gadget), 'evidence_empty')
apply('close_unit', 'done-no-intent', dict(unit=done, evidence=evidence), 'intent_parameter_required')
apply('close_unit', 'done-other-intent', dict(unit=done, evidence=evidence, intent=covered), 'intent_differs')
held_done = hold('done', unit=done); apply('close_unit', 'done-held', dict(unit=done, evidence=evidence, intent=gadget), 'held'); lift('done', held_done)
apply('close_unit', 'done', dict(unit=done, evidence=evidence, intent=gadget))
f = fields(done); assert f['state'] == 'done' and f['evidence'] == evidence and f['head'] == digest(evidence) and f['ended_at'] == stamp(), f
revoked, accepting = no_repository_unit('fixture-revoked')
apply('revoke_review', 'revoked', dict(review=accepting, reason='the review missed a path'))
apply('close_unit', 'revoked', dict(unit=revoked, evidence=evidence, intent=gadget), 'head_not_reviewed')
tick(); effect_doc = unit('fixture-effect-doc'); apply('add_unit', 'fixture-effect-doc', dict(nonrepo, unit=effect_doc, **{'class': 'effect'}))
tick(); review('page-effect-doc', 'page', digest('page effect doc'), unit=effect_doc)
apply('ready_unit', 'fixture-effect-doc', dict(unit=effect_doc, intent=gadget)); apply('start_unit', 'fixture-effect-doc', dict(unit=effect_doc, intent=gadget))
apply('record_head', 'fixture-effect-doc', dict(unit=effect_doc, head=digest(evidence), writers=['fixture-implementer']))
review('fixture-effect-doc', 'packet', digest(evidence), unit=effect_doc); apply('accept_unit', 'fixture-effect-doc', dict(unit=effect_doc, head=digest(evidence)))
apply('close_unit', 'effect-doc-unapproved', dict(unit=effect_doc, evidence=evidence, intent=gadget), 'not_approved')
tick(); apply('amend_intent', 'gadget-why-3', dict(intent=gadget, why='The fixture needs gadgets again.'))
apply('reopen_unit', 'effect-doc-draft-intent', dict(unit=effect_doc, cause='diff_rejected', intent=gadget), 'intent_not_confirmed')
confirm('gadget-yes-fourth', gadget)
if knows_role:
    apply('approve_unit', 'effect-doc', dict(unit=effect_doc, head=digest(evidence), reason='The Lead read the evidence.'), role='lead')
    apply('close_unit', 'effect-doc', dict(unit=effect_doc, evidence=evidence, intent=gadget)); assert fields(effect_doc)['state'] == 'done'
else:
    skipped.append('approve_unit and close_unit of fixture-effect-doc')
apply('check_criterion', 'gadget', dict(criterion=gadget_criterion, state='pass', evidence='fixture-check: 3 gadgets'))
apply('close_intent', 'gadget-live-units', dict(intent=gadget, reason='fixture-check passed'), 'unit_not_ended')
if skipped: print('rehearsal: skipped without the role of the actor: ' + '; '.join(skipped), file=sys.stderr)
print('v11_unit_rows_complete')
# Runs.
apply('start_run', 'unknown-unit', dict(role='implementer', runtime='fixture-cli', model='fixture-model', unit=unit('absent-v11-unit')), 'unknown_unit')
apply('start_run', 'unknown-intent', dict(role='lead', runtime='fixture-cli', model='fixture-model', intent=oid('intent', 'absent-v11-intent')), 'unknown_intent')
run = created(apply('start_run', 'implementer', dict(role='implementer', runtime='fixture-cli', model='fixture-model', effort='high', unit=stopped, session='fixture-implementer')), 'run')
f = fields(run); assert f['status'] == 'running' and f['running_in'] == stopped and f['unit'] == stopped and f['started_at'] == stamp(), f
view = call(['render'], dict(view='runs_running', args={})); assert view['rows'] == 1 and '| implementer | fixture-model |' in view['markdown'], view
tick(); counts = dict(input_tokens=1200, cache_read_tokens=30000, cache_write_tokens=4000, output_tokens=900)
apply('close_run', 'implementer', dict(run=run, state='pushed', served_by='fixture-model-2', limit='five-hour 12%', record='/home/palantirkc/work/fixture/run.json', **counts))
f = fields(run); assert f['status'] == 'closed' and f.get('running_in') is None and f['ended_at'] == stamp() and f['state'] == 'pushed' and all(f[k] == v for k, v in counts.items()), f
apply('close_run', 'implementer-again', dict(run=run, state='pushed'), 'run_not_running')
view = call(['render'], dict(view='runs_of_unit', args=dict(unit='fixture-stopped'))); assert view['rows'] == 1 and '| closed | pushed |' in view['markdown'], view
# Findings.
finding = oid('finding', 'fixture-detector-widget-drift')
fp = dict(finding=finding, kind='drift', detector='fixture-detector', summary='The invented board drifts from its page.', evidence='line 3 differs', severity='act')
apply('open_finding', 'drift', fp); f = fields(finding); assert f['state'] == 'open' and f['first_seen_at'] == f['last_seen_at'] == stamp(), f
apply('open_finding', 'drift-again', fp, 'revision_conflict')
tick(); apply('see_finding_again', 'drift', dict(finding=finding, evidence='line 4 differs', severity='urgent'))
f = fields(finding); assert f['last_seen_at'] == stamp() and f['first_seen_at'] != stamp() and f['severity'] == 'urgent' and f['evidence'] == 'line 4 differs', f
apply('plan_finding_as_unit', 'unknown-unit', dict(finding=finding, unit=unit('absent-v11-unit')), 'unknown_unit')
apply('plan_finding_as_unit', 'drift', dict(finding=finding, unit=stopped)); f = fields(finding); assert f['state'] == 'planned' and f['unit'] == stopped, f
apply('plan_finding_as_unit', 'planned', dict(finding=finding, unit=stopped), 'finding_not_open')
apply('plan_finding_as_intent', 'planned', dict(finding=finding, intent=gadget), 'finding_not_open')
apply('reopen_finding', 'live', dict(finding=finding, evidence='x'), 'finding_live')
apply('close_finding', 'reason-empty', dict(finding=finding, reason=''), 'reason_empty')
apply('close_finding', 'drift', dict(finding=finding, reason='The page and the board agree.')); f = fields(finding); assert f['state'] == 'closed' and f['ended_at'] == stamp(), f
apply('close_finding', 'closed', dict(finding=finding, reason='again'), 'finding_not_live')
apply('see_finding_again', 'closed', dict(finding=finding), 'finding_not_live')
apply('dismiss_finding', 'closed', dict(finding=finding, reason='x'), 'finding_not_live')
tick(); apply('reopen_finding', 'drift', dict(finding=finding, evidence='line 5 differs')); f = fields(finding); assert f['state'] == 'open' and f['last_seen_at'] == stamp(), f
apply('plan_finding_as_intent', 'unknown-intent', dict(finding=finding, intent=oid('intent', 'absent-v11-intent')), 'unknown_intent')
apply('plan_finding_as_intent', 'drift', dict(finding=finding, intent=gadget)); f = fields(finding); assert f['state'] == 'planned' and f['intent'] == gadget and f.get('unit') is None, f
view = call(['render'], dict(view='findings_open', args={})); assert view['rows'] == 1 and '| fixture-detector-widget-drift | urgent | drift |' in view['markdown'], view
apply('dismiss_finding', 'reason-empty', dict(finding=finding, reason=''), 'reason_empty')
apply('dismiss_finding', 'drift', dict(finding=finding, reason='The fixture accepts the drift.')); assert fields(finding)['state'] == 'dismissed'
# Requests routed to a unit or a finding.
tick(); answered = request('answered', 'order', topic='v11-small')
apply('route_request_to_unit', 'superseded', dict(request=r2, unit=small), 'request_not_current')
apply('route_request_to_unit', 'confirming', dict(request=r4, unit=small), 'request_confirms_an_intent')
apply('route_request_to_unit', 'unknown-unit', dict(request=answered, unit=unit('absent-v11-unit')), 'unknown_unit')
apply('route_request', 'answered', dict(request=answered, intent=gadget))
r = apply('route_request_to_unit', 'answered', dict(request=answered, unit=small)); assert r['edits'] and all(e['id'] == answered for e in r['edits']), r
f = fields(answered); assert f['route'] == 'unit' and f['unit'] == small and f.get('intent') is None and f['routed_at'] == stamp(), f
apply('route_request_to_finding', 'superseded', dict(request=r2, finding=finding), 'request_not_current')
apply('route_request_to_finding', 'confirming', dict(request=r4, finding=finding), 'request_confirms_an_intent')
apply('route_request_to_finding', 'unknown-finding', dict(request=answered, finding=oid('finding', 'absent-v11-finding')), 'unknown_finding')
apply('route_request_to_finding', 'answered', dict(request=answered, finding=finding)); f = fields(answered); assert f['route'] == 'finding' and f['finding'] == finding and f.get('unit') is None, f
apply('route_request', 'answered-back', dict(request=answered, intent=gadget)); f = fields(answered); assert f['route'] == 'intent' and f.get('finding') is None and f.get('unit') is None, f
apply('route_request_to_unit', 'answered-again', dict(request=answered, unit=small))
apply('set_request_aside', 'answered', dict(request=answered, reason='answered at once')); f = fields(answered); assert f['route'] == 'aside' and f.get('unit') is None and f.get('intent') is None, f
# The views of version 11.
view = call(['render'], dict(view='units_of_intent', args=dict(intent='gadget-board')))
assert view['rows'] == len(call(['search'], dict(object_type='unit', where={'eq': ['intent', gadget]}))['objects']) > 5 and '| fixture-dep | merged | plain | ' + REPO + ' | 6 |' in view['markdown'], view
view = call(['render'], dict(view='units_live', args={}))
live = sorted(o['id']['native_key'] for o in call(['search'], dict(object_type='unit', where={'in': ['state', ['ready', 'running', 'accepted', 'stopped']]}))['objects'])
assert view['rows'] == len(live) > 0 and all('| ' + k + ' |' in view['markdown'] for k in live) and '| fixture-dep |' not in view['markdown'], view
held_effect = hold('effect', unit=effect)
view = call(['render'], dict(view='holds_on_work', args={})); assert view['rows'] == 1 and 'fixture effect effect' in view['markdown'], view
lift('effect', held_effect)
view = call(['render'], dict(view='reviews_of_unit', args=dict(unit='fixture-contract')))
assert view['rows'] == len(call(['search'], dict(object_type='review', where={'eq': ['unit', contract]}))['objects']) >= 8 and '| revoked |' not in view['markdown'], view
print(view['markdown'])
# Refusals read back from the action log.
refused = call(['log'], dict(outcome='refused', action_type='add_unit', parameters=True))['rows']
assert any(r.get('failed') == ['intent_not_open_for_units'] and r['parameters']['unit'] == dep for r in refused), refused
refused = call(['log'], dict(outcome='refused', action_type='ready_unit', parameters=True))['rows']
assert any(r.get('failed') == ['page_not_reviewed'] and r['parameters']['unit'] == contract for r in refused), refused
print('v11_gate_rows_complete')

# Version 12, pinned in Git by the commit of main that holds it, over the objects made under version 11. A unit of
# class contract made under version 11, with a dependency and a page review accepted after its last amendment, waits for it as a draft.
tick(); digested, PAGE = unit('fixture-digest'), digest('page digest')
apply('add_unit', 'digest', dict(op, unit=digested, depends_on=[dep], estimate_lines=200, **{'class': 'contract'}))
accepted_page = review('page-digest', 'page', PAGE, unit=digested)
v11_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v11_before = [obj(i) for i in v11_objects]
v11_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args in [
    ('intents_open', {}), ('criteria_of_intent', dict(intent='gadget-board')), ('requests_unrouted', {}), ('units_of_intent', dict(intent='gadget-board')),
    ('units_live', {}), ('reviews_of_unit', dict(unit='fixture-digest')), ('reviews_of_unit', dict(unit='fixture-contract')),
    ('runs_of_unit', dict(unit='fixture-stopped')), ('findings_open', {}), ('holds_on_work', {})]]
V12_PIN = ('60b52b1ccc99f50ac04fba7f3c472d5dc16261f9', '2310273cccfceae52d352ba112340ef695b5f959a48b89b6b2453173c7cf4ee1')
schema = install(*V12_PIN)
assert schema['version'] == 12
v12_schema = schema
# Every action but ready_unit, and every other part of the schema, reads the same as at version 11.
assert {k: v for k, v in schema.items() if k not in ('version', 'action_types')} == {k: v for k, v in v11_schema.items() if k not in ('version', 'action_types')}
assert sorted(schema['action_types']) == sorted(v11_schema['action_types'])
assert [a for a in schema['action_types'] if schema['action_types'][a] != v11_schema['action_types'][a]] == ['ready_unit']
print('gate_row=v12-only-ready-unit-changed expected=admitted actions=' + str(len(schema['action_types'])))
redirect.trace(work, copies)
assert [obj(i) for i in v11_objects] == v11_before
for view, args, before in v11_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@11', 'schema=work@12'), view
print('gate_row=v12-v11-objects-and-views-unchanged expected=admitted objects=' + str(len(v11_objects)) + ' views=' + str(len(v11_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v12_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
unchanged = fingerprint()
assert migrate(11, 12) == dict(carried=0, already=0)
assert migrate(9, 12) == dict(carried=0, already=3)
assert fingerprint() == unchanged and [obj(i) for i in v11_objects] == v11_before
print('v12_migration_rows_complete')
phase = 'v12'
# ready_unit: a unit of class contract or effect needs a closed accepting page review whose head is page_digest.
apply('ready_unit', 'digest-no-page-digest', dict(unit=digested, intent=gadget), 'page_not_reviewed')
apply('ready_unit', 'digest-other-digest', dict(unit=digested, intent=gadget, page_digest=digest('another page')), 'page_not_reviewed')
apply('ready_unit', 'digest-not-hex', dict(unit=digested, intent=gadget, page_digest=PAGE.upper()), 'invalid_parameter')
# An amendment of the dependencies, the estimate and a tier keeps the accept: the review started before it.
tick(); apply('amend_unit', 'digest-tiers', dict(unit=digested, depends_on=[], estimate_lines=120, implementer=dict(page['implementer'], model='fixture-model-2')))
f = fields(digested); assert f['state'] == 'draft' and f['amended_at'] == stamp() > fields(accepted_page)['started_at'] and f['estimate_lines'] == 120, f
apply('ready_unit', 'digest', dict(unit=digested, intent=gadget, page_digest=PAGE)); assert fields(digested)['state'] == 'ready'
apply('ready_unit', 'digest-again', dict(unit=digested, intent=gadget, page_digest=PAGE), 'unit_not_draft')
apply('ready_unit', 'digest-again-other', dict(unit=digested, intent=gadget, page_digest=digest('another page')), ('unit_not_draft', 'page_not_reviewed'))
apply('start_unit', 'digest', dict(unit=digested, intent=gadget)); assert fields(digested)['state'] == 'running'
# A review that is running, or closed repair, of the digest given does not count; a plain unit needs none.
effect12 = unit('fixture-effect-12'); apply('add_unit', 'effect-12', dict(op, unit=effect12, **{'class': 'effect'}))
running_page = review('page-effect-12-running', 'page', digest('page effect 12'), close=False, unit=effect12)
apply('ready_unit', 'effect-12-running', dict(unit=effect12, intent=gadget, page_digest=digest('page effect 12')), 'page_not_reviewed')
apply('close_unit_review', 'page-effect-12-running', dict(review=running_page, verdict='repair', session='fixture-verifier', unit=effect12, **REPORT))
apply('ready_unit', 'effect-12-repair', dict(unit=effect12, intent=gadget, page_digest=digest('page effect 12')), 'page_not_reviewed')
review('page-effect-12', 'page', digest('page effect 12 b'), unit=effect12)
apply('ready_unit', 'effect-12', dict(unit=effect12, intent=gadget, page_digest=digest('page effect 12 b'))); assert fields(effect12)['state'] == 'ready'
plain12 = unit('fixture-plain-12'); apply('add_unit', 'plain-12', dict(op, unit=plain12)); apply('ready_unit', 'plain-12', dict(unit=plain12, intent=gadget))
refused = call(['log'], dict(outcome='refused', action_type='ready_unit', parameters=True))['rows']
assert any(r.get('failed') == ['page_not_reviewed'] and r['parameters']['unit'] == digested and r['parameters'].get('page_digest') == digest('another page') for r in refused), refused
print('v12_gate_rows_complete')

# Version 13, pinned in Git by the commit of main that holds it, over the objects made under version 12. Every object
# type, link type, action, view, value type and source of version 12 reads the same, but the optional property
# principles of intent.
v12_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v12_before = [obj(i) for i in v12_objects]
v12_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args in [
    ('intents_open', {}), ('criteria_of_intent', dict(intent='gadget-board')), ('requests_unrouted', {}), ('units_of_intent', dict(intent='gadget-board')),
    ('units_live', {}), ('reviews_of_unit', dict(unit='fixture-digest')), ('reviews_of_unit', dict(unit='fixture-effect-12')),
    ('runs_of_unit', dict(unit='fixture-stopped')), ('findings_open', {}), ('holds_on_work', {})]]
V13_PIN = ('4338bf4b9369c55dd0180f91ca39f430fac178d6', 'fdbb0f838035b28c41b81cc08ddacd6ea2e69480156dbe58fafa9409792b825c')
schema = json.loads(subprocess.check_output(['git', '-C', repo, 'show', V13_PIN[0] + ':ontology/work.schema.json'], timeout=10))
assert schema['version'] == 13
head = json.loads(json.dumps(schema))
assert head['object_types']['intent']['properties'].pop('principles') == {'type': {'array': {'ref': 'principle'}}, 'required': False, 'description': 'The principles the intent obeys; set whole by set_intent_principles.'}
for kind in ('object_types', 'link_types', 'action_types', 'views', 'value_types', 'sources'):
    assert all(head[kind].get(name) == body for name, body in v12_schema[kind].items()), kind
assert sorted(set(schema['object_types']) - set(v12_schema['object_types'])) == ['principle']
assert sorted(set(schema['link_types']) - set(v12_schema['link_types'])) == ['intent_principles', 'principle_parent', 'principle_requests']
assert sorted(set(schema['action_types']) - set(v12_schema['action_types'])) == ['amend_principle', 'record_principle', 'retire_principle', 'set_intent_principles']
assert sorted(set(schema['views']) - set(v12_schema['views'])) == ['principles'] and sorted(set(schema['value_types']) - set(v12_schema['value_types'])) == ['principle_status']
print('gate_row=v13-only-principles-added expected=admitted actions=' + str(len(schema['action_types'])))
assert install(*V13_PIN) == schema
redirect.trace(work, copies)
assert [obj(i) for i in v12_objects] == v12_before
for view, args, before in v12_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@12', 'schema=work@13'), view
print('gate_row=v13-v12-objects-and-views-unchanged expected=admitted objects=' + str(len(v12_objects)) + ' views=' + str(len(v12_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v13_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
unchanged = fingerprint()
assert migrate(12, 13) == dict(carried=0, already=0)
assert migrate(11, 13) == dict(carried=0, already=0)
assert migrate(9, 13) == dict(carried=0, already=3)
assert fingerprint() == unchanged and [obj(i) for i in v12_objects] == v12_before
print('v13_migration_rows_complete')
phase = 'v13'
# The walk of the principles. Each refusal row is before the action it guards; a refusal writes nothing.
def principle(key): return oid('principle', key)
tick(); said_a = request('principle-said-a', 'order', topic='v13-principles'); tick(); said_b = request('principle-said-b', 'decision', topic='v13-principles')
pillar, want = principle('fixture-pillar'), principle('fixture-want')
pp = dict(principle=pillar, order_key='01', statement='The invented widgets stay green.', requests=[said_a])
apply('record_principle', 'statement-empty', dict(pp, statement=''), 'statement_empty')
apply('record_principle', 'order-key-empty', dict(pp, order_key=''), 'order_key_empty')
apply('record_principle', 'requests-empty', dict(pp, requests=[]), 'requests_empty')
apply('record_principle', 'parent-absent', dict(pp, parent=principle('absent-v13-principle')), 'parent_not_current')
apply('record_principle', 'itself-as-parent', dict(pp, parent=pillar), 'parent_not_current')
tick(); apply('record_principle', 'pillar', pp)
f = fields(pillar)
assert f['status'] == 'current' and f['order_key'] == '01' and f['statement'] == pp['statement'] and f['requests'] == [said_a] and f.get('parent') is None, f
assert f['recorded_at'] == f['amended_at'] == stamp() and f.get('retired_at') is None, f
apply('record_principle', 'pillar-key-used', dict(pp, statement='Another want.'), 'revision_conflict')
tick(); apply('record_principle', 'want', dict(principle=want, order_key='01.01', statement='The invented widgets are a light green.', requests=[said_b], parent=pillar))
f = fields(want); assert f['status'] == 'current' and f['parent'] == pillar and f['requests'] == [said_b] and f['recorded_at'] == stamp(), f
# The view: current principles in order_key order; a pillar has no parent, so the engine's text for an absent reference stands there.
view = call(['render'], dict(view='principles', args={}))
rows = [l for l in view['markdown'].splitlines() if l.startswith('| ') and not l.startswith('| Order') and not l.startswith('| ---')]
assert view['rows'] == 2 and len(rows) == 2, view
cells = [[c.strip() for c in l.strip().strip('|').split('|')] for l in rows]
assert [c[0] for c in cells] == ['01', '01.01'] and [c[1] for c in cells] == ['fixture-pillar', 'fixture-want'], cells
assert cells[0][2] == '(missing)' and cells[1][2] == 'fixture-pillar' and cells[0][3] == pp['statement'], cells
print(view['markdown'])
# ont around: from the child by principle_requests to the request that states it; from the intent by intent_principles to the principle.
def around(source, where, path):
    return call(['around'], dict(**{'from': dict(object_type=source, where=where)}, path=path))
reached = around('principle', {'eq': ['order_key', '01.01']}, ['principle_requests'])
assert [o['id'] for o in reached['objects']] == [said_b], reached
reached = around('request', {'eq': ['topic', 'v13-principles']}, ['request_principles'])
assert sorted(o['id']['native_key'] for o in reached['objects']) == ['fixture-pillar', 'fixture-want'], reached
reached = around('principle', {'eq': ['order_key', '01.01']}, ['principle_parent'])
assert [o['id'] for o in reached['objects']] == [pillar], reached
reached = around('principle', {'eq': ['order_key', '01']}, ['principle_children'])
assert [o['id'] for o in reached['objects']] == [want], reached
# set_intent_principles: only an intent that is neither met nor retired.
apply('set_intent_principles', 'closed', dict(intent=covered, principles=[pillar]), 'intent_not_open')
apply('set_intent_principles', 'absent', dict(intent=oid('intent', 'absent-v13-intent'), principles=[pillar]), 'intent_not_open')
assert fields(gadget)['status'] in ('draft', 'assumed', 'confirmed') and fields(gadget).get('principles') is None
apply('set_intent_principles', 'gadget', dict(intent=gadget, principles=[pillar, want])); assert fields(gadget)['principles'] == [pillar, want]
reached = around('intent', {'eq': ['status', fields(gadget)['status']]}, ['intent_principles'])
assert sorted(o['id']['native_key'] for o in reached['objects']) == ['fixture-pillar', 'fixture-want'], reached
reached = around('principle', {'eq': ['order_key', '01']}, ['principle_intents'])
assert [o['id'] for o in reached['objects']] == [gadget], reached
apply('set_intent_principles', 'gadget-one', dict(intent=gadget, principles=[pillar])); assert fields(gadget)['principles'] == [pillar]
# amend_principle: the members given replace their values; the others stay.
amend = dict(principle=want)
apply('amend_principle', 'absent', dict(principle=principle('absent-v13-principle'), statement='x'), 'principle_not_current')
apply('amend_principle', 'statement-empty', dict(amend, statement=''), 'statement_empty')
apply('amend_principle', 'requests-empty', dict(amend, requests=[]), 'requests_empty')
apply('amend_principle', 'parent-absent', dict(amend, parent=principle('absent-v13-principle')), 'parent_not_current')
tick(); apply('amend_principle', 'want', dict(amend, order_key='01.02', statement='The invented widgets are a dark green.', requests=[said_a, said_b]))
f = fields(want)
assert f['order_key'] == '01.02' and f['statement'] == 'The invented widgets are a dark green.' and f['requests'] == [said_a, said_b] and f['parent'] == pillar, f
assert f['amended_at'] == stamp() and f['recorded_at'] != f['amended_at'] and f['status'] == 'current', f
tick(); apply('amend_principle', 'want-statement-only', dict(amend, statement='The invented widgets are a pale green.'))
f = fields(want); assert f['statement'] == 'The invented widgets are a pale green.' and f['order_key'] == '01.02' and f['requests'] == [said_a, said_b] and f['parent'] == pillar, f
apply('amend_principle', 'want-reparent', dict(amend, parent=pillar)); assert fields(want)['parent'] == pillar
# retire_principle: the view lists current principles only; a retired principle takes no parent and no amendment.
apply('retire_principle', 'absent', dict(principle=principle('absent-v13-principle'), reason='gone'), 'principle_not_current')
tick(); apply('retire_principle', 'want', dict(principle=want, reason='The user no longer wants a green shade.'))
f = fields(want); assert f['status'] == 'retired' and f['retired_at'] == stamp() and f['retired_reason'] == 'The user no longer wants a green shade.' and f['statement'] == 'The invented widgets are a pale green.', f
apply('retire_principle', 'want-again', dict(principle=want, reason='again'), 'principle_not_current')
apply('amend_principle', 'retired', dict(amend, statement='x'), 'principle_not_current')
apply('record_principle', 'retired-parent', dict(pp, principle=principle('fixture-under-retired'), parent=want), 'parent_not_current')
apply('amend_principle', 'retired-parent', dict(principle=pillar, parent=want), 'parent_not_current')
view = call(['render'], dict(view='principles', args={})); assert view['rows'] == 1 and 'fixture-want' not in view['markdown'] and 'fixture-pillar' in view['markdown'], view
print('v13_gate_rows_complete')
print('v13 principles: ok')

# Version 14, pinned in Git by the commit of main that holds it, over the objects made under version 13. Every value
# type, link type, action, view and source of version 13 reads the same; the two object types and sources of the
# routes, the function route and two types in the grant's read are added.
v13_schema = schema
v13_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold', 'principle')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v13_before = [obj(i) for i in v13_objects]
v13_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args in [
    ('intents_open', {}), ('criteria_of_intent', dict(intent='gadget-board')), ('requests_unrouted', {}), ('units_of_intent', dict(intent='gadget-board')),
    ('units_live', {}), ('reviews_of_unit', dict(unit='fixture-digest')), ('runs_of_unit', dict(unit='fixture-stopped')), ('findings_open', {}),
    ('holds_on_work', {}), ('principles', {})]]
V14_PIN = ('d0d469fc8b9cfa390c82b27acffbedd26388d342', 'b0af778a40e7bdb87219ea91c41720e5866ddcd0d7f5910f106d2698edae36b7')
v14_bytes = subprocess.check_output(['git', '-C', repo, 'show', V14_PIN[0] + ':ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v14_bytes).hexdigest() == V14_PIN[1]
v14_path = pathlib.Path(store).with_name('pinned-' + V14_PIN[0] + '.json'); v14_path.write_bytes(v14_bytes)
schema = json.loads(v14_bytes)
assert schema['version'] == 14
for kind in ('object_types', 'link_types', 'action_types', 'views', 'value_types', 'sources'):
    assert all(schema[kind].get(name) == body for name, body in v13_schema[kind].items()), kind
assert sorted(set(schema['object_types']) - set(v13_schema['object_types'])) == ['component', 'section']
assert sorted(set(schema['sources']) - set(v13_schema['sources'])) == ['components', 'sections']
for kind in ('link_types', 'action_types', 'views', 'value_types'):
    assert sorted(schema[kind]) == sorted(v13_schema[kind]), kind
assert v13_schema['functions'] == {} and sorted(schema['functions']) == ['route']
route_fn = schema['functions']['route']
assert route_fn['program'] == '/usr/local/bin/ap' and route_fn['timeout_ms'] == 2000 and route_fn['answer_bytes'] == 65536
assert {n: (p['type'], p['required']) for n, p in route_fn['parameters'].items()} == dict(words=('string', True), limit=('integer', False))
for name in ('sections', 'components'):
    source = schema['sources'][name]
    assert source['kind'] == 'records' and source['complete_listing'] is True and source['fresh_for_s'] == 3600 and source['time_member'] == 'observed_at'
    assert source['glob'] == '/var/lib/agent-platform/route/*.' + name + '.jsonl'
[grant], [grant13] = schema['grants'], v13_schema['grants']
assert grant['read'] == grant13['read'] + ['section', 'component'] and {k: v for k, v in grant.items() if k != 'read'} == {k: v for k, v in grant13.items() if k != 'read'}
assert {k: v for k, v in schema.items() if k not in ('version', 'object_types', 'sources', 'functions', 'grants')} == {k: v for k, v in v13_schema.items() if k not in ('version', 'object_types', 'sources', 'functions', 'grants')}
print('gate_row=v14-only-routes-added expected=admitted sources=' + str(len(schema['sources'])))
# The record sources read a directory of the rehearsal's own root instead of the host's route directory.
route_dir = pathlib.Path(store).parent / 'route'
HOST_ROUTE = '/var/lib/agent-platform/route/'
def routed_copy(path, label, name):
    # The copy of a schema file whose record sources read the rehearsal's own route directory.
    _, copy = redirect.make(path, work, copies, label)
    for source in ('sections', 'components'):
        assert copy['sources'][source]['glob'].startswith(HOST_ROUTE)
        copy['sources'][source]['glob'] = str(route_dir) + '/' + copy['sources'][source]['glob'][len(HOST_ROUTE):]
    assert HOST_ROUTE not in json.dumps(copy['sources'])
    out = pathlib.Path(store).with_name(name); out.write_text(json.dumps(copy))
    return out
routed = routed_copy(v14_path, 'pinned-v14.json', 'pinned-v14-route.json')
call(['schema', 'install', str(routed)])
redirect.trace(work, copies)
assert [obj(i) for i in v13_objects] == v13_before
for view, args, before in v13_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@13', 'schema=work@14'), view
print('gate_row=v14-v13-objects-and-views-unchanged expected=admitted objects=' + str(len(v13_objects)) + ' views=' + str(len(v13_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v14_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
described = call(['describe'], {})
[listed] = [f for f in described['functions'] if f['name'] == 'route']
assert listed['domain'] == 'work' and 'program' not in listed and sorted(p['name'] for p in listed['parameters']) == ['limit', 'words']
unchanged = fingerprint()
assert migrate(13, 14) == dict(carried=0, already=0)
assert migrate(12, 14) == dict(carried=0, already=0)
assert migrate(9, 14) == dict(carried=0, already=3)
assert fingerprint() == unchanged and [obj(i) for i in v13_objects] == v13_before
print('v14_migration_rows_complete')
# The walk of the two record sources. A source that fails appends nothing.
def index(code=0):
    p = subprocess.run([binary, '--store', store, '--now', str(now), 'index', '--source', 'sections', '--source', 'components'], text=True, capture_output=True, timeout=60)
    assert p.returncode == code, (p.returncode, p.stdout, p.stderr)
    return {s['source']: s for s in json.loads(p.stdout)['result']['sources']}
def heads(kind): return {o['id']['native_key']: {k: v.get('value') for k, v in o['head']['fields'].items()} for o in call(['search'], dict(object_type=kind, limit=1000))['objects']}
def lines(name, records): (route_dir / name).write_text(''.join(json.dumps(r) + '\n' for r in records))
def digest(text): return hashlib.sha256(text.encode()).hexdigest()
assert not route_dir.exists()
answer = index(2)
assert {n: (s['status'], s['code']) for n, s in answer.items()} == dict(sections=('source_failed', 'glob_root_missing'), components=('source_failed', 'glob_root_missing')), answer
route_dir.mkdir()
tick(); seen = now - 1
section_a = dict(type='section', key=digest('a'), observed_at=seen, repository='fixture/repo', path='README.md', heading='Fixture > Use', first_line=3, last_line=9, commit='1' * 40, digest=digest('readme'), text='Use the invented widget.')
section_b = dict(type='section', key=digest('b'), observed_at=seen, path=str(route_dir / 'AGENTS.md'), heading='AGENTS.md', first_line=1, last_line=2, digest=digest('agents'), text='')
component = dict(type='component', key=digest('c'), observed_at=seen, repository='fixture/repo', name='widget', purpose='The invented widget.', paths=['src/'], entry=['src/main.rs:1-10'], spec=[], tests=['tests/widget.rs'], commit='2' * 40)
lines('fixture--repo.sections.jsonl', [section_a]); lines('instructions.sections.jsonl', [section_b]); lines('fixture--repo.components.jsonl', [component])
answer = index()
assert (answer['sections']['appended']['initial'], answer['components']['appended']['initial']) == (2, 1), answer
found = heads('section')
assert found[digest('a')] == {k: v for k, v in section_a.items() if k not in ('type', 'key', 'observed_at')}, found
assert found[digest('b')] == dict({k: v for k, v in section_b.items() if k not in ('type', 'key', 'observed_at')}, repository=None, commit=None), found
assert heads('component') == {digest('c'): {k: v for k, v in component.items() if k not in ('type', 'key', 'observed_at')}}
print('gate_row=v14-records-indexed expected=admitted sections=2 components=1')
answer = index()
assert sum(answer['sections']['appended'].values()) == 0 and answer['sections']['unchanged'] == 2, answer
tick(); lines('fixture--repo.sections.jsonl', [dict(section_a, observed_at=now - 1, text='Use the invented gadget.')])
answer = index()
assert answer['sections']['appended']['update'] == 1 and heads('section')[digest('a')]['text'] == 'Use the invented gadget.', answer
# A line of the other type, and a text over 4,000 bytes, fail the source; the heads stay as they were.
before = heads('section')
lines('wrong.sections.jsonl', [dict(component, key=digest('w'))])
assert index(2)['sections']['code'] == 'record_type_mismatch'
lines('wrong.sections.jsonl', [dict(section_a, key=digest('w'), text='x' * 4001)])
assert index(2)['sections']['code'] == 'schema_violation:value_constraint_violation'
(route_dir / 'wrong.sections.jsonl').unlink()
assert heads('section') == before
print('gate_row=v14-records-refused expected=record_type_mismatch,value_constraint_violation')
# The complete listing: a file removed retracts its sections; an empty directory retracts every one.
(route_dir / 'instructions.sections.jsonl').unlink()
answer = index()
assert answer['sections']['appended']['retraction'] == 1 and sorted(heads('section')) == [digest('a')], answer
for f in route_dir.iterdir(): f.unlink()
answer = index()
assert (answer['sections']['appended']['retraction'], answer['components']['appended']['retraction']) == (1, 1) and heads('section') == {} and heads('component') == {}, answer
print('gate_row=v14-records-retracted expected=admitted')
print('v14_gate_rows_complete')
print('v14 routes: ok')

# Version 15, pinned in Git by the commit of main that holds it, over the objects made under version 14. Every member
# of version 14 reads the same in canonical JSON; only the function horizon is added.
v14_schema = schema
v14_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold', 'principle', 'section', 'component')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v14_before = [obj(i) for i in v14_objects]
v14_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args, _ in v13_views]
V15_PIN = ('8663375dde9d0557fbde136730766ef1f1d02210', '4fc42799c7cbd47c9e53bd2c3b1e6e2389d915e8aa79f2b501034f3793a1574a')
v15_bytes = subprocess.check_output(['git', '-C', repo, 'show', V15_PIN[0] + ':ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v15_bytes).hexdigest() == V15_PIN[1]
v15_path = pathlib.Path(store).with_name('pinned-' + V15_PIN[0] + '.json'); v15_path.write_bytes(v15_bytes)
schema = json.loads(v15_bytes)
assert schema['version'] == 15
head = json.loads(json.dumps(schema))
horizon_fn = head['functions'].pop('horizon')
assert {k: v for k, v in head.items() if k != 'version'} == {k: v for k, v in v14_schema.items() if k != 'version'}
assert horizon_fn['program'] == '/usr/local/bin/ap' and horizon_fn['timeout_ms'] == 30000 and horizon_fn['answer_bytes'] == 65536
assert {n: (p['type'], p['required']) for n, p in horizon_fn['parameters'].items()} == dict(intent=('string', True))
print('gate_row=v15-only-horizon-added expected=admitted functions=' + str(len(schema['functions'])))
call(['schema', 'install', str(routed_copy(v15_path, 'pinned-v15.json', 'pinned-v15-route.json'))])
redirect.trace(work, copies)
assert [obj(i) for i in v14_objects] == v14_before
for view, args, before in v14_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@14', 'schema=work@15'), view
print('gate_row=v15-v14-objects-and-views-unchanged expected=admitted objects=' + str(len(v14_objects)) + ' views=' + str(len(v14_views)))
described = call(['describe'], {})
[listed] = [f for f in described['functions'] if f['name'] == 'horizon']
assert listed['domain'] == 'work' and 'program' not in listed and [p['name'] for p in listed['parameters']] == ['intent']
assert sorted(f['name'] for f in described['functions'] if f['domain'] == 'work') == ['horizon', 'route']
unchanged = fingerprint()
assert migrate(14, 15) == dict(carried=0, already=0)
assert fingerprint() == unchanged and [obj(i) for i in v14_objects] == v14_before
print('v15_migration_rows_complete')
print('v15 horizon: ok')
# Version 16: the repository's file over the objects made under version 15. Before it, under version 15: two units of
# class contract with an accepted page review, and a draft and a running unit whose write_set holds the old form a/*.
phase = 'v15'
a16, b16, e_draft, e_running = unit('fixture-v16-a'), unit('fixture-v16-b'), unit('fixture-v16-e-draft'), unit('fixture-v16-e-running')
tick(); apply('add_unit', 'v16-a', dict(op, unit=a16, **{'class': 'contract'})); apply('add_unit', 'v16-b', dict(op, unit=b16, **{'class': 'contract'}))
review('page-v16-a', 'page', digest('v16 page a'), unit=a16); review('page-v16-b', 'page', digest('v16 page b'), unit=b16)
apply('add_unit', 'v16-e-draft', dict(op, unit=e_draft, write_set=['a/*'])); apply('add_unit', 'v16-e-running', dict(op, unit=e_running, write_set=['a/*']))
apply('ready_unit', 'v16-e-running', dict(unit=e_running, intent=gadget)); apply('start_unit', 'v16-e-running', dict(unit=e_running, intent=gadget))
assert fields(e_running)['state'] == 'running' and fields(e_draft)['write_set'] == fields(e_running)['write_set'] == ['a/*']
v15_schema = schema
v15_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold', 'principle', 'section', 'component')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v15_before = [obj(i) for i in v15_objects]
v15_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args, _ in v13_views]
V16_PIN = ('89d2a9f20141f91cfcf8437714e337996b51aa75', 'b96965329a1732168c1451b43240776af9fbaf24978913ad7e82eee03dd96b56')
v16_bytes = subprocess.check_output(['git', '-C', repo, 'show', V16_PIN[0] + ':ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v16_bytes).hexdigest() == V16_PIN[1]
v16_path = pathlib.Path(store).with_name('pinned-' + V16_PIN[0] + '.json'); v16_path.write_bytes(v16_bytes)
schema = json.loads(v16_bytes)
assert schema['version'] == 16
# The schema differs from version 15 only by points 1 to 6 of ontology/README.md, "Version 16": each is taken off a copy,
# and what is left reads as version 15.
head, old = json.loads(json.dumps(schema)), json.loads(json.dumps(v15_schema))
clause_type, clauses_source = head['object_types'].pop('clause'), head['sources'].pop('clauses')
assert clause_type['owner'] == {'source': 'clauses'} and clause_type['primary_key'] == {'from': '/key'} and 'title' not in clause_type
assert {n: (p['type'], p['required'], p['from']) for n, p in clause_type['properties'].items()} == dict(
    repository=('string', True, '/repository'), path=('string', True, '/path'), rule=('string', True, '/rule'), commit=('string', True, '/commit'),
    line=('integer', True, '/line'), tests_total=('integer', True, '/tests_total'), digest=({'value_type': 'sha256_hex'}, True, '/digest'),
    text=('string', True, '/text'), tests=({'array': 'string'}, True, '/tests'))
assert clause_type['properties']['text']['max_bytes'] == 4000
assert clauses_source == dict(kind='records', glob='/var/lib/agent-platform/clause/*.clauses.jsonl', time_member='observed_at', type_member='type', complete_listing=True, fresh_for_s=3600)
[grant], [grant15] = head['grants'], old['grants']
assert grant['read'] == grant15['read'] + ['clause']; grant['read'] = grant15['read']
clause_fn = head['functions'].pop('clause')
assert clause_fn['program'] == '/usr/local/bin/ap' and clause_fn['timeout_ms'] == 10000 and clause_fn['answer_bytes'] == 65536
assert {n: (p['type'], p['required']) for n, p in clause_fn['parameters'].items()} == dict(key=('string', True))
NEW_FORM, OLD_FORM = {'array': {'value_type': 'write_pattern'}}, {'array': 'string'}
unit_properties = head['object_types']['unit']['properties']
for name, kind in (('clauses', {'array': {'ref': 'clause'}}), ('wave', 'integer'), ('page_digest', {'value_type': 'sha256_hex'})):
    p = unit_properties.pop(name); assert p['type'] == kind and p['required'] is False, name
assert unit_properties['write_set']['type'] == NEW_FORM and old['object_types']['unit']['properties']['write_set']['type'] == OLD_FORM
unit_properties['write_set']['type'] = OLD_FORM
assert head['link_types'].pop('unit_clauses') == {'from': 'unit', 'via': 'clauses', 'to': 'clause', 'cardinality': 'many_to_many', 'names': {'forward': 'unit_clauses', 'reverse': 'clause_units'}}
for action, made, required in (('add_unit', 'create_object', False), ('amend_unit', 'modify_object', True)):
    a = head['action_types'][action]
    for name, kind in (('clauses', {'array': {'ref': 'clause'}}), ('wave', 'integer'), ('page_digest', {'value_type': 'sha256_hex'})):
        p = a['parameters'].pop(name); assert p['type'] == kind and p['required'] is (required and name == 'page_digest'), (action, name)
        assert a['rules'][0][made] == 'unit' and a['rules'][0]['set'].pop(name) == '$' + name, (action, name)
    assert a['parameters']['write_set']['type'] == NEW_FORM; a['parameters']['write_set']['type'] = OLD_FORM
ready, ready15 = head['action_types']['ready_unit'], old['action_types']['ready_unit']
codes = [c['code'] for c in ready['submission_criteria']]
assert codes == [c['code'] for c in ready15['submission_criteria']][:-1] + ['page_digest_differs', 'page_not_reviewed'], codes
assert ready['parameters']['page_digest']['type'] == ready15['parameters']['page_digest']['type'] and ready['parameters']['page_digest']['required'] is False
plan, differs, page_check = ready['submission_criteria'][-3:]
assert '{"gt": ["started_at", "$unit.added_at"]}' in json.dumps(plan) and '"$unit.page_digest"' in json.dumps(page_check) and '$page_digest"' not in json.dumps(page_check).replace('$unit.page_digest', '')
assert json.dumps(differs['none']) == json.dumps([{'all': [{'present': '$page_digest'}, {'exists': '$unit'}, {'present': '$unit.page_digest'}, {'ne': ['$page_digest', '$unit.page_digest']}]}])
assert [c for c in ready['submission_criteria'] if c['code'] not in ('plan_not_reviewed', 'page_digest_differs', 'page_not_reviewed')] == ready15['submission_criteria'][:-2]
head['action_types']['ready_unit'] = ready15
EDU = ['/home/palantirkc/work/edu-*', '/home/palantirkc/work/*/edu-*', '/home/palantirkc/work/*/*/edu-*']
for name in ('task_states', 'packets', 'reports'):
    assert head['sources'][name]['exclude'] == old['sources'][name]['exclude'] + EDU, name
    head['sources'][name]['exclude'] = old['sources'][name]['exclude']
assert {k: v for k, v in head.items() if k != 'version'} == {k: v for k, v in old.items() if k != 'version'}
print('gate_row=v16-only-points-1-to-6 expected=admitted object_types=' + str(len(schema['object_types'])) + ' functions=' + str(len(schema['functions'])))
# The record source of the clauses reads a directory of the rehearsal's own root instead of the host's clause directory.
clause_dir = pathlib.Path(store).parent / 'clause'
HOST_CLAUSE = '/var/lib/agent-platform/clause/'
def clause_copy(path, label, name):
    out = routed_copy(path, label, name)
    copy = json.loads(out.read_text())
    assert copy['sources']['clauses']['glob'] == HOST_CLAUSE + '*.clauses.jsonl'
    copy['sources']['clauses']['glob'] = str(clause_dir) + '/*.clauses.jsonl'
    assert HOST_CLAUSE not in json.dumps(copy['sources'])
    out.write_text(json.dumps(copy))
    return out
call(['schema', 'install', str(clause_copy(v16_path, 'pinned-v16.json', 'pinned-v16-route.json'))])
redirect.trace(work, copies)
assert [obj(i) for i in v15_objects] == v15_before
for view, args, before in v15_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@15', 'schema=work@16'), view
print('gate_row=v16-v15-objects-and-views-unchanged expected=admitted objects=' + str(len(v15_objects)) + ' views=' + str(len(v15_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v16_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
described = call(['describe'], {})
[listed] = [f for f in described['functions'] if f['name'] == 'clause']
assert listed['domain'] == 'work' and 'program' not in listed and [p['name'] for p in listed['parameters']] == ['key']
assert sorted(f['name'] for f in described['functions'] if f['domain'] == 'work') == ['clause', 'horizon', 'route']
unchanged = fingerprint()
assert migrate(15, 16) == dict(carried=0, already=0)
assert fingerprint() == unchanged and [obj(i) for i in v15_objects] == v15_before
print('v16_migration_rows_complete')
phase = 'v16'
# The walk of version 16. Each refusal row is before the action it guards; a refusal writes nothing.
c16 = unit('fixture-v16-c'); apply('add_unit', 'c-plain', dict(op, unit=c16)); assert fields(c16).get('page_digest') is None
# A plan review counts for a unit only when it started after the unit was added: one for the units above.
review('plan-v16', 'plan', digest('the plan, version 16'), intent=gadget)
# (a) A contract unit of version 15 with an accepted page review stores no digest: it is made ready after an amendment.
apply('ready_unit', 'a-reviewed-digest', dict(unit=a16, intent=gadget, page_digest=digest('v16 page a')), 'page_not_reviewed')
apply('amend_unit', 'a-no-digest', dict(unit=a16, estimate_lines=40), 'invalid_parameter')
tick(); apply('amend_unit', 'a-estimate', dict(unit=a16, estimate_lines=40, page_digest=digest('v16 page a')))
f = fields(a16); assert f['page_digest'] == digest('v16 page a') and f['estimate_lines'] == 40 and f['state'] == 'draft' and f.get('wave') is None and f.get('clauses') is None, f
apply('ready_unit', 'a-other-digest', dict(unit=a16, intent=gadget, page_digest=digest('another page')), 'page_digest_differs')
apply('ready_unit', 'a', dict(unit=a16, intent=gadget, page_digest=digest('v16 page a'))); assert fields(a16)['state'] == 'ready'
# (b) An amendment of the objective stores a new digest: the review of the old one no longer counts.
tick(); apply('amend_unit', 'b-objective', dict(unit=b16, objective='The invented widget board lists every widget twice.', page_digest=digest('v16 page b2')))
apply('ready_unit', 'b-old-digest', dict(unit=b16, intent=gadget, page_digest=digest('v16 page b')), ('page_digest_differs', 'page_not_reviewed'))
apply('ready_unit', 'b-new-digest', dict(unit=b16, intent=gadget, page_digest=digest('v16 page b2')), 'page_not_reviewed')
# (c) A plain unit needs no digest.
apply('ready_unit', 'c-plain', dict(unit=c16, intent=gadget)); assert fields(c16)['state'] == 'ready'
# (d) A unit added after the intent's only accepted plan review waits for a later one; a unit added before it does not.
tick(); plans_origin = request('v16-plans-origin', 'order', topic='v16-plans'); plans = oid('intent', 'v16-plans')
apply('draft_intent', 'plans', dict(dp, intent=plans, origin=plans_origin)); apply('add_criterion', 'plans', dict(cp, intent=plans)); confirm('v16-plans-yes', plans)
d_before, d_after = unit('fixture-v16-d-before'), unit('fixture-v16-d-after')
tick(); apply('add_unit', 'd-before', dict(op, unit=d_before, intent=plans))
review('plan-v16-plans', 'plan', digest('the plans, version 16'), intent=plans)
tick(); apply('add_unit', 'd-after', dict(op, unit=d_after, intent=plans))
apply('ready_unit', 'd-after-earlier-plan', dict(unit=d_after, intent=plans), 'plan_not_reviewed')
apply('ready_unit', 'd-before', dict(unit=d_before, intent=plans)); assert fields(d_before)['state'] == 'ready'
review('plan-v16-plans-2', 'plan', digest('the plans, version 16, again'), intent=plans)
apply('ready_unit', 'd-after', dict(unit=d_after, intent=plans)); assert fields(d_after)['state'] == 'ready'
# (e) write_set is an array of write_pattern. A unit stored under version 15 with the old form reads the same, and an
# action that would write it again is refused by the engine until amend_unit gives it the form.
e_new = unit('fixture-v16-e-new')
apply('add_unit', 'e-old-form', dict(op, unit=e_new, write_set=['a/*']), 'invalid_parameter')
apply('add_unit', 'e-new-form', dict(op, unit=e_new, write_set=['a/**', 'a/b.rs'])); assert fields(e_new)['write_set'] == ['a/**', 'a/b.rs']
assert obj(e_draft) == v15_before[v15_objects.index(e_draft)] and obj(e_running) == v15_before[v15_objects.index(e_running)]
apply('drop_unit', 'e-draft-old-form', dict(unit=e_draft, reason='The fixture drops it.'), 'invalid_parameter')
apply('stop_unit', 'e-running-old-form', dict(unit=e_running, cause='fixture_stop'), 'invalid_parameter')
tick(); apply('amend_unit', 'e-draft-new-form', dict(unit=e_draft, write_set=['a/**'], page_digest=digest('v16 page e')))
assert fields(e_draft)['write_set'] == ['a/**'] and fields(e_draft)['page_digest'] == digest('v16 page e')
apply('amend_unit', 'e-running-new-form', dict(unit=e_running, write_set=['a/**'], page_digest=digest('v16 page e')), 'unit_not_amendable')
# (f) The clauses: two records indexed, one changed, one retracted; a unit names one of them and one that does not exist.
def clause_index(code=0):
    p = subprocess.run([binary, '--store', store, '--now', str(now), 'index', '--source', 'clauses'], text=True, capture_output=True, timeout=60)
    assert p.returncode == code, (p.returncode, p.stdout, p.stderr)
    [one] = json.loads(p.stdout)['result']['sources']
    return one
def clause_lines(name, records): (clause_dir / name).write_text(''.join(json.dumps(r) + '\n' for r in records))
assert not clause_dir.exists()
assert clause_index(2)['code'] == 'glob_root_missing'
clause_dir.mkdir()
K1, K2, ABSENT = REPO + ':specs/widgets.md#R1', REPO + ':specs/widgets.md#R2', REPO + ':specs/widgets.md#R99'
tick()
clause_1 = dict(type='clause', key=K1, observed_at=now - 1, repository=REPO, path='specs/widgets.md', rule='R1', commit='4' * 40, line=7,
                digest=digest('- R1: The board lists every widget.'), text='- R1: The board lists every widget.', tests=['tests/widgets.rs:12'], tests_total=1)
clause_2 = dict(clause_1, key=K2, rule='R2', line=8, digest=digest('- R2: The board is green.'), text='- R2: The board is green.', tests=[], tests_total=0)
clause_lines('fixture-owner__widgets.clauses.jsonl', [clause_1]); clause_lines('fixture-owner__other.clauses.jsonl', [clause_2])
assert clause_index()['appended']['initial'] == 2
found = heads('clause')
assert found == {K1: {k: v for k, v in clause_1.items() if k not in ('type', 'key', 'observed_at')}, K2: {k: v for k, v in clause_2.items() if k not in ('type', 'key', 'observed_at')}}, found
assert {k: v.get('value') for k, v in call(['get'], dict(id=oid('clause', K1)))['objects'][0]['observations'][-1]['fields'].items()} == found[K1]
print('gate_row=v16-clauses-indexed expected=admitted clauses=2')
tick(); clause_lines('fixture-owner__widgets.clauses.jsonl', [dict(clause_1, observed_at=now - 1, text='- R1: The board lists every widget once.')])
assert clause_index()['appended']['update'] == 1 and heads('clause')[K1]['text'] == '- R1: The board lists every widget once.'
f16 = unit('fixture-v16-f')
apply('add_unit', 'f-clauses', dict(op, unit=f16, clauses=[oid('clause', K1), oid('clause', ABSENT)]))
assert fields(f16)['clauses'] == [oid('clause', K1), oid('clause', ABSENT)]
reached = around('clause', {'all': [{'eq': ['repository', REPO]}, {'eq': ['path', 'specs/widgets.md']}, {'eq': ['rule', 'R1']}]}, ['clause_units'])
assert [o['id'] for o in reached['objects']] == [f16] and reached['basis']['matched'] == 1, reached
(clause_dir / 'fixture-owner__other.clauses.jsonl').unlink()
assert clause_index()['appended']['retraction'] == 1 and sorted(heads('clause')) == [K1]
print('gate_row=v16-clauses-retracted expected=admitted')
# (g) The wave: set by add_unit, changed by amend_unit.
g16 = unit('fixture-v16-g')
apply('add_unit', 'g-wave', dict(op, unit=g16, wave=3)); assert fields(g16)['wave'] == 3
tick(); apply('amend_unit', 'g-wave', dict(unit=g16, wave=4, page_digest=digest('v16 page g')))
f = fields(g16); assert f['wave'] == 4 and f['page_digest'] == digest('v16 page g'), f
print('v16_gate_rows_complete')
print('v16 clauses: ok')
# Version 17, pinned in Git by the commit of main that holds it, over the objects made under version 16. The schema
# differs from version 16 only by points 1 to 9 of ontology/README.md, "Version 17": each is taken off a copy, and what
# is left reads as version 16.
v16_schema = schema
v16_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold', 'principle', 'section', 'component', 'clause')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v16_before = [obj(i) for i in v16_objects]
v16_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args, _ in v13_views]
V17_PIN = ('9683153aeb4f8ba0e855a78a94a2338d94587680', 'e944d88e686926eb4f1295e89ab6836d5280f004b43d12e154ea1eba0981510d')
v17_bytes = subprocess.check_output(['git', '-C', repo, 'show', V17_PIN[0] + ':ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v17_bytes).hexdigest() == V17_PIN[1]
v17_path = pathlib.Path(store).with_name('pinned-' + V17_PIN[0] + '.json'); v17_path.write_bytes(v17_bytes)
schema = json.loads(v17_bytes)
assert schema['version'] == 17
head, old = json.loads(json.dumps(schema)), json.loads(json.dumps(v16_schema))
UTC, NE = {'value_type': 'utc_time'}, {'value_type': 'nonempty_text'}
def shapes(properties): return {n: (p['type'], p['required'], p.get('max_bytes'), p.get('from')) for n, p in properties.items()}
for name in ('host', 'account', 'host_sample', 'check'):
    assert len(head['object_types'][name]['description'].encode()) <= 200 and all('description' not in p for p in head['object_types'][name]['properties'].values()), name
host_type, account_type = head['object_types'].pop('host'), head['object_types'].pop('account')
assert host_type['owner'] == 'ontology' and host_type['primary_key'] == {'from': '/native_key'} and host_type['title'] == 'node'
assert shapes(host_type['properties']) == dict(node=('string', True, 200, None), machine=('string', True, 100, None), declared_at=(UTC, True, None, None),
    answers=('boolean', False, None, None), free=('boolean', False, None, None), observed_at=(UTC, False, None, None), seen_at=(UTC, False, None, None),
    cpus=('integer', False, None, None), load_x100=('integer', False, None, None), available_mb=('integer', False, None, None))
assert account_type['owner'] == 'ontology' and account_type['primary_key'] == {'from': '/native_key'} and 'title' not in account_type
assert shapes(account_type['properties']) == dict(runtime=('string', False, 40, None), limit=('string', False, 300, None), limit_at=(UTC, False, None, None),
    limit_run=({'ref': 'run'}, False, None, None), observed_at=(UTC, False, None, None), allowance=('string', False, 300, None), allowance_at=(UTC, False, None, None),
    allowance_by=({'ref': 'request'}, False, None, None))
sample_type, check_type = head['object_types'].pop('host_sample'), head['object_types'].pop('check')
assert sample_type['owner'] == {'source': 'host_samples'} and check_type['owner'] == {'source': 'checks'} and sample_type['primary_key'] == check_type['primary_key'] == {'from': '/key'}
assert shapes(sample_type['properties']) == dict(host=({'ref': 'host'}, True, None, '/host'), at=(UTC, True, None, '/at'), answers=('boolean', True, None, '/answers'),
    free=('boolean', True, None, '/free'), **{k: ('integer', False, None, '/' + k) for k in ('cpus', 'load_x100', 'available_mb', 'slots', 'units_running', 'units_ready', 'cpu_waits')})
assert shapes(check_type['properties']) == dict(host=({'ref': 'host'}, True, None, '/host'), repository=('string', True, None, '/repository'), head=('string', True, None, '/head'),
    dir=('string', True, None, '/dir'), command=('string', True, 1000, '/command'), end=(UTC, True, None, '/end'), exit=('integer', True, None, '/exit'),
    milliseconds=('integer', True, None, '/milliseconds'), reason=('string', False, 40, '/reason'), start=(UTC, False, None, '/start'))
RESOURCE = '/var/lib/agent-platform/resource/'
assert head['sources'].pop('host_samples') == dict(kind='records', glob=RESOURCE + '*.samples.jsonl', time_member='observed_at', type_member='type', complete_listing=True, fresh_for_s=1728000)
assert head['sources'].pop('checks') == dict(kind='records', glob=RESOURCE + '*.checks.jsonl', time_member='observed_at', type_member='type', complete_listing=True, fresh_for_s=5184000)
for kind, names in (('run', ('host', 'uses')), ('install', ('host',))):
    for name in names:
        assert head['object_types'][kind]['properties'].pop(name) == {'type': {'ref': 'host' if name == 'host' else 'account'}, 'required': False}, (kind, name)
for action, names in (('start_run', ('host', 'uses')), ('start_install', ('host',))):
    a = head['action_types'][action]
    for name in names:
        assert a['parameters'].pop(name) == {'type': {'ref': 'host' if name == 'host' else 'account'}, 'required': False}, (action, name)
        assert a['rules'][0]['set'].pop(name) == '$' + name, (action, name)
LINKS = dict(run_host=('run', 'host', 'host', 'host_runs'), run_account=('run', 'uses', 'account', 'account_runs'), install_host=('install', 'host', 'host', 'host_installs'),
             host_sample_host=('host_sample', 'host', 'host', 'host_samples'), check_host=('check', 'host', 'host', 'host_checks'),
             account_limit_run=('account', 'limit_run', 'run', 'run_account_limits'), account_allowance_by=('account', 'allowance_by', 'request', 'request_allowances'))
for name, (a, via, b, reverse) in LINKS.items():
    assert head['link_types'].pop(name) == {'from': a, 'via': via, 'to': b, 'cardinality': 'many_to_one', 'names': {'forward': name, 'reverse': reverse}}, name
for name in ('declare_host', 'observe_host', 'observe_account', 'state_allowance'):
    a = head['action_types'].pop(name)
    assert len(a['description'].encode()) <= 200 and all('description' not in p for p in a['parameters'].values()), name
    globals()['act_' + name] = a
assert shapes(act_declare_host['parameters']) == dict(host=({'ref': 'host'}, True, None, None), node=(NE, True, 200, None), machine=(NE, True, 100, None))
assert act_declare_host['rules'] == [{'create_or_modify_object': 'host', 'set': {'node': '$node', 'machine': '$machine', 'declared_at': '$now'}}]
assert [c['code'] for c in act_observe_host['submission_criteria']] == ['unknown_host', 'seen_at_in_future']
assert act_observe_host['rules'] == [{'modify_object': 'host', 'set': {'answers': '$answers', 'free': '$free', 'seen_at': '$seen_at', 'cpus': '$cpus', 'load_x100': '$load_x100', 'available_mb': '$available_mb', 'observed_at': '$now'}}]
assert act_observe_account['rules'][0]['create_or_modify_object'] == 'account' and act_observe_account['rules'][0]['set']['observed_at'] == '$now'
[by_word] = act_state_allowance['submission_criteria']
waive_word = [c for c in old['action_types']['waive_criterion']['submission_criteria'] if c['code'] == 'by_not_the_users_word'][0]
assert by_word['code'] == 'by_not_the_users_word' and by_word['all'] == waive_word['all']
assert act_state_allowance['rules'] == [{'create_or_modify_object': 'account', 'set': {'allowance': '$allowance', 'allowance_by': '$by', 'allowance_at': '$now'}}]
capacity_fn, gate_fn = head['functions'].pop('capacity'), head['functions'].pop('gate')
assert capacity_fn['program'] == gate_fn['program'] == '/usr/local/bin/ap' and capacity_fn['answer_bytes'] == gate_fn['answer_bytes'] == 65536
assert capacity_fn['timeout_ms'] == 10000 and gate_fn['timeout_ms'] == 600000 and capacity_fn['parameters'] == {}
assert {n: (p['type'], p['required']) for n, p in gate_fn['parameters'].items()} == dict(since=(UTC, True), intent=('string', False))
[grant], [grant16] = head['grants'], old['grants']
assert grant['read'] == grant16['read'] + ['host', 'account', 'host_sample', 'check'] and grant['derive'] == grant16['derive'] + ['host', 'account']
assert grant['actions'] == grant16['actions'] + ['declare_host', 'observe_host', 'observe_account', 'state_allowance']
head['grants'] = old['grants']
assert {k: v for k, v in head.items() if k != 'version'} == {k: v for k, v in old.items() if k != 'version'}
print('gate_row=v17-only-points-1-to-9 expected=admitted object_types=' + str(len(schema['object_types'])) + ' functions=' + str(len(schema['functions'])))
# The record sources of the samples and the checks read a directory of the rehearsal's own root.
resource_dir = pathlib.Path(store).parent / 'resource'
def resource_copy(path, label, name):
    out = clause_copy(path, label, name)
    copy = json.loads(out.read_text())
    for source, suffix in (('host_samples', '*.samples.jsonl'), ('checks', '*.checks.jsonl')):
        assert copy['sources'][source]['glob'] == RESOURCE + suffix
        copy['sources'][source]['glob'] = str(resource_dir) + '/' + suffix
    assert RESOURCE not in json.dumps(copy['sources'])
    out.write_text(json.dumps(copy))
    return out
call(['schema', 'install', str(resource_copy(v17_path, 'pinned-v17.json', 'pinned-v17-route.json'))])
redirect.trace(work, copies)
assert [obj(i) for i in v16_objects] == v16_before
for view, args, before in v16_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@16', 'schema=work@17'), view
print('gate_row=v17-v16-objects-and-views-unchanged expected=admitted objects=' + str(len(v16_objects)) + ' views=' + str(len(v16_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v17_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
described = call(['describe'], {})
listed = {f['name']: f for f in described['functions'] if f['domain'] == 'work'}
assert sorted(listed) == ['capacity', 'clause', 'gate', 'horizon', 'route'], sorted(listed)
assert all('program' not in f for f in listed.values()) and listed['capacity']['parameters'] == [] and sorted(p['name'] for p in listed['gate']['parameters']) == ['intent', 'since'], listed
unchanged = fingerprint()
assert migrate(16, 17) == dict(carried=0, already=0)
assert fingerprint() == unchanged and [obj(i) for i in v16_objects] == v16_before
print('v17_migration_rows_complete')
phase = 'v17'
# The walk of version 17. Each refusal row is before the action it guards; a refusal writes nothing.
pc, pc_user, local = oid('host', 'pc'), oid('host', 'pc-user'), oid('host', 'local')
READINGS = ('answers', 'free', 'seen_at', 'cpus', 'load_x100', 'available_mb')
apply('observe_host', 'unknown', dict(host=pc, answers=True, free=True), 'unknown_host')
tick(); apply('declare_host', 'pc', dict(host=pc, node='fixture-pc', machine='pc'))
f = fields(pc); assert f['node'] == 'fixture-pc' and f['machine'] == 'pc' and f['declared_at'] == stamp() and all(f.get(k) is None for k in READINGS + ('observed_at',)), f
apply('declare_host', 'node-empty', dict(host=pc_user, node='', machine='pc'), 'invalid_parameter')
apply('observe_host', 'future', dict(host=pc, answers=True, free=True, seen_at=stamp(now + 1)), 'seen_at_in_future')
tick(); seen = stamp(); apply('observe_host', 'pc', dict(host=pc, answers=True, free=True, seen_at=seen, cpus=8, load_x100=150, available_mb=9000))
f = fields(pc); assert (f['answers'], f['free'], f['seen_at'], f['cpus'], f['load_x100'], f['available_mb'], f['observed_at']) == (True, True, seen, 8, 150, 9000, stamp()), f
# A host that does not answer: no reading and no seen_at given, so they stay.
tick(); apply('observe_host', 'pc-silent', dict(host=pc, answers=False, free=False))
f = fields(pc); assert (f['answers'], f['free'], f['seen_at'], f['cpus'], f['load_x100'], f['available_mb'], f['observed_at']) == (False, False, seen, 8, 150, 9000, stamp()), f
# A second declare changes node and machine and keeps the readings; a second node of the machine is a host of its own.
tick(); apply('declare_host', 'pc-again', dict(host=pc, node='fixture-pc-wsl', machine='fixture-pc-machine'))
f = fields(pc); assert (f['node'], f['machine'], f['declared_at'], f['answers'], f['seen_at'], f['cpus'], f['available_mb']) == ('fixture-pc-wsl', 'fixture-pc-machine', stamp(), False, seen, 8, 9000), f
apply('declare_host', 'pc-user', dict(host=pc_user, node='fixture-pc-user', machine='fixture-pc-machine'))
assert fields(pc_user)['machine'] == fields(pc)['machine'] and fields(pc_user).get('answers') is None
# Accounts: observe_account makes one and changes its limit; state_allowance makes one no run names.
acct, stated = oid('account', 'fixture-account'), oid('account', 'fixture-stated')
tick(); apply('observe_account', 'first', dict(account=acct, runtime='claude'))
f = fields(acct); assert f['runtime'] == 'claude' and f['observed_at'] == stamp() and f.get('limit') is None, f
# A run names its host and its account; one without them has neither.
tick(); with_both = created(apply('start_run', 'v17-host-uses', dict(role='lead', runtime='fixture-cli', model='fixture-model', host=pc, uses=acct)), 'run')
f = fields(with_both); assert f['host'] == pc and f['uses'] == acct and f['status'] == 'running', f
without_both = created(apply('start_run', 'v17-no-host', dict(role='lead', runtime='fixture-cli', model='fixture-model')), 'run')
f = fields(without_both); assert f.get('host') is None and f.get('uses') is None, f
tick(); apply('observe_account', 'limit', dict(account=acct, limit='{"utilization": 0.5}', limit_at=stamp(), limit_run=with_both))
f = fields(acct); assert (f['limit'], f['limit_at'], f['limit_run'], f['runtime']) == ('{"utilization": 0.5}', stamp(), with_both, 'claude'), f
tick(); apply('observe_account', 'limit-again', dict(account=acct, limit='{"utilization": 0.9}', limit_at=stamp()))
f = fields(acct); assert (f['limit'], f['limit_at'], f['limit_run']) == ('{"utilization": 0.9}', stamp(), with_both), f
tick(); preference = request('v17-preference', 'preference', topic='v17-accounts')
apply('state_allowance', 'preference', dict(account=stated, allowance='Five hours a day.', by=preference), 'by_not_the_users_word')
tick(); word = request('v17-allowance', 'decision', topic='v17-accounts')
apply('state_allowance', 'stated', dict(account=stated, allowance='Five hours a day.', by=word))
f = fields(stated); assert (f['allowance'], f['allowance_by'], f['allowance_at']) == ('Five hours a day.', word, stamp()) and f.get('runtime') is None and f.get('limit') is None, f
print('gate_row=v17-hosts-and-accounts expected=admitted')
# A sample line and a check line: indexed, read with every property, reached from their host, retracted with their file.
def resource_index(code=0):
    p = subprocess.run([binary, '--store', store, '--now', str(now), 'index', '--source', 'host_samples', '--source', 'checks'], text=True, capture_output=True, timeout=60)
    assert p.returncode == code, (p.returncode, p.stdout, p.stderr)
    return {s['source']: s for s in json.loads(p.stdout)['result']['sources']}
def resource_lines(name, records): (resource_dir / name).write_text(''.join(json.dumps(r) + '\n' for r in records))
assert not resource_dir.exists()
answer = resource_index(2)
assert {n: s['code'] for n, s in answer.items()} == dict(host_samples='glob_root_missing', checks='glob_root_missing'), answer
resource_dir.mkdir()
tick(); seen = now - 1
sample = dict(type='host_sample', key='pc@' + str(seen), observed_at=seen, at=stamp(seen), host='pc', answers=True, free=False, cpus=4, load_x100=380,
              available_mb=1800, slots=3, units_running=3, units_ready=1, cpu_waits=1)
check = dict(type='check', key='0123456789abcdef0123456789abcdef', observed_at=seen, host='pc', repository=REPO, head='5' * 40, dir='/fixture/wt/widgets',
             command='./check', start=stamp(seen - 60), end=stamp(seen), exit=0, milliseconds=60000, reason='pc_busy')
resource_lines('20261112.samples.jsonl', [sample]); resource_lines('log.checks.jsonl', [check])
answer = resource_index()
assert (answer['host_samples']['appended']['initial'], answer['checks']['appended']['initial']) == (1, 1), answer
def got(kind, key): return {k: v.get('value') for k, v in call(['get'], dict(id=oid(kind, key)))['objects'][0]['observations'][-1]['fields'].items()}
def mapped(record): return dict({k: v for k, v in record.items() if k not in ('type', 'key', 'observed_at')}, host=pc)
assert got('host_sample', sample['key']) == mapped(sample), got('host_sample', sample['key'])
assert got('check', check['key']) == mapped(check), got('check', check['key'])
for path, kind, key in ((['host_samples'], 'host_sample', sample['key']), (['host_checks'], 'check', check['key'])):
    reached = around('host', {'eq': ['node', 'fixture-pc-wsl']}, path)
    assert [o['id'] for o in reached['objects']] == [oid(kind, key)] and reached['basis']['matched'] == 1, reached
print('gate_row=v17-records-indexed expected=admitted samples=1 checks=1')
(resource_dir / '20261112.samples.jsonl').unlink(); (resource_dir / 'log.checks.jsonl').unlink()
answer = resource_index()
assert (answer['host_samples']['appended']['retraction'], answer['checks']['appended']['retraction']) == (1, 1) and heads('host_sample') == {} and heads('check') == {}, answer
print('gate_row=v17-records-retracted expected=admitted')
print('v17_gate_rows_complete')
print('v17 resources: ok')
# Version 18: the repository's file over the objects made under version 17. The schema differs from version 17 only by
# points 1 and 2 of ontology/README.md, "Version 18": each is taken off a copy, and what is left reads as version 17.
v17_schema = schema
v17_objects = [o['id'] for t in ('task', 'decision', 'request', 'intent', 'criterion', 'unit', 'review', 'run', 'finding', 'hold', 'principle', 'host', 'account')
               for o in call(['search'], dict(object_type=t, limit=1000))['objects']]
v17_before = [obj(i) for i in v17_objects]
v17_views = [(view, args, call(['render'], dict(view=view, args=args))) for view, args, _ in v13_views]
schema = json.loads(pathlib.Path(repo, 'ontology/work.schema.json').read_text())
assert schema['version'] == 18
head, old = json.loads(json.dumps(schema)), json.loads(json.dumps(v17_schema))
SHAPES18 = dict(job={'type': 'string', 'required': False, 'max_bytes': 40}, reserved_usd_micros={'type': 'integer', 'required': False},
                settled_usd_micros={'type': 'integer', 'required': False})
for name, shape in SHAPES18.items():
    assert head['object_types']['run']['properties'].pop(name) == shape, name
for action, names in (('start_run', ('job', 'reserved_usd_micros')), ('close_run', ('settled_usd_micros',))):
    a = head['action_types'][action]
    for name in names:
        assert a['parameters'].pop(name) == SHAPES18[name], (action, name)
        assert a['rules'][0]['set'].pop(name) == '$' + name, (action, name)
        assert '$' + name not in json.dumps(a['submission_criteria']), (action, name)
spend_fn = head['functions'].pop('api_spend')
assert len(spend_fn.pop('description').encode()) <= 200
assert spend_fn == dict(parameters=dict(month={'type': 'string', 'required': True}), program='/usr/local/bin/ap', answer_bytes=65536, timeout_ms=30000), spend_fn
assert {k: v for k, v in head.items() if k != 'version'} == {k: v for k, v in old.items() if k != 'version'}
print('gate_row=v18-only-points-1-and-2 expected=admitted functions=' + str(len(schema['functions'])))
call(['schema', 'install', str(resource_copy(pathlib.Path(repo, 'ontology/work.schema.json'), 'repository.json', 'repository-route.json'))])
redirect.trace(work, copies)
assert [obj(i) for i in v17_objects] == v17_before
for view, args, before in v17_views:
    answer = call(['render'], dict(view=view, args=args))
    assert answer['rows'] == before['rows'] and answer['markdown'] == before['markdown'].replace('schema=work@17', 'schema=work@18'), view
print('gate_row=v18-v17-objects-and-views-unchanged expected=admitted objects=' + str(len(v17_objects)) + ' views=' + str(len(v17_views)))
sizes = []
for name in ('ont_describe', 'ont_action_list'):
    wire = subprocess.run([binary, '--store', store, 'mcp'], input=json.dumps(dict(jsonrpc='2.0', id=1, method='tools/call', params=dict(name=name, arguments={}))) + '\n', text=True, capture_output=True, timeout=60)
    answer = json.loads(wire.stdout.splitlines()[0])['result']; assert wire.returncode == 0 and not answer['isError'], answer
    sizes.append(len(answer['content'][0]['text'].encode())); assert sizes[-1] <= 262144, (name, sizes[-1])
print('v18_mcp_bytes=' + ','.join(map(str, sizes)), file=sys.stderr)
described = call(['describe'], {})
listed = {f['name']: f for f in described['functions'] if f['domain'] == 'work'}
assert sorted(listed) == ['api_spend', 'capacity', 'clause', 'gate', 'horizon', 'route'], sorted(listed)
assert 'program' not in listed['api_spend'] and [p['name'] for p in listed['api_spend']['parameters']] == ['month'], listed['api_spend']
unchanged = fingerprint()
assert migrate(17, 18) == dict(carried=0, already=0)
assert migrate(18, 19, 2) == dict(status='unknown_pair')
assert fingerprint() == unchanged and [obj(i) for i in v17_objects] == v17_before
print('v18_migration_rows_complete')
phase = 'v18'
# The walk of version 18: a batch run reserved and settled, read back; a stage's run without the new parameters.
credit = oid('account', 'anthropic-api-key')
def batch_start(rid, job, reserved):
    return created(apply('start_run', rid, dict(role='batch', runtime='api', model='claude-sonnet-5-5', account='anthropic-api-key', uses=credit, job=job,
                                               reserved_usd_micros=reserved)), 'run')
tick(); settled = batch_start('settled', 'drift', 4000000)
f = fields(settled); assert (f['job'], f['reserved_usd_micros'], f['status'], f['uses'], f.get('settled_usd_micros')) == ('drift', 4000000, 'running', credit, None), f
tick(); apply('close_run', 'settled', dict(run=settled, state='ended', session='msgbatch_fixture', input_tokens=10, output_tokens=20, settled_usd_micros=1500000))
f = fields(settled); assert (f['job'], f['reserved_usd_micros'], f['settled_usd_micros'], f['status'], f['state'], f['session']) == ('drift', 4000000, 1500000, 'closed', 'ended', 'msgbatch_fixture'), f
# A close without a settlement leaves none: the run counts at its reservation.
tick(); unsettled = batch_start('unsettled', 'mining', 2000000)
apply('close_run', 'unsettled', dict(run=unsettled, state='close_failed'))
f = fields(unsettled); assert (f['status'], f['reserved_usd_micros'], f.get('settled_usd_micros')) == ('closed', 2000000, None), f
# The store does not check the amounts or the job: a negative reservation and a settlement above it are stored.
tick(); odd = batch_start('odd', 'no-such-job', -5)
apply('close_run', 'odd', dict(run=odd, state='ended', settled_usd_micros=7))
f = fields(odd); assert (f['job'], f['reserved_usd_micros'], f['settled_usd_micros']) == ('no-such-job', -5, 7), f
# A start and a close without the new parameters read as on version 17.
tick(); plain = created(apply('start_run', 'plain', dict(role='implementer', runtime='fixture-cli', model='fixture-model')), 'run')
apply('close_run', 'plain', dict(run=plain, state='done', input_tokens=1))
f = fields(plain); assert f['status'] == 'closed' and f['input_tokens'] == 1 and all(f.get(k) is None for k in SHAPES18), f
print('gate_row=v18-credit-ledger expected=admitted')
print('v18_gate_rows_complete')
print('v18 credit: ok')
PY
