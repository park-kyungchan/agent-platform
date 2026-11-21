//! `ap unit review` (`specs/ap/unit-review.md`) against fakes: a fake of `ont` that answers the reads and
//! the actions of this verb, of `ap run` and of `ap unit ready` as the engine does (submission criteria,
//! `expected` revisions, request ids replayed as `already_applied` or refused `request_id_reused`, a
//! refused request id left unused, the key of a start derived from the client, the agent and the request
//! id, the view `reviews_of_unit`); the fake `claude` of tests/fixtures/fake-claude.sh in mode `script`
//! with the reviewer of tests/fixtures/unit_review/script.sh; a scratch repository and its clone;
//! `systemctl` and the unit launcher replaced as in tests/run.rs. The policy file and the reviews
//! directory are under a temporary directory: no test starts a unit of the service manager or reaches
//! the host's store, policy file or reviews directory.
use ap::host::{Answer, Failure, Host, Reply, System};
use ap::unit::page_review::{Refusal, arguments, execute};
use ap::unit::render::{Role, page, page_digest};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
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
const PLAN_PART: &str = include_str!("fixtures/unit_review/plan-part.md");
const REPAIR_LOOK: &str = include_str!("../src/unit/frames/repair-look.md");
const PLAN_REVIEWER: &str = include_str!("../src/unit/frames/plan-reviewer.md");
const STORE: &str = "/invented/scratch/store";
const BWRAP: &str = "/usr/bin/bwrap";
/// The epoch second of `2026-11-18T00:00:00Z`, the fake store's clock at 0.
const EPOCH: u64 = 1_794_960_000;

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
fn lost() -> Answer {
    Err(Failure {
        code: "process_failed",
        field: "ont".into(),
        detail: Value::Null,
    })
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
    policy: Option<PathBuf>,
    launches: usize,
    /// `systemctl stop` does nothing.
    stop_fails: bool,
    /// After this action is applied, every later call of `ont` fails and no stage launches: the call was cut.
    die_after: Option<&'static str>,
    dead: bool,
    /// When a stage launches, or when a start is validated, this unit is amended (another session's).
    amend_at_launch: Option<&'static str>,
    /// When a stage launches, this unit's tiers and estimate are amended, and nothing else.
    tiers_at_launch: Option<&'static str>,
    amend_at_validate: Option<&'static str>,
    /// When `close_run` is applied, the run's report is replaced.
    replace_report: bool,
    /// Every session but the caller's wrote to the units.
    stage_writes: bool,
    /// When a start is applied, a file is put at the review's out: its stage is refused at its start.
    block_out: bool,
    /// The apply of a start names another key than its validate.
    other_key: bool,
    /// Every call of `git`, by its arguments.
    git: Vec<Vec<String>>,
    /// The unit of a check of the base is read as active after its stop.
    checks_alive: bool,
    /// The words of each launch between `systemd-run … --` and the CLI: `bwrap … --` under
    /// `limits`, else none. They are dropped before the fake CLI runs.
    wraps: Vec<Vec<String>>,
    /// The answer of `/usr/bin/bwrap --version`, and how many times it was called.
    bwrap: bool,
    probes: usize,
}
impl F {
    fn new() -> Self {
        // Resolved through links once: a scratch path below it holds no link (R2 of run-limits).
        let t = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "ap-unit-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "origin", "home/.ssh"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let fake = t.join("bin/claude");
        std::fs::write(&fake, FAKE).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        for account in ["a1", "a2"] {
            std::fs::write(t.join(format!("bin/mode-{account}")), "script").unwrap();
            std::fs::write(t.join(format!("bin/script-{account}.sh")), SCRIPT).unwrap();
        }
        let origin = t.join("origin");
        git(&origin, &["init", "--quiet", "-b", "main"]);
        std::fs::write(
            origin.join("AGENTS.md"),
            "# Invented widgets\n\nRead src/.\n",
        )
        .unwrap();
        git(&origin, &["add", "-A"]);
        git(&origin, &["commit", "--quiet", "-m", "invented"]);
        git(&t, &["clone", "--quiet", origin.to_str().unwrap(), "clone"]);
        let mut f = Self {
            policy: Some(t.join("policy.json")),
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
            launches: 0,
            stop_fails: false,
            die_after: None,
            dead: false,
            amend_at_launch: None,
            tiers_at_launch: None,
            amend_at_validate: None,
            replace_report: false,
            stage_writes: false,
            block_out: false,
            other_key: false,
            git: vec![],
            checks_alive: false,
            wraps: vec![],
            bwrap: true,
            probes: 0,
        };
        f.write_policy(json!({}));
        f.put("task", "invented-task", json!({"status": "open"}));
        f.put(
            "intent",
            "widgets",
            json!({"status": "confirmed", "outcome": "Every invented widget is listed once.",
                "first_user": "An invented reader of the widget list.",
                "not_included": "No prices of widgets.", "constraints": "One line per widget."}),
        );
        f.unit("u1", "contract", true);
        f
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_string_lossy().into_owned()
    }
    fn dir(&self) -> PathBuf {
        self.t.join("reviews")
    }
    /// The policy file: `models` and `accounts` for `ap run`, the clone, and `reviews` with `extra`'s
    /// members over the defaults (a null member left out).
    fn write_policy(&self, extra: Value) {
        let mut reviews = json!({"model": "sonnet", "effort": "high", "account": self.p("a1"),
            "permission_mode": "auto", "directory": self.dir(), "task": "invented-task"});
        for (key, v) in extra.as_object().unwrap() {
            match v {
                Value::Null => {
                    reviews.as_object_mut().unwrap().remove(key);
                }
                v => reviews[key] = v.clone(),
            }
        }
        let policy = json!({"models": {"allowed": ["sonnet", "opus"], "never": []},
            "accounts": {"claude": [self.p("a1"), self.p("a2")]},
            "repositories": {"invented/widgets": {"clone": self.p("clone")}}, "reviews": reviews});
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
    /// A `draft` unit of the intent `widgets`, with or without the repository `invented/widgets`.
    fn unit(&mut self, key: &str, class: &str, repository: bool) {
        let now = stamp(self.clock);
        let mut u = json!({"lane": "objective", "class": class, "intent": oid("intent", "widgets"),
            "objective": "The invented widget list shows each widget once.",
            "interface": "1. `widgets list` prints one line per widget.",
            "write_set": ["src/widgets.rs"], "acceptance": [{"check": "true", "kind": "invariant"}],
            "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
            "verifier": {"runtime": "claude", "model": "opus", "effort": "xhigh"},
            "estimate_lines": 40, "state": "draft", "writers": [], "rounds": 0,
            "added_at": now, "amended_at": now});
        if repository {
            u["repository"] = json!("invented/widgets");
            u["branch"] = json!("ap/widgets-list");
        }
        self.put("unit", key, u);
    }
    /// Another session's `amend_unit`: a new objective, `amended_at` now.
    fn amend(&mut self, key: &str) {
        let now = stamp(self.clock);
        let objective = format!("The invented widget list shows each widget once ({now}).");
        self.modify(
            "unit",
            key,
            json!({"objective": objective, "amended_at": now}),
        );
    }
    fn tick(&mut self, seconds: u64) {
        self.clock += seconds;
    }
    fn mode(&self, account: &str, mode: &str) {
        std::fs::write(self.t.join(format!("bin/mode-{account}")), mode).unwrap();
    }
    /// A file of the reviewer's directory: `verdict`, `digest`, `digest-once`, `none` or `touch`.
    fn answer(&self, name: &str, text: Option<&str>) {
        let path = self.t.join("bin").join(name);
        match text {
            Some(text) => std::fs::write(path, text).unwrap(),
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    fn count(&self) -> String {
        std::fs::read_to_string(self.t.join("bin/count"))
            .unwrap_or_default()
            .trim()
            .to_owned()
    }
    fn seen(&self, what: &str) -> String {
        std::fs::read_to_string(self.t.join(format!("bin/{what}.{}", self.count()))).unwrap()
    }
    fn req(&self, kind: &str, key: &str) -> Value {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        json!({"request_id": format!("rq{n}"), "agent": "lead", "session": "lead-1", kind: key})
    }
    fn review(&mut self, r: &Value) -> Result<(Value, u8), Box<Refusal>> {
        execute(self, Some(STORE), r)
    }
    fn ok(&mut self, r: &Value) -> (Value, u8) {
        self.review(r).unwrap_or_else(|e| panic!("refused: {e:?}"))
    }
    fn refusal(&mut self, r: &Value) -> Refusal {
        match self.review(r) {
            Err(e) => *e,
            Ok(v) => panic!("expected a refusal, got {v:?}"),
        }
    }
    /// The actions applied, by type, in order.
    fn actions(&self) -> Vec<String> {
        let applies = self
            .calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1[..2] == ["action", "apply"]);
        applies
            .map(|c| c.2["action_type"].as_str().unwrap().to_owned())
            .collect()
    }
    fn bodies(&self, verb: &str, action: &str) -> Vec<Value> {
        let all = self
            .calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1.len() > 1 && c.1[1] == verb);
        all.filter(|c| c.2["action_type"] == action)
            .map(|c| c.2.clone())
            .collect()
    }
    fn reviews_of(&self, member: &str, key: &str) -> Vec<(String, Value)> {
        let all = self
            .objects
            .iter()
            .filter(|((t, _), v)| t == "review" && v[member]["native_key"] == key);
        all.map(|((_, k), v)| (k.clone(), v.clone())).collect()
    }
    fn work(&self) -> Vec<String> {
        let entries = std::fs::read_dir(self.dir().join("work"))
            .into_iter()
            .flatten();
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
    fn wrap(fields: &Value) -> Value {
        let o = fields.as_object().unwrap().iter();
        o.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
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
        let missing = (p["kind"] == "page" && p.get("unit").is_none())
            || (p["kind"] == "plan" && p.get("intent").is_none());
        if missing {
            failed.push("subject_required");
        }
        if p.get("unit").is_some_and(|u| !self.exists(u)) {
            failed.push("unknown_unit");
        }
        if p.get("intent").is_some_and(|i| !self.exists(i)) {
            failed.push("unknown_intent");
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
        if self.dead {
            return lost();
        }
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
                let rows = self.reviews_of("unit", input["args"]["unit"].as_str().unwrap());
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
            "action" if args[1] == "validate" => self.validate(input),
            "action" => self.apply(input),
            other => panic!("ont {other}"),
        }
    }
    fn validate(&mut self, body: &Value) -> Answer {
        assert_eq!(body["action_type"], "start_unit_review");
        if let Some(key) = self.amend_at_validate.take() {
            self.amend(key);
        }
        if let Some(conflict) = self.expected(body) {
            return reply(false, &conflict);
        }
        let failed = self.start_failed(&body["parameters"]);
        let criteria: Vec<Value> = [
            "unknown_task",
            "duplicate_running_review",
            "subject_required",
            "unknown_unit",
            "unknown_intent",
        ]
        .iter()
        .map(|c| json!({"code": c, "passed": !failed.contains(c)}))
        .collect();
        let key = made_key(
            body["agent"].as_str().unwrap(),
            body["request_id"].as_str().unwrap(),
        );
        let edits = match failed.is_empty() {
            true => json!([{"change": "initial", "id": oid("review", &key)}]),
            false => json!([]),
        };
        reply(
            true,
            &json!({"status": "ok", "now": EPOCH + self.clock, "result": {
            "valid": failed.is_empty(), "criteria": criteria, "edits": edits}}),
        )
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
            "start_unit_review" => {
                let failed = self.start_failed(p);
                if !failed.is_empty() {
                    return reply(false, &criteria(failed));
                }
                let mut key = made_key(
                    body["agent"].as_str().unwrap(),
                    body["request_id"].as_str().unwrap(),
                );
                if self.other_key {
                    key.push_str("-other");
                }
                let mut r = p.clone();
                (r["status"], r["started_at"], r["running_in"]) =
                    (json!("running"), json!(now), p["task"].clone());
                self.put("review", &key, r);
                if self.block_out {
                    std::fs::write(self.dir().join(format!("{key}.md")), "in the way").unwrap();
                }
                json!([{"change": "initial", "id": oid("review", &key)}])
            }
            "close_unit_review" => {
                let key = p["review"]["native_key"].as_str().unwrap().to_owned();
                let r = self.objects.get(&k("review", &key)).cloned();
                let mut failed = vec![];
                if r.as_ref().is_none_or(|r| r["status"] != "running") {
                    failed.push("review_not_running");
                }
                if p["verdict"] != "abandoned"
                    && (p.get("report").is_none() || p.get("report_sha256").is_none())
                {
                    failed.push("report_required");
                }
                if r.as_ref()
                    .is_some_and(|r| r.get("unit").is_some() && r["unit"] != p["unit"])
                {
                    failed.push("unit_differs");
                }
                if p.get("unit").is_some() && p.get("session").is_none() {
                    failed.push("session_required");
                }
                let unit = p["unit"]["native_key"]
                    .as_str()
                    .and_then(|u| self.objects.get(&k("unit", u)));
                let wrote =
                    unit.is_some_and(|u| u["writers"].as_array().unwrap().contains(&p["session"]));
                if wrote
                    || (self.stage_writes && p.get("unit").is_some() && p["session"] != "lead-1")
                {
                    failed.push("writer_cannot_review");
                }
                if !failed.is_empty() {
                    return reply(false, &criteria(failed));
                }
                let mut set = json!({"status": "closed", "closed_at": now, "running_in": null});
                for m in ["verdict", "report", "report_sha256", "account", "session"] {
                    set[m] = p.get(m).cloned().unwrap_or(Value::Null);
                }
                self.modify("review", &key, set);
                json!([{"change": "update", "id": oid("review", &key)}])
            }
            "start_run" => {
                assert_eq!(
                    (&body["agent"], &body["role"]),
                    (&json!("ap-run"), &json!("reviewer"))
                );
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
                assert_eq!(
                    (&body["agent"], &body["role"]),
                    (&json!("ap-run"), &json!("reviewer"))
                );
                let key = p["run"]["native_key"].as_str().unwrap().to_owned();
                let run = self.objects.get(&k("run", &key)).cloned();
                if run.as_ref().is_none_or(|r| r["status"] != "running") {
                    return reply(false, &criteria(vec!["run_not_running"]));
                }
                if self.replace_report {
                    let record = run.unwrap()["record"].as_str().unwrap().to_owned();
                    std::fs::write(
                        record.trim_end_matches(".run.json"),
                        "Verdict: accept\nreplaced",
                    )
                    .unwrap();
                }
                self.modify(
                    "run",
                    &key,
                    json!({"status": "closed", "state": p["state"], "ended_at": now}),
                );
                json!([{"change": "update", "id": oid("run", &key)}])
            }
            "ready_unit" => {
                let key = p["unit"]["native_key"].as_str().unwrap().to_owned();
                let u = self.get("unit", &key);
                let mut failed = vec![];
                if u["state"] != "draft" {
                    failed.push("unit_not_draft");
                }
                // Version 12: an accept whose head is `page_digest`, whenever it started.
                let reviewed = self.reviews_of("unit", &key).iter().any(|(_, r)| {
                    r["kind"] == "page"
                        && r["status"] == "closed"
                        && r["verdict"] == "accept"
                        && p.get("page_digest").is_some_and(|d| &r["head"] == d)
                });
                let class = &u["class"];
                if (class == "contract" || class == "effect") && !reviewed {
                    failed.push("page_not_reviewed");
                }
                if !failed.is_empty() {
                    return reply(false, &criteria(failed));
                }
                self.modify("unit", &key, json!({"state": "ready", "ready_at": now}));
                json!([{"change": "update", "id": oid("unit", &key)}])
            }
            other => panic!("action {other}"),
        };
        self.applied.insert(derived, (content, edits.clone()));
        if self.die_after == Some(action.as_str()) {
            self.dead = true;
        }
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
                    "stop" if self.stop_fails => (false, vec![]),
                    "stop" => {
                        if let Some(pgid) = self.groups.get(unit) {
                            kill_group(*pgid);
                        }
                        self.alive.remove(unit);
                        (true, vec![])
                    }
                    "is-active"
                        if self.alive.contains(unit)
                            || (self.checks_alive && unit.starts_with("ap-check-")) =>
                    {
                        (true, b"active\n".to_vec())
                    }
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
                self.git.push(args.to_vec());
                Host.call(program, args, input, ms)
            }
            // A check of `ap unit ready`: the command after `--`, run here (no unit is started).
            "systemd-run" => {
                let at = args.iter().position(|a| a == "--").unwrap();
                Host.call(&args[at + 1], &args[at + 2..], input, ms)
            }
            // The probe of `limits`: no `bwrap` of the host is called.
            BWRAP => {
                assert_eq!(args, ["--version"]);
                self.probes += 1;
                Ok(Reply {
                    success: self.bwrap,
                    bytes: vec![],
                    stdout: b"bubblewrap 0.0.0\n".to_vec(),
                })
            }
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
        if self.dead {
            return Err(std::io::ErrorKind::Other.into());
        }
        if let Some(key) = self.amend_at_launch.take() {
            self.amend(key);
        }
        if let Some(key) = self.tiers_at_launch.take() {
            let now = stamp(self.clock);
            self.modify(
                "unit",
                key,
                json!({"implementer": {"runtime": "codex", "model": "m9", "effort": "low"},
                    "estimate_lines": 77, "amended_at": now}),
            );
        }
        self.launches += 1;
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
        // `bwrap … --` in front of the CLI is kept for the tests and dropped here.
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
        self.policy.clone()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}
fn code(e: &Refusal) -> (String, String) {
    (e.code.clone(), e.field.clone())
}
fn record(report: &str) -> Value {
    serde_json::from_slice(&std::fs::read(format!("{report}.run.json")).unwrap()).unwrap()
}
fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

// spec: specs/ap/unit-review.md#R1
#[test]
fn r1_the_form_and_a_malformed_request_are_refused_before_any_call() {
    let words = |w: &[&str]| w.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(arguments(&words(&[])).unwrap(), None);
    assert_eq!(
        arguments(&words(&["--store", "/s"])).unwrap(),
        Some("/s".into())
    );
    for bad in [
        &["--store", "s"][..],
        &["--store"],
        &["x"],
        &["--store", "/s", "--store", "/t"],
    ] {
        assert_eq!(arguments(&words(bad)).unwrap_err().code, "usage", "{bad:?}");
    }
    let mut f = F::new();
    let base = f.req("unit", "u1");
    let mut cases = vec![(json!([]), "stdin"), (json!("u1"), "stdin")];
    for (member, value, field) in [
        ("colour", json!("blue"), "colour"),
        ("request_id", json!("a b"), "request_id"),
        ("request_id", json!("r".repeat(45)), "request_id"),
        ("agent", json!(""), "agent"),
        ("session", json!(7), "session"),
        ("timeout_seconds", json!(0), "timeout_seconds"),
    ] {
        let mut r = base.clone();
        r[member] = value;
        cases.push((r, field));
    }
    for (r, field) in cases {
        let e = f.refusal(&r);
        assert_eq!(code(&e), ("invalid_input".into(), field.into()), "{r}");
        assert_eq!((e.after, &e.reviews), (None, &Value::Null));
    }
    f.env.push(("ONT_ROLE".into(), "lead".into()));
    assert_eq!(
        code(&f.refusal(&base)),
        ("role_in_environment".into(), "ONT_ROLE".into())
    );
    assert!(f.calls.is_empty(), "{:?}", f.calls);
    // The command: `usage`, and a refusal object with its seven members on standard error.
    let ap = |args: &[&str], stdin: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(args)
            .env_remove("ONT_ROLE")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        // A refusal by the arguments comes before standard input is read: the pipe may be closed.
        let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
        child.wait_with_output().unwrap()
    };
    let out = ap(&["unit", "review", "--colour"], "{}");
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (out.status.code(), &e["error"]["code"]),
        (Some(2), &json!("usage"))
    );
    let out = ap(
        &["unit", "review", "--store", "/s"],
        r#"{"request_id":"r","agent":"a","session":"s","unit":"u1","colour":1}"#,
    );
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    let members: BTreeSet<&str> = e["error"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        members,
        BTreeSet::from([
            "code", "field", "detail", "after", "unit", "intent", "reviews"
        ])
    );
    assert_eq!(
        (out.status.code(), &e["error"]["field"], &e["error"]["unit"]),
        (Some(2), &json!("colour"), &json!("u1"))
    );
    let help = String::from_utf8(
        Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(["unit", "--help"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert!(
        help.contains("ap unit review [--store <path>] < request.json"),
        "{help}"
    );
}

// spec: specs/ap/unit-review.md#R2
#[test]
fn r2_the_subject_and_every_key_are_checked_before_any_call() {
    let mut f = F::new();
    let mut both = f.req("unit", "u1");
    both["intent"] = json!("widgets");
    let none = json!({"request_id": "r", "agent": "lead", "session": "lead-1"});
    let mut cases = vec![(both, "intent"), (none, "intent")];
    for bad in ["..", "a/b", "/etc", "a.b", "", &"u".repeat(81)] {
        cases.push((f.req("unit", bad), "unit"));
        cases.push((f.req("intent", bad), "intent"));
        for member in ["resume", "abandon", "finish"] {
            let mut r = f.req("unit", "u1");
            r[member] = json!(bad);
            cases.push((r, member));
        }
        let mut r = f.req("unit", "u1");
        r["earlier"] = json!(["fine-key", bad]);
        cases.push((r, "earlier"));
    }
    let mut r = f.req("unit", "u1");
    (r["abandon"], r["finish"]) = (json!("a"), json!("b"));
    cases.push((r, "finish"));
    let mut r = f.req("unit", "u1");
    (r["abandon"], r["task"]) = (json!("a"), json!("invented-task"));
    cases.push((r, "task"));
    for (r, field) in cases {
        let e = f.refusal(&r);
        assert_eq!(code(&e), ("invalid_input".into(), field.into()), "{r}");
    }
    assert!(f.calls.is_empty(), "{:?}", f.calls);
    assert!(!f.dir().exists());
}

// spec: specs/ap/unit-review.md#R3
#[test]
fn r3_the_task_is_the_request_s_or_the_policy_s() {
    let mut f = F::new();
    f.write_policy(json!({"task": null}));
    assert_eq!(
        code(&f.refusal(&f.req("unit", "u1"))),
        ("invalid_input".into(), "task".into())
    );
    assert!(f.calls.is_empty() && !f.dir().exists());
    let mut r = f.req("unit", "u1");
    r["task"] = json!("missing-task");
    let e = f.refusal(&r);
    assert_eq!(
        (code(&e), e.after),
        (("unknown_task".into(), "start".into()), None)
    );
    assert!(f.actions().is_empty() && f.reviews_of("unit", "u1").is_empty());
    let mut r = f.req("unit", "u1");
    r["task"] = json!("invented-task");
    let (v, _) = f.ok(&r);
    assert_eq!(
        f.get("review", v["review"].as_str().unwrap())["task"],
        oid("task", "invented-task")
    );
    f.write_policy(json!({}));
    f.put("task", "other-task", json!({}));
    f.write_policy(json!({"task": "other-task"}));
    let (v, _) = f.ok(&f.req("unit", "u1"));
    assert_eq!(
        f.get("review", v["review"].as_str().unwrap())["task"],
        oid("task", "other-task")
    );
}

// spec: specs/ap/unit-review.md#R4
#[test]
fn r4_the_reviewer_s_members_come_from_the_request_or_the_policy() {
    let mut f = F::new();
    let mut r = f.req("unit", "u1");
    r["reviewer"] = json!({"model": "opus", "effort": "xhigh"});
    let (v, _) = f.ok(&r);
    let review = f.get("review", v["review"].as_str().unwrap());
    assert_eq!(
        (&review["model"], &review["effort"]),
        (&json!("opus"), &json!("xhigh"))
    );
    let mut r = f.req("unit", "u1");
    r["reviewer"] = json!({"colour": "blue"});
    assert_eq!(
        code(&f.refusal(&r)),
        ("invalid_input".into(), "reviewer.colour".into())
    );
    for (policy, member) in [
        (json!({"colour": "blue"}), "reviews.colour"),
        (json!({"compact_window": "large"}), "reviews.compact_window"),
        (json!({"account": "relative"}), "reviews.account"),
        (json!({"models": []}), "reviews.models"),
        (json!({"directory": "/a b"}), "reviews.directory"),
    ] {
        f.write_policy(policy);
        let e = f.refusal(&f.req("unit", "u1"));
        assert_eq!(code(&e), ("policy_invalid".into(), "policy".into()));
        assert!(
            e.detail.as_str().unwrap().starts_with(member),
            "{:?}",
            e.detail
        );
    }
    let file = f.t.join("policy.json");
    std::fs::write(
        &file,
        r#"{"models":{"allowed":["sonnet"],"never":[]},"accounts":{},"reviews":"x"}"#,
    )
    .unwrap();
    let e = f.refusal(&f.req("unit", "u1"));
    assert!(
        e.detail.as_str().unwrap().starts_with("reviews "),
        "{:?}",
        e.detail
    );
    f.write_policy(json!({"model": null}));
    assert_eq!(
        code(&f.refusal(&f.req("unit", "u1"))),
        ("invalid_input".into(), "reviewer".into())
    );
    f.write_policy(json!({"models": ["opus"]}));
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        code(&e),
        ("reviewer_model_not_allowed".into(), "reviewer.model".into())
    );
    let mut r = f.req("unit", "u1");
    r["reviewer"] = json!({"model": "opus"});
    f.ok(&r);
    // The policy of `ap run` still applies to the stage: refused at admission, nothing applied.
    f.write_policy(json!({}));
    let mut r = f.req("unit", "u1");
    r["reviewer"] = json!({"model": "haiku"});
    let before = f.actions().len();
    let e = f.refusal(&r);
    assert_eq!(
        (code(&e), e.after),
        (("model_not_allowed".into(), "reviewer.model".into()), None)
    );
    assert_eq!(f.actions().len(), before);
    // Without a policy file the host has said nothing; refused before the default directory is made.
    f.policy = None;
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(code(&e), ("invalid_input".into(), "task".into()));
    let mut r = f.req("unit", "u1");
    r["task"] = json!("invented-task");
    let e = f.refusal(&r);
    assert_eq!(code(&e), ("invalid_input".into(), "reviewer".into()));
}

// spec: specs/ap/unit-review.md#R5
#[test]
fn r5_the_prompt_file_and_the_directories() {
    let mut f = F::new();
    let digest = page_digest("u1", &f.get("unit", "u1"));
    let dir = f.dir();
    let (v, _) = f.ok(&f.req("unit", "u1"));
    assert_eq!(v["round"], 1);
    let first = dir.join(format!("prompts/page-u1-{}-1.md", &digest[..16]));
    assert_eq!(mode(&first), 0o600);
    for d in [dir.clone(), dir.join("prompts"), dir.join("work")] {
        assert_eq!(mode(&d), 0o700, "{d:?}");
    }
    let review = f.get("review", v["review"].as_str().unwrap());
    assert_eq!(review["prompt"], json!(first));
    assert_eq!(
        review["prompt_sha256"],
        json!(sha(&std::fs::read(&first).unwrap()))
    );
    assert_eq!(review["round"], 1);
    // A link at the path of the next prompt: replaced, the file it names untouched.
    let outside = f.t.join("outside.md");
    std::fs::write(&outside, "invented, not the verb's").unwrap();
    let second = dir.join(format!("prompts/page-u1-{}-2.md", &digest[..16]));
    symlink(&outside, &second).unwrap();
    let (v, _) = f.ok(&f.req("unit", "u1"));
    assert_eq!(v["round"], 2);
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "invented, not the verb's"
    );
    assert!(std::fs::symlink_metadata(&second).unwrap().is_file());
    assert_eq!(mode(&second), 0o600);
    // A call refused before its start removes the prompt it wrote.
    let mut r = f.req("unit", "u1");
    r["task"] = json!("missing-task");
    f.refusal(&r);
    assert!(
        !dir.join(format!("prompts/page-u1-{}-3.md", &digest[..16]))
            .exists()
    );
    let mut names: Vec<String> = std::fs::read_dir(dir.join("prompts"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "{names:?}");
    f.write_policy(json!({"directory": "relative/reviews"}));
    assert_eq!(code(&f.refusal(&f.req("unit", "u1"))).0, "policy_invalid");
}

// spec: specs/ap/unit-review.md#R6
#[test]
fn r6_a_page_is_the_reviewer_s_page_and_its_digest_from_one_read() {
    let mut f = F::new();
    // A commit on the origin the clone has not fetched: the review fetches first.
    std::fs::write(f.t.join("origin/AGENTS.md"), "# Invented widgets, second\n").unwrap();
    git(&f.t.join("origin"), &["commit", "--quiet", "-am", "second"]);
    let commit = git(&f.t.join("origin"), &["rev-parse", "HEAD"]);
    let (v, status) = f.ok(&f.req("unit", "u1"));
    assert_eq!((status, &v["verdict"]), (0, &json!("accept")), "{v}");
    let digest = page_digest("u1", &f.get("unit", "u1"));
    assert_eq!(v["digest"], json!(digest));
    let prompt = std::fs::read_to_string(
        f.get("review", v["review"].as_str().unwrap())["prompt"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let text = page(&mut f, Some(STORE), "u1", Some(Role::Reviewer), false).unwrap();
    assert_eq!(prompt, format!("{text}\nDigest: {digest}\n"));
    assert!(prompt.contains(&format!("## The repository at {commit}\n")));
    // ONE read of the unit before the validate.
    let validate = f
        .calls
        .iter()
        .position(|c| c.1.get(1).is_some_and(|w| w == "validate"))
        .unwrap();
    let reads = f.calls[..validate]
        .iter()
        .filter(|c| c.1[0] == "get" && c.2["id"]["object_type"] == "unit");
    assert_eq!(reads.count(), 1);
    git(
        &f.t.join("clone"),
        &["remote", "set-url", "origin", "/invented/nowhere"],
    );
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        (code(&e), e.after),
        (("fetch_failed".into(), "clone".into()), None)
    );
    let e = f.refusal(&f.req("unit", "no-such-unit"));
    assert_eq!(code(&e), ("unknown_unit".into(), "unit".into()));
}

/// The intent `widgets` with two criteria and seven units: `a`, `b` after `a`, `c` and `d` on a cycle,
/// `e` dropped, `f` of another intent, and `g` of the lane `small` that names the intent.
fn plan_fixture(f: &mut F) {
    f.put(
        "intent",
        "other",
        json!({"status": "confirmed", "outcome": "x", "first_user": "y"}),
    );
    for (key, statement, check, added) in [
        (
            "c2",
            "Each widget appears once.",
            "widgets list | sort | uniq -d is empty",
            "2026-11-18T00:00:02Z",
        ),
        (
            "c1",
            "The list names every widget.",
            "widgets list | wc -l equals the count",
            "2026-11-18T00:00:01Z",
        ),
    ] {
        f.put(
            "criterion",
            key,
            json!({"intent": oid("intent", "widgets"), "statement": statement,
            "check": check, "state": "open", "added_at": added}),
        );
    }
    for key in ["a", "b", "c", "d", "e", "f"] {
        f.unit(key, "plain", key != "c");
    }
    f.modify(
        "unit",
        "b",
        json!({"depends_on": [oid("unit", "a")], "specs": ["specs/widgets.md#R1"]}),
    );
    f.modify("unit", "c", json!({"depends_on": [oid("unit", "d")]}));
    f.modify(
        "unit",
        "d",
        json!({"depends_on": [oid("unit", "c"), oid("unit", "a")], "estimate_lines": null}),
    );
    f.modify("unit", "e", json!({"state": "dropped"}));
    f.modify("unit", "f", json!({"intent": oid("intent", "other")}));
    f.unit("g", "plain", true);
    f.modify(
        "unit",
        "g",
        json!({"lane": "small", "origin": oid("request", "q-small"), "estimate_lines": 12}),
    );
    f.objects.remove(&k("unit", "u1"));
}

// spec: specs/ap/unit-review.md#R7
#[test]
fn r7_the_plan_part_is_a_golden_text_and_its_digest_ignores_every_state() {
    let mut f = F::new();
    plan_fixture(&mut f);
    let (v, status) = f.ok(&f.req("intent", "widgets"));
    assert_eq!(
        (status, &v["kind"], &v["intent"]),
        (0, &json!("plan"), &json!("widgets")),
        "{v}"
    );
    let review = f.get("review", v["review"].as_str().unwrap());
    let prompt = std::fs::read_to_string(review["prompt"].as_str().unwrap()).unwrap();
    let (part, rest) = prompt.split_once("\n## Repositories\n").unwrap();
    assert_eq!(part, PLAN_PART);
    assert_eq!(v["digest"], json!(sha(part.as_bytes())));
    assert_eq!(
        (&review["head"], &review["intent"]),
        (&v["digest"], &oid("intent", "widgets"))
    );
    let commit = git(&f.t.join("clone"), &["rev-parse", "origin/main"]);
    let repositories = format!(
        "\n- invented/widgets: clone {}, origin/main at {commit}\n",
        f.p("clone")
    );
    assert!(rest.starts_with(&repositories), "{rest}");
    let states = "## States now\n\n- unit a: draft\n- unit b: draft\n- unit g: draft\n- unit c: draft\n- unit d: draft\n- criterion c1: open\n- criterion c2: open\n";
    assert!(rest.contains(states), "{rest}");
    assert!(rest.ends_with(&format!(
        "{PLAN_REVIEWER}\nDigest: {}\n",
        v["digest"].as_str().unwrap()
    )));
    // A state moves, the plan does not: the same digest, the states seen.
    f.modify("unit", "a", json!({"state": "running"}));
    f.modify("criterion", "c1", json!({"state": "met"}));
    let (again, _) = f.ok(&f.req("intent", "widgets"));
    assert_eq!(
        (&again["digest"], &again["round"]),
        (&v["digest"], &json!(2))
    );
    let prompt = std::fs::read_to_string(
        f.get("review", again["review"].as_str().unwrap())["prompt"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(
        prompt.starts_with(PLAN_PART)
            && prompt.contains("- unit a: running\n")
            && prompt.contains("- criterion c1: met\n")
    );
    // A plan's reviews come from a search by kind: both, and none of another intent's.
    assert_eq!(again["reviews"].as_array().unwrap().len(), 2);
    let e = f.refusal(&f.req("intent", "no-such-intent"));
    assert_eq!(code(&e), ("unknown_intent".into(), "intent".into()));
}

/// A first look of `u1` that answers `verdict`: the answer.
fn first_look(f: &mut F, verdict: &str) -> Value {
    f.answer("verdict", Some(verdict));
    let (v, _) = f.ok(&f.req("unit", "u1"));
    f.answer("verdict", None);
    v
}
fn resume(f: &F, key: &Value) -> Value {
    let mut r = f.req("unit", "u1");
    r["resume"] = key.clone();
    r
}

// spec: specs/ap/unit-review.md#R8
#[test]
fn r8_a_repair_look_resumes_the_session_in_the_same_directory_three_times_at_most() {
    let mut f = F::new();
    let first = first_look(&mut f, "repair");
    assert_eq!(first["verdict"], "repair");
    let first_key = first["review"].as_str().unwrap().to_owned();
    let dir = f.dir().join("work").join(&first_key);
    assert_eq!(f.seen("cwd").trim(), dir.to_str().unwrap());
    f.tick(5);
    f.amend("u1");
    f.tick(5);
    let mut r = resume(&f, &first["review"]);
    r["reviewer"] = json!({"next_accounts": [f.p("a2")]});
    assert_eq!(
        code(&f.refusal(&r)),
        ("invalid_input".into(), "reviewer.next_accounts".into())
    );
    let (second, status) = f.ok(&resume(&f, &first["review"]));
    assert_eq!(
        (status, &second["verdict"], &second["session"]),
        (0, &json!("accept"), &first["session"]),
        "{second}"
    );
    assert_eq!(f.seen("cwd").trim(), dir.to_str().unwrap());
    assert!(
        f.seen("ls").contains("AGENTS.md"),
        "the copy was made again"
    );
    assert!(!dir.exists());
    let prompt = std::fs::read_to_string(
        f.get("review", second["review"].as_str().unwrap())["prompt"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(prompt.contains(&format!("\n{REPAIR_LOOK}\nDigest: ")));
    let rec = record(second["report"].as_str().unwrap());
    assert_eq!(
        (&rec["cwd"], &rec["resumes"]),
        (
            &json!(dir),
            &json!(format!("{}.run.json", first["report"].as_str().unwrap()))
        )
    );
    let (third, _) = f.ok(&resume(&f, &second["review"]));
    assert_eq!(third["session"], first["session"]);
    // A fourth look of the session.
    let e = f.refusal(&resume(&f, &third["review"]));
    assert_eq!(
        (code(&e), e.after),
        (("resume_limit".into(), "resume".into()), None)
    );
    let e = f.refusal(&resume(&f, &json!("no-such-review")));
    assert_eq!(code(&e), ("resume_not_possible".into(), "resume".into()));
}

// spec: specs/ap/unit-review.md#R8
#[test]
fn r8_the_resumed_record_s_directory_and_the_running_reviews_are_checked() {
    let mut f = F::new();
    let first = first_look(&mut f, "repair");
    let report = first["report"].as_str().unwrap().to_owned();
    let path = format!("{report}.run.json");
    let kept = std::fs::read(&path).unwrap();
    let other = f.t.join("another-directory");
    std::fs::create_dir(&other).unwrap();
    let work = f.dir().join("work");
    for cwd in [
        other.to_string_lossy().into_owned(),
        format!("{}/.", work.display()),
        format!("{}/", work.display()),
    ] {
        let mut rec = record(&report);
        rec["cwd"] = json!(cwd);
        std::fs::write(&path, rec.to_string()).unwrap();
        let e = f.refusal(&resume(&f, &first["review"]));
        assert_eq!(
            code(&e),
            ("resume_not_possible".into(), "resume".into()),
            "{cwd}"
        );
        assert!(other.is_dir() && work.is_dir());
    }
    std::fs::write(&path, &kept).unwrap();
    // A leftover at the path is removed and the copy made again.
    let leftover = work.join(first["review"].as_str().unwrap());
    std::fs::create_dir(&leftover).unwrap();
    std::fs::write(leftover.join("leftover.txt"), "invented").unwrap();
    // While a review of this subject runs, no look resumes.
    f.die_after = Some("close_run");
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(e.after, Some("start"));
    f.die_after = None;
    f.dead = false;
    let running = f
        .reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0;
    let e = f.refusal(&resume(&f, &first["review"]));
    assert_eq!(code(&e), ("resume_not_possible".into(), "resume".into()));
    assert!(
        e.detail.as_str().unwrap().contains(&running),
        "{:?}",
        e.detail
    );
    assert!(
        leftover.join("leftover.txt").exists(),
        "nothing removed before the checks pass"
    );
    let mut r = f.req("unit", "u1");
    r["finish"] = json!(running);
    f.ok(&r);
    let (v, _) = f.ok(&resume(&f, &first["review"]));
    assert_eq!(v["verdict"], "accept");
    assert!(!f.seen("ls").contains("leftover.txt") && f.seen("ls").contains("AGENTS.md"));
    assert!(!leftover.exists());
}

// spec: specs/ap/unit-review.md#R9
#[test]
fn r9_a_new_session_is_given_the_earlier_reports_as_claims() {
    let mut f = F::new();
    let first = first_look(&mut f, "repair");
    let mut r = f.req("unit", "u1");
    r["earlier"] = json!([first["review"]]);
    let (v, _) = f.ok(&r);
    assert_ne!(v["session"], first["session"]);
    let prompt = std::fs::read_to_string(
        f.get("review", v["review"].as_str().unwrap())["prompt"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let section = format!(
        "\n## Earlier reports (claims of earlier reviewers; check them, do not trust them)\n\n- {}: {}\n\nDigest: ",
        first["review"].as_str().unwrap(),
        first["report"].as_str().unwrap()
    );
    assert!(prompt.contains(&section), "{prompt}");
    assert!(!prompt.contains(REPAIR_LOOK));
    let mut r = f.req("unit", "u1");
    r["earlier"] = json!(["no-such-review"]);
    let e = f.refusal(&r);
    assert_eq!(
        (code(&e), &e.detail),
        (
            ("earlier_not_possible".into(), "earlier".into()),
            &json!("no-such-review")
        )
    );
    let mut r = resume(&f, &first["review"]);
    r["earlier"] = json!([first["review"]]);
    assert_eq!(
        code(&f.refusal(&r)),
        ("invalid_input".into(), "earlier".into())
    );
    let mut r = f.req("unit", "u1");
    r["earlier"] = json!(["a", "b", "c", "d", "e", "f"]);
    assert_eq!(
        code(&f.refusal(&r)),
        ("invalid_input".into(), "earlier".into())
    );
}

// spec: specs/ap/unit-review.md#R10
#[test]
fn r10_the_start_is_validated_and_the_stage_admitted_before_anything_is_written() {
    let mut f = F::new();
    let (v, _) = f.ok(&f.req("unit", "u1"));
    let validates = f.bodies("validate", "start_unit_review");
    let applies = f.bodies("apply", "start_unit_review");
    assert_eq!((validates.len(), applies.len()), (1, 1));
    assert_eq!(
        validates[0], applies[0],
        "the same request, client, agent and session"
    );
    let order: Vec<String> = f
        .calls
        .iter()
        .filter(|c| c.0 == "ont" && c.1[0] == "action")
        .map(|c| format!("{} {}", c.1[1], c.2["action_type"].as_str().unwrap()))
        .collect();
    assert_eq!(
        order[..2],
        ["validate start_unit_review", "apply start_unit_review"]
    );
    assert!(
        f.calls
            .iter()
            .filter(|c| c.1[0] == "action")
            .all(|c| c.1[2..4] == ["--client", "ap"])
    );
    assert_eq!(
        v["review"],
        json!(made_key("lead", applies[0]["request_id"].as_str().unwrap()))
    );
    // A refusal of admission: nothing applied, the prompt removed.
    f.write_policy(json!({"account": f.p("bin")}));
    let before = f.actions().len();
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        (code(&e), e.after),
        (
            ("account_not_allowed".into(), "reviewer.account".into()),
            None
        )
    );
    assert_eq!(f.actions().len(), before);
    assert_eq!(
        std::fs::read_dir(f.dir().join("prompts")).unwrap().count(),
        1
    );
    f.write_policy(json!({}));
    // A key of the apply that is not the validated one.
    f.other_key = true;
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        (code(&e), e.after),
        (
            ("store_answer_unreadable".into(), "start".into()),
            Some("start")
        )
    );
    assert_eq!(f.launches, 1, "no stage for an unreadable start");
}

// spec: specs/ap/unit-review.md#R11
#[test]
fn r11_the_start_names_the_page_it_read_and_makes_no_pause() {
    let mut f = F::new();
    let unit = f.get("unit", "u1");
    // The read in the second of the amendment: no pause before the apply, as an accept counts by
    // its digest and not by when it started.
    f.ok(&f.req("unit", "u1"));
    // A pause of the verb's own would be one second; the stage's polling pauses are shorter.
    let seconds = |f: &F| {
        f.pauses
            .iter()
            .filter(|d| **d == Duration::from_secs(1))
            .count()
    };
    assert_eq!(seconds(&f), 0);
    let body = &f.bodies("apply", "start_unit_review")[0];
    let p = &body["parameters"];
    assert_eq!(
        body["expected"],
        json!([{"id": oid("unit", "u1"), "revision": "r#1"}])
    );
    assert_eq!(
        body["request_id"],
        json!(format!(
            "{}-start",
            body["request_id"]
                .as_str()
                .unwrap()
                .trim_end_matches("-start")
        ))
    );
    assert!(body.get("role").is_none());
    let keys: BTreeSet<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "task",
            "kind",
            "head",
            "model",
            "effort",
            "prompt",
            "prompt_sha256",
            "unit",
            "round",
            "repository"
        ])
    );
    assert_eq!(
        (&p["kind"], &p["head"], &p["repository"], &p["round"]),
        (
            &json!("page"),
            &json!(page_digest("u1", &unit)),
            &json!("invented/widgets"),
            &json!(1)
        )
    );
    f.tick(3);
    f.ok(&f.req("unit", "u1"));
    assert_eq!(seconds(&f), 0, "nor a read later than the amendment");
    // Amended between the read and the start: nothing started.
    f.amend_at_validate = Some("u1");
    let n = f.reviews_of("unit", "u1").len();
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        (code(&e), e.after),
        (("revision_conflict".into(), "start".into()), None)
    );
    assert_eq!(f.reviews_of("unit", "u1").len(), n);
    // A running review of the same page.
    f.die_after = Some("close_run");
    f.refusal(&f.req("unit", "u1"));
    (f.dead, f.die_after) = (false, None);
    let running = f
        .reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0;
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        (code(&e), e.after),
        (("duplicate_running_review".into(), "start".into()), None)
    );
    assert_eq!(e.detail["running"], json!(running));
    let next = e.detail["next"].as_str().unwrap();
    assert!(next.contains("abandon") && next.contains("finish"));
    assert!(e.reviews.as_array().is_some_and(|l| l.len() == n + 1));
    // A request id used before, with other content.
    let mut r = f.req("unit", "u1");
    r["abandon"] = json!(running);
    r["discard"] = json!(true);
    f.ok(&r);
    let reused = f.req("unit", "u1");
    f.ok(&reused);
    let e = f.refusal(&reused);
    assert_eq!(
        (code(&e), e.after),
        (("request_id_reused".into(), "start".into()), None)
    );
}

// spec: specs/ap/unit-review.md#R12
#[test]
fn r12_the_working_directory_is_a_throwaway_copy_removed_after_the_stage() {
    let mut f = F::new();
    let clone = f.t.join("clone");
    let shown = |c: &Path| {
        (
            git(c, &["worktree", "list", "--porcelain"]),
            git(c, &["branch", "--list", "--all"]),
            git(c, &["status", "--porcelain"]),
        )
    };
    let before = shown(&clone);
    f.answer("touch", Some(""));
    let (v, _) = f.ok(&f.req("unit", "u1"));
    let key = v["review"].as_str().unwrap();
    assert_eq!(
        f.seen("cwd").trim(),
        f.dir().join("work").join(key).to_str().unwrap()
    );
    assert_eq!(f.seen("ls"), ".git\nAGENTS.md\n");
    assert_eq!(f.work(), Vec::<String>::new());
    assert_eq!(shown(&clone), before, "nothing added to the clone");
    f.answer("touch", None);
    // Without a repository: an empty directory.
    f.unit("u2", "contract", false);
    let (v, _) = f.ok(&f.req("unit", "u2"));
    assert_eq!(f.seen("ls"), "");
    assert_eq!(
        record(v["report"].as_str().unwrap())["cwd"],
        json!(f.dir().join("work").join(v["review"].as_str().unwrap()))
    );
    assert!(f.work().is_empty());
}

// spec: specs/ap/unit-review.md#R13
#[test]
fn r13_the_stage_is_ap_run_s_and_a_stage_refused_at_its_start_abandons() {
    let mut f = F::new();
    let mut r = f.req("unit", "u1");
    r["timeout_seconds"] = json!(600);
    let (v, _) = f.ok(&r);
    let key = v["review"].as_str().unwrap();
    let out = f.dir().join(format!("{key}.md"));
    assert_eq!(v["report"], json!(out));
    let rec = record(out.to_str().unwrap());
    assert_eq!(
        (&rec["role"], &rec["model"], &rec["effort"], &rec["account"]),
        (
            &json!("reviewer"),
            &json!("sonnet"),
            &json!("high"),
            &json!(f.p("a1"))
        )
    );
    assert_eq!(rec["prompt"]["path"], f.get("review", key)["prompt"]);
    assert_eq!(rec["run_object"]["start"], "applied");
    // The stage's `expect`: the verdict line, and the digest within the second line
    // (`second_line`): a second line without the digest is asked again in the same session, and
    // the ask that names it closes the review.
    let digest = page_digest("u1", &f.get("unit", "u1"));
    f.answer("digest-once", Some(&"0".repeat(64)));
    let before = f.launches;
    let (asked, status) = f.ok(&f.req("unit", "u1"));
    f.answer("digest-once", None);
    assert_eq!(
        (status, &asked["verdict"], f.launches - before),
        (0, &json!("accept"), 2)
    );
    assert_eq!(
        f.seen("prompt"),
        "Your last answer did not have as its second line the line your task asks for: give the whole answer again with the same first line, that line as its second line and nothing before them, and post, push, send or change nothing a second time.\n"
    );
    let asked_rec = record(asked["report"].as_str().unwrap());
    assert_eq!(
        (&asked_rec["state"], &asked_rec["attempts"][1]["asked"]),
        (&json!("ok"), &json!(true))
    );
    let report = std::fs::read_to_string(asked["report"].as_str().unwrap()).unwrap();
    assert_eq!(
        report.lines().nth(1),
        Some(format!("Digest: {digest}").as_str())
    );
    // A plan's stage has the same `expect`: missed twice, the review closes `abandoned`.
    let mut g = F::new();
    plan_fixture(&mut g);
    g.answer("digest", Some(&"0".repeat(64)));
    let (plan, status) = g.ok(&g.req("intent", "widgets"));
    assert_eq!(
        (status, &plan["verdict"], &plan["cause"], g.launches),
        (3, &json!("abandoned"), &json!("no_result"), 2)
    );
    let plan_rec = record(
        g.dir()
            .join(format!("{}.md", plan["review"].as_str().unwrap()))
            .to_str()
            .unwrap(),
    );
    assert_eq!(plan_rec["unmet"], "second_line");
    let run = f.get("run", rec["run_object"]["key"].as_str().unwrap());
    assert_eq!(
        (&run["unit"], &run["status"]),
        (&oid("unit", "u1"), &json!("closed"))
    );
    // A stage refused although it was admitted.
    f.block_out = true;
    let (v, status) = f.ok(&f.req("unit", "u1"));
    assert_eq!(
        (status, &v["verdict"], &v["cause"]),
        (3, &json!("abandoned"), &json!("stage_refused")),
        "{v}"
    );
    assert!(f.work().is_empty());
}

// spec: specs/ap/unit-review.md#R14
#[test]
fn r14_only_a_report_of_the_form_naming_the_digest_closes_accept_or_repair() {
    let mut f = F::new();
    let digest = page_digest("u1", &f.get("unit", "u1"));
    let (v, status) = f.ok(&f.req("unit", "u1"));
    assert_eq!((status, &v["verdict"]), (0, &json!("accept")));
    let close = f.bodies("apply", "close_unit_review").pop().unwrap();
    let p = &close["parameters"];
    let report = std::fs::read(v["report"].as_str().unwrap()).unwrap();
    let rec = record(v["report"].as_str().unwrap());
    assert_eq!(
        (&p["verdict"], &p["report"], &p["report_sha256"]),
        (&json!("accept"), &v["report"], &json!(sha(&report)))
    );
    assert_eq!(
        (&p["session"], &p["account"], &p["unit"]),
        (&rec["session"], &json!("a1"), &oid("unit", "u1"))
    );
    assert!(close["request_id"].as_str().unwrap().ends_with("-close"));
    // The second line the stage's `expect.second_line` (`*<digest>*`) misses is asked again and,
    // missed again, ends `no_result`; one it matches but the close refuses is `report_form`, with no
    // ask (R13).
    let cases: [(&str, Option<&str>, &str, usize); 7] = [
        ("verdict", Some("repair"), "repair", 1),
        ("none", Some(""), "no_result", 2),
        ("digest", Some(&"0".repeat(64)), "no_result", 2),
        (
            "digest",
            Some(&format!("{digest} {digest}")),
            "report_form",
            1,
        ),
        ("digest", Some(&format!("{digest}a")), "report_form", 1),
        ("digest", Some(&digest[..63]), "no_result", 2),
        ("verdict", Some("accepted"), "report_form", 1),
    ];
    for (file, text, want, launches) in cases {
        f.answer(file, text);
        let before = f.launches;
        let (v, status) = f.ok(&f.req("unit", "u1"));
        f.answer(file, None);
        assert_eq!(f.launches - before, launches, "{file} {text:?}");
        match want {
            "repair" => assert_eq!(
                (status, &v["verdict"], &v["report"]),
                (0, &json!("repair"), &v["report"])
            ),
            cause => {
                assert_eq!(
                    (status, &v["verdict"], &v["cause"], &v["report"]),
                    (3, &json!("abandoned"), &json!(cause), &Value::Null),
                    "{file} {text:?}"
                );
                let p = &f.bodies("apply", "close_unit_review").pop().unwrap()["parameters"];
                assert!(
                    p.get("report").is_none()
                        && p["unit"] == oid("unit", "u1")
                        && p["session"].is_string()
                );
            }
        }
    }
    // A stage that fails, and one cut by its bound: the record's state is the cause.
    for (mode, timeout) in [("noout", 600), ("other", 2)] {
        f.mode("a1", mode);
        let mut r = f.req("unit", "u1");
        r["timeout_seconds"] = json!(timeout);
        let (v, status) = f.ok(&r);
        let key = v["review"].as_str().unwrap();
        let state = record(f.dir().join(format!("{key}.md")).to_str().unwrap())["state"].clone();
        assert_ne!(state, "ok");
        assert_eq!(
            (status, &v["verdict"], &v["cause"]),
            (3, &json!("abandoned"), &state),
            "{mode}"
        );
    }
    f.mode("a1", "script");
    // The report replaced between the stage's end and the close.
    f.replace_report = true;
    let (v, _) = f.ok(&f.req("unit", "u1"));
    assert_eq!(
        (&v["verdict"], &v["cause"]),
        (&json!("abandoned"), &json!("report_changed"))
    );
    let accepted = f
        .reviews_of("unit", "u1")
        .iter()
        .filter(|(_, r)| r["verdict"] == "accept")
        .count();
    assert_eq!(accepted, 1);
}

// spec: specs/ap/unit-review.md#R14
#[test]
fn r14_a_session_that_wrote_to_the_unit_does_not_close_its_review() {
    let mut f = F::new();
    f.stage_writes = true;
    let (v, status) = f.ok(&f.req("unit", "u1"));
    assert_eq!(
        (status, &v["verdict"], &v["cause"], &v["session"]),
        (
            3,
            &json!("abandoned"),
            &json!("writer_cannot_review"),
            &json!("lead-1")
        ),
        "{v}"
    );
    let closes = f.bodies("apply", "close_unit_review");
    assert_eq!(closes.len(), 2);
    assert_eq!(closes[0]["request_id"], closes[1]["request_id"]);
    f.modify("unit", "u1", json!({"writers": ["lead-1"]}));
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(
        (code(&e), e.after),
        (
            ("writer_cannot_review".into(), "session".into()),
            Some("start")
        )
    );
    assert!(
        e.detail["next"]
            .as_str()
            .unwrap()
            .contains("another session must abandon")
    );
    let running = f
        .reviews_of("unit", "u1")
        .into_iter()
        .filter(|(_, r)| r["status"] == "running")
        .count();
    assert_eq!(running, 1);
}

/// A call of `u1` cut after the start and before its stage: the review is left `running` with an
/// empty-handed record; the answer is the review's key.
fn cut_after_start(f: &mut F) -> String {
    f.die_after = Some("start_unit_review");
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(e.after, Some("start"));
    (f.dead, f.die_after) = (false, None);
    f.reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0
}
fn ending(f: &F, which: &str, key: &str) -> Value {
    let mut r = f.req("unit", "u1");
    r[which] = json!(key);
    r
}

// spec: specs/ap/unit-review.md#R15
#[test]
fn r15_abandon_and_finish_read_the_review_first() {
    let mut f = F::new();
    let closed = first_look(&mut f, "accept")["review"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::create_dir_all(f.dir().join("work").join(&closed)).unwrap();
    f.unit("u2", "contract", false);
    f.die_after = Some("start_unit_review");
    f.refusal(&f.req("unit", "u2"));
    (f.dead, f.die_after) = (false, None);
    let others = f.reviews_of("unit", "u2")[0].0.clone();
    std::fs::create_dir_all(f.dir().join("work").join(&others)).unwrap();
    let from = f.calls.len();
    for which in ["abandon", "finish"] {
        for key in [closed.as_str(), others.as_str(), "no-such-review"] {
            let calls = f.calls.len();
            let e = f.refusal(&ending(&f, which, key));
            assert_eq!(
                code(&e),
                ("review_not_running".into(), which.into()),
                "{which} {key}"
            );
            assert!(
                f.calls[calls..]
                    .iter()
                    .all(|c| c.0 == "ont" && c.1[0] == "get"),
                "only the read"
            );
        }
    }
    let mut r = ending(&f, "finish", "x");
    r["discard"] = json!(true);
    assert_eq!(
        code(&f.refusal(&r)),
        ("invalid_input".into(), "discard".into())
    );
    let mut kept = vec![closed.clone(), others.clone()];
    kept.sort();
    assert_eq!(f.work(), kept);
    assert!(!f.calls[from..].iter().any(|c| c.0 == "systemctl"));
    // `finish` needs the report and a record of state `ok`; `abandon` then needs `discard`.
    let running = cut_after_start(&mut f);
    let e = f.refusal(&ending(&f, "finish", &running));
    assert_eq!(
        (code(&e), &e.detail),
        (
            ("finish_not_possible".into(), "finish".into()),
            &json!("no report")
        )
    );
    let out = f.dir().join(format!("{running}.md"));
    let rec = record(out.to_str().unwrap());
    std::fs::write(
        &out,
        format!(
            "Verdict: accept\n{}\n",
            page_digest("u1", &f.get("unit", "u1"))
        ),
    )
    .unwrap();
    let e = f.refusal(&ending(&f, "finish", &running));
    assert_eq!(e.detail, json!("the run record's state is not ok"));
    let mut ok = rec.clone();
    ok["state"] = json!("ok");
    std::fs::write(format!("{}.run.json", out.display()), ok.to_string()).unwrap();
    let e = f.refusal(&ending(&f, "abandon", &running));
    assert_eq!(code(&e), ("report_exists".into(), "abandon".into()));
    let mut r = ending(&f, "abandon", &running);
    r["discard"] = json!(true);
    let (v, status) = f.ok(&r);
    assert_eq!(
        (status, &v["verdict"], &v["cause"]),
        (3, &json!("abandoned"), &json!("abandon"))
    );
}

// spec: specs/ap/unit-review.md#R16
#[test]
fn r16_only_a_stage_s_unit_is_stopped_and_finish_stops_nothing() {
    let mut f = F::new();
    let running = cut_after_start(&mut f);
    let out = f.dir().join(format!("{running}.md"));
    let path = format!("{}.run.json", out.display());
    let mut rec = record(out.to_str().unwrap());
    let stage_unit = rec["unit"].as_str().unwrap().to_owned();
    assert!(stage_unit.starts_with("ap-run-") && stage_unit.ends_with(".scope"));
    // A unit of the stage's form that does not end: nothing applied, nothing removed.
    let dir = f.dir().join("work").join(&running);
    std::fs::create_dir_all(&dir).unwrap();
    f.alive.insert(stage_unit.clone());
    f.stop_fails = true;
    let applied = f.actions().len();
    let e = f.refusal(&ending(&f, "abandon", &running));
    assert_eq!(
        (code(&e), &e.detail),
        (
            ("run_still_active".into(), "abandon".into()),
            &json!(stage_unit)
        )
    );
    assert_eq!(f.actions().len(), applied);
    assert!(dir.is_dir());
    // `finish` reads the unit and stops nothing.
    std::fs::write(&out, "Verdict: accept\nx\n").unwrap();
    rec["state"] = json!("ok");
    std::fs::write(&path, rec.to_string()).unwrap();
    let stops = |f: &F| {
        f.calls
            .iter()
            .filter(|c| c.0 == "systemctl" && c.1[0] == "stop")
            .count()
    };
    let before = stops(&f);
    let e = f.refusal(&ending(&f, "finish", &running));
    assert_eq!(code(&e).0, "run_still_active");
    assert_eq!(stops(&f), before);
    // Another unit's name: not stopped, named in the answer.
    rec["unit"] = json!("invented-database.service");
    rec["state"] = json!("running");
    std::fs::write(&path, rec.to_string()).unwrap();
    std::fs::remove_file(&out).unwrap();
    let calls = f.calls.len();
    let (v, _) = f.ok(&ending(&f, "abandon", &running));
    assert_eq!(v["unit_not_stopped"], "invented-database.service");
    assert!(!f.calls[calls..].iter().any(|c| c.0 == "systemctl"));
    // The stage's form: stopped and read as ended.
    f.stop_fails = false;
    let running = cut_after_start(&mut f);
    let calls = f.calls.len();
    let (v, _) = f.ok(&ending(&f, "abandon", &running));
    let unit = record(f.dir().join(format!("{running}.md")).to_str().unwrap())["unit"].clone();
    let systemctl: Vec<&Vec<String>> = f.calls[calls..]
        .iter()
        .filter(|c| c.0 == "systemctl")
        .map(|c| &c.1)
        .collect();
    assert_eq!(
        systemctl.iter().map(|a| a[0].as_str()).collect::<Vec<_>>(),
        ["stop", "is-active"]
    );
    assert_eq!(json!(systemctl[0][1]), unit);
    assert_eq!(v["unit_not_stopped"], Value::Null);
}

// spec: specs/ap/unit-review.md#R17
// spec: specs/ap/unit-review.md#R19
#[test]
fn r17_r19_a_call_cut_after_the_stage_is_finished_from_its_report() {
    let mut f = F::new();
    f.die_after = Some("close_run");
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(e.after, Some("start"));
    (f.dead, f.die_after) = (false, None);
    let (key, review) = f.reviews_of("unit", "u1").pop().unwrap();
    assert_eq!(review["status"], "running");
    let run = record(f.dir().join(format!("{key}.md")).to_str().unwrap())["run_object"]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(f.get("run", &run)["status"], "closed");
    let (v, status) = f.ok(&ending(&f, "finish", &key));
    assert_eq!(
        (status, &v["verdict"], &v["review"], &v["digest"]),
        (0, &json!("accept"), &json!(key), &review["head"]),
        "{v}"
    );
    assert_eq!(
        v["run_closed"], "request_id_reused",
        "the stage's own run closed its run"
    );
    let close = f.bodies("apply", "close_unit_review").pop().unwrap();
    assert!(close["request_id"].as_str().unwrap().ends_with("-close"));
    assert_eq!(
        close["parameters"]["report_sha256"],
        json!(sha(&std::fs::read(v["report"].as_str().unwrap()).unwrap()))
    );
    // A report whose second line the stage's pattern matches but that does not name the review's
    // own head as one run of 64 hexadecimal characters (here joined to a letter).
    f.die_after = Some("close_run");
    let joined = format!("{}z", page_digest("u1", &f.get("unit", "u1")));
    f.answer("digest", Some(&joined));
    f.refusal(&f.req("unit", "u1"));
    f.answer("digest", None);
    (f.dead, f.die_after) = (false, None);
    let key = f
        .reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0;
    let (v, status) = f.ok(&ending(&f, "finish", &key));
    assert_eq!(
        (status, &v["verdict"], &v["cause"]),
        (3, &json!("abandoned"), &json!("report_form"))
    );
    // A record whose run names another record: not closed.
    f.die_after = Some("start_run");
    f.refusal(&f.req("unit", "u1"));
    (f.dead, f.die_after) = (false, None);
    let key = f
        .reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0;
    let run = record(f.dir().join(format!("{key}.md")).to_str().unwrap())["run_object"]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    f.modify("run", &run, json!({"record": "/invented/other.run.json"}));
    let (v, _) = f.ok(&ending(&f, "abandon", &key));
    assert_eq!(
        (&v["run_closed"], &f.get("run", &run)["status"]),
        (&Value::Null, &json!("running"))
    );
}

// spec: specs/ap/unit-review.md#R17
// spec: specs/ap/unit-review.md#R18
#[test]
fn r17_r18_a_call_cut_after_the_start_is_abandoned_and_its_directory_removed() {
    let mut f = F::new();
    f.die_after = Some("start_run");
    let e = f.refusal(&f.req("unit", "u1"));
    assert_eq!(e.after, Some("start"));
    (f.dead, f.die_after) = (false, None);
    let key = f.reviews_of("unit", "u1").pop().unwrap().0;
    let out = f.dir().join(format!("{key}.md"));
    let rec = record(out.to_str().unwrap());
    let run = rec["run_object"]["key"].as_str().unwrap().to_owned();
    assert_eq!(f.get("run", &run)["status"], "running");
    // The call was killed before it removed its directory.
    let dir = f.dir().join("work").join(&key);
    std::fs::create_dir_all(dir.join("deep")).unwrap();
    std::fs::set_permissions(dir.join("deep"), std::fs::Permissions::from_mode(0o500)).unwrap();
    let (v, status) = f.ok(&ending(&f, "abandon", &key));
    assert_eq!(
        (status, &v["verdict"], &v["cause"], &v["session"]),
        (3, &json!("abandoned"), &json!("abandon"), &rec["session"]),
        "{v}"
    );
    assert_eq!(v["run_closed"], "applied");
    let run_object = f.get("run", &run);
    assert_eq!(
        (&run_object["status"], &run_object["state"]),
        (&json!("closed"), &rec["state"])
    );
    assert!(!dir.exists() && f.work().is_empty());
    let close = f.bodies("apply", "close_unit_review").pop().unwrap();
    assert!(close["request_id"].as_str().unwrap().ends_with("-abandon"));
    assert_eq!(close["parameters"]["unit"], oid("unit", "u1"));
    // No record: `<directory>/work/<key>` of the checked key; a link there is removed as a link.
    let key = cut_after_start(&mut f);
    std::fs::remove_file(format!(
        "{}.run.json",
        f.dir().join(format!("{key}.md")).display()
    ))
    .unwrap();
    let target = f.t.join("not-the-verb-s");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("kept.txt"), "invented").unwrap();
    symlink(&target, f.dir().join("work").join(&key)).unwrap();
    let decoy = f.dir().join("work/decoy");
    std::fs::create_dir(&decoy).unwrap();
    let (v, _) = f.ok(&ending(&f, "abandon", &key));
    assert_eq!(
        (&v["session"], &v["run_closed"]),
        (&json!("lead-1"), &Value::Null)
    );
    assert!(target.join("kept.txt").exists() && decoy.is_dir());
    assert_eq!(f.work(), ["decoy"]);
}

// spec: specs/ap/unit-review.md#R18
// spec: specs/ap/unit-review.md#R8
#[test]
fn r18_a_resumed_look_cut_during_its_stage_is_abandoned_and_resumed_again() {
    let mut f = F::new();
    let first = first_look(&mut f, "repair");
    let dir = f.dir().join("work").join(first["review"].as_str().unwrap());
    f.die_after = Some("start_run");
    f.refusal(&resume(&f, &first["review"]));
    (f.dead, f.die_after) = (false, None);
    let cut = f
        .reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0;
    let rec = record(f.dir().join(format!("{cut}.md")).to_str().unwrap());
    assert_eq!(
        (&rec["cwd"], &rec["session"]),
        (&json!(dir), &first["session"])
    );
    std::fs::create_dir_all(&dir).unwrap();
    let (v, _) = f.ok(&ending(&f, "abandon", &cut));
    assert_eq!(
        v["session"], first["session"],
        "the look counts for its session"
    );
    assert!(!dir.exists());
    let (v, status) = f.ok(&resume(&f, &first["review"]));
    assert_eq!((status, &v["session"]), (0, &first["session"]));
    assert_eq!(f.seen("cwd").trim(), dir.to_str().unwrap());
    let e = f.refusal(&resume(&f, &first["review"]));
    assert_eq!(code(&e).0, "resume_limit");
}

// spec: specs/ap/unit-review.md#R19
#[test]
fn r19_abandon_takes_the_caller_s_session_when_the_store_refuses_the_record_s() {
    let mut f = F::new();
    let key = cut_after_start(&mut f);
    f.stage_writes = true;
    let (v, _) = f.ok(&ending(&f, "abandon", &key));
    assert_eq!(
        (&v["session"], &v["cause"]),
        (&json!("lead-1"), &json!("abandon"))
    );
    let closes = f.bodies("apply", "close_unit_review");
    let (a, b) = (&closes[closes.len() - 2], &closes[closes.len() - 1]);
    assert_ne!(a["parameters"]["session"], json!("lead-1"));
    assert_eq!(
        (&a["request_id"], &b["parameters"]["session"]),
        (&b["request_id"], &json!("lead-1"))
    );
    let key = cut_after_start(&mut f);
    f.modify("unit", "u1", json!({"writers": ["lead-1"]}));
    let e = f.refusal(&ending(&f, "abandon", &key));
    assert_eq!(code(&e), ("writer_cannot_review".into(), "session".into()));
    assert!(e.detail["next"].as_str().unwrap().contains(&key));
}

// spec: specs/ap/unit-review.md#R20
#[test]
fn r20_the_answer_names_the_review_and_every_review_of_the_subject() {
    let mut f = F::new();
    let (v, status) = f.ok(&f.req("unit", "u1"));
    let members: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        members,
        BTreeSet::from([
            "review",
            "kind",
            "unit",
            "digest",
            "round",
            "verdict",
            "report",
            "session",
            "reviews",
            "page_amended_during_review"
        ])
    );
    assert_eq!(
        (status, &v["kind"], &v["unit"]),
        (0, &json!("page"), &json!("u1"))
    );
    let review = f.get("review", v["review"].as_str().unwrap());
    assert_eq!(
        v["reviews"],
        json!([{"review": v["review"], "head": review["head"], "round": 1, "status": "closed", "verdict": "accept"}])
    );
    f.answer("digest", Some(&"2".repeat(64)));
    let (v, status) = f.ok(&f.req("unit", "u1"));
    assert_eq!(
        (status, &v["cause"], v["reviews"].as_array().unwrap().len()),
        (3, &json!("no_result"), 2)
    );
    let rounds: Vec<&Value> = v["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["round"])
        .collect();
    assert_eq!(rounds, [&json!(1), &json!(2)]);
}

// spec: specs/ap/unit-review.md#R21
#[test]
fn r21_a_page_amended_while_its_review_ran_is_not_made_ready() {
    use ap::unit::object;
    let mut f = F::new();
    f.unit("u2", "contract", false);
    let ready = |f: &mut F| {
        let call = object::arguments(&["ready", "--store", STORE].map(OsString::from)).unwrap();
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let r = json!({"request_id": format!("rd{n}"), "agent": "lead", "session": "lead-1", "unit": "u2"});
        object::execute(f, &call, &r)
    };
    f.tick(1);
    f.amend_at_launch = Some("u2");
    let (v, status) = f.ok(&f.req("unit", "u2"));
    assert_eq!(
        (status, &v["verdict"], &v["page_amended_during_review"]),
        (0, &json!("accept"), &json!(true)),
        "{v}"
    );
    let e = ready(&mut f).unwrap_err();
    assert_eq!(e.code, "page_not_reviewed");
    // Amended while it ran, but only its tiers and estimate: the digest is the one reviewed, and the
    // accept counts although the amendment is later than its start.
    f.tiers_at_launch = Some("u2");
    let (v, _) = f.ok(&f.req("unit", "u2"));
    assert_eq!(v["page_amended_during_review"], false, "{v}");
    let unit = f.get("unit", "u2");
    let review = f.get("review", v["review"].as_str().unwrap());
    assert_eq!(unit["estimate_lines"], 77);
    assert!(unit["amended_at"].as_str() >= review["started_at"].as_str());
    let answer: Value = serde_json::from_str(&ready(&mut f).unwrap()).unwrap();
    assert_eq!(answer["state"], "ready", "{answer}");
}

// spec: specs/ap/unit-review.md#R22
#[test]
fn r22_the_verb_writes_only_its_actions_and_removes_only_its_own() {
    let mut f = F::new();
    let first = first_look(&mut f, "repair");
    let key = cut_after_start(&mut f);
    f.ok(&ending(&f, "abandon", &key));
    let mut r = f.req("unit", "u1");
    r["earlier"] = json!([first["review"]]);
    f.ok(&r);
    let actions: BTreeSet<String> = f.actions().into_iter().collect();
    let allowed = [
        "start_unit_review",
        "close_unit_review",
        "start_run",
        "close_run",
    ];
    assert!(
        actions.iter().all(|a| allowed.contains(&a.as_str())),
        "{actions:?}"
    );
    assert!(
        f.calls
            .iter()
            .filter(|c| c.0 == "ont")
            .all(|c| c.1.ends_with(&["--store".into(), STORE.into()]))
    );
    // Left on the host: the prompts, the reports with their run files; no working directory.
    assert!(f.work().is_empty());
    let mut names: Vec<String> = std::fs::read_dir(f.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(
        names
            .iter()
            .all(|n| ["prompts", "work", "scratch"].contains(&n.as_str())
                || n.starts_with("act-rq-")),
        "{names:?}"
    );
    // No scratch directory of a look is left either.
    assert_eq!(
        std::fs::read_dir(f.dir().join("scratch")).unwrap().count(),
        0
    );
}

/// What a refused look must leave as it was: the unit, its revision and digest, the reviews, the
/// sessions started, the prompts and the validates.
fn untouched(f: &F, key: &str) -> (Value, u64, String, usize, usize, usize, usize) {
    let unit = f.get("unit", key);
    let prompts = std::fs::read_dir(f.dir().join("prompts")).map_or(0, |d| d.count());
    (
        unit.clone(),
        f.revisions[&k("unit", key)],
        page_digest(key, &unit),
        f.reviews_of("unit", key).len(),
        f.launches,
        prompts,
        f.bodies("validate", "start_unit_review").len(),
    )
}
/// A look at `key` with these acceptance checks: refused before anything is validated or started.
fn refused_look(f: &mut F, key: &str, acceptance: Value) -> Refusal {
    f.modify("unit", key, json!({"acceptance": acceptance}));
    let before = untouched(f, key);
    let e = f.refusal(&f.req("unit", key));
    assert_eq!(e.after, None, "{e:?}");
    assert_eq!(untouched(f, key), before, "{e:?}");
    e
}

// spec: specs/ap/unit-review.md#R23
// spec: specs/ap/unit.md#R36
#[test]
fn r23_a_look_runs_the_checks_of_ready_on_the_base_at_the_commit_of_its_text() {
    let mut f = F::new();
    // A commit on the origin the clone has not fetched: the one fetch of the look serves both.
    std::fs::write(f.t.join("origin/AGENTS.md"), "# Invented widgets, second\n").unwrap();
    git(&f.t.join("origin"), &["commit", "--quiet", "-am", "second"]);
    let commit = git(&f.t.join("origin"), &["rev-parse", "HEAD"]);
    let seen = f.t.join("seen");
    std::fs::create_dir(&seen).unwrap();
    let seen = seen.display();
    let record = format!(
        "git rev-parse HEAD > {seen}/head && pwd > {seen}/pwd && printf '%s %s' \"$AP_BASE\" \"$AP_HEAD\" > {seen}/env"
    );
    f.modify(
        "unit",
        "u1",
        json!({"acceptance": [{"check": record, "kind": "invariant"},
            {"check": "test -e src/widgets.rs", "kind": "fails_on_base"}]}),
    );
    let before = f.get("unit", "u1");
    let (v, status) = f.ok(&f.req("unit", "u1"));
    assert_eq!((status, &v["verdict"]), (0, &json!("accept")), "{v}");
    // The unit is as it was: state, `amended_at`, digest; no `page_not_reviewed` of a contract page.
    assert_eq!(f.get("unit", "u1"), before);
    let read = |name: &str| std::fs::read_to_string(format!("{seen}/{name}")).unwrap();
    assert_eq!(read("head").trim(), commit);
    assert_eq!(read("env"), format!("{commit} {commit}"));
    let prompt = std::fs::read_to_string(
        f.get("review", v["review"].as_str().unwrap())["prompt"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(prompt.contains(&format!("## The repository at {commit}\n")));
    let fetches = f.git.iter().filter(|a| a.iter().any(|w| w == "fetch"));
    assert_eq!(fetches.count(), 1, "{:?}", f.git);
    // `ready`'s temporary directory, removed after the checks.
    let pwd = PathBuf::from(read("pwd").trim());
    let name = pwd.parent().unwrap().file_name().unwrap().to_string_lossy();
    assert!(
        pwd.starts_with(std::env::temp_dir())
            && name.starts_with("ap-ready-")
            && pwd.ends_with("base"),
        "{pwd:?}"
    );
    assert!(!pwd.parent().unwrap().exists());
    // Each check in its own unit, stopped and read as ended, before the start is validated.
    let validate = f
        .calls
        .iter()
        .position(|c| c.1.get(1).is_some_and(|w| w == "validate"))
        .unwrap();
    let checks: Vec<&Vec<String>> = f.calls[..validate]
        .iter()
        .filter(|c| c.0 == "systemctl")
        .map(|c| &c.1)
        .collect();
    assert_eq!(checks.len(), 4, "{checks:?}");
    assert!(checks.iter().all(|a| a[1].starts_with("ap-check-")));
    assert_eq!(
        checks.iter().map(|a| a[0].as_str()).collect::<Vec<_>>(),
        ["stop", "is-active", "stop", "is-active"]
    );
}

// spec: specs/ap/unit-review.md#R23
// spec: specs/ap/unit.md#R36
#[test]
fn r23_a_check_that_does_not_hold_on_the_base_refuses_the_look_before_anything_starts() {
    let mut f = F::new();
    let e = refused_look(
        &mut f,
        "u1",
        json!([{"check": "true", "kind": "fails_on_base"}]),
    );
    assert_eq!(
        (code(&e), &e.detail["index"], &e.detail["status"]),
        (
            ("acceptance_passes_on_base".into(), "acceptance".into()),
            &json!(1),
            &json!(0)
        )
    );
    let e = refused_look(
        &mut f,
        "u1",
        json!([{"check": "true", "kind": "invariant"},
            {"check": "echo invented-output; exit 3", "kind": "invariant"}]),
    );
    assert_eq!(
        (code(&e), &e.detail["index"], &e.detail["status"]),
        (
            ("invariant_fails_on_base".into(), "acceptance".into()),
            &json!(2),
            &json!(3)
        )
    );
    assert!(
        e.detail["output"]
            .as_str()
            .unwrap()
            .contains("invented-output")
    );
    let e = refused_look(
        &mut f,
        "u1",
        json!([{"check": "exit 127", "kind": "fails_on_base"}]),
    );
    assert_eq!(code(&e).0, "acceptance_not_runnable");
    let e = refused_look(
        &mut f,
        "u1",
        json!([{"check": "exit 124", "kind": "invariant"}]),
    );
    assert_eq!(code(&e).0, "acceptance_timeout");
    f.checks_alive = true;
    let e = refused_look(
        &mut f,
        "u1",
        json!([{"check": "false", "kind": "fails_on_base"}]),
    );
    assert_eq!(code(&e), ("check_not_stopped".into(), "acceptance".into()));
    assert!(e.detail.as_str().unwrap().starts_with("ap-check-"));
    f.checks_alive = false;
    // The store's criteria, from the unit read, in the store's order; no check runs.
    let never = json!([{"check": "exit 99", "kind": "invariant"}]);
    let e = refused_look(&mut f, "u1", json!([]));
    assert_eq!(
        (code(&e), &e.detail),
        (
            ("acceptance_missing".into(), "unit".into()),
            &json!({"failed": ["acceptance_missing"]})
        )
    );
    f.modify("unit", "u1", json!({"verifier": null}));
    let e = refused_look(&mut f, "u1", never.clone());
    assert_eq!(
        (code(&e), &e.detail),
        (
            ("tiers_missing".into(), "unit".into()),
            &json!({"failed": ["tiers_missing"]})
        )
    );
    f.modify("unit", "u1", json!({"write_set": []}));
    let e = refused_look(&mut f, "u1", json!([]));
    assert_eq!(
        (code(&e), &e.detail),
        (
            ("write_set_missing".into(), "unit".into()),
            &json!({"failed": ["write_set_missing", "acceptance_missing", "tiers_missing"]})
        )
    );
    // A unit without a repository: an empty directory, `AP_BASE` and `AP_HEAD` empty.
    f.unit("u2", "contract", false);
    let empty = r#"test -z "$(ls -A)" && test -z "$AP_BASE$AP_HEAD" && exit 1"#;
    let e = refused_look(&mut f, "u2", json!([{"check": empty, "kind": "invariant"}]));
    assert_eq!(code(&e).0, "invariant_fails_on_base");
    // A `plain` page, a `ready` and a `stopped` unit are checked alike.
    for (key, class, state) in [
        ("u3", "plain", "draft"),
        ("u4", "contract", "ready"),
        ("u5", "effect", "stopped"),
    ] {
        f.unit(key, class, true);
        f.modify("unit", key, json!({"state": state}));
        let e = refused_look(
            &mut f,
            key,
            json!([{"check": "false", "kind": "invariant"}]),
        );
        assert_eq!(code(&e).0, "invariant_fails_on_base", "{key}");
        f.modify(
            "unit",
            key,
            json!({"acceptance": [{"check": "true", "kind": "invariant"}]}),
        );
        let (v, _) = f.ok(&f.req("unit", key));
        assert_eq!(v["verdict"], "accept", "{key}");
        assert_eq!(f.get("unit", key)["state"], state);
    }
}

// spec: specs/ap/unit-review.md#R23
// spec: specs/ap/unit-review.md#R8
#[test]
fn r23_a_look_that_resumes_runs_the_same_checks_first() {
    let mut f = F::new();
    let first = first_look(&mut f, "repair");
    let leftover = f.dir().join("work").join(first["review"].as_str().unwrap());
    std::fs::create_dir_all(&leftover).unwrap();
    f.modify(
        "unit",
        "u1",
        json!({"acceptance": [{"check": "false", "kind": "invariant"}]}),
    );
    let before = untouched(&f, "u1");
    let e = f.refusal(&resume(&f, &first["review"]));
    assert_eq!(
        (code(&e), e.after),
        (
            ("invariant_fails_on_base".into(), "acceptance".into()),
            None
        )
    );
    assert_eq!(untouched(&f, "u1"), before);
    assert!(leftover.is_dir(), "nothing removed before the checks pass");
    f.modify(
        "unit",
        "u1",
        json!({"acceptance": [{"check": "true", "kind": "invariant"}]}),
    );
    let (v, _) = f.ok(&resume(&f, &first["review"]));
    assert_eq!(
        (&v["verdict"], &v["session"]),
        (&json!("accept"), &first["session"])
    );
    assert!(!leftover.exists());
}

// spec: specs/ap/unit-review.md#R24
#[test]
fn r24_the_frames_name_the_kinds_of_a_blocking_finding() {
    let page = include_str!("../src/unit/frames/reviewer.md");
    for name in [
        "unbuildable",
        "contradiction",
        "check",
        "write_set",
        "unguarded_effect",
    ] {
        assert!(page.contains(&format!("`blocking {name}:`")), "{name}");
    }
    for name in [
        "unreached_criterion",
        "dependency",
        "conflict",
        "unguarded_effect",
    ] {
        assert!(
            PLAN_REVIEWER.contains(&format!("`blocking {name}:`")),
            "{name}"
        );
    }
    for frame in [page, PLAN_REVIEWER] {
        assert!(frame.contains("`note:`") && frame.contains("case-sensitive"));
    }
    assert!(REPAIR_LOOK.contains("`earlier <kind>: closed`"));
    assert!(REPAIR_LOOK.contains("`earlier <kind>: open`"));
}

// ---- A look under `limits` (specs/ap/run-limits.md) -------------------------------------------

/// The words of a launch under `limits` for a look whose working directory is `cwd`, on `account`
/// with the scratch directory of `review`, up to the hidden paths.
fn limited(f: &F, account: &str, review: &str) -> Vec<String> {
    let (account, scratch) = (f.p(account), f.dir().join("scratch").join(review));
    let scratch = scratch.to_str().unwrap();
    [
        BWRAP,
        "--unshare-pid",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--ro-bind",
        "/proc/sys",
        "/proc/sys",
        "--bind",
        &account,
        &account,
        "--bind",
        scratch,
        scratch,
    ]
    .map(str::to_owned)
    .to_vec()
}
/// Checks the launch `wrap` of the look of `review` in `cwd`: the binds, the hidden home, a
/// working directory that is not bound (read-only), and the record's `limits`.
fn under_limits(f: &F, wrap: &[String], review: &str, cwd: &Path, report: &str) {
    let want = limited(f, "a1", review);
    assert_eq!(wrap[..want.len()], want[..], "{wrap:?}");
    let ssh = f.p("home/.ssh");
    let hidden = ["--tmpfs", &ssh, "--remount-ro", &ssh].map(str::to_owned);
    assert!(wrap.windows(4).any(|w| w == hidden), "{wrap:?}");
    let cwd = cwd.to_str().unwrap().to_owned();
    let tail = ["--cap-drop", "ALL", "--chdir", &cwd, "--"].map(str::to_owned);
    assert_eq!(wrap[wrap.len() - 5..], tail[..], "{wrap:?}");
    let bound = wrap.windows(2).any(|w| w[0] == "--bind" && w[1] == cwd);
    assert!(
        !bound,
        "the working directory is read-only to the stage: {wrap:?}"
    );
    let limits = &record(report)["limits"];
    let scratch = f.dir().join("scratch").join(review);
    assert_eq!(limits["scratch"], json!(scratch), "{limits}");
    assert!(
        limits["hidden"].as_array().unwrap().contains(&json!(ssh)),
        "{limits}"
    );
    // `ap run` made it and removed it after the stage.
    assert!(!scratch.exists());
}

// spec: specs/ap/unit-review.md#R5
// spec: specs/ap/unit-review.md#R13
#[test]
fn r13_a_look_on_claude_runs_under_limits_with_its_own_scratch_directory() {
    let mut f = F::new();
    // A page.
    let first = first_look(&mut f, "repair");
    let key = first["review"].as_str().unwrap().to_owned();
    assert_eq!(mode(&f.dir().join("scratch")), 0o700);
    let work = f.dir().join("work").join(&key);
    let wrap = f.wraps.last().unwrap().clone();
    under_limits(&f, &wrap, &key, &work, first["report"].as_str().unwrap());
    assert_eq!(f.probes, 2, "the admission and the stage each probe once");
    // A resumed look: its own key's scratch directory, the resumed record's working directory.
    f.tick(5);
    let (v, _) = f.ok(&resume(&f, &first["review"]));
    let again = v["review"].as_str().unwrap().to_owned();
    assert_ne!(again, key);
    let wrap = f.wraps.last().unwrap().clone();
    under_limits(&f, &wrap, &again, &work, v["report"].as_str().unwrap());
    // A plan.
    let (v, _) = f.ok(&f.req("intent", "widgets"));
    let plan = v["review"].as_str().unwrap().to_owned();
    let wrap = f.wraps.last().unwrap().clone();
    let work = f.dir().join("work").join(&plan);
    under_limits(&f, &wrap, &plan, &work, v["report"].as_str().unwrap());
    assert_eq!(
        std::fs::read_dir(f.dir().join("scratch")).unwrap().count(),
        0
    );
}

// spec: specs/ap/unit-review.md#R13
#[test]
fn r13_a_look_on_another_runtime_is_launched_without_limits() {
    let mut f = F::new();
    std::fs::create_dir(f.t.join("muse")).unwrap();
    f.write_policy(json!({"runtime": "muse", "account": f.p("muse"),
        "permission_mode": "bypassPermissions"}));
    let path = f.t.join("policy.json");
    let mut policy: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    policy["accounts"]["muse"] = json!([f.p("muse")]);
    std::fs::write(&path, policy.to_string()).unwrap();
    // The fake has no `muse`: the launch fails, after the words it was given are kept.
    let (v, _) = f.ok(&f.req("unit", "u1"));
    assert_eq!(v["verdict"], "abandoned", "{v}");
    assert_eq!(f.wraps, [Vec::<String>::new()]);
    assert_eq!(f.probes, 0);
    let rec = record(
        f.dir()
            .join(format!("{}.md", v["review"].as_str().unwrap()))
            .to_str()
            .unwrap(),
    );
    assert!(rec.get("limits").is_none(), "{rec}");
}

// spec: specs/ap/unit-review.md#R18
// spec: specs/ap/unit-review.md#R22
#[test]
fn r18_abandon_and_finish_remove_the_scratch_directory_a_killed_run_left() {
    let mut f = F::new();
    let scratch = |f: &F, k: &str| f.dir().join("scratch").join(k);
    // Another review's directory, and a directory outside that a link points to: both stay.
    std::fs::create_dir_all(scratch(&f, "other/tmp")).unwrap();
    std::fs::create_dir_all(f.t.join("outside")).unwrap();
    std::fs::write(f.t.join("outside/keep"), "k").unwrap();
    // `abandon` of a cut look: the directory with everything in it, a link inside as a link.
    let key = cut_after_start(&mut f);
    std::fs::create_dir_all(scratch(&f, &key).join("tmp")).unwrap();
    symlink(f.t.join("outside"), scratch(&f, &key).join("tmp/link")).unwrap();
    f.ok(&ending(&f, "abandon", &key));
    assert!(std::fs::symlink_metadata(scratch(&f, &key)).is_err());
    // `finish` of a look cut after its stage; the path a link: removed as a link.
    f.die_after = Some("close_run");
    f.refusal(&f.req("unit", "u1"));
    (f.dead, f.die_after) = (false, None);
    let key = f
        .reviews_of("unit", "u1")
        .into_iter()
        .find(|(_, r)| r["status"] == "running")
        .unwrap()
        .0;
    symlink(f.t.join("outside"), scratch(&f, &key)).unwrap();
    let (v, _) = f.ok(&ending(&f, "finish", &key));
    assert_eq!(v["verdict"], "accept", "{v}");
    assert!(std::fs::symlink_metadata(scratch(&f, &key)).is_err());
    assert_eq!(
        std::fs::read_to_string(f.t.join("outside/keep")).unwrap(),
        "k"
    );
    assert!(scratch(&f, "other/tmp").is_dir());
}
