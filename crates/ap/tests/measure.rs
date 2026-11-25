//! `ap measure sessions` and the functions `cost`, `flow` and `share` (`specs/ap/measure.md`) against a fake host:
//! invented session records, stage records and a door record in a scratch directory of each test, and a fake `ont`
//! whose store is a map in memory (search, get, log, render and the five actions of schema version 19 the passes apply). No test
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

mod roles_file;
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
    refuse: Vec<String>,
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
            refuse: vec![],
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
        // `ap measure sessions` and `kpis` claim `measure` (specs/ap/roles.md R3).
        assert_eq!(b["role"], "measure", "{b}");
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
            "record_measurement" => {
                let kpi = p["kpi"]["native_key"].as_str().unwrap().to_owned();
                if self.refuse.contains(&kpi) {
                    return reply(&json!({"status": "refused", "code": "invalid_parameter"}));
                }
                let latest = self
                    .objects
                    .iter()
                    .filter(|((k, _), f)| {
                        k == "measurement" && f["kpi"]["native_key"] == kpi.as_str()
                    })
                    .filter_map(|(_, f)| f["observed_at"].as_str().map(str::to_owned))
                    .max();
                if latest.is_some_and(|l| l.as_str() > p["observed_at"].as_str().unwrap()) {
                    return reply(&json!({"status": "refused", "code": "older_measurement"}));
                }
                self.made += 1;
                let key = format!("act-measurement-{}", self.made);
                let mut f = p.clone();
                f["recorded_at"] = json!(now);
                self.put("measurement", &key, f);
                let current = self.objects.get_mut(&("kpi".into(), kpi.clone())).unwrap();
                for m in ["value", "observed_at", "evidence"] {
                    current[m] = p[m].clone();
                }
                key
            }
            other => panic!("action {other}"),
        };
        let kind = if action == "record_measurement" {
            "measurement"
        } else {
            "run"
        };
        let edits = json!([{"change": if action == "record_session" || kind == "measurement" { "initial" } else { "update" }, "id": oid(kind, &key)}]);
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
            "render" => {
                let unit = input["args"]["unit"].as_str().unwrap_or_default();
                let kind = match input["view"].as_str() {
                    Some("reviews_of_unit") => "review",
                    _ => "run",
                };
                let keys: Vec<&String> = self
                    .objects
                    .iter()
                    .filter(|((k, _), f)| k == kind && f["unit"]["native_key"] == unit)
                    .map(|((_, key), _)| key)
                    .collect();
                let table: String = keys.iter().map(|k| format!("| {k} |\n")).collect();
                reply(
                    &json!({"status": "ok", "now": self.wall, "truncated": false, "markdown": format!("| Key |\n| --- |\n{table}"), "rows": keys.len()}),
                )
            }
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
    for args in [&[][..], &["kpi"][..], &["--since"][..]] {
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
    let usage = String::from_utf8(help.stdout).unwrap();
    assert!(usage.contains("Usage: ap measure sessions [--since <YYYY-MM-DD>]"));
    assert!(usage.contains("Usage: ap measure kpis [--day <YYYY-MM-DD>] [--store <path>]"));
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

// The functions `flow` and `share` and the pass `kpis`: one store of an intent with merged units, their reviews, runs
// of every role with counts, and the KPIs of the table.
fn d(day: &str, usd: i64, refusals: u64, [fr, sr, fw, sw]: [u64; 4]) -> Value {
    json!({"day": day, "messages": 1, "usd_micros": usd, "refusals": refusals, "file_reads": fr,
        "store_reads": sr, "file_writes": fw, "store_writes": sw})
}
fn flow_store(f: &mut Fake) {
    f.put("intent", "gadget", json!({"status": "confirmed"}));
    f.put("intent", "old", json!({"status": "met"}));
    f.put("intent", "other-intent", json!({"status": "confirmed"}));
    let unit = |intent: &str| json!({"state": "merged", "intent": oid("intent", intent), "class": "plain"});
    for (u, intent) in [
        ("u1", "gadget"),
        ("u2", "gadget"),
        ("u3", "gadget"),
        ("u4", "gadget"),
        ("u5", "other-intent"),
    ] {
        f.put("unit", u, unit(intent));
    }
    // (unit, start, merge, causes of its stops)
    for (u, start, merge, stops) in [
        (
            "u1",
            "2026-11-17T08:00:00Z",
            "2026-11-17T10:00:00Z",
            &[][..],
        ),
        (
            "u2",
            "2026-11-18T11:00:00Z",
            "2026-11-18T12:00:00Z",
            &["ci_failed", "page_cannot_hold"][..],
        ),
        (
            "u3",
            "2026-11-19T10:00:00Z",
            "2026-11-19T10:30:00Z",
            &["ci_failed"][..],
        ),
        (
            "u4",
            "2026-11-29T10:00:00Z",
            "2026-11-30T10:00:00Z",
            &["outside"][..],
        ),
        (
            "u5",
            "2026-11-17T08:00:00Z",
            "2026-11-17T09:00:00Z",
            &["elsewhere"][..],
        ),
    ] {
        f.log.push(json!({"action_type": "start_unit", "submitted_at": start, "parameters": {"unit": oid("unit", u)}}));
        f.log.push(json!({"action_type": "merge_unit", "submitted_at": merge, "parameters": {"unit": oid("unit", u)}}));
        for c in stops {
            f.log.push(json!({"action_type": "stop_unit", "submitted_at": start, "parameters": {"unit": oid("unit", u), "cause": c}}));
        }
    }
    // (key, unit, kind, status, verdict, closed_at)
    for (k, u, kind, status, verdict, at) in [
        (
            "p1a",
            "u1",
            "page",
            "closed",
            "repair",
            "2026-11-16T10:00:00Z",
        ),
        (
            "p1b",
            "u1",
            "page",
            "closed",
            "accept",
            "2026-11-16T12:00:00Z",
        ),
        (
            "p1c",
            "u1",
            "page",
            "closed",
            "accept",
            "2026-11-16T13:00:00Z",
        ),
        (
            "k1",
            "u1",
            "packet",
            "closed",
            "accept",
            "2026-11-17T09:00:00Z",
        ),
        (
            "p2a",
            "u2",
            "page",
            "closed",
            "accept",
            "2026-11-17T10:00:00Z",
        ),
        (
            "p2b",
            "u2",
            "page",
            "revoked",
            "accept",
            "2026-11-17T11:00:00Z",
        ),
        (
            "k2a",
            "u2",
            "packet",
            "closed",
            "repair",
            "2026-11-18T09:00:00Z",
        ),
        (
            "k2b",
            "u2",
            "packet",
            "closed",
            "accept",
            "2026-11-18T10:00:00Z",
        ),
        (
            "p3a",
            "u3",
            "page",
            "closed",
            "accept",
            "2026-11-18T10:00:00Z",
        ),
        (
            "p3b",
            "u3",
            "page",
            "closed",
            "accept",
            "2026-11-18T11:00:00Z",
        ),
        (
            "p4",
            "u4",
            "page",
            "closed",
            "accept",
            "2026-11-29T11:00:00Z",
        ),
        (
            "p5",
            "u5",
            "page",
            "closed",
            "repair",
            "2026-11-17T08:30:00Z",
        ),
    ] {
        f.put("review", k, json!({"unit": oid("unit", u), "kind": kind, "status": status, "verdict": verdict, "closed_at": at}));
    }
    // The runs: the Lead (a day before the window, a day in it), the stages of u1 and u2, a page writer, a session of
    // no intent, a verifier with no step, and a run of the API credit with a day (no group counts it).
    let run = |role: &str, days: Value| json!({"role": role, "runtime": "claude", "model": OPUS, "status": "closed", "started_at": "2026-11-15T10:00:00Z", "measured_at": "2026-11-20T00:00:00Z", "days": days});
    let with = |mut r: Value, k: &str, v: Value| {
        r[k] = v;
        r
    };
    f.put(
        "run",
        "r-lead",
        with(
            run(
                "lead",
                json!([
                    d("2026-11-15", 9_000_000, 9, [100, 100, 100, 100]),
                    d("2026-11-17", 4_000_000, 2, [6, 2, 3, 1])
                ]),
            ),
            "intent",
            oid("intent", "gadget"),
        ),
    );
    f.put(
        "run",
        "r-writer",
        with(
            run(
                "writer",
                json!([d("2026-11-17", 1_000_000, 0, [0, 4, 0, 5])]),
            ),
            "intent",
            oid("intent", "gadget"),
        ),
    );
    f.put(
        "run",
        "r-impl",
        with(
            run(
                "implementer",
                json!([d("2026-11-17", 3_000_000, 0, [5, 0, 7, 0])]),
            ),
            "unit",
            oid("unit", "u1"),
        ),
    );
    f.put(
        "run",
        "r-ver",
        with(
            run(
                "verifier",
                json!([d("2026-11-18", 1_000_000, 0, [0, 0, 0, 0])]),
            ),
            "unit",
            oid("unit", "u2"),
        ),
    );
    f.put(
        "run",
        "r-inter",
        run(
            "interactive",
            json!([d("2026-11-18", 500_000, 0, [10, 10, 0, 0])]),
        ),
    );
    f.put("run", "r-batch", json!({"role": "batch", "runtime": "api", "model": OPUS, "status": "closed", "started_at": "2026-11-17T01:00:00Z", "settled_usd_micros": 1_500_000, "reserved_usd_micros": 4_000_000, "days": [d("2026-11-17", 0, 0, [1, 1, 1, 1])]}));
}
const WINDOW: &str = "2026-11-16..2026-11-22";
fn flow(f: &mut Fake, p: Value) -> Value {
    measure::answer(f, "flow", "/scratch/store.sqlite", &p).unwrap()
}
fn share(f: &mut Fake, p: Value) -> Value {
    measure::answer(f, "share", "/scratch/store.sqlite", &p).unwrap()
}
fn window(intent: &str) -> Value {
    json!({"intent": intent, "since": "2026-11-16", "until": "2026-11-22"})
}

// spec: specs/ap/measure.md#R11
#[test]
fn r11_flow_answers_the_merged_units_of_the_window() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    flow_store(&mut f);
    let a = flow(&mut f, window("gadget"));
    // u1, u2 and u3 merged in the window; u4 merged after it and u5 belongs to another intent.
    assert_eq!(
        a,
        json!({"intent": "gadget", "since": "2026-11-16", "until": "2026-11-22", "merged": 3,
            "merged_a_day": 0.43, "median_minutes": 60, "packet_first_accept": 0.5, "units_stopped": 2,
            "stops": {"ci_failed": 2, "page_cannot_hold": 1},
            // u1 three looks, u2 one (its revoked look is none), u3 two accepts
            "page_looks": 2.0, "looks_after_accept": 0.67, "first_accept": 0.667, "log_cut": false})
    );
    // A window that holds no merged unit.
    let none = flow(
        &mut f,
        json!({"intent": "gadget", "since": "2026-12-01", "until": "2026-12-07"}),
    );
    assert_eq!(none["merged"], 0);
    assert_eq!(none["merged_a_day"], 0.0);
    assert_eq!(none["stops"], json!({}));
    assert_eq!(none["units_stopped"], 0);
    for m in [
        "median_minutes",
        "packet_first_accept",
        "page_looks",
        "looks_after_accept",
        "first_accept",
    ] {
        assert!(none[m].is_null(), "{m}");
    }
    // One day is a window of its own: u1's day alone.
    let day = flow(
        &mut f,
        json!({"intent": "gadget", "since": "2026-11-17", "until": "2026-11-17"}),
    );
    assert_eq!(
        (
            day["merged"].as_u64(),
            day["merged_a_day"].as_f64(),
            day["median_minutes"].as_u64()
        ),
        (Some(1), Some(1.0), Some(120))
    );
    assert_eq!(
        (
            day["page_looks"].as_f64(),
            day["looks_after_accept"].as_f64(),
            day["first_accept"].as_f64()
        ),
        (Some(3.0), Some(1.0), Some(0.0))
    );
}

// spec: specs/ap/measure.md#R11
#[test]
fn r11_a_log_the_store_cut_marks_the_answer() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    flow_store(&mut f);
    f.log.retain(|r| {
        !(r["action_type"] == "start_unit" && r["parameters"]["unit"]["native_key"] == "u3")
    });
    let a = flow(&mut f, window("gadget"));
    assert_eq!(
        (
            a["merged"].as_u64(),
            a["log_cut"].as_bool(),
            a["median_minutes"].as_u64()
        ),
        (Some(3), Some(true), Some(90))
    );
}

// spec: specs/ap/measure.md#R11
// spec: specs/ap/measure.md#R9
#[test]
fn r11_flow_refuses_its_parameters_by_member_and_reads_nothing_of_the_host() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    flow_store(&mut f);
    let ok = window("gadget");
    let with = |m: &str, v: Value| {
        let mut p = ok.clone();
        p[m] = v;
        p
    };
    for (p, field) in [
        (
            json!({"since": "2026-11-16", "until": "2026-11-22"}),
            "intent",
        ),
        (with("intent", json!("")), "intent"),
        (with("intent", json!(7)), "intent"),
        (with("since", json!("2026-11-23")), "until"),
        (with("since", json!("2026-02-30")), "since"),
        (with("until", json!("2027-03-01")), "until"),
        (with("by", json!("day")), "by"),
        (json!([]), "stdin"),
    ] {
        let e = measure::answer(&mut f, "flow", "/scratch/store.sqlite", &p).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", field), "{p}");
        assert_eq!(measure::exit_of(&e), 2);
    }
    let e =
        measure::answer(&mut f, "flow", "/scratch/store.sqlite", &window("absent")).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("unknown_intent", "intent"));
    assert_eq!(measure::exit_of(&e), 2);
    assert!(f.applies().is_empty());
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .env("ONT_FN_NAME", "flow")
        .env("ONT_FN_STORE", bed.dir.path("no-store.sqlite"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(e["error"]["code"], "invalid_input");
    assert!(out.stdout.is_empty());
}

/// One unit of `tier_store`: its key, kind, implementer run (runtime, model, messages), packet verdicts, stop causes and fixers.
type TierUnit = (
    &'static str,
    Option<&'static str>,
    Option<(&'static str, &'static str, u64)>,
    &'static [&'static str],
    &'static [&'static str],
    u32,
);
/// A store of merged units that different implementers ran, for `flow` by tier: each row is the unit, its kind, the
/// implementer's run (runtime, model, messages) or none, the verdicts of its packet reviews, its stops and its fixers.
fn tier_store(f: &mut Fake) {
    f.put("intent", "gadget", json!({"status": "confirmed"}));
    let rows: [TierUnit; 7] = [
        (
            "t1",
            Some("rust"),
            Some(("claude", OPUS, 30)),
            &["accept"],
            &[],
            0,
        ),
        (
            "t2",
            Some("rust"),
            Some(("claude", OPUS, 30)),
            &["repair", "accept"],
            &[],
            1,
        ),
        (
            "t3",
            Some("rust"),
            Some(("claude", OPUS, 30)),
            &["accept"],
            &["ci_failed"],
            0,
        ),
        (
            "t4",
            Some("script"),
            Some(("agy", "agy-model", 4)),
            &["accept"],
            &[],
            0,
        ),
        ("t5", None, Some(("claude", OPUS, 9)), &["accept"], &[], 2),
        ("t6", Some("rust"), None, &["repair", "accept"], &[], 0),
        (
            "t7",
            Some("rust"),
            Some(("claude", OPUS, 1)),
            &[],
            &["a", "b"],
            1,
        ),
    ];
    for (u, kind, ran, verdicts, stops, fixers) in rows {
        let mut unit =
            json!({"state": "merged", "intent": oid("intent", "gadget"), "class": "plain"});
        if let Some(k) = kind {
            unit["kind"] = json!(k);
        }
        f.put("unit", u, unit);
        f.log.push(json!({"action_type": "start_unit", "submitted_at": "2026-11-17T08:00:00Z", "parameters": {"unit": oid("unit", u)}}));
        f.log.push(json!({"action_type": "merge_unit", "submitted_at": "2026-11-17T09:00:00Z", "parameters": {"unit": oid("unit", u)}}));
        for c in stops {
            f.log.push(json!({"action_type": "stop_unit", "submitted_at": "2026-11-17T08:30:00Z", "parameters": {"unit": oid("unit", u), "cause": c}}));
        }
        for (i, v) in verdicts.iter().enumerate() {
            f.put("review", &format!("{u}-k{i}"), json!({"unit": oid("unit", u), "kind": "packet", "status": "closed", "verdict": v, "closed_at": format!("2026-11-17T08:{:02}:00Z", 10 + i)}));
        }
        let run = |role: &str, runtime: &str, model: &str, messages: u64| json!({"role": role, "runtime": runtime, "model": model, "status": "closed", "started_at": "2026-11-17T08:00:00Z", "unit": oid("unit", u), "days": [{"day": "2026-11-17", "messages": messages}]});
        if let Some((runtime, model, messages)) = ran {
            f.put(
                "run",
                &format!("{u}-impl"),
                run("implementer", runtime, model, messages),
            );
        }
        for i in 0..fixers {
            f.put(
                "run",
                &format!("{u}-fix{i}"),
                run("fixer", "claude", OPUS, 3),
            );
        }
    }
}
fn by_tier(window: Value) -> Value {
    let mut p = window;
    p["by"] = json!("tier");
    p
}

// spec: specs/ap/measure.md#R17
#[test]
fn r17_tier_flow_groups_the_merged_units_by_implementer_and_kind() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    tier_store(&mut f);
    let base = flow(&mut f, window("gadget"));
    assert!(
        base.get("groups").is_none(),
        "without by the answer has no groups: {base}"
    );
    let a = flow(&mut f, by_tier(window("gadget")));
    // Nothing but the member `groups` is added.
    let mut without = a.clone();
    let groups = without.as_object_mut().unwrap().remove("groups").unwrap();
    assert_eq!(without, base);
    let group = |runtime: Value, model: Value, kind: Value, m: [Value; 6]| {
        let [merged, first_accept, stopped, repairs, either, rate] = m;
        json!({"runtime": runtime, "model": model, "kind": kind, "merged": merged,
            "first_accept": first_accept, "units_stopped": stopped, "repairs": repairs,
            "stopped_or_repaired": either, "stop_repair_rate": rate})
    };
    // By runtime, model, kind, a null last in its member: the claude unit with no kind (t5) after the claude units of
    // kind rust, and the unit with no implementer run (t6) after every group that has one.
    assert_eq!(
        groups,
        json!([
            group(
                json!("agy"),
                json!("agy-model"),
                json!("script"),
                [
                    json!(1),
                    json!(1.0),
                    json!(0),
                    json!(0),
                    json!(0),
                    json!(0.0)
                ]
            ),
            group(
                json!("claude"),
                json!(OPUS),
                json!("rust"),
                [
                    json!(4),
                    json!(0.5),
                    json!(2),
                    json!(2),
                    json!(3),
                    json!(0.75)
                ]
            ),
            group(
                json!("claude"),
                json!(OPUS),
                Value::Null,
                [
                    json!(1),
                    json!(1.0),
                    json!(0),
                    json!(2),
                    json!(1),
                    json!(1.0)
                ]
            ),
            group(
                Value::Null,
                Value::Null,
                json!("rust"),
                [
                    json!(1),
                    json!(0.0),
                    json!(0),
                    json!(0),
                    json!(0),
                    json!(0.0)
                ]
            ),
        ])
    );
    // Nothing is applied.
    assert!(f.applies().is_empty());
}

// spec: specs/ap/measure.md#R17
#[test]
fn r17_tier_flow_has_no_group_without_a_merged_unit_and_refuses_another_by() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    tier_store(&mut f);
    let none = flow(
        &mut f,
        by_tier(json!({"intent": "gadget", "since": "2026-12-01", "until": "2026-12-02"})),
    );
    assert_eq!(none["merged"], 0);
    assert_eq!(none["groups"], json!([]));
    for by in [
        json!("role"),
        json!("day"),
        json!("Tier"),
        json!(7),
        json!(["tier"]),
    ] {
        let mut p = window("gadget");
        p["by"] = by.clone();
        let e = measure::answer(&mut f, "flow", "/scratch/store.sqlite", &p).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", "by"), "{by}");
    }
    // `share` has no such member.
    let mut p = json!({"since": "2026-11-16", "until": "2026-11-22"});
    p["by"] = json!("tier");
    let e = measure::answer(&mut f, "share", "/scratch/store.sqlite", &p).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "by"));
}

// spec: specs/ap/measure.md#R12
#[test]
fn r12_share_by_role_and_by_group_with_the_null_shares() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    flow_store(&mut f);
    let row = |role: &str, runs: u64, c: [u64; 4], r: Value, w: Value| {
        json!({"role": role, "runs": runs, "file_reads": c[0], "store_reads": c[1], "file_writes": c[2],
            "store_writes": c[3], "read_share": r, "write_share": w})
    };
    let group = |g: &str, runs: u64, c: [u64; 4], r: Value, w: Value| {
        let mut v = row(g, runs, c, r, w);
        v["group"] = v["role"].take();
        v.as_object_mut().unwrap().remove("role");
        v
    };
    let a = share(
        &mut f,
        json!({"since": "2026-11-16", "until": "2026-11-22"}),
    );
    assert_eq!(
        a,
        json!({"since": "2026-11-16", "until": "2026-11-22", "intent": null,
        "roles": [
            row("batch", 1, [1, 1, 1, 1], json!(50.0), json!(50.0)),
            row("implementer", 1, [5, 0, 7, 0], json!(0.0), json!(0.0)),
            row("interactive", 1, [10, 10, 0, 0], json!(50.0), Value::Null),
            row("lead", 1, [6, 2, 3, 1], json!(25.0), json!(25.0)),
            row("verifier", 1, [0, 0, 0, 0], Value::Null, Value::Null),
            row("writer", 1, [0, 4, 0, 5], json!(100.0), json!(100.0)),
        ],
        "groups": [
            group("stages", 3, [5, 4, 7, 5], json!(44.4), json!(41.7)),
            group("sessions", 2, [16, 12, 3, 1], json!(42.9), json!(25.0)),
        ]})
    );
    // Of one intent: the runs of its own and of its units (the session of no intent and the batch run are out).
    let g = share(
        &mut f,
        json!({"since": "2026-11-16", "until": "2026-11-22", "intent": "gadget"}),
    );
    assert_eq!(g["intent"], "gadget");
    assert_eq!(
        g["groups"][0],
        group("stages", 3, [5, 4, 7, 5], json!(44.4), json!(41.7))
    );
    assert_eq!(
        g["groups"][1],
        group("sessions", 1, [6, 2, 3, 1], json!(25.0), json!(25.0))
    );
    // A window without a run: no role, every share null.
    let none = share(
        &mut f,
        json!({"since": "2026-12-01", "until": "2026-12-07"}),
    );
    assert_eq!(none["roles"], json!([]));
    assert_eq!(
        none["groups"][0],
        group("stages", 0, [0, 0, 0, 0], Value::Null, Value::Null)
    );
    assert_eq!(
        none["groups"][1],
        group("sessions", 0, [0, 0, 0, 0], Value::Null, Value::Null)
    );
    let e = measure::answer(
        &mut f,
        "share",
        "/scratch/store.sqlite",
        &json!({"since": "2026-11-16", "until": "2026-11-22", "intent": "absent"}),
    )
    .unwrap_err();
    assert_eq!(e.code, "unknown_intent");
    let e = measure::answer(
        &mut f,
        "share",
        "/scratch/store.sqlite",
        &json!({"since": "2026-11-16", "until": "2026-11-22", "by": "role"}),
    )
    .unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "by"));
    assert!(f.applies().is_empty());
}

// spec: specs/ap/measure.md#R13
#[test]
fn r13_a_run_across_the_windows_edge_counts_only_its_days_inside_it() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    flow_store(&mut f);
    // The Lead's run has a day of 100 each on 2026-11-15 and a day in the window.
    let before = share(
        &mut f,
        json!({"since": "2026-11-15", "until": "2026-11-15"}),
    );
    assert_eq!(before["roles"][0]["role"], "lead");
    assert_eq!(before["roles"][0]["file_reads"], 100);
    let inside = share(
        &mut f,
        json!({"since": "2026-11-16", "until": "2026-11-22"}),
    );
    let lead = inside["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"] == "lead")
        .unwrap()
        .clone();
    assert_eq!(
        (
            lead["runs"].as_u64(),
            lead["file_reads"].as_u64(),
            lead["store_writes"].as_u64()
        ),
        (Some(1), Some(6), Some(1))
    );
    let both = share(
        &mut f,
        json!({"since": "2026-11-15", "until": "2026-11-22"}),
    );
    let lead = both["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"] == "lead")
        .unwrap()
        .clone();
    assert_eq!(
        (
            lead["runs"].as_u64(),
            lead["file_reads"].as_u64(),
            lead["store_writes"].as_u64()
        ),
        (Some(1), Some(106), Some(101))
    );
    // An entry that lacks a count counts 0; the run is still counted.
    f.put("run", "r-old", json!({"role": "lead", "runtime": "claude", "days": [{"day": "2026-11-17", "messages": 1, "usd_micros": 0, "refusals": 0}]}));
    let old = share(
        &mut f,
        json!({"since": "2026-11-17", "until": "2026-11-17"}),
    );
    let lead = old["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"] == "lead")
        .unwrap()
        .clone();
    assert_eq!(
        (lead["runs"].as_u64(), lead["file_reads"].as_u64()),
        (Some(2), Some(6))
    );
    assert!(f.applies().is_empty());
}

fn kpi_store(f: &mut Fake) {
    flow_store(f);
    f.wall = epoch("2026-11-23T00:20:00Z");
    f.put("intent", &"x".repeat(300), json!({"status": "confirmed"}));
    let kpi = |name: &str, intent: Option<&str>| {
        let mut k = json!({"task": oid("task", "ws-test"), "name": name, "definition": "d"});
        if let Some(i) = intent {
            k["intent"] = oid("intent", i);
        }
        k
    };
    for (name, _, _) in measure::TABLE {
        f.put("kpi", &format!("k-{name}"), kpi(name, Some("gadget")));
    }
    f.put("kpi", "k-closed", kpi("usd_a_pr", Some("old")));
    f.put("kpi", "k-gate", kpi("gate", Some("gadget")));
    f.put("kpi", "k-noint", kpi("usd_a_pr", None));
}
fn kpis(f: &mut Fake, args: &[&str]) -> (Value, u8) {
    let argv: Vec<OsString> = std::iter::once("kpis")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    measure::run(f, &argv).unwrap()
}
fn measurements(f: &Fake) -> Vec<(String, Value)> {
    f.objects
        .iter()
        .filter(|((k, _), _)| k == "measurement")
        .map(|((_, key), v)| (key.clone(), v.clone()))
        .collect()
}
fn value_of(line: &Value, kpi: &str) -> String {
    let found: Vec<&Value> = line["measured"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kpi"] == format!("k-{kpi}").as_str())
        .collect();
    assert_eq!(found.len(), 1, "{kpi}: {line}");
    found[0]["value"].as_str().unwrap().to_owned()
}

// spec: specs/ap/measure.md#R14
// spec: specs/ap/measure.md#R15
#[test]
fn r15_a_pass_records_one_measurement_of_each_kpi_of_a_confirmed_intent() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    kpi_store(&mut f);
    let (line, status) = kpis(&mut f, &[]);
    assert_eq!(status, 0, "{line}");
    assert_eq!(
        (
            line["at"].as_str(),
            line["day"].as_str(),
            line["since"].as_str(),
            line["until"].as_str()
        ),
        (
            Some("2026-11-23T00:20:00Z"),
            Some("2026-11-23"),
            Some("2026-11-16"),
            Some("2026-11-22")
        )
    );
    // The KPI of a closed intent, the one of a name outside the table and the one with no intent are skipped.
    assert_eq!(
        (
            line["measured"].as_array().unwrap().len(),
            line["unchanged"].as_u64(),
            line["skipped"].as_u64()
        ),
        (13, Some(0), Some(3))
    );
    assert_eq!(line["failed"], json!([]));
    for (kpi, text) in [
        ("usd_a_pr", "3.0 (WINDOW, 3 merged)"),
        ("usd_stages", "1.33 (WINDOW, 3 merged)"),
        ("usd_lead", "1.67 (WINDOW, 3 merged)"),
        ("lead_share", "55.6 (WINDOW, 3 merged)"),
        ("lead_refusals", "0.67 (WINDOW, 3 merged)"),
        ("minutes_to_merge", "60 (WINDOW, 3 merged)"),
        ("merged_a_day", "0.43 (WINDOW, 3 merged)"),
        (
            "stops",
            "2 of 3 stopped: ci_failed 2, page_cannot_hold 1 (WINDOW)",
        ),
        ("page_looks", "2.0 (WINDOW, 3 merged)"),
        ("looks_after_accept", "0.67 (WINDOW, 3 merged)"),
        ("first_accept", "0.667 (WINDOW, 3 merged)"),
        ("store_writes", "sessions 25.0 % stages 41.7 % (WINDOW)"),
        ("store_queries", "sessions 25.0 % stages 44.4 % (WINDOW)"),
    ] {
        assert_eq!(
            value_of(&line, kpi),
            text.replace("WINDOW", WINDOW),
            "{kpi}"
        );
    }
    // What is stored: the measurement of each KPI, its evidence the call of its function, its request id of the day.
    let stored = measurements(&f);
    assert_eq!(stored.len(), 13);
    let of = |kpi: &str| -> Value {
        let found: Vec<_> = stored
            .iter()
            .filter(|(_, m)| m["kpi"]["native_key"] == format!("k-{kpi}").as_str())
            .collect();
        assert_eq!(found.len(), 1, "{kpi}");
        found[0].1.clone()
    };
    let p = r#"{"intent":"gadget","since":"2026-11-16","until":"2026-11-22"}"#;
    for (kpi, function) in [
        ("usd_a_pr", "cost"),
        ("first_accept", "flow"),
        ("store_queries", "share"),
    ] {
        let m = of(kpi);
        assert_eq!(
            m["evidence"],
            format!("ont fn {function} {p}").as_str(),
            "{kpi}"
        );
        assert_eq!(m["observed_at"], "2026-11-23T00:20:00Z");
        assert_eq!(
            f.objects[&("kpi".to_owned(), format!("k-{kpi}"))]["value"],
            m["value"],
            "{kpi} is the KPI's value"
        );
    }
    let rids: Vec<String> = f
        .applies()
        .into_iter()
        .map(|(a, r)| {
            assert_eq!(a, "record_measurement");
            r
        })
        .collect();
    assert!(
        rids.contains(&"measure-k-stops-20261123".to_owned()),
        "{rids:?}"
    );
    assert_eq!(rids.len(), 13);
    assert!(
        f.applies()
            .iter()
            .all(|(_, r)| !r.contains("closed") && !r.contains("gate") && !r.contains("noint"))
    );
    // Each function ran once for the intent: `cost` and `flow` each read the merged units' log once.
    let merges = f
        .calls
        .iter()
        .filter(|(a, b)| a[0] == "log" && b["action_type"] == "merge_unit")
        .count();
    assert_eq!(merges, 2);
    // No answer holds a path or a message's text.
    assert!(!line.to_string().contains('/'));
}

// spec: specs/ap/measure.md#R15
#[test]
fn r15_a_second_pass_of_a_day_adds_no_measurement_even_after_a_run_grew() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    kpi_store(&mut f);
    let (first, _) = kpis(&mut f, &[]);
    assert_eq!(first["measured"].as_array().unwrap().len(), 13);
    // The same second, the same values: the engine answers already_applied.
    let (again, status) = kpis(&mut f, &[]);
    assert_eq!(
        (
            status,
            again["measured"].as_array().unwrap().len(),
            again["unchanged"].as_u64()
        ),
        (0, 0, Some(13))
    );
    // A run grew and the clock moved: the same request ids with other parameters are request_id_reused.
    let lead = f
        .objects
        .get_mut(&("run".to_owned(), "r-lead".to_owned()))
        .unwrap();
    lead["days"][1] = d("2026-11-17", 40_000_000, 2, [60, 20, 3, 1]);
    f.wall += 3600;
    let (later, status) = kpis(&mut f, &[]);
    assert_eq!(status, 0, "{later}");
    assert_eq!(
        (
            later["measured"].as_array().unwrap().len(),
            later["unchanged"].as_u64(),
            later["failed"].clone()
        ),
        (0, Some(13), json!([]))
    );
    assert_eq!(measurements(&f).len(), 13);
    assert_eq!(
        f.objects[&("kpi".to_owned(), "k-usd_a_pr".to_owned())]["value"],
        format!("3.0 ({WINDOW}, 3 merged)").as_str()
    );
    // Another day is another request id: a pass for the day before measures again, each KPI twice in all.
    f.wall += 3600;
    let (before, _) = kpis(&mut f, &["--day", "2026-11-22"]);
    assert_eq!(before["measured"].as_array().unwrap().len(), 13);
    assert_eq!(
        (before["since"].as_str(), before["until"].as_str()),
        (Some("2026-11-15"), Some("2026-11-21"))
    );
    assert_eq!(measurements(&f).len(), 26);
}

// spec: specs/ap/measure.md#R14
#[test]
fn r14_the_options_are_checked_before_any_effect() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    kpi_store(&mut f);
    for args in [
        &["--day"][..],
        &["--store"],
        &["--since", "2026-11-01"],
        &["--day", "2026-11-20", "--day", "2026-11-21"],
        &["extra"],
    ] {
        let argv: Vec<OsString> = std::iter::once("kpis")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        let e = measure::run(&mut f, &argv).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{args:?}");
        assert_eq!(measure::exit_of(&e), 2);
    }
    for (args, field) in [
        (&["--day", "2026-11-24"][..], "day"),
        (&["--day", "2026-02-30"], "day"),
        (&["--day", "yesterday"], "day"),
        (&["--day", "2026-11-1"], "day"),
        (&["--store", "relative.sqlite"], "store"),
    ] {
        let argv: Vec<OsString> = std::iter::once("kpis")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        let e = measure::run(&mut f, &argv).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_str()),
            ("invalid_input", field),
            "{args:?}"
        );
        assert_eq!(measure::exit_of(&e), 2);
    }
    assert!(f.calls.is_empty());
    // Today and the days before it are allowed.
    for day in ["2026-11-23", "2026-11-01"] {
        let (line, status) = kpis(&mut f, &["--day", day]);
        assert_eq!((status, line["day"].as_str()), (0, Some(day)));
    }
    let (line, _) = kpis(&mut f, &["--day", "2026-11-01"]);
    assert_eq!(
        (line["since"].as_str(), line["until"].as_str()),
        (Some("2026-10-25"), Some("2026-10-31"))
    );
}

// spec: specs/ap/measure.md#R15
#[test]
fn r15_a_failed_measurement_does_not_end_the_pass() {
    let bed = Bed::new();
    let mut f = Fake::new(&bed.dir);
    kpi_store(&mut f);
    f.refuse = vec!["k-stops".to_owned()];
    let (line, status) = kpis(&mut f, &[]);
    assert_eq!(status, 3);
    assert_eq!(line["measured"].as_array().unwrap().len(), 12);
    assert_eq!(
        line["failed"],
        json!([{"kpi": "k-stops", "name": "stops", "code": "invalid_parameter"}])
    );
    assert_eq!(measurements(&f).len(), 12);
    // A KPI whose intent's key makes the evidence over 300 bytes is failed and not sent.
    let long = "x".repeat(300);
    let mut g = Fake::new(&bed.dir);
    g.wall = epoch("2026-11-23T00:20:00Z");
    g.put("intent", &long, json!({"status": "confirmed"}));
    g.put("kpi", "k-long", json!({"task": oid("task", "ws-test"), "name": "usd_a_pr", "definition": "d", "intent": oid("intent", &long)}));
    let (line, status) = kpis(&mut g, &[]);
    assert_eq!(status, 3);
    assert_eq!(
        line["failed"],
        json!([{"kpi": "k-long", "name": "usd_a_pr", "code": "evidence_too_long"}])
    );
    assert!(g.applies().is_empty());
    // A search of the store that fails ends the pass with nothing applied.
    let mut h = Fake::new(&bed.dir);
    kpi_store(&mut h);
    h.fail_search = true;
    let e = measure::run(&mut h, &[OsString::from("kpis")]).unwrap_err();
    assert_eq!(measure::exit_of(&e), 3);
    assert!(h.applies().is_empty());
}

// spec: specs/ap/measure.md#R15
#[test]
fn r15_the_text_of_each_kind_of_kpi_and_its_bounds() {
    let w = ("2026-11-16", "2026-11-22");
    let text = |name: &str, a: Value| measure::value_text(name, &a, w);
    assert_eq!(
        text("usd_a_pr", json!({"usd_a_pr": null, "merged": 0})).unwrap(),
        format!("n/a ({WINDOW}, 0 merged)")
    );
    assert_eq!(
        text(
            "minutes_to_merge",
            json!({"median_minutes": 90.5, "merged": 4})
        )
        .unwrap(),
        format!("90.5 ({WINDOW}, 4 merged)")
    );
    assert_eq!(
        text("usd_lead", json!({"usd_lead_a_pr": 1.67, "merged": 3})).unwrap(),
        format!("1.67 ({WINDOW}, 3 merged)")
    );
    assert_eq!(
        text(
            "stops",
            json!({"units_stopped": 0, "merged": 2, "stops": {}})
        )
        .unwrap(),
        format!("0 of 2 stopped: none ({WINDOW})")
    );
    assert_eq!(
        text(
            "stops",
            json!({"units_stopped": 2, "merged": 5, "stops": {"b": 1, "a": 2}})
        )
        .unwrap(),
        format!("2 of 5 stopped: a 2, b 1 ({WINDOW})")
    );
    let groups = json!({"groups": [{"group": "stages", "write_share": 41.7, "read_share": null}, {"group": "sessions", "write_share": null, "read_share": 12.5}]});
    assert_eq!(
        text("store_writes", groups.clone()).unwrap(),
        format!("sessions n/a % stages 41.7 % ({WINDOW})")
    );
    assert_eq!(
        text("store_queries", groups).unwrap(),
        format!("sessions 12.5 % stages n/a % ({WINDOW})")
    );
    assert_eq!(text("gate", json!({})), Err("unknown_kpi"));
    // A `stops` list over 200 bytes keeps the causes that fit whole and ends with `...`.
    let causes: serde_json::Map<String, Value> = (0..30)
        .map(|n| (format!("cause_number_{n:02}"), json!(n + 1)))
        .collect();
    let cut = text(
        "stops",
        json!({"units_stopped": 30, "merged": 31, "stops": causes}),
    )
    .unwrap();
    assert!(cut.len() <= 200, "{} bytes", cut.len());
    let head = "30 of 31 stopped: cause_number_00 1, cause_number_01 2, ";
    assert!(cut.starts_with(head), "{cut}");
    assert!(cut.ends_with(&format!(", ... ({WINDOW})")), "{cut}");
    let kept = cut.matches("cause_number_").count();
    let one_more = format!(
        "{}, cause_number_{kept:02} {}, ... ({WINDOW})",
        cut.trim_end_matches(&format!(", ... ({WINDOW})")),
        kept + 1
    );
    assert!(one_more.len() > 200, "the cut keeps every cause that fits");
    // Any other value over 200 bytes is a failed measurement; so is an evidence over 300 bytes.
    let long = "x".repeat(200);
    assert_eq!(
        measure::value_text(
            "usd_a_pr",
            &json!({"usd_a_pr": 1.0, "merged": 1}),
            (&long, "2026-11-22")
        ),
        Err("value_too_long")
    );
    assert_eq!(
        measure::value_text(
            "store_writes",
            &json!({"groups": []}),
            (&long, "2026-11-22")
        ),
        Err("value_too_long")
    );
    let p = json!({"intent": "gadget", "since": "2026-11-16", "until": "2026-11-22"});
    assert_eq!(
        measure::evidence_text("flow", &p).unwrap(),
        format!(
            "ont fn flow {}",
            r#"{"intent":"gadget","since":"2026-11-16","until":"2026-11-22"}"#
        )
    );
    let p = json!({"intent": "i".repeat(300), "since": "2026-11-16", "until": "2026-11-22"});
    assert_eq!(measure::evidence_text("flow", &p), Err("evidence_too_long"));
    // The table names thirteen KPIs of three functions.
    assert_eq!(measure::TABLE.len(), 13);
}

// spec: specs/ap/measure.md#R16
#[test]
fn r16_the_timer_runs_the_sessions_pass_and_then_the_kpi_pass_each_night() {
    let service = include_str!("../../../deploy/ap-measure.service");
    let timer = include_str!("../../../deploy/ap-measure.timer");
    let route = include_str!("../../../deploy/ap-route.service");
    let lines: Vec<&str> = service.lines().collect();
    let at = |l: &str| lines.iter().position(|x| *x == l);
    let (sessions, kpis) = (
        at("ExecStart=-/usr/local/bin/ap measure sessions"),
        at("ExecStart=/usr/local/bin/ap measure kpis"),
    );
    assert!(sessions.is_some() && kpis.is_some() && sessions < kpis);
    for l in [
        "Type=oneshot",
        "TimeoutStartSec=3600",
        "Slice=agents.slice",
        "Nice=10",
        "ProtectSystem=full",
        "NoNewPrivileges=yes",
        "ProtectKernelModules=yes",
        "ProtectKernelLogs=yes",
        "ProtectKernelTunables=yes",
        "ProtectControlGroups=yes",
        "ProtectHostname=yes",
        "LockPersonality=yes",
        "RestrictRealtime=yes",
        "RestrictSUIDSGID=yes",
        "SystemCallArchitectures=native",
    ] {
        assert_eq!(lines.iter().filter(|x| **x == l).count(), 1, "{l}");
    }
    // The environment is the route service's but for the role line, which is this service's own (roles.md R4).
    for l in route
        .lines()
        .filter(|l| l.starts_with("Environment=") && !l.starts_with("Environment=ONT_ROLE="))
    {
        assert!(lines.contains(&l), "{l}");
    }
    assert!(lines.contains(&"Environment=ONT_ROLE=measure"));
    let timer: Vec<&str> = timer.lines().collect();
    for l in [
        "OnCalendar=*-*-* 00:20:00 UTC",
        "Persistent=true",
        "WantedBy=timers.target",
    ] {
        assert!(timer.contains(&l), "{l}");
    }
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
            .envs(roles_file::fn_role(args))
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
    roles_file::put(root, &schema);
    let record = bed.stage(
        "units/u1/1.implementer.md.run.json",
        "implementer",
        "s-engine",
        "2026-11-10T09:00:00Z",
    );
    let apply = |action: &str, rid: &str, p: Value| -> Value {
        let v = ont_call(
            &["action", "apply"],
            &json!({"action_type": action, "request_id": rid, "role": "lead", "parameters": p})
                .to_string(),
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

// The KPI pass and the functions `flow` and `share` against the real engine: a scratch store with the repository's
// schema (its task source redirected to the scratch directory, its functions to a copy of the built `ap`), an indexed
// task, a confirmed intent with KPIs set to it, a sessions pass and then the KPI pass twice.
// spec: specs/ap/measure.md#R11
// spec: specs/ap/measure.md#R12
// spec: specs/ap/measure.md#R14
// spec: specs/ap/measure.md#R15
#[test]
#[ignore = "needs the engine in ONT_BIN as uid 0; ./check runs it"]
fn engine_the_kpi_pass_records_once_a_day_and_flow_and_share_answer() {
    let ont = PathBuf::from(std::env::var("ONT_BIN").expect("ONT_BIN names the engine"));
    let bed = Bed::new();
    let root = &bed.dir.0;
    let ap = root.join("ap");
    std::fs::copy(env!("CARGO_BIN_EXE_ap"), &ap).unwrap();
    std::fs::set_permissions(&ap, std::fs::Permissions::from_mode(0o755)).unwrap();
    fn redirect(v: &mut Value, host: &str, to: &str) {
        match v {
            Value::String(s) => {
                if let Some(rest) = s.strip_prefix(host) {
                    *s = format!("{to}{rest}");
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| redirect(x, host, to)),
            Value::Object(o) => o.values_mut().for_each(|x| redirect(x, host, to)),
            _ => {}
        }
    }
    let mut schema: Value =
        serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
    for f in ["cost", "flow", "share"] {
        assert_eq!(schema["functions"][f]["program"], "/usr/local/bin/ap");
        schema["functions"][f]["program"] = json!(ap.to_str().unwrap());
    }
    let work = format!("{}/", bed.dir.path("work"));
    let host = "/home/palantirkc/work/";
    redirect(&mut schema["sources"], host, &work);
    for kind in schema["object_types"].as_object_mut().unwrap().values_mut() {
        redirect(&mut kind["primary_key"], host, &work);
        if let Some(props) = kind["properties"].as_object_mut() {
            for p in props.values_mut() {
                redirect(&mut p["transform"], host, &work);
            }
        }
    }
    std::fs::write(root.join("schema.json"), schema.to_string()).unwrap();
    std::fs::create_dir_all(bed.dir.at("work/ws-test")).unwrap();
    std::fs::write(
        bed.dir.at("work/ws-test/STATE.md"),
        "# A test task\nOwner: nobody\n",
    )
    .unwrap();
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
            .envs(roles_file::fn_role(args))
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
    roles_file::put(root, &schema);
    let indexed = Command::new(&ont)
        .args(["--store", &store, "index", "--source", "task_states"])
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stdout)
    );
    let apply = |action: &str, rid: &str, p: Value| -> Value {
        let v = ont_call(
            &["action", "apply"],
            &json!({"action_type": action, "request_id": rid, "role": "lead", "parameters": p})
                .to_string(),
        );
        assert_eq!(v["status"], "ok", "{action}: {v}");
        v["result"].clone()
    };
    let made = |r: &Value, kind: &str| -> Value {
        r["edits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"]["object_type"] == kind && e["change"] == "initial")
            .unwrap()["id"]
            .clone()
    };
    let now = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    let request = |rid: &str, kind: &str| -> Value {
        made(
            &apply(
                "record_request",
                rid,
                json!({"words": format!("Invented words of {rid}."), "words_verified": true, "reading": format!("The user says {rid}."),
                    "kind": kind, "topic": "kpi-engine", "said_at": ap::watch::utc(now()), "session": "fixture-session", "runtime": "fixture-cli"}),
            ),
            "request",
        )
    };
    let intent = oid("intent", "widget");
    let r0 = request("r0", "order");
    apply(
        "draft_intent",
        "draft",
        json!({"intent": intent, "outcome": "The invented widget board lists every widget.", "first_user": "The fixture reader.",
            "origin": r0, "directory": bed.work("task-a")}),
    );
    apply(
        "add_criterion",
        "c1",
        json!({"intent": intent, "statement": "The board lists 3 widgets.", "check": "fixture-check --count 3", "baseline": "0 widgets"}),
    );
    std::thread::sleep(Duration::from_millis(1_100));
    let r3 = request("r3", "decision");
    apply(
        "route_request",
        "route-r3",
        json!({"request": r3, "subject": intent}),
    );
    apply(
        "confirm_intent",
        "confirmed",
        json!({"intent": intent, "by": r3}),
    );
    let task = oid("task", "ws-test");
    let mut kpi_ids = vec![];
    for name in ["store_writes", "merged_a_day", "stops", "gate"] {
        let k = made(
            &apply(
                "define_kpi",
                &format!("kpi-{name}"),
                json!({"task": task, "name": name, "definition": "d"}),
            ),
            "kpi",
        );
        apply(
            "set_kpi_intent",
            &format!("set-{name}"),
            json!({"kpi": k, "intent": intent}),
        );
        kpi_ids.push(k);
    }
    // A Lead session of the intent's directory: one file write and one store read on 2026-11-10.
    let cwd = bed.work("task-a");
    bed.file(
        "accounts/claude-a/projects/p/s-lead.jsonl",
        &[
            msg("s-lead", "2026-11-10T09:20:00Z", &cwd, "e2"),
            synthetic(
                "s-lead",
                "2026-11-10T09:21:00Z",
                &cwd,
                "u-edit",
                "Edit",
                json!({"file_path": "/x"}),
            ),
            synthetic(
                "s-lead",
                "2026-11-10T09:22:00Z",
                &cwd,
                "u-search",
                "mcp__ont__ont_search",
                json!({}),
            ),
        ],
        "2026-11-10T09:30:01Z",
    );
    let run = |args: &[String]| -> (Option<i32>, Value) {
        let out = Command::new(&ap)
            .arg("measure")
            .args(args)
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
    let mut sessions: Vec<String> = bed
        .args("2026-11-10")
        .iter()
        .map(|a| a.to_str().unwrap().to_owned())
        .collect();
    sessions.extend(["--store".to_owned(), store.clone()]);
    let (code, line) = run(&sessions);
    assert_eq!(
        (code, line["runs"]["recorded"].as_u64()),
        (Some(0), Some(1)),
        "{line}"
    );
    let kpis: Vec<String> = ["kpis", "--day", "2026-11-11", "--store", &store]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
    let (code, line) = run(&kpis);
    assert_eq!(code, Some(0), "{line}");
    assert_eq!(
        (
            line["measured"].as_array().unwrap().len(),
            line["skipped"].as_u64(),
            line["unchanged"].as_u64()
        ),
        (3, Some(1), Some(0)),
        "{line}"
    );
    let w = "2026-11-04..2026-11-10";
    let texts: BTreeMap<String, String> = line["measured"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["name"].as_str().unwrap().to_owned(),
                m["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        texts["store_writes"],
        format!("sessions 0.0 % stages n/a % ({w})")
    );
    assert_eq!(texts["merged_a_day"], format!("0.0 ({w}, 0 merged)"));
    assert_eq!(texts["stops"], format!("0 of 0 stopped: none ({w})"));
    let found = ont_call(&["search"], r#"{"object_type":"measurement","limit":50}"#);
    assert_eq!(
        found["result"]["objects"].as_array().unwrap().len(),
        3,
        "{found}"
    );
    // The same day again, at once and a second later: no measurement is added and the pass succeeds.
    for wait in [0, 1_100] {
        std::thread::sleep(Duration::from_millis(wait));
        let (code, again) = run(&kpis);
        assert_eq!(
            (
                code,
                again["measured"].as_array().unwrap().len(),
                again["unchanged"].as_u64(),
                again["failed"].clone()
            ),
            (Some(0), 0, Some(3), json!([])),
            "{again}"
        );
    }
    let found = ont_call(&["search"], r#"{"object_type":"measurement","limit":50}"#);
    assert_eq!(
        found["result"]["objects"].as_array().unwrap().len(),
        3,
        "{found}"
    );
    let got = ont_call(&["get"], &json!({"id": kpi_ids[0]}).to_string());
    let fields = &got["result"]["objects"][0]["observations"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["fields"];
    assert_eq!(fields["value"]["value"], texts["store_writes"].as_str());
    // The functions as the engine runs them.
    let v = ont_call(
        &["fn", "flow"],
        r#"{"parameters": {"intent": "widget", "since": "2026-11-04", "until": "2026-11-10"}}"#,
    );
    assert_eq!(v["function"], "flow", "{v}");
    let a = &v["answer"];
    assert_eq!(
        (
            a["merged"].as_u64(),
            a["units_stopped"].as_u64(),
            a["stops"].clone(),
            a["median_minutes"].clone()
        ),
        (Some(0), Some(0), json!({}), Value::Null),
        "{v}"
    );
    let v = ont_call(
        &["fn", "share"],
        r#"{"parameters": {"since": "2026-11-10", "until": "2026-11-10", "intent": "widget"}}"#,
    );
    assert_eq!(v["function"], "share", "{v}");
    let sessions = &v["answer"]["groups"][1];
    assert_eq!(sessions["group"], "sessions");
    assert_eq!(
        (
            sessions["runs"].as_u64(),
            sessions["file_writes"].as_u64(),
            sessions["store_reads"].as_u64(),
            sessions["read_share"].as_f64(),
            sessions["write_share"].as_f64()
        ),
        (Some(1), Some(1), Some(1), Some(100.0), Some(0.0)),
        "{v}"
    );
}
