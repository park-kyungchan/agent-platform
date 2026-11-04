#!/usr/bin/env python3
"""Run the byte-frozen host oracle with fake proc, tty, sqlite and Herdr I/O.
Only harness I/O is substituted; oracle source bytes and expectations stay fixed.
"""
import argparse
import builtins
import contextlib
import copy
import io
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent
assert hashlib.sha256((ROOT/'reference-pane.sh').read_bytes()).hexdigest() == 'c58948fc590fe1d30c14b0c6e2e069f36f8b2979f808253200d42d0cfe1aaa4b'
REFERENCE = (ROOT / 'reference-pane.sh').read_text().split("<<'PY'\n", 1)[1].rsplit('\nPY\n', 1)[0]

def oracle(case):
    pending = copy.deepcopy(case['queries'])
    def query(args, **kwargs):
        item = pending.pop(0)
        assert args == ['herdr'] + item['args'], (args, item)
        kind = item.get('error')
        if kind == 'timeout':
            raise subprocess.TimeoutExpired(args, 5)
        if kind == 'missing':
            raise OSError()
        return subprocess.CompletedProcess(args, int(kind == 'status'),
            'invalid' if kind == 'parse' else json.dumps({'result': item['result']}) if 'result' in item else '{}', '')
    actual_open = builtins.open
    def opened(path, *args, **kwargs):
        path = str(path)
        if path == '/proc/stat':
            return io.StringIO('btime 1000\n')
        assert path.startswith('/proc/'), path
        return actual_open(Path(case['proc_root']) / path.removeprefix('/proc/'), *args, **kwargs)
    def device(path):
        path = str(path)
        if path.startswith('/tty/'):
            return type('Device', (), {'st_mode': stat.S_IFCHR, 'st_rdev': int(path.split('/')[-1])})()
        if path.startswith('/proc/'):
            pid = path.split('/')[2]
            if pid in case.get('shell_ttys', {}):
                return type('Device', (), {'st_mode': stat.S_IFCHR, 'st_rdev': case['shell_ttys'][pid]})()
        raise OSError()
    def tty(fd):
        devices = case.get('ttys', [])
        if devices:
            return '/tty/' + str(devices[0])
        raise OSError()
    class Database:
        def __enter__(self): return self
        def __exit__(self, *args): pass
        def execute(self, sql, params):
            assert sql == 'SELECT created_at_ms,cwd FROM threads WHERE id=?'
            return self
        def fetchone(self): return case.get('origin')
    out, err = io.StringIO(), io.StringIO()
    env = {'HERDR_ENV':'1' if case.get('herdr_env', True) else '0',
           'CODEX_THREAD_ID':case.get('session',''), 'CODEX_HOME':'/fake/codex'}
    with contextlib.ExitStack() as stack:
        stack.enter_context(patch.dict(os.environ, env, clear=True))
        for name, replacement in [('builtins.open', opened), ('os.getpid', lambda:900),
            ('os.stat', device), ('os.ttyname', tty), ('os.sysconf', lambda _:100),
            ('os.path.isfile', lambda _: bool(case.get('origin'))), ('os.path.realpath', lambda p:p),
            ('sqlite3.connect', lambda *a,**k:Database()), ('subprocess.run', query)]:
            stack.enter_context(patch(name, replacement))
        stack.enter_context(patch.object(sys, 'argv', ['pane']+case['args']))
        stack.enter_context(contextlib.redirect_stdout(out))
        stack.enter_context(contextlib.redirect_stderr(err))
        try:
            exec(compile(REFERENCE, 'reference-pane.sh', 'exec'), {'__name__':'__main__'})
            status = 0
        except SystemExit as e:
            status = e.code
    assert not pending, pending
    return dict(stdout=out.getvalue(), stderr=err.getvalue(), status=status)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    cases = json.loads((ROOT/'cases.json').read_text())
    for case in cases:
        with tempfile.TemporaryDirectory(prefix='pane-fixture-', dir=ROOT) as tmp:
            case = copy.deepcopy(case)
            for pid, text in case.pop('proc', {}).items():
                target = Path(tmp)/pid/'stat'
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text)
            case['proc_root'] = tmp
            expected = oracle(case)
            assert expected == case['expected'], (case['name'], 'frozen oracle drift', expected)
            result = subprocess.run([args.binary], input=json.dumps(case), capture_output=True, text=True, check=True)
            actual = json.loads(result.stdout)
            assert actual == expected, (case['name'], expected, actual)
            print('PASS', case['name'])
    print(f'Pane golden comparison: {len(cases)} exact stdout/stderr/status fixtures passed')

if __name__ == '__main__': main()
