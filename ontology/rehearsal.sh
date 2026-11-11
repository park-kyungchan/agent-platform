#!/usr/bin/env bash
set -euo pipefail
[[ $# == 1 ]] || { echo 'usage: rehearsal.sh /path/to/ont' >&2; exit 2; }
ont_binary=$(realpath -- "$1")
repo=$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
rehearsal_root=$(mktemp -d)
trap 'python3 -c '\''import shutil,sys; shutil.rmtree(sys.argv[1])'\'' "$rehearsal_root"' EXIT
store="$rehearsal_root/store.sqlite"
[[ $store == "$rehearsal_root/"* ]] || exit 2
python3 - "$ont_binary" "$repo" "$rehearsal_root" "$store" <<'PY'
import hashlib, json, os, pathlib, subprocess, sys
binary, repo, root, store = sys.argv[1:]
assert pathlib.Path(store).resolve().parent == pathlib.Path(root).resolve()
def call(args, request=None, code=0):
    p = subprocess.run([binary, '--store', store, *args], input=json.dumps(request) if request else None,
                       text=True, capture_output=True, timeout=60)
    assert p.returncode == code, (args, p.returncode, p.stdout, p.stderr)
    data = json.loads(p.stdout)
    assert data['status'] == ('refused' if code else 'ok'), data
    return data.get('result', data)
v1 = subprocess.check_output(['git', '-C', repo, 'show',
    'f2d1b030bd1a0a66928ef57d9365027526e664da:ontology/work.schema.json'], timeout=10)
assert hashlib.sha256(v1).hexdigest() == '4573ab180f63889029ae98c892c996e810f9d8140cf26726ee6d5fffbfc10605'
first = pathlib.Path(root, 'v1.json'); first.write_bytes(v1)
call(['schema', 'install', str(first)])
call(['schema', 'install', os.path.join(repo, 'ontology/work.schema.json')])
call(['index', '--source', 'task_states'])
ns = {'instance': 'vps://palantirkc', 'account': 'root'}
task = {'object_type': 'task', 'namespace': ns, 'native_key': 'workspace-strategy-reassessment-20261109'}
directory = '/home/palantirkc/work/' + task['native_key']
base = dict(task=task, task_directory=directory)
def apply(action, request_id, parameters, refusal=None, outcome='applied'):
    r = call(['action', 'apply'], dict(op='apply_action', action_type=action,
             request_id=request_id, parameters=parameters), 2 if refusal else 0)
    if refusal:
        assert r['code'] == 'submission_criteria' and refusal in json.dumps(r), r
    else:
        assert r['outcome'] == outcome and r['actor_trust'] == 'self_reported', r
    return r

def created(r, kind):
    return next(e['id'] for e in r['edits'] if e['id']['object_type'] == kind and e['change'] == 'initial')
hp = dict(base, effect='publish', until='review accepted')
h = created(apply('set_hold', 'hold-1', hp), 'hold')
apply('set_hold', 'hold-1', hp, outcome='already_applied')
apply('set_hold', 'hold-duplicate', hp, 'duplicate_active_hold')
apply('set_hold', 'hold-directory', dict(hp, task_directory='/wrong'), 'task_directory_mismatch')
apply('lift_hold', 'lift-1', dict(hold=h, reason='review accepted'))
apply('lift_hold', 'lift-refused', dict(hold=h, reason='again'), 'hold_not_active')
apply('set_hold', 'hold-2', hp)
dp = dict(base, topic='route', text='use actions', decided_by='lead', reason='auditable', undo='supersede')
d = created(apply('record_decision', 'decision-1', dp), 'decision')
apply('record_decision', 'decision-duplicate', dp, 'decision_already_current')
sp = dict(dp, decision=d, text='use rendered actions')
apply('supersede_decision', 'decision-context', dict(sp, topic='wrong'), 'decision_context_mismatch')
new_d = created(apply('supersede_decision', 'decision-2', sp), 'decision')
apply('supersede_decision', 'decision-refused', sp, 'decision_not_current')
kp = dict(base, name='checks', definition='passing checks from rehearsal output', target='10')
k = created(apply('define_kpi', 'kpi-1', kp), 'kpi')
apply('define_kpi', 'kpi-duplicate', kp, 'duplicate_kpi')
empty = call(['render'], dict(view='kpi_board', args=dict(task=directory)))
assert '| checks | (missing) | (missing) | 10 | (missing) |' in empty['markdown'], empty
mp = dict(kpi=k, value='10', observed_at='2026-11-11T00:00:00Z', evidence='ontology/rehearsal.sh')
apply('record_measurement', 'measurement-1', mp)
apply('record_measurement', 'measurement-2', dict(mp, value='11'))
apply('set_kpi_target', 'target-1', dict(kpi=k, target='11'))
wp = dict(base, name='O13', tier='bounded', runtime='codex', session='rehearsal',
          packet_path='TASK-O13.md', started_at='2026-11-11T00:00:00Z')
w = created(apply('start_worker_run', 'worker-1', wp), 'worker_run')
apply('start_worker_run', 'worker-duplicate', wp, 'duplicate_running_worker')
apply('note_worker_run', 'note-1', dict(worker_run=w, result='checked'))
cp = dict(worker_run=w, result='passed', report_path='REPORT-O13.md', closed_at='2026-11-11T00:01:00Z')
apply('close_worker_run', 'close-1', cp)
apply('note_worker_run', 'note-refused', dict(worker_run=w, result='late'), 'worker_run_not_running')
apply('close_worker_run', 'close-refused', cp, 'worker_run_not_running')
apply('start_worker_run', 'worker-2', dict(wp, name='O13-review'))
measurements = call(['search'], dict(object_type='measurement', where={'eq': ['kpi', k]}))
assert len(measurements['objects']) == 2, measurements
def fields(oid):
    obj = call(['get'], dict(id=oid))['objects'][0]
    return {k: v.get('value') for k, v in obj['observations'][-1]['fields'].items()}
assert fields(h)['status'] == 'lifted' and fields(h)['lift_reason'] == 'review accepted'
assert fields(d)['status'] == 'superseded' and fields(new_d)['supersedes'] == d
assert fields(new_d)['task'] == task and fields(new_d)['topic'] == 'route'
assert fields(k)['value'] == '11' and fields(k)['evidence'] == 'ontology/rehearsal.sh'
assert sorted(fields(m['id'])['value'] for m in measurements['objects']) == ['10', '11']
assert fields(w)['status'] == 'closed' and fields(w)['report_path'] == 'REPORT-O13.md'
expected = {
    'kpi_board': '| checks | 11 | 2026-11-11T00:00:00Z | 11 | ontology/rehearsal.sh |',
    'active_holds': '| publish | review accepted |',
    'current_decisions': '| route | use rendered actions | lead | supersede |',
    'live_workers': '| O13-review | bounded | codex | rehearsal | TASK-O13.md | 2026-11-11T00:00:00Z |',
    'closed_workers': '| O13 | TASK-O13.md | REPORT-O13.md | 2026-11-11T00:01:00Z | passed |'}
for view, row in expected.items():
    r = call(['render'], dict(view=view, args=dict(task=directory)))
    assert r['rows'] == 1 and not r['truncated'] and row in r['markdown'], r
    other = call(['render'], dict(view=view, args=dict(task='/absent')))
    assert other['rows'] == 0, other
    print(r['markdown'])
PY
