#!/usr/bin/env python3
"""Breadth-first native-store walk: WRAPPER SCRATCH [--records 2|3] [--depth N].

Document ids equal record ids; hashes use one fixed valid representative. They
are not read by any transition criterion, so states abstract only status/ref.
Each edge runs on a complete copy of its source store. Snapshots retain the
first (shortest) history for each state; fresh request ids avoid replay effects.
"""
import argparse
from collections import deque
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

MAX_STATES = 2000
MAX_CALLS = 100000
MAX_SECONDS = 1150  # Below the packet's 20-minute ceiling.
CALL_TIMEOUT = 20
BODY_HASH = "0" * 64
NAMESPACE = {"instance": "vps://palantirkc", "account": "root"}


class Stop(Exception):
    pass


class Walk:
    def __init__(self, args):
        self.args = args
        self.started = time.monotonic()
        args.scratch.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.root = Path(tempfile.mkdtemp(prefix="memory-walk-", dir=args.scratch))
        (self.root / "states").mkdir(mode=0o700)
        self.keys = ["walk-" + chr(ord("a") + i) for i in range(args.records)]
        self.calls = self.applied = self.refused = self.attempts = 0
        self.states, self.parents, self.depths, self.edges = [], [], [], []
        self.seen = {}
        self.failure = None
        self.i4_checked = False
        self.frontier_witnesses = 0

    def append(self, name, value):
        with (self.root / name).open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(value, sort_keys=True) + "\n")

    def limit(self):
        if time.monotonic() - self.started >= MAX_SECONDS:
            raise Stop(f"walk_bound: wall-clock limit {MAX_SECONDS}s reached")
        if self.calls >= MAX_CALLS:
            raise Stop(f"walk_bound: maximum native calls {MAX_CALLS} reached")

    def call(self, folder, argv, request=None):
        self.limit()
        self.calls += 1
        command = [str(self.args.wrapper), "--store", str(folder / "store.sqlite"), *argv]
        try:
            result = subprocess.run(command, input=json.dumps(request) if request is not None else None,
                                    text=True, capture_output=True,
                                    timeout=min(CALL_TIMEOUT, MAX_SECONDS - (time.monotonic() - self.started)))
        except subprocess.TimeoutExpired:
            raise Stop("walk_bound: native call timeout") from None
        try:
            answer = json.loads(result.stdout)
        except ValueError:
            raise Stop("walk_engine_error: non-JSON native answer") from None
        self.append("calls.jsonl", dict(call=self.calls, store=str(folder / "store.sqlite"),
                                       argv=argv, request=request, exit=result.returncode, answer=answer))
        if result.returncode not in (0, 2) or answer.get("status") not in ("ok", "refused"):
            raise Stop("walk_engine_error: " + str(answer.get("code", result.returncode)))
        return result.returncode, answer.get("result", answer)

    def oid(self, index):
        return dict(object_type="memory_record", namespace=NAMESPACE, native_key=self.keys[index])

    def actions(self):
        for record in range(self.args.records):
            for predecessor in [None, *range(self.args.records)]:
                params = dict(record=self.oid(record), document_id=self.keys[record], body_hash=BODY_HASH)
                if predecessor is not None:
                    params["supersedes"] = self.oid(predecessor)
                yield dict(action_type="mark_memory_record_pending", parameters=params)
            yield dict(action_type="mark_memory_record_ready", parameters=dict(record=self.oid(record)))
            for successor in range(self.args.records):
                yield dict(action_type="mark_memory_record_superseded",
                           parameters=dict(record=self.oid(record), successor=self.oid(successor)))
            yield dict(action_type="withdraw_memory_record", parameters=dict(record=self.oid(record)))

    def read_state(self, folder):
        code, result = self.call(folder, ["search"], dict(object_type="memory_record"))
        if code or not result["basis"]["qualified"] or result.get("continuation"):
            raise Stop("walk_engine_error: incomplete state read")
        values = [(None, None)] * self.args.records
        for obj in result["objects"]:
            key = obj["id"]["native_key"]
            if key not in self.keys:
                raise Stop("walk_engine_error: unknown record")
            fields = {name: item.get("value") for name, item in obj["head"]["fields"].items()}
            if fields["document_id"] != key or fields["body_hash"] != BODY_HASH:
                raise Stop("walk_abstraction_error: document/hash representative changed")
            status = fields["status"]
            if status not in ("pending", "ready", "superseded"):
                raise Stop("walk_engine_error: unknown status")
            ref = fields.get("supersedes")
            predecessor = None if ref is None else self.keys.index(ref["native_key"])
            if ref is not None and ref != self.oid(predecessor):
                raise Stop("walk_engine_error: reference namespace/type changed")
            values[self.keys.index(key)] = (status, predecessor)
        return tuple(values)

    def check_view(self, folder, state, sequence):
        code, result = self.call(folder, ["render"], dict(view="current_memory_records", args={}))
        wanted = {self.keys[i]: (self.keys[i], BODY_HASH, status)
                  for i, (status, _) in enumerate(state) if status in ("pending", "ready")}
        got = {}
        for line in result.get("markdown", "").split("\n"):
            if line.startswith("| walk-"):
                cells = [cell.strip() for cell in line.strip("|").split("|")]
                if len(cells) != 4 or cells[0] in got:
                    self.fail("I5", sequence)
                got[cells[0]] = tuple(cells[1:])
        if code or result.get("truncated") or result.get("rows") != len(got) or got != wanted:
            self.fail("I5", sequence)

    def sequence(self, node):
        steps = []
        while self.parents[node] is not None:
            node, action = self.parents[node]
            steps.append(action)
        return list(reversed(steps))

    def fail(self, invariant, sequence):
        self.failure = dict(invariant=invariant, sequence=sequence)
        raise Stop("walk_invariant: " + invariant + "; shortest sequence=" + json.dumps(sequence, sort_keys=True))

    def check_transition(self, before, after, sequence):
        for i, (status, predecessor) in enumerate(after):
            if before[i][0] == "superseded" and status != "superseded":
                self.fail("I1", sequence)
            if status == "ready" and predecessor is not None and after[predecessor][0] == "ready":
                self.fail("I2", sequence)
            if status in ("pending", "ready") and predecessor == i:
                self.fail("I3", sequence)

    def contract(self, state, action):
        """Admission and resulting fields from A1, independent of schema criteria."""
        params = action["parameters"]
        record = self.keys.index(params["record"]["native_key"])
        status, predecessor = state[record]
        kind = action["action_type"]
        after = list(state)
        if kind == "mark_memory_record_pending":
            if "supersedes" in params:
                predecessor = self.keys.index(params["supersedes"]["native_key"])
            admitted = status != "superseded" and predecessor != record
            after[record] = ("pending", predecessor)
        elif kind == "mark_memory_record_ready":
            admitted = (status == "pending"
                        and (predecessor is None or state[predecessor][0] != "ready")
                        and not any(s in ("pending", "ready") and p == record for s, p in state))
            after[record] = ("ready", predecessor)
        elif kind == "mark_memory_record_superseded":
            successor = self.keys.index(params["successor"]["native_key"])
            admitted = status in ("pending", "ready") and state[successor] == ("ready", record)
            after[record] = ("superseded", predecessor)
        else:
            admitted = status in ("pending", "ready")
            after[record] = ("superseded", predecessor)
        return admitted, tuple(after) if admitted else state

    def snapshot(self, state, folder, parent, action):
        if state in self.seen:
            return self.seen[state], False
        if len(self.states) >= MAX_STATES:
            raise Stop(f"walk_bound: maximum states {MAX_STATES} reached")
        node = len(self.states)
        target = self.root / "states" / f"{node:06}"
        shutil.copytree(folder, target)
        self.seen[state] = node
        self.states.append(state)
        self.parents.append(None if parent is None else (parent, action))
        self.depths.append(0 if parent is None else self.depths[parent] + 1)
        self.edges.append(set())
        self.append("states.jsonl", dict(state=node, depth=self.depths[node],
                    records={key: dict(status=status, supersedes=None if ref is None else self.keys[ref])
                             for key, (status, ref) in zip(self.keys, state, strict=True)}))
        return node, True

    @staticmethod
    def fingerprint(folder):
        return {str(path.relative_to(folder)): hashlib.sha256(path.read_bytes()).hexdigest()
                for path in folder.rglob("*") if path.is_file() and path.name in ("store.sqlite", "edits.jsonl")}

    def attempt(self, node, action, witness=False):
        self.limit()
        trial = self.root / "trial"
        if trial.exists():
            shutil.rmtree(trial)  # Only this disposable, just-tested edge copy.
        shutil.copytree(self.root / "states" / f"{node:06}", trial)
        before = self.fingerprint(trial)
        self.attempts += 1
        request = dict(action, request_id=f"walk-edge-{self.attempts}")
        code, answer = self.call(trial, ["action", "apply"], request)
        sequence = self.sequence(node) + [action]
        if code:
            self.refused += 1
            if before != self.fingerprint(trial):
                raise Stop("walk_engine_error: refused action changed store/log bytes")
            after, target, new = self.states[node], node, False
        else:
            if answer.get("outcome") != "applied":
                raise Stop("walk_engine_error: request replay instead of a new edge")
            self.applied += 1
            after = self.read_state(trial)
            target, new = (None, False) if witness else self.snapshot(after, trial, node, action)
        if not witness:
            self.edges[node].add(target)
        self.append("transitions.jsonl", dict(source=node, target=target, action=action,
                    outcome="refused" if code else "applied", witness=witness,
                    records={key: dict(status=status, supersedes=None if ref is None else self.keys[ref])
                             for key, (status, ref) in zip(self.keys, after, strict=True)}))
        self.check_transition(self.states[node], after, sequence)
        admitted, wanted = self.contract(self.states[node], action)
        if admitted != (code == 0) or after != wanted:
            self.failure = dict(contract=True, sequence=sequence, expected_applied=admitted)
            raise Stop("walk_contract: admission/result mismatch; shortest sequence=" + json.dumps(sequence, sort_keys=True))
        if not code:
            self.check_view(trial, after, sequence)
        return after, target, new

    def check_liveness(self, frontier):
        failures = []
        reverse = [set() for _ in self.states]
        for source, targets in enumerate(self.edges):
            for target in targets:
                reverse[target].add(source)
        for record in range(self.args.records):
            goals = {i for i, state in enumerate(self.states) if state[record][0] in ("ready", "superseded")}
            # A bounded-depth frontier is not expanded, so verify its escape by
            # an actual withdrawal on a copy rather than assuming liveness.
            for node in frontier:
                if self.states[node][record][0] == "pending":
                    action = dict(action_type="withdraw_memory_record", parameters=dict(record=self.oid(record)))
                    after, _, _ = self.attempt(node, action, witness=True)
                    self.frontier_witnesses += 1
                    if after[record][0] not in ("ready", "superseded"):
                        failures.append(node)
                    goals.add(node)
            pending = deque(goals)
            while pending:
                for predecessor in reverse[pending.popleft()] - goals:
                    goals.add(predecessor)
                    pending.append(predecessor)
            for node, state in enumerate(self.states):
                if state[record][0] == "pending" and node not in goals:
                    failures.append(node)
        if failures:
            self.fail("I4", self.sequence(min(failures, key=lambda node: (self.depths[node], node))))
        self.i4_checked = True

    def run(self):
        initial = self.root / "initial"
        initial.mkdir(mode=0o700)
        schema = Path(__file__).resolve().with_name("work.schema.json")
        definitions = json.loads(schema.read_text())
        criteria = json.dumps([value["submission_criteria"] for name, value in definitions["action_types"].items()
                               if name in {a["action_type"] for a in self.actions()}])
        if "body_hash" in criteria or "document_id" in criteria:
            raise Stop("walk_abstraction_error: hash/document affect transition admission")
        for argv in [["schema", "check", str(schema)], ["schema", "install", str(schema)]]:
            if self.call(initial, argv)[0]:
                raise Stop("walk_engine_error: schema check/install refused")
        first = self.read_state(initial)
        if first != ((None, None),) * self.args.records:
            raise Stop("walk_engine_error: initial index not empty")
        self.snapshot(first, initial, None, None)
        self.check_view(initial, first, [])
        pending, frontier = deque([0]), []
        while pending:
            node = pending.popleft()
            if self.args.depth is not None and self.depths[node] >= self.args.depth:
                frontier.append(node)
                continue
            for action in self.actions():
                _, target, new = self.attempt(node, action)
                if new:
                    pending.append(target)
        self.check_liveness(frontier)
        return not frontier

    def summary(self, complete, error):
        result = dict(records=self.args.records, states=len(self.states), applied=self.applied,
                      refused=self.refused, calls=self.calls, depth_limit=self.args.depth,
                      maximum_depth=max(self.depths, default=0), complete=complete,
                      i4_checked=self.i4_checked, frontier_witnesses=self.frontier_witnesses,
                      bounds=dict(states=MAX_STATES, calls=MAX_CALLS, seconds=MAX_SECONDS),
                      elapsed_seconds=round(time.monotonic() - self.started, 3),
                      exit=1 if error else 0, error=error, failure=self.failure, artifacts=str(self.root))
        (self.root / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
        print("memory index walk: " + json.dumps(result, sort_keys=True), flush=True)
        return result["exit"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wrapper", type=Path)
    parser.add_argument("scratch", type=Path)
    parser.add_argument("--records", type=int, choices=(2, 3), default=3)
    parser.add_argument("--depth", type=int)
    args = parser.parse_args()
    if args.depth is not None and args.depth < 0:
        parser.error("depth must be nonnegative")
    args.wrapper = args.wrapper.resolve(strict=True)
    args.scratch = args.scratch.resolve()
    walk = Walk(args)
    complete, error = False, None
    try:
        complete = walk.run()
    except Stop as exc:
        error = str(exc)
        print(error, file=sys.stderr, flush=True)
    return walk.summary(complete, error)


if __name__ == "__main__":
    sys.exit(main())
