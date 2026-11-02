#!/usr/bin/env python3
"""Additive commands have their own literal transcript contract, separate from S02."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

HERE=Path(__file__).resolve().parent
DATA=HERE/'commands-v1.json'
DIGEST_FILE=HERE/'commands-pins-v1.json'


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--self-check',action='store_true')
    args=parser.parse_args();pins=json.loads(DIGEST_FILE.read_text())
    for name,digest in pins.items():
        assert hashlib.sha256((HERE/name).read_bytes()).hexdigest()==digest
    cases=json.loads(DATA.read_text())['cases']
    assert len(cases)==17 and len({c['id'] for c in cases})==17
    sample=(0,b'help\n',b'')
    for mutant in [(3,b'help\n',b''),(0,b'help',b''),(0,b'help\n',b'noise'),(0,b'noise\nhelp\n',b'')]:
        assert mutant != sample
    print('S03 additive CLI: 17 literals; 4 transcript controls rejected')
    if args.self_check:return
    if args.binary is None:parser.error('explicit binary required')
    binary=args.binary.resolve(strict=True)
    for case in cases:
        p=subprocess.run([str(binary),*case['args']],input=case['stdin'].encode(),capture_output=True,timeout=5,check=False)
        e=case['expected'];assert (p.returncode,p.stdout,p.stderr)==(e['exit'],e['stdout'].encode(),e['stderr'].encode()),case['id']
    print('17 compiled additive CLI transcripts passed')


if __name__=='__main__':main()
