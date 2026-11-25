//! `ap unit write` (`specs/ap/unit-write.md`) and the role `writer` of `ap run` (`specs/ap/run.md` R53)
//! against fakes, in the manner of tests/unit_review.rs: a fake of `ont` that answers the reads and the
//! actions of this verb, of `ap unit amend`, of `ap unit review` and of `ap run` as the engine does; the
//! fake `claude` of tests/fixtures/fake-claude.sh, in mode `typed` for the writer (its answer, one object
//! per launch, written by the fake host before the launch) and in mode `script` for the reviewer of
//! tests/fixtures/unit_review/script.sh; a scratch repository and its clone; `systemctl` and the unit
//! launcher replaced. The policy file, the pages, units and reviews directories are under a temporary
//! directory: no test starts a unit of the service manager or reaches the host's store, policy file or
//! any file under /var/lib/agent-platform.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Host, Reply, System};
use ap::unit::page_write::{Refusal, arguments, execute, inside};
use ap::unit::render::page_digest;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsString,
    fs::File,
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const FAKE: &str = include_str!("fixtures/fake-claude.sh");
const SCRIPT: &str = include_str!("fixtures/unit_review/script.sh");
const FRAME: &str = include_str!("../src/unit/frames/writer.md");
const STORE: &str = "/invented/scratch/store";
const BWRAP: &str = "/usr/bin/bwrap";
/// The epoch second of `2026-11-18T00:00:00Z`, the fake store's clock at 0.
const EPOCH: u64 = 1_794_960_000;
const SCOPE: &str = "ap-run-0123abcd-4567-89ef-0123-456789abcdef.scope";

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn reply(success: bool, v: &Value) -> Answer {
    let bytes = serde_json::to_vec(v).unwrap();
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
fn refused(code: &str, extra: Value) -> Value {
    let mut v = json!({"status": "refused", "code": code});
    for (k, x) in extra.as_object().into_iter().flatten() {
        v[k] = x.clone();
    }
    v
}
/// The key the engine gives the object a start creates: from the client, the agent and the request id.
fn made_key(agent: &str, rid: &str) -> String {
    format!(
        "act-rq-{}",
        &sha(format!("ap/{agent}/{rid}").as_bytes())[..32]
    )
}
fn stamp(clock: u64) -> String {
    format!(
        "2026-11-18T{:02}:{:02}:{:02}Z",
        clock / 3600,
        clock / 60 % 60,
        clock % 60
    )
}
/// Git with fixed names and times, so a commit is the same on every run.
fn git(dir: &Path, words: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(words)
        .env("GIT_AUTHOR_NAME", "Invented")
        .env("GIT_AUTHOR_EMAIL", "invented@example.invalid")
        .env("GIT_COMMITTER_NAME", "Invented")
        .env("GIT_COMMITTER_EMAIL", "invented@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {words:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(std::process::Stdio::null())
        .status();
}
type Key = (String, String);
fn k(kind: &str, key: &str) -> Key {
    (kind.to_owned(), key.to_owned())
}
/// An exclusive lock on `path`, held by the test until the value is dropped.
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
fn hold(path: &Path) -> Held {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let f = File::create(path).unwrap();
    rustix::fs::flock(&f, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    Held(f)
}

/// The fake store and host.
struct F {
    t: PathBuf,
    objects: BTreeMap<Key, Value>,
    revisions: BTreeMap<Key, u64>,
    /// The request ids applied (`agent/request_id`): the action with its parameters, and its edits.
    applied: BTreeMap<String, (Value, Value)>,
    clock: u64,
    /// Every call of `ont` and `systemctl`: the program, its arguments and its input.
    calls: Vec<(String, Vec<String>, Value)>,
    pauses: Vec<Duration>,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
    env: Vec<(String, String)>,
    /// The writer's answers, one per launch of the writer: the object it gives (None: no launch is
    /// expected beyond them).
    answers: VecDeque<Value>,
    /// The reviewer's verdicts, one per look (default accept).
    verdicts: VecDeque<&'static str>,
    /// The answers of the next calls of `git … fetch`: a failure with this text, or None for the real
    /// fetch.
    fetches: VecDeque<Option<&'static str>>,
    /// When the writer launches, this unit is amended by another session.
    amend_at_launch: Option<&'static str>,
    /// When the writer launches, the state of `u1` becomes this one, its revision kept.
    state_at_launch: Option<&'static str>,
    /// The roles of the stages launched, in order (`writer` or `reviewer`), and the words of each launch
    /// between `systemd-run … --` and the CLI.
    roles: Vec<String>,
    wraps: Vec<Vec<String>>,
}
impl F {
    fn new() -> Self {
        let t = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "ap-unit-write-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "w1", "origin", "home/.ssh"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let fake = t.join("bin/claude");
        std::fs::write(&fake, FAKE).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        for account in ["a1", "a2"] {
            std::fs::write(t.join(format!("bin/mode-{account}")), "script").unwrap();
            std::fs::write(t.join(format!("bin/script-{account}.sh")), SCRIPT).unwrap();
        }
        std::fs::write(t.join("bin/mode-w1"), "typed").unwrap();
        let origin = t.join("origin");
        git(&origin, &["init", "--quiet", "-b", "main"]);
        std::fs::create_dir_all(origin.join("src")).unwrap();
        std::fs::write(
            origin.join("AGENTS.md"),
            "# Invented widgets\n\nRead src/.\n",
        )
        .unwrap();
        std::fs::write(origin.join("src/widgets.rs"), "// invented\n").unwrap();
        git(&origin, &["add", "-A"]);
        git(&origin, &["commit", "--quiet", "-m", "invented"]);
        git(&t, &["clone", "--quiet", origin.to_str().unwrap(), "clone"]);
        let mut f = Self {
            t,
            objects: BTreeMap::new(),
            revisions: BTreeMap::new(),
            applied: BTreeMap::new(),
            clock: 10,
            calls: vec![],
            pauses: vec![],
            groups: BTreeMap::new(),
            alive: BTreeSet::new(),
            env: vec![
                ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
                ("HOME".into(), "/root".into()),
                ("LANG".into(), "C.UTF-8".into()),
            ],
            answers: VecDeque::new(),
            verdicts: VecDeque::new(),
            fetches: VecDeque::new(),
            amend_at_launch: None,
            state_at_launch: None,
            roles: vec![],
            wraps: vec![],
        };
        f.write_policy(json!({}));
        f.put("task", "invented-task", json!({"status": "open"}));
        f.put(
            "intent",
            "widgets",
            json!({"status": "confirmed", "outcome": "Every invented widget is listed once.",
                "first_user": "An invented reader of the widget list."}),
        );
        f.unit("u1", "contract", true);
        f
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_string_lossy().into_owned()
    }
    fn home(&self) -> PathBuf {
        self.t.join("pages/u1")
    }
    /// The policy file, with `extra`'s members over the defaults (a null member left out).
    fn write_policy(&self, extra: Value) {
        let mut policy = json!({"models": {"allowed": ["sonnet", "opus", "haiku"], "never": ["haiku"]},
            "accounts": {"claude": [self.p("a1"), self.p("a2")], "claude/writer": [self.p("w1")]},
            "repositories": {"invented/widgets": {"clone": self.p("clone")}},
            "reviews": {"model": "sonnet", "effort": "high", "account": self.p("a1"),
                "permission_mode": "auto", "directory": self.p("reviews"), "task": "invented-task"},
            "units": {"directory": self.p("units")},
            "writer": {"model": "opus", "effort": "high", "permission_mode": "auto",
                "directory": self.p("pages")}});
        for (key, v) in extra.as_object().unwrap() {
            match v {
                Value::Null => {
                    policy.as_object_mut().unwrap().remove(key);
                }
                v => policy[key] = v.clone(),
            }
        }
        std::fs::write(self.t.join("policy.json"), policy.to_string()).unwrap();
    }
    fn put(&mut self, kind: &str, key: &str, o: Value) {
        self.objects.insert(k(kind, key), o);
        *self.revisions.entry(k(kind, key)).or_insert(0) += 1;
    }
    fn modify(&mut self, kind: &str, key: &str, set: Value) {
        let o = self.objects.get_mut(&k(kind, key)).unwrap();
        for (name, v) in set.as_object().unwrap() {
            o[name] = v.clone();
        }
        *self.revisions.get_mut(&k(kind, key)).unwrap() += 1;
    }
    fn get(&self, kind: &str, key: &str) -> Value {
        self.objects[&k(kind, key)].clone()
    }
    /// A `draft` unit of the intent `widgets` as the Lead added it: no acceptance yet.
    fn unit(&mut self, key: &str, class: &str, repository: bool) {
        let now = stamp(self.clock);
        let mut u = json!({"lane": "objective", "class": class, "intent": oid("intent", "widgets"),
            "objective": "The invented widget list shows each widget once.",
            "write_set": ["src/**", "README.md"],
            "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
            "verifier": {"runtime": "claude", "model": "opus", "effort": "xhigh"},
            "notes": "Invented notes of the Lead.",
            "state": "draft", "writers": [], "rounds": 0, "added_at": now, "amended_at": now});
        if repository {
            u["repository"] = json!("invented/widgets");
            u["branch"] = json!("ap/widgets-list");
        }
        self.put("unit", key, u);
    }
    /// A good amendment: an interface, a check that passes on the base as an invariant, a write set
    /// inside the bound.
    fn good() -> Value {
        json!({"interface": "1. `widgets list` prints one line per widget.",
            "acceptance": [{"check": "true", "kind": "invariant"}],
            "write_set": ["src/widgets.rs"]})
    }
    fn says(&mut self, answers: &[Value]) {
        self.answers = answers.iter().cloned().collect();
    }
    fn write(&mut self) -> Result<(Value, u8), Box<Refusal>> {
        execute(self, "u1", Some(STORE))
    }
    fn ok(&mut self) -> (Value, u8) {
        self.write().unwrap_or_else(|e| panic!("refused: {e:?}"))
    }
    fn refusal(&mut self) -> Refusal {
        match self.write() {
            Err(e) => *e,
            Ok(v) => panic!("expected a refusal, got {v:?}"),
        }
    }
    /// The actions applied, by type, in order.
    fn actions(&self) -> Vec<String> {
        let applies = self
            .calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1.len() > 1 && c.1[..2] == ["action", "apply"]);
        applies
            .map(|c| c.2["action_type"].as_str().unwrap().to_owned())
            .collect()
    }
    fn bodies(&self, action: &str) -> Vec<Value> {
        let all = self
            .calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1.len() > 1 && c.1[..2] == ["action", "apply"]);
        all.filter(|c| c.2["action_type"] == action)
            .map(|c| c.2.clone())
            .collect()
    }
    /// The pauses of 2 seconds: a fetch or a look tried again (`ap run` pauses 100 ms while it waits).
    fn again(&self) -> usize {
        self.pauses
            .iter()
            .filter(|d| **d == Duration::from_secs(2))
            .count()
    }
    fn fetch_calls(&self) -> usize {
        let all = self.calls.iter().filter(|c| c.0 == "git");
        all.filter(|c| c.1.iter().any(|a| a == "fetch")).count()
    }
    fn record(&self, n: u32) -> Value {
        let path = self.home().join(format!("{n}.amend.json.run.json"));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }
    fn prompt(&self, n: u32) -> String {
        std::fs::read_to_string(self.home().join(format!("{n}.prompt.md"))).unwrap()
    }
    /// The environment of the fake CLI's `n`-th launch, over every stage.
    fn env_of(&self, n: usize) -> String {
        std::fs::read_to_string(self.t.join(format!("bin/env.{n}"))).unwrap()
    }
    fn wrap(fields: &Value) -> Value {
        let o = fields.as_object().unwrap().iter();
        o.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
    }
    fn reviews_of(&self, key: &str) -> Vec<(String, Value)> {
        let all = self
            .objects
            .iter()
            .filter(|((t, _), v)| t == "review" && v["unit"]["native_key"] == key);
        all.map(|((_, k), v)| (k.clone(), v.clone())).collect()
    }
    fn exists(&self, id: &Value) -> bool {
        let (kind, key) = (id["object_type"].as_str(), id["native_key"].as_str());
        matches!((kind, key), (Some(t), Some(n)) if self.objects.contains_key(&k(t, n)))
    }
    fn start_failed(&self, p: &Value) -> Vec<&'static str> {
        let mut failed = vec![];
        if !self.exists(&p["task"]) {
            failed.push("unknown_task");
        }
        let duplicate = self.objects.iter().any(|((t, _), r)| {
            t == "review"
                && r["status"] == "running"
                && r["running_in"] == p["task"]
                && r["head"] == p["head"]
                && r["kind"] == p["kind"]
        });
        if duplicate {
            failed.push("duplicate_running_review");
        }
        failed
    }
    fn expected(&self, body: &Value) -> Option<Value> {
        for e in body["expected"].as_array().into_iter().flatten() {
            let key = k("unit", e["id"]["native_key"].as_str().unwrap());
            if Some(format!("r#{}", self.revisions[&key]))
                != e["revision"].as_str().map(str::to_owned)
            {
                return Some(refused("revision_conflict", json!({"id": e["id"]})));
            }
        }
        None
    }
    fn ont(&mut self, args: &[String], input: &Value) -> Answer {
        assert_eq!(args[args.len() - 2..], ["--store", STORE], "{args:?}");
        let now = EPOCH + self.clock;
        match args[0].as_str() {
            "get" => {
                let id = &input["id"];
                let key = k(
                    id["object_type"].as_str().unwrap(),
                    id["native_key"].as_str().unwrap(),
                );
                let Some(o) = self.objects.get(&key) else {
                    return reply(false, &refused("unavailable", json!({})));
                };
                let revision = format!("r#{}", self.revisions[&key]);
                let object = json!({"id": id, "head": revision, "observations": [
                    {"revision": revision, "fields": Self::wrap(o)}]});
                reply(
                    true,
                    &json!({"status": "ok", "now": now, "result": {"objects": [object]}}),
                )
            }
            "search" => {
                let kind = input["object_type"].as_str().unwrap();
                let all = input["where"]["all"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let found = self.objects.iter().filter(|((t, _), o)| {
                    t == kind
                        && all.iter().all(|c| match (c.get("eq"), c.get("in")) {
                            (Some(eq), _) => o[eq[0].as_str().unwrap()] == eq[1],
                            (_, Some(any)) => any[1]
                                .as_array()
                                .unwrap()
                                .contains(&o[any[0].as_str().unwrap()]),
                            _ => panic!("filter {c}"),
                        })
                });
                let objects: Vec<Value> = found
                    .map(|((t, key), o)| json!({"id": oid(t, key), "head": {"fields": Self::wrap(o)}}))
                    .collect();
                reply(
                    true,
                    &json!({"status": "ok", "result": {"basis": {"qualified": true},
                    "objects": objects, "continuation": null}}),
                )
            }
            "render" => {
                assert_eq!(input["view"], "reviews_of_unit");
                let rows = self.reviews_of(input["args"]["unit"].as_str().unwrap());
                let mut table = "| Key | Kind | Head | Round | Status | Verdict | Closed at |\n| --- | --- | --- | --- | --- | --- | --- |\n".to_owned();
                for (key, r) in &rows {
                    table.push_str(&format!(
                        "| {key} | {} | {} | {} | {} | {} | |\n",
                        r["kind"], r["head"], r["round"], r["status"], r["verdict"]
                    ));
                }
                reply(
                    true,
                    &json!({"status": "ok", "markdown": table, "rows": rows.len(), "truncated": false}),
                )
            }
            "action" if args[1] == "validate" => {
                assert_eq!(input["action_type"], "start_unit_review");
                if let Some(conflict) = self.expected(input) {
                    return reply(false, &conflict);
                }
                let failed = self.start_failed(&input["parameters"]);
                let criteria: Vec<Value> = ["unknown_task", "duplicate_running_review"]
                    .iter()
                    .map(|c| json!({"code": c, "passed": !failed.contains(c)}))
                    .collect();
                let key = made_key(
                    input["agent"].as_str().unwrap(),
                    input["request_id"].as_str().unwrap(),
                );
                let edits = match failed.is_empty() {
                    true => json!([{"change": "initial", "id": oid("review", &key)}]),
                    false => json!([]),
                };
                reply(
                    true,
                    &json!({"status": "ok", "now": now, "result": {
                    "valid": failed.is_empty(), "criteria": criteria, "edits": edits}}),
                )
            }
            "action" => self.apply(input),
            other => panic!("ont {other}"),
        }
    }
    fn apply(&mut self, body: &Value) -> Answer {
        let (action, p) = (
            body["action_type"].as_str().unwrap().to_owned(),
            &body["parameters"],
        );
        let derived = format!(
            "{}/{}",
            body["agent"].as_str().unwrap(),
            body["request_id"].as_str().unwrap()
        );
        let content = json!([action, p]);
        if let Some((first, edits)) = self.applied.get(&derived) {
            return match *first == content {
                true => reply(
                    true,
                    &json!({"status": "ok", "result": {"outcome": "already_applied", "edits": edits}}),
                ),
                false => reply(false, &refused("request_id_reused", json!({}))),
            };
        }
        if let Some(conflict) = self.expected(body) {
            return reply(false, &conflict);
        }
        let now = stamp(self.clock);
        let criteria =
            |failed: Vec<&str>| refused("submission_criteria", json!({"failed": failed}));
        let edits = match action.as_str() {
            "amend_unit" => {
                let key = p["unit"]["native_key"].as_str().unwrap().to_owned();
                if self.get("unit", &key)["state"] != "draft" {
                    return reply(false, &criteria(vec!["unit_not_draft"]));
                }
                let mut set = p.clone();
                set.as_object_mut().unwrap().remove("unit");
                set["amended_at"] = json!(now);
                self.modify("unit", &key, set);
                json!([{"change": "update", "id": oid("unit", &key)}])
            }
            "start_unit_review" => {
                let failed = self.start_failed(p);
                if !failed.is_empty() {
                    return reply(false, &criteria(failed));
                }
                let key = made_key(
                    body["agent"].as_str().unwrap(),
                    body["request_id"].as_str().unwrap(),
                );
                let mut r = p.clone();
                (r["status"], r["started_at"], r["running_in"]) =
                    (json!("running"), json!(now), p["task"].clone());
                self.put("review", &key, r);
                json!([{"change": "initial", "id": oid("review", &key)}])
            }
            "close_unit_review" => {
                let key = p["review"]["native_key"].as_str().unwrap().to_owned();
                let mut set = json!({"status": "closed", "closed_at": now, "running_in": null});
                for m in ["verdict", "report", "report_sha256", "account", "session"] {
                    set[m] = p.get(m).cloned().unwrap_or(Value::Null);
                }
                self.modify("review", &key, set);
                json!([{"change": "update", "id": oid("review", &key)}])
            }
            "start_run" => {
                assert_eq!(body["agent"], "ap-run");
                assert_eq!(body["role"], "dispatch");
                assert!(p["role"] == "writer" || p["role"] == "reviewer", "{body}");
                let key = format!(
                    "run-{}",
                    self.objects.keys().filter(|(t, _)| t == "run").count() + 1
                );
                let mut r = p.clone();
                (r["status"], r["started_at"]) = (json!("running"), json!(now));
                self.put("run", &key, r);
                json!([{"change": "initial", "id": oid("run", &key)}])
            }
            "close_run" => {
                assert_eq!(body["agent"], "ap-run");
                let key = p["run"]["native_key"].as_str().unwrap().to_owned();
                let run = self.objects.get(&k("run", &key)).cloned();
                if run.as_ref().is_none_or(|r| r["status"] != "running") {
                    return reply(false, &criteria(vec!["run_not_running"]));
                }
                self.modify(
                    "run",
                    &key,
                    json!({"status": "closed", "state": p["state"], "ended_at": now}),
                );
                json!([{"change": "update", "id": oid("run", &key)}])
            }
            other => panic!("action {other}"),
        };
        self.applied.insert(derived, (content, edits.clone()));
        reply(
            true,
            &json!({"status": "ok", "result": {"outcome": "applied", "edits": edits}}),
        )
    }
}
impl Drop for F {
    fn drop(&mut self) {
        for pgid in self.groups.values() {
            kill_group(*pgid);
        }
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
impl System for F {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        match program {
            "ont" => {
                self.calls
                    .push((program.into(), args.to_vec(), input.clone()));
                self.ont(args, input)
            }
            "systemctl" => {
                self.calls
                    .push((program.into(), args.to_vec(), input.clone()));
                let unit = args.last().unwrap();
                let (success, stdout) = match args[0].as_str() {
                    "stop" => {
                        if let Some(pgid) = self.groups.get(unit) {
                            kill_group(*pgid);
                        }
                        self.alive.remove(unit);
                        (true, vec![])
                    }
                    "is-active" if self.alive.contains(unit) => (true, b"active\n".to_vec()),
                    "is-active" => (false, b"inactive\n".to_vec()),
                    _ => (false, vec![]),
                };
                Ok(Reply {
                    success,
                    bytes: stdout.clone(),
                    stdout,
                })
            }
            "git" => {
                self.calls
                    .push((program.into(), args.to_vec(), input.clone()));
                if args.iter().any(|a| a == "fetch")
                    && let Some(Some(text)) = self.fetches.pop_front()
                {
                    return Ok(Reply {
                        success: false,
                        bytes: text.as_bytes().to_vec(),
                        stdout: vec![],
                    });
                }
                Host.call(program, args, input, ms)
            }
            // A check on the base: the command after `--`, run here, without its view.
            "systemd-run" => {
                let at = args.iter().position(|a| a == "--").unwrap();
                let mut command = args[at + 1..].to_vec();
                if let Some(from) = command.iter().position(|a| a == BWRAP) {
                    let to = from + command[from..].iter().position(|a| a == "--").unwrap();
                    command.drain(from..=to);
                }
                Host.call(&command[0], &command[1..], input, ms)
            }
            BWRAP => Ok(Reply {
                success: true,
                bytes: vec![],
                stdout: b"bubblewrap 0.0.0\n".to_vec(),
            }),
            _ => Host.call(program, args, input, ms),
        }
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<Child> {
        let role = env
            .iter()
            .find(|(k, _)| k == "ONT_ROLE")
            .map(|(_, v)| v.clone());
        let role = role.unwrap_or_default();
        if role == "writer" {
            if let Some(key) = self.amend_at_launch.take() {
                let now = stamp(self.clock);
                self.modify(
                    "unit",
                    key,
                    json!({"objective": "Another session's objective.", "amended_at": now}),
                );
            }
            // The state alone, at the same revision: the revision check of R8 passes, its state check not.
            if let Some(state) = self.state_at_launch.take() {
                self.objects.get_mut(&k("unit", "u1")).unwrap()["state"] = json!(state);
            }
            // The answer of this launch, then a second result the stage must not reach.
            let answer = self
                .answers
                .pop_front()
                .expect("a writer launch no answer was given for");
            let text = format!("{answer}\n{}\n", json!({"objective": "A second result."}));
            std::fs::write(self.t.join("bin/object"), text).unwrap();
        } else {
            let verdict = self.verdicts.pop_front().unwrap_or("accept");
            std::fs::write(self.t.join("bin/verdict"), verdict).unwrap();
        }
        self.roles.push(role);
        let at = argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &argv[..4],
            ["systemd-run", "--scope", "--quiet", "--collect"]
        );
        let unit = argv[..at]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
        let mut at = at;
        let mut wrap = vec![];
        if argv[at + 1] == BWRAP {
            let end = at + 1 + argv[at + 1..].iter().position(|a| a == "--").unwrap();
            wrap = argv[at + 1..=end].to_vec();
            at = end;
        }
        self.wraps.push(wrap);
        let child = Command::new(self.t.join("bin").join(&argv[at + 1]))
            .args(&argv[at + 2..])
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(cwd)
            .stdin(files[0].try_clone()?)
            .stdout(files[1].try_clone()?)
            .stderr(files[2].try_clone()?)
            .process_group(0)
            .spawn()?;
        self.groups.insert(unit.clone(), child.id());
        self.alive.insert(unit);
        Ok(child)
    }
    fn pause(&mut self, duration: Duration) {
        self.pauses.push(duration);
        self.clock += duration.as_secs();
    }
    fn units(&self) -> bool {
        true
    }
    fn home(&self) -> Option<PathBuf> {
        Some(self.t.join("home"))
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(self.t.join("policy.json"))
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}
fn code(r: &Refusal) -> (String, String) {
    (r.code.clone(), r.field.clone())
}

// spec: specs/ap/unit-write.md#R1
#[test]
fn r1_the_form_the_role_and_the_key() {
    let words = |w: &[&str]| w.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(arguments(&words(&["u1"])).unwrap(), ("u1".into(), None));
    assert_eq!(
        arguments(&words(&["u1", "--store", "/s"])).unwrap(),
        ("u1".into(), Some("/s".into()))
    );
    for bad in [
        &[][..],
        &["u1", "--store"],
        &["u1", "--store", "s"],
        &["u1", "u2"],
        &["--store", "/s"],
        &["u1", "--store", "/s", "--store", "/t"],
    ] {
        let e = arguments(&words(bad)).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{bad:?}");
    }
    let mut f = F::new();
    f.env.push(("ONT_ROLE".into(), "lead".into()));
    let r = f.refusal();
    assert_eq!(code(&r), ("role_in_environment".into(), "ONT_ROLE".into()));
    assert_eq!((r.after, r.unit.as_str()), (None, "u1"));
    f.env.pop();
    for key in ["U1", "1u", "u_1", &"u".repeat(60)] {
        let r = *execute(&mut f, key, Some(STORE)).unwrap_err();
        assert_eq!(code(&r), ("invalid_input".into(), "unit".into()), "{key}");
    }
    assert!(f.calls.is_empty(), "no call before the form is checked");
}

// spec: specs/ap/unit-write.md#R2
#[test]
fn r2_the_policy_member_writer_and_its_accounts() {
    let mut f = F::new();
    f.write_policy(json!({"writer": null}));
    assert_eq!(
        code(&f.refusal()),
        ("writer_not_configured".into(), "policy".into())
    );
    std::fs::remove_file(f.t.join("policy.json")).unwrap();
    assert_eq!(
        code(&f.refusal()),
        ("writer_not_configured".into(), "policy".into())
    );
    for (writer, member) in [
        (json!("opus"), "writer"),
        (
            json!({"model": "opus", "effort": "high"}),
            "writer.permission_mode",
        ),
        (
            json!({"model": "opus", "effort": "high", "permission_mode": "auto", "other": 1}),
            "writer",
        ),
        (
            json!({"model": "o p", "effort": "high", "permission_mode": "auto"}),
            "writer.model",
        ),
        (
            json!({"model": "opus", "effort": "high", "permission_mode": "never"}),
            "writer.permission_mode",
        ),
        (
            json!({"model": "opus", "effort": "high", "permission_mode": "auto", "timeout_seconds": 0}),
            "writer.timeout_seconds",
        ),
        (
            json!({"model": "opus", "effort": "high", "permission_mode": "auto", "compact_window": 10}),
            "writer.compact_window",
        ),
        (
            json!({"model": "opus", "effort": "high", "permission_mode": "auto", "directory": "a b"}),
            "writer.directory",
        ),
    ] {
        f.write_policy(json!({ "writer": writer }));
        let r = f.refusal();
        assert_eq!(
            code(&r),
            ("policy_invalid".into(), "policy".into()),
            "{writer}"
        );
        assert!(
            r.detail.as_str().unwrap().starts_with(member),
            "{}",
            r.detail
        );
    }
    // With `--store`, the pages directory is named.
    f.write_policy(
        json!({"writer": {"model": "opus", "effort": "high", "permission_mode": "auto"}}),
    );
    assert_eq!(
        code(&f.refusal()),
        ("invalid_input".into(), "policy".into())
    );
    f.write_policy(json!({"accounts": {"claude/reviewer": [f.p("a1")]}}));
    assert_eq!(
        code(&f.refusal()),
        ("account_not_mapped".into(), "writer".into())
    );
    f.write_policy(json!({"accounts": {"claude/writer": "x"}}));
    assert_eq!(
        code(&f.refusal()),
        ("policy_invalid".into(), "policy".into())
    );
    assert!(
        f.calls.iter().all(|c| c.0 != "ont"),
        "no call of the store before the policy"
    );
    // The key `claude/writer` comes before `claude/<model>` and `claude`; the first path is the account.
    f.write_policy(
        json!({"accounts": {"claude": [f.p("a1"), f.p("a2")], "claude/opus": [f.p("a2")],
        "claude/writer": [f.p("w1"), f.p("a2")]}}),
    );
    f.says(&[F::good()]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("accept"), 0), "{v}");
    let rec = f.record(1);
    assert_eq!(rec["account"], f.p("w1"));
    assert_eq!(rec["model"], "opus");
}

// spec: specs/ap/unit-write.md#R3
#[test]
fn r3_the_unit_is_a_draft_with_a_write_set() {
    let mut f = F::new();
    let r = *execute(&mut f, "u9", Some(STORE)).unwrap_err();
    assert_eq!(code(&r), ("unknown_unit".into(), "unit".into()));
    f.modify("unit", "u1", json!({"state": "ready"}));
    assert_eq!(code(&f.refusal()), ("unit_not_draft".into(), "unit".into()));
    f.modify("unit", "u1", json!({"state": "draft", "write_set": []}));
    assert_eq!(
        code(&f.refusal()),
        ("write_set_missing".into(), "unit".into())
    );
    assert!(f.actions().is_empty());
    assert!(
        !f.t.join("pages").exists(),
        "nothing made before the unit is read"
    );
}

// spec: specs/ap/unit-write.md#R4
#[test]
fn r4_the_lock_and_a_record_left_running() {
    let mut f = F::new();
    let home = f.home();
    // Another call holds the lock.
    let held = hold(&home.join("lock"));
    assert_eq!(code(&f.refusal()), ("write_active".into(), "unit".into()));
    drop(held);
    // A record a killed call left `running`: its scope is stopped, its run closed `killed`, the
    // working and scratch directories emptied; the next launch is the second.
    let path = home.join("1.amend.json.run.json");
    let path_text = path.to_string_lossy().into_owned();
    std::fs::write(
        &path,
        json!({"state": "running", "unit": SCOPE, "session": "s-old",
        "run_object": {"key": "run-9", "start": "applied", "close": null}})
        .to_string(),
    )
    .unwrap();
    f.put(
        "run",
        "run-9",
        json!({"status": "running", "record": path_text, "role": "writer"}),
    );
    f.alive.insert(SCOPE.into());
    for d in ["work/1/x", "scratch/1/tmp"] {
        std::fs::create_dir_all(home.join(d)).unwrap();
    }
    f.says(&[F::good()]);
    let (v, _) = f.ok();
    assert_eq!(v["end"], "accept", "{v}");
    assert!(
        f.calls
            .iter()
            .any(|c| c.0 == "systemctl" && c.1 == ["stop", SCOPE])
    );
    let closed = f.bodies("close_run");
    let first = &closed[0];
    assert_eq!(first["parameters"]["run"]["native_key"], "run-9");
    assert_eq!(first["parameters"]["state"], "killed");
    assert_eq!(first["role"], "dispatch");
    let out = path_text.strip_suffix(".run.json").unwrap();
    assert_eq!(
        first["request_id"],
        format!("run-{}-close", &sha(out.as_bytes())[..32])
    );
    assert_eq!(f.get("run", "run-9")["state"], "killed");
    assert!(!home.join("work/1").exists() && !home.join("scratch/1").exists());
    assert_eq!(v["launches"][0]["n"], 2);
    assert!(home.join("started").is_file());
    // A record of another name is not stopped: the call is refused.
    std::fs::write(
        home.join("3.amend.json.run.json"),
        json!({"state": "running", "unit": "other.scope"}).to_string(),
    )
    .unwrap();
    let r = f.refusal();
    assert_eq!(code(&r), ("run_still_active".into(), "record".into()));
    assert_eq!(r.detail, "other.scope");
    // The home is no link.
    std::fs::remove_file(home.join("3.amend.json.run.json")).unwrap();
    let pages = f.t.join("pages");
    std::fs::rename(&home, pages.join("elsewhere")).unwrap();
    symlink(pages.join("elsewhere"), &home).unwrap();
    assert_eq!(f.refusal().code, "io_error");
}

// spec: specs/ap/unit-write.md#R5
#[test]
fn r5_the_fetch_tried_again_on_a_locked_ref_and_the_text() {
    let mut f = F::new();
    f.fetches = VecDeque::from([Some("error: cannot lock ref 'refs/remotes/origin/main'"); 2]);
    f.says(&[F::good()]);
    let (v, _) = f.ok();
    assert_eq!(v["end"], "accept");
    assert_eq!(f.again(), 2);
    // Five failures of a locked ref: `fetch_failed` after the fifth.
    let mut f = F::new();
    f.fetches = VecDeque::from([Some("error: cannot lock ref 'refs/remotes/origin/main'"); 5]);
    assert_eq!(code(&f.refusal()), ("fetch_failed".into(), "clone".into()));
    assert_eq!((f.fetch_calls(), f.again()), (5, 4));
    // Another failure is not tried again.
    let mut f = F::new();
    f.fetches = VecDeque::from([Some("fatal: invented")]);
    assert_eq!(code(&f.refusal()), ("fetch_failed".into(), "clone".into()));
    assert_eq!((f.fetch_calls(), f.again()), (1, 0));
    assert!(f.actions().is_empty());
    // The text: the page without `--for`, the pages of the dependencies, the frame.
    let mut f = F::new();
    f.unit("u0", "plain", false);
    f.modify("unit", "u0", json!({"state": "merged"}));
    f.modify(
        "unit",
        "u1",
        json!({"depends_on": [oid("unit", "u0"), oid("unit", "ghost")]}),
    );
    f.says(&[F::good()]);
    f.ok();
    let text = f.prompt(1);
    assert!(
        text.starts_with("# Unit u1: contract, lane objective, invented/widgets\n"),
        "{text}"
    );
    assert!(text.contains("\n## The repository at "));
    assert!(text.contains("\n## Pages of the dependencies\n\n### u0: merged\n\n# Unit u0: plain, lane objective, no repository\n"), "{text}");
    assert!(
        text.contains("\n### ghost\n\n(not in the store)\n"),
        "{text}"
    );
    assert!(text.ends_with(FRAME));
    assert!(!text.contains("## Verifier questions"));
    // A text over 1,000,000 bytes.
    let mut f = F::new();
    f.modify("unit", "u1", json!({"interface": "x".repeat(1_000_001)}));
    assert_eq!(
        code(&f.refusal()),
        ("prompt_too_large".into(), "unit".into())
    );
}

// spec: specs/ap/unit-write.md#R6
#[test]
fn r6_the_stage_and_its_working_directory() {
    let mut f = F::new();
    f.says(&[F::good()]);
    f.ok();
    let home = f.home();
    let rec = f.record(1);
    assert_eq!(rec["role"], "writer");
    assert_eq!(rec["runtime"], "claude");
    assert_eq!(rec["cwd"], home.join("work/1").to_string_lossy().as_ref());
    assert_eq!(
        rec["limits"]["scratch"],
        home.join("scratch/1").to_string_lossy().as_ref()
    );
    assert_eq!(
        rec["answer_schema"]["path"],
        home.join("amendment.schema.json")
            .to_string_lossy()
            .as_ref()
    );
    let schema: Value =
        serde_json::from_slice(&std::fs::read(home.join("amendment.schema.json")).unwrap())
            .unwrap();
    assert_eq!(schema["additionalProperties"], false);
    let members: BTreeSet<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(members.len(), 12);
    assert!(!members.contains("class") && !members.contains("notes"));
    // The working directory was the copy of the clone, and it is gone.
    assert!(
        f.env_of(1)
            .contains(&format!("PWD={}", home.join("work/1").display()))
    );
    assert!(!home.join("work/1").exists());
    // The run object of the stage: role writer, its start claimed as `dispatch`.
    let run = f.bodies("start_run");
    assert_eq!(run[0]["role"], "dispatch");
    assert_eq!(run[0]["parameters"]["role"], "writer");
    assert_eq!(run[0]["parameters"]["unit"]["native_key"], "u1");
    // A refusal of `ap run` ends the call with its code and field.
    let mut f = F::new();
    f.write_policy(
        json!({"writer": {"model": "haiku", "effort": "high", "permission_mode": "auto",
        "directory": f.p("pages")}}),
    );
    let r = f.refusal();
    assert_eq!(code(&r), ("model_not_allowed".into(), "model".into()));
    assert!(!f.home().join("work/1").exists());
}

// spec: specs/ap/unit-write.md#R7
#[test]
fn r7_the_bound_of_an_amendment() {
    let bound = vec!["a/**".to_owned(), "b/c.rs".to_owned()];
    for p in ["a/x.rs", "a/d/**", "a/**", "b/c.rs"] {
        assert!(inside(p, &bound), "{p}");
    }
    for p in ["a", "b/**", "b/c.rs/x", "ab/x", "a/*", "**"] {
        assert!(!inside(p, &bound), "{p}");
    }
    // A member the Lead keeps, and a pattern outside: refused, and no `amend_unit` for either.
    let mut f = F::new();
    f.says(&[
        json!({"class": "plain"}),
        json!({"write_set": ["src/widgets.rs", "docs/**"]}),
        json!({"depends_on": []}),
    ]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("refused"), 3), "{v}");
    assert_eq!(v["amendments"], 0);
    assert!(f.bodies("amend_unit").is_empty());
    assert_eq!(
        v["refusal"],
        json!({"code": "amendment_not_admitted", "field": "depends_on", "detail": null})
    );
    let second = f.prompt(3);
    assert!(second.contains("\"code\":\"write_set_outside\",\"detail\":{\"bound\":[\"src/**\",\"README.md\"],\"pattern\":\"docs/**\"},\"field\":\"write_set.1\""), "{second}");
}

// spec: specs/ap/unit-write.md#R8
#[test]
fn r8_the_apply_by_the_code_of_amend() {
    let mut f = F::new();
    f.says(&[F::good()]);
    f.ok();
    let [body] = &f.bodies("amend_unit")[..] else {
        panic!()
    };
    let rid = body["request_id"].as_str().unwrap();
    let (head, n) = rid
        .strip_suffix("-amend")
        .unwrap()
        .rsplit_once('-')
        .unwrap();
    let drawn = head.strip_prefix("pw-").unwrap();
    assert!(
        drawn.len() == 16 && drawn.bytes().all(|c| c.is_ascii_hexdigit()) && n == "1",
        "{rid}"
    );
    assert_eq!(
        (&body["agent"], &body["session"]),
        (&json!("ap-write"), &json!("p-u1"))
    );
    assert_eq!(body["parameters"]["write_set"], json!(["src/widgets.rs"]));
    assert_eq!(f.get("unit", "u1")["notes"], "Invented notes of the Lead.");
    // Another session amended the unit while the writer ran.
    let mut f = F::new();
    f.amend_at_launch = Some("u1");
    f.says(&[F::good()]);
    let r = f.refusal();
    assert_eq!(code(&r), ("unit_changed".into(), "unit".into()));
    assert_eq!(r.after, None);
    // The unit left `draft`.
    let mut f = F::new();
    f.state_at_launch = Some("dropped");
    f.says(&[F::good()]);
    assert_eq!(code(&f.refusal()), ("unit_not_draft".into(), "unit".into()));
    // A refusal of the checks of `amend` goes back with its `fields`.
    let mut f = F::new();
    f.says(&[json!({"objective": "x".repeat(2001)}), F::good()]);
    let (v, _) = f.ok();
    assert_eq!(v["end"], "accept");
    assert_eq!(v["amendments"], 1);
    let back = f.prompt(2);
    let line: Value = serde_json::from_str(back.lines().nth(1).unwrap()).unwrap();
    assert_eq!(line["code"], "invalid_input");
    assert_eq!(line["fields"][0]["rule"], "max_bytes");
}

// spec: specs/ap/unit-write.md#R9
#[test]
fn r9_the_look_resumes_the_last_closed_look_and_a_failed_fetch_is_repeated() {
    let mut f = F::new();
    f.verdicts = VecDeque::from(["repair", "accept"]);
    f.says(&[
        F::good(),
        json!({"interface": "1. `widgets list` prints each widget once."}),
    ]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("accept"), 0), "{v}");
    let reviews: Vec<&str> = v["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(reviews.len(), 2);
    let starts = f.bodies("start_unit_review");
    assert_eq!(starts[0]["agent"], "ap-write");
    assert_eq!(starts[0]["session"], "p-u1");
    let second = &f.get("review", reviews[1]);
    assert_eq!(second["head"], page_digest("u1", &f.get("unit", "u1")));
    // The second look resumed the first: its stage resumed that look's record.
    let report = f.get("review", reviews[0])["report"]
        .as_str()
        .unwrap()
        .to_owned();
    let rec2: Value = serde_json::from_slice(
        &std::fs::read(f.p(&format!("reviews/{}.md.run.json", reviews[1]))).unwrap(),
    )
    .unwrap();
    assert_eq!(rec2["resumes"], format!("{report}.run.json"));
    // A plain unit is looked at too.
    let mut f = F::new();
    f.modify("unit", "u1", json!({"class": "plain"}));
    f.says(&[F::good()]);
    assert_eq!(f.ok().0["reviews"].as_array().unwrap().len(), 1);
    // A look refused `fetch_failed` is repeated after 2 seconds, twice at most.
    let mut f = F::new();
    f.fetches = VecDeque::from([None, Some("fatal: a"), Some("fatal: b")]);
    f.says(&[F::good()]);
    let (v, _) = f.ok();
    assert_eq!(v["end"], "accept");
    assert_eq!(f.again(), 2);
    let mut f = F::new();
    f.fetches = VecDeque::from([None, Some("fatal: a"), Some("fatal: b"), Some("fatal: c")]);
    f.says(&[F::good()]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("look_refused"), 3));
    assert_eq!(v["refusal"]["code"], "fetch_failed");
    assert_eq!(f.fetch_calls(), 4);
}

// spec: specs/ap/unit-write.md#R10
#[test]
fn r10_each_way_back_resumes_the_launch_before() {
    // A refused amendment, a look refused on the base, a repair.
    let mut f = F::new();
    f.verdicts = VecDeque::from(["repair", "accept"]);
    f.says(&[
        json!({"branch": "x"}),
        json!({"acceptance": [{"check": "true", "kind": "fails_on_base"}], "write_set": ["src/widgets.rs"],
            "interface": "1. An invented point."}),
        F::good(),
    ]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("repair"), 3), "{v}");
    let ends: Vec<&str> = v["launches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["state"].as_str().unwrap())
        .collect();
    assert_eq!(ends, ["ok", "ok", "ok"]);
    for n in [2, 3] {
        let rec = f.record(n);
        let before = f.home().join(format!("{}.amend.json.run.json", n - 1));
        assert_eq!(rec["resumes"], before.to_string_lossy().as_ref(), "{n}");
        assert_eq!(rec["session"], f.record(1)["session"]);
    }
    let back = f.prompt(2);
    assert!(
        back.starts_with("Your amendment was not taken as it stands:"),
        "{back}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(back.lines().nth(1).unwrap()).unwrap()["code"],
        "amendment_not_admitted"
    );
    let page = f.prompt(3);
    assert_eq!(
        serde_json::from_str::<Value>(page.lines().nth(1).unwrap()).unwrap()["code"],
        "acceptance_passes_on_base"
    );
    // The third launch's look said `repair`: what would go back ends the call.
    assert_eq!(v["amendments"], 2);
    assert_eq!(v["refusal"], Value::Null);
    // A repair then goes back as the path of the look's report.
    let mut f = F::new();
    f.verdicts = VecDeque::from(["repair", "accept"]);
    f.says(&[F::good(), F::good()]);
    let (v, _) = f.ok();
    let first = v["reviews"][0].as_str().unwrap();
    let report = f.get("review", first)["report"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(f.prompt(2).lines().nth(1).unwrap(), report);
}

// spec: specs/ap/unit-write.md#R10
#[test]
fn r10_a_look_refused_for_a_citation_goes_back_to_the_writer() {
    let mut f = F::new();
    let mut missing = F::good();
    missing["read_first"] = json!(["docs/missing.txt what it is for"]);
    let mut held = F::good();
    held["read_first"] = json!(["AGENTS.md what it is for"]);
    f.says(&[missing, held]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("accept"), 0), "{v}");
    assert_eq!(v["launches"].as_array().unwrap().len(), 2);
    assert_eq!(v["amendments"], 2);
    // The next launch's prompt: the fixed sentence, then the look's refusal as one JSON line.
    let back = f.prompt(2);
    assert!(
        back.starts_with("Your amendment was not taken as it stands:"),
        "{back}"
    );
    let line: Value = serde_json::from_str(back.lines().nth(1).unwrap()).unwrap();
    assert_eq!(line["code"], "read_first_missing");
    assert_eq!(line["field"], "read_first.0");
    assert_eq!(line["after"], Value::Null);
}

// spec: specs/ap/unit-write.md#R11
#[test]
fn r11_the_answer_and_each_end() {
    let mut f = F::new();
    f.says(&[F::good()]);
    let (v, status) = f.ok();
    assert_eq!(status, 0);
    let members: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        members,
        BTreeSet::from([
            "unit",
            "end",
            "launches",
            "amendments",
            "reviews",
            "digest",
            "refusal"
        ])
    );
    assert_eq!(v["digest"], page_digest("u1", &f.get("unit", "u1")));
    let l = &v["launches"][0];
    assert_eq!(
        (l["n"].as_u64(), l["state"].as_str()),
        (Some(1), Some("ok"))
    );
    assert_eq!(l["run"], f.record(1)["run_object"]["key"]);
    // `stage`: the writer did not end `ok`.
    let mut f = F::new();
    std::fs::write(f.t.join("bin/mode-w1"), "noout").unwrap();
    f.says(&[F::good()]);
    let (v, status) = f.ok();
    assert_eq!(
        (v["end"].as_str(), status, v["amendments"].as_u64()),
        (Some("stage"), 3, Some(0))
    );
    assert_eq!(v["launches"][0]["state"], "failed");
    // `look_abandoned`: the reviewer gave no verdict.
    let mut f = F::new();
    std::fs::write(f.t.join("bin/none"), "").unwrap();
    f.says(&[F::good()]);
    let (v, status) = f.ok();
    assert_eq!(
        (v["end"].as_str(), status),
        (Some("look_abandoned"), 3),
        "{v}"
    );
    // `refused`: three amendments refused.
    let mut f = F::new();
    f.says(&[
        json!({"notes": "x"}),
        json!({"notes": "x"}),
        json!({"repository": "o/r"}),
    ]);
    let (v, status) = f.ok();
    assert_eq!((v["end"].as_str(), status), (Some("refused"), 3));
    assert_eq!(v["refusal"]["field"], "repository");
    assert_eq!(v["reviews"], json!([]));
}

// spec: specs/ap/unit-write.md#R12
#[test]
fn r12_the_store_only_through_amend_the_look_and_the_run() {
    let mut f = F::new();
    f.verdicts = VecDeque::from(["repair", "accept"]);
    f.says(&[json!({"class": "effect"}), F::good(), F::good()]);
    let (v, _) = f.ok();
    assert_eq!(v["end"], "accept");
    let allowed = [
        "amend_unit",
        "start_unit_review",
        "close_unit_review",
        "start_run",
        "close_run",
    ];
    let actions = f.actions();
    assert!(
        actions.iter().all(|a| allowed.contains(&a.as_str())),
        "{actions:?}"
    );
    assert!(!actions.contains(&"ready_unit".to_owned()));
    let u = f.get("unit", "u1");
    assert_eq!(
        (u["state"].as_str(), u["class"].as_str()),
        (Some("draft"), Some("contract"))
    );
    // Files of its own only under its home: the prompts, the stage's files, the schema, the lock.
    let mut names: Vec<String> = std::fs::read_dir(f.home())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for n in &names {
        let known = [
            "lock",
            "started",
            "amendment.schema.json",
            "work",
            "scratch",
        ]
        .contains(&n.as_str())
            || n.split_once('.')
                .is_some_and(|(m, _)| m.bytes().all(|c| c.is_ascii_digit()));
        assert!(known, "{n}");
    }
    assert!(
        std::fs::read_dir(f.home().join("work"))
            .unwrap()
            .next()
            .is_none()
    );
}

// spec: specs/ap/unit-write.md#R13
#[test]
fn r13_the_frame() {
    assert!(FRAME.starts_with("## How you work\n"));
    let order = [
        "You are the writer of this unit's PAGE",
        "Your final message is the amendment",
        "The write set stays inside the one above",
        "The objective is sharpened and never widened",
        "Every fact about the source is read now",
        "Each acceptance check you give is run",
        "echo '{\"parameters\":{\"words\":\"<what you look for>\",\"limit\":5}}' | ap fn route",
        "Each message you send re-reads this whole session",
    ];
    let mut at = 0;
    for words in order {
        let found = FRAME[at..].find(words).unwrap_or_else(|| panic!("{words}"));
        at += found + words.len();
    }
    assert!(FRAME.contains("mktemp -d") && FRAME.contains("English only"));
    assert!(!FRAME.contains("amend.json") && !FRAME.contains("scratch"));
}

// spec: specs/ap/run.md#R53
#[test]
fn r53_a_writer_stage() {
    let mut f = F::new();
    f.says(&[F::good()]);
    f.ok();
    // The writer's launch: its role, the view of `limits`, and the end at its first result.
    assert_eq!(f.roles[0], "writer");
    assert!(f.env_of(1).lines().any(|l| l == "ONT_ROLE=writer"));
    assert_eq!(f.wraps[0].first().map(String::as_str), Some(BWRAP));
    let rec = f.record(1);
    assert_eq!(rec["state"], "ok");
    assert_eq!(rec["results"], 1);
    let answer: Value =
        serde_json::from_slice(&std::fs::read(f.home().join("1.amend.json")).unwrap()).unwrap();
    assert_eq!(answer, F::good());
    // Admission: Claude alone, and `limits` with `scratch` alone.
    let t = f.t.clone();
    let p = |rel: &str| t.join(rel).to_string_lossy().into_owned();
    std::fs::write(t.join("prompt.md"), "invented").unwrap();
    let base = json!({"role": "writer", "account": p("w1"), "model": "opus", "effort": "high",
        "permission_mode": "bypassPermissions", "cwd": p("origin"), "prompt": p("prompt.md"),
        "out": p("w.json"), "limits": {"scratch": p("ws")}});
    let mut r = base.clone();
    r["runtime"] = json!("agy");
    let e = ap::run::execute(&mut f, &r).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "role"), "agy");
    let mut r = base.clone();
    r.as_object_mut().unwrap().remove("limits");
    let e = ap::run::execute(&mut f, &r).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "limits"));
    let mut r = base.clone();
    r["limits"]["write"] = json!([p("origin")]);
    let e = ap::run::execute(&mut f, &r).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "limits"));
    assert!(!t.join("w.json.run.json").exists() && !t.join("ws").exists());
}
