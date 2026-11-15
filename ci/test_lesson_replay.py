"""Tests of evals/lesson/replay on invented records and an invented store (synthetic values only).

I1 no value leaves (a marker in every name, id, text, key and handle appears in no output stream, on the ok
path and on every failure path); I2 each outcome and each word by one invented call, a forked record counts
once, a call outside the asked days or the lesson directory is not counted; I3 `saved_later` needs a save
after the call, of the same student and date; I4 the store's bytes do not change; I5 the store is built with
the `ont` that `check` found or built (ONT_BIN).
Run (by `check`): ONT_BIN=<ont> python3 -I -B -m unittest discover -s ci -p 'test_lesson_replay.py'
"""
import hashlib
import json
import os
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE.parent / "evals" / "lesson" / "replay"
SCHEMA = HERE / "fixtures" / "lesson-replay" / "schema.json"
ONT = os.environ.get("ONT_BIN", "")
GET = "mcp__academy_operations__academy_dailyrecord_get"
SAVE = "mcp__academy_operations__academy_dailyrecord_save"
MARK = "LEAKMARK"
PREFIX_TEXT = "mcp__academy_operations__academy_dailyrecord_get"  # a line holding it is parsed
LESSON = "/lesson"
FIELDS = {"attendance": "attendance", "dailyTestAssessment": "daily_test_assessment", "homework": "homework",
          "lessonMemo": "lesson_memo", "previousHomeworkAssessment": "previous_homework_assessment", "progress": "progress"}
DAY, DAY2, PREV = "2026-11-12", "2026-11-13", "2026-11-05"
WORDS = ("equal", "equal_after_whitespace", "store_cut", "store_missing", "differs")


def base(i, date=DAY):
    """The members of the store's object of student i, and the recorded answer's values: equal by default."""
    m = {"lesson_date": date, "student_name": f"{MARK}-name-{i}", "student_id": f"{MARK}-sid-{i}", "membership_key": f"{MARK}-mem-{i}",
         "record_key": f"{MARK}-rec-{i}-{date}", "previous_record_key": f"{MARK}-rec-{i}-{PREV}", "cut_fields": 0}
    for f in FIELDS.values():
        m[f] = f"{MARK} text {f} {i}\nsecond line"
    return m


def answer(m, status="observed", **over):
    """What a `dailyrecord_get` answered, from a store object's members."""
    if status != "observed":
        return {"status": status}
    fields = []
    for f, member in FIELDS.items():
        rec = m["previous_record_key"] if f == "previousHomeworkAssessment" else m["record_key"]
        fields.append({"field": f, "before": {"value": m.get(member, "")}, "target": {
            "studentId": m["student_id"], "membershipId": m["membership_key"], "recordId": rec, "courseId": "c",
            "lessonDate": PREV if f == "previousHomeworkAssessment" else m["lesson_date"]}})
    out = {"status": "observed", "lessonDate": m["lesson_date"], "authoringHandle": {"observationHandle": f"{MARK}-h-{m['student_id']}-{m['lesson_date']}"}, "fields": fields}
    out.update(over)
    return out


class Lines:
    """Builds session records; a call is an assistant line, its result a user line."""
    def __init__(self):
        self.n, self.lines = 0, []

    def call(self, name, inp, result, ts=f"{DAY}T03:00:00.000Z", cwd=LESSON + "/a", call_id=None):
        self.n += 1
        cid = call_id or f"toolu_{self.n}"
        self.lines.append({"type": "assistant", "cwd": cwd, "timestamp": ts, "message": {"content": [{"type": "tool_use", "id": cid, "name": name, "input": inp}]}})
        if result is not None:
            text = json.dumps(result)
            self.lines.append({"type": "user", "cwd": cwd, "timestamp": ts, "message": {"content": [{"type": "tool_result", "tool_use_id": cid, "content": [{"type": "text", "text": text}]}]}})
        return cid

    def get(self, m, ans=None, **kw):
        return self.call(GET, {"displayName": m["student_name"]}, ans if ans is not None else answer(m), **kw)

    def text(self):
        return "".join(json.dumps(x) + "\n" for x in self.lines)


class Replay(unittest.TestCase):
    def setUp(self):
        self.assertTrue(ONT, "ONT_BIN names the engine `check` found or built")
        self.tmp = Path(tempfile.mkdtemp(prefix="lesson-replay-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.store = self.tmp / "store.sqlite"
        self.sessions = self.tmp / "sessions"
        self.sessions.mkdir()
        self.install(SCHEMA)
        self.n_put = 0

    def ont(self, *args, stdin=None):
        p = subprocess.run([ONT, "--store", str(self.store), *args], input=stdin, capture_output=True, text=True, timeout=120, check=False)
        self.assertEqual(p.returncode, 0, (args, p.stdout[:200]))
        return json.loads(p.stdout)

    def install(self, schema):
        self.ont("schema", "install", str(schema))

    def put(self, m):
        self.n_put += 1
        self.ont("action", "apply", stdin=json.dumps({"action_type": "put", "request_id": f"r{self.n_put}", "parameters": m}))

    def write(self, lines, name="s1.jsonl"):
        (self.sessions / name).write_text(lines.text() if isinstance(lines, Lines) else lines)

    def run_script(self, *args, days=(DAY,), ont=None, store=None, sessions=True):
        cmd = [sys.executable, "-I", "-B", str(SCRIPT), "--store", str(store or self.store), "--lesson-cwd", LESSON, "--ont", ont or ONT]
        for d in days:
            cmd += ["--day", d]
        if sessions:
            cmd += ["--sessions", str(self.sessions / "**" / "*.jsonl")]
        p = subprocess.run(cmd + list(args), capture_output=True, text=True, timeout=300, check=False)
        return p

    def ok(self, *args, **kw):
        p = self.run_script(*args, **kw)
        self.assertEqual((p.returncode, p.stderr), (0, ""), p.stdout)
        self.assertEqual(p.stdout.count("\n"), 1)
        self.assertNotIn(MARK, p.stdout)
        out = json.loads(p.stdout)
        self.assertEqual(out["status"], "ok")
        return out

    def fake_ont(self, body):
        path = self.tmp / "fake-ont"
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return str(path)

    def make(self, L, i, store=None, drop=(), rec=None, ans=None, put=True, date=DAY, **kw):
        """Store object and recorded call of student i: equal unless `store` / `drop` / `rec` say otherwise."""
        m = base(i, date)
        if put:
            self.put({**{k: v for k, v in m.items() if k not in drop}, **(store or {})})
        L.get(m, ans if ans is not None else answer({**m, **(rec or {})}), **kw)
        return m

    def squeeze(self, i):
        return {f: base(i)[f].replace("\n", "  ") for f in FIELDS.values()}  # the same text with another white space

    # --- I2: every outcome and every word

    def test_i2_each_outcome_and_word(self):
        L = Lines()
        self.make(L, 1)  # all equal
        self.make(L, 2, store={"attendance": "a b c", "homework": "cut\u2026", "cut_fields": 1, "progress": "x", "previous_record_key": "zzz"},
                  drop=("lesson_memo", "record_key"), rec={"attendance": "a\n b   c\t", "homework": "cut more", "progress": "y"})
        self.make(L, 3, drop=("previous_record_key",), rec=self.squeeze(3))
        self.make(L, 4, store={"record_key": "another"})
        self.make(L, 5, put=False, ans=answer(base(5), "unavailable"))
        self.make(L, 6, put=False, ans=dict(answer(base(6)), fields=answer(base(6))["fields"][:5]))
        self.make(L, 7, put=False)  # no object
        self.put(dict(base(8), membership_key="m-a"))
        self.make(L, 8, store={"membership_key": "m-b"})  # two objects of the name, neither of the asked membership
        self.put(dict(base(9), membership_key="m-a", student_id="elsewhere"))
        self.make(L, 9)  # two objects of the name: the membership leaves one
        self.make(L, 10, store={"student_id": "someone-else"})
        self.write(L)
        out = self.ok()
        self.assertEqual((out["days"], out["calls"]), (1, 10))
        self.assertEqual(out["outcomes"], {"unavailable": 1, "malformed": 1, "saved_later": 0, "no_object": 1, "ambiguous": 1, "identity_differs": 1, "compared": 5})
        self.assertEqual((out["answered_equal"], out["k3_equal"], out["k3_of"]), (3, 3, 10))
        word = lambda **n: {w: n.get(w, 0) for w in WORDS}
        self.assertEqual(out["fields"], {
            "attendance": word(equal=3, equal_after_whitespace=2), "dailyTestAssessment": word(equal=4, equal_after_whitespace=1),
            "homework": word(equal=3, equal_after_whitespace=1, store_cut=1), "lessonMemo": word(equal=3, equal_after_whitespace=1, store_missing=1),
            "previousHomeworkAssessment": word(equal=4, equal_after_whitespace=1), "progress": word(equal=3, equal_after_whitespace=1, differs=1)})
        self.assertEqual(out["keys"], {"record_key": {"key_equal": 3, "key_missing": 1, "key_differs": 1},
                                       "previous_record_key": {"key_equal": 3, "key_missing": 1, "key_differs": 1}})

    def test_cut_marker_is_a_parameter(self):
        L = Lines()
        self.make(L, 1, store={"progress": "cut~~", "cut_fields": 2}, rec={"progress": "cut more"})
        self.make(L, 2, store={"progress": "cut~~", "cut_fields": 0}, rec={"progress": "cut more"})  # nothing was cut: differs
        self.make(L, 3, store={"progress": "cut more~~", "cut_fields": 1}, rec={"progress": "cut more"})  # not a proper prefix
        self.write(L)
        out = self.ok("--cut-marker", "~~")
        self.assertEqual(out["fields"]["progress"], {"equal": 0, "equal_after_whitespace": 0, "store_cut": 1, "store_missing": 0, "differs": 2})

    def test_i2_forked_record_boundaries_and_directory(self):
        L = Lines()
        self.make(L, 1, call_id="toolu_fork")
        fork = L.text()
        self.make(L, 2, ts="2026-11-11T15:00:00.000Z")  # 00:00 in Seoul: counted
        self.make(L, 3, ts="2026-11-11T14:59:59.000Z")  # the day before
        self.make(L, 4, ts="2026-11-12T15:00:00.000Z")  # the day after
        self.make(L, 5, cwd=LESSON)
        self.make(L, 6, cwd=LESSON + "/deep/er")
        self.make(L, 7, cwd=LESSON + "-other/x")
        self.make(L, 8, cwd="/elsewhere")
        L.call("mcp__academy_operations__academy_other", {"displayName": "x"}, answer(base(9)))
        self.write(L, "a.jsonl")
        self.write(fork.split("\n", 2)[0] + "\n" + fork.split("\n", 2)[1] + "\n", "b.jsonl")  # a forked session repeats the lines
        out = self.ok()
        self.assertEqual(out["calls"], 4)
        self.assertEqual(out["outcomes"]["compared"], 4)

    # --- I3: saved_later

    def test_i3_saved_later_needs_a_later_save_of_the_same_student_and_date(self):
        L = Lines()
        sv = lambda handle, ts, status="saved": L.call(SAVE, {"observationHandle": handle, "displayName": "x"}, {"status": status}, ts=ts)
        handle = lambda i, date=DAY: answer(base(i, date))["authoringHandle"]["observationHandle"]
        self.make(L, 1)
        sv(handle(1), f"{DAY}T05:00:00.000Z")  # after: saved_later
        self.make(L, 2)
        sv(handle(2), f"{DAY}T02:00:00.000Z")  # before the call
        self.make(L, 3)
        self.make(L, 4)
        sv(handle(4), f"{DAY}T05:00:00.000Z")  # the save of student 4 does not touch student 3
        self.make(L, 5)
        self.make(L, 5, date=DAY2, ts=f"{DAY}T04:00:00.000Z")  # another date, asked on the same day
        sv(handle(5, DAY2), f"{DAY}T05:30:00.000Z")
        self.make(L, 6)
        sv(handle(6), f"{DAY}T05:00:00.000Z", "refused")
        self.make(L, 7, ts=f"{DAY}T14:00:00.000Z")
        sv(handle(7), f"{DAY}T16:00:00.000Z")  # after midnight in Seoul: another day
        self.write(L)
        out = self.ok(days=(DAY, DAY2))
        self.assertEqual((out["days"], out["calls"]), (2, 8))
        self.assertEqual(out["outcomes"]["saved_later"], 3)  # students 1 and 4, and 5 on DAY2
        self.assertEqual(out["outcomes"]["compared"], 5)

    # --- the check inside the store

    def test_store_internal_previous_fields(self):
        def pair(i, day=None, prev=None, drop=()):
            d = dict(base(i), previous_progress="pp", previous_homework="hh", previous_lesson_memo="mm")
            self.put({**{k: v for k, v in d.items() if k not in drop}, **(day or {})})
            self.put({**base(i, PREV), "progress": "pp", "homework": "hh", "lesson_memo": "mm", **(prev or {})})
        pair(1, day={"previous_homework": "h h"}, prev={"homework": "h  h"})
        pair(2, day={"previous_progress": "cut~~", "cut_fields": 1}, prev={"progress": "cut more"})
        pair(3, prev={"student_id": "other"})  # not the same student: no pair
        pair(4, day={"previous_record_key": "nowhere"})  # the previous key leads to no object
        pair(5, day={"previous_progress": "p1", "previous_homework": "other"}, drop=("previous_lesson_memo",))
        out = self.ok("--cut-marker", "~~")
        self.assertEqual(out["calls"], 0)
        word = lambda **n: {w: n.get(w, 0) for w in WORDS}
        self.assertEqual(out["store"], {"pairs": 3, "members": {
            "progress": word(equal=1, store_cut=1, differs=1),
            "homework": word(equal=1, equal_after_whitespace=1, differs=1),
            "lesson_memo": word(equal=2, store_missing=1)}})

    # --- I4: the store is not written

    def test_i4_store_bytes_do_not_change(self):
        L = Lines()
        self.make(L, 1)
        self.write(L)
        digest = lambda: sorted((p.name, hashlib.sha256(p.read_bytes()).hexdigest()) for p in self.tmp.glob("store.sqlite*") if p.is_file()) + \
            [("log", hashlib.sha256((self.tmp / "edits.jsonl").read_bytes()).hexdigest())]
        before = digest()
        self.ok()
        self.assertEqual(digest(), before)
        self.run_script(days=("bad",))
        self.assertEqual(digest(), before)

    # --- I1 and point 9: no value leaves, on every failure path

    def fails(self, p, code, extra=None):
        self.assertEqual((p.returncode, p.stderr), (2, ""), p.stdout)
        self.assertEqual(p.stdout.count("\n"), 1)
        self.assertNotIn(MARK, p.stdout)
        got = json.loads(p.stdout)
        self.assertEqual(got, {"status": code, **(extra or {})})

    def test_i1_failure_codes_and_no_marker(self):
        L = Lines()
        self.make(L, 1)
        self.write(L)
        self.fails(self.run_script(days=("2026-13-45",)), "day_invalid")
        self.fails(self.run_script(days=("20261112",)), "day_invalid")
        self.fails(self.run_script(days=()), "bad_arguments")
        self.fails(self.run_script("--no-such-option"), "bad_arguments")
        self.fails(self.run_script("--client", "x"), "bad_arguments")  # the engine refuses the flag on reads: the option is not offered
        self.fails(self.run_script(store=self.tmp / "absent.sqlite"), "store_unreadable")
        self.fails(self.run_script(ont=str(self.tmp / "no-such-ont")), "ont_not_found")
        self.fails(self.run_script(ont=self.fake_ont(f"echo {MARK}; echo {MARK} >&2; exit 1")), "ont_failed")
        self.fails(self.run_script(ont=self.fake_ont(f"echo {MARK}")), "ont_failed")
        self.fails(self.run_script(ont=self.fake_ont(f"echo '{{\"result\":{{\"object_types\":[]}},\"m\":\"{MARK}\"}}'; exit 1")), "ont_failed")
        self.fails(self.run_script(ont=self.fake_ont(f"echo '\"{MARK}\"'")), "internal_error", {"exception": "TypeError"})
        self.fails(self.run_script(ont=self.fake_ont(f'case "$*" in *describe*) exec {ONT} "$@";; *) echo {MARK}; echo {MARK} >&2; exit 1;; esac')), "ont_failed")

    # --- an interrupt, a closed standard output, records and answers of an unexpected type

    def start(self, ont, *args):
        cmd = [sys.executable, "-I", "-B", str(SCRIPT), "--store", str(self.store), "--day", DAY, "--lesson-cwd", LESSON, "--ont", ont,
               "--sessions", str(self.sessions / "*.jsonl"), *args]
        return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def wait_for(self, path, proc):
        for _ in range(600):
            if path.exists() or proc.poll() is not None:
                return
            time.sleep(0.05)
        self.fail("the script did not reach the engine")

    def slow_ont(self, first_only=False):
        """An engine that announces itself and then waits (`exec`, so that it ends when it is killed)."""
        ready = self.tmp / "ready"
        wait = f"if [ ! -e {ready} ]; then touch {ready}; sleep 1; fi; exec {ONT} \"$@\"" if first_only else f"touch {ready}; exec sleep 60"
        return ready, self.fake_ont(wait)

    def test_interrupt_during_an_engine_call_ends_with_one_line(self):
        self.write(Lines())
        ready, ont = self.slow_ont()
        proc = self.start(ont)
        self.wait_for(ready, proc)
        proc.send_signal(signal.SIGINT)
        out, err = proc.communicate(timeout=60)
        self.assertEqual((proc.returncode, err, out), (2, "", '{"status": "interrupted"}\n'))

    def test_interrupt_while_a_large_record_is_read(self):
        big = json.dumps({"type": "assistant", "cwd": LESSON, "timestamp": f"{DAY}T03:00:00Z", "note": MARK + "x" * 64_000_000, "n": PREFIX_TEXT})
        (self.sessions / "big.jsonl").write_text(big + "\n")
        proc = self.start(ONT)
        for _ in range(600):  # as soon as the script has the record open
            if proc.poll() is not None or any(str(self.sessions / "big.jsonl") == os.path.realpath(f"/proc/{proc.pid}/fd/{fd}") for fd in os.listdir(f"/proc/{proc.pid}/fd")):
                break
            time.sleep(0.005)
        proc.send_signal(signal.SIGINT)
        out, err = proc.communicate(timeout=120)
        self.assertEqual(err, "")  # the interrupt may come after the last read: then the run is a complete one
        self.assertIn((proc.returncode, json.loads(out)["status"]), ((2, "interrupted"), (0, "ok")))
        self.assertNotIn(MARK, out)

    def test_closed_standard_output_ends_with_code_2_and_nothing_on_standard_error(self):
        L = Lines()
        self.make(L, 1)
        self.write(L)
        ready, ont = self.slow_ont(first_only=True)
        proc = self.start(ont)
        self.wait_for(ready, proc)
        proc.stdout.close()
        err = proc.stderr.read()
        proc.stderr.close()
        proc.wait(timeout=60)
        self.assertEqual((proc.returncode, err), (2, ""))

    def test_a_record_or_an_answer_of_an_unexpected_type_never_ends_the_run(self):
        L = Lines()
        self.make(L, 1)  # one good call
        self.write(f'["{GET}"]\n"{GET}"\n5\nnull\n' + L.text())
        L = Lines()
        L.n = 100  # other call ids than the first file's
        self.make(L, 2)
        L.lines[-1]["message"]["content"][0]["content"][0]["text"] = 5  # the result's text is a number
        listed = answer(base(3))
        for f in listed["fields"]:
            f["target"]["studentId"] = ["a", "list"]  # an unhashable id
        self.make(L, 3, ans=listed)
        handle = answer(base(4))
        handle["authoringHandle"]["observationHandle"] = {"a": 1}
        self.make(L, 4, ans=handle)
        tag = answer(base(5))
        tag["fields"][0]["field"] = ["a", "list"]  # an unhashable field name
        self.make(L, 5, ans=tag)
        twice = answer(base(6))
        twice["fields"].append(dict(twice["fields"][0]))  # one field named twice
        self.make(L, 6, ans=twice)
        self.write(L, "b.jsonl")
        out = self.ok()
        self.assertEqual(out["calls"], 6)
        self.assertEqual(out["outcomes"]["compared"], 1)
        self.assertEqual(out["outcomes"]["malformed"], 5)
        self.assertEqual(set(out), {"status", "days", "calls", "outcomes", "answered_equal", "fields", "keys", "store", "k3_equal", "k3_of"})

    def test_point7_schema_without_members_ends_before_any_read(self):
        schema = json.loads(SCHEMA.read_text())
        for name, edit in (("member", lambda s: s["object_types"]["lesson_record"]["properties"].pop("previous_lesson_memo")),
                           ("cut", lambda s: s["object_types"]["lesson_record"]["properties"].pop("cut_fields")),
                           ("type", lambda s: s["object_types"].update(other=s["object_types"].pop("lesson_record")))):
            s = json.loads(json.dumps(schema))
            edit(s)
            for gone in {*schema["object_types"]["lesson_record"]["properties"]} - {*next(iter(s["object_types"].values()))["properties"]}:
                del s["action_types"]["put"]["parameters"][gone], s["action_types"]["put"]["rules"][0]["set"][gone]
            if name == "type":
                s["action_types"]["put"]["rules"][0]["create_object"] = "other"
                s["grants"][0]["read"] = ["other"]
            store = self.tmp / f"{name}.sqlite"
            path = self.tmp / f"{name}.json"
            path.write_text(json.dumps(s))
            subprocess.run([ONT, "--store", str(store), "schema", "install", str(path)], capture_output=True, check=True)
            log = self.tmp / "calls.log"
            wrapper = self.fake_ont(f'echo "$*" >> {log}; exec {ONT} "$@"')
            self.fails(self.run_script(store=store, ont=wrapper), "schema_missing_members")
            self.assertTrue(all(line.endswith(" describe") for line in log.read_text().splitlines()))  # no search before the check
            log.unlink()


if __name__ == "__main__":
    unittest.main()
