#!/usr/bin/env python3
"""Independent literal S03 effect judge. Self-check does not execute Rust."""
import argparse
import copy
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
CORPUS = HERE / 'cases-v2.json'


def equal(actual, expected):
    # Type-sensitive, array-order-sensitive comparison.
    if type(actual) is not type(expected):
        raise AssertionError('literal conformance type mismatch')
    if isinstance(actual, dict):
        equal(set(actual), set(expected))
        for key in expected: equal(actual[key], expected[key])
    elif isinstance(actual, (list, tuple)):
        equal(len(actual), len(expected))
        for left, right in zip(actual, expected): equal(left, right)
    elif actual != expected:
        raise AssertionError('literal conformance mismatch')


def load():
    pins = json.loads((HERE / 'pins-v2.json').read_text())
    for name, digest in pins.items():
        equal(hashlib.sha256((HERE.parent / name).read_bytes()).hexdigest(), digest)
    data = json.loads(CORPUS.read_text())
    equal(data['contract_version'], 2)
    ids = [c['id'] for c in data['cases'] + data['internal_fault_cases']]
    equal(len(ids), len(set(ids)))
    return data


def self_check(data):
    expected = data['cases'][0]['expected']
    mutants = []
    for key, value in [('outcome','all_ok'), ('phases',['unchanged','applied']),
                       ('phases',['applied']), ('phases',['pending','applied']),
                       ('codex_target','/wrong'), ('claude_text','extra'),
                       ('outcome',0)]:
        v = copy.deepcopy(expected); v[key] = value; mutants.append(v)
    v = copy.deepcopy(expected); del v['phases']; mutants.append(v)
    v = copy.deepcopy(expected); v['extra'] = True; mutants.append(v)
    refusal = data['cases'][5]['expected']
    mutants += [dict(refusal, outcome='installed')]
    for n, mutant in enumerate(mutants):
        baseline = refusal if n == len(mutants)-1 else expected
        try:
            equal(mutant, baseline)
        except AssertionError:
            continue
        raise AssertionError('judge accepted negative control')
    # Wire admission checks also compare stdout/stderr/status independently.
    for actual in [(0,b'ok\n',b''),(3,b'payload',b'error'),(3,b'',b'wrong')]:
        try:
            equal([actual[0],actual[1].hex(),actual[2].hex()], [3,'',b'error'.hex()])
        except AssertionError:
            continue
        raise AssertionError('judge accepted wire negative control')
    print(f"S03 literal self-check: {len(data['cases'])} filesystem cases, "
          f"{len(data['internal_fault_cases'])} internal fault obligations, 13 controls rejected")


def invoke(binary, args, request):
    p = subprocess.run([str(binary),*args],input=json.dumps(request).encode(),
                       capture_output=True,timeout=10,check=False)
    if p.returncode == 3:
        equal(p.stdout, b'')
        value = json.loads(p.stderr)
        equal(set(value), {'error'})
        equal(set(value['error']), {'code','field'})
        return value['error']['code'], None
    if p.returncode not in (0,5):
        raise AssertionError(f'unexpected candidate exit {p.returncode}')
    equal(p.stderr, b'')
    if not p.stdout.endswith(b'\n') or p.stdout.count(b'\n') != 1:
        raise AssertionError('stdout must be one JSON line')
    value = json.loads(p.stdout)
    if 'outcome' in value:
        equal(p.returncode, 0 if value['outcome'] in ('installed','recovered','diagnosed') else 5)
    return value.get('outcome','planned'), value


def replace_tokens(text, root):
    if text is None:
        return None
    return text.replace('@ROOT_IMPORT@', f'@{root}/source/AGENTS.md').replace('@ROOT@',str(root))


def run_case(binary, parent, case):
    with tempfile.TemporaryDirectory(prefix='s03-', dir=parent) as td:
        root = Path(td); root.chmod(0o700)
        c = root/'a-codex'; h = root/'b-claude'; source = root/'source'
        command = root/'command'
        for d in (c,h,source,command): d.mkdir(mode=0o700)
        canonical = source/'AGENTS.md'; canonical.write_text('# Disposable guidance\n'); canonical.chmod(0o644)
        cf = c/'AGENTS.md'; hf = h/'CLAUDE.md'
        preserved = {c/'config.toml':b'[mcp_servers.daniel]\ncommand="${HOME}/vendor"\nunknown=7\n',
                     h/'settings.json':b'{"hooks":{"Stop":[{"matcher":"one","command":"./fake-hook"},{"matcher":"two","command":"./fake-hook"}]},"unknown":true}\n',
                     root/'fake-hook':b'#!/bin/sh\ntouch "$0.executed"\n'}
        for p,b in preserved.items(): p.write_bytes(b)
        (root/'fake-hook').chmod(0o700)
        setup = case['setup']
        if setup in ('comment','rollback-edit'): hf.write_text('# User comment\n')
        if setup in ('no-newline','rollback-bridge'): hf.write_text('# User comment')
        if setup == 'crlf': hf.write_bytes(b'# User comment\r\n')
        if setup == 'relative-link': cf.symlink_to('../source/AGENTS.md')
        if setup == 'native':
            cf.symlink_to(canonical); hf.write_text(f'# User instructions (Claude Code)\n\n@{canonical}\n')
        if setup == 'owned-conflict': cf.write_text('# Unowned instructions\n')
        if setup == 'override': (c/'AGENTS.override.md').write_text('# Override\n')
        if setup == 'claude-link': hf.symlink_to(canonical)
        if setup == 'dangling-link': cf.symlink_to(source/'absent')
        if setup == 'link-chain':
            (source/'hop').symlink_to(canonical); cf.symlink_to(source/'hop')
        if setup == 'ancestor-link':
            h.rename(root/'real-claude'); h.symlink_to(root/'real-claude')
        if setup in ('unsafe-mode','special-mode','foreign-owner','hardlink'):
            hf.write_text('# Original\n')
            if setup == 'unsafe-mode': hf.chmod(0o666)
            if setup == 'special-mode': hf.chmod(0o4644)
            if setup == 'foreign-owner': os.chown(hf,1,1)
            if setup == 'hardlink': os.link(hf,root/'other-link')
        if setup == 'unsafe-dir-mode': h.chmod(0o777)
        if setup == 'directory-target': hf.mkdir()
        if setup == 'fifo-target': os.mkfifo(hf)
        if setup == 'source-link':
            canonical.rename(source/'real'); canonical.symlink_to(source/'real')
        if setup == 'source-write-mode': canonical.chmod(0o666)
        if setup == 'source-hardlink': os.link(canonical,source/'other')
        if setup == 'nonutf8': hf.write_bytes(b'\xff')
        if setup == 'large-file': hf.write_bytes(b'x'*1048577)
        if setup == 'scope':
            # Unselected profile cannot be opened, parsed, or required to exist.
            h = root/'does-not-exist'
        spec = dict(schema_version=1,canonical_path=str(canonical),codex_dir=str(c),
                    claude_dir=str(h),scope=case['scope'],uid=os.geteuid(),
                    environment=dict(kind='disposable',root=str(root)))
        if setup == 'wrong-uid': spec['uid'] += 1
        if setup == 'nested': spec['claude_dir'] = str(c/'nested')
        if setup == 'doctor':
            outcome, result = invoke(binary,['doctor','inspect'],spec)
            equal(outcome,'diagnosed'); equal(result['hooks_executed'],0)
            equal(result['native_loading'],'unverified'); equal(result['p6_complete'],False)
            equal([e['status'] for e in result['entries']],['needs_install','needs_install'])
        else:
            outcome, plan = invoke(binary,['install','plan'],spec)
            if outcome == 'planned':
                if setup == 'config-edit':
                    p=c/'config.toml'; p.write_bytes(p.read_bytes()+b'# later unrelated\n'); preserved[p]=p.read_bytes()
                if setup == 'plan-edit': hf.write_text('# Edited after plan\n')
                if setup == 'source-edit': canonical.write_text('# Changed source\n')
                if setup == 'forged': plan['entries'][0]['after'] = dict(kind='file',text='unowned replacement')
                lock = None
                if setup == 'lock-busy':
                    lock = open(h/'.ap-install.lock','w'); os.chmod(lock.name,0o600); fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
                apply = dict(plan=plan,command_dir=str(command),writers_quiesced=setup!='no-coordination')
                outcome, result = invoke(binary,['install','apply'],apply)
                if lock: lock.close()
                if setup == 'state-reuse': outcome,result = invoke(binary,['install','apply'],apply)
                if setup.startswith('rollback'):
                    equal(outcome,'installed')
                    if setup in ('rollback-edit','rollback-bridge','rollback-created-edit'):
                        hf.write_bytes(hf.read_bytes()+b'# Later unrelated\n'); hf.chmod(0o600)
                    if setup == 'rollback-conflict': hf.write_text('@/different/source\n')
                    recover = dict(command_dir=str(command),writers_quiesced=True)
                    outcome,result = invoke(binary,['install','recover'],recover)
                    if setup == 'rollback-twice': outcome,result = invoke(binary,['install','recover'],recover)
                    if setup in ('rollback-edit','rollback-bridge','rollback-created-edit'): equal(stat.S_IMODE(hf.stat().st_mode),0o600)
                if case['id'] == 'install-twice':
                    first = [(p,os.lstat(p).st_ino) for p in (cf,hf)]
                    outcome2,plan2=invoke(binary,['install','plan'],spec); equal(outcome2,'planned')
                    second=root/'second-command'; second.mkdir(mode=0o700)
                    outcome2,result2=invoke(binary,['install','apply'],dict(plan=plan2,command_dir=str(second),writers_quiesced=True))
                    equal(outcome2,'installed'); equal([e['phase'] for e in result2['entries']],['unchanged','unchanged'])
                    equal([(str(p),os.lstat(p).st_ino) for p in (cf,hf)],[(str(p),ino) for p,ino in first])
            equal(outcome,case['expected']['outcome'])
            if case['expected']['phases']:
                equal([e['phase'] for e in result['entries']],case['expected']['phases'])
            if outcome in ('installed','recovered','partial','recovery_required'):
                equal(os.readlink(cf) if cf.is_symlink() else None,replace_tokens(case['expected']['codex_target'],root))
                equal(hf.read_bytes().decode() if hf.is_file() else None,replace_tokens(case['expected']['claude_text'],root))
        for p,b in preserved.items(): equal(p.read_bytes(),b)
        equal((root/'fake-hook.executed').exists(),False)
        # No output/temp files in source; no canonical mutation except explicit race setup.
        if setup not in ('source-link','source-edit'): equal(canonical.read_bytes(),b'# Disposable guidance\n')
        print(f"PASS {case['id']}")


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--self-check',action='store_true')
    parser.add_argument('--binary',type=Path)
    parser.add_argument('--scratch-parent',type=Path)
    args=parser.parse_args(); data=load(); self_check(data)
    if args.self_check: return
    if args.binary is None or args.scratch_parent is None: parser.error('explicit binary and existing private scratch-parent required')
    parent=args.scratch_parent.resolve(strict=True)
    equal(stat.S_IMODE(parent.stat().st_mode),0o700)
    for case in data['cases']: run_case(args.binary.resolve(strict=True),parent,case)
    print(f"S03 compiled CLI effect verification: {len(data['cases'])} cases passed")


if __name__ == '__main__': main()
