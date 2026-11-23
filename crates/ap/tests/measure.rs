//! `ap measure sessions` and the functions `cost`, `flow` and `share` (`specs/ap/measure.md`) against a fake host:
//! invented session records, stage records and a door record in a scratch directory of each test, and a fake `ont`
//! whose store is a map in memory (search, get, log, render and the four actions of schema version 19). No test
//! reads the host's accounts, stage directories, door record or store. The test with the real engine is ignored by
//! the plain `cargo test`: `./check` runs it with `--ignored` and the engine in `ONT_BIN`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Reply, System};
use ap::measure;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const OPUS: &str = "claude-opus-5-5";

fn epoch(t: &str) -> u64 {
    ap::watch::epoch(t).unwrap()
}
fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A new empty directory under the temporary directory, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ap-measure-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(std::fs::canonicalize(&p).unwrap())
    }
    fn at(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn path(&self, name: &str) -> String {
        self.at(name).to_str().unwrap().to_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The fake host: the port's wall clock and environment, and a store in memory.
struct Fake {
    wall: u64,
    env: Vec<(String, String)>,
    objects: BTreeMap<(String, String), Value>,
    requests: BTreeMap<String, (Value, Value)>,
    log: Vec<Value>,
    calls: Vec<(Vec<String>, Value)>,
    made: u32,
    fail_search: bool,
}
impl Fake {
    fn new(dir: &Scratch) -> Self {
        Self {
            wall: epoch("2026-11-20T12:00:00Z"),
            env: vec![
                ("HOME".into(), dir.path("home")),
                ("AP_DOOR_LOG".into(), dir.path("door.jsonl")),
            ],
            objects: BTreeMap::new(),
            requests: BTreeMap::new(),
            log: vec![],
            calls: vec![],
            made: 0,
            fail_search: false,
        }
    }
    fn put(&mut self, kind: &str, key: &str, fields: Value) {
        self.objects.insert((kind.into(), key.into()), fields);
    }
    fn runs(&self) -> Vec<(String, Value)> {
        self.objects
            .iter()
            .filter(|((k, _), _)| k == "run")
            .map(|((_, key), f)| (key.clone(), f.clone()))
            .collect()
    }
    fn run_of(&self, field: &str, value: &str) -> Value {
        let found: Vec<_> = self
            .runs()
            .into_iter()
            .filter(|(_, f)| f[field] == value)
            .collect();
        assert_eq!(found.len(), 1, "one run of {field} {value}: {found:?}");
        found[0].1.clone()
    }
    fn applies(&self) -> Vec<(String, String)> {
        self.calls
            .iter()
            .filter(|(a, _)| a[0] == "action")
            .map(|(_, b)| {
                (
                    b["action_type"].as_str().unwrap().to_owned(),
                    b["request_id"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
    fn ok(&self, result: Value) -> Answer {
        let v = json!({"status": "ok", "now": self.wall, "schema": {"work": {"version": 19}}, "result": result});
        reply(&v)
    }
    fn stamp(&self) -> String {
        ap::watch::utc(self.wall)
    }
    fn search(&self, b: &Value) -> Answer {
        let kind = b["object_type"].as_str().unwrap();
        let all = b["where"]["all"].as_array().cloned().unwrap_or_default();
        let found: Vec<Value> = self
            .objects
            .iter()
            .filter(|((k, _), f)| {
                k == kind
                    && all
                        .iter()
                        .all(|c| f[c["eq"][0].as_str().unwrap()] == c["eq"][1])
            })
            .map(|((k, key), f)| {
                let fields: serde_json::Map<String, Value> = f
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(n, v)| (n.clone(), json!({"kind": "value", "value": v})))
                    .collect();
                json!({"id": oid(k, key), "head": {"fields": fields}})
            })
            .collect();
        self.ok(json!({"basis": {"qualified": true}, "objects": found, "continuation": null}))
    }
    fn get(&self, b: &Value) -> Answer {
        let id = &b["id"];
        let key = (
            id["object_type"].as_str().unwrap().to_owned(),
            id["native_key"].as_str().unwrap().to_owned(),
        );
        let Some(f) = self.objects.get(&key) else {
            return reply(&json!({"status": "refused", "code": "unavailable"}));
        };
        let fields: serde_json::Map<String, Value> = f
            .as_object()
            .unwrap()
            .iter()
            .map(|(n, v)| (n.clone(), json!({"kind": "value", "value": v})))
            .collect();
        self.ok(json!({"objects": [{"id": id, "head": "r1", "observations": [{"revision": "r1", "fields": fields}]}]}))
    }
    fn apply(&mut self, b: &Value) -> Answer {
        let (action, rid, p) = (
            b["action_type"].as_str().unwrap().to_owned(),
            b["request_id"].as_str().unwrap().to_owned(),
            b["parameters"].clone(),
        );
        assert_eq!(b["agent"], "ap-measure");
        assert!(rid.len() <= 64, "{rid}");
        if let Some((before, edits)) = self.requests.get(&rid) {
            if *before != p {
                return reply(&json!({"status": "refused", "code": "request_id_reused"}));
            }
            let edits = edits.clone();
            return self.ok(json!({"outcome": "already_applied", "edits": edits}));
        }
        let now = self.stamp();
        let key = match action.as_str() {
            "record_session" => {
                assert!(p["started_at"].as_str() <= p["ended_at"].as_str());
                self.made += 1;
                let key = format!("act-made-{}", self.made);
                let mut f = p.clone();
                f["status"] = json!("closed");
                f["state"] = json!("session");
                f["measured_at"] = json!(now);
                self.put("run", &key, f);
                key
            }
            "extend_session" | "measure_run" => {
                let key = p["run"]["native_key"].as_str().unwrap().to_owned();
                let f = self.objects.get_mut(&("run".into(), key.clone())).unwrap();
                if action == "extend_session" {
                    assert_eq!(f["state"], "session");
                }
                for (n, v) in p.as_object().unwrap() {
                    if n != "run" {
                        f[n] = v.clone();
                    }
                }
                if action == "measure_run" {
                    f["measured_at"] = json!(now);
                }
                key
            }
            other => panic!("action {other}"),
        };
        let edits = json!([{"change": if action == "record_session" { "initial" } else { "update" }, "id": oid("run", &key)}]);
        self.requests.insert(rid, (p, edits.clone()));
        self.ok(json!({"outcome": "applied", "edits": edits}))
    }
}
fn reply(v: &Value) -> Answer {
    let bytes = v.to_string().into_bytes();
    Ok(Reply {
        success: v["status"] == "ok",
        stdout: bytes.clone(),
        bytes,
    })
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _ms: u64) -> Answer {
        assert_eq!(program, "ont");
        self.calls.push((args.to_vec(), input.clone()));
        match args[0].as_str() {
            "search" if self.fail_search => {
                reply(&json!({"status": "refused", "code": "store_unreadable"}))
            }
            "search" => self.search(input),
            "get" => self.get(input),
            "action" => self.apply(input),
            "log" => {
                let action = input["action_type"].clone();
                let rows: Vec<Value> = self
                    .log
                    .iter()
                    .filter(|r| r["action_type"] == action)
                    .cloned()
                    .collect();
                self.ok(json!({"rows": rows, "withheld": 0, "continuation": null}))
            }
            "render" => reply(
                &json!({"status": "ok", "now": self.wall, "truncated": false, "markdown": "| Key |\n| --- |\n", "rows": 0}),
            ),
            other => panic!("ont {other}"),
        }
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.wall)
    }
}

/// The scratch layout of a pass: two accounts, the roots, the stage directories, and the records written there.
struct Bed {
    dir: Scratch,
}
impl Bed {
    fn new() -> Self {
        let dir = Scratch::new();
        for d in [
            "accounts/claude-a/projects",
            "accounts/claude-b/projects",
            "work/task-a/wt/agent-platform",
            "platforms/agent-platform",
            "units",
            "reviews",
            "home",
            "elsewhere",
        ] {
            std::fs::create_dir_all(dir.at(d)).unwrap();
        }
        Self { dir }
    }
    fn work(&self, sub: &str) -> String {
        self.dir.path(&format!("work/{sub}"))
    }
    /// A session file of `lines` at `rel` under the scratch directory, modified at `modified`.
    fn file(&self, rel: &str, lines: &[Value], modified: &str) -> PathBuf {
        let path = self.dir.at(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, text).unwrap();
        touch(&path, modified);
        path
    }
    fn stage(&self, rel: &str, role: &str, session: &str, start: &str) -> String {
        let path = self.dir.at(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let record = json!({"role": role, "model": OPUS, "session": session, "start": start, "state": "ok",
            "cwd": self.dir.path("elsewhere"), "tokens": {}, "resumes": null});
        std::fs::write(&path, record.to_string()).unwrap();
        path.to_str().unwrap().to_owned()
    }
    fn args(&self, since: &str) -> Vec<OsString> {
        [
            "sessions",
            "--since",
            since,
            "--accounts",
            &self.dir.path("accounts/claude-a"),
            "--accounts",
            &self.dir.path("accounts/claude-b"),
            "--work-root",
            &self.dir.path("work"),
            "--platforms-root",
            &self.dir.path("platforms"),
            "--stage-dir",
            &self.dir.path("units"),
            "--stage-dir",
            &self.dir.path("reviews"),
        ]
        .iter()
        .map(OsString::from)
        .collect()
    }
    fn pass(&self, f: &mut Fake, since: &str) -> Value {
        let (line, status) = measure::run(f, &self.args(since)).unwrap();
        assert_eq!(status, 0, "{line}");
        line
    }
}
fn touch(path: &Path, at: &str) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_secs(epoch(at)))
        .unwrap();
}
fn assistant(
    sid: &str,
    ts: &str,
    cwd: &str,
    mid: &str,
    model: &str,
    input: u64,
    content: Value,
) -> Value {
    json!({"type": "assistant", "sessionId": sid, "timestamp": ts, "cwd": cwd,
        "message": {"id": mid, "model": model, "role": "assistant", "content": content,
            "usage": {"input_tokens": input, "output_tokens": 0, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}})
}
fn msg(sid: &str, ts: &str, cwd: &str, mid: &str) -> Value {
    assistant(
        sid,
        ts,
        cwd,
        mid,
        OPUS,
        1000,
        json!([{"type": "text", "text": "SECRET-TEXT"}]),
    )
}
/// A `<synthetic>` line of a code-mode script: one call, no usage.
fn synthetic(sid: &str, ts: &str, cwd: &str, use_id: &str, name: &str, input: Value) -> Value {
    json!({"type": "assistant", "sessionId": sid, "timestamp": ts, "cwd": cwd,
        "message": {"model": "<synthetic>", "role": "assistant",
            "content": [{"type": "tool_use", "id": use_id, "name": name, "input": input}]}})
}
fn result(sid: &str, ts: &str, cwd: &str, use_id: &str, text: &str) -> Value {
    json!({"type": "user", "sessionId": sid, "timestamp": ts, "cwd": cwd,
        "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": use_id, "content": text}]}})
}
fn days(run: &Value) -> Vec<(String, u64)> {
    run["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["day"].as_str().unwrap().to_owned(),
                d["messages"].as_u64().unwrap(),
            )
        })
        .collect()
}
fn day<'a>(run: &'a Value, d: &str) -> &'a Value {
    run["days"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["day"] == d)
        .unwrap()
}

// spec: specs/ap/measure.md#R1
#[test]
fn r1_the_verb_names_its_pass_and_refuses_another() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    for args in [&[][..], &["kpis"][..], &["--since"][..]] {
        let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
        let e = measure::run(&mut f, &argv).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{args:?}");
        assert_eq!(measure::exit_of(&e), 2);
    }
    assert!(f.calls.is_empty());
    let help = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["measure", "--help"])
        .output()
        .unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Usage: ap measure sessions [--since <YYYY-MM-DD>]")
    );
    let none = Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("measure")
        .env_remove("ONT_FN_NAME")
        .output()
        .unwrap();
    assert_eq!(none.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&none.stderr).unwrap();
    assert_eq!(e["error"]["code"], "usage");
    assert!(none.stdout.is_empty());
}

// spec: specs/ap/measure.md#R2
#[test]
fn r2_a_malformed_argument_list_is_refused_before_any_effect() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let bad_prices = bed.dir.at("prices.tsv");
    std::fs::write(&bad_prices, "claude-x\tinput\n").unwrap();
    let cases: [(&[&str], &str, &str); 9] = [
        (&["sessions", "--unknown", "x"], "usage", "args"),
        (&["sessions", "--since"], "usage", "args"),
        (
            &["sessions", "--since", "2026-11-01", "--since", "2026-11-02"],
            "usage",
            "args",
        ),
        (
            &["sessions", "--since", "2026-11-31"],
            "invalid_input",
            "since",
        ),
        (
            &["sessions", "--since", "20261101"],
            "invalid_input",
            "since",
        ),
        (
            &["sessions", "--accounts", "relative/dir"],
            "invalid_input",
            "accounts",
        ),
        (
            &["sessions", "--stage-dir", "relative"],
            "invalid_input",
            "stage-dir",
        ),
        (
            &["sessions", "--store", "store.sqlite"],
            "invalid_input",
            "store",
        ),
        (
            &["sessions", "--prices", bad_prices.to_str().unwrap()],
            "invalid_input",
            "prices",
        ),
    ];
    for (args, code, field) in cases {
        let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
        let e = measure::run(&mut f, &argv).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), (code, field), "{args:?}");
        assert_eq!(measure::exit_of(&e), 2);
    }
    assert!(f.calls.is_empty(), "no store call before a refusal");
}

// spec: specs/ap/measure.md#R2
#[test]
fn r2_the_defaults_are_two_days_back_and_the_accounts_of_home() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let account = bed.dir.at("home/.claude-x");
    std::fs::create_dir_all(account.join("projects/p")).unwrap();
    let cwd = bed.work("task-a");
    std::fs::write(
        account.join("projects/p/s-default.jsonl"),
        format!(
            "{}\n",
            msg("s-default", "2026-11-18T10:00:00Z", &cwd, "m-default")
        ),
    )
    .unwrap();
    // Only the accounts and the roots are given by default here; the stage directories are the test's.
    let argv: Vec<OsString> = [
        "sessions",
        "--work-root",
        &bed.dir.path("work"),
        "--platforms-root",
        &bed.dir.path("platforms"),
        "--stage-dir",
        &bed.dir.path("units"),
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let (line, _) = measure::run(&mut f, &argv).unwrap();
    assert_eq!(line["since"], "2026-11-18");
    let run = f.run_of("session", "s-default");
    assert_eq!(
        (run["account"].as_str(), run["role"].as_str()),
        (Some(".claude-x"), Some("lead"))
    );
}

// spec: specs/ap/measure.md#R3
#[test]
fn r3_the_academy_is_skipped_by_its_names() {
    assert!(measure::academy("edu-lessons", false));
    assert!(measure::academy("edu-lessons", true));
    assert!(measure::academy("-home-palantirkc-work-edu-lessons", true));
    assert!(measure::academy(
        "-home-palantirkc-platforms-education-platform",
        true
    ));
    assert!(
        !measure::academy("-home-palantirkc-work-edu-lessons", false),
        "the substring rule is a project directory's"
    );
    assert!(!measure::academy("-home-palantirkc-work-task-a", true));
    assert!(!measure::academy("education", true));
    assert!(!measure::academy("reduce-x", false));
}

// spec: specs/ap/measure.md#R3
#[test]
fn r3_a_project_directory_of_the_academy_is_not_read() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    bed.file(
        "accounts/claude-a/projects/-scratch-work-edu-x/s-skip.jsonl",
        &[msg("s-skip", "2026-11-18T10:00:00Z", &cwd, "m-skip")],
        "2026-11-18T10:00:01Z",
    );
    bed.file(
        "accounts/claude-a/projects/-scratch-work-task-a/s-kept.jsonl",
        &[msg("s-kept", "2026-11-18T10:00:00Z", &cwd, "m-kept")],
        "2026-11-18T10:00:01Z",
    );
    let line = bed.pass(&mut f, "2026-11-18");
    assert_eq!(line["files"]["skipped_academy"], 1);
    assert_eq!(line["files"]["read"], 1);
    assert_eq!(f.runs().len(), 1);
    f.run_of("session", "s-kept");
}

// spec: specs/ap/measure.md#R3
// spec: specs/ap/measure.md#R5
#[test]
fn r3_a_stage_session_in_an_account_home_is_measured_on_its_records_run() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let record = bed.stage(
        "reviews/act-rq-1.md.run.json",
        "reviewer",
        "s-home",
        "2026-11-18T09:00:00Z",
    );
    f.put(
        "run",
        "act-stage-1",
        json!({"role": "reviewer", "runtime": "claude", "model": OPUS, "status": "closed", "state": "ok",
            "started_at": "2026-11-18T09:00:00Z", "ended_at": "2026-11-18T09:30:00Z", "record": record,
            "session": "s-home", "input_tokens": 7, "output_tokens": 8}),
    );
    let elsewhere = bed.dir.path("elsewhere");
    bed.file(
        "accounts/claude-b/homes/s-home/projects/-scratch-elsewhere/s-home.jsonl",
        &[msg("s-home", "2026-11-18T09:10:00Z", &elsewhere, "m-home")],
        "2026-11-18T09:10:01Z",
    );
    // A second stage under a home, whose run `ap run` did not make: recorded with the account's last component.
    let other = bed.stage(
        "units/u1/1.implementer.md.run.json",
        "implementer",
        "s-home-2",
        "2026-11-18T09:00:00Z",
    );
    bed.file(
        "accounts/claude-b/homes/s-home-2/projects/-scratch-elsewhere/s-home-2.jsonl",
        &[msg(
            "s-home-2",
            "2026-11-18T09:20:00Z",
            &elsewhere,
            "m-home-2",
        )],
        "2026-11-18T09:20:01Z",
    );
    let line = bed.pass(&mut f, "2026-11-18");
    assert_eq!(line["runs"]["recorded"], 1);
    let stage = f.run_of("record", &record);
    assert_eq!(days(&stage), [("2026-11-18".to_owned(), 1)]);
    assert_eq!(day(&stage, "2026-11-18")["usd_micros"], 4000);
    for (k, v) in [
        ("role", json!("reviewer")),
        ("state", json!("ok")),
        ("ended_at", json!("2026-11-18T09:30:00Z")),
        ("input_tokens", json!(7)),
    ] {
        assert_eq!(stage[k], v, "{k} stays as start_run and close_run wrote it");
    }
    let made = f.run_of("record", &other);
    assert_eq!(made["account"], "claude-b");
    assert_eq!(made["uses"], oid("account", "claude-b"));
    assert_eq!(made["host"], oid("host", "local"));
    assert_eq!(made["role"], "implementer");
    assert_eq!(made["session"], "s-home-2");
}

// spec: specs/ap/measure.md#R4
#[test]
fn r4_a_fork_repeats_messages_and_a_later_pass_counts_no_day_twice() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    let first = [
        msg("s-a", "2026-11-16T10:00:00Z", &cwd, "m1"),
        msg("s-a", "2026-11-17T10:00:00Z", &cwd, "m2"),
    ];
    bed.file(
        "accounts/claude-a/projects/p1/s-a.jsonl",
        &first,
        "2026-11-17T10:00:01Z",
    );
    // The fork's file repeats the first session's lines under its own id, then goes on.
    let fork: Vec<Value> = first
        .iter()
        .map(|l| {
            let mut l = l.clone();
            l["sessionId"] = json!("s-b");
            l
        })
        .chain([msg("s-b", "2026-11-18T10:00:00Z", &cwd, "m3")])
        .collect();
    bed.file(
        "accounts/claude-a/projects/p1/s-b.jsonl",
        &fork,
        "2026-11-18T10:00:01Z",
    );
    bed.pass(&mut f, "2026-11-16");
    let a = f.run_of("session", "s-a");
    let b = f.run_of("session", "s-b");
    assert_eq!(
        days(&a),
        [("2026-11-16".to_owned(), 1), ("2026-11-17".to_owned(), 1)]
    );
    assert_eq!(days(&b), [("2026-11-18".to_owned(), 1)]);
    // A later pass leaves the first session's file unread: its days stay, the fork takes none of its messages.
    let line = bed.pass(&mut f, "2026-11-18");
    assert_eq!(
        line["files"]["read"], 2,
        "the older file is still read for its starts"
    );
    assert_eq!(f.run_of("session", "s-a"), a);
    assert_eq!(
        days(&f.run_of("session", "s-b")),
        [("2026-11-18".to_owned(), 1)]
    );
    let total: u64 = f
        .runs()
        .iter()
        .flat_map(|(_, r)| days(r))
        .map(|(_, n)| n)
        .sum();
    assert_eq!(total, 3);
}

// spec: specs/ap/measure.md#R4
#[test]
fn r4_a_long_haiku_prompt_takes_the_long_rate() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    let haiku = "claude-haiku-5-5";
    bed.file(
        "accounts/claude-a/projects/p/s-h.jsonl",
        &[
            assistant(
                "s-h",
                "2026-11-18T10:00:00Z",
                &cwd,
                "h-long",
                haiku,
                200_000,
                json!([]),
            ),
            assistant(
                "s-h",
                "2026-11-19T10:00:00Z",
                &cwd,
                "h-short",
                haiku,
                50_000,
                json!([]),
            ),
        ],
        "2026-11-19T10:00:01Z",
    );
    bed.pass(&mut f, "2026-11-18");
    let run = f.run_of("session", "s-h");
    // 200,000 input tokens over the 100,000 threshold at 0.5 USD a million; 50,000 at 0.1.
    assert_eq!(day(&run, "2026-11-18")["usd_micros"], 100_000);
    assert_eq!(day(&run, "2026-11-19")["usd_micros"], 5_000);
    assert_eq!(run["model"], haiku);
    assert_eq!(run["input_tokens"], 250_000);
}

// spec: specs/ap/measure.md#R5
#[test]
fn r5_a_session_with_two_stage_records_is_two_runs() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let first = bed.stage(
        "units/u1/1.implementer.md.run.json",
        "implementer",
        "s-two",
        "2026-11-18T10:00:00Z",
    );
    let second = bed.stage(
        "units/u1/3.fixer.md.run.json",
        "fixer",
        "s-two",
        "2026-11-18T12:00:00Z",
    );
    let elsewhere = bed.dir.path("elsewhere");
    bed.file(
        "accounts/claude-a/projects/p/s-two.jsonl",
        &[
            msg("s-two", "2026-11-18T09:00:00Z", &elsewhere, "t0"),
            msg("s-two", "2026-11-18T11:00:00Z", &elsewhere, "t1"),
            msg("s-two", "2026-11-18T13:00:00Z", &elsewhere, "t2"),
        ],
        "2026-11-18T13:00:01Z",
    );
    let line = bed.pass(&mut f, "2026-11-18");
    assert_eq!(line["runs"]["recorded"], 2);
    let a = f.run_of("record", &first);
    let b = f.run_of("record", &second);
    assert_eq!(
        (a["role"].as_str(), b["role"].as_str()),
        (Some("implementer"), Some("fixer"))
    );
    assert_eq!(
        days(&a),
        [("2026-11-18".to_owned(), 2)],
        "a message before the first record is the first's"
    );
    assert_eq!(days(&b), [("2026-11-18".to_owned(), 1)]);
    assert_eq!(
        (a["started_at"].as_str(), a["ended_at"].as_str()),
        (Some("2026-11-18T09:00:00Z"), Some("2026-11-18T11:00:00Z"))
    );
}

// spec: specs/ap/measure.md#R5
#[test]
fn r5_a_session_of_a_temporary_or_other_directory_is_not_recorded() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    bed.file(
        "accounts/claude-a/projects/p/s-tmp.jsonl",
        &[msg(
            "s-tmp",
            "2026-11-18T10:00:00Z",
            "/tmp/eval-suite-x",
            "x1",
        )],
        "2026-11-18T10:00:01Z",
    );
    let elsewhere = bed.dir.path("elsewhere");
    bed.file(
        "accounts/claude-a/projects/p/s-other.jsonl",
        &[msg("s-other", "2026-11-18T10:00:00Z", &elsewhere, "x2")],
        "2026-11-18T10:00:01Z",
    );
    bed.file(
        "accounts/claude-a/projects/p/s-worker.jsonl",
        &[msg(
            "s-worker",
            "2026-11-18T10:00:00Z",
            &bed.work("task-a/wt/agent-platform"),
            "x3",
        )],
        "2026-11-18T10:00:01Z",
    );
    let line = bed.pass(&mut f, "2026-11-18");
    assert_eq!(line["skipped_route"], 2);
    assert_eq!(f.runs().len(), 1);
    assert_eq!(f.run_of("session", "s-worker")["role"], "worker, no record");
}

// spec: specs/ap/measure.md#R5
#[test]
fn r5_a_session_run_takes_the_intent_of_its_directory_or_its_lead() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    f.put(
        "intent",
        "by-dir",
        json!({"directory": bed.work("task-a"), "status": "confirmed"}),
    );
    f.put(
        "intent",
        "by-parent",
        json!({"directory": bed.dir.path("work"), "status": "confirmed"}),
    );
    f.put(
        "intent",
        "by-lead",
        json!({"lead_session": "s-led", "status": "confirmed"}),
    );
    let deep = bed.work("task-a/notes");
    bed.file(
        "accounts/claude-a/projects/p/s-dir.jsonl",
        &[msg("s-dir", "2026-11-18T10:00:00Z", &deep, "i1")],
        "2026-11-18T10:00:01Z",
    );
    bed.file(
        "accounts/claude-a/projects/p/s-led.jsonl",
        &[msg(
            "s-led",
            "2026-11-18T10:00:00Z",
            &bed.dir.path("platforms/agent-platform"),
            "i2",
        )],
        "2026-11-18T10:00:01Z",
    );
    bed.pass(&mut f, "2026-11-18");
    assert_eq!(
        f.run_of("session", "s-dir")["intent"],
        oid("intent", "by-dir")
    );
    assert_eq!(
        f.run_of("session", "s-led")["intent"],
        oid("intent", "by-lead")
    );
}

// spec: specs/ap/measure.md#R6
#[test]
fn r6_a_second_pass_applies_nothing_new_and_a_run_it_reads_nothing_of_is_unchanged() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let kept = json!({"role": "lead", "runtime": "claude", "model": OPUS, "status": "closed", "state": "session",
        "session": "s-gone", "started_at": "2026-11-10T10:00:00Z", "ended_at": "2026-11-10T11:00:00Z",
        "days": [{"day": "2026-11-10", "messages": 4, "usd_micros": 9, "refusals": 0, "file_reads": 0,
            "file_writes": 0, "store_reads": 0, "store_writes": 0}], "refusals": 0, "measured_at": "2026-11-11T00:00:00Z"});
    f.put("run", "act-gone", kept.clone());
    // A Codex stage's run has no session and another runtime; a Lead's run of start_run has no record.
    f.put("run", "act-codex", json!({"role": "implementer", "runtime": "codex", "model": "gpt-6.1-sol", "status": "closed", "started_at": "2026-11-18T10:00:00Z"}));
    f.put("run", "act-no-record", json!({"role": "lead", "runtime": "claude", "model": OPUS, "status": "running", "started_at": "2026-11-18T10:00:00Z", "intent": oid("intent", "x")}));
    let cwd = bed.work("task-a");
    bed.file(
        "accounts/claude-a/projects/p/s-new.jsonl",
        &[
            msg("s-new", "2026-11-18T10:00:00Z", &cwd, "n1"),
            msg("s-new", "2026-11-19T10:00:00Z", &cwd, "n2"),
        ],
        "2026-11-19T10:00:01Z",
    );
    let first = bed.pass(&mut f, "2026-11-18");
    assert_eq!(
        first["runs"],
        json!({"recorded": 1, "extended": 0, "measured": 1, "unchanged": 2})
    );
    let made = f.run_of("session", "s-new");
    let base = &hex("claude:s-new:")[..32];
    let applies = f.applies();
    assert_eq!(
        applies[0],
        ("record_session".to_owned(), format!("measure-{base}"))
    );
    assert_eq!(applies[1].0, "measure_run");
    let p = json!({"run": oid("run", "act-made-1"), "days": made["days"], "refusals": 0});
    assert_eq!(
        applies[1].1,
        format!("measure-{base}-{}", &hex(&p.to_string())[..16])
    );
    assert_eq!(f.objects[&("run".to_owned(), "act-gone".to_owned())], kept);
    let before = f.objects.clone();
    let second = bed.pass(&mut f, "2026-11-18");
    assert_eq!(
        second["runs"],
        json!({"recorded": 0, "extended": 0, "measured": 0, "unchanged": 3})
    );
    assert_eq!(f.objects, before);
    assert!(
        f.applies()[2..].iter().all(|(a, _)| a == "measure_run"),
        "only the same measure, already applied"
    );
    let searches: Vec<_> = f
        .calls
        .iter()
        .filter(|(a, b)| a[0] == "search" && b["object_type"] == "run")
        .collect();
    assert_eq!(searches.len(), 2, "one search of run a pass");
    assert_eq!(
        searches[0].1["where"],
        json!({"all": [{"eq": ["runtime", "claude"]}]})
    );
}

// spec: specs/ap/measure.md#R6
#[test]
fn r6_a_session_that_grew_is_extended_and_keeps_its_earlier_days() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    let path = bed.file(
        "accounts/claude-a/projects/p/s-grow.jsonl",
        &[
            msg("s-grow", "2026-11-17T10:00:00Z", &cwd, "g1"),
            msg("s-grow", "2026-11-18T10:00:00Z", &cwd, "g2"),
        ],
        "2026-11-18T10:00:01Z",
    );
    bed.pass(&mut f, "2026-11-17");
    let before = f.run_of("session", "s-grow");
    assert_eq!(before["input_tokens"], 2000);
    let mut text = std::fs::read_to_string(&path).unwrap();
    text += &format!("{}\n", msg("s-grow", "2026-11-19T10:00:00Z", &cwd, "g3"));
    std::fs::write(&path, text).unwrap();
    touch(&path, "2026-11-19T10:00:01Z");
    let line = bed.pass(&mut f, "2026-11-19");
    assert_eq!(
        (
            line["runs"]["extended"].as_u64(),
            line["runs"]["measured"].as_u64()
        ),
        (Some(1), Some(1))
    );
    let after = f.run_of("session", "s-grow");
    assert_eq!(after["ended_at"], "2026-11-19T10:00:00Z");
    assert_eq!(after["input_tokens"], 3000);
    assert_eq!(after["started_at"], before["started_at"]);
    assert_eq!(
        days(&after),
        [
            ("2026-11-17".to_owned(), 1),
            ("2026-11-18".to_owned(), 1),
            ("2026-11-19".to_owned(), 1)
        ]
    );
    assert!(
        f.applies()
            .iter()
            .any(|(a, r)| a == "extend_session" && r.contains("-x-"))
    );
}

// spec: specs/ap/measure.md#R6
#[test]
fn r6_a_run_across_midnight_has_two_days() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    bed.file(
        "accounts/claude-a/projects/p/s-night.jsonl",
        &[
            msg("s-night", "2026-11-18T23:59:00.500Z", &cwd, "d1"),
            msg("s-night", "2026-11-19T00:01:00Z", &cwd, "d2"),
        ],
        "2026-11-19T00:01:01Z",
    );
    bed.pass(&mut f, "2026-11-18");
    let run = f.run_of("session", "s-night");
    assert_eq!(
        days(&run),
        [("2026-11-18".to_owned(), 1), ("2026-11-19".to_owned(), 1)]
    );
    assert_eq!(
        (run["started_at"].as_str(), run["ended_at"].as_str()),
        (Some("2026-11-18T23:59:00Z"), Some("2026-11-19T00:01:00Z"))
    );
}

// spec: specs/ap/measure.md#R7
#[test]
fn r7_the_calls_of_a_code_mode_script_and_the_doors_lines_count_on_their_days() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let record = bed.stage(
        "units/u7/1.implementer.md.run.json",
        "implementer",
        "s-code",
        "2026-11-18T09:00:00Z",
    );
    let elsewhere = bed.dir.path("elsewhere");
    let e = elsewhere.as_str();
    let bash = |c: &str| json!({"command": c});
    bed.file(
        "accounts/claude-a/projects/p/s-code.jsonl",
        &[
            assistant("s-code", "2026-11-18T10:00:00Z", e, "c0", OPUS, 1000,
                json!([{"type": "tool_use", "id": "repl-1", "name": "REPL", "input": {"code": "SECRET"}}])),
            synthetic("s-code", "2026-11-18T10:00:01Z", e, "u-read", "Read", json!({"file_path": "/x"})),
            synthetic("s-code", "2026-11-18T10:00:01Z", e, "u-edit", "Edit", json!({"file_path": "/x"})),
            synthetic("s-code", "2026-11-18T10:00:02Z", e, "u-apply", "mcp__ont__ont_action_apply", json!({})),
            synthetic("s-code", "2026-11-18T10:00:02Z", e, "u-search", "mcp__ont__ont_search", json!({})),
            synthetic("s-code", "2026-11-18T10:00:02Z", e, "u-index", "mcp__ont__ont_index", json!({})),
            synthetic("s-code", "2026-11-18T10:00:03Z", e, "u-bash",  "Bash",
                bash("cd /x && git -C /x show HEAD:a && cat a > /dev/null; echo hi > out.txt | tee x\nap unit run k 2>&1 | tail -3")),
            synthetic("s-code", "2026-11-18T10:00:04Z", e, "u-bash-2", "Bash",
                bash("ont --store /s get < q.json; ap status --brief i; time ap request < r.json; sed -i s/a/b/ f; A=1 grep x y")),
            synthetic("s-code", "2026-11-18T10:00:05Z", e, "u-poll", "Poll", json!({})),
        ],
        "2026-11-18T10:00:06Z",
    );
    let door = |at: &str, run: &str, verb: &str, exit: Option<i32>| json!({"at": at, "run": run, "role": "implementer", "verb": verb, "name": "route", "ms": 5, "exit": exit, "reason": null});
    let lines = [
        door("2026-11-19T08:00:00Z", &record, "fn", Some(0)),
        door("2026-11-19T08:00:01Z", &record, "fn", None),
        door("2026-11-19T08:00:02Z", &record, "check", Some(0)),
        door(
            "2026-11-19T08:00:03Z",
            "/nowhere/other.run.json",
            "fn",
            Some(0),
        ),
        door("2026-11-17T08:00:00Z", &record, "fn", Some(0)),
    ];
    std::fs::write(
        bed.dir.at("door.jsonl"),
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )
    .unwrap();
    bed.pass(&mut f, "2026-11-18");
    let run = f.run_of("record", &record);
    let d18 = day(&run, "2026-11-18");
    // Read, git show, cat, tail, grep; Edit, the redirect to out.txt, tee, sed -i; the apply, ap unit run, ap request;
    // the search, ont get, ap status.
    assert_eq!(
        (
            d18["file_reads"].as_u64(),
            d18["file_writes"].as_u64(),
            d18["store_writes"].as_u64(),
            d18["store_reads"].as_u64()
        ),
        (Some(5), Some(4), Some(3), Some(3)),
        "{d18}"
    );
    assert_eq!(d18["messages"], 1);
    let d19 = day(&run, "2026-11-19");
    assert_eq!(
        (d19["store_reads"].as_u64(), d19["messages"].as_u64()),
        (Some(1), Some(0))
    );
    assert_eq!(
        days(&run).len(),
        2,
        "the door's line before --since counts on no day"
    );
    let again = bed.pass(&mut f, "2026-11-18");
    assert_eq!(again["runs"]["measured"], 0);
    assert_eq!(f.run_of("record", &record)["days"], run["days"]);
}

// spec: specs/ap/measure.md#R7
#[test]
fn r7_the_counts_of_one_call() {
    let c = measure::of_bash(
        "FOO=1 cd /a && /usr/local/bin/ap measure sessions | jq . >> /tmp/out ; echo x 2>&1 >/dev/null",
    );
    assert_eq!((c.store_writes, c.file_reads, c.file_writes), (1, 1, 1));
    let c = measure::of_bash("python3 -c 'print(1 > 2)'; git commit -m 'a; b' && git -c x=y log");
    assert_eq!(
        (c.file_writes, c.file_reads),
        (1, 1),
        "a `>` or `;` inside quotes is no redirect and no split"
    );
    let c =
        measure::of_bash("ont action apply < x.json; ont fn route; ap intent show; ap unit page k");
    assert_eq!((c.store_writes, c.store_reads), (2, 1));
    assert_eq!(
        measure::of_tool("mcp__ont__ont_index", &json!({})),
        measure::Counts::default()
    );
    assert_eq!(
        measure::of_tool("REPL", &json!({"code": "x"})),
        measure::Counts::default()
    );
    assert_eq!(measure::of_tool("Glob", &json!({})).file_reads, 1);
    assert_eq!(
        measure::of_tool("mcp__ont__ont_action_validate", &json!({})).store_reads,
        1
    );
}

// spec: specs/ap/measure.md#R7
#[test]
fn r7_a_refusal_counts_once_in_its_call_and_its_carrier_and_in_a_fork() {
    let found = measure::refusals(
        "x {\"error\":{\"code\":\"usage\",\"field\":\"args\",\"detail\":null}} y {\"code\":\"submission_criteria\",\"status\":\"refused\"}",
    );
    assert_eq!(found.len(), 2);
    let escaped =
        measure::refusals(r#"{"output": "{\"error\":{\"code\":\"usage\",\"field\":\"args\"}}"}"#);
    assert_eq!(
        escaped.get(&("usage".to_owned(), "args".to_owned())),
        Some(&1)
    );
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    let refusal = r#"{"error":{"code":"unknown_intent","field":"intent","detail":null}}"#;
    let ont = r#"{"status":"refused","code":"submission_criteria","result":{"failed":["x"]}}"#;
    let lines = [
        assistant(
            "s-r",
            "2026-11-18T10:00:00Z",
            &cwd,
            "r0",
            OPUS,
            1000,
            json!([{"type": "tool_use", "id": "repl-r", "name": "REPL", "input": {}}]),
        ),
        synthetic(
            "s-r",
            "2026-11-18T10:00:01Z",
            &cwd,
            "call-r",
            "Bash",
            json!({"command": "ap status x"}),
        ),
        result(
            "s-r",
            "2026-11-18T10:00:05.100Z",
            &cwd,
            "call-r",
            &format!("{refusal}\n{ont}"),
        ),
        result(
            "s-r",
            "2026-11-18T10:00:05.900Z",
            &cwd,
            "repl-r",
            &json!({"stderr": refusal}).to_string(),
        ),
    ];
    bed.file(
        "accounts/claude-a/projects/p/s-r.jsonl",
        &lines,
        "2026-11-18T10:00:06Z",
    );
    // A fork copies the results with their ids.
    let fork: Vec<Value> = lines
        .iter()
        .map(|l| {
            let mut l = l.clone();
            l["sessionId"] = json!("s-r2");
            l
        })
        .collect();
    bed.file(
        "accounts/claude-a/projects/p/s-r2.jsonl",
        &fork,
        "2026-11-18T10:00:07Z",
    );
    bed.pass(&mut f, "2026-11-18");
    let run = f.run_of("session", "s-r");
    assert_eq!(run["refusals"], 2);
    assert_eq!(day(&run, "2026-11-18")["refusals"], 2);
    assert!(
        f.runs().iter().all(|(_, r)| r["session"] != "s-r2"),
        "the fork owns nothing"
    );
}

// spec: specs/ap/measure.md#R8
#[test]
fn r8_the_line_holds_no_path_or_text_and_nothing_is_written() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    let cwd = bed.work("task-a");
    bed.file(
        "accounts/claude-a/projects/p/s-p.jsonl",
        &[
            msg("s-p", "2026-11-18T10:00:00Z", &cwd, "p1"),
            json!("not an object"),
        ],
        "2026-11-18T10:00:01Z",
    );
    std::fs::write(bed.dir.at("accounts/claude-a/.credentials.json"), "{}").unwrap();
    std::fs::set_permissions(
        bed.dir.at("accounts/claude-a/.credentials.json"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let tree = |d: &Path| {
        let mut out = vec![];
        let mut stack = vec![d.to_path_buf()];
        while let Some(p) = stack.pop() {
            for e in std::fs::read_dir(&p).unwrap().flatten() {
                let m = e.metadata().unwrap();
                if m.is_dir() {
                    stack.push(e.path());
                } else {
                    out.push((e.path(), m.len(), m.modified().unwrap()));
                }
            }
        }
        out.sort();
        out
    };
    let before = tree(&bed.dir.0);
    let line = bed.pass(&mut f, "2026-11-18");
    assert_eq!(tree(&bed.dir.0), before);
    let text = line.to_string();
    assert!(
        !text.contains(bed.dir.0.to_str().unwrap()) && !text.contains("SECRET"),
        "{text}"
    );
    assert_eq!(line["files"]["unreadable_lines"], 1);
    assert_eq!(line["at"], "2026-11-20T12:00:00Z");
    let made = f.run_of("session", "s-p");
    assert!(!made.to_string().contains(&cwd), "no cwd in a store value");
    // A failed search is an error of the pass's own, exit 3, with nothing applied.
    let mut broken = Fake::new(&bed.dir);
    broken.fail_search = true;
    let e = measure::run(&mut broken, &bed.args("2026-11-18")).unwrap_err();
    assert_eq!(measure::exit_of(&e), 3);
    assert!(broken.applies().is_empty());
}

// The function `cost`: runs of every role, two merged units, a batch run and an intent.
fn cost_store(bed: &Bed) -> Fake {
    let mut f = Fake::new(&bed.dir);
    let d = |day: &str, micros: i64, refusals: u64| {
        json!({"day": day, "messages": 2, "usd_micros": micros, "refusals": refusals, "file_reads": 0,
            "file_writes": 0, "store_reads": 0, "store_writes": 0})
    };
    f.put("intent", "gadget", json!({"status": "confirmed"}));
    f.put(
        "unit",
        "u1",
        json!({"state": "merged", "intent": oid("intent", "gadget"), "class": "plain"}),
    );
    f.put("unit", "u2", json!({"state": "merged", "class": "plain"}));
    let run = |role: &str,
               unit: Option<&str>,
               intent: Option<&str>,
               days: Value,
               measured: bool| {
        let mut r = json!({"role": role, "runtime": "claude", "model": OPUS, "status": "closed", "started_at": "2026-11-16T10:00:00Z", "days": days});
        if let Some(u) = unit {
            r["unit"] = oid("unit", u);
        }
        if let Some(i) = intent {
            r["intent"] = oid("intent", i);
        }
        if measured {
            r["measured_at"] = json!("2026-11-20T00:00:00Z");
        }
        r
    };
    f.put(
        "run",
        "r-impl",
        run(
            "implementer",
            Some("u1"),
            None,
            json!([d("2026-11-16", 3_000_000, 0), d("2026-11-25", 9_000_000, 0)]),
            true,
        ),
    );
    f.put(
        "run",
        "r-ver",
        run(
            "verifier",
            Some("u2"),
            None,
            json!([d("2026-11-17", 1_000_000, 1)]),
            true,
        ),
    );
    f.put(
        "run",
        "r-lead",
        run(
            "lead",
            None,
            Some("gadget"),
            json!([d("2026-11-16", 4_000_004, 2), d("2026-11-17", 1_000_000, 1)]),
            true,
        ),
    );
    f.put(
        "run",
        "r-writer",
        run(
            "writer",
            None,
            None,
            json!([d("2026-11-17", 1_000_000, 0)]),
            true,
        ),
    );
    f.put(
        "run",
        "r-cold",
        run("implementer", None, None, json!([]), false),
    );
    f.put("run", "r-batch", json!({"role": "batch", "runtime": "api", "model": OPUS, "status": "closed", "started_at": "2026-11-17T01:00:00Z", "settled_usd_micros": 1_500_000, "reserved_usd_micros": 4_000_000}));
    f.put("run", "r-batch-open", json!({"role": "batch", "runtime": "api", "model": OPUS, "status": "running", "started_at": "2026-11-17T02:00:00Z", "reserved_usd_micros": 250_000}));
    for (unit, at) in [
        ("u1", "2026-11-17T10:00:00Z"),
        ("u2", "2026-11-30T10:00:00Z"),
    ] {
        f.log.push(json!({"action_type": "start_unit", "submitted_at": "2026-11-15T10:00:00Z", "parameters": {"unit": oid("unit", unit)}}));
        f.log.push(json!({"action_type": "merge_unit", "submitted_at": at, "parameters": {"unit": oid("unit", unit)}}));
    }
    f
}
fn cost(f: &mut Fake, p: Value) -> Value {
    measure::answer(f, "cost", "/scratch/store.sqlite", &p).unwrap()
}

// spec: specs/ap/measure.md#R9
#[test]
fn r9_the_parameters_are_refused_by_member() {
    let bed = Bed::new();
    let mut f = cost_store(&bed);
    let cases = [
        (json!({"until": "2026-11-20"}), "invalid_input", "since"),
        (
            json!({"since": "2026-11-20", "until": "2026-11-19"}),
            "invalid_input",
            "until",
        ),
        (
            json!({"since": "2026-08-01", "until": "2026-11-01"}),
            "invalid_input",
            "until",
        ),
        (
            json!({"since": "2026-11-1", "until": "2026-11-20"}),
            "invalid_input",
            "since",
        ),
        (
            json!({"since": "2026-11-01", "until": "2026-11-20", "by": "model"}),
            "invalid_input",
            "by",
        ),
        (
            json!({"since": "2026-11-01", "until": "2026-11-20", "extra": 1}),
            "invalid_input",
            "extra",
        ),
        (
            json!({"since": "2026-11-01", "until": "2026-11-20", "intent": 3}),
            "invalid_input",
            "intent",
        ),
        (
            json!({"since": "2026-11-01", "until": "2026-11-20", "intent": "absent"}),
            "unknown_intent",
            "intent",
        ),
        (json!([1]), "invalid_input", "stdin"),
    ];
    for (p, code, field) in cases {
        let e = measure::answer(&mut f, "cost", "/scratch/store.sqlite", &p).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), (code, field), "{p}");
        assert_eq!(measure::exit_of(&e), 2);
    }
    assert!(f.applies().is_empty());
    // 92 days are a window; the refusals above came before any search.
    assert!(
        measure::answer(
            &mut f,
            "cost",
            "/scratch/store.sqlite",
            &json!({"since": "2026-08-20", "until": "2026-11-19"})
        )
        .is_ok()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .env("ONT_FN_NAME", "cost")
        .env("ONT_FN_STORE", bed.dir.path("no-store.sqlite"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .take()
                .unwrap()
                .write_all(br#"{"since":"2026-11-02","until":"2026-11-01"}"#)?;
            c.wait_with_output()
        })
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (e["error"]["code"].as_str(), e["error"]["field"].as_str()),
        (Some("invalid_input"), Some("until"))
    );
}

// spec: specs/ap/measure.md#R10
#[test]
fn r10_cost_by_role_per_merged_unit_and_the_leads_share() {
    let bed = Bed::new();
    let mut f = cost_store(&bed);
    let v = cost(
        &mut f,
        json!({"since": "2026-11-16", "until": "2026-11-22"}),
    );
    assert_eq!(v["by"], "role");
    let groups: Vec<(Value, Value, Value)> = v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["key"].clone(), g["usd"].clone(), g["runs"].clone()))
        .collect();
    assert_eq!(
        groups,
        [
            (json!("implementer"), json!(3.0), json!(1)),
            (json!("lead"), json!(5.0), json!(1)),
            (json!("verifier"), json!(1.0), json!(1)),
            (json!("writer"), json!(1.0), json!(1)),
        ]
    );
    assert_eq!(v["total_usd"], 10.0);
    assert_eq!(v["merged"], 1, "u2 merged after the window");
    assert_eq!(v["usd_a_pr"], 10.0);
    assert_eq!(v["usd_stages_a_pr"], 4.0);
    assert_eq!(v["usd_lead_a_pr"], 6.0);
    assert_eq!(v["lead_share"], 60.0);
    assert_eq!(v["lead_refusals_a_pr"], 3.0);
    assert_eq!(v["api"], json!({"settled_usd": 1.5, "reserved_usd": 0.25}));
    // r-cold and the two batch runs: a run of the credit is never measured by a pass.
    assert_eq!(v["unmeasured"], 3);
    assert_eq!(f.applies().len(), 0, "a function applies nothing");
    let none = cost(
        &mut f,
        json!({"since": "2026-11-01", "until": "2026-11-02"}),
    );
    assert_eq!(
        (
            none["merged"].as_u64(),
            &none["usd_a_pr"],
            &none["lead_share"]
        ),
        (Some(0), &Value::Null, &Value::Null)
    );
}

// spec: specs/ap/measure.md#R10
#[test]
fn r10_cost_by_intent_unit_and_day_and_of_one_intent() {
    let bed = Bed::new();
    let mut f = cost_store(&bed);
    let window = |by: &str| json!({"since": "2026-11-16", "until": "2026-11-22", "by": by});
    let keys = |v: &Value| -> Vec<(Value, Value)> {
        v["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| (g["key"].clone(), g["usd"].clone()))
            .collect()
    };
    let by_intent = cost(&mut f, window("intent"));
    assert_eq!(
        keys(&by_intent),
        [(json!("gadget"), json!(8.0)), (Value::Null, json!(2.0))],
        "u1's run counts in its unit's intent"
    );
    let by_unit = cost(&mut f, window("unit"));
    assert_eq!(
        keys(&by_unit),
        [
            (json!("u1"), json!(3.0)),
            (json!("u2"), json!(1.0)),
            (Value::Null, json!(6.0))
        ]
    );
    let by_day = cost(&mut f, window("day"));
    assert_eq!(
        keys(&by_day),
        [
            (json!("2026-11-16"), json!(7.0)),
            (json!("2026-11-17"), json!(3.0))
        ]
    );
    assert_eq!(by_day["groups"][1]["runs"], 3);
    let one = cost(
        &mut f,
        json!({"since": "2026-11-16", "until": "2026-11-22", "intent": "gadget"}),
    );
    assert_eq!(
        keys(&one),
        [
            (json!("implementer"), json!(3.0)),
            (json!("lead"), json!(5.0))
        ]
    );
    assert_eq!(
        (
            one["intent"].as_str(),
            one["merged"].as_u64(),
            one["unmeasured"].as_u64()
        ),
        (Some("gadget"), Some(1), Some(0))
    );
}

// spec: specs/ap/measure.md#R11
#[test]
fn r11_flow_and_share_are_not_built() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    for name in ["flow", "share"] {
        let e = measure::answer(&mut f, name, "/scratch/store.sqlite", &json!({})).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_str()),
            ("function_not_built", "ONT_FN_NAME")
        );
        let out = Command::new(env!("CARGO_BIN_EXE_ap"))
            .env("ONT_FN_NAME", name)
            .env("ONT_FN_STORE", bed.dir.path("no-store.sqlite"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{name}");
        let e: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(e["error"]["code"], "function_not_built");
        assert!(out.stdout.is_empty());
    }
    assert!(f.calls.is_empty());
}

// spec: specs/ap/run-object.md#R16
#[test]
fn run_object_r16_a_stage_run_is_measured_and_a_session_run_recorded_by_the_new_actions() {
    let schema: Value =
        serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
    let a = &schema["action_types"];
    for new in ["days", "refusals", "measured_at"] {
        assert!(
            schema["object_types"]["run"]["properties"][new].is_object(),
            "{new}"
        );
        for old in ["start_run", "close_run"] {
            assert!(a[old]["parameters"][new].is_null(), "{old} takes no {new}");
            assert!(
                !a[old]["rules"].to_string().contains(&format!("\"{new}\"")),
                "{old} sets no {new}"
            );
        }
    }
    assert_eq!(
        a["measure_run"]["rules"],
        json!([{"modify_object": "run", "set": {"days": "$days", "refusals": "$refusals", "measured_at": "$now"}}])
    );
    let made = &a["record_session"]["rules"][0];
    assert_eq!(
        (
            made["create_object"].as_str(),
            made["set"]["state"].as_str(),
            made["set"]["status"].as_str()
        ),
        (Some("run"), Some("session"), Some("closed"))
    );
    let codes: Vec<&str> = a["extend_session"]["submission_criteria"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"not_a_session_run"));
}

// The pass and `cost` against the real engine: a scratch store with the repository's schema, whose function
// `cost` runs a copy of the built `ap`.
// spec: specs/ap/measure.md#R6
// spec: specs/ap/measure.md#R10
#[test]
#[ignore = "needs the engine in ONT_BIN as uid 0; ./check runs it"]
fn engine_a_pass_measures_a_stage_run_and_cost_answers_from_it() {
    let ont = PathBuf::from(std::env::var("ONT_BIN").expect("ONT_BIN names the engine"));
    let bed = Bed::new();
    let root = &bed.dir.0;
    let ap = root.join("ap");
    std::fs::copy(env!("CARGO_BIN_EXE_ap"), &ap).unwrap();
    std::fs::set_permissions(&ap, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut schema: Value =
        serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
    assert_eq!(schema["functions"]["cost"]["program"], "/usr/local/bin/ap");
    schema["functions"]["cost"]["program"] = json!(ap.to_str().unwrap());
    std::fs::write(root.join("schema.json"), schema.to_string()).unwrap();
    let store = root.join("store.sqlite").to_str().unwrap().to_owned();
    let path = format!(
        "{}:{}",
        ont.parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let ont_call = |args: &[&str], input: &str| -> Value {
        let mut c = Command::new(&ont)
            .arg("--store")
            .arg(&store)
            .args(args)
            .env("PATH", &path)
            .env_remove("ONT_ROLE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        }
        let out = c.wait_with_output().unwrap();
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)))
    };
    let installed = ont_call(
        &[
            "schema",
            "install",
            root.join("schema.json").to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(installed["status"], "ok", "{installed}");
    let record = bed.stage(
        "units/u1/1.implementer.md.run.json",
        "implementer",
        "s-engine",
        "2026-11-10T09:00:00Z",
    );
    let apply = |action: &str, rid: &str, p: Value| -> Value {
        let v = ont_call(
            &["action", "apply"],
            &json!({"action_type": action, "request_id": rid, "parameters": p}).to_string(),
        );
        assert_eq!(v["status"], "ok", "{v}");
        v["result"].clone()
    };
    let started = apply(
        "start_run",
        "engine-start",
        json!({"role": "implementer", "runtime": "claude", "model": OPUS, "record": record, "session": "s-engine"}),
    );
    let run = started["edits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["change"] == "initial")
        .unwrap()["id"]
        .clone();
    apply(
        "close_run",
        "engine-close",
        json!({"run": run, "state": "ok", "input_tokens": 11, "output_tokens": 12}),
    );
    let get = |id: &Value| -> Value {
        let v = ont_call(&["get"], &json!({"id": id}).to_string());
        let o = &v["result"]["objects"][0];
        let fields = o["observations"].as_array().unwrap().last().unwrap()["fields"].clone();
        fields
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v["value"].clone()))
            .collect()
    };
    let closed = get(&run);
    let elsewhere = bed.dir.path("elsewhere");
    bed.file(
        "accounts/claude-a/projects/p/s-engine.jsonl",
        &[msg("s-engine", "2026-11-10T09:10:00Z", &elsewhere, "e1")],
        "2026-11-10T09:10:01Z",
    );
    let cwd = bed.work("task-a");
    bed.file(
        "accounts/claude-a/projects/p/s-lead.jsonl",
        &[
            msg("s-lead", "2026-11-10T09:20:00Z", &cwd, "e2"),
            msg("s-lead", "2026-11-10T09:30:00Z", &cwd, "e3"),
        ],
        "2026-11-10T09:30:01Z",
    );
    let pass = |extra: &[&str]| -> (Option<i32>, Value) {
        let mut args = bed.args("2026-11-10");
        args.extend(["--store", &store].iter().map(OsString::from));
        args.extend(extra.iter().map(OsString::from));
        let out = Command::new(&ap)
            .arg("measure")
            .args(&args)
            .env_clear()
            .env("PATH", &path)
            .env("HOME", bed.dir.path("home"))
            .env("AP_DOOR_LOG", bed.dir.path("door.jsonl"))
            .output()
            .unwrap();
        (
            out.status.code(),
            serde_json::from_slice(&out.stdout)
                .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr))),
        )
    };
    let (code, line) = pass(&[]);
    assert_eq!(code, Some(0), "{line}");
    assert_eq!(
        (
            line["runs"]["recorded"].as_u64(),
            line["runs"]["measured"].as_u64()
        ),
        (Some(1), Some(2)),
        "{line}"
    );
    let measured = get(&run);
    for k in [
        "ended_at",
        "input_tokens",
        "output_tokens",
        "role",
        "status",
        "started_at",
    ] {
        assert_eq!(measured[k], closed[k], "{k}");
    }
    assert_eq!(measured["days"][0]["usd_micros"], 4000);
    let (code, again) = pass(&[]);
    assert_eq!(
        (
            code,
            again["runs"]["measured"].as_u64(),
            again["runs"]["recorded"].as_u64()
        ),
        (Some(0), Some(0), Some(0))
    );
    let v = ont_call(
        &["fn", "cost"],
        r#"{"parameters": {"since": "2026-11-10", "until": "2026-11-10"}}"#,
    );
    assert_eq!(v["function"], "cost", "{v}");
    let answer = &v["answer"];
    let groups: Vec<(Value, Value)> = answer["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["key"].clone(), g["usd"].clone()))
        .collect();
    assert_eq!(
        groups,
        [
            (json!("implementer"), json!(0.0)),
            (json!("lead"), json!(0.01))
        ]
    );
    assert_eq!(answer["total_usd"], 0.01);
}
