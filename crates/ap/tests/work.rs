//! `ap work` (`specs/ap/work.md`) against a fake host: the fake answers the store (`ont search`, `get`,
//! `action validate` and `action apply` of `note_unit_wait` as the engine answers them), `systemctl
//! list-units`, `systemd-run`, `git check-ref-format`, `/proc/stat` and the control group's file itself.
//! The policy file and the units directory are under a temporary directory: no test starts a unit of
//! the service manager or reaches the host's store, policy file or units directory.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
#[path = "fixtures/work/forks.rs"]
mod forks;
use ap::host::{Answer, Failure, Reply, System};
use ap::work::{Observed, arguments, execute, observe, overlap};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsString,
    fs::File,
    io::ErrorKind,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const STORE: &str = "/invented/scratch/store.sqlite";
const PREFIX: &str = "aptest";
/// The prefix of the writers' units (R24).
const PAGES: &str = "aptestpg";
const SLICE: &str = "aptest.slice";
const CPU_STAT: &str = "/sys/fs/cgroup/aptest.slice/cpu.stat";
const UUID: &str = "0123abcd-4567-89ef-0123-456789abcdef";
/// The request of delivery, and the record of its default state directory (R26).
const DELIVER: &str = "/etc/agent-platform/deliver.json";
const DEFAULT_RECORD: &str = "/var/lib/agent-platform/deliver/last.json";

fn reply(success: bool, v: &Value) -> Answer {
    let bytes = serde_json::to_vec(v).unwrap();
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
fn text(success: bool, t: &str) -> Answer {
    Ok(Reply {
        success,
        stdout: t.as_bytes().to_vec(),
        bytes: t.as_bytes().to_vec(),
    })
}
fn broken(program: &str) -> Answer {
    Err(Failure {
        code: "process_failed",
        field: program.into(),
        detail: Value::Null,
    })
}
fn wrap(fields: &Value) -> Value {
    let o = fields.as_object().unwrap().iter();
    o.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
        .collect()
}
fn uid(key: &str) -> Value {
    json!({"object_type": "unit", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": key})
}
fn stat(cpus: u64, user: u64, idle: u64) -> String {
    let mut t = format!("cpu  {user} 0 0 {idle} 0 0 0 0 0 0\n");
    for n in 0..cpus {
        t += &format!("cpu{n} 1 2 3 4 5 6 7 8 9 10\n");
    }
    t
}

/// The fake host.
struct W {
    dir: PathBuf,
    policy: Option<PathBuf>,
    units: bool,
    env: Vec<(String, String)>,
    /// The units of the store: the revision counter and the fields.
    objects: BTreeMap<String, (u64, Value)>,
    /// The lines `systemctl list-units` prints; None is a failed call.
    listed: Option<String>,
    /// The keys whose `systemd-run` fails.
    run_fails: BTreeSet<String>,
    /// The codes `start_unit` and `merge_unit` fail beside the fake's own rules, per unit.
    refuse: BTreeMap<String, Vec<String>>,
    /// The units whose `note_unit_wait` the store refuses `unit_not_ready`.
    moved: BTreeSet<String>,
    /// The store fails every call after this many applies.
    store_dies_after: Option<usize>,
    applies: usize,
    calls: RefCell<Vec<(String, Vec<String>, Value)>>,
    reads: RefCell<Vec<String>>,
    proc: RefCell<VecDeque<String>>,
    cgroup: RefCell<VecDeque<String>>,
    base: Instant,
    clock: Cell<Duration>,
    /// A lock the test holds, let go at the first pause.
    held: Option<Held>,
    /// The objects of the store that are not units (intents, reviews), by kind and key.
    others: BTreeMap<(String, String), Value>,
    /// The lines `systemctl list-units` prints for the writers' units.
    listed_pages: String,
    /// Every call of `ont` fails.
    broken: bool,
    /// The bytes of the request of delivery (`/etc/agent-platform/deliver.json`); None: absent.
    deliver: Option<String>,
    /// The pairs `(clone, commit, tip)` for which `git merge-base --is-ancestor` succeeds.
    ancestors: BTreeSet<(String, String, String)>,
}
impl W {
    fn new() -> Self {
        forks::gap();
        let dir = std::env::temp_dir().join(format!(
            "ap-work-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a1")).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let mut w = W {
            policy: Some(dir.join("policy.json")),
            units: true,
            env: vec![
                ("PATH".into(), "/invented/bin:/usr/bin".into()),
                ("HOME".into(), "/invented/home".into()),
                ("LANG".into(), "C.UTF-8".into()),
                ("FOO".into(), "not kept".into()),
                (
                    "AP_POLICY_FILE".into(),
                    dir.join("policy.json").to_string_lossy().into_owned(),
                ),
            ],
            dir,
            objects: BTreeMap::new(),
            listed: Some(String::new()),
            run_fails: BTreeSet::new(),
            refuse: BTreeMap::new(),
            moved: BTreeSet::new(),
            store_dies_after: None,
            applies: 0,
            calls: RefCell::default(),
            reads: RefCell::default(),
            proc: RefCell::default(),
            cgroup: RefCell::default(),
            base: Instant::now(),
            clock: Cell::default(),
            held: None,
            others: BTreeMap::new(),
            listed_pages: String::new(),
            broken: false,
            deliver: None,
            ancestors: BTreeSet::new(),
        };
        w.policy_with(json!({}));
        w
    }
    fn p(&self, rel: &str) -> String {
        self.dir.join(rel).to_string_lossy().into_owned()
    }
    fn root(&self) -> PathBuf {
        self.dir.join("units")
    }
    /// The trial's policy with `extra`'s members over it (a null member left out).
    fn policy_with(&mut self, extra: Value) {
        let mut policy = json!({"models": {"allowed": ["opus"], "never": []},
            "accounts": {"claude": [self.p("a1")]},
            "repositories": {"o/r": {"clone": self.p("clone-r")}, "o/s": {"clone": self.p("clone-s")}},
            "reviews": {"task": "invented-task"},
            "units": {"directory": self.p("units")},
            "work": {"prefix": PREFIX, "slice": SLICE}});
        for (k, v) in extra.as_object().unwrap() {
            match v {
                Value::Null => {
                    policy.as_object_mut().unwrap().remove(k);
                }
                v => policy[k] = v.clone(),
            }
        }
        std::fs::write(self.dir.join("policy.json"), policy.to_string()).unwrap();
    }
    /// A ready unit of `o/r` with the write set `ws`, ready at second `at`.
    fn ready(&mut self, key: &str, ws: &[&str], at: u32) {
        self.put(
            key,
            json!({"lane": "small", "class": "plain", "objective": "invented", "repository": "o/r",
            "write_set": ws, "state": "ready", "ready_at": format!("2026-11-18T10:00:{at:02}Z"),
            "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
            "verifier": {"runtime": "claude", "model": "opus", "effort": "high"}}),
        );
    }
    fn put(&mut self, key: &str, f: Value) {
        let n = self.objects.get(key).map_or(1, |o| o.0 + 1);
        self.objects.insert(key.into(), (n, f));
    }
    fn set(&mut self, key: &str, set: Value) {
        let o = self.objects.get_mut(key).unwrap();
        for (k, v) in set.as_object().unwrap() {
            o.1[k] = v.clone();
        }
        o.0 += 1;
    }
    fn f(&self, key: &str) -> &Value {
        &self.objects[key].1
    }
    fn revision(&self, key: &str) -> String {
        format!("r#{}", self.objects[key].0)
    }
    fn live(&mut self, lines: &[&str]) {
        self.listed = Some(lines.iter().map(|l| format!("{l}\n")).collect());
    }
    fn pass(&mut self) -> Result<Value, Box<ap::work::Refusal>> {
        execute(self, Some(STORE))
    }
    fn ok(&mut self) -> Value {
        self.pass().unwrap_or_else(|e| panic!("refused: {e:?}"))
    }
    fn refused(&mut self) -> (String, u8) {
        match self.pass() {
            Err(e) => (format!("{}@{}", e.failure.code, e.failure.field), e.status),
            Ok(v) => panic!("expected a refusal, got {v}"),
        }
    }
    fn calls_of(&self, program: &str) -> Vec<Vec<String>> {
        let calls = self.calls.borrow();
        calls
            .iter()
            .filter(|c| c.0 == program)
            .map(|c| c.1.clone())
            .collect()
    }
    fn bodies(&self, what: &str) -> Vec<Value> {
        let calls = self.calls.borrow();
        calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1[0] == "action" && c.1[1] == what)
            .map(|c| c.2.clone())
            .collect()
    }
    fn notes(&self) -> Vec<(String, String)> {
        let notes = self.bodies("apply");
        notes
            .iter()
            .map(|b| {
                assert_eq!(b["action_type"], "note_unit_wait");
                let p = &b["parameters"];
                (
                    p["unit"]["native_key"].as_str().unwrap().to_owned(),
                    p["waits_on"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
    fn gets(&self) -> Vec<String> {
        let calls = self.calls.borrow();
        calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1[0] == "get")
            .map(|c| c.2["id"]["native_key"].as_str().unwrap().to_owned())
            .collect()
    }
    fn reset(&mut self) {
        self.calls.borrow_mut().clear();
        self.reads.borrow_mut().clear();
    }
    /// Two readings of the load: `cpus` CPUs at `busy` per cent of all, the slice at `inside` per cent
    /// of one CPU, over ten seconds.
    fn load(&mut self, cpus: u64, busy: u64, inside: u64) {
        let (user, idle) = (busy * 10 * cpus, (100 - busy) * 10 * cpus);
        self.proc = RefCell::new(VecDeque::from([
            stat(cpus, 1_000, 9_000),
            stat(cpus, 1_000 + user, 9_000 + idle),
        ]));
        self.cgroup = RefCell::new(VecDeque::from([
            "usage_usec 5000000\n".to_owned(),
            format!("usage_usec {}\n", 5_000_000 + inside * 100_000),
        ]));
    }
    /// The criteria of `start_unit` or `merge_unit` that fail for `p`.
    fn failed(&self, action: &str, p: &Value) -> Vec<String> {
        let key = p["unit"]["native_key"].as_str().unwrap();
        let mut failed = vec![];
        let Some((_, u)) = self.objects.get(key) else {
            return vec!["unit_not_ready".into()];
        };
        if action == "start_unit" {
            if u["state"] != "ready" {
                failed.push("unit_not_ready".to_owned());
            }
            let deps = u["depends_on"].as_array().cloned().unwrap_or_default();
            let merged = |d: &Value| {
                let k = d["native_key"].as_str().unwrap();
                self.objects
                    .get(k)
                    .is_some_and(|o| o.1["state"] == "merged")
            };
            if !deps.iter().all(merged) {
                failed.push("dependency_not_merged".to_owned());
            }
        } else {
            if u["state"] != "accepted" {
                failed.push("unit_not_accepted".to_owned());
            }
            if p["head"] != u["head"] {
                failed.push("head_differs".to_owned());
            }
            assert_eq!(p["merged_commit"], "0".repeat(40));
        }
        if u["lane"] == "objective" && p["intent"] != u["intent"] {
            failed.push("intent_differs".to_owned());
        }
        failed.extend(self.refuse.get(key).cloned().unwrap_or_default());
        failed
    }
    fn ont(&mut self, args: &[String], input: &Value) -> Answer {
        assert_eq!(args[args.len() - 2..], ["--store", STORE], "{args:?}");
        if self.broken {
            return broken("ont");
        }
        if self
            .store_dies_after
            .is_some_and(|n| self.applies >= n && n > 0)
        {
            return broken("ont");
        }
        match args[0].as_str() {
            "search" => {
                assert_eq!(input["object_type"], "unit");
                let states = &input["where"]["all"][0]["in"];
                assert_eq!(states[0], "state");
                let states = states[1].as_array().unwrap();
                let objects: Vec<Value> = self
                    .objects
                    .iter()
                    .filter(|(_, (_, f))| states.contains(&f["state"]))
                    .map(|(k, (_, f))| json!({"id": uid(k), "head": {"fields": wrap(f)}}))
                    .collect();
                reply(
                    true,
                    &json!({"status": "ok", "result": {"basis": {"qualified": true},
                    "objects": objects, "continuation": null}}),
                )
            }
            "get" if input["id"]["object_type"] != "unit" => {
                let id = &input["id"];
                let at = (
                    id["object_type"].as_str().unwrap().to_owned(),
                    id["native_key"].as_str().unwrap().to_owned(),
                );
                let Some(f) = self.others.get(&at) else {
                    return reply(false, &json!({"status": "refused", "code": "unavailable"}));
                };
                reply(
                    true,
                    &json!({"status": "ok", "result": {"objects": [{"id": id, "head": "r#1",
                    "observations": [{"revision": "r#1", "fields": wrap(f)}]}]}}),
                )
            }
            "render" => {
                assert_eq!(input["view"], "reviews_of_unit");
                let unit = &input["args"]["unit"];
                let rows: Vec<&String> = self
                    .others
                    .iter()
                    .filter(|((t, _), r)| t == "review" && r["unit"]["native_key"] == *unit)
                    .map(|((_, k), _)| k)
                    .collect();
                let mut table = "| Key | Kind |\n| --- | --- |\n".to_owned();
                for k in &rows {
                    table.push_str(&format!("| {k} | page |\n"));
                }
                reply(
                    true,
                    &json!({"status": "ok", "markdown": table, "rows": rows.len(), "truncated": false}),
                )
            }
            "get" => {
                let id = &input["id"];
                assert_eq!(id["object_type"], "unit");
                let key = id["native_key"].as_str().unwrap();
                let Some((n, f)) = self.objects.get(key) else {
                    return reply(false, &json!({"status": "refused", "code": "unavailable"}));
                };
                let r = format!("r#{n}");
                reply(
                    true,
                    &json!({"status": "ok", "result": {"objects": [{"id": id, "head": r,
                    "observations": [{"revision": r, "fields": wrap(f)}]}]}}),
                )
            }
            "action" if args[1] == "validate" => {
                let action = input["action_type"].as_str().unwrap();
                let failed = self.failed(action, &input["parameters"]);
                let criteria: Vec<Value> = failed
                    .iter()
                    .map(|c| json!({"code": c, "passed": false}))
                    .chain([json!({"code": "held", "passed": true})])
                    .collect();
                reply(
                    true,
                    &json!({"status": "ok", "result": {"valid": failed.is_empty(), "criteria": criteria}}),
                )
            }
            "action" => {
                assert_eq!(args[1], "apply");
                assert_eq!(input["action_type"], "note_unit_wait", "{input}");
                self.applies += 1;
                let key = input["parameters"]["unit"]["native_key"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                if self.moved.contains(&key) || self.f(&key)["state"] != "ready" {
                    let refusal = json!({"status": "refused", "code": "submission_criteria",
                        "failed": ["unit_not_ready"]});
                    return reply(false, &refusal);
                }
                self.set(&key, json!({"waits_on": input["parameters"]["waits_on"]}));
                let r = self.revision(&key);
                reply(
                    true,
                    &json!({"status": "ok", "result": {"outcome": "applied",
                    "edits": [{"change": "update", "id": uid(&key), "revision": r}]}}),
                )
            }
            other => panic!("ont {other}"),
        }
    }
}
impl Drop for W {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl System for W {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _: u64) -> Answer {
        self.calls
            .borrow_mut()
            .push((program.to_owned(), args.to_vec(), input.clone()));
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        match (program, &words[..]) {
            ("ont", _) => self.ont(args, input),
            ("git", ["check-ref-format", "--branch", _]) => text(true, ""),
            (
                "git",
                [
                    "--no-replace-objects",
                    "-C",
                    clone,
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "core.fsmonitor=false",
                    "merge-base",
                    "--is-ancestor",
                    commit,
                    tip,
                ],
            ) => {
                let pair = ((*clone).to_owned(), (*commit).to_owned(), (*tip).to_owned());
                text(self.ancestors.contains(&pair), "")
            }
            ("systemctl", ["list-units", "--all", "--plain", "--no-legend", pattern])
                if *pattern == format!("{PAGES}-*") =>
            {
                text(true, &self.listed_pages)
            }
            ("systemctl", ["list-units", "--all", "--plain", "--no-legend", pattern]) => {
                assert_eq!(*pattern, format!("{PREFIX}-*"));
                match &self.listed {
                    Some(t) => text(true, t),
                    None => broken("systemctl"),
                }
            }
            ("systemctl", ["show", "-p", "ControlGroup", "--value", SLICE]) => {
                text(true, &format!("/{SLICE}\n"))
            }
            ("systemd-run", ["--unit", unit, ..]) => {
                let key = (unit.strip_prefix(&format!("{PAGES}-")))
                    .or_else(|| unit.strip_prefix(&format!("{PREFIX}-")))
                    .unwrap();
                assert!(!words.contains(&"--wait"), "the pass waited: {words:?}");
                text(!self.run_fails.contains(key), "")
            }
            _ => panic!("a call the pass does not make: {program} {words:?}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_owned());
        match path {
            "/proc/sys/kernel/random/uuid" => Ok(format!("{UUID}\n").into_bytes()),
            DELIVER => self
                .deliver
                .clone()
                .map(String::into_bytes)
                .ok_or_else(|| ErrorKind::NotFound.into()),
            DEFAULT_RECORD => Err(ErrorKind::NotFound.into()),
            "/proc/stat" => {
                let next = self.proc.borrow_mut().pop_front();
                Ok(next.expect("one reading too many").into_bytes())
            }
            CPU_STAT => {
                let next = self.cgroup.borrow_mut().pop_front();
                next.map(String::into_bytes)
                    .ok_or_else(|| ErrorKind::NotFound.into())
            }
            _ => {
                assert!(
                    path.starts_with(self.dir.to_str().unwrap()),
                    "a read of {path}"
                );
                std::fs::read(path)
            }
        }
    }
    fn units(&self) -> bool {
        self.units
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
    fn now(&self) -> Instant {
        self.base + self.clock.get()
    }
    fn pause(&mut self, d: Duration) {
        self.held = None;
        self.clock.set(self.clock.get() + d);
    }
    fn launch(
        &mut self,
        argv: &[String],
        _: &[(String, String)],
        _: &str,
        _: [std::fs::File; 3],
    ) -> std::io::Result<std::process::Child> {
        panic!("the pass launched {argv:?}")
    }
}
/// A lock the test holds, let go by an explicit `LOCK_UN` before its descriptor closes: a child another
/// test's thread forked holds a copy of the descriptor until its `exec` (`fixtures/work/forks.rs`), and
/// a close alone would leave the lock held through that copy.
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock).unwrap();
    }
}
/// An exclusive lock on `path` (made), held by the test.
fn hold(path: &Path) -> Held {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let f = File::create(path).unwrap();
    rustix::fs::flock(&f, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    Held(f)
}
fn keys(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect()
}
fn waiting(v: &Value) -> Vec<(String, String)> {
    v["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            (
                w["unit"].as_str().unwrap().to_owned(),
                w["waits_on"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}
fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}
/// The files under `dir`, relative, with a `/` after a directory.
fn tree(dir: &Path) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let rel = e
                .path()
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if e.file_type().unwrap().is_dir() {
                all.insert(format!("{rel}/"));
                todo.push(e.path());
            } else {
                all.insert(rel);
            }
        }
    }
    all
}

/// The command `ap` with exactly `env`, `PATH` being the directory of stand-ins that log each call.
fn ap_cli(args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
    c.args(args).env_clear().stdin(Stdio::null());
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

// spec: specs/ap/work.md#R1
#[test]
fn r1_the_form_and_the_refusals_before_any_effect() {
    let a = |words: &[&str]| -> Vec<OsString> { words.iter().map(OsString::from).collect() };
    assert_eq!(arguments(&a(&[])).unwrap(), None);
    assert_eq!(
        arguments(&a(&["--store", "/s"])).unwrap().as_deref(),
        Some("/s")
    );
    for bad in [
        &["now"][..],
        &["--store"],
        &["--store", "s"],
        &["--store", "/s", "--store", "/t"],
        &["--now"],
    ] {
        assert_eq!(arguments(&a(bad)).unwrap_err().code, "usage", "{bad:?}");
    }
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.env.push(("ONT_ROLE".into(), "lead".into()));
    assert_eq!(w.refused(), ("role_in_environment@ONT_ROLE".into(), 2));
    w.env.pop();
    // A trial's pass never reads the host's store; a scratch store never runs under the host's policy.
    match execute(&mut w, None) {
        Err(e) => assert_eq!(
            (e.failure.code, e.failure.field.as_str(), e.status),
            ("invalid_input", "store", 2)
        ),
        Ok(v) => panic!("{v}"),
    }
    w.env.retain(|(k, _)| k != "AP_POLICY_FILE");
    assert_eq!(w.refused(), ("invalid_input@policy".into(), 2));
    w.env.push(("AP_POLICY_FILE".into(), String::new()));
    assert_eq!(w.refused(), ("invalid_input@policy".into(), 2));
    assert!(w.calls.borrow().is_empty() && !w.root().exists());
}

// spec: specs/ap/work.md#R2
#[test]
fn r2_each_skip_makes_no_call_and_no_file() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    let skipped = |w: &mut W, why: &str| {
        assert_eq!(w.ok(), json!({ "skipped": why }));
        assert!(w.calls.borrow().is_empty(), "{:?}", w.calls.borrow());
        assert!(!w.root().exists());
    };
    w.policy = None;
    skipped(&mut w, "no policy file");
    w.policy = Some(w.dir.join("absent.json"));
    skipped(&mut w, "no policy file");
    w.policy = Some(w.dir.join("policy.json"));
    w.policy_with(json!({"work": null}));
    skipped(&mut w, "no work");
    w.policy_with(json!({}));
    w.units = false;
    skipped(&mut w, "no service manager");
    std::fs::write(w.dir.join("policy.json"), "[1]").unwrap();
    let e = w.pass().unwrap_err();
    assert_eq!((e.failure.code, e.status), ("policy_invalid", 2));
    assert!(e.failure.detail.as_str().unwrap().starts_with("file"));
}

// spec: specs/ap/work.md#R3
#[test]
fn r3_the_member_work_its_defaults_and_its_forms() {
    let mut w = W::new();
    let bad = [
        json!(1),
        json!({"slots": 0}),
        json!({"slots": 17}),
        json!({"slots": "3"}),
        json!({"slice": "a-b.slice"}),
        json!({"slice": "ab"}),
        json!({"prefix": "-ap"}),
        json!({"prefix": "ap-"}),
        json!({"prefix": "1ap"}),
        json!({"prefix": "Ap"}),
        json!({"prefix": "abcdefghijklmnopq"}),
        json!({"other": 1}),
    ];
    for work in bad {
        w.policy_with(json!({ "work": work }));
        let e = w.pass().unwrap_err();
        assert_eq!((e.failure.code, e.status), ("policy_invalid", 2), "{work}");
        assert!(
            e.failure.detail.as_str().unwrap().starts_with("work"),
            "{work}"
        );
    }
    // With `--store`, the trial names its directory, prefix and slice.
    for extra in [
        json!({"work": {"slice": SLICE}}),
        json!({"work": {"prefix": PREFIX}}),
        json!({"units": null}),
        json!({"units": {}}),
    ] {
        w.policy_with(extra.clone());
        assert_eq!(w.refused(), ("invalid_input@policy".into(), 2), "{extra}");
    }
    w.policy_with(json!({"units": {"directory": "relative"}}));
    assert_eq!(w.refused(), ("policy_invalid@policy".into(), 2));
    w.policy_with(json!({"quota": {"slice": "a-b.slice"}}));
    assert_eq!(w.refused(), ("policy_invalid@policy".into(), 2));
    assert!(w.calls.borrow().is_empty() && !w.root().exists());
    // The defaults: three slots, `ap-unit` and `agents.slice`, read by the host's own pass.
    w.policy_with(json!({"work": {}}));
    w.env.retain(|(k, _)| k != "AP_POLICY_FILE");
    for k in ["a", "b", "c", "d"] {
        w.ready(k, &[&format!("{k}/**")], 1);
    }
    w.listed = Some(String::new());
    let mut host = HostDefaults(w);
    let v = execute(&mut host, None).unwrap();
    assert_eq!(keys(&v["started"]), ["a", "b", "c"]);
    assert_eq!(waiting(&v), pairs(&[("d", "slots 3/3")]));
    let runs = host.0.calls_of("systemd-run");
    assert_eq!(
        runs[0][..4],
        ["--unit", "ap-unit-a", "--slice", "agents.slice"]
    );
    assert!(!runs[0].iter().any(|a| a == "--store"));
}
/// The fake under the default prefix and slice.
struct HostDefaults(W);
impl System for HostDefaults {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        let mut args = args.to_vec();
        match program {
            "ont" => args.extend(["--store".into(), STORE.into()]),
            "systemctl" => {
                assert_eq!(args.last().unwrap(), "ap-unit-*");
                *args.last_mut().unwrap() = format!("{PREFIX}-*");
            }
            "systemd-run" => {
                assert!(args[1].starts_with("ap-unit-"));
                self.0
                    .calls
                    .borrow_mut()
                    .push((program.into(), args.clone(), input.clone()));
                return text(true, "");
            }
            _ => {}
        }
        self.0.call(program, &args, input, ms)
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.0.read(path)
    }
    fn units(&self) -> bool {
        true
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.0.policy_file()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.0.environment()
    }
    fn now(&self) -> Instant {
        self.0.now()
    }
    fn pause(&mut self, d: Duration) {
        self.0.pause(d)
    }
}

// spec: specs/ap/work.md#R4
#[test]
fn r4_the_pass_waits_for_its_lock_and_reads_the_store_only_with_it() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    // Held all the time: 120 seconds, then the skip, and no call to the store.
    std::fs::create_dir_all(w.root()).unwrap();
    let held = hold(&w.root().join("work.lock"));
    struct Keep<'a>(&'a mut W);
    impl System for Keep<'_> {
        fn call(&mut self, p: &str, a: &[String], i: &Value, ms: u64) -> Answer {
            self.0.call(p, a, i, ms)
        }
        fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
            self.0.read(path)
        }
        fn policy_file(&self) -> Option<PathBuf> {
            self.0.policy_file()
        }
        fn environment(&self) -> Vec<(String, String)> {
            self.0.environment()
        }
        fn now(&self) -> Instant {
            self.0.now()
        }
        fn pause(&mut self, d: Duration) {
            self.0.clock.set(self.0.clock.get() + d);
        }
        fn units(&self) -> bool {
            true
        }
    }
    let v = execute(&mut Keep(&mut w), Some(STORE)).unwrap();
    assert_eq!(v, json!({"skipped": "pass_running"}));
    assert!(w.calls.borrow().is_empty());
    assert_eq!(w.clock.get(), Duration::from_secs(120));
    // Let go at the first pause: the pass then reads the store and starts.
    w.held = Some(held);
    w.clock.set(Duration::ZERO);
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["a"]);
    assert_eq!(w.clock.get(), Duration::from_secs(1));
    // The directory is made with mode 0700 when absent.
    let mut w = W::new();
    w.ok();
    let mode = std::fs::metadata(w.root()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    let lock = std::fs::metadata(w.root().join("work.lock")).unwrap();
    assert_eq!(lock.permissions().mode() & 0o777, 0o600);
}

// spec: specs/ap/work.md#R5
#[test]
fn r5_the_reading_of_the_store_and_of_the_service_manager() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.put("d", json!({"state": "draft"}));
    w.live(&[
        "aptest-x.scope loaded active running x",
        "aptestx-y.service loaded active running y",
        "aptest-z.service loaded failed failed z",
        "aptest-q.service loaded activating start q",
    ]);
    let v = w.ok();
    let search = w.calls.borrow()[0].clone();
    assert_eq!(search.0, "ont");
    assert_eq!(
        search.2["where"]["all"],
        json!([{"in": ["state", ["ready", "running", "accepted"]]}])
    );
    // Only `aptest-q.service` is a live run unit; `d` is not read.
    assert_eq!(keys(&v["in_flight"]), ["aptest-q.service"]);
    assert_eq!(keys(&v["started"]), ["a"]);
    assert!(!w.gets().contains(&"d".to_owned()));
    for listed in [None, Some("aptest-a.service loaded\n".to_owned())] {
        let mut w = W::new();
        w.ready("a", &["a/**"], 1);
        w.listed = listed;
        assert_eq!(w.refused(), ("reading_failed@list_units".into(), 3));
        assert!(w.calls_of("systemd-run").is_empty());
    }
}

// spec: specs/ap/work.md#R6
#[test]
fn r6_in_flight_by_a_live_run_unit_or_a_held_lock_and_keys_of_another_form() {
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 4}}));
    w.ready("Bad_Key", &["z/**"], 0);
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.ready("c", &["c/**"], 3);
    w.live(&[
        "aptest-a.service loaded active running a",
        "aptest-gone.service loaded deactivating stop-sigterm gone",
    ]);
    let lock = hold(&w.root().join("b").join("lock"));
    let v = w.ok();
    assert_eq!(keys(&v["in_flight"]), ["a", "b", "aptest-gone.service"]);
    // Three in flight of four slots: one start.
    assert_eq!(keys(&v["started"]), ["c"]);
    assert!(!w.gets().contains(&"Bad_Key".to_owned()));
    assert!(!w.root().join("Bad_Key").exists());
    // The test of a lock makes no file: `c` has none and none was made.
    assert!(!w.root().join("c").join("lock").exists());
    drop(lock);
    // A lock that no process holds is not in flight.
    let mut w = W::new();
    w.ready("b", &["b/**"], 2);
    std::fs::create_dir_all(w.root().join("b")).unwrap();
    std::fs::write(w.root().join("b").join("lock"), "").unwrap();
    assert_eq!(keys(&w.ok()["started"]), ["b"]);
}

// spec: specs/ap/work.md#R7
#[test]
fn r7_a_running_unit_no_run_holds_is_cut_and_never_started() {
    let mut w = W::new();
    w.put(
        "r",
        json!({"state": "running", "repository": "o/r", "write_set": ["r/**"]}),
    );
    w.put(
        "s",
        json!({"state": "running", "repository": "o/r", "write_set": ["s/**"]}),
    );
    w.live(&["aptest-s.service loaded active running s"]);
    let v = w.ok();
    assert_eq!(keys(&v["cut"]), ["r"]);
    assert_eq!(keys(&v["in_flight"]), ["s"]);
    assert!(w.calls_of("systemd-run").is_empty() && w.gets().is_empty());
}

// spec: specs/ap/work.md#R8
#[test]
fn r8_the_candidates_their_order_and_their_one_read() {
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 16}}));
    let head = "a".repeat(40);
    let accepted = |class: &str, approved: Option<&str>| {
        let mut f = json!({"state": "accepted", "class": class, "lane": "small", "repository": "o/r",
            "head": head, "write_set": ["x/**"],
            "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
            "verifier": {"runtime": "claude", "model": "opus", "effort": "high"}});
        if let Some(a) = approved {
            f["approved_head"] = json!(a);
        }
        f
    };
    w.ready("r-late", &["l/**"], 9);
    w.ready("r-b", &["b/**"], 1);
    w.ready("r-a", &["a/**"], 1);
    w.put("m-plain", accepted("plain", None));
    w.put("m-approved", accepted("contract", Some(&head)));
    w.put("m-waits", accepted("contract", None));
    w.put("m-old", accepted("effect", Some(&"b".repeat(40))));
    let v = w.ok();
    assert_eq!(
        keys(&v["started"]),
        ["m-approved", "m-plain", "r-a", "r-b", "r-late"]
    );
    assert_eq!(w.gets(), ["m-approved", "m-plain", "r-a", "r-b", "r-late"]);
    let merges = w.bodies("validate");
    assert_eq!(merges[0]["action_type"], "merge_unit");
    assert_eq!(merges[0]["parameters"]["head"], head);
    // The read decides: a unit that moved out of `ready` since the search is skipped.
    struct Moving(W);
    impl System for Moving {
        fn call(&mut self, p: &str, a: &[String], i: &Value, ms: u64) -> Answer {
            if p == "ont" && a[0] == "get" {
                self.0.set("x", json!({"state": "running"}));
            }
            self.0.call(p, a, i, ms)
        }
        fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
            self.0.read(path)
        }
        fn policy_file(&self) -> Option<PathBuf> {
            self.0.policy_file()
        }
        fn environment(&self) -> Vec<(String, String)> {
            self.0.environment()
        }
        fn units(&self) -> bool {
            true
        }
    }
    let mut m = Moving(W::new());
    m.0.ready("x", &["x/**"], 1);
    let v = execute(&mut m, Some(STORE)).unwrap();
    assert_eq!(v["started"], json!([]));
    assert_eq!(v["waiting"], json!([]));
    // A `waits_on` is cut to 300 bytes.
    let mut w = W::new();
    w.ready("long", &["l/**"], 1);
    let codes: Vec<String> = (0..40).map(|n| format!("invented_code_{n:02}")).collect();
    w.refuse.insert("long".into(), codes);
    let v = w.ok();
    let why = &waiting(&v)[0].1;
    assert_eq!(why.len(), 300);
    assert!(why.starts_with("invented_code_00, invented_code_01"));
    assert_eq!(w.notes(), [("long".to_owned(), why.clone())]);
}

// spec: specs/ap/work.md#R9
#[test]
fn r9_a_unit_is_started_once_for_one_revision() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.ok();
    let started = std::fs::read_to_string(w.root().join("a").join("started")).unwrap();
    assert_eq!(started, w.revision("a"));
    // The run ended with the unit as it was: no second start.
    w.reset();
    let v = w.ok();
    assert_eq!(keys(&v["not_restarted"]), ["a"]);
    assert!(w.calls_of("systemd-run").is_empty());
    // A new revision (the Lead made it ready again, approved it) is started again.
    w.set("a", json!({"ready_at": "2026-11-18T11:00:00Z"}));
    w.reset();
    assert_eq!(keys(&w.ok()["started"]), ["a"]);
    let started = std::fs::read_to_string(w.root().join("a").join("started")).unwrap();
    assert_eq!(started, w.revision("a"));
}

// spec: specs/ap/work.md#R10
#[test]
fn r10_a_repository_the_runs_own_checks_and_the_stores_validate() {
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 16}}));
    w.ready("no-repo", &["n/**"], 1);
    w.set("no-repo", json!({"repository": null}));
    w.ready("branch", &["b/**"], 2);
    w.set("branch", json!({"branch": "main"}));
    w.ready("tier", &["t/**"], 3);
    w.set(
        "tier",
        json!({"implementer": {"runtime": "codex", "model": "m", "effort": "e"}}),
    );
    w.ready("unmapped", &["u/**"], 4);
    w.set("unmapped", json!({"repository": "o/unmapped"}));
    w.ready("held", &["h/**"], 5);
    w.refuse.insert(
        "held".into(),
        vec!["held".into(), "intent_not_confirmed".into()],
    );
    w.ready("dep", &["d/**"], 6);
    w.set("dep", json!({"depends_on": [uid("gone"), uid("open")]}));
    w.put("gone", json!({"state": "dropped"}));
    w.put("open", json!({"state": "draft"}));
    w.ready("dep2", &["e/**"], 7);
    w.set("dep2", json!({"depends_on": [uid("open")]}));
    w.put(
        "m-bad",
        json!({"state": "accepted", "class": "plain", "repository": "o/r",
        "head": "c".repeat(40), "write_set": ["m/**"],
        "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
        "verifier": {"runtime": "claude", "model": "opus", "effort": "high"}}),
    );
    w.refuse.insert("m-bad".into(), vec!["held".into()]);
    let v = w.ok();
    assert_eq!(v["started"], json!([]));
    let expected = [
        ("m-bad", "held"),
        ("no-repo", "no repository"),
        ("branch", "run refused: invalid_branch"),
        ("tier", "run refused: tier_not_supported"),
        ("unmapped", "run refused: repository_not_mapped"),
        ("held", "held, intent_not_confirmed"),
        ("dep", "dependency dropped: gone"),
        ("dep2", "dependency_not_merged"),
    ];
    assert_eq!(waiting(&v), pairs(&expected));
    // The accepted unit gets no note; the dependencies are read only for the failed validate.
    assert_eq!(w.notes(), pairs(&expected[1..]));
    let gets = w.gets();
    assert_eq!(gets.iter().filter(|g| *g == "gone").count(), 1);
    assert_eq!(gets.iter().filter(|g| *g == "open").count(), 1);
}

// spec: specs/ap/work.md#R11
#[test]
fn r11_the_overlap_rule() {
    assert!(overlap("crates/ap/src/**", "crates/ap/src/run.rs"));
    assert!(!overlap("crates/ap/**", "crates/ont/**"));
    assert!(overlap("check", "check"));
    assert!(overlap("a/b*", "a/bc/d"));
    // Both ways round, whole components without `*`, and the matcher's `<dir>/**`.
    assert!(overlap("crates/ap/src/run.rs", "crates/ap/src/**"));
    assert!(!overlap("check", "checks"));
    assert!(!overlap("a/b", "a/bc"));
    assert!(overlap("a/b", "a/b/c"));
    assert!(!overlap("crates/ap/**", "crates/apx/y"));
    assert!(overlap("**", "anything"));
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 16}}));
    w.ready("first", &["crates/ap/src/**"], 1);
    w.ready("second", &["crates/ont/**"], 2);
    w.ready("third", &["crates/ap/src/run.rs"], 3);
    w.ready("fourth", &["docs/x"], 4);
    w.set("fourth", json!({"repository": "o/s"}));
    w.put("busy", json!({"state": "accepted", "class": "contract", "repository": "o/s", "write_set": ["docs/**"]}));
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["first", "second"]);
    assert_eq!(
        waiting(&v),
        pairs(&[
            ("third", "write set overlaps first"),
            ("fourth", "write set overlaps busy")
        ])
    );
    // The first merged: the third starts by itself in the next pass.
    w.set("first", json!({"state": "merged"}));
    w.put("busy", json!({"state": "merged"}));
    w.live(&["aptest-second.service loaded active running s"]);
    w.reset();
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["third", "fourth"]);
}

// spec: specs/ap/work.md#R12
#[test]
fn r12_the_slots() {
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 2}}));
    for (n, k) in ["a", "b", "c"].iter().enumerate() {
        w.ready(k, &[&format!("{k}/**")], n as u32);
    }
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["a", "b"]);
    assert_eq!(waiting(&v), pairs(&[("c", "slots 2/2")]));
    // A live run unit of a unit not read counts.
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 2}}));
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.live(&["aptest-dropped.service loaded active running d"]);
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["a"]);
    assert_eq!(waiting(&v), pairs(&[("b", "slots 2/2")]));
}

// spec: specs/ap/work.md#R13
#[test]
fn r13_the_cpu_limit_and_the_load_outside_the_slice() {
    let quota = json!({"slice": SLICE});
    // Two CPUs, 150 % of one CPU outside the slice and 10 % inside, Q1's defaults: nothing starts.
    let mut w = W::new();
    w.policy_with(json!({ "quota": quota }));
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.load(2, 80, 10);
    let v = w.ok();
    assert_eq!(v["started"], json!([]));
    assert_eq!(
        (v["busy"].clone(), v["outside"].clone()),
        (json!(80), json!(150))
    );
    assert_eq!(
        waiting(&v),
        pairs(&[("a", "cpu outside"), ("b", "cpu outside")])
    );
    // One reading per pass: two moments of /proc/stat.
    let stats = w
        .reads
        .borrow()
        .iter()
        .filter(|r| *r == "/proc/stat")
        .count();
    assert_eq!(stats, 2);
    // The next pass reads again and notes nothing new.
    w.load(2, 80, 10);
    w.reset();
    w.ok();
    assert!(w.notes().is_empty());
    // Over the busy limit.
    let mut w = W::new();
    w.policy_with(json!({"quota": {"slice": SLICE, "busy_limit": 50}}));
    w.ready("a", &["a/**"], 1);
    w.load(4, 60, 200);
    let v = w.ok();
    assert_eq!(waiting(&v), pairs(&[("a", "cpu over 50%")]));
    // Under both: the start, with the readings in the answer.
    let mut w = W::new();
    w.policy_with(json!({ "quota": quota }));
    w.ready("a", &["a/**"], 1);
    w.load(4, 30, 100);
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["a"]);
    assert_eq!(
        (v["busy"].clone(), v["outside"].clone()),
        (json!(30), json!(20))
    );
    // No candidate that passed every other constraint: no reading at all.
    let mut w = W::new();
    w.policy_with(json!({ "quota": quota }));
    w.ready("a", &["a/**"], 1);
    w.set("a", json!({"repository": null}));
    let v = w.ok();
    assert_eq!(
        (v["busy"].clone(), v["outside"].clone()),
        (Value::Null, Value::Null)
    );
    assert!(!w.reads.borrow().iter().any(|r| r == "/proc/stat"));
    // Without `quota`, nothing is read.
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    assert_eq!(w.ok()["busy"], Value::Null);
    assert!(!w.reads.borrow().iter().any(|r| r == "/proc/stat"));
}

// spec: specs/ap/work.md#R14
#[test]
fn r14_the_start_clears_waits_on_writes_started_and_runs_the_unit() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.set("a", json!({"waits_on": "slots 3/3"}));
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["a"]);
    assert_eq!(w.notes(), pairs(&[("a", "")]));
    // The revision after the clear is the one written.
    let started = std::fs::read_to_string(w.root().join("a").join("started")).unwrap();
    assert_eq!(started, w.revision("a"));
    let mode = std::fs::metadata(w.root().join("a"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    let run = &w.calls_of("systemd-run")[0];
    let head = [
        "--unit",
        "aptest-a",
        "--slice",
        SLICE,
        "--collect",
        "--quiet",
        "--expand-environment=no",
    ];
    assert_eq!(run[..7], head);
    let policy = w.p("policy.json");
    let env = [
        "--setenv",
        "PATH=/invented/bin:/usr/bin",
        "--setenv",
        "HOME=/invented/home",
        "--setenv",
        "LANG=C.UTF-8",
        "--setenv",
        &format!("AP_POLICY_FILE={policy}"),
        "--setenv",
        "ONT_ROLE=dispatch",
        "--",
    ];
    assert_eq!(run[7..18], env);
    assert!(Path::new(&run[18]).is_absolute() && !run[18].ends_with(" (deleted)"));
    assert_eq!(run[19..], ["unit", "run", "a", "--store", STORE]);
    assert!(
        !run.iter()
            .any(|a| a.contains("FOO") || a.contains("RuntimeMax"))
    );
}

// spec: specs/ap/work.md#R15
#[test]
fn r15_a_failed_systemd_run_is_no_start() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.run_fails.insert("a".into());
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["b"]);
    assert_eq!(waiting(&v), pairs(&[("a", "start failed")]));
    assert!(!w.root().join("a").join("started").exists());
    assert_eq!(w.f("a")["waits_on"], "start failed");
    // A start that keeps failing: each pass clears the note before it tries, and notes the failure
    // again after it, so the store reads `start failed` after every such pass.
    for _ in 0..3 {
        w.reset();
        let v = w.ok();
        assert_eq!(waiting(&v), pairs(&[("a", "start failed")]));
        assert_eq!(w.notes(), pairs(&[("a", ""), ("a", "start failed")]));
        assert_eq!(w.f("a")["waits_on"], "start failed");
        assert!(!w.root().join("a").join("started").exists());
    }
    // The next pass tries it again.
    w.run_fails.clear();
    w.reset();
    assert_eq!(keys(&w.ok()["started"]), ["a"]);
}

// spec: specs/ap/work.md#R16
#[test]
fn r16_notes_only_when_they_change_and_a_store_failure_ends_the_pass() {
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 1}}));
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.ok();
    assert_eq!(w.notes(), pairs(&[("b", "slots 1/1")]));
    w.live(&["aptest-a.service loaded active running a"]);
    w.reset();
    w.ok();
    assert!(w.notes().is_empty());
    // A unit that moved meanwhile: its note refused `unit_not_ready` is no failure.
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.set("a", json!({"repository": null}));
    w.moved.insert("a".into());
    assert_eq!(waiting(&w.ok()), pairs(&[("a", "no repository")]));
    // Nor is a clear refused so; that unit does not start.
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.set("a", json!({"waits_on": "slots 3/3"}));
    w.moved.insert("a".into());
    let v = w.ok();
    assert_eq!(v["started"], json!([]));
    assert!(w.calls_of("systemd-run").is_empty() && !w.root().join("a").exists());
    // A store that fails after a start ends the pass, exit 3, with that start.
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.set("b", json!({"repository": null}));
    w.store_dies_after = Some(1);
    w.set("a", json!({"waits_on": "x"}));
    let e = w.pass().unwrap_err();
    assert_eq!((e.failure.code, e.status), ("process_failed", 3));
    assert_eq!(e.started, Some(vec!["a".to_owned()]));
}

// spec: specs/ap/work.md#R17
#[test]
fn r17_the_actor_and_the_request_ids() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.set("a", json!({"waits_on": "old"}));
    w.ready("b", &["b/**"], 2);
    w.set("b", json!({"repository": null}));
    w.ok();
    let mut bodies = w.bodies("validate");
    bodies.extend(w.bodies("apply"));
    assert_eq!(bodies.len(), 3);
    let mut ids = BTreeSet::new();
    for b in &bodies {
        assert_eq!(
            (b["agent"].as_str(), b["session"].as_str()),
            (Some("ap-work"), Some("w-0123abcd456789ef"))
        );
        assert_eq!(b["role"], "dispatch", "{b}");
        ids.insert(b["request_id"].as_str().unwrap().to_owned());
    }
    let want: BTreeSet<String> = (1..=3)
        .map(|n| format!("wk-0123abcd456789ef-{n}"))
        .collect();
    assert_eq!(ids, want);
    let applies = w
        .calls
        .borrow()
        .iter()
        .filter(|c| c.1[..2] == ["action", "apply"])
        .count();
    let clients = w
        .calls
        .borrow()
        .iter()
        .filter(|c| c.0 == "ont" && c.1[0] == "action")
        .all(|c| c.1[2..4] == ["--client", "ap"]);
    assert!(applies == 2 && clients);
}

// spec: specs/ap/work.md#R18
#[test]
fn r18_the_answer() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    let v = w.ok();
    let mut members: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    members.sort();
    assert_eq!(
        members,
        [
            "busy",
            "cut",
            "in_flight",
            "not_restarted",
            "outside",
            "started",
            "waiting"
        ]
    );
    let out = ap_cli(&["work", "--help"], &[]);
    assert!(
        out.status.success()
            && String::from_utf8(out.stdout)
                .unwrap()
                .starts_with("Usage: ap work")
    );
    let help = String::from_utf8(ap_cli(&["--help"], &[]).stdout).unwrap();
    assert!(
        help.contains("  work: ap work [--store <path>] (README \"The dispatch pass\")\n"),
        "{help}"
    );
}

// spec: specs/ap/work.md#R19
#[test]
fn r19_the_only_effects_and_no_call_of_a_test_reaches_the_real_service_manager() {
    let mut w = W::new();
    w.ready("a", &["a/**"], 1);
    w.ready("b", &["b/**"], 2);
    w.ready("c", &["c/**"], 3);
    w.run_fails.insert("c".into());
    std::fs::create_dir_all(w.root().join("old")).unwrap();
    std::fs::write(w.root().join("old").join("started"), "r#9").unwrap();
    w.ok();
    let programs: BTreeSet<String> = w.calls.borrow().iter().map(|c| c.0.clone()).collect();
    assert_eq!(
        programs,
        ["git", "ont", "systemctl", "systemd-run"]
            .map(String::from)
            .into()
    );
    for c in w.calls_of("systemctl") {
        assert_eq!(c[0], "list-units");
    }
    let made = tree(&w.root());
    let want = [
        "a/",
        "a/started",
        "b/",
        "b/started",
        "c/",
        "old/",
        "old/started",
        "work.lock",
    ];
    assert_eq!(made, want.map(String::from).into());
    // The command itself, with stand-ins first on PATH: nothing reaches the host's service manager or
    // store. Each stand-in logs its call; the pass's own calls are answered by them.
    let t = W::new();
    let bin = t.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = t.dir.join("calls.log");
    let stand_in = |name: &str, answer: &str| {
        let path = bin.join(name);
        let script = format!(
            "#!/bin/sh\nwhile read -r _; do :; done\necho \"{name} $*\" >> '{}'\n{answer}\n",
            log.display()
        );
        // Written by a child: no thread of this process holds a descriptor open for writing on an
        // executable that another thread's child may run (`ETXTBSY`).
        let mut writer = Command::new("sh")
            .args(["-c", "cat > \"$1\" && chmod 0755 \"$1\"", "sh"])
            .arg(&path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(writer.stdin.as_mut().unwrap(), script.as_bytes()).unwrap();
        drop(writer.stdin.take());
        assert!(writer.wait().unwrap().success());
    };
    stand_in("systemctl", "exit 0");
    stand_in("systemd-run", "exit 1");
    stand_in("git", "exit 0");
    stand_in(
        "ont",
        "echo '{\"status\":\"ok\",\"result\":{\"basis\":{\"qualified\":true},\"objects\":[],\"continuation\":null}}'",
    );
    let path = bin.to_string_lossy().into_owned();
    let policy = t.p("policy.json");
    let store = t.p("store.sqlite");
    let out = ap_cli(
        &["work", "--store", &store],
        &[("PATH", &path), ("AP_POLICY_FILE", &policy)],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    // A runner without systemd as its init skips; the host and CI's runner have it.
    if Path::new("/run/systemd/system").is_dir() {
        assert_eq!(v["started"], json!([]));
        let calls = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = calls.lines().collect();
        assert_eq!(lines.len(), 2, "{calls}");
        assert!(lines[0].starts_with("ont search --store "));
        assert_eq!(
            lines[1],
            "systemctl list-units --all --plain --no-legend aptest-*"
        );
        // The refusals and skips of the command make no call at all.
        std::fs::remove_file(&log).unwrap();
    } else {
        assert_eq!(v, json!({"skipped": "no service manager"}));
    }
    for (args, env, code) in [
        (vec!["work", "now"], vec![("PATH", path.as_str())], 2),
        (
            vec!["work"],
            vec![("PATH", path.as_str()), ("AP_POLICY_FILE", policy.as_str())],
            2,
        ),
        (
            vec!["work", "--store", &store],
            vec![("PATH", path.as_str())],
            2,
        ),
        (
            vec!["work", "--store", &store],
            vec![
                ("PATH", path.as_str()),
                ("AP_POLICY_FILE", policy.as_str()),
                ("ONT_ROLE", "lead"),
            ],
            2,
        ),
    ] {
        let out = ap_cli(&args, &env);
        assert_eq!(out.status.code(), Some(code), "{args:?} {out:?}");
        let e: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert!(e["error"]["code"].is_string());
    }
    std::fs::write(&policy, "{}").unwrap();
    let out = ap_cli(
        &["work", "--store", &store],
        &[("PATH", &path), ("AP_POLICY_FILE", &policy)],
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "{\"skipped\":\"no work\"}\n"
    );
    assert!(!log.exists(), "a call was made");
}

// spec: specs/ap/work.md#R20
#[test]
fn r20_observe_reads_without_writing_or_taking_the_pass_lock() {
    let mut w = W::new();
    let root = w.root();
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::write(root.join("a").join("started"), "r#2").unwrap();
    std::fs::create_dir_all(root.join("c")).unwrap();
    std::fs::write(root.join("c").join("started"), "r#1").unwrap();
    let _pass = hold(&root.join("work.lock"));
    let _b = hold(&root.join("b").join("lock"));
    w.live(&[
        "aptest-c.service loaded active running c",
        "aptest-other.service loaded active running o",
    ]);
    let before = tree(&root);
    let units = [("a", "r#2"), ("b", "r#1"), ("c", "r#5"), ("Bad", "r#1")]
        .map(|(k, r)| (k.to_owned(), r.to_owned()));
    let seen = observe(&mut w, &root, PREFIX, &units).unwrap();
    assert_eq!(
        seen,
        Observed {
            in_flight: vec!["b".into(), "c".into()],
            started: vec!["a".into()],
            others: vec!["aptest-other.service".into()],
        }
    );
    assert_eq!(tree(&root), before);
    assert!(w.calls.borrow().iter().all(|c| c.0 == "systemctl"));
}

// spec: specs/ap/work.md#R21
#[test]
fn r21_the_timer_and_its_service() {
    let service = include_str!("../../../deploy/ap-work.service");
    let lines: Vec<&str> = service.lines().collect();
    for line in [
        "Type=oneshot",
        "ExecStart=/usr/local/bin/ap work",
        "Environment=HOME=/root",
        "Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin",
        "TimeoutStartSec=300",
    ] {
        assert!(lines.contains(&line), "{line}");
    }
    assert!(!lines.iter().any(|l| l.starts_with("Slice=")));
    let timer = include_str!("../../../deploy/ap-work.timer");
    let lines: Vec<&str> = timer.lines().collect();
    for line in [
        "OnBootSec=2min",
        "OnUnitInactiveSec=60s",
        "AccuracySec=1s",
        "WantedBy=timers.target",
    ] {
        assert!(lines.contains(&line), "{line}");
    }
}

/// An `accepted` unit of `o/r` at head `h`, of class `class`, approved at `approved` when given.
fn accepted(class: &str, head: &str, approved: Option<&str>) -> Value {
    let mut f = json!({"state": "accepted", "class": class, "lane": "small", "repository": "o/r",
        "head": head, "write_set": ["x/**"],
        "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
        "verifier": {"runtime": "claude", "model": "opus", "effort": "high"}});
    if let Some(a) = approved {
        f["approved_head"] = json!(a);
    }
    f
}

fn stat_reads(w: &W) -> usize {
    w.reads
        .borrow()
        .iter()
        .filter(|r| *r == "/proc/stat")
        .count()
}

// spec: specs/ap/work.md#R22
#[test]
fn r22_a_merge_run_starts_whatever_the_slots_and_the_cpu_reading_say() {
    let head = "a".repeat(40);
    // (a) Every slot taken: both merge candidates start, the `ready` unit counts them.
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 2}}));
    w.put("m-approved", accepted("contract", &head, Some(&head)));
    w.put("m-plain", accepted("plain", &head, None));
    w.ready("r", &["r/**"], 1);
    w.live(&[
        "aptest-d1.service loaded active running d",
        "aptest-d2.service loaded active running d",
    ]);
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["m-approved", "m-plain"]);
    assert_eq!(w.calls_of("systemd-run").len(), 2);
    assert_eq!(waiting(&v), pairs(&[("r", "slots 4/2")]));
    assert_eq!(w.notes(), pairs(&[("r", "slots 4/2")]));
    // (b) A load over the busy limit, and a load that leaves the slice less than its floor.
    for (quota, load, over) in [
        (
            json!({"slice": SLICE, "busy_limit": 50}),
            (4, 60, 200),
            "cpu over 50%",
        ),
        (json!({"slice": SLICE}), (2, 80, 10), "cpu outside"),
    ] {
        let mut w = W::new();
        w.policy_with(json!({ "quota": quota }));
        w.put("m", accepted("plain", &head, None));
        w.load(load.0, load.1, load.2);
        let v = w.ok();
        assert_eq!(keys(&v["started"]), ["m"]);
        assert_eq!(v["waiting"], json!([]));
        assert_eq!(
            (v["busy"].clone(), v["outside"].clone()),
            (Value::Null, Value::Null)
        );
        assert_eq!(stat_reads(&w), 0);
        let mut w = W::new();
        w.policy_with(json!({ "quota": quota }));
        w.put("m", accepted("plain", &head, None));
        w.ready("r", &["r/**"], 1);
        w.load(load.0, load.1, load.2);
        let v = w.ok();
        assert_eq!(keys(&v["started"]), ["m"]);
        assert_eq!(waiting(&v), pairs(&[("r", over)]));
        assert_eq!(stat_reads(&w), 2);
        assert_ne!(v["busy"], Value::Null);
    }
    // (c) What still holds, with every slot taken by live run units.
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 2}}));
    w.put("m-bad", accepted("plain", &head, None));
    w.refuse.insert("m-bad".into(), vec!["held".into()]);
    w.put("m-again", accepted("plain", &head, None));
    w.put("c-none", accepted("contract", &head, None));
    w.put("c-old", accepted("effect", &head, Some(&"b".repeat(40))));
    w.put("m-live", accepted("plain", &head, None));
    std::fs::create_dir_all(w.root().join("m-again")).unwrap();
    std::fs::write(
        w.root().join("m-again").join("started"),
        w.revision("m-again"),
    )
    .unwrap();
    w.live(&[
        "aptest-d1.service loaded active running d",
        "aptest-d2.service loaded active running d",
        "aptest-m-live.service loaded active running m",
    ]);
    let v = w.ok();
    assert_eq!(v["started"], json!([]));
    assert!(w.calls_of("systemd-run").is_empty());
    assert_eq!(waiting(&v), pairs(&[("m-bad", "held")]));
    assert!(w.notes().is_empty());
    assert_eq!(keys(&v["not_restarted"]), ["m-again"]);
    assert!(keys(&v["in_flight"]).contains(&"m-live".to_owned()));
    let gets = w.gets();
    assert!(!gets.contains(&"c-none".to_owned()) && !gets.contains(&"c-old".to_owned()));
    assert!(!gets.contains(&"m-live".to_owned()));
    // (d) The one slot free: the merge candidate takes it, the `ready` unit waits.
    let mut w = W::new();
    w.policy_with(json!({"work": {"prefix": PREFIX, "slice": SLICE, "slots": 1}}));
    w.put("m", accepted("plain", &head, None));
    w.ready("r", &["r/**"], 1);
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["m"]);
    assert_eq!(waiting(&v), pairs(&[("r", "slots 1/1")]));
}

/// The id of an intent.
fn iid(key: &str) -> Value {
    json!({"object_type": "intent", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": key})
}
impl W {
    /// A draft unit of the lane `objective` of the intent `intent`, added at second `at`, with
    /// dependencies `deps` and the write set `ws`.
    fn draft(&mut self, key: &str, intent: &str, at: u32, deps: &[&str], ws: &[&str]) {
        let deps: Vec<Value> = deps.iter().map(|d| uid(d)).collect();
        self.put(
            key,
            json!({"lane": "objective", "class": "contract", "intent": iid(intent),
            "objective": format!("invented {key}"), "repository": "o/r", "write_set": ws,
            "state": "draft", "added_at": format!("2026-11-18T10:00:{at:02}Z"), "depends_on": deps}),
        );
    }
    /// A review of kind `page` of the unit `unit`, closed with `verdict` at the head `head`.
    fn review(&mut self, key: &str, unit: &str, verdict: &str, head: &str) {
        self.others.insert(
            ("review".into(), key.into()),
            json!({"kind": "page", "status": "closed", "verdict": verdict, "head": head,
            "unit": uid(unit)}),
        );
    }
    /// An accepted page of the unit `key` as it is now.
    fn accept_page(&mut self, review: &str, key: &str) {
        let digest = ap::unit::render::page_digest(key, self.f(key));
        self.review(review, key, "accept", &digest);
    }
    fn intent(&mut self, key: &str) {
        self.others.insert(
            ("intent".into(), key.into()),
            json!({"status": "confirmed"}),
        );
    }
    fn horizon(&mut self, p: Value) -> Result<Value, (String, u8)> {
        ap::work::horizon(self, STORE, &p)
            .map_err(|(f, status)| (format!("{}@{}", f.code, f.field), status))
    }
    /// The policy with a writer and `pages`.
    fn writers(&mut self, pages: u64) {
        let dir = self.p("pages");
        self.policy_with(json!({
            "writer": {"model": "opus", "effort": "high", "permission_mode": "auto", "directory": dir},
            "work": {"prefix": PREFIX, "slice": SLICE, "page_prefix": PAGES, "pages": pages}}));
    }
}
fn horizon_units(v: &Value) -> Vec<(String, String)> {
    let all = v["units"].as_array().unwrap().iter();
    all.map(|u| {
        (
            u["unit"].as_str().unwrap().to_owned(),
            u["page"].as_str().unwrap().to_owned(),
        )
    })
    .collect()
}

// spec: specs/ap/work.md#R23
#[test]
fn r23_the_horizon_of_an_intent() {
    let mut w = W::new();
    w.intent("i1");
    w.intent("i2");
    // The dependencies of every kind.
    for (key, state) in [
        ("d-merged", "merged"),
        ("d-done", "done"),
        ("d-ready", "ready"),
        ("d-running", "running"),
        ("d-accepted", "accepted"),
        ("d-stopped", "stopped"),
        ("d-dropped", "dropped"),
    ] {
        w.draft(key, "i9", 1, &[], &["x/**"]);
        w.set(key, json!({ "state": state }));
    }
    w.draft("d-page", "i9", 1, &[], &["x/**"]);
    w.set("d-page", json!({"state": "draft", "intent": iid("i9")}));
    w.accept_page("rv-page", "d-page");
    // An accept of an older digest does not count.
    w.draft("d-old", "i9", 1, &[], &["x/**"]);
    w.accept_page("rv-old", "d-old");
    w.set("d-old", json!({"objective": "amended since"}));
    let every = [
        "d-merged",
        "d-done",
        "d-ready",
        "d-running",
        "d-accepted",
        "d-stopped",
        "d-page",
    ];
    w.draft("x-all", "i1", 3, &every, &["a/**"]);
    w.draft("x-old", "i1", 1, &["d-old"], &["a/**"]);
    w.draft("x-dropped", "i1", 1, &["d-dropped"], &["a/**"]);
    w.draft("x-ghost", "i1", 1, &["ghost"], &["a/**"]);
    w.draft("x-none", "i1", 2, &[], &["a/**"]);
    w.draft("x-a-written", "i1", 2, &[], &["a/**"]);
    w.set(
        "x-a-written",
        json!({"acceptance": [{"check": "true", "kind": "invariant"}]}),
    );
    w.draft("x-accepted", "i1", 4, &[], &["a/**"]);
    w.set(
        "x-accepted",
        json!({"acceptance": [{"check": "true", "kind": "invariant"}]}),
    );
    w.accept_page("rv-x", "x-accepted");
    w.draft("y-other", "i2", 1, &[], &["a/**"]);
    let v = w.horizon(json!({"intent": "i1"})).unwrap();
    assert_eq!(v["intent"], "i1");
    assert_eq!(
        horizon_units(&v),
        pairs(&[
            ("x-a-written", "written"),
            ("x-none", "none"),
            ("x-all", "none"),
            ("x-accepted", "accepted"),
        ])
    );
    assert_eq!(v["units"][0]["class"], "contract");
    // Only reads: no apply and no validate.
    assert!(w.bodies("apply").is_empty() && w.bodies("validate").is_empty());
    // The refusals: an intent the store does not have, input of another form, a read that fails.
    assert_eq!(
        w.horizon(json!({"intent": "nope"})).unwrap_err(),
        ("unknown_intent@intent".into(), 2)
    );
    for bad in [
        json!({"intent": 1}),
        json!({"intent": "i1", "x": 1}),
        json!([]),
    ] {
        assert_eq!(w.horizon(bad).unwrap_err().1, 2);
    }
    w.broken = true;
    assert_eq!(w.horizon(json!({"intent": "i1"})).unwrap_err().1, 3);
}

// spec: specs/ap/work.md#R24
#[test]
fn r24_the_writers_of_the_pages() {
    let pass_of = |w: &mut W| {
        let v = w.ok();
        v["pages"].clone()
    };
    // `pages` 2 and four candidates: two start, two wait.
    let mut w = W::new();
    w.writers(2);
    for (i, key) in ["c1", "c2", "c3", "c4"].iter().enumerate() {
        w.draft(key, "i1", i as u32, &[], &["a/**"]);
    }
    let p = pass_of(&mut w);
    assert_eq!(keys(&p["started"]), ["c1", "c2"]);
    assert_eq!(
        waiting(&p),
        pairs(&[("c3", "pages 2/2"), ("c4", "pages 2/2")])
    );
    let runs = w.calls_of("systemd-run");
    assert_eq!(runs.len(), 2);
    let first = &runs[0];
    assert_eq!(first[..2], ["--unit".to_owned(), format!("{PAGES}-c1")]);
    let at = first.iter().position(|a| a == "--").unwrap();
    assert_eq!(
        first[at + 2..],
        ["unit", "write", "c1", "--store", STORE].map(str::to_owned)
    );
    for key in ["c1", "c2"] {
        let started = w.dir.join("pages").join(key).join("started");
        let mode = std::fs::metadata(&started).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(!w.dir.join("pages/c3").exists());
    // The next pass starts none again: `started` holds them.
    w.reset();
    let p = pass_of(&mut w);
    assert_eq!(keys(&p["not_restarted"]), ["c1", "c2"]);
    assert_eq!(keys(&p["started"]), ["c3", "c4"]);
    // A live writer's unit, a held lock, a file `started` and an empty write set each keep a unit from a start.
    let mut w = W::new();
    w.writers(8);
    for (i, key) in ["e1", "e2", "e3", "e4"].iter().enumerate() {
        w.draft(key, "i1", i as u32, &[], &["a/**"]);
    }
    w.set("e4", json!({"write_set": []}));
    w.listed_pages = format!("{PAGES}-e1.service loaded active running e1\n");
    let _lock = hold(&w.dir.join("pages/e2/lock"));
    std::fs::create_dir_all(w.dir.join("pages/e3")).unwrap();
    std::fs::write(w.dir.join("pages/e3/started"), "").unwrap();
    let p = pass_of(&mut w);
    assert_eq!(p["started"], json!([]));
    assert_eq!(keys(&p["in_flight"]), ["e1", "e2"]);
    assert_eq!(keys(&p["not_restarted"]), ["e3"]);
    assert_eq!(p["waiting"], json!([]));
    assert!(w.calls_of("systemd-run").is_empty());
    // A failed `systemd-run` leaves no file `started`.
    let mut w = W::new();
    w.writers(2);
    w.draft("f1", "i1", 1, &[], &["a/**"]);
    w.run_fails.insert("f1".into());
    let p = pass_of(&mut w);
    assert_eq!(waiting(&p), pairs(&[("f1", "start failed")]));
    assert!(!w.dir.join("pages/f1/started").exists());
    // A draft with a page, or whose dependency is a draft without one, is no candidate.
    let mut w = W::new();
    w.writers(2);
    w.draft("g0", "i1", 1, &[], &["a/**"]);
    w.set(
        "g0",
        json!({"acceptance": [{"check": "true", "kind": "invariant"}]}),
    );
    w.draft("g1", "i1", 2, &["g0"], &["a/**"]);
    let p = pass_of(&mut w);
    assert_eq!(p["started"], json!([]));
    // A writer starts while every slot is taken, and while the load is over the limit.
    let mut w = W::new();
    w.writers(2);
    w.ready("r", &["r/**"], 1);
    w.draft("s1", "i1", 1, &[], &["a/**"]);
    w.live(&[
        "aptest-d1.service loaded active running d",
        "aptest-d2.service loaded active running d",
        "aptest-d3.service loaded active running d",
    ]);
    let v = w.ok();
    assert_eq!(waiting(&v), pairs(&[("r", "slots 3/3")]));
    assert_eq!(keys(&v["pages"]["started"]), ["s1"]);
    assert_eq!(v["started"], json!([]));
    let mut w = W::new();
    w.writers(2);
    let mut policy: Value =
        serde_json::from_slice(&std::fs::read(w.dir.join("policy.json")).unwrap()).unwrap();
    policy["quota"] = json!({"slice": SLICE, "busy_limit": 50});
    std::fs::write(w.dir.join("policy.json"), policy.to_string()).unwrap();
    w.ready("r", &["r/**"], 1);
    w.draft("s1", "i1", 1, &[], &["a/**"]);
    w.load(4, 60, 200);
    let v = w.ok();
    assert_eq!(waiting(&v), pairs(&[("r", "cpu over 50%")]));
    assert_eq!(keys(&v["pages"]["started"]), ["s1"]);
    // A writer of a wrong form refuses the pass before the lock, whatever `pages`.
    let mut w = W::new();
    w.policy_with(json!({"writer": {"model": "opus"},
        "work": {"prefix": PREFIX, "slice": SLICE, "page_prefix": PAGES, "pages": 0}}));
    assert_eq!(w.refused(), ("policy_invalid@policy".into(), 2));
    assert!(!w.root().exists());
    // `page_prefix` equal to `prefix`, or one beginning with the other and `-`: refused only with a writer.
    for page_prefix in [PREFIX, "aptest-p"] {
        let mut w = W::new();
        w.policy_with(
            json!({"work": {"prefix": PREFIX, "slice": SLICE, "page_prefix": page_prefix}}),
        );
        w.ok();
        w.writers(2);
        let mut policy: Value =
            serde_json::from_slice(&std::fs::read(w.dir.join("policy.json")).unwrap()).unwrap();
        policy["work"]["page_prefix"] = json!(page_prefix);
        std::fs::write(w.dir.join("policy.json"), policy.to_string()).unwrap();
        let e = w.pass().unwrap_err();
        assert_eq!((e.failure.code, e.status), ("policy_invalid", 2));
        assert!(
            e.failure
                .detail
                .as_str()
                .unwrap()
                .starts_with("work.page_prefix")
        );
    }
    // With `--store`, a writer needs `work.page_prefix` and `writer.directory`.
    let mut w = W::new();
    w.policy_with(
        json!({"writer": {"model": "opus", "effort": "high", "permission_mode": "auto"}}),
    );
    assert_eq!(w.refused(), ("invalid_input@policy".into(), 2));
}

// spec: specs/ap/work.md#R25
#[test]
fn r25_the_member_pages_only_in_a_pass_of_the_writers() {
    // Without a writer, and with a writer and `pages` 0, the same calls and the same answer.
    let run = |writer: bool| {
        let mut w = W::new();
        match writer {
            true => w.writers(0),
            false => w.policy_with(json!({})),
        }
        w.ready("r", &["r/**"], 1);
        w.draft("c1", "i1", 1, &[], &["a/**"]);
        let v = w.ok();
        let calls: Vec<(String, Vec<String>)> = w
            .calls
            .borrow()
            .iter()
            .map(|c| (c.0.clone(), c.1.clone()))
            .collect();
        (v, calls)
    };
    let (without, calls) = run(false);
    let (with, calls_with) = run(true);
    assert_eq!(without, with);
    assert!(without.get("pages").is_none());
    assert_eq!(calls.len(), calls_with.len());
    // A pass of R24: the member `pages` with its four lists; its files are the directories and `started`.
    let mut w = W::new();
    w.writers(1);
    w.draft("c1", "i1", 1, &[], &["a/**"]);
    w.draft("c2", "i1", 2, &[], &["a/**"]);
    w.listed_pages = format!("{PAGES}-gone.service loaded active running x\n");
    let v = w.ok();
    let p = &v["pages"];
    let members: BTreeSet<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        members,
        BTreeSet::from(["started", "not_restarted", "waiting", "in_flight"])
    );
    assert_eq!(keys(&p["in_flight"]), [format!("{PAGES}-gone.service")]);
    assert_eq!(
        waiting(p),
        pairs(&[("c1", "pages 1/1"), ("c2", "pages 1/1")])
    );
    assert!(w.bodies("apply").is_empty(), "no action for a writer");
    assert_eq!(tree(&w.dir.join("pages")), BTreeSet::new());
    let mut w = W::new();
    w.writers(1);
    w.draft("c1", "i1", 1, &[], &["a/**"]);
    w.ok();
    assert_eq!(
        tree(&w.dir.join("pages")),
        BTreeSet::from(["c1/".to_owned(), "c1/started".to_owned()])
    );
}

/// The commits of the delivery trials: a dependency's merge, and the tips of the record.
const MERGED: &str = "1111111111111111111111111111111111111111";
const TIP: &str = "2222222222222222222222222222222222222222";
const ETC_TIP: &str = "3333333333333333333333333333333333333333";
impl W {
    /// The request of delivery over `o/r` with the configuration repository `o/etc`, its state
    /// directory under the trial's, and the record `record` there (None: no record).
    fn delivery(&mut self, record: Option<Value>) {
        let state = self.p("deliver");
        let request = json!({"repository": "o/r", "url": "https://invented.example/o/r.git",
            "state": state, "etc": {"repository": "o/etc", "url": "https://invented.example/o/etc.git"}});
        self.deliver = Some(request.to_string());
        std::fs::create_dir_all(&state).unwrap();
        let last = self.dir.join("deliver/last.json");
        let _ = std::fs::remove_file(&last);
        if let Some(r) = record {
            std::fs::write(last, r.to_string()).unwrap();
        }
    }
    /// A record of delivery that ended `end` with the target `target`, `etc` ended `etc`.
    fn record(end: &str, target: &str, etc: &str) -> Value {
        json!({"at": "2026-11-24T10:00:00Z", "end": end, "target": target, "repository": "o/r",
            "tip": TIP, "etc": {"repository": "o/etc", "tip": ETC_TIP, "result": etc}})
    }
    /// `MERGED` is in the tip of `repo`'s clone of delivery.
    fn ancestor(&mut self, name: &str, tip: &str) {
        let clone = self.p(&format!("deliver/{name}"));
        self.ancestors
            .insert((clone, MERGED.to_owned(), tip.to_owned()));
    }
    /// A dependency of `repository` (None: without one) in `state`, with `MERGED` as its commit.
    fn dependency(&mut self, key: &str, repository: Option<&str>, state: &str) {
        let mut f = json!({"lane": "small", "class": "plain", "objective": "invented",
            "write_set": [format!("{key}/**")], "state": state, "merged_commit": MERGED});
        if let Some(r) = repository {
            f["repository"] = json!(r);
        }
        self.put(key, f);
    }
    /// The ready unit `b1` of `o/r` that waits for the delivery of `deps`.
    fn dependent(&mut self, deps: &[&str]) {
        self.ready("b1", &["b/**"], 1);
        let deps: Vec<Value> = deps.iter().map(|d| uid(d)).collect();
        self.set("b1", json!({ "depends_delivered": deps }));
    }
    /// The pass's end for `b1`: started, or what it waits on.
    fn end_of_b1(&mut self) -> String {
        self.reset();
        let v = self.ok();
        if keys(&v["started"]).contains(&"b1".to_owned()) {
            return "started".into();
        }
        let all = waiting(&v);
        let found = all.iter().find(|(k, _)| k == "b1");
        found.map(|(_, w)| w.clone()).unwrap_or_default()
    }
    /// A fresh trial: the request and record of delivery, the dependency `a1` and `b1`.
    fn delivery_trial(record: Option<Value>, repository: Option<&str>, state: &str) -> Self {
        let mut w = W::new();
        w.delivery(record);
        w.dependency("a1", repository, state);
        w.dependent(&["a1"]);
        w
    }
}

// spec: specs/ap/work.md#R26
#[test]
fn r26_not_delivered_until_the_newest_record_holds_the_merge_in_its_tip() {
    let ap = "/usr/local/bin/ap";
    // Delivered: merged, the record current or installed for `ap`, the merge in its tip.
    for end in ["current", "installed"] {
        let mut w = W::delivery_trial(Some(W::record(end, ap, "same")), Some("o/r"), "merged");
        w.ancestor("r", TIP);
        assert_eq!(w.end_of_b1(), "started");
        let git = w.calls_of("git");
        assert!(
            git.iter().any(|g| g.contains(&"--is-ancestor".to_owned())),
            "{git:?}"
        );
    }
    // `done` counts as merged.
    let mut w = W::delivery_trial(Some(W::record("current", ap, "same")), Some("o/r"), "done");
    w.ancestor("r", TIP);
    assert_eq!(w.end_of_b1(), "started");
    // The merge not in the tip, or an ancestry that cannot be read: not delivered.
    let mut w = W::delivery_trial(
        Some(W::record("current", ap, "same")),
        Some("o/r"),
        "merged",
    );
    assert_eq!(w.end_of_b1(), "not delivered: a1");
    assert_eq!(w.f("b1")["waits_on"], "not delivered: a1");
    // A record of another end, or of the engine's install.
    for (end, target) in [
        ("refused", ap),
        ("failed", ap),
        ("not_started", ap),
        ("current", "/usr/local/bin/ont"),
        ("installed", "/usr/local/bin/ont"),
    ] {
        let mut w = W::delivery_trial(Some(W::record(end, target, "same")), Some("o/r"), "merged");
        w.ancestor("r", TIP);
        assert_eq!(w.end_of_b1(), "not delivered: a1", "{end} {target}");
    }
    // A dependency of a covered repository that has not merged: no Git is run for it.
    let mut w = W::delivery_trial(
        Some(W::record("current", ap, "same")),
        Some("o/r"),
        "accepted",
    );
    w.ancestor("r", TIP);
    assert_eq!(w.end_of_b1(), "not delivered: a1");
    assert!(
        w.calls_of("git")
            .iter()
            .all(|g| !g.contains(&"--is-ancestor".to_owned()))
    );
}

// spec: specs/ap/work.md#R26
#[test]
fn r26_not_delivered_without_a_record_or_a_readable_request() {
    let ap = "/usr/local/bin/ap";
    // No record.
    let mut w = W::delivery_trial(None, Some("o/r"), "merged");
    w.ancestor("r", TIP);
    assert_eq!(w.end_of_b1(), "not delivered: a1");
    // A record that is no JSON object.
    let mut w = W::delivery_trial(None, Some("o/r"), "merged");
    w.ancestor("r", TIP);
    std::fs::write(w.dir.join("deliver/last.json"), "[1]").unwrap();
    assert_eq!(w.end_of_b1(), "not delivered: a1");
    // A request that is absent or cannot be read: not delivered, whatever the repository.
    for request in [None, Some("{not json".to_owned()), Some("[]".to_owned())] {
        for repository in ["o/r", "o/s"] {
            let mut w = W::delivery_trial(
                Some(W::record("current", ap, "same")),
                Some(repository),
                "merged",
            );
            w.ancestor("r", TIP);
            w.deliver = request.clone();
            assert_eq!(
                w.end_of_b1(),
                "not delivered: a1",
                "{request:?} {repository}"
            );
        }
    }
}

// spec: specs/ap/work.md#R26
#[test]
fn r26_not_delivered_after_a_failed_configuration_step() {
    let ap = "/usr/local/bin/ap";
    // The configuration repository is delivered by the record's member `etc`, its tip in its own clone.
    for result in ["same", "installed"] {
        let mut w = W::delivery_trial(
            Some(W::record("current", ap, result)),
            Some("o/etc"),
            "merged",
        );
        w.ancestor("etc", ETC_TIP);
        assert_eq!(w.end_of_b1(), "started", "{result}");
    }
    // Its step failed, or never ran: not delivered.
    for result in ["fetch_failed", "install_failed", "check_failed"] {
        let mut w = W::delivery_trial(
            Some(W::record("current", ap, result)),
            Some("o/etc"),
            "merged",
        );
        w.ancestor("etc", ETC_TIP);
        assert_eq!(w.end_of_b1(), "not delivered: a1", "{result}");
    }
    let mut record = W::record("current", ap, "same");
    record["etc"] = Value::Null;
    let mut w = W::delivery_trial(Some(record), Some("o/etc"), "merged");
    w.ancestor("etc", ETC_TIP);
    assert_eq!(w.end_of_b1(), "not delivered: a1");
    // The tip of the main repository does not deliver the configuration repository.
    let mut w = W::delivery_trial(
        Some(W::record("current", ap, "same")),
        Some("o/etc"),
        "merged",
    );
    w.ancestor("etc", TIP);
    w.ancestor("r", TIP);
    assert_eq!(w.end_of_b1(), "not delivered: a1");
}

// spec: specs/ap/work.md#R26
#[test]
fn r26_not_delivered_a_dropped_dependency_and_repositories_delivery_does_not_cover() {
    let ap = "/usr/local/bin/ap";
    // A dropped dependency, and a dependency the store does not hold.
    let mut w = W::delivery_trial(
        Some(W::record("current", ap, "same")),
        Some("o/r"),
        "dropped",
    );
    assert_eq!(w.end_of_b1(), "dependency dropped: a1");
    assert_eq!(w.f("b1")["waits_on"], "dependency dropped: a1");
    let mut w = W::new();
    w.delivery(Some(W::record("current", ap, "same")));
    w.dependent(&["gone"]);
    assert_eq!(w.end_of_b1(), "not delivered: gone");
    // A repository the request does not cover: released by the merge, with no Git.
    for (state, end) in [
        ("merged", "started"),
        ("done", "started"),
        ("accepted", "not delivered: a1"),
    ] {
        let mut w = W::delivery_trial(None, Some("o/s"), state);
        assert_eq!(w.end_of_b1(), end, "{state}");
        assert!(
            w.calls_of("git")
                .iter()
                .all(|g| !g.contains(&"--is-ancestor".to_owned()))
        );
    }
    // No repository: released by `done` alone.
    for (state, end) in [("done", "started"), ("merged", "not delivered: a1")] {
        let mut w = W::delivery_trial(None, None, state);
        assert_eq!(w.end_of_b1(), end, "{state}");
    }
    // The first dependency that is not delivered names the wait.
    let mut w = W::new();
    w.delivery(Some(W::record("current", ap, "same")));
    w.dependency("a1", Some("o/s"), "merged");
    w.dependency("a2", Some("o/r"), "merged");
    w.dependency("a3", Some("o/r"), "dropped");
    w.dependent(&["a1", "a2", "a3"]);
    assert_eq!(w.end_of_b1(), "not delivered: a2");
    w.ancestor("r", TIP);
    assert_eq!(w.end_of_b1(), "dependency dropped: a3");
}

// spec: specs/ap/work.md#R26
// spec: specs/ap/work.md#R10
#[test]
fn r26_not_delivered_comes_after_the_validate_and_is_read_only_for_such_a_candidate() {
    let ap = "/usr/local/bin/ap";
    // A failed validate of `start_unit` names the wait first.
    let mut w = W::delivery_trial(None, Some("o/r"), "merged");
    w.refuse.insert("b1".into(), vec!["held".into()]);
    assert_eq!(w.end_of_b1(), "held");
    // A unit without `depends_delivered` reads neither file of delivery and runs no Git for it.
    let mut w = W::new();
    w.delivery(Some(W::record("current", ap, "same")));
    w.ready("c1", &["c/**"], 1);
    w.reset();
    let v = w.ok();
    assert_eq!(keys(&v["started"]), ["c1"]);
    let reads = w.reads.borrow().clone();
    assert!(
        !reads
            .iter()
            .any(|r| r == DELIVER || r.ends_with("last.json")),
        "{reads:?}"
    );
    assert!(
        w.calls_of("git")
            .iter()
            .all(|g| !g.contains(&"--is-ancestor".to_owned()))
    );
    // Two candidates with `depends_delivered` read the files once.
    let mut w = W::delivery_trial(
        Some(W::record("current", ap, "same")),
        Some("o/r"),
        "merged",
    );
    w.ready("b2", &["b2/**"], 2);
    w.set("b2", json!({"depends_delivered": [uid("a1")]}));
    w.reset();
    let v = w.ok();
    assert_eq!(waiting(&v).len(), 2, "{v}");
    let reads = w.reads.borrow().clone();
    assert_eq!(
        reads.iter().filter(|r| *r == DELIVER).count(),
        1,
        "{reads:?}"
    );
}
