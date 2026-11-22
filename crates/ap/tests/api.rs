//! `ap api batch` and the function `api_spend` (`specs/ap/api.md`) against a fake of the port: it answers `curl` by
//! URL and `ont` from a fake store that keeps request ids as the engine does, records every call, and keeps time by
//! the port's clock. The key file, the policy, the run directory and `--out` are under the test's own directory; no
//! test reads `/etc/agent-platform`, calls the real `curl` or opens a connection. The test marked `#[ignore]` runs
//! the real engine in a scratch store (`./check` runs it with `--ignored` and `ONT_BIN`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::api::{Api, Examined, Stop, batch, examine_path, spend};
use ap::host::{Answer, Failure, Reply, System};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsString,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const KEY_FILE: &str = "/etc/agent-platform/credentials/anthropic-api-key";
const MARKER: &str = "sk-ant-fixture-MARKER-7f3a9c";
const BASE: &str = "https://api.anthropic.com/v1/messages/";
/// 2027-01-15T08:00:00Z.
const NOW: u64 = 1_800_000_000;
const PRICES: &str = include_str!("../../../evals/prices.tsv");
static NEXT: AtomicU32 = AtomicU32::new(0);

fn utc(secs: u64) -> String {
    ap::watch::utc(secs)
}
fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type": kind, "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": key})
}

/// The member `api` of the host's policy, as unit api-policy fixes it.
fn api_member() -> Value {
    json!({"key_file": KEY_FILE, "stops_usd": {"standard": 160, "soak": 185}, "day_cap_usd": 40,
        "jobs": {
            "drift": {"cap_usd": 60, "models": ["claude-sonnet-5-5", "claude-opus-5-5"], "max_tokens": 8000, "max_requests": 100},
            "merge-review": {"cap_usd": 45, "models": ["claude-opus-5-5", "claude-fable-5-2"], "max_tokens": 8000, "max_requests": 100},
            "mining": {"cap_usd": 18, "models": ["claude-sonnet-5-5", "claude-fable-5-2"], "max_tokens": 8000, "max_requests": 400},
            "evals": {"cap_usd": 39, "models": ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5", "claude-fable-5-2"], "max_tokens": 8000, "max_requests": 300}}})
}

#[derive(Clone, Debug)]
struct Called {
    program: String,
    args: Vec<String>,
    input: Value,
    /// The header file's text while the call ran, when the call named one.
    header: Option<String>,
}
#[derive(PartialEq)]
enum StartMode {
    Normal,
    /// The first `start_run` answers `already_applied` with the edits of an earlier run.
    AlreadyFirst,
    /// The first `start_run` applies and gives no answer.
    SilentOnce,
    /// Every `start_run` is refused.
    Refused,
}

struct Fake {
    dir: PathBuf,
    calls: Vec<Called>,
    runs: BTreeMap<String, Value>,
    ids: BTreeMap<String, (Value, Value)>,
    base: Instant,
    offset: Duration,
    counts: VecDeque<u64>,
    count_status: u16,
    submit: Option<(u16, Value)>,
    polls: VecDeque<&'static str>,
    never_end: bool,
    results: String,
    start_mode: StartMode,
    search_fails: bool,
    foreign_owner: bool,
    next_run: u32,
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl Fake {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "ap-api-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let dir = fs::canonicalize(&base).unwrap();
        let f = Fake {
            dir,
            calls: vec![],
            runs: BTreeMap::new(),
            ids: BTreeMap::new(),
            base: Instant::now(),
            offset: Duration::ZERO,
            counts: VecDeque::new(),
            count_status: 200,
            submit: Some((
                200,
                json!({"id": "msgbatch_01Fixture", "processing_status": "in_progress"}),
            )),
            polls: VecDeque::from(["in_progress"]),
            never_end: false,
            results: String::new(),
            start_mode: StartMode::Normal,
            search_fails: false,
            foreign_owner: false,
            next_run: 0,
        };
        f.policy(json!({"models": {"allowed": ["sonnet"], "never": []}, "accounts": {"claude": ["/root/.claude"]}, "api": api_member()}));
        f.key(&format!("{MARKER}\n"), 0o600);
        f
    }
    fn at(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    fn path(&self, name: &str) -> String {
        self.at(name).to_str().unwrap().to_owned()
    }
    fn policy(&self, v: Value) {
        fs::write(self.at("policy.json"), v.to_string()).unwrap();
    }
    fn key(&self, text: &str, mode: u32) {
        let _ = fs::remove_file(self.at("key"));
        fs::write(self.at("key"), text).unwrap();
        fs::set_permissions(self.at("key"), fs::Permissions::from_mode(mode)).unwrap();
    }
    fn requests(&self, lines: &[Value]) {
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        fs::write(self.at("requests.jsonl"), text).unwrap();
    }
    fn mapped(&self, path: &str) -> PathBuf {
        if path == KEY_FILE {
            self.at("key")
        } else {
            PathBuf::from(path)
        }
    }
    fn wall_secs(&self) -> u64 {
        NOW + self.offset.as_secs()
    }
    /// A run of the store as the engine keeps it.
    fn seed(&mut self, key: &str, fields: Value) {
        self.runs.insert(key.to_owned(), fields);
    }
    fn curls(&self) -> Vec<&Called> {
        self.calls.iter().filter(|c| c.program == "curl").collect()
    }
    fn urls(&self) -> Vec<String> {
        self.curls()
            .iter()
            .map(|c| {
                c.args
                    .last()
                    .unwrap()
                    .strip_prefix(BASE)
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }
    fn actions(&self) -> Vec<&Called> {
        self.calls
            .iter()
            .filter(|c| c.program == "ont" && c.args.first().is_some_and(|a| a == "action"))
            .collect()
    }
    fn answer_store(v: Value, success: bool) -> Answer {
        let bytes = v.to_string().into_bytes();
        Ok(Reply {
            success,
            bytes: bytes.clone(),
            stdout: bytes,
        })
    }
    fn refused(code: &str, failed: &[&str]) -> Answer {
        Self::answer_store(
            json!({"status": "refused", "code": code, "failed": failed}),
            false,
        )
    }
    fn ont(&mut self, args: &[String], input: &Value) -> Answer {
        let no_answer = || {
            Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            })
        };
        if args[0] == "search" {
            if self.search_fails {
                return no_answer();
            }
            assert_eq!(input["object_type"], "run");
            let all = input["where"]["all"].as_array().unwrap();
            let objects: Vec<Value> = self
                .runs
                .iter()
                .filter(|(_, f)| {
                    all.iter().all(|c| {
                        if let Some([k, v]) = c["eq"].as_array().map(Vec::as_slice) {
                            return f[k.as_str().unwrap()] == *v;
                        }
                        let [k, v] = c["gt"].as_array().unwrap().as_slice() else {
                            panic!("{c}")
                        };
                        f[k.as_str().unwrap()]
                            .as_str()
                            .is_some_and(|t| t > v.as_str().unwrap())
                    })
                })
                .map(|(k, f)| {
                    let fields: serde_json::Map<String, Value> = f
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(n, v)| (n.clone(), json!({"kind": "value", "value": v})))
                        .collect();
                    json!({"id": oid("run", k), "head": {"fields": fields}})
                })
                .collect();
            return Self::answer_store(
                json!({"status": "ok", "now": self.wall_secs(), "result": {"objects": objects,
                    "basis": {"qualified": true}, "continuation": null}}),
                true,
            );
        }
        assert_eq!(&args[..2], ["action", "apply"]);
        assert_eq!(input["role"], "batch");
        let rid = input["request_id"].as_str().unwrap().to_owned();
        let p = input["parameters"].clone();
        let action = input["action_type"].as_str().unwrap().to_owned();
        if action == "start_run" && self.start_mode == StartMode::Refused {
            return Self::refused("invalid_parameter", &[]);
        }
        if action == "start_run" && self.start_mode == StartMode::AlreadyFirst {
            self.start_mode = StartMode::Normal;
            let edits = json!([{"change": "initial", "id": oid("run", "run-earlier")}]);
            return Self::answer_store(
                json!({"status": "ok", "result": {"outcome": "already_applied", "edits": edits}}),
                true,
            );
        }
        if let Some((params, edits)) = self.ids.get(&rid) {
            if *params != p {
                return Self::refused("request_id_reused", &[]);
            }
            return Self::answer_store(
                json!({"status": "ok", "result": {"outcome": "already_applied", "edits": edits}}),
                true,
            );
        }
        let edits = match action.as_str() {
            "start_run" => {
                self.next_run += 1;
                let key = format!("run-{}", self.next_run);
                let mut fields = p.clone();
                fields["status"] = json!("running");
                fields["started_at"] = json!(utc(self.wall_secs()));
                self.runs.insert(key.clone(), fields);
                json!([{"change": "initial", "id": oid("run", &key)}])
            }
            "close_run" => {
                let key = p["run"]["native_key"].as_str().unwrap().to_owned();
                let run = self.runs.get_mut(&key).unwrap();
                if run["status"] != "running" {
                    return Self::refused("submission_criteria", &["run_not_running"]);
                }
                for (k, v) in p.as_object().unwrap() {
                    if k != "run" {
                        run[k] = v.clone();
                    }
                }
                run["status"] = json!("closed");
                json!([{"change": "update", "id": oid("run", &key)}])
            }
            other => panic!("{other}"),
        };
        self.ids.insert(rid, (p, edits.clone()));
        if action == "start_run" && self.start_mode == StartMode::SilentOnce {
            self.start_mode = StartMode::Normal;
            return no_answer();
        }
        Self::answer_store(
            json!({"status": "ok", "result": {"outcome": "applied", "edits": edits}}),
            true,
        )
    }
    fn curl(&mut self, args: &[String], input: &Value) -> Answer {
        assert_eq!(args[0], "-q", "every curl call begins with -q");
        let url = args.last().unwrap();
        let rest = url.strip_prefix(BASE).expect("a URL under the API");
        let no_answer = || {
            Err(Failure {
                code: "process_failed",
                field: "curl".into(),
                detail: Value::Null,
            })
        };
        let (status, body) = if rest == "count_tokens" {
            assert!(input.get("max_tokens").is_none(), "{input}");
            let n = self.counts.pop_front().unwrap_or(1000);
            match self.count_status {
                200 => (200, json!({"input_tokens": n}).to_string()),
                s => (s, json!({"type": "error"}).to_string()),
            }
        } else if rest == "batches" {
            match &self.submit {
                None => return no_answer(),
                Some((s, b)) => (*s, b.to_string()),
            }
        } else if let Some(id) = rest.strip_suffix("/results") {
            assert!(id.starts_with("batches/"));
            let at = args.iter().position(|a| a == "-o").unwrap();
            let out = Path::new(&args[at + 1]);
            assert_eq!(fs::metadata(out).unwrap().mode() & 0o777, 0o600);
            fs::write(out, &self.results).unwrap();
            (200, String::new())
        } else {
            assert!(rest.starts_with("batches/"), "{rest}");
            let status = match self.polls.pop_front() {
                Some(s) => s,
                None if self.never_end => "in_progress",
                None => "ended",
            };
            (200, json!({"processing_status": status}).to_string())
        };
        let out = format!("{body}\n{status}").into_bytes();
        Ok(Reply {
            success: true,
            bytes: out.clone(),
            stdout: out,
        })
    }
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _: u64) -> Answer {
        let header = args.iter().find_map(|a| a.strip_prefix('@')).map(|path| {
            let file = Path::new(path);
            assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o600);
            let parent = file.parent().unwrap();
            assert_eq!(fs::metadata(parent).unwrap().mode() & 0o777, 0o700);
            assert_eq!(parent.parent().unwrap(), self.at("run"));
            fs::read_to_string(file).unwrap()
        });
        self.calls.push(Called {
            program: program.to_owned(),
            args: args.to_vec(),
            input: input.clone(),
            header,
        });
        match program {
            "curl" => self.curl(args, input),
            "ont" => self.ont(args, input),
            other => panic!("program {other}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let path = self.mapped(path);
        if !fs::metadata(&path)?.is_file() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        fs::read(path)
    }
    fn now(&self) -> Instant {
        self.base + self.offset
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(NOW) + self.offset
    }
    fn pause(&mut self, d: Duration) {
        self.offset += d;
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(self.at("policy.json"))
    }
    fn environment(&self) -> Vec<(String, String)> {
        vec![("AP_API_RUN".into(), self.path("run"))]
    }
}
impl Api for Fake {
    fn examine(&self, path: &str) -> std::io::Result<Examined> {
        let mut e = examine_path(self.mapped(path).to_str().unwrap())?;
        if self.foreign_owner {
            e.uid = e.euid.unwrap_or(0) + 4242;
        }
        Ok(e)
    }
}

type Refused = (String, String, Value, u8);
fn go(f: &mut Fake, args: &[&str]) -> Result<Value, Refused> {
    let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
    batch(f, &argv)
        .map_err(|Stop { failure: e, status }| (e.code.to_owned(), e.field, e.detail, status))
}
fn call(f: &mut Fake, job: &str, more: &[&str]) -> Result<Value, Refused> {
    let (requests, out) = (f.path("requests.jsonl"), f.path("out"));
    let mut args = vec!["batch", job, "--requests", &requests, "--out", &out];
    args.extend_from_slice(more);
    go(f, &args)
}
fn request(id: &str, model: &str, max_tokens: u64) -> Value {
    json!({"custom_id": id, "params": {"model": model, "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": format!("Say {id}.")}]}})
}
fn many(model: &str, n: usize, max_tokens: u64) -> Vec<Value> {
    (0..n)
        .map(|i| request(&format!("r{i}"), model, max_tokens))
        .collect()
}
fn refused(code: &str, field: &str, status: u8) -> impl Fn(&Refused) -> bool {
    let (code, field) = (code.to_owned(), field.to_owned());
    move |r: &Refused| r.0 == code && r.1 == field && r.3 == status
}
/// A refusal before any effect (R7): no call, no run directory, no `--out`.
fn before_any_effect(f: &Fake, r: &Result<Value, Refused>, code: &str, field: &str) {
    let e = r.as_ref().unwrap_err();
    assert!(refused(code, field, 2)(e), "{e:?}, want {code} {field}");
    assert!(f.calls.is_empty(), "{:?}", f.calls);
    assert!(!f.at("run").exists() && !f.at("out").exists());
}
fn settled_run(started: u64, job: &str, settled: i64) -> Value {
    json!({"role": "batch", "runtime": "api", "model": "claude-sonnet-5-5", "job": job, "status": "closed",
        "started_at": utc(started), "reserved_usd_micros": settled * 2, "settled_usd_micros": settled})
}
/// The rate of a row in millionths of a micro-USD per token, as the test reads the price file.
fn rate(model: &str, kind: &str) -> u128 {
    let text = PRICES
        .lines()
        .find_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() == 3 && f[0] == model && f[1] == kind).then(|| f[2])
        })
        .unwrap();
    let (w, fr) = text.split_once('.').unwrap_or((text, ""));
    w.parse::<u128>().unwrap() * 1_000_000 + format!("{fr:0<6}").parse::<u128>().unwrap()
}
fn half_up(sum: u128) -> i64 {
    i64::try_from(sum.div_ceil(2_000_000)).unwrap()
}
fn mode(p: &Path) -> u32 {
    fs::symlink_metadata(p).unwrap().mode() & 0o777
}
fn listing(dir: &Path) -> Vec<String> {
    let mut all: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    all.sort();
    all
}

// spec: specs/ap/api.md#R10
// spec: specs/ap/api.md#R7
#[test]
fn over_budget_is_refused_before_any_call() {
    // The output part alone passes the day's cap: 30 USD spent today and 100 x 8,000 tokens at 40 / 2.
    let mut f = Fake::new();
    f.seed("run-today", settled_run(NOW - 3_600, "drift", 30_000_000));
    f.requests(&many("claude-fable-5-2", 100, 8000));
    let e = call(&mut f, "merge-review", &[]).unwrap_err();
    assert!(refused("budget_exceeded", "day", 2)(&e), "{e:?}");
    assert_eq!(
        e.2,
        json!({"cap_usd_micros": 40_000_000, "counted_usd_micros": 30_000_000, "worst_usd_micros": 16_000_000})
    );
    assert!(f.curls().is_empty() && f.actions().is_empty());
    assert!(!f.at("out").exists());
    // The search of the spend names only role and started_at.
    let searches: Vec<&Called> = f.calls.iter().filter(|c| c.args[0] == "search").collect();
    assert_eq!(searches.len(), 1);
    let named: Vec<&str> = searches[0].input["where"]["all"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c.as_object().unwrap().values().next().unwrap()[0]
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(named, ["role", "started_at"]);
    // A file of api-policy's mining job, 400 requests of 8,000 tokens at claude-fable-5-2: 64 USD of output
    // alone, over the job's 18.
    let mut f = Fake::new();
    f.requests(&many("claude-fable-5-2", 400, 8000));
    let e = call(&mut f, "mining", &[]).unwrap_err();
    assert!(refused("budget_exceeded", "job", 2)(&e), "{e:?}");
    assert_eq!(e.2["worst_usd_micros"], 64_000_000);
    assert!(f.curls().is_empty() && f.actions().is_empty() && !f.at("out").exists());
}

// spec: specs/ap/api.md#R10
#[test]
fn the_worst_case_after_counting_is_refused_with_only_count_calls() {
    let mut f = Fake::new();
    f.seed("run-today", settled_run(NOW - 60, "drift", 30_000_000));
    f.requests(&many("claude-sonnet-5-5", 10, 8000));
    f.counts = VecDeque::from(vec![1_000_000; 10]);
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("budget_exceeded", "day", 2)(&e), "{e:?}");
    // Each request: ceil((1,000,000 x 2 + 8,000 x 10) / 2) micro-USD.
    assert_eq!(e.2["worst_usd_micros"], 10 * 1_040_000);
    assert_eq!(f.urls(), vec!["count_tokens"; 10]);
    assert!(f.actions().is_empty() && !f.at("out").exists());
    assert_eq!(
        listing(&f.at("run")),
        ["api.lock"],
        "no key directory is left"
    );
}

// spec: specs/ap/api.md#R10
// spec: specs/ap/api.md#R1
#[test]
fn queue_soak_is_admitted_where_standard_is_refused_by_month() {
    let mut f = Fake::new();
    // 159 USD spent this month, before today, by a job the policy does not name.
    f.seed(
        "run-early",
        settled_run(NOW - 10 * 86_400, "other", 159_000_000),
    );
    f.requests(&many("claude-sonnet-5-5", 50, 8000));
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("budget_exceeded", "month", 2)(&e), "{e:?}");
    assert_eq!(e.2["cap_usd_micros"], 160_000_000);
    assert!(f.curls().is_empty());
    f.results = (0..50)
        .map(|i| {
            format!(
                "{}\n",
                json!({"custom_id": format!("r{i}"), "result": {"type": "expired"}})
            )
        })
        .collect();
    let v = call(&mut f, "drift", &["--queue", "soak"]).unwrap();
    assert_eq!(v["results"]["expired"], 50);
    assert_eq!(v["settled_usd_micros"], 0);
    // The last month's runs and another role's are not this month's spend.
    let mut f = Fake::new();
    f.seed(
        "run-last-month",
        settled_run(NOW - 40 * 86_400, "drift", 159_000_000),
    );
    let mut other = settled_run(NOW - 60, "drift", 159_000_000);
    other["role"] = json!("implementer");
    f.seed("run-other-role", other);
    f.requests(&many("claude-sonnet-5-5", 50, 8000));
    f.results = (0..50)
        .map(|i| {
            format!(
                "{}\n",
                json!({"custom_id": format!("r{i}"), "result": {"type": "canceled"}})
            )
        })
        .collect();
    assert!(call(&mut f, "drift", &[]).is_ok());
}

/// The requests of the whole flow: a plain one, a cache write of 1h, a long prompt, and one that errs.
fn flow_requests() -> Vec<Value> {
    let mut cached = request("cached", "claude-haiku-5-5", 2000);
    cached["params"]["system"] = json!([{"type": "text", "text": "Fixture.", "cache_control": {"type": "ephemeral", "ttl": "1h"}}]);
    vec![
        request("plain", "claude-haiku-5-5", 1000),
        cached,
        request("long", "claude-haiku-5-5", 4000),
        request("erred", "claude-haiku-5-5", 100),
    ]
}
fn flow_results() -> String {
    let ok = |id: &str, stop: &str, usage: Value| json!({"custom_id": id, "result": {"type": "succeeded", "message": {"stop_reason": stop, "usage": usage}}});
    [
        ok(
            "plain",
            "end_turn",
            json!({"input_tokens": 900, "output_tokens": 500, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}),
        ),
        ok(
            "cached",
            "max_tokens",
            json!({"input_tokens": 100, "output_tokens": 700, "cache_read_input_tokens": 300,
                "cache_creation_input_tokens": 2000, "cache_creation": {"ephemeral_1h_input_tokens": 1500, "ephemeral_5m_input_tokens": 500}}),
        ),
        ok(
            "long",
            "end_turn",
            json!({"input_tokens": 150000, "output_tokens": 800, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}),
        ),
        json!({"custom_id": "erred", "result": {"type": "errored", "error": {"type": "invalid_request"}}}),
    ]
    .iter()
    .map(|l| format!("{l}\n"))
    .collect()
}
fn flow(f: &mut Fake) {
    f.requests(&flow_requests());
    f.counts = VecDeque::from(vec![1000, 2000, 150_000, 10]);
    f.results = flow_results();
}

// spec: specs/ap/api.md#R10
// spec: specs/ap/api.md#R11
// spec: specs/ap/api.md#R12
// spec: specs/ap/api.md#R13
// spec: specs/ap/api.md#R14
// spec: specs/ap/api.md#R15
// spec: specs/ap/api.md#R16
// spec: specs/ap/api.md#R18
#[test]
fn the_whole_flow_reserves_submits_polls_and_settles() {
    let mut f = Fake::new();
    flow(&mut f);
    let v = call(&mut f, "evals", &[]).unwrap();
    let h = "claude-haiku-5-5";
    let l = "claude-haiku-5-5+long";
    // The worst case: the cached request at the 1h write rate, the long one at the +long rows.
    let worst = half_up(1000 * rate(h, "input") + 1000 * rate(h, "output"))
        + half_up(2000 * rate(h, "cache_write_1h") + 2000 * rate(h, "output"))
        + half_up(150_000 * rate(l, "input") + 4000 * rate(l, "output"))
        + half_up(10 * rate(h, "input") + 100 * rate(h, "output"));
    let settled = half_up(900 * rate(h, "input") + 500 * rate(h, "output"))
        + half_up(
            100 * rate(h, "input")
                + 300 * rate(h, "cache_read")
                + 500 * rate(h, "cache_write_5m")
                + 1500 * rate(h, "cache_write_1h")
                + 700 * rate(h, "output"),
        )
        + half_up(150_000 * rate(l, "input") + 800 * rate(l, "output"));
    let run = v["run"].as_str().unwrap().to_owned();
    assert_eq!(
        v,
        json!({"job": "evals", "run": run, "batch": "msgbatch_01Fixture", "requests": 4,
            "results": {"succeeded": 3, "errored": 1, "canceled": 0, "expired": 0},
            "stop_reasons": {"end_turn": 2, "max_tokens": 1},
            "reserved_usd_micros": worst, "settled_usd_micros": settled, "out": f.path("out")})
    );
    // The calls in their order: four counts, the start, the submit, two polls, the results, the close.
    let order: Vec<String> = f
        .calls
        .iter()
        .filter(|c| c.args[0] != "search")
        .map(|c| match c.program.as_str() {
            "curl" => c
                .args
                .last()
                .unwrap()
                .strip_prefix(BASE)
                .unwrap()
                .to_owned(),
            _ => c.input["action_type"].as_str().unwrap().to_owned(),
        })
        .collect();
    assert_eq!(
        order,
        [
            "count_tokens",
            "count_tokens",
            "count_tokens",
            "count_tokens",
            "start_run",
            "batches",
            "batches/msgbatch_01Fixture",
            "batches/msgbatch_01Fixture",
            "batches/msgbatch_01Fixture/results",
            "close_run"
        ]
    );
    assert_eq!(
        f.offset,
        Duration::from_secs(120),
        "a poll every 60 seconds"
    );
    // The submit sends the lines as read, in order; a POST carries its body on standard input.
    let submit = f.curls()[4];
    assert_eq!(submit.input, json!({"requests": flow_requests()}));
    for c in f.curls() {
        let a = &c.args;
        assert_eq!(a[0], "-q");
        for pair in [
            ["--proto", "=https"],
            ["-H", "anthropic-version: 2026-10-21"],
            ["-H", "content-type: application/json"],
        ] {
            assert!(a.windows(2).any(|w| w == pair), "{a:?}");
        }
        assert!(a.iter().any(|w| w == "--max-time"));
        assert!(c.args.last().unwrap().starts_with(BASE));
    }
    // The start and the close.
    let actions = f.actions();
    let start = &actions[0].input;
    let rid = start["request_id"].as_str().unwrap();
    assert!(
        rid.len() == 4 + 32 + 6
            && rid.starts_with("api-")
            && rid.ends_with("-start")
            && rid[4..36].bytes().all(|b| b.is_ascii_hexdigit())
    );
    assert_eq!(
        start["parameters"],
        json!({"role": "batch", "runtime": "api", "model": h, "account": "anthropic-api-key",
            "uses": oid("account", "anthropic-api-key"), "job": "evals", "reserved_usd_micros": worst,
            "record": f.path("out/batch.json")})
    );
    let close = &actions[1].input;
    assert_eq!(close["request_id"], rid.replace("-start", "-close"));
    assert_eq!(
        close["parameters"],
        json!({"run": oid("run", &run), "state": "ended", "session": "msgbatch_01Fixture",
            "input_tokens": 151_000, "cache_read_tokens": 300, "cache_write_tokens": 2000, "output_tokens": 2000,
            "settled_usd_micros": settled})
    );
    assert_eq!(f.runs[&run]["status"], "closed");
    // The effects: --out with its two files, the run directory with its lock alone.
    assert_eq!(mode(&f.at("out")), 0o700);
    assert_eq!(listing(&f.at("out")), ["batch.json", "results.jsonl"]);
    assert_eq!(mode(&f.at("out/batch.json")), 0o600);
    assert_eq!(mode(&f.at("out/results.jsonl")), 0o600);
    assert_eq!(
        fs::read_to_string(f.at("out/results.jsonl")).unwrap(),
        flow_results()
    );
    let record: Value = serde_json::from_slice(&fs::read(f.at("out/batch.json")).unwrap()).unwrap();
    assert_eq!(
        record,
        json!({"job": "evals", "run": run, "model": h, "requests": 4, "reserved_usd_micros": worst,
            "batch": "msgbatch_01Fixture", "state": "settled"})
    );
    assert_eq!(listing(&f.at("run")), ["api.lock"]);
    assert_eq!(mode(&f.at("run")), 0o700);
    assert_eq!(mode(&f.at("run/api.lock")), 0o600);
}

// spec: specs/ap/api.md#R4
// spec: specs/ap/api.md#R11
#[test]
fn the_key_is_only_in_the_header_file_while_its_call_runs() {
    let mut f = Fake::new();
    flow(&mut f);
    let v = call(&mut f, "evals", &[]).unwrap();
    assert!(!v.to_string().contains(MARKER));
    let curls = f.curls();
    assert_eq!(curls.len(), 8);
    for c in &curls {
        assert_eq!(
            c.header.as_deref(),
            Some(format!("x-api-key: {MARKER}\n").as_str())
        );
    }
    for c in &f.calls {
        assert!(!c.args.iter().any(|a| a.contains(MARKER)), "{c:?}");
        assert!(!c.input.to_string().contains(MARKER), "{c:?}");
    }
    assert!(
        !f.environment()
            .iter()
            .any(|(k, v)| k.contains(MARKER) || v.contains(MARKER))
    );
    for name in ["out/batch.json", "out/results.jsonl"] {
        assert!(!fs::read_to_string(f.at(name)).unwrap().contains(MARKER));
    }
    assert!(!serde_json::to_string(&f.runs).unwrap().contains(MARKER));
    assert_eq!(
        listing(&f.at("run")),
        ["api.lock"],
        "no key directory is left"
    );
    // A failed call leaves none either.
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 2, 100));
    f.count_status = 500;
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("count_failed", "requests", 3)(&e), "{e:?}");
    assert_eq!(e.2, json!({"line": 1}));
    assert_eq!(
        f.curls()[0].header.as_deref(),
        Some(format!("x-api-key: {MARKER}\n").as_str())
    );
    assert_eq!(listing(&f.at("run")), ["api.lock"]);
    assert!(!format!("{e:?}").contains(MARKER));
}

// spec: specs/ap/api.md#R4
#[test]
fn a_key_file_of_another_form_is_key_unusable_without_its_bytes() {
    let unusable = |f: &mut Fake, detail: &str| {
        f.requests(&many("claude-sonnet-5-5", 1, 100));
        let r = call(f, "drift", &[]);
        before_any_effect(f, &r, "key_unusable", "key_file");
        let e = r.unwrap_err();
        assert_eq!(e.2, json!(detail));
        assert!(!format!("{e:?}").contains("fixture"));
    };
    for text in [
        "sk-ant-fixture-one\nsk-ant-fixture-two\n",
        "sk-ant-fixture one\n",
        "\n",
        "sk-ant-fixture\n\n",
    ] {
        let mut f = Fake::new();
        f.key(text, 0o600);
        unusable(&mut f, "not_one_line");
    }
    let mut f = Fake::new();
    f.key(&format!("{MARKER}\n"), 0o640);
    unusable(&mut f, "group_or_other_access");
    let mut f = Fake::new();
    f.foreign_owner = true;
    unusable(&mut f, "owner_not_trusted");
    let mut f = Fake::new();
    fs::rename(f.at("key"), f.at("real-key")).unwrap();
    std::os::unix::fs::symlink(f.at("real-key"), f.at("key")).unwrap();
    unusable(&mut f, "not_regular_file");
    let mut f = Fake::new();
    fs::remove_file(f.at("key")).unwrap();
    unusable(&mut f, "not_found");
    // Without its final line feed, the one line is the key.
    let mut f = Fake::new();
    f.key(MARKER, 0o400);
    flow(&mut f);
    assert!(call(&mut f, "evals", &[]).is_ok());
    assert!(f.curls()[0].header.as_deref() == Some(format!("x-api-key: {MARKER}\n").as_str()));
}

// spec: specs/ap/api.md#R1
// spec: specs/ap/api.md#R2
// spec: specs/ap/api.md#R7
#[test]
fn the_form_and_the_academy_are_refused_before_any_effect() {
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 1, 100));
    let (req, out) = (f.path("requests.jsonl"), f.path("out"));
    let usage: Vec<Vec<&str>> = vec![
        vec![],
        vec!["single", "drift"],
        vec!["batch"],
        vec!["batch", "drift"],
        vec!["batch", "drift", "--requests", &req],
        vec!["batch", "drift", "--out", &out],
        vec!["batch", "--requests", &req, "--out", &out],
        vec!["batch", "drift", "other", "--requests", &req, "--out", &out],
        vec![
            "batch",
            "drift",
            "--requests",
            &req,
            "--out",
            &out,
            "--queue",
        ],
        vec![
            "batch",
            "drift",
            "--requests",
            &req,
            "--out",
            &out,
            "--out",
            &out,
        ],
        vec![
            "batch",
            "drift",
            "--requests",
            &req,
            "--out",
            &out,
            "--split",
            "2",
        ],
    ];
    for args in usage {
        let r = go(&mut f, &args);
        before_any_effect(&f, &r, "usage", "args");
    }
    // The options in any order.
    let mut g = Fake::new();
    g.requests(&many("claude-sonnet-5-5", 1, 100));
    g.results = format!(
        "{}\n",
        json!({"custom_id": "r0", "result": {"type": "expired"}})
    );
    let (req2, out2, store2) = (
        g.path("requests.jsonl"),
        g.path("out"),
        g.path("store.sqlite"),
    );
    let v = go(
        &mut g,
        &[
            "batch",
            "--queue",
            "standard",
            "--out",
            &out2,
            "--store",
            &store2,
            "drift",
            "--requests",
            &req2,
        ],
    )
    .unwrap();
    assert_eq!(v["requests"], 1);
    assert!(
        g.calls
            .iter()
            .filter(|c| c.program == "ont")
            .all(|c| c.args.windows(2).any(|w| w == ["--store", store2.as_str()]))
    );
    for (args, field) in [
        (
            vec!["batch", "Drift", "--requests", &req, "--out", &out],
            "job",
        ),
        (
            vec!["batch", &"x".repeat(41), "--requests", &req, "--out", &out],
            "job",
        ),
        (
            vec![
                "batch",
                "drift",
                "--requests",
                "requests.jsonl",
                "--out",
                &out,
            ],
            "requests",
        ),
        (
            vec!["batch", "drift", "--requests", &req, "--out", "out"],
            "out",
        ),
        (
            vec![
                "batch",
                "drift",
                "--requests",
                &req,
                "--out",
                &out,
                "--queue",
                "fast",
            ],
            "queue",
        ),
        (
            vec![
                "batch",
                "drift",
                "--requests",
                &req,
                "--out",
                &out,
                "--store",
                "s.sqlite",
            ],
            "store",
        ),
    ] {
        let r = go(&mut f, &args);
        before_any_effect(&f, &r, "invalid_input", field);
    }
    for (args, field) in [
        (
            vec![
                "batch",
                "drift",
                "--requests",
                "/nowhere/edu-lesson/r.jsonl",
                "--out",
                &out,
            ],
            "requests",
        ),
        (
            vec![
                "batch",
                "drift",
                "--requests",
                &req,
                "--out",
                "/nowhere/education-platform/out",
            ],
            "out",
        ),
        (
            vec![
                "batch",
                "drift",
                "--requests",
                "/nowhere/evals/lesson/r.jsonl",
                "--out",
                &out,
            ],
            "requests",
        ),
        (
            vec![
                "batch",
                "drift",
                "--requests",
                &req,
                "--out",
                &out,
                "--store",
                "/nowhere/edu-x/s.sqlite",
            ],
            "store",
        ),
    ] {
        let r = go(&mut f, &args);
        before_any_effect(&f, &r, "academy_path", field);
    }
    assert!(ap::api::academy("/a/evals/lesson"));
    assert!(!ap::api::academy("/a/evals/lessons/x") && !ap::api::academy("/a/my-edu-x/evals"));
}

// spec: specs/ap/api.md#R3
#[test]
fn the_policy_member_is_read_before_any_effect() {
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 1, 100));
    fs::remove_file(f.at("policy.json")).unwrap();
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "api_not_configured", "policy");
    f.policy(json!({"models": {"allowed": ["sonnet"], "never": []}, "accounts": {"claude": ["/root/.claude"]}}));
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "api_not_configured", "policy");
    fs::write(f.at("policy.json"), "{not json").unwrap();
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "policy_invalid", "policy");
    let mut api = api_member();
    api["jobs"]["drift"]["models"] = json!(["gpt-6.1-sol"]);
    f.policy(json!({"api": api}));
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "policy_invalid", "policy");
    assert!(
        r.unwrap_err()
            .2
            .as_str()
            .unwrap()
            .starts_with("api.jobs.drift.models ")
    );
    f.policy(json!({"api": api_member()}));
    let r = call(&mut f, "nightly", &[]);
    before_any_effect(&f, &r, "unknown_job", "job");
    // Each malformed member is named.
    let cases: Vec<(Box<dyn Fn(&mut Value)>, &str)> = vec![
        (
            Box::new(|a| a["key_file"] = json!("/etc/agent-platform/anthropic-api-key")),
            "api.key_file ",
        ),
        (
            Box::new(|a| a["key_file"] = json!("/etc/agent-platform/credentials/a/b")),
            "api.key_file ",
        ),
        (
            Box::new(|a| a["stops_usd"] = json!({"standard": 190, "soak": 185})),
            "api.stops_usd ",
        ),
        (
            Box::new(|a| a["stops_usd"]["soak"] = json!(1001)),
            "api.stops_usd.soak ",
        ),
        (
            Box::new(|a| a["day_cap_usd"] = json!(0)),
            "api.day_cap_usd ",
        ),
        (Box::new(|a| a["jobs"] = json!({})), "api.jobs "),
        (
            Box::new(|a| a["jobs"]["Bad"] = a["jobs"]["drift"].clone()),
            "api.jobs ",
        ),
        (
            Box::new(|a| a["jobs"]["drift"]["cap_usd"] = json!(1.5)),
            "api.jobs.drift.cap_usd ",
        ),
        (
            Box::new(|a| {
                a["jobs"]["drift"]["models"] = json!(["claude-sonnet-5-5", "claude-sonnet-5-5"])
            }),
            "api.jobs.drift.models ",
        ),
        (
            Box::new(|a| a["jobs"]["drift"]["models"] = json!(["claude-unpriced-9"])),
            "api.jobs.drift.models ",
        ),
        (
            Box::new(|a| a["jobs"]["drift"]["max_tokens"] = json!(128_001)),
            "api.jobs.drift.max_tokens ",
        ),
        (
            Box::new(|a| a["jobs"]["drift"]["max_requests"] = json!(100_001)),
            "api.jobs.drift.max_requests ",
        ),
        (
            Box::new(|a| {
                a["jobs"]["drift"]
                    .as_object_mut()
                    .unwrap()
                    .remove("max_requests");
            }),
            "api.jobs.drift.max_requests ",
        ),
        (Box::new(|a| a["fallbacks"] = json!(true)), "api "),
    ];
    for (change, member) in cases {
        let mut api = api_member();
        change(&mut api);
        f.policy(json!({"api": api}));
        let r = call(&mut f, "drift", &[]);
        before_any_effect(&f, &r, "policy_invalid", "policy");
        let detail = r.unwrap_err().2;
        assert!(
            detail.as_str().unwrap().starts_with(member),
            "{detail} {member}"
        );
    }
}

// spec: specs/ap/api.md#R5
// spec: specs/ap/api.md#R6
// spec: specs/ap/api.md#R7
#[test]
fn the_request_file_and_out_are_refused_before_any_effect() {
    let mut f = Fake::new();
    let line = |v: Value| v;
    let ok = request("a", "claude-sonnet-5-5", 100);
    let mut extra = ok.clone();
    extra["params"]["fallbacks"] = json!([]);
    let mut no_messages = ok.clone();
    no_messages["params"]
        .as_object_mut()
        .unwrap()
        .remove("messages");
    let mut zero = ok.clone();
    zero["params"]["max_tokens"] = json!(0);
    let mut third = ok.clone();
    third["batch"] = json!(1);
    for (text, at, reason) in [
        ("not json\n".to_owned(), 1, "not_json"),
        ("[1]\n".to_owned(), 1, "not_an_object"),
        (format!("{ok}\n{third}\n"), 2, "members"),
        (
            format!("{}\n", json!({"custom_id": "a b", "params": ok["params"]})),
            1,
            "custom_id",
        ),
        (format!("{ok}\n{ok}\n"), 2, "custom_id_repeated"),
        (
            format!("{}\n", json!({"custom_id": "a", "params": "x"})),
            1,
            "params",
        ),
        (format!("{}\n", line(extra)), 1, "params_member"),
        (format!("{}\n", line(no_messages)), 1, "messages"),
        (format!("{}\n", line(zero)), 1, "max_tokens"),
        (format!("{ok}\n\n"), 2, "not_json"),
    ] {
        fs::write(f.at("requests.jsonl"), text).unwrap();
        let r = call(&mut f, "drift", &[]);
        before_any_effect(&f, &r, "request_invalid", "requests");
        assert_eq!(r.unwrap_err().2, json!({"line": at, "reason": reason}));
    }
    fs::write(f.at("requests.jsonl"), "").unwrap();
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "no_requests", "requests");
    // A file over 200,000,000 bytes, sparse.
    let big = fs::File::create(f.at("requests.jsonl")).unwrap();
    big.set_len(200_000_001).unwrap();
    drop(big);
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "requests_too_large", "requests");
    f.requests(&many("claude-sonnet-5-5", 101, 100));
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "over_max_requests", "requests");
    f.requests(&[
        request("a", "claude-sonnet-5-5", 100),
        request("b", "claude-opus-5-5", 100),
    ]);
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "models_mixed", "model");
    f.requests(&many("claude-haiku-5-5", 2, 100));
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "model_not_allowed", "model");
    f.requests(&many("claude-sonnet-5-5", 2, 8001));
    let r = call(&mut f, "drift", &[]);
    before_any_effect(&f, &r, "over_max_tokens", "max_tokens");
    // --out.
    f.requests(&many("claude-sonnet-5-5", 2, 100));
    fs::write(f.at("out"), "").unwrap();
    let r = call(&mut f, "drift", &[]);
    assert!(refused("out_exists", "out", 2)(r.as_ref().unwrap_err()));
    assert!(f.calls.is_empty() && !f.at("run").exists());
    fs::remove_file(f.at("out")).unwrap();
    std::os::unix::fs::symlink("/nowhere", f.at("out")).unwrap();
    let r = call(&mut f, "drift", &[]);
    assert!(refused("out_exists", "out", 2)(r.as_ref().unwrap_err()));
    fs::remove_file(f.at("out")).unwrap();
    let req = f.path("requests.jsonl");
    let out = f.path("missing/out");
    let r = go(
        &mut f,
        &["batch", "drift", "--requests", &req, "--out", &out],
    );
    before_any_effect(&f, &r, "invalid_input", "out");
}

// spec: specs/ap/api.md#R12
// spec: specs/ap/api.md#R10
#[test]
fn a_refused_submit_closes_its_run_and_a_second_call_makes_its_own() {
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 3, 1000));
    f.submit = Some((
        400,
        json!({"type": "error", "error": {"type": "invalid_request_error"}}),
    ));
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("submit_refused", "requests", 3)(&e), "{e:?}");
    assert_eq!(e.2, json!({"status": 400}));
    let first = f.actions()[0].input.clone();
    let close = &f.actions()[1].input;
    assert_eq!(
        close["parameters"],
        json!({"run": oid("run", "run-1"), "state": "submit_refused", "settled_usd_micros": 0})
    );
    assert_eq!(f.runs["run-1"]["status"], "closed");
    assert_eq!(f.urls().iter().filter(|u| *u == "batches").count(), 1);
    assert!(!f.urls().iter().any(|u| u.starts_with("batches/")));
    let record: Value = serde_json::from_slice(&fs::read(f.at("out/batch.json")).unwrap()).unwrap();
    assert_eq!(record["state"], "submit_refused");
    // The same --out once more, later, after it was removed: a run of its own with its own reservation.
    fs::remove_dir_all(f.at("out")).unwrap();
    f.pause(Duration::from_secs(1));
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("submit_refused", "requests", 3)(&e), "{e:?}");
    let second = f.actions()[2].input.clone();
    assert_ne!(first["request_id"], second["request_id"]);
    assert_eq!(first["parameters"], second["parameters"]);
    assert_eq!(f.runs.len(), 2);
    assert_eq!(
        f.runs["run-2"]["reserved_usd_micros"],
        first["parameters"]["reserved_usd_micros"]
    );
}

// spec: specs/ap/api.md#R12
#[test]
fn an_uncertain_submit_is_sent_once_and_leaves_the_run_running() {
    for answer in [
        None,
        Some((500, json!({"type": "error"}))),
        Some((200, json!({"processing_status": "in_progress"}))),
        Some((200, json!({"id": "a/b"}))),
        Some((200, json!({"id": ".."}))),
        Some((200, json!({"id": "a?b"}))),
        Some((200, json!({"id": "a#b"}))),
    ] {
        let mut f = Fake::new();
        f.requests(&many("claude-sonnet-5-5", 2, 1000));
        f.submit = answer.clone();
        let e = call(&mut f, "drift", &[]).unwrap_err();
        assert!(
            refused("submit_uncertain", "requests", 3)(&e),
            "{answer:?} {e:?}"
        );
        assert_eq!(
            f.urls(),
            ["count_tokens", "count_tokens", "batches"],
            "{answer:?}"
        );
        assert_eq!(f.actions().len(), 1, "no close");
        assert_eq!(f.runs["run-1"]["status"], "running");
        let record: Value =
            serde_json::from_slice(&fs::read(f.at("out/batch.json")).unwrap()).unwrap();
        assert_eq!(
            (record["state"].clone(), record["batch"].clone()),
            (json!("submit_uncertain"), Value::Null)
        );
    }
}

// spec: specs/ap/api.md#R13
#[test]
fn a_poll_without_an_end_for_25_hours_is_poll_timeout() {
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 1, 1000));
    f.never_end = true;
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("poll_timeout", "batch", 3)(&e), "{e:?}");
    assert!(
        f.offset >= Duration::from_secs(25 * 3_600)
            && f.offset < Duration::from_secs(25 * 3_600 + 61)
    );
    assert_eq!(
        f.urls().iter().filter(|u| u.ends_with("/results")).count(),
        0
    );
    assert_eq!(f.runs["run-1"]["status"], "running");
    let record: Value = serde_json::from_slice(&fs::read(f.at("out/batch.json")).unwrap()).unwrap();
    assert_eq!(record["state"], "poll_timeout");
    assert_eq!(listing(&f.at("run")), ["api.lock"]);
}

// spec: specs/ap/api.md#R13
#[test]
fn a_result_of_another_custom_id_is_results_unreadable() {
    let one = |id: &str| {
        format!(
            "{}\n",
            json!({"custom_id": id, "result": {"type": "expired"}})
        )
    };
    for results in [
        one("r0") + &one("stranger"),
        one("r0") + &one("r0") + &one("r1"),
        one("r0"),
        one("r0") + "not json\n",
        one("r0")
            + &format!(
                "{}\n",
                json!({"custom_id": "r1", "result": {"type": "lost"}})
            ),
    ] {
        let mut f = Fake::new();
        f.requests(&many("claude-sonnet-5-5", 2, 1000));
        f.results = results;
        let e = call(&mut f, "drift", &[]).unwrap_err();
        assert!(refused("results_unreadable", "results", 3)(&e), "{e:?}");
        assert_eq!(f.runs["run-1"]["status"], "running");
        assert_eq!(f.actions().len(), 1);
    }
}

// spec: specs/ap/api.md#R8
#[test]
fn a_held_lock_is_lock_busy_after_300_seconds() {
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 1, 1000));
    fs::create_dir(f.at("run")).unwrap();
    let held = fs::File::create(f.at("run/api.lock")).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("lock_busy", "run", 3)(&e), "{e:?}");
    assert_eq!(f.offset, Duration::from_secs(300));
    assert!(f.calls.is_empty() && !f.at("out").exists());
    // A run directory that is a link is invalid_input, exit 3.
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 1, 1000));
    fs::create_dir(f.at("elsewhere")).unwrap();
    std::os::unix::fs::symlink(f.at("elsewhere"), f.at("run")).unwrap();
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("invalid_input", "run", 3)(&e), "{e:?}");
    assert!(f.calls.is_empty());
}

// spec: specs/ap/api.md#R8
#[test]
fn a_key_directory_of_a_gone_process_is_removed_and_a_live_one_kept() {
    let mut f = Fake::new();
    fs::create_dir(f.at("run")).unwrap();
    let dead = f.at("run/key.999999999.0");
    let live = f.at(&format!("run/key.{}.0", std::process::id()));
    for d in [&dead, &live] {
        fs::create_dir(d).unwrap();
        fs::write(d.join("header"), "x-api-key: old\n").unwrap();
    }
    flow(&mut f);
    call(&mut f, "evals", &[]).unwrap();
    assert!(!dead.exists());
    assert!(live.join("header").exists());
    assert_eq!(
        listing(&f.at("run")),
        [
            "api.lock".to_owned(),
            format!("key.{}.0", std::process::id())
        ]
    );
}

// spec: specs/ap/api.md#R9
// spec: specs/ap/api.md#R10
#[test]
fn a_store_that_fails_or_a_start_that_does_not_apply_makes_no_batch_call() {
    // The spend cannot be read: nothing is called.
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 1, 1000));
    f.search_fails = true;
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("store_failed", "store", 3)(&e), "{e:?}");
    assert!(f.curls().is_empty() && f.actions().is_empty() && !f.at("out").exists());
    // A first start answered already_applied: store_failed, no submit, batch.json at store_failed.
    for start in [StartMode::AlreadyFirst, StartMode::Refused] {
        let mut f = Fake::new();
        f.requests(&many("claude-sonnet-5-5", 1, 1000));
        f.start_mode = start;
        let e = call(&mut f, "drift", &[]).unwrap_err();
        assert!(refused("store_failed", "store", 3)(&e), "{e:?}");
        assert_eq!(f.urls(), ["count_tokens"]);
        assert_eq!(f.actions().len(), 1);
        let record: Value =
            serde_json::from_slice(&fs::read(f.at("out/batch.json")).unwrap()).unwrap();
        assert_eq!(
            (record["state"].clone(), record["run"].clone()),
            (json!("store_failed"), Value::Null)
        );
    }
    // A start without an answer is sent once more with its id; that retry may answer already_applied.
    let mut f = Fake::new();
    flow(&mut f);
    f.start_mode = StartMode::SilentOnce;
    let v = call(&mut f, "evals", &[]).unwrap();
    let starts: Vec<&Called> = f
        .actions()
        .into_iter()
        .filter(|c| c.input["action_type"] == "start_run")
        .collect();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].input, starts[1].input);
    assert_eq!(v["run"], "run-1");
    assert_eq!(f.runs.len(), 1);
}

// spec: specs/ap/api.md#R10
#[test]
fn a_count_answered_429_is_count_failed_without_a_retry() {
    let mut f = Fake::new();
    f.requests(&many("claude-sonnet-5-5", 3, 1000));
    f.count_status = 429;
    let e = call(&mut f, "drift", &[]).unwrap_err();
    assert!(refused("count_failed", "requests", 3)(&e), "{e:?}");
    assert_eq!(f.urls(), ["count_tokens"]);
    assert!(f.actions().is_empty() && !f.at("out").exists());
}

// spec: specs/ap/api.md#R14
#[test]
fn a_close_that_fails_twice_is_close_failed() {
    let mut f = Fake::new();
    flow(&mut f);
    // The run is closed by another hand before the settlement: the close is refused.
    struct Closing(Fake);
    impl System for Closing {
        fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
            if input["action_type"] == "close_run" {
                self.0.runs.get_mut("run-1").unwrap()["status"] = json!("closed");
            }
            self.0.call(program, args, input, ms)
        }
        fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
            self.0.read(path)
        }
        fn now(&self) -> Instant {
            self.0.now()
        }
        fn wall(&self) -> SystemTime {
            self.0.wall()
        }
        fn pause(&mut self, d: Duration) {
            self.0.pause(d)
        }
        fn policy_file(&self) -> Option<PathBuf> {
            self.0.policy_file()
        }
        fn environment(&self) -> Vec<(String, String)> {
            self.0.environment()
        }
    }
    impl Api for Closing {
        fn examine(&self, path: &str) -> std::io::Result<Examined> {
            self.0.examine(path)
        }
    }
    let (req, out) = (f.path("requests.jsonl"), f.path("out"));
    let mut c = Closing(f);
    let argv: Vec<OsString> = ["batch", "evals", "--requests", &req, "--out", &out]
        .iter()
        .map(OsString::from)
        .collect();
    let e = batch(&mut c, &argv).unwrap_err();
    assert_eq!((e.failure.code, e.status), ("close_failed", 3));
    let record: Value =
        serde_json::from_slice(&fs::read(c.0.at("out/batch.json")).unwrap()).unwrap();
    assert_eq!(record["state"], "close_failed");
}

// spec: specs/ap/api.md#R17
// spec: specs/ap/api.md#R9
#[test]
fn api_spend_counts_settled_and_reserved_by_job() {
    let mut f = Fake::new();
    f.seed("a-settled", settled_run(NOW - 86_400, "drift", 5_000_000));
    f.seed(
        "b-running",
        json!({"role": "batch", "job": "drift", "status": "running", "started_at": utc(NOW - 60), "reserved_usd_micros": 3_000_000}),
    );
    f.seed(
        "c-closed-unsettled",
        json!({"role": "batch", "job": "mining", "status": "closed", "started_at": utc(NOW - 120), "reserved_usd_micros": 2_000_000}),
    );
    f.seed(
        "d-last-month",
        settled_run(NOW - 40 * 86_400, "drift", 7_000_000),
    );
    let mut other = settled_run(NOW - 60, "drift", 9_000_000);
    other["role"] = json!("implementer");
    f.seed("e-other-role", other);
    let v = spend(
        &mut f,
        "/scratch/store.sqlite",
        &json!({"month": "2027-01"}),
    )
    .unwrap();
    assert_eq!(
        v,
        json!({"month": "2027-01", "jobs": [
            {"job": "drift", "runs": 2, "running": 1, "settled_usd_micros": 5_000_000, "reserved_usd_micros": 3_000_000, "counted_usd_micros": 8_000_000},
            {"job": "mining", "runs": 1, "running": 0, "settled_usd_micros": 0, "reserved_usd_micros": 2_000_000, "counted_usd_micros": 2_000_000}],
            "settled_usd_micros": 5_000_000, "reserved_usd_micros": 5_000_000, "counted_usd_micros": 10_000_000})
    );
    assert!(f.calls.iter().all(|c| {
        c.args
            .windows(2)
            .any(|w| w == ["--store", "/scratch/store.sqlite"])
    }));
    let v = spend(
        &mut f,
        "/scratch/store.sqlite",
        &json!({"month": "2026-12"}),
    )
    .unwrap();
    assert_eq!(v["counted_usd_micros"], 7_000_000);
    let v = spend(
        &mut f,
        "/scratch/store.sqlite",
        &json!({"month": "2026-11"}),
    )
    .unwrap();
    assert_eq!(v["jobs"], json!([]));
    let calls = f.calls.len();
    for (p, field) in [
        (json!({"month": "2027-13"}), "month"),
        (json!({"month": "2027-00"}), "month"),
        (json!({"month": "2027-1"}), "month"),
        (json!({"month": 202701}), "month"),
        (json!({}), "month"),
        (json!({"month": "2027-01", "job": "drift"}), "job"),
        (json!(["2027-01"]), "stdin"),
    ] {
        let e = spend(&mut f, "/scratch/store.sqlite", &p).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", field), "{p}");
    }
    assert_eq!(f.calls.len(), calls, "a refusal reads nothing");
    f.search_fails = true;
    assert_eq!(
        spend(
            &mut f,
            "/scratch/store.sqlite",
            &json!({"month": "2027-01"})
        )
        .unwrap_err()
        .code,
        "process_failed"
    );
}

fn ap_bin(args: &[&str], env: &[(&str, &str)], stdin: &str) -> std::process::Output {
    use std::io::Write;
    let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
    c.args(args)
        .env_remove("ONT_FN_NAME")
        .env_remove("AP_API_RUN")
        .env("AP_POLICY_FILE", "/nonexistent/ap-api-test/policy.json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// spec: specs/ap/api.md#R1
// spec: specs/ap/api.md#R17
// spec: specs/ap/verbs.md#R4
#[test]
fn the_command_prints_its_usage_and_answers_the_function() {
    for args in [&["api", "--help"][..], &["api", "batch", "--help"][..]] {
        let out = ap_bin(args, &[], "");
        assert_eq!(out.status.code(), Some(0));
        assert!(
            String::from_utf8(out.stdout)
                .unwrap()
                .starts_with("Usage: ap api batch <job>")
        );
    }
    for args in [&["api"][..], &["api", "batch"][..], &["api", "status"][..]] {
        let out = ap_bin(args, &[], "");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let e: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(e["error"]["code"], "usage");
    }
    let out = ap_bin(
        &[
            "api",
            "batch",
            "drift",
            "--requests",
            "/nonexistent/r",
            "--out",
            "/nonexistent/o",
        ],
        &[],
        "",
    );
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(e["error"]["code"], "api_not_configured");
    // The function: a parameter it refuses is exit 2 before the store is read.
    let env = [
        ("ONT_FN_NAME", "api_spend"),
        ("ONT_FN_STORE", "/nonexistent/ap-api-test/store.sqlite"),
    ];
    let out = ap_bin(&[], &env, r#"{"month": "2027-13"}"#);
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (e["error"]["code"].clone(), e["error"]["field"].clone()),
        (json!("invalid_input"), json!("month"))
    );
    // Without ONT_FN_NAME, no argument is the usage refusal.
    let out = ap_bin(&[], &[], "");
    assert_eq!(out.status.code(), Some(2));
}

// The real engine.

struct Bed {
    root: PathBuf,
    ont: PathBuf,
}
impl Drop for Bed {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Bed {
    fn new() -> Self {
        let ont = PathBuf::from(std::env::var("ONT_BIN").expect("ONT_BIN names the engine"));
        let base = std::env::temp_dir().join(format!(
            "ap-api-engine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("bin")).unwrap();
        let root = fs::canonicalize(&base).unwrap();
        let ap = root.join("bin/ap");
        fs::copy(env!("CARGO_BIN_EXE_ap"), &ap).unwrap();
        fs::set_permissions(&ap, fs::Permissions::from_mode(0o755)).unwrap();
        let mut schema: Value =
            serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
        assert_eq!(
            schema["functions"]["api_spend"]["program"],
            "/usr/local/bin/ap"
        );
        schema["functions"]["api_spend"]["program"] = json!(ap.to_str().unwrap());
        fs::write(root.join("schema.json"), schema.to_string()).unwrap();
        let bed = Self { root, ont };
        let (code, out) = bed.ont(
            &[
                "schema",
                "install",
                bed.root.join("schema.json").to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, 0, "{out}");
        bed
    }
    fn ont(&self, args: &[&str], stdin: &str) -> (i32, Value) {
        use std::io::Write;
        let mut child = Command::new(&self.ont)
            .arg("--store")
            .arg(self.root.join("store.sqlite"))
            .args(args)
            .env_remove("ONT_ROLE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "ont {args:?}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.code().unwrap_or(-1), v)
    }
    /// An apply as `ap api batch` sends it through `store::start_run` and `store::close_run`.
    fn apply(&self, action: &str, rid: &str, parameters: Value) -> Value {
        let body = json!({"action_type": action, "request_id": rid, "agent": "ap-run", "role": "batch",
            "parameters": parameters});
        let (code, v) = self.ont(&["action", "apply", "--client", "ap"], &body.to_string());
        assert_eq!(code, 0, "{v}");
        v
    }
}

// spec: specs/ap/api.md#R17
// spec: specs/ap/api.md#R18
#[test]
#[ignore = "runs the real engine: ./check runs it with --ignored and ONT_BIN"]
fn engine_api_spend_answers_a_settled_and_a_running_batch_run() {
    let bed = Bed::new();
    let start = |rid: &str, job: &str, reserved: i64| {
        json!({"role": "batch", "runtime": "api", "model": "claude-sonnet-5-5", "account": "anthropic-api-key",
            "uses": oid("account", "anthropic-api-key"), "job": job, "reserved_usd_micros": reserved,
            "record": format!("/fixture/{rid}/batch.json")})
    };
    let first = bed.apply(
        "start_run",
        "api-fixture-1-start",
        start("one", "drift", 4_000_000),
    );
    assert_eq!(first["result"]["outcome"], "applied");
    let run = first["result"]["edits"][0]["id"].clone();
    // The same id and parameters once more: already_applied with the earlier edits.
    let again = bed.apply(
        "start_run",
        "api-fixture-1-start",
        start("one", "drift", 4_000_000),
    );
    assert_eq!(again["result"]["outcome"], "already_applied");
    assert_eq!(again["result"]["edits"][0]["id"], run);
    bed.apply(
        "close_run",
        "api-fixture-1-close",
        json!({"run": run, "state": "ended", "session": "msgbatch_fixture", "input_tokens": 10,
            "output_tokens": 20, "settled_usd_micros": 1_500_000}),
    );
    bed.apply(
        "start_run",
        "api-fixture-2-start",
        start("two", "mining", 2_000_000),
    );
    // A stage's run, without the new parameters, is no batch run.
    let body = json!({"action_type": "start_run", "request_id": "run-fixture-start", "agent": "ap-run",
        "role": "implementer", "parameters": {"role": "implementer", "runtime": "claude", "model": "m"}});
    assert_eq!(
        bed.ont(&["action", "apply", "--client", "ap"], &body.to_string())
            .0,
        0
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let month = utc(now)[..7].to_owned();
    let (code, v) = bed.ont(
        &["fn", "api_spend"],
        &json!({"parameters": {"month": month}}).to_string(),
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(
        v["answer"],
        json!({"month": month, "jobs": [
            {"job": "drift", "runs": 1, "running": 0, "settled_usd_micros": 1_500_000, "reserved_usd_micros": 0, "counted_usd_micros": 1_500_000},
            {"job": "mining", "runs": 1, "running": 1, "settled_usd_micros": 0, "reserved_usd_micros": 2_000_000, "counted_usd_micros": 2_000_000}],
            "settled_usd_micros": 1_500_000, "reserved_usd_micros": 2_000_000, "counted_usd_micros": 3_500_000}),
        "{v}"
    );
}
