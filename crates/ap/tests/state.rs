//! `ap state` against a fixture task directory made here: a Git repository whose `runs/` holds
//! records of the shapes of real ones (members copied, no session id). `systemctl` and the process
//! table are answered by the fake `System`; Git is the real one, in the temporary directory.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Failure, Host, Reply, System};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// The units `systemctl is-active` answers `active` for, and the rows `ps` prints (None: `ps` fails).
struct Fake {
    active: BTreeSet<String>,
    ps: Option<Vec<String>>,
    /// `systemctl` cannot reach its manager: exit 1, nothing on stdout.
    systemctl_fails: bool,
    /// What `systemctl` answers for a unit instead of its `active`/`inactive`: exit success, stdout.
    words: BTreeMap<String, (bool, String)>,
    /// `systemctl` cannot be started at all.
    systemctl_error: bool,
    /// What `ps` answers instead of its rows: exit success, stdout.
    ps_reply: Option<(bool, String)>,
    programs: Vec<(String, Vec<String>)>,
}
impl Fake {
    fn new() -> Self {
        Fake {
            active: BTreeSet::new(),
            ps: Some(vec!["/usr/lib/systemd/systemd --system".into()]),
            systemctl_fails: false,
            words: BTreeMap::new(),
            systemctl_error: false,
            ps_reply: None,
            programs: vec![],
        }
    }
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        self.programs.push((program.into(), args.to_vec()));
        let reply = |success: bool, text: String| {
            Ok(Reply {
                success,
                bytes: text.clone().into_bytes(),
                stdout: text.into_bytes(),
            })
        };
        match program {
            "systemctl" => {
                assert_eq!(args[0], "is-active");
                if self.systemctl_error {
                    return Err(ap_failure());
                }
                if self.systemctl_fails {
                    return reply(false, String::new());
                }
                if let Some((ok, word)) = self.words.get(args.last().unwrap()) {
                    return reply(*ok, word.clone());
                }
                match self.active.contains(args.last().unwrap()) {
                    true => reply(true, "active\n".into()),
                    false => reply(false, "inactive\n".into()),
                }
            }
            "ps" if self.ps_reply.is_some() => {
                let (ok, text) = self.ps_reply.clone().unwrap();
                reply(ok, text)
            }
            "ps" => match &self.ps {
                Some(rows) => reply(true, rows.join("\n") + "\n"),
                None => Err(ap_failure()),
            },
            "git" => Host.call(program, args, input, ms),
            other => panic!("ap state called {other}"),
        }
    }
}
fn ap_failure() -> Failure {
    Host.call("/missing-ap-fixture", &[], &Value::Null, 100)
        .err()
        .unwrap()
}

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let d = std::env::temp_dir().join(format!(
            "ap-state-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("runs")).unwrap();
        Dir(d)
    }
    fn put(&self, name: &str, text: &str) {
        std::fs::write(self.0.join("runs").join(name), text).unwrap();
    }
    fn json(&self, name: &str, v: &Value) {
        self.put(name, &v.to_string());
    }
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["-c", "user.name=t", "-c", "user.email=t@e"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
    fn arg(&self) -> String {
        self.0.to_str().unwrap().to_owned()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(f: &mut Fake, args: &[&str]) -> Result<String, Failure> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    ap::state::execute(f, &args)
}
fn state(f: &mut Fake, d: &Dir) -> Value {
    serde_json::from_str(&run(f, &[&d.arg()]).unwrap()).unwrap()
}

/// A stage record of the shape `ap run` writes (members of a real one, without `session`).
fn stage(role: &str, account: &str, start: &str, end: Option<&str>, unit: &str) -> Value {
    json!({"account": account, "answer": null, "cwd": "/w/wt/r", "effort": "high",
        "end": end, "expect_met": null, "model": "sonnet", "permission_denials": [],
        "prompt": {"path": "/w/p.md", "sha256": "00"}, "results": 0, "resumes": null,
        "role": role, "runtime": "claude", "start": start,
        "state": if end.is_some() { "ok" } else { "running" }, "turns": null,
        "unit": unit, "unmet": null, "compact_window": null})
}
/// A unit record of the shape `ap unit` writes.
fn unit(pr: Value, state: &str, start: &str, end: Option<&str>) -> Value {
    json!({"base": "b".repeat(40), "branch": "ap/unit-x", "cause": null, "detail": null,
        "end": end, "head": if pr.is_null() { Value::Null } else { json!("a".repeat(40)) },
        "merge": true, "merge_commit": if state == "merged" { json!("c".repeat(40)) } else { Value::Null },
        "mode": "unit", "pull_request": pr, "repository": "o/r", "start": start, "state": state,
        "step": null, "stopped_at": if state == "stopped" { json!("gate") } else { Value::Null },
        "stages": {"implementer": null, "verifier": null}, "steps": [], "task_key": "t",
        "write_set": ["crates/ap/**"]})
}
fn with_item(mut u: Value, key: &str) -> Value {
    u["item"] = json!({"key": key, "id": {"native_key": "k", "object_type": "item"},
        "objective": {"name": "n", "id": {"native_key": "o", "object_type": "objective"}}});
    u
}
fn limit(status: &str) -> Value {
    json!({"status": status, "kind": "five_hour", "resets_at": 1794805800,
        "windows": {"five_hour": {"resets_at": 1794805800, "status": status, "utilization": 1}}})
}

#[test]
fn sr1_task_head_and_uncommitted_lines() {
    // spec: specs/ap/state.md#R4
    let d = Dir::new();
    let mut f = Fake::new();
    let v = state(&mut f, &d);
    // Without Git: no head, and no answer of `git status` to count.
    assert_eq!(
        (v["task"]["head"].clone(), v["task"]["uncommitted"].clone()),
        (Value::Null, Value::Null)
    );
    let at = v["at"].as_str().unwrap();
    assert!(
        at.len() == 20 && at.ends_with('Z') && &at[10..11] == "T",
        "{at}"
    );
    assert_eq!(
        v["task"]["dir"],
        d.0.canonicalize().unwrap().to_str().unwrap()
    );
    d.git(&["init", "-q"]);
    d.json(
        "a.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T11:00:00Z",
            Some("2026-11-14T11:30:00Z"),
            "u1",
        ),
    );
    d.git(&["add", "-A"]);
    d.git(&["commit", "-qm", "a"]);
    std::fs::write(d.0.join("x"), "1").unwrap();
    std::fs::write(d.0.join("y"), "1").unwrap();
    let v = state(&mut f, &d);
    assert_eq!(v["task"]["head"], d.git(&["rev-parse", "HEAD"]));
    assert_eq!(v["task"]["uncommitted"], 2);
}

#[test]
fn sr1_units_are_chains_of_calls_newest_first() {
    // spec: specs/ap/state.md#R5
    let d = Dir::new();
    // One pull request with a call and its come-back; a call without a pull request; another
    // repository's pull request of the same number.
    d.json(
        "U-A.unit.json",
        &with_item(
            unit(
                json!(7),
                "stopped",
                "2026-11-14T10:00:00Z",
                Some("2026-11-14T10:30:00Z"),
            ),
            "item-a",
        ),
    );
    d.json(
        "U-A-2.unit.json",
        &unit(
            json!(7),
            "merged",
            "2026-11-14T12:00:00Z",
            Some("2026-11-14T12:20:00Z"),
        ),
    );
    d.json(
        "U-B.unit.json",
        &unit(
            Value::Null,
            "stopped",
            "2026-11-15T09:00:00Z",
            Some("2026-11-15T09:01:00Z"),
        ),
    );
    let mut other = unit(
        json!(7),
        "merged",
        "2026-11-13T09:00:00Z",
        Some("2026-11-13T09:30:00Z"),
    );
    other["repository"] = json!("o/s");
    d.json("U-C.unit.json", &other);
    let v = state(&mut Fake::new(), &d);
    let chains = v["units"].as_array().unwrap();
    assert_eq!(chains.len(), 3);
    // Newest first by the last call's start.
    assert_eq!(chains[0]["calls"][0]["record"], "runs/U-B.unit.json");
    assert_eq!(chains[1]["repository"], "o/r");
    assert_eq!(chains[2]["repository"], "o/s");
    // A call without a pull request is a chain of its own and has no item.
    assert_eq!(
        (chains[0]["pull_request"].clone(), chains[0]["item"].clone()),
        (Value::Null, Value::Null)
    );
    // The chain's item is the first record's that has one; calls are in order of start.
    let chain = &chains[1];
    assert_eq!(
        (chain["pull_request"].clone(), chain["item"].clone()),
        (json!(7), json!("item-a"))
    );
    let calls: Vec<_> = chain["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["record"].as_str().unwrap())
        .collect();
    assert_eq!(calls, ["runs/U-A.unit.json", "runs/U-A-2.unit.json"]);
    let last = &chain["calls"][1];
    assert_eq!(last["state"], "merged");
    assert_eq!(last["merge_commit"], "c".repeat(40));
    assert_eq!(last["head"], "a".repeat(40));
    assert_eq!(
        (last["start"].clone(), last["end"].clone()),
        (json!("2026-11-14T12:00:00Z"), json!("2026-11-14T12:20:00Z"))
    );
    assert_eq!(
        (
            last["stopped_at"].clone(),
            last["cause"].clone(),
            last["stale"].clone()
        ),
        (Value::Null, Value::Null, Value::Null)
    );
    let first = &chain["calls"][0];
    assert_eq!(
        (first["state"].clone(), first["stopped_at"].clone()),
        (json!("stopped"), json!("gate"))
    );
    let mut keys: Vec<_> = first.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "cause",
            "end",
            "head",
            "merge_commit",
            "record",
            "stale",
            "start",
            "state",
            "stopped_at"
        ]
    );
}

#[test]
fn sr1_stages_are_the_running_ones_and_the_newest_twenty_of_no_call() {
    // spec: specs/ap/state.md#R6
    let d = Dir::new();
    d.json(
        "U-A.unit.json",
        &unit(
            json!(1),
            "merged",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:30:00Z"),
        ),
    );
    // Owned by the call (its path less `.unit.json`, then `.`): left out unless it runs.
    d.json(
        "U-A.implementer.md.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:01:00Z",
            Some("2026-11-14T10:20:00Z"),
            "u",
        ),
    );
    // Not owned: the prefix is not followed by a dot, so it is listed, as the newest of no call.
    d.json(
        "U-A-2.fixer.md.run.json",
        &stage(
            "fixer",
            "/a",
            "2026-11-20T10:01:00Z",
            Some("2026-11-20T10:20:00Z"),
            "u",
        ),
    );
    for n in 0..25 {
        let start = format!("2026-11-02T10:{n:02}:00Z");
        d.json(
            &format!("s{n:02}.run.json"),
            &stage("reviewer", "/a", &start, Some("2026-11-02T11:00:00Z"), "u"),
        );
    }
    let mut running = stage(
        "implementer",
        "/b",
        "2026-10-01T10:00:00Z",
        None,
        "ap-run-1.scope",
    );
    running["served_by"] = json!("other-model");
    d.json("U-A.old.run.json", &running);
    let mut f = Fake::new();
    f.active.insert("ap-run-1.scope".into());
    let v = state(&mut f, &d);
    let stages = v["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 21);
    let names: Vec<_> = stages
        .iter()
        .map(|s| s["record"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"runs/U-A.old.run.json"), "{names:?}");
    assert!(!names.contains(&"runs/U-A.implementer.md.run.json"));
    // The newest twenty of no call by start: U-A-2 (not owned by U-A) and s24 .. s06.
    assert!(names.contains(&"runs/U-A-2.fixer.md.run.json"));
    assert!(names.contains(&"runs/s24.run.json") && names.contains(&"runs/s06.run.json"));
    assert!(!names.contains(&"runs/s05.run.json"));
    let r = stages
        .iter()
        .find(|s| s["record"] == "runs/U-A.old.run.json")
        .unwrap();
    assert_eq!(r["served_by"], "other-model");
    let mut keys: Vec<_> = r.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "account",
            "end",
            "model",
            "record",
            "role",
            "runtime",
            "served_by",
            "stale",
            "start",
            "state"
        ]
    );
    // A record without `served_by` has no such member.
    let s = stages
        .iter()
        .find(|s| s["record"] == "runs/s24.run.json")
        .unwrap();
    assert!(s.get("served_by").is_none());
    assert_eq!(
        (
            s["role"].clone(),
            s["runtime"].clone(),
            s["model"].clone(),
            s["account"].clone(),
            s["state"].clone()
        ),
        (
            json!("reviewer"),
            json!("claude"),
            json!("sonnet"),
            json!("/a"),
            json!("ok")
        )
    );
    // Newest first.
    assert_eq!(names[0], "runs/U-A-2.fixer.md.run.json");
}

#[test]
fn sr1_stale_of_each_kind_of_record() {
    // spec: specs/ap/state.md#R7
    let d = Dir::new();
    d.json(
        "live.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-live.scope",
        ),
    );
    d.json(
        "dead.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-dead.scope",
        ),
    );
    d.json(
        "done.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:10:00Z"),
            "ap-run-live.scope",
        ),
    );
    d.json(
        "U-live.unit.json",
        &unit(Value::Null, "running", "2026-11-14T10:00:00Z", None),
    );
    d.json(
        "U-live.implementer.md.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-live.scope",
        ),
    );
    d.json(
        "U-dead.unit.json",
        &unit(Value::Null, "running", "2026-11-14T11:00:00Z", None),
    );
    d.json(
        "U-dead.implementer.md.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T11:00:00Z",
            None,
            "ap-run-dead.scope",
        ),
    );
    d.json(
        "U-gone.unit.json",
        &unit(Value::Null, "running", "2026-11-14T12:00:00Z", None),
    );
    d.json(
        "U-merged.unit.json",
        &unit(
            Value::Null,
            "merged",
            "2026-11-14T13:00:00Z",
            Some("2026-11-14T13:10:00Z"),
        ),
    );
    let stale = |v: &Value| {
        let mut m = BTreeMap::new();
        for s in v["stages"].as_array().unwrap() {
            m.insert(s["record"].as_str().unwrap().to_owned(), s["stale"].clone());
        }
        for c in v["units"].as_array().unwrap() {
            for c in c["calls"].as_array().unwrap() {
                m.insert(c["record"].as_str().unwrap().to_owned(), c["stale"].clone());
            }
        }
        m
    };
    // No process of `ap unit` lives: a call that says running and has no live stage is stale.
    let mut f = Fake::new();
    f.active.insert("ap-run-live.scope".into());
    let m = stale(&state(&mut f, &d));
    assert_eq!(m["runs/live.run.json"], false);
    assert_eq!(m["runs/dead.run.json"], true);
    assert_eq!(m["runs/U-live.implementer.md.run.json"], false);
    assert_eq!(m["runs/U-dead.implementer.md.run.json"], true);
    assert_eq!(m["runs/U-live.unit.json"], false, "a live stage record");
    assert_eq!(m["runs/U-dead.unit.json"], true);
    assert_eq!(m["runs/U-gone.unit.json"], true);
    assert_eq!(m["runs/U-merged.unit.json"], Value::Null);
    assert_eq!(
        m["runs/done.run.json"],
        Value::Null,
        "a stage that ended is not stale"
    );
    // Some `ap unit` lives and nothing ties it to the record: null, whatever the path.
    f.ps = Some(vec!["/usr/local/bin/ap unit".into(), "bash".into()]);
    let m = stale(&state(&mut f, &d));
    assert_eq!(m["runs/U-live.unit.json"], false);
    assert_eq!(m["runs/U-dead.unit.json"], Value::Null);
    assert_eq!(m["runs/U-gone.unit.json"], Value::Null);
    assert_eq!(
        m["runs/dead.run.json"], true,
        "a stage is judged by its unit alone"
    );
    // A process of another verb, or a word of another program, is no `ap unit`.
    f.ps = Some(vec![
        "ap state /w".into(),
        "vim unit".into(),
        "/usr/bin/apt unit".into(),
    ]);
    assert_eq!(stale(&state(&mut f, &d))["runs/U-gone.unit.json"], true);
    // A process table that cannot be read says nothing.
    f.ps = None;
    assert_eq!(
        stale(&state(&mut f, &d))["runs/U-gone.unit.json"],
        Value::Null
    );
}

#[test]
fn sr1_streams_without_record() {
    // spec: specs/ap/state.md#R8
    let d = Dir::new();
    d.put("a.stream.jsonl", "{}\n");
    d.put("a.run.json", "{}");
    d.put("by-hand.stream.jsonl", "{}\n");
    d.put("ERRAND.report.json.stream.jsonl", "{}\n");
    d.json(
        "ERRAND.report.json.run.json",
        &stage(
            "reviewer",
            "/a",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:10:00Z"),
            "u",
        ),
    );
    let v = state(&mut Fake::new(), &d);
    let s = v["streams_without_record"].as_array().unwrap();
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0]["path"], "runs/by-hand.stream.jsonl");
    let modified = s[0]["modified"].as_str().unwrap();
    assert!(
        modified.len() == 20 && modified.ends_with('Z'),
        "{modified}"
    );
}

#[test]
fn sr1_limits_are_the_newest_present_limit_of_each_account() {
    // spec: specs/ap/state.md#R9
    let d = Dir::new();
    let with = |mut v: Value, l: Option<Value>| {
        if let Some(l) = l {
            v["limit"] = l;
        }
        v
    };
    // Account /a: an old limit, a newer one (by end), a newest record with `limit` null, and one
    // that ended later still without the member.
    d.json(
        "a1.run.json",
        &with(
            stage(
                "implementer",
                "/a",
                "2026-11-10T10:00:00Z",
                Some("2026-11-10T11:00:00Z"),
                "u",
            ),
            Some(limit("allowed")),
        ),
    );
    d.json(
        "a2.run.json",
        &with(
            stage(
                "implementer",
                "/a",
                "2026-11-09T10:00:00Z",
                Some("2026-11-12T11:00:00Z"),
                "u",
            ),
            Some(limit("rejected")),
        ),
    );
    d.json(
        "a3.run.json",
        &with(
            stage(
                "implementer",
                "/a",
                "2026-11-13T10:00:00Z",
                Some("2026-11-13T11:00:00Z"),
                "u",
            ),
            Some(Value::Null),
        ),
    );
    d.json(
        "a4.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T11:00:00Z"),
            "u",
        ),
    );
    // Account /b: the newest is a running record (no end), by its start.
    d.json(
        "b1.run.json",
        &with(
            stage(
                "reviewer",
                "/b",
                "2026-11-01T10:00:00Z",
                Some("2026-11-01T11:00:00Z"),
                "u",
            ),
            Some(limit("allowed")),
        ),
    );
    d.json(
        "b2.run.json",
        &with(
            stage("reviewer", "/b", "2026-11-15T10:00:00Z", None, "u"),
            Some(limit("allowed_warning")),
        ),
    );
    // Account /c: no limit anywhere: not listed.
    d.json(
        "c1.run.json",
        &with(
            stage(
                "fixer",
                "/c",
                "2026-11-15T10:00:00Z",
                Some("2026-11-15T11:00:00Z"),
                "u",
            ),
            Some(Value::Null),
        ),
    );
    d.json(
        "c2.run.json",
        &stage(
            "fixer",
            "/c",
            "2026-11-15T12:00:00Z",
            Some("2026-11-15T13:00:00Z"),
            "u",
        ),
    );
    let v = state(&mut Fake::new(), &d);
    let l = v["limits"].as_array().unwrap();
    assert_eq!(l.len(), 2, "{l:?}");
    let by: BTreeMap<_, _> = l
        .iter()
        .map(|e| (e["account"].as_str().unwrap(), e))
        .collect();
    assert_eq!(by["/a"]["record"], "runs/a2.run.json");
    assert_eq!(by["/a"]["limit"], limit("rejected"));
    assert_eq!(by["/b"]["record"], "runs/b2.run.json");
    assert_eq!(by["/b"]["limit"], limit("allowed_warning"));
    assert!(!by.contains_key("/c"));
}

#[test]
fn sr1_an_unreadable_record_is_listed_and_the_rest_is_printed() {
    // spec: specs/ap/state.md#R3
    // spec: specs/ap/state.md#R10
    let d = Dir::new();
    d.put("cut.run.json", "{\"role\": \"implementer\", ");
    d.put("list.unit.json", "[]");
    d.put("empty.run.json", "");
    d.put("two.unit.json", "{} {}");
    d.put("other.json", "not a record, not read");
    d.json(
        "ok.run.json",
        &stage(
            "reviewer",
            "/a",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:10:00Z"),
            "u",
        ),
    );
    d.json(
        "U-ok.unit.json",
        &unit(
            json!(3),
            "merged",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:10:00Z"),
        ),
    );
    let v = state(&mut Fake::new(), &d);
    assert_eq!(
        v["unreadable"],
        json!([
            "runs/cut.run.json",
            "runs/empty.run.json",
            "runs/list.unit.json",
            "runs/two.unit.json"
        ])
    );
    assert_eq!(v["units"].as_array().unwrap().len(), 1);
    assert_eq!(v["stages"].as_array().unwrap().len(), 1);
}

#[test]
fn sr1_text_form_is_bounded_and_ordered() {
    // spec: specs/ap/state.md#R11
    let d = Dir::new();
    // Twelve chains, a running stage, a running call and two accounts with limits.
    for n in 0..12 {
        let start = format!("2026-11-{:02}T10:00:00Z", n + 1);
        d.json(
            &format!("U-{n:02}.unit.json"),
            &with_item(
                unit(json!(n + 1), "merged", &start, Some(&start)),
                &format!("item-{n:02}"),
            ),
        );
    }
    d.json(
        "run.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-x.scope",
        ),
    );
    d.json(
        "U-run.unit.json",
        &unit(Value::Null, "running", "2026-11-14T10:00:00Z", None),
    );
    let mut a = stage(
        "implementer",
        "/acc",
        "2026-11-14T09:00:00Z",
        Some("2026-11-14T09:30:00Z"),
        "u",
    );
    a["limit"] = limit("rejected");
    d.json("lim.run.json", &a);
    let mut f = Fake::new();
    let text = run(&mut f, &[&d.arg(), "--text"]).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1 + 1 + 1 + 10 + 1, "{text}");
    // The form line, then what runs or is stale, then the newest ten chains, then the limits.
    assert!(
        lines[0].starts_with("form: STATE.md absent, runs/ present"),
        "{text}"
    );
    assert!(
        lines[1].contains("runs/run.run.json") && lines[1].contains("stale"),
        "{text}"
    );
    assert!(
        lines[2].contains("runs/U-run.unit.json") && lines[2].contains("running"),
        "{text}"
    );
    // The newest chain is the call that runs (no item, no pull request), then item-11 downwards.
    assert!(lines[3].contains("running"), "{text}");
    let chains: Vec<_> = lines[3..13].to_vec();
    assert!(
        chains[1].contains("item-11")
            && chains[1].contains("merged")
            && chains[1].contains(&"c".repeat(12)),
        "{text}"
    );
    assert!(
        chains[9].contains("item-03") && !text.contains("item-02"),
        "{text}"
    );
    assert!(
        lines[13].contains("/acc") && lines[13].contains("rejected"),
        "{text}"
    );
    assert!(!text.contains("more"), "no line is cut: {text}");
    // More than sixty lines: sixty are printed, the last says how many more the JSON holds.
    for n in 0..70 {
        let start = format!("2026-11-14T10:{:02}:{:02}Z", n / 60, n % 60);
        d.json(
            &format!("r{n:02}.run.json"),
            &stage("implementer", "/a", &start, None, "ap-run-y.scope"),
        );
    }
    let text = run(&mut f, &[&d.arg(), "--text"]).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 60, "{text}");
    assert!(
        lines[0].starts_with("form: ") && lines[1].contains("runs/") && lines[1].contains("stale")
    );
    let total = 71 + 1 + 10 + 1;
    assert!(
        lines[59].contains(&format!("{} more", total - 58)),
        "{}",
        lines[59]
    );
    // Without --text the JSON holds every record that runs.
    let v = state(&mut f, &d);
    assert_eq!(v["stages"].as_array().unwrap().len(), 71 + 1);
}

#[test]
fn sr1_arguments_are_refused_with_a_usage_error() {
    // spec: specs/ap/state.md#R1
    let d = Dir::new();
    let (dir, missing) = (d.arg(), format!("{}/missing", d.arg()));
    let file = d.0.join("runs").join("a-file");
    std::fs::write(&file, "x").unwrap();
    let (dir, missing, file) = (dir.as_str(), missing.as_str(), file.to_str().unwrap());
    let refused: [&[&str]; 9] = [
        &[],
        &["--text"],
        &[dir, "x"],
        &[dir, "--text", "--text"],
        &[dir, "--json"],
        &[dir, "--store"],
        &["relative-dir"],
        &[file],
        &[missing],
    ];
    for args in refused {
        let e = run(&mut Fake::new(), args).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{args:?}");
    }
    // The binary: exit 2 and the error on stderr; a readable directory exits 0.
    let ap = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ap"))
            .arg("state")
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let out = ap(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        v,
        json!({"error": {"code": "usage", "field": "args", "detail": null}})
    );
    d.json(
        "a.run.json",
        &stage(
            "reviewer",
            "/a",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:10:00Z"),
            "u",
        ),
    );
    let out = ap(&[&d.arg()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stages"].as_array().unwrap().len(), 1);
    let out = ap(&[&d.arg(), "--text"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8(out.stdout).unwrap().ends_with('\n'));
}

/// Every file of the tree with its bytes, `.git` included.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                todo.push(p);
            } else {
                out.insert(p.clone(), std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

#[test]
fn sr1_a_run_writes_nothing_and_calls_only_git_systemctl_and_ps() {
    // spec: specs/ap/state.md#R2
    let d = Dir::new();
    d.git(&["init", "-q"]);
    d.json(
        "a.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-1.scope",
        ),
    );
    d.json(
        "U-a.unit.json",
        &unit(Value::Null, "running", "2026-11-14T10:00:00Z", None),
    );
    d.put("a.stream.jsonl", "{}\n");
    d.put("cut.run.json", "{");
    d.git(&["add", "-A"]);
    d.git(&["commit", "-qm", "a"]);
    // The index no longer matches the files' stat data: a plain `git status` would refresh it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    d.json(
        "a.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-1.scope",
        ),
    );
    std::fs::write(d.0.join("loose"), "x").unwrap();
    let before = tree(&d.0);
    let mut f = Fake::new();
    for args in [vec![d.arg()], vec![d.arg(), "--text".into()]] {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        run(&mut f, &args).unwrap();
    }
    assert_eq!(tree(&d.0), before, "the tree changed");
    assert!(!d.0.join(".git/index.lock").exists());
    let programs: BTreeSet<_> = f.programs.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(programs, BTreeSet::from(["git", "ps", "systemctl"]));
    for (program, args) in &f.programs {
        if program == "git" {
            assert!(args.iter().any(|a| a == "--no-optional-locks"), "{args:?}");
        }
    }
}

#[test]
fn sr1_stale_of_a_stage_is_null_when_systemctl_cannot_be_read() {
    // spec: specs/ap/state.md#R7
    let d = Dir::new();
    d.json(
        "live.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-1.scope",
        ),
    );
    d.json(
        "U-live.unit.json",
        &unit(Value::Null, "running", "2026-11-14T10:00:00Z", None),
    );
    d.json(
        "U-live.implementer.md.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            None,
            "ap-run-1.scope",
        ),
    );
    let mut f = Fake::new();
    f.systemctl_fails = true;
    let v = state(&mut f, &d);
    for s in v["stages"].as_array().unwrap() {
        assert_eq!(s["stale"], Value::Null, "{s}");
    }
    // The call's own rule does not read `systemctl`: no `ap unit` lives, so it is stale.
    assert_eq!(v["units"][0]["calls"][0]["stale"], true);
}

/// The stamp of the time `ago` seconds before now, in the form of the records.
fn stamp_ago(ago: u32) -> String {
    let out = Command::new("date")
        .args([
            "-u",
            "-d",
            &format!("{ago} seconds ago"),
            "+%Y-%m-%dT%H:%M:%SZ",
        ])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// `stale` of every stage and call of the answer by record path.
fn stale_of(v: &Value) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    for s in v["stages"].as_array().unwrap() {
        m.insert(s["record"].as_str().unwrap().to_owned(), s["stale"].clone());
    }
    for c in v["units"].as_array().unwrap() {
        for c in c["calls"].as_array().unwrap() {
            m.insert(c["record"].as_str().unwrap().to_owned(), c["stale"].clone());
        }
    }
    m
}

/// A running call `U-<name>` that owns one open stage whose unit is `<name>.scope`, and the
/// same stage alone as `<name>.run.json`.
fn call_with_stage(d: &Dir, name: &str, start: &str) {
    let unit_name = format!("{name}.scope");
    d.json(
        &format!("U-{name}.unit.json"),
        &unit(Value::Null, "running", start, None),
    );
    for file in [
        format!("U-{name}.implementer.md.run.json"),
        format!("{name}.run.json"),
    ] {
        d.json(&file, &stage("implementer", "/a", start, None, &unit_name));
    }
}

#[test]
fn sr1_stale_follows_the_word_of_systemctl_and_not_its_exit() {
    // spec: specs/ap/state.md#R7
    let d = Dir::new();
    let words = [
        ("active", true, "active"),
        ("activating", true, "activating"),
        ("deactivating", true, "deactivating"),
        ("reloading", true, "reloading"),
        ("refreshing", true, "refreshing"),
        // A unit that does not exist: exit 4, `inactive`.
        ("missing", false, "inactive"),
        ("failed", false, "failed"),
        ("strange", true, "something-new"),
        // A word printed with a non-zero exit is still the word.
        ("exit-with-alive-word", false, "activating"),
        // No word: nothing to read, whatever the exit.
        ("silent-ok", true, ""),
        ("silent-fail", false, ""),
    ];
    let mut f = Fake::new();
    for (name, ok, word) in words {
        call_with_stage(&d, name, "2026-11-14T10:00:00Z");
        f.words
            .insert(format!("{name}.scope"), (ok, format!("{word}\n")));
    }
    f.words
        .insert("silent-ok.scope".into(), (true, "\n".into()));
    f.ps = Some(vec!["/usr/lib/systemd/systemd".into()]);
    let m = stale_of(&state(&mut f, &d));
    let alive = [
        "active",
        "activating",
        "deactivating",
        "reloading",
        "refreshing",
    ];
    for (name, _, word) in words {
        let (stage, call) = (
            &m[&format!("runs/{name}.run.json")],
            &m[&format!("runs/U-{name}.unit.json")],
        );
        let want = match name {
            "silent-ok" | "silent-fail" => Value::Null,
            "exit-with-alive-word" => json!(false),
            _ => json!(!alive.contains(&word)),
        };
        assert_eq!(*stage, want, "stage of {name}");
        // A call is false when its stage lives; with no word it is decided by `ps` (no `ap unit`).
        let call_want = if want == json!(false) {
            json!(false)
        } else {
            json!(true)
        };
        assert_eq!(*call, call_want, "call of {name}");
    }
    // The same with `ap unit` alive somewhere: a call no live stage decides is null.
    f.ps = Some(vec!["ap unit".into()]);
    let m = stale_of(&state(&mut f, &d));
    assert_eq!(m["runs/U-missing.unit.json"], Value::Null);
    assert_eq!(m["runs/U-activating.unit.json"], false);
    // `systemctl` that cannot be started says nothing, for a stage and for a call.
    let mut f = Fake::new();
    f.systemctl_error = true;
    f.ps = Some(vec!["ap unit".into()]);
    let m = stale_of(&state(&mut f, &d));
    for (name, _, _) in words {
        assert_eq!(m[&format!("runs/{name}.run.json")], Value::Null, "{name}");
        assert_eq!(
            m[&format!("runs/U-{name}.unit.json")],
            Value::Null,
            "{name}"
        );
    }
    f.ps = Some(vec!["/usr/lib/systemd/systemd".into()]);
    let m = stale_of(&state(&mut f, &d));
    assert_eq!(
        m["runs/U-active.unit.json"], true,
        "no live stage, no `ap unit`"
    );
    assert_eq!(m["runs/active.run.json"], Value::Null);
}

#[test]
fn sr1_a_record_under_ten_seconds_old_is_never_stale() {
    // spec: specs/ap/state.md#R7
    let d = Dir::new();
    call_with_stage(&d, "young", &stamp_ago(3));
    call_with_stage(&d, "old", &stamp_ago(60));
    call_with_stage(&d, "young-alive", &stamp_ago(3));
    let mut f = Fake::new();
    f.active.insert("young-alive.scope".into());
    // Nothing is active but `young-alive`: `ap run` has not started the unit of `young` yet.
    f.ps = Some(vec!["ap unit".into()]);
    let m = stale_of(&state(&mut f, &d));
    assert_eq!(m["runs/young.run.json"], Value::Null);
    assert_eq!(m["runs/old.run.json"], true);
    assert_eq!(m["runs/young-alive.run.json"], false);
    assert_eq!(m["runs/U-young.unit.json"], Value::Null);
    assert_eq!(m["runs/U-young-alive.unit.json"], false);
    // A call whose only stage is young is decided by the process table, as any call without a live stage.
    f.ps = Some(vec!["/usr/lib/systemd/systemd".into()]);
    assert_eq!(stale_of(&state(&mut f, &d))["runs/U-young.unit.json"], true);
}

#[test]
fn sr1_stale_of_a_call_is_null_when_ps_was_not_read() {
    // spec: specs/ap/state.md#R7
    let d = Dir::new();
    d.json(
        "U-gone.unit.json",
        &unit(Value::Null, "running", "2026-11-14T10:00:00Z", None),
    );
    call_with_stage(&d, "live", "2026-11-14T11:00:00Z");
    d.json(
        "U-merged.unit.json",
        &unit(
            Value::Null,
            "merged",
            "2026-11-14T12:00:00Z",
            Some("2026-11-14T12:10:00Z"),
        ),
    );
    for reply in [
        (false, String::new()),
        (false, "ps: error\n".to_owned()),
        (true, String::new()),
        (true, "  \n".to_owned()),
    ] {
        let mut f = Fake::new();
        f.active.insert("live.scope".into());
        f.ps_reply = Some(reply.clone());
        let m = stale_of(&state(&mut f, &d));
        assert_eq!(m["runs/U-gone.unit.json"], Value::Null, "{reply:?}");
        // A live stage decides without `ps`; a call that does not run is null.
        assert_eq!(m["runs/U-live.unit.json"], false, "{reply:?}");
        assert_eq!(m["runs/live.run.json"], false, "{reply:?}");
        assert_eq!(m["runs/U-merged.unit.json"], Value::Null, "{reply:?}");
    }
    // A table that was read (exit 0, rows without `ap unit`) still says true.
    let mut f = Fake::new();
    f.ps_reply = Some((true, "bash\n".into()));
    assert_eq!(stale_of(&state(&mut f, &d))["runs/U-gone.unit.json"], true);
}

#[test]
fn sr1_git_runs_no_hook_of_the_repository_and_a_failed_status_is_null() {
    // spec: specs/ap/state.md#R2
    let d = Dir::new();
    d.git(&["init", "-q"]);
    d.json(
        "a.run.json",
        &stage(
            "implementer",
            "/a",
            "2026-11-14T10:00:00Z",
            Some("2026-11-14T10:10:00Z"),
            "u",
        ),
    );
    d.git(&["add", "-A"]);
    d.git(&["commit", "-qm", "a"]);
    // A hook named by the repository's own configuration leaves a mark outside the tree.
    let aside = std::env::temp_dir().join(format!("ap-state-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&aside);
    std::fs::create_dir_all(&aside).unwrap();
    let (hook, mark) = (aside.join("hook.sh"), aside.join("ran"));
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ntouch {}\nexit 1\n", mark.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    d.git(&["config", "core.fsmonitor", hook.to_str().unwrap()]);
    let mut f = Fake::new();
    let v = state(&mut f, &d);
    assert!(!mark.exists(), "the repository's fsmonitor hook ran");
    assert_eq!(v["task"]["uncommitted"], 0);
    for (_, args) in f.programs.iter().filter(|(p, _)| p == "git") {
        let flags: Vec<_> = args.iter().map(String::as_str).collect();
        assert!(
            flags
                .windows(2)
                .any(|w| w == ["-c", "core.fsmonitor=false"]),
            "{flags:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&aside);
    // `git status` fails on a corrupt index: no count, the head is still read.
    std::fs::write(d.0.join(".git/index"), "not an index").unwrap();
    let v = state(&mut f, &d);
    assert_eq!(v["task"]["uncommitted"], Value::Null);
    assert_eq!(v["task"]["head"], d.git(&["rev-parse", "HEAD"]));
}

/// `n` lines of text, each ended by a line feed.
fn lines_of(n: usize) -> String {
    (0..n).map(|i| format!("line {i}\n")).collect()
}
fn form(f: &mut Fake, d: &Dir) -> Value {
    state(f, d)["form"].clone()
}

#[test]
fn sr_form_counts_state_lines_and_blocks() {
    // spec: specs/ap/state.md#R13
    let d = Dir::new();
    let mut f = Fake::new();
    let state_md = d.0.join("STATE.md");
    let at = |lines: u64, blocks: u64| json!({"state_md": {"lines": lines, "limit": 30, "blocks": blocks}, "run_tree": true});
    // Absent: null, and `runs/` is a directory.
    assert_eq!(
        form(&mut f, &d),
        json!({"state_md": null, "run_tree": true})
    );
    // A symbolic link, also to a regular file, is not read; nor is a directory.
    std::fs::write(d.0.join("target.md"), lines_of(5)).unwrap();
    std::os::unix::fs::symlink(d.0.join("target.md"), &state_md).unwrap();
    assert_eq!(form(&mut f, &d)["state_md"], Value::Null);
    std::fs::remove_file(&state_md).unwrap();
    std::fs::create_dir(&state_md).unwrap();
    assert_eq!(form(&mut f, &d)["state_md"], Value::Null);
    std::fs::remove_dir(&state_md).unwrap();
    // Empty: no line. Thirty and thirty-one lines are counted, and the limit stays 30.
    for (text, lines) in [
        (String::new(), 0),
        (lines_of(30), 30),
        (lines_of(31), 31),
        // A last line without a line feed counts; so does a lone line feed.
        ("one\ntwo".to_owned(), 2),
        ("one\n\n".to_owned(), 2),
        ("\n".to_owned(), 1),
    ] {
        std::fs::write(&state_md, &text).unwrap();
        assert_eq!(form(&mut f, &d), at(lines, 0), "{text:?}");
    }
    // Two generated blocks: the lines that begin with the opening marker, and no other.
    let marker = "<!-- ont:render lead_state directory=d schema=work@21 -->";
    let text = format!(
        "head\n{marker}\n## Intent\n- (none)\n<!-- /ont:render -->\n\n {marker}\n<!-- ont:render\n{marker}\n- x\n<!-- /ont:render -->"
    );
    std::fs::write(&state_md, &text).unwrap();
    assert_eq!(form(&mut f, &d), at(11, 2), "{text}");
    // `run_tree` is true only for a directory named `runs`.
    std::fs::remove_dir_all(d.0.join("runs")).unwrap();
    assert_eq!(form(&mut f, &d)["run_tree"], false);
    std::fs::write(d.0.join("runs"), "x").unwrap();
    assert_eq!(form(&mut f, &d)["run_tree"], false);
    // The answer holds the base's members beside `form`.
    let v = state(&mut f, &d);
    let mut keys: Vec<_> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "at",
            "form",
            "limits",
            "stages",
            "streams_without_record",
            "task",
            "units",
            "unreadable"
        ]
    );
}

#[test]
fn sr_a_directory_without_runs_is_answered() {
    // spec: specs/ap/state.md#R12
    let d = Dir::new();
    std::fs::remove_dir(d.0.join("runs")).unwrap();
    d.git(&["init", "-q"]);
    std::fs::write(d.0.join("STATE.md"), lines_of(3)).unwrap();
    d.git(&["add", "-A"]);
    d.git(&["commit", "-qm", "a"]);
    std::fs::write(d.0.join("loose"), "x").unwrap();
    let mut f = Fake::new();
    let v = state(&mut f, &d);
    for member in [
        "units",
        "stages",
        "streams_without_record",
        "limits",
        "unreadable",
    ] {
        assert_eq!(v[member], json!([]), "{member}");
    }
    // Git is read as for any directory; neither `systemctl` nor `ps` is called.
    assert_eq!(v["task"]["head"], d.git(&["rev-parse", "HEAD"]));
    assert_eq!(v["task"]["uncommitted"], 1);
    assert_eq!(
        v["form"],
        json!({"state_md": {"lines": 3, "limit": 30, "blocks": 0}, "run_tree": false})
    );
    let programs: BTreeSet<_> = f.programs.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(programs, BTreeSet::from(["git"]));
    // An entry named `runs` that is not a directory is the same answer; nothing is written.
    std::fs::write(d.0.join("runs"), "{}").unwrap();
    let before = tree(&d.0);
    let mut f = Fake::new();
    let v = state(&mut f, &d);
    assert_eq!(v["units"], json!([]));
    assert_eq!(v["form"]["run_tree"], false);
    assert!(f.programs.iter().all(|(p, _)| p == "git"));
    run(&mut f, &[&d.arg(), "--text"]).unwrap();
    assert_eq!(tree(&d.0), before, "the tree changed");
    // The binary exits 0 with the same members.
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["state", &d.arg()])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["stages"], json!([]));
}

#[test]
fn sr_text_form_begins_with_the_form_line() {
    // spec: specs/ap/state.md#R14
    let d = Dir::new();
    let mut f = Fake::new();
    let state_md = d.0.join("STATE.md");
    let text = |f: &mut Fake| run(f, &[&d.arg(), "--text"]).unwrap();
    // No line of the base: the form line alone, with its line feed.
    assert_eq!(text(&mut f), "form: STATE.md absent, runs/ present\n");
    std::fs::remove_dir(d.0.join("runs")).unwrap();
    assert_eq!(text(&mut f), "form: STATE.md absent, runs/ absent\n");
    // A file of 12 lines and two blocks; then 30 and 31 lines, with and without `runs/`.
    let marker = "<!-- ont:render lead_state directory=d schema=work@21 -->";
    std::fs::write(&state_md, format!("{}{marker}\nx\n{marker}\n", lines_of(9))).unwrap();
    assert_eq!(
        text(&mut f),
        "form: STATE.md 12 of 30 lines, 2 rendered blocks, runs/ absent\n"
    );
    std::fs::create_dir(d.0.join("runs")).unwrap();
    assert_eq!(
        text(&mut f),
        "form: STATE.md 12 of 30 lines, 2 rendered blocks, runs/ present\n"
    );
    std::fs::write(&state_md, lines_of(30)).unwrap();
    assert_eq!(
        text(&mut f),
        "form: STATE.md 30 of 30 lines, 0 rendered blocks, runs/ present\n"
    );
    std::fs::write(&state_md, lines_of(31)).unwrap();
    assert_eq!(
        text(&mut f),
        "form: STATE.md 31 of 30 lines, 0 rendered blocks, runs/ present over\n"
    );
    std::fs::remove_dir(d.0.join("runs")).unwrap();
    assert_eq!(
        text(&mut f),
        "form: STATE.md 31 of 30 lines, 0 rendered blocks, runs/ absent over\n"
    );
    // The base's lines come after it, and at most 60 lines are printed in all: with 59 lines of
    // the base none is cut, with 60 the first 58 and the line that counts the two left out.
    std::fs::remove_file(&state_md).unwrap();
    std::fs::create_dir(d.0.join("runs")).unwrap();
    let stages = |from: usize, to: usize| {
        for n in from..to {
            let start = format!("2026-11-14T10:{:02}:{:02}Z", n / 60, n % 60);
            d.json(
                &format!("r{n:02}.run.json"),
                &stage("implementer", "/a", &start, None, "ap-run-y.scope"),
            );
        }
    };
    stages(0, 59);
    let out = text(&mut f);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 60, "{out}");
    assert_eq!(lines[0], "form: STATE.md absent, runs/ present");
    assert!(
        lines[1..].iter().all(|l| l.starts_with("runs: stage ")),
        "{out}"
    );
    assert!(!out.contains("more lines"), "no line is cut: {out}");
    stages(59, 60);
    let out = text(&mut f);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 60, "{out}");
    assert_eq!(lines[0], "form: STATE.md absent, runs/ present");
    assert!(
        lines[1..59].iter().all(|l| l.starts_with("runs: stage ")),
        "{out}"
    );
    assert_eq!(
        lines[59],
        "... 2 more lines: ap state without --text prints them all"
    );
    stages(60, 90);
    let out = text(&mut f);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 60, "{out}");
    assert_eq!(
        lines[59],
        "... 32 more lines: ap state without --text prints them all"
    );
    // The JSON form still holds every stage that runs.
    assert_eq!(state(&mut f, &d)["stages"].as_array().unwrap().len(), 90);
}
