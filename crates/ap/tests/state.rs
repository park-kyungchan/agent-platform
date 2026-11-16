//! `ap state` against a fixture task directory made here: a Git repository whose `runs/` holds
//! records of the shapes of real ones (members copied, no session id). `systemctl` and the process
//! table are answered by the fake `System`; Git is the real one, in the temporary directory.
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
    programs: Vec<(String, Vec<String>)>,
}
impl Fake {
    fn new() -> Self {
        Fake {
            active: BTreeSet::new(),
            ps: Some(vec!["/usr/lib/systemd/systemd --system".into()]),
            systemctl_fails: false,
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
                if self.systemctl_fails {
                    return reply(false, String::new());
                }
                match self.active.contains(args.last().unwrap()) {
                    true => reply(true, "active\n".into()),
                    false => reply(false, "inactive\n".into()),
                }
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
    let d = Dir::new();
    let mut f = Fake::new();
    let v = state(&mut f, &d);
    // Without Git: no head, nothing uncommitted.
    assert_eq!(
        (v["task"]["head"].clone(), v["task"]["uncommitted"].clone()),
        (Value::Null, json!(0))
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
    assert_eq!(lines.len(), 1 + 1 + 10 + 1, "{text}");
    // First what runs or is stale, then the newest ten chains, then the limits.
    assert!(
        lines[0].contains("runs/run.run.json") && lines[0].contains("stale"),
        "{text}"
    );
    assert!(
        lines[1].contains("runs/U-run.unit.json") && lines[1].contains("running"),
        "{text}"
    );
    // The newest chain is the call that runs (no item, no pull request), then item-11 downwards.
    assert!(lines[2].contains("running"), "{text}");
    let chains: Vec<_> = lines[2..12].to_vec();
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
        lines[12].contains("/acc") && lines[12].contains("rejected"),
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
    assert!(lines[0].contains("runs/") && lines[0].contains("stale"));
    let total = 71 + 1 + 10 + 1;
    assert!(
        lines[59].contains(&format!("{} more", total - 59)),
        "{}",
        lines[59]
    );
    // Without --text the JSON holds every record that runs.
    let v = state(&mut f, &d);
    assert_eq!(v["stages"].as_array().unwrap().len(), 71 + 1);
}

#[test]
fn sr1_arguments_are_refused_with_a_usage_error() {
    let d = Dir::new();
    let bare = std::env::temp_dir().join(format!("ap-state-bare-{}", std::process::id()));
    std::fs::create_dir_all(&bare).unwrap();
    let (dir, missing) = (d.arg(), format!("{}/missing", d.arg()));
    let bare_dir = bare.to_str().unwrap().to_owned();
    let (dir, missing, bare_dir) = (dir.as_str(), missing.as_str(), bare_dir.as_str());
    let refused: [&[&str]; 9] = [
        &[],
        &["--text"],
        &[dir, "x"],
        &[dir, "--text", "--text"],
        &[dir, "--json"],
        &[dir, "--store"],
        &["relative-dir"],
        &[bare_dir],
        &[missing],
    ];
    for args in refused {
        let e = run(&mut Fake::new(), args).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&bare);
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
