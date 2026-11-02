#!/usr/bin/env python3
"""Frozen S02 CLI judge. --self-check never launches or builds a candidate."""

import argparse
import copy
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent


def wire(value):
    return (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def expected_transcript(expected):
    if "plan" in expected:
        return (0, wire(expected["plan"]), b"")
    error = expected["error"]
    return (3, b"", wire({"error": error}))


def compare(actual, expected):
    # Exact bytes include field types/order, array ordering, trailing LF and stderr.
    if actual != expected:
        raise AssertionError("candidate transcript differs from frozen expectation")


def load_corpora():
    corpus = json.loads((ROOT / "cases.json").read_text())
    cli = json.loads((ROOT / "cli-cases.json").read_text())
    if corpus["contract_version"] != 1 or cli["contract_version"] != 1:
        raise AssertionError("unsupported fixture version")
    ids = [case["id"] for case in corpus["cases"] + cli["cases"]]
    if len(set(ids)) != len(ids):
        raise AssertionError("duplicate fixture id")
    for case in corpus["cases"]:
        if set(case["expected"]) not in ({"plan"}, {"error"}):
            raise AssertionError("ambiguous fixture expectation")
        if "error" in case["expected"]:
            if case["expected"]["error"]["code"] == "renderer_unimplemented":
                raise AssertionError("stub cannot be an accepted expectation")
            continue
        plan = case["expected"]["plan"]
        if len(plan["artifacts"]) != 2:
            raise AssertionError("wrong fixture artifact count")
        paths = [entry["path"] for entry in plan["artifacts"]]
        if paths != sorted(paths):
            raise AssertionError("fixture artifacts not sorted")
        preserved = sorted(case["request"]["preserved"], key=lambda entry: entry["path"])
        if plan["preserved"] != preserved:
            raise AssertionError("fixture lost opaque bytes")
        for artifact in plan["artifacts"]:
            if artifact["changed"] != (artifact["before"] != artifact["after"]):
                raise AssertionError("fixture changed flag inconsistent")
    return corpus["cases"], cli["cases"]


def self_check(cases):
    expected = expected_transcript(next(case["expected"] for case in cases if "plan" in case["expected"]))
    compare(expected, expected)
    status, stdout, stderr = expected
    value = json.loads(stdout)
    changed = copy.deepcopy(value)
    changed["artifacts"][0]["after"]["text"] += "wrong bytes"
    missing = copy.deepcopy(value)
    del missing["artifacts"][0]["selector"]
    reordered = copy.deepcopy(value)
    reordered["artifacts"].reverse()
    wrong_type = copy.deepcopy(value)
    wrong_type["artifacts"][0]["changed"] = 0
    controls = [
        (status, wire(changed), stderr),
        (status, wire(missing), stderr),
        (status, wire(reordered), stderr),
        (status, wire(wrong_type), stderr),
        (7, stdout, stderr),
        (status, stdout + b"extra stdout\n", stderr),
        (status, stdout, b"unexpected stderr\n"),
        (status, stdout.rstrip(b"\n"), stderr),
        (70, b"", wire({"error": {"code": "renderer_unimplemented", "field": "renderer"}})),
    ]
    for mutant in controls:
        try:
            compare(mutant, expected)
        except AssertionError:
            continue
        raise AssertionError("judge accepted a negative control")
    # Refusal category, selector and exit must also be compared, not just success bytes.
    refusal = expected_transcript(next(case["expected"] for case in cases if "error" in case["expected"]))
    mutant = (refusal[0], refusal[1], wire({"error": {"code": "invalid_path", "field": "wrong"}}))
    try:
        compare(mutant, refusal)
    except AssertionError:
        pass
    else:
        raise AssertionError("judge ignored refusal identity")
    print(f"self-check: {len(controls) + 1} negative controls rejected")


def execute(binary, args, stdin, cwd):
    result = subprocess.run(
        [str(binary), *args], input=stdin, capture_output=True, cwd=cwd, timeout=5, check=False
    )
    return result.returncode, result.stdout, result.stderr


def reordered_objects(value):
    if isinstance(value, dict):
        return {key: reordered_objects(value[key]) for key in reversed(value)}
    if isinstance(value, list):
        return [reordered_objects(item) for item in value]
    return value


def check_binary(binary, cases, cli_cases):
    count = 0
    with tempfile.TemporaryDirectory(prefix="ap-s02-conformance-") as cwd:
        for case in cases:
            expected = expected_transcript(case["expected"])
            inputs = [wire(case["request"]), wire(case["request"]), wire(reordered_objects(case["request"]))]
            for stdin in inputs:
                try:
                    compare(execute(binary, ["render"], stdin, cwd), expected)
                except AssertionError as error:
                    raise AssertionError(f"{case['id']}: {error}") from error
                count += 1
        for case in cli_cases:
            stdin = case["stdin"].encode()
            if "stdin_hex" in case:
                stdin = bytes.fromhex(case["stdin_hex"])
            if "stdin_repeat" in case:
                stdin = (case["stdin_repeat"]["text"] * case["stdin_repeat"]["count"]).encode()
            expected = case["expected"]
            transcript = (expected["exit"], b"", wire({"error": {"code": expected["code"], "field": expected["field"]}}))
            try:
                compare(execute(binary, case["args"], stdin, cwd), transcript)
            except AssertionError as error:
                raise AssertionError(f"{case['id']}: {error}") from error
            count += 1
        help_bytes = b"Usage: ap render < request.json\nPure JSON proposal on stdout; no file access or installation.\n"
        for flag in ["--help", "-h"]:
            compare(execute(binary, [flag], b"", cwd), (0, help_bytes, b""))
            count += 1
    print(f"candidate: {count} exact CLI transcripts matched")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--self-check", action="store_true")
    group.add_argument("--binary", type=Path)
    args = parser.parse_args()
    cases, cli_cases = load_corpora()
    self_check(cases)
    print(f"frozen corpus: {len(cases)} renderer cases; {len(cli_cases)} CLI admission cases")
    if args.binary:
        check_binary(args.binary.resolve(strict=True), cases, cli_cases)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        # These messages contain only our fixed diagnostics and synthetic case IDs.
        print(f"conformance_failed@crates/ap/conformance/verify.py: {error}")
        raise SystemExit(1) from None
    except (OSError, subprocess.TimeoutExpired, ValueError) as error:
        # Fixtures are synthetic, but candidate stderr is still never echoed.
        print(f"conformance_failed@crates/ap/conformance/verify.py: {type(error).__name__}")
        raise SystemExit(1) from None
