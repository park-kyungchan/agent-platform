//! `ap unit run` (`specs/ap/unit-run.md`) against fakes: a fake of `ont` that answers the reads and the
//! actions of the run, of `ap run` and of the page as the engine does (submission criteria, request ids
//! replayed as `already_applied` or refused `request_id_reused`, the key of a start derived from the
//! agent and the request id, the views `reviews_of_unit` and `holds_on_work`); a fake hosting service
//! that reads a pull request's head from a bare origin; the fake `claude` of
//! tests/fixtures/fake-claude.sh in mode `script` with the stages of tests/fixtures/unit_run/stage.sh;
//! `systemctl` and the unit launcher replaced as in tests/unit_review.rs, a check's `systemd-run`
//! answered by running its command here. The policy file, the units and the reviews directory are under
//! a temporary directory: no test starts a unit of the service manager or reaches the host's store,
//! policy file, units, reviews, a clone of the host or the hosting service.
use ap::host::{Answer, Failure, Host, Reply, System};
use ap::unit::drive::{Refusal, arguments, execute};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const FAKE: &str = include_str!("fixtures/fake-claude.sh");
const STAGE: &str = include_str!("fixtures/unit_run/stage.sh");
const FIXER: &str = include_str!("../src/unit/frames/fixer.md");
const SECOND_LOOK: &str = include_str!("../src/unit/frames/second-look.md");
const CONTINUE: &str = include_str!("../src/unit/frames/continue.md");
const STORE: &str = "/invented/scratch/store";
const KEY: &str = "widgets-list";

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://invented","account":"root"},"native_key":key})
}
fn reply(success: bool, v: &Value) -> Answer {
    let bytes = serde_json::to_vec(v).unwrap();
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
/// UTC RFC 3339 to the second, as the engine and `ap run` write a time.
fn stamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (h, mi, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}
fn now() -> String {
    stamp(SystemTime::now())
}
fn git(dir: &Path, words: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(words)
        .env("GIT_AUTHOR_NAME", "Invented")
        .env("GIT_AUTHOR_EMAIL", "invented@example.invalid")
        .env("GIT_COMMITTER_NAME", "Invented")
        .env("GIT_COMMITTER_EMAIL", "invented@example.invalid")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {words:?}: {out:?}");
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

/// The fake store, hosting service and host.
struct F {
    t: PathBuf,
    objects: BTreeMap<Key, Value>,
    revisions: BTreeMap<Key, u64>,
    applied: BTreeMap<String, (Value, Value)>,
    /// Every call of a program through the port: the program, its arguments and its input.
    calls: Vec<(String, Vec<String>, Value)>,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
    env: Vec<(String, String)>,
    clock: Duration,
    launches: usize,
    /// At this launch the store dies: every later call of `ont` fails and no stage launches.
    die_at_launch: Option<usize>,
    /// After this action is applied, the store dies.
    die_after: Option<&'static str>,
    dead: bool,
    /// The store refuses this action with this code.
    refuse: Option<(&'static str, &'static str)>,
    pr_state: &'static str,
    /// Further pull requests the list reports: a fork's of the same branch, or a second one.
    extra_prs: Vec<Value>,
    check: &'static str,
    threads: Vec<Value>,
    /// The processes that stand for a cut stage's unit, reaped at the end.
    sleepers: Vec<Child>,
}
impl F {
    fn new() -> Self {
        let t = std::env::temp_dir().join(format!(
            "ap-unit-run-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "a3", "seed"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let t = std::fs::canonicalize(&t).unwrap();
        let fake = t.join("bin/claude");
        std::fs::write(&fake, FAKE).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        for account in ["a1", "a2", "a3"] {
            std::fs::write(t.join(format!("bin/mode-{account}")), "script").unwrap();
            std::fs::write(t.join(format!("bin/script-{account}.sh")), STAGE).unwrap();
        }
        std::fs::write(t.join("bin/branch"), format!("unit/{KEY}")).unwrap();
        git(
            &t,
            &["init", "--quiet", "--bare", "-b", "main", "origin.git"],
        );
        let seed = t.join("seed");
        git(&seed, &["init", "--quiet", "-b", "main"]);
        std::fs::write(seed.join("AGENTS.md"), "# Invented widgets\n").unwrap();
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "--quiet", "-m", "invented base"]);
        let origin = t.join("origin.git");
        git(
            &seed,
            &["push", "--quiet", origin.to_str().unwrap(), "main"],
        );
        git(&t, &["clone", "--quiet", origin.to_str().unwrap(), "clone"]);
        let mut f = Self {
            t,
            objects: BTreeMap::new(),
            revisions: BTreeMap::new(),
            applied: BTreeMap::new(),
            calls: vec![],
            groups: BTreeMap::new(),
            alive: BTreeSet::new(),
            env: vec![
                ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
                ("HOME".into(), "/root".into()),
                ("LANG".into(), "C.UTF-8".into()),
            ],
            clock: Duration::ZERO,
            launches: 0,
            die_at_launch: None,
            die_after: None,
            dead: false,
            refuse: None,
            pr_state: "OPEN",
            extra_prs: vec![],
            check: "SUCCESS",
            threads: vec![],
            sleepers: vec![],
        };
        f.policy(json!({}));
        f.put("task", "invented-task", json!({"status": "open"}));
        f.put("intent", "widgets", json!({"status": "confirmed",
            "outcome": "Every invented widget is listed once.", "first_user": "An invented reader."}));
        let earlier = stamp(SystemTime::now() - Duration::from_secs(100));
        f.put("unit", KEY, json!({"lane": "objective", "class": "plain",
            "intent": oid("intent", "widgets"), "objective": "The invented widget list shows each widget once.",
            "repository": "invented/widgets", "write_set": ["src/**"],
            "acceptance": [{"check": "test -f src/widgets.rs", "kind": "fails_on_base"}],
            "implementer": {"runtime": "claude", "model": "opus", "effort": "high"},
            "verifier": {"runtime": "claude", "model": "opus", "effort": "xhigh"},
            "state": "ready", "writers": [], "rounds": 0, "added_at": earlier, "amended_at": earlier}));
        f
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_string_lossy().into_owned()
    }
    fn home(&self) -> PathBuf {
        self.t.join("units").join(KEY)
    }
    fn wt(&self) -> PathBuf {
        self.home().join("wt")
    }
    /// The policy file: `models`, `accounts`, `repositories`, `reviews` and `units`, with `extra`'s
    /// members over them (a null member left out).
    fn policy(&self, extra: Value) {
        let mut policy = json!({"models": {"allowed": ["opus", "sonnet"], "never": []},
            "accounts": {"claude/implementer": [self.p("a1")], "claude/fixer": [self.p("a1"), self.p("a3")],
                "claude/reviewer": [self.p("a2")], "claude": [self.p("a3")]},
            "repositories": {"invented/widgets": {"clone": self.p("clone")}},
            "reviews": {"task": "invented-task", "directory": self.p("reviews")},
            "units": {"directory": self.p("units")}});
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
    fn steps(&self, steps: &[&str]) {
        std::fs::write(self.t.join("bin/steps"), steps.join("\n") + "\n").unwrap();
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
    fn unit(&self) -> Value {
        self.objects[&k("unit", KEY)].clone()
    }
    fn set_unit(&mut self, set: Value) {
        self.modify("unit", KEY, set);
    }
    fn run(&mut self) -> Result<(Value, u8), Box<Refusal>> {
        execute(self, Some(STORE), KEY)
    }
    fn ok(&mut self) -> (Value, u8) {
        self.run().unwrap_or_else(|e| panic!("refused: {e:?}"))
    }
    fn refusal(&mut self) -> Refusal {
        match self.run() {
            Err(e) => *e,
            Ok(v) => panic!("expected a refusal, got {v:?}"),
        }
    }
    /// The answer of a call that ended `stopped`, with its cause.
    fn stopped(&mut self, cause: &str) -> Value {
        let (a, code) = self.ok();
        assert_eq!((a["state"].as_str(), code), (Some("stopped"), 3), "{a}");
        assert_eq!(a["stop_cause"], cause, "{a}");
        a
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
    fn bodies(&self, action: &str) -> Vec<Value> {
        let applies = self
            .calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1[..2] == ["action", "apply"]);
        applies
            .filter(|c| c.2["action_type"] == action)
            .map(|c| c.2.clone())
            .collect()
    }
    fn packets(&self) -> Vec<(String, Value)> {
        let mut list: Vec<(String, Value)> = self
            .objects
            .iter()
            .filter(|((t, _), v)| t == "review" && v["kind"] == "packet")
            .map(|((_, k), v)| (k.clone(), v.clone()))
            .collect();
        list.sort_by_key(|(k, v)| (v["started_at"].as_str().unwrap().to_owned(), k.clone()));
        list
    }
    /// The arguments of launch `n` and the prompt it was given.
    fn launch(&self, n: usize) -> (String, String) {
        let log = std::fs::read_to_string(self.t.join("bin/calls.log")).unwrap();
        let line = log.lines().nth(n - 1).unwrap().to_owned();
        (
            line,
            std::fs::read_to_string(self.t.join(format!("bin/prompt.{n}"))).unwrap(),
        )
    }
    fn session_of(&self, n: usize) -> String {
        let (line, _) = self.launch(n);
        let words: Vec<&str> = line.split(' ').collect();
        let at = words
            .iter()
            .position(|w| *w == "--session-id" || *w == "--resume")
            .unwrap();
        words[at + 1].to_owned()
    }
    fn origin_head(&self) -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.t.join("origin.git"))
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/unit/{KEY}"),
            ])
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).unwrap().trim().to_owned())
    }
    /// A commit pushed to `branch` of the origin from a clone that is not the run's.
    fn push_from_elsewhere(&self, branch: &str, file: &str) -> String {
        let other = self.t.join("other");
        if !other.exists() {
            git(
                &self.t,
                &[
                    "clone",
                    "--quiet",
                    self.t.join("origin.git").to_str().unwrap(),
                    "other",
                ],
            );
        }
        git(&other, &["fetch", "--quiet", "origin"]);
        let theirs = format!("refs/remotes/origin/{branch}");
        let known = Command::new("git")
            .arg("-C")
            .arg(&other)
            .args(["rev-parse", "--verify", "--quiet", &theirs])
            .output()
            .unwrap();
        let from = match known.status.success() {
            true => theirs,
            false => "refs/remotes/origin/main".to_owned(),
        };
        git(&other, &["checkout", "--quiet", "-B", "work", &from]);
        std::fs::create_dir_all(other.join("src")).unwrap();
        std::fs::write(other.join(file), format!("{file} from elsewhere\n")).unwrap();
        git(&other, &["add", "-A"]);
        git(&other, &["commit", "--quiet", "-m", "from elsewhere"]);
        git(
            &other,
            &[
                "push",
                "--quiet",
                "origin",
                &format!("HEAD:refs/heads/{branch}"),
            ],
        );
        git(&other, &["rev-parse", "HEAD"])
    }
    /// The state a call killed during the stage of `record` leaves: the record says `running`, the
    /// stage's unit still lives, the unit and the run are `running` in the store.
    fn cut(&mut self, record: &Path) {
        let mut rec: Value = serde_json::from_slice(&std::fs::read(record).unwrap()).unwrap();
        rec["state"] = json!("running");
        rec["end"] = Value::Null;
        std::fs::write(record, rec.to_string()).unwrap();
        let unit = rec["unit"].as_str().unwrap().to_owned();
        let child = Command::new("sleep")
            .arg("600")
            .process_group(0)
            .spawn()
            .unwrap();
        self.groups.insert(unit.clone(), child.id());
        self.sleepers.push(child);
        self.alive.insert(unit);
        (self.dead, self.die_at_launch, self.die_after) = (false, None, None);
    }
    fn record(&self, rel: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.home().join(rel)).unwrap()).unwrap()
    }

    fn wrap(fields: &Value) -> Value {
        let o = fields.as_object().unwrap().iter();
        o.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
    }
    fn table(header: &str, rows: &[String]) -> Answer {
        let n = header.matches(" | ").count() + 1;
        let mut t = format!("<!-- ont:render -->\n{header}\n|{}\n", " --- |".repeat(n));
        for r in rows {
            t.push_str(r);
            t.push('\n');
        }
        reply(
            true,
            &json!({"status": "ok", "markdown": t, "rows": rows.len(), "truncated": false}),
        )
    }
    fn ont(&mut self, args: &[String], input: &Value) -> Answer {
        assert_eq!(args[args.len() - 2..], ["--store", STORE], "{args:?}");
        if self.dead {
            return Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            });
        }
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        match args[0].as_str() {
            "get" => {
                let id = &input["id"];
                let key = k(
                    id["object_type"].as_str().unwrap(),
                    id["native_key"].as_str().unwrap(),
                );
                let Some(o) = self.objects.get(&key) else {
                    return reply(false, &json!({"status": "refused", "code": "unavailable"}));
                };
                let revision = format!("r#{}", self.revisions[&key]);
                let object = json!({"id": id, "head": revision, "observations": [
                    {"revision": revision, "fields": Self::wrap(o)}]});
                reply(
                    true,
                    &json!({"status": "ok", "now": epoch, "result": {"objects": [object]}}),
                )
            }
            "render" if input["view"] == "reviews_of_unit" => {
                let rows: Vec<String> = self
                    .objects
                    .iter()
                    .filter(|((t, _), v)| {
                        t == "review" && v["unit"]["native_key"] == input["args"]["unit"]
                    })
                    .map(|((_, key), r)| {
                        format!(
                            "| {key} | {} | {} | {} | {} | {} | |",
                            r["kind"], r["head"], r["round"], r["status"], r["verdict"]
                        )
                    })
                    .collect();
                Self::table(
                    "| Key | Kind | Head | Round | Status | Verdict | Closed at |",
                    &rows,
                )
            }
            "render" if input["view"] == "holds_on_work" => {
                let cell = |v: &Value| v["native_key"].as_str().unwrap_or("(absent)").to_owned();
                let rows: Vec<String> = self
                    .objects
                    .iter()
                    .filter(|((t, _), v)| t == "hold" && v["status"] == "active")
                    .map(|(_, h)| {
                        format!(
                            "| {} | an invented \\| condition | {} | {} |  |",
                            h["effect"].as_str().unwrap(),
                            cell(&h["unit"]),
                            cell(&h["intent"])
                        )
                    })
                    .collect();
                Self::table("| Effect | Until | Unit | Intent | Scope |", &rows)
            }
            "action" if args[1] == "validate" => self.validate(input),
            "action" => self.apply(input),
            other => panic!("ont {other} {input}"),
        }
    }
    fn made_key(agent: &str, rid: &str) -> String {
        format!(
            "act-rq-{}",
            &format!("{:x}", Sha256::digest(format!("ap/{agent}/{rid}")))[..32]
        )
    }
    fn validate(&mut self, body: &Value) -> Answer {
        assert_eq!(body["action_type"], "start_unit_review");
        let known = self.objects.contains_key(&k(
            "task",
            body["parameters"]["task"]["native_key"].as_str().unwrap(),
        ));
        let key = Self::made_key(
            body["agent"].as_str().unwrap(),
            body["request_id"].as_str().unwrap(),
        );
        let edits = if known {
            json!([{"change": "initial", "id": oid("review", &key)}])
        } else {
            json!([])
        };
        reply(
            true,
            &json!({"status": "ok", "result": {"valid": known,
            "criteria": [{"code": "unknown_task", "passed": known}], "edits": edits}}),
        )
    }
    fn held(&self, u: &Value) -> bool {
        self.objects.iter().any(|((t, _), h)| {
            t == "hold"
                && h["status"] == "active"
                && (h["unit"]["native_key"] == KEY || h["intent"] == u["intent"])
        })
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
                false => reply(
                    false,
                    &json!({"status": "refused", "code": "request_id_reused"}),
                ),
            };
        }
        let criteria = |failed: Vec<&str>| json!({"status": "refused", "code": "submission_criteria", "failed": failed});
        if let Some((a, code)) = self.refuse
            && a == action
        {
            return reply(false, &criteria(vec![code]));
        }
        let now = now();
        let u = self.unit();
        let unit_key = p["unit"]["native_key"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let state = u["state"].as_str().unwrap_or_default().to_owned();
        let mut failed = vec![];
        let edits = match action.as_str() {
            "start_unit" | "resume_unit" => {
                let from = if action == "start_unit" {
                    "ready"
                } else {
                    "stopped"
                };
                if state != from {
                    failed.push(if from == "ready" {
                        "unit_not_ready"
                    } else {
                        "unit_not_stopped"
                    });
                }
                if p["intent"] != u["intent"] {
                    failed.push("intent_differs");
                }
                if self.held(&u) {
                    failed.push("held");
                }
                if failed.is_empty() {
                    self.set_unit(json!({"state": "running", "started_at": now}));
                }
                json!([{"change": "update", "id": oid("unit", KEY)}])
            }
            "record_head" | "stop_unit" | "accept_unit" => {
                let ok = state == "running" || (action == "record_head" && state == "stopped");
                if !ok {
                    failed.push("unit_not_running");
                }
                if action == "accept_unit" {
                    if p["head"] != u["head"] {
                        failed.push("head_differs");
                    }
                    let reviewed = self.packets().iter().any(|(_, r)| {
                        r["head"] == p["head"]
                            && r["status"] == "closed"
                            && r["verdict"] == "accept"
                            && r["started_at"].as_str() > u["amended_at"].as_str()
                    });
                    if !reviewed {
                        failed.push("head_not_reviewed");
                    }
                }
                if failed.is_empty() {
                    let set = match action.as_str() {
                        "record_head" => json!({"head": p["head"], "writers": p["writers"],
                            "pull_request": p["pull_request"], "rounds": p["rounds"]}),
                        "stop_unit" => json!({"state": "stopped", "stop_cause": p["cause"],
                            "stop_detail": p["detail"], "ended_at": now}),
                        _ => json!({"state": "accepted"}),
                    };
                    self.set_unit(set);
                }
                json!([{"change": "update", "id": oid("unit", &unit_key)}])
            }
            "start_unit_review" => {
                let key = Self::made_key(
                    body["agent"].as_str().unwrap(),
                    body["request_id"].as_str().unwrap(),
                );
                let mut r = p.clone();
                (r["status"], r["started_at"]) = (json!("running"), json!(now));
                self.put("review", &key, r);
                json!([{"change": "initial", "id": oid("review", &key)}])
            }
            "close_unit_review" => {
                let key = p["review"]["native_key"].as_str().unwrap().to_owned();
                let r = self.objects[&k("review", &key)].clone();
                if r["status"] != "running" {
                    failed.push("review_not_running");
                }
                if r["unit"] != p["unit"] {
                    failed.push("unit_differs");
                }
                if p["session"].is_null() {
                    failed.push("session_required");
                }
                if u["writers"].as_array().unwrap().contains(&p["session"]) {
                    failed.push("writer_cannot_review");
                }
                if failed.is_empty() {
                    let mut set = json!({"status": "closed", "closed_at": now});
                    for m in ["verdict", "report", "report_sha256", "account", "session"] {
                        set[m] = p.get(m).cloned().unwrap_or(Value::Null);
                    }
                    self.modify("review", &key, set);
                }
                json!([{"change": "update", "id": oid("review", &key)}])
            }
            "start_run" => {
                assert_eq!(body["agent"], "ap-run");
                let key = format!(
                    "run-{}",
                    self.objects.keys().filter(|(t, _)| t == "run").count() + 1
                );
                let mut r = p.clone();
                (r["status"], r["started_at"], r["role"]) =
                    (json!("running"), json!(now), body["role"].clone());
                self.put("run", &key, r);
                json!([{"change": "initial", "id": oid("run", &key)}])
            }
            "close_run" => {
                let key = p["run"]["native_key"].as_str().unwrap().to_owned();
                if self.objects[&k("run", &key)]["status"] != "running" {
                    failed.push("run_not_running");
                } else {
                    self.modify(
                        "run",
                        &key,
                        json!({"status": "closed", "state": p["state"], "closed_by": body["role"]}),
                    );
                }
                json!([{"change": "update", "id": oid("run", &key)}])
            }
            other => panic!("action {other}"),
        };
        if !failed.is_empty() {
            return reply(false, &criteria(failed));
        }
        self.applied.insert(derived, (content, edits.clone()));
        if self.die_after == Some(action.as_str()) {
            self.dead = true;
        }
        reply(
            true,
            &json!({"status": "ok", "result": {"outcome": "applied", "edits": edits}}),
        )
    }
    /// The hosting service: the pull request of the branch is open once the implementer opened it.
    fn gh(&mut self, a: &[String]) -> Answer {
        let ok = |v: Value| reply(true, &v);
        let head = self.origin_head();
        let opened = self.t.join("bin/pr-open").exists() && head.is_some();
        match (a[0].as_str(), a[1].as_str()) {
            ("pr", "list") => {
                assert!(
                    a.windows(2)
                        .any(|w| w[0] == "--head" && w[1] == format!("unit/{KEY}")),
                    "{a:?}"
                );
                let mut list = vec![];
                if opened && self.pr_state == "OPEN" {
                    list.push(json!({"number": 7, "headRefOid": head, "headRefName": format!("unit/{KEY}"),
                        "baseRefName": "main", "isCrossRepository": false,
                        "headRepositoryOwner": {"login": "invented"}, "headRepository": {"name": "widgets"}}));
                }
                if opened {
                    list.extend(self.extra_prs.iter().cloned());
                }
                ok(json!(list))
            }
            ("pr", "view") => {
                assert_eq!(a[2], "7");
                ok(json!({"state": self.pr_state, "headRefOid": head}))
            }
            ("api", "graphql") => {
                let query = a.last().unwrap();
                if query.contains("reviewThreads") {
                    return ok(
                        json!({"data":{"repository":{"pullRequest":{"reviewThreads":{
                        "pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":self.threads}}}}}),
                    );
                }
                let check = json!({"__typename":"CheckRun","name":"rust","status":"COMPLETED",
                    "conclusion":self.check,"isRequired":true,"checkSuite":{"workflowRun":{"url":"https://example.invalid/1"}}});
                ok(
                    json!({"data":{"repository":{"pullRequest":{"state":self.pr_state,"isDraft":false,
                    "headRefOid":head,"commits":{"nodes":[{"commit":{"oid":head,
                    "statusCheckRollup":{"contexts":{"nodes":[check]}}}}]},"comments":{"nodes":[]}}}}}),
                )
            }
            _ => panic!("unexpected gh call {a:?}"),
        }
    }
}
impl Drop for F {
    fn drop(&mut self) {
        for pgid in self.groups.values() {
            kill_group(*pgid);
        }
        for child in &mut self.sleepers {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
impl System for F {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        self.calls
            .push((program.into(), args.to_vec(), input.clone()));
        match program {
            "ont" => self.ont(args, input),
            "gh" => self.gh(args),
            "systemctl" => {
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
            // A check's unit: the command after `--`, run here (no unit is started).
            "systemd-run" => {
                let at = args.iter().position(|a| a == "--").unwrap();
                Host.call(&args[at + 1], &args[at + 2..], input, ms)
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
        self.launches += 1;
        if self.die_at_launch == Some(self.launches) {
            self.dead = true;
        }
        let at = argv.iter().position(|a| a == "--").unwrap();
        let unit = argv[..at]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
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
    fn now(&self) -> Instant {
        Instant::now() + self.clock
    }
    /// The clock of a wait moves without the wait; the one second before a review's start (against
    /// the store's strict comparison of times) passes for real, as the fake store's clock is real.
    fn pause(&mut self, duration: Duration) {
        self.clock += duration;
        let real = if duration == Duration::from_secs(1) {
            duration
        } else {
            Duration::from_millis(20)
        };
        std::thread::sleep(duration.min(real));
    }
    fn units(&self) -> bool {
        true
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(self.t.join("policy.json"))
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}
fn code(e: &Refusal) -> (&str, &str, u8) {
    (e.code.as_str(), e.field.as_str(), e.status)
}

// spec: specs/ap/unit-run.md#R1
#[test]
fn r1_the_form_the_role_and_the_actor_of_every_apply() {
    let words = |w: &[&str]| w.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(arguments(&words(&["u1"])).unwrap(), ("u1".into(), None));
    let with = arguments(&words(&["u1", "--store", "/s"])).unwrap();
    assert_eq!(with, ("u1".into(), Some("/s".into())));
    for bad in [
        &[][..],
        &["--store", "/s"],
        &["u1", "--store", "s"],
        &["u1", "x"],
        &["-u"],
    ] {
        assert_eq!(arguments(&words(bad)).unwrap_err().code, "usage", "{bad:?}");
    }
    // The binary: `usage` on standard error with exit 2; the present forms answer as before.
    let ap = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(args)
            .env_remove("ONT_ROLE")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let out = ap(&["unit", "run"]);
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (&e["error"]["code"], &e["error"]["field"]),
        (&json!("usage"), &json!("args"))
    );
    assert_eq!(ap(&["unit", "run", "a", "b"]).status.code(), Some(2));
    let help = String::from_utf8(ap(&["unit", "--help"]).stdout).unwrap();
    assert!(
        help.contains("ap unit run <key> [--store <path>]"),
        "{help}"
    );
    let help = String::from_utf8(ap(&["--help"]).stdout).unwrap();
    assert!(help.contains("unit run <key> [--store <path>]"), "{help}");
    assert_eq!(ap(&["unit", "page"]).status.code(), Some(2));
    let mut f = F::new();
    f.env.push(("ONT_ROLE".into(), "lead".into()));
    assert_eq!(code(&f.refusal()), ("role_in_environment", "ONT_ROLE", 2));
    assert!(f.calls.is_empty(), "{:?}", f.calls);
    f.env.pop();
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    let ours: Vec<Value> = f
        .calls
        .iter()
        .filter(|c| c.0 == "ont" && c.1[0] == "action" && c.2["agent"] == "ap-unit")
        .map(|c| c.2.clone())
        .collect();
    assert!(ours.len() >= 5, "{ours:?}");
    let prefix = ours[0]["request_id"]
        .as_str()
        .unwrap()
        .rsplit_once('-')
        .unwrap()
        .0
        .to_owned();
    assert!(
        prefix.len() == 19
            && prefix.starts_with("ur-")
            && prefix[3..].bytes().all(|c| c.is_ascii_hexdigit()),
        "{prefix}"
    );
    for b in &ours {
        assert_eq!(
            (&b["session"], b.get("role")),
            (&json!(format!("u-{KEY}")), None),
            "{b}"
        );
        let rid = b["request_id"].as_str().unwrap();
        assert!(rid.starts_with(&format!("{prefix}-")), "{rid}");
    }
    // Each apply has its own request id; the validate of a start has its apply's.
    let rids = |verb: &str| -> Vec<&str> {
        let of = ours.iter().zip(
            f.calls
                .iter()
                .filter(|c| c.0 == "ont" && c.1[0] == "action" && c.2["agent"] == "ap-unit"),
        );
        of.filter(|(_, c)| c.1[1] == verb)
            .map(|(b, _)| b["request_id"].as_str().unwrap())
            .collect()
    };
    let applies = rids("apply");
    assert_eq!(applies.iter().collect::<BTreeSet<_>>().len(), applies.len());
    assert!(rids("validate").iter().all(|r| applies.contains(r)));
}

// spec: specs/ap/unit-run.md#R2
#[test]
fn r2_every_check_before_anything_is_made_or_applied() {
    type Case = (&'static str, Value, Value, (&'static str, &'static str));
    let cases: Vec<Case> = vec![
        ("Bad", json!({}), json!({}), ("invalid_input", "unit")),
        ("absent", json!({}), json!({}), ("unknown_unit", "unit")),
        (
            KEY,
            json!({"repository": null}),
            json!({}),
            ("no_repository", "unit"),
        ),
        (
            KEY,
            json!({"branch": "main"}),
            json!({}),
            ("invalid_branch", "branch"),
        ),
        (
            KEY,
            json!({"branch": "origin/main"}),
            json!({}),
            ("invalid_branch", "branch"),
        ),
        (
            KEY,
            json!({"branch": "-f"}),
            json!({}),
            ("invalid_branch", "branch"),
        ),
        (
            KEY,
            json!({"branch": "a b"}),
            json!({}),
            ("invalid_branch", "branch"),
        ),
        (
            KEY,
            json!({"branch": "a/.x"}),
            json!({}),
            ("invalid_branch", "branch"),
        ),
        (
            KEY,
            json!({}),
            json!({"repositories": {}}),
            ("repository_not_mapped", "repository"),
        ),
        (
            KEY,
            json!({}),
            json!({"reviews": {}}),
            ("invalid_input", "task"),
        ),
        (
            KEY,
            json!({"verifier": {"runtime": "codex", "model": "o", "effort": "high"}}),
            json!({}),
            ("tier_not_supported", "verifier"),
        ),
        (
            KEY,
            json!({}),
            json!({"accounts": {"claude/reviewer": ["/a"]}}),
            ("account_not_mapped", "implementer"),
        ),
    ];
    for (key, unit, policy, want) in cases {
        let mut f = F::new();
        f.set_unit(unit.clone());
        f.policy(policy.clone());
        let e = match execute(&mut f, Some(STORE), key) {
            Err(e) => *e,
            Ok(v) => panic!("{v:?}"),
        };
        assert_eq!(code(&e), (want.0, want.1, 2), "{key} {unit} {policy}");
        assert!(
            f.actions().is_empty() && !f.t.join("units").exists(),
            "{unit}"
        );
    }
}

// spec: specs/ap/unit-run.md#R3
// spec: specs/ap/unit-run.md#R24
#[test]
fn r3_the_units_member_and_where_a_run_keeps_its_files() {
    for (units, member) in [
        (json!([]), "units"),
        (json!({"directory": "relative"}), "units.directory"),
        (json!({"directory": "/a b"}), "units.directory"),
        (
            json!({"timeouts": {"implementer": 0}}),
            "units.timeouts.implementer",
        ),
        (json!({"timeouts": {"lead": 5}}), "units.timeouts.lead"),
        (json!({"other": 1}), "units.other"),
    ] {
        let mut f = F::new();
        f.policy(json!({"units": units}));
        let e = f.refusal();
        assert_eq!(code(&e), ("policy_invalid", "policy", 2), "{units}");
        assert!(e.detail.as_str().unwrap().starts_with(member), "{e:?}");
    }
    let mut f = F::new();
    // The directory through a link is resolved once: the worktree is under the resolved path.
    std::os::unix::fs::symlink(f.t.join("real-units"), f.t.join("units-link")).unwrap();
    std::fs::create_dir(f.t.join("real-units")).unwrap();
    f.policy(json!({"units": {"directory": f.p("units-link"), "timeouts": {"verifier": 600}}}));
    f.steps(&["implement", "repair", "fix", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    let home = f.t.join("real-units").join(KEY);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&home), 0o700);
    let entries: BTreeSet<String> = std::fs::read_dir(&home)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    for name in [
        "lock",
        "wt",
        "1.implementer.md",
        "1.implementer.prompt.md",
        "1.implementer.md.run.json",
        "2.verifier.prompt.md",
        "3.fixer.md",
        "3.fixer.prompt.md",
        "4.verifier.prompt.md",
    ] {
        assert!(entries.contains(name), "{name}: {entries:?}");
    }
    let worktrees = git(&f.t.join("clone"), &["worktree", "list", "--porcelain"]);
    assert!(
        worktrees.contains(&format!("worktree {}", home.join("wt").display())),
        "{worktrees}"
    );
    // The reports are where `ap unit review` puts them; the verifier's bound is the policy's.
    for (key, r) in f.packets() {
        assert_eq!(r["report"], f.p(&format!("reviews/{key}.md")));
        let rec: Value = serde_json::from_slice(
            &std::fs::read(f.p(&format!("reviews/{key}.md.run.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(rec["cwd"], home.join("wt").to_str().unwrap());
    }
    let start = f.bodies("start_unit_review");
    assert_eq!(
        start[0]["parameters"]["prompt"],
        home.join("2.verifier.prompt.md").to_str().unwrap()
    );
    // R24: the frames byte for byte after the page.
    let (_, fixer) = f.launch(3);
    assert!(
        fixer.contains(&format!("\n{FIXER}\n## The round\n\n")),
        "{fixer}"
    );
    let (_, second) = f.launch(4);
    assert!(
        second.contains(&format!("\n{SECOND_LOOK}\n## The round\n\n")),
        "{second}"
    );
}

// spec: specs/ap/unit-run.md#R4
#[test]
fn r4_the_accounts_of_each_stage_by_the_policy_order() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "fix", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    let dir = |n: usize| f.launch(n).0.split('|').next().unwrap().to_owned();
    // `claude/implementer` before `claude`; the fixer has the implementer's accounts and resumes on
    // the record's; `claude/reviewer` for the verifier.
    assert_eq!(
        (dir(1), dir(2), dir(3), dir(4)),
        (f.p("a1"), f.p("a2"), f.p("a1"), f.p("a2"))
    );
    let rec = f.record("1.implementer.md.run.json");
    assert_eq!(rec["account"], f.p("a1"));
    let mut f = F::new();
    f.policy(json!({"accounts": {"claude/opus": [f.p("a3")], "claude/fixer": [f.p("a3")]}}));
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    assert_eq!(f.launch(1).0.split('|').next().unwrap(), f.p("a3"));
}

// spec: specs/ap/unit-run.md#R5
#[test]
fn r5_one_run_per_unit_by_an_advisory_lock() {
    let mut f = F::new();
    std::fs::create_dir_all(f.home()).unwrap();
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(f.home().join("lock"))
        .unwrap();
    held.try_lock().unwrap();
    assert_eq!(code(&f.refusal()), ("unit_run_active", "unit", 2));
    assert!(f.actions().is_empty());
    drop(held);
    // A link at the unit's directory is no directory of its own.
    let mut g = F::new();
    std::fs::create_dir_all(g.t.join("units")).unwrap();
    std::os::unix::fs::symlink(g.t.join("seed"), g.home()).unwrap();
    assert_eq!(code(&g.refusal()), ("worktree_failed", "directory", 2));
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
}

// spec: specs/ap/unit-run.md#R6
#[test]
fn r6_what_the_state_read_decides() {
    let mut f = F::new();
    f.put(
        "hold",
        "h1",
        json!({"status": "active", "effect": "no starts", "intent": oid("intent", "widgets")}),
    );
    assert_eq!(code(&f.refusal()), ("held", "start_unit", 2));
    assert_eq!(
        f.bodies("start_unit")[0]["parameters"]["intent"],
        oid("intent", "widgets")
    );
    // A cut run is taken over without an apply: an active hold on the unit refuses it.
    f.objects.remove(&k("hold", "h1"));
    f.set_unit(json!({"state": "running"}));
    f.put(
        "hold",
        "h2",
        json!({"status": "active", "effect": "no work", "unit": oid("unit", KEY)}),
    );
    let e = f.refusal();
    assert_eq!(code(&e), ("held", "unit", 2));
    assert_eq!(e.detail["Until"], "an invented | condition");
    f.objects.remove(&k("hold", "h2"));
    for (state, want) in [
        ("draft", "unit_not_runnable"),
        ("merged", "unit_not_runnable"),
    ] {
        f.set_unit(json!({"state": state}));
        assert_eq!(code(&f.refusal()), (want, "unit", 2));
    }
    f.set_unit(json!({"state": "accepted", "head": "a".repeat(40), "class": "contract"}));
    let calls = f.actions().len();
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["waits_for"].as_str(), status),
        (Some("accepted"), Some("approve"), 0)
    );
    assert_eq!(f.actions().len(), calls);
    f.set_unit(json!({"state": "stopped", "head": null}));
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    assert_eq!(f.bodies("resume_unit").len(), 1);
}

// spec: specs/ap/unit-run.md#R7
// spec: specs/ap/unit-run.md#R11
#[test]
fn r7_a_call_killed_during_the_implementer_before_its_pull_request() {
    let mut f = F::new();
    f.steps(&["local", "settle", "accept"]);
    f.die_at_launch = Some(1);
    assert_eq!(f.refusal().status, 3);
    let record = f.home().join("1.implementer.md.run.json");
    f.cut(&record);
    let rec: Value = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    let scope = rec["unit"].as_str().unwrap().to_owned();
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("accepted"), 0), "{a}");
    assert!(
        f.calls
            .iter()
            .any(|c| c.0 == "systemctl" && c.1 == ["stop", &scope])
    );
    let run = &f.objects[&k("run", "run-1")];
    assert_eq!(
        (&run["status"], &run["state"], &run["closed_by"]),
        (&json!("closed"), &json!("killed"), &json!("implementer"))
    );
    // The implementer's session resumed with `continue.md`.
    let (line, prompt) = f.launch(2);
    assert!(
        line.contains(&format!("--resume {}", rec["session"].as_str().unwrap())),
        "{line}"
    );
    assert!(prompt.ends_with(CONTINUE), "{prompt}");
}

// spec: specs/ap/unit-run.md#R7
#[test]
fn r7_a_stage_unit_of_another_name_is_not_stopped_and_refuses() {
    let mut f = F::new();
    f.steps(&["local"]);
    f.die_at_launch = Some(1);
    assert_eq!(f.refusal().status, 3);
    let record = f.home().join("1.implementer.md.run.json");
    f.cut(&record);
    let mut rec: Value = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    rec["unit"] = json!("ssh.service");
    std::fs::write(&record, rec.to_string()).unwrap();
    let before = f.calls.len();
    let e = f.refusal();
    assert_eq!(code(&e), ("run_still_active", "record", 2));
    assert!(
        !f.calls[before..]
            .iter()
            .any(|c| c.0 == "systemctl" && c.1[0] == "stop")
    );
}

// spec: specs/ap/unit-run.md#R7
// spec: specs/ap/unit-run.md#R12
#[test]
fn r7_a_call_killed_after_the_implementer_opened_its_pull_request() {
    let mut f = F::new();
    f.steps(&["dirty", "settle", "accept"]);
    f.die_at_launch = Some(1);
    assert_eq!(f.refusal().status, 3);
    f.cut(&f.home().join("1.implementer.md.run.json"));
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted", "{a}");
    let (line, prompt) = f.launch(2);
    assert!(
        line.contains("--resume") && prompt.ends_with(CONTINUE),
        "{line}"
    );
    assert_eq!(f.unit()["head"], json!(f.origin_head()));
}

// spec: specs/ap/unit-run.md#R7
// spec: specs/ap/unit-run.md#R12
// spec: specs/ap/unit-run.md#R19
#[test]
fn r7_a_call_killed_during_the_fixer_resumes_the_repair() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "local", "settle", "accept"]);
    f.die_at_launch = Some(3);
    assert_eq!(f.refusal().status, 3);
    f.cut(&f.home().join("3.fixer.md.run.json"));
    let fixer = f.session_of(3);
    let (a, _) = f.ok();
    assert_eq!(
        (
            a["state"].as_str(),
            a["repairs"].as_u64(),
            a["rounds"].as_u64()
        ),
        (Some("accepted"), Some(1), Some(2)),
        "{a}"
    );
    let (line, prompt) = f.launch(4);
    assert!(
        line.contains(&format!("--resume {fixer}")) && prompt.contains(FIXER),
        "{line}"
    );
    // This call's own fixer moved the head: the verifier who judged it looks again.
    let (line, prompt) = f.launch(5);
    assert!(
        line.contains(&format!("--resume {}", f.session_of(2))) && prompt.contains(SECOND_LOOK),
        "{line}"
    );
}

// spec: specs/ap/unit-run.md#R7
// spec: specs/ap/unit-run.md#R19
#[test]
fn r7_a_call_killed_during_the_verifier_abandons_its_review() {
    let mut f = F::new();
    f.steps(&["implement", "accept", "accept"]);
    f.die_at_launch = Some(2);
    assert_eq!(f.refusal().status, 3);
    let (key, _) = f.packets()[0].clone();
    let record = f.t.join(format!("reviews/{key}.md.run.json"));
    f.cut(&record);
    let session = f.session_of(2);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted", "{a}");
    let first = &f.objects[&k("review", &key)];
    assert_eq!(
        (&first["verdict"], &first["session"]),
        (&json!("abandoned"), &json!(session))
    );
    assert_eq!(f.objects[&k("run", "run-2")]["state"], "killed");
    // The next round is a new session.
    assert!(f.launch(3).0.contains("--session-id"));
    assert_eq!(a["reviews"].as_array().unwrap().len(), 2);
}

// spec: specs/ap/unit-run.md#R7
#[test]
fn r7_a_review_started_and_never_run_closes_with_the_recovery_session() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    f.die_after = Some("start_unit_review");
    assert_eq!(f.refusal().status, 3);
    (f.dead, f.die_after) = (false, None);
    let (key, _) = f.packets()[0].clone();
    std::fs::remove_file(f.t.join(format!("reviews/{key}.md.run.json"))).unwrap();
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted", "{a}");
    assert_eq!(
        f.objects[&k("review", &key)]["session"],
        format!("u-{KEY}:recovery")
    );
}

// spec: specs/ap/unit-run.md#R8
#[test]
fn r8_the_base_is_one_commit_and_the_worktree_is_guarded() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    let base = git(
        &f.t.join("clone"),
        &["rev-parse", "refs/remotes/origin/main"],
    );
    assert_eq!(f.bodies("start_unit_review")[0]["parameters"]["base"], base);
    // No git command of the run names `origin/main`.
    let named = f
        .calls
        .iter()
        .filter(|c| c.0 == "git")
        .any(|c| c.1.iter().any(|a| a == "origin/main"));
    assert!(!named);
    // A worktree that became a link is not used.
    let mut g = F::new();
    g.steps(&["implement", "repair", "nothing"]);
    g.stopped("repair_refused");
    let wt = g.wt();
    std::fs::rename(&wt, g.t.join("moved")).unwrap();
    std::os::unix::fs::symlink(g.t.join("moved"), &wt).unwrap();
    g.set_unit(json!({"state": "stopped"}));
    assert_eq!(
        g.stopped("worktree_failed")["stop_detail"],
        "not a directory"
    );
}

// spec: specs/ap/unit-run.md#R9
#[test]
fn r9_the_read_of_the_hosting_service() {
    // A fork's pull request of the same branch name does not count.
    let mut f = F::new();
    f.extra_prs.push(json!({"number": 9, "headRefOid": "b".repeat(40), "headRefName": format!("unit/{KEY}"),
        "baseRefName": "main", "isCrossRepository": true, "headRepositoryOwner": {"login": "someone"},
        "headRepository": {"name": "widgets"}}));
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["pull_request"], 7);
    // A pull request of the branch the unit did not make.
    let mut f = F::new();
    f.push_from_elsewhere(&format!("unit/{KEY}"), "src/elsewhere.rs");
    std::fs::write(f.t.join("bin/pr-open"), "").unwrap();
    assert!(
        f.stopped("branch_in_use")["stop_detail"]
            .as_str()
            .unwrap()
            .contains("7")
    );
    // Two of the branch from the repository itself.
    let mut f = F::new();
    f.extra_prs.push(json!({"number": 8, "headRefOid": "b".repeat(40), "headRefName": format!("unit/{KEY}"),
        "baseRefName": "main", "isCrossRepository": false, "headRepositoryOwner": {"login": "invented"},
        "headRepository": {"name": "widgets"}}));
    f.steps(&["implement"]);
    f.stopped("pull_request_ambiguous");
    // A pull request closed under a running unit starts no stage.
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    let launches = f.launches;
    f.pr_state = "CLOSED";
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("pull_request_not_open");
    assert_eq!(f.launches, launches);
}

// spec: specs/ap/unit-run.md#R10
#[test]
fn r10_an_existing_branch_must_be_the_units_own() {
    let mut f = F::new();
    let clone = f.t.join("clone");
    git(&clone, &["branch", &format!("unit/{KEY}"), "main"]);
    f.stopped("branch_in_use");
    assert!(!f.wt().exists());
}

// spec: specs/ap/unit-run.md#R11
// spec: specs/ap/unit-run.md#R22
#[test]
fn r11_a_worker_that_opens_no_pull_request_and_a_dirty_worktree() {
    let mut f = F::new();
    let answer = "Ä".repeat(1500);
    std::fs::write(f.t.join("bin/nopr-answer"), &answer).unwrap();
    f.steps(&["nopr"]);
    let a = f.stopped("no_pull_request");
    let detail = a["stop_detail"].as_str().unwrap();
    assert!(
        detail.starts_with("ok: ÄÄ") && detail.len() <= 1000 && detail.len() >= 998,
        "{}",
        detail.len()
    );
    assert_eq!(f.unit()["stop_detail"], detail);
    // The next call resumes the implementer's session; a worktree left dirty without a session is not
    // given to a fresh implementer.
    let mut g = F::new();
    g.steps(&["nopr"]);
    g.stopped("no_pull_request");
    std::fs::remove_file(g.home().join("1.implementer.md.run.json")).unwrap();
    std::fs::write(g.wt().join("stray.txt"), "x").unwrap();
    g.set_unit(json!({"state": "stopped"}));
    let a = g.stopped("worktree_dirty");
    assert!(
        a["stop_detail"].as_str().unwrap().contains("\"changed\":1"),
        "{a}"
    );
    assert_eq!(g.launches, 1);
}

// spec: specs/ap/unit-run.md#R12
// spec: specs/ap/unit-run.md#R21
#[test]
fn r12_a_head_pushed_from_another_clone_is_fast_forwarded_and_judged_anew() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing", "accept"]);
    let a = f.stopped("repair_refused");
    assert!(
        a["stop_detail"]
            .as_str()
            .unwrap()
            .starts_with("ok: Not changed"),
        "{a}"
    );
    let pushed = f.push_from_elsewhere(&format!("unit/{KEY}"), "src/widgets.rs");
    f.set_unit(json!({"state": "stopped"}));
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["head"].as_str()),
        (Some("accepted"), Some(pushed.as_str())),
        "{a}"
    );
    assert_eq!(git(&f.wt(), &["rev-parse", "HEAD"]), pushed);
    assert!(f.calls.iter().any(|c| c.0 == "git"
        && c.1.ends_with(&[
            "merge".into(),
            "--ff-only".into(),
            "--quiet".into(),
            pushed.clone()
        ])));
    let (line, prompt) = f.launch(4);
    assert!(
        line.contains("--session-id") && !prompt.contains(SECOND_LOOK),
        "{line}"
    );
}

// spec: specs/ap/unit-run.md#R13
// spec: specs/ap/unit-run.md#R16
#[test]
fn r13_a_stop_amended_and_run_again_gives_the_author_one_turn() {
    let mut f = F::new();
    f.set_unit(json!({"acceptance": [{"check": "grep -q amended src/widgets.rs", "kind": "fails_on_base"}]}));
    f.steps(&["implement", "fix", "accept"]);
    let a = f.stopped("acceptance_failed");
    assert!(
        a["stop_detail"].as_str().unwrap().contains("\"index\":1"),
        "{a}"
    );
    std::thread::sleep(Duration::from_millis(1100));
    // The Lead amends the page and makes the unit ready again.
    let amended = now();
    f.set_unit(
        json!({"acceptance": [{"check": "test -f src/widgets.rs", "kind": "fails_on_base"}],
        "amended_at": amended, "state": "ready"}),
    );
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["rounds"].as_u64()),
        (Some("accepted"), Some(1)),
        "{a}"
    );
    let (line, prompt) = f.launch(2);
    assert!(
        line.contains(&format!("--resume {}", f.session_of(1))) && prompt.ends_with(CONTINUE),
        "{line}"
    );
    assert_eq!(f.launches, 3);
}

// spec: specs/ap/unit-run.md#R14
#[test]
fn r14_the_writers_hold_every_session_of_every_author_attempt() {
    let mut f = F::new();
    // The implementer's first account is cut by a usage limit after work; the stage moves to the next.
    f.policy(json!({"accounts": {"claude/implementer": [f.p("a1"), f.p("a3")], "claude/fixer": [f.p("a1"), f.p("a3")], "claude/reviewer": [f.p("a2")]}}));
    std::fs::write(f.t.join("bin/mode-a1"), "limit45").unwrap();
    f.steps(&["", "implement", "repair", "fix", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted", "{a}");
    let rec = f.record("1.implementer.md.run.json");
    let sessions: Vec<Value> = rec["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["session"].clone())
        .collect();
    assert_eq!(sessions.len(), 2);
    let writers = f.unit()["writers"].clone();
    for s in &sessions {
        assert!(writers.as_array().unwrap().contains(s), "{writers} {s}");
    }
    let heads = f.bodies("record_head");
    assert_eq!(heads.len(), 2);
    assert_eq!(heads[1]["parameters"]["rounds"], 1);
    // No session of a writer closed a review of the unit.
    for (_, r) in f.packets() {
        assert!(!writers.as_array().unwrap().contains(&r["session"]), "{r}");
    }
}

// spec: specs/ap/unit-run.md#R15
#[test]
fn r15_a_head_behind_main_stops() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    f.push_from_elsewhere("main", "src/main-moved.rs");
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("behind_main");
}

// spec: specs/ap/unit-run.md#R16
#[test]
fn r16_the_write_set_and_the_checks_on_the_head() {
    let mut f = F::new();
    f.steps(&["outside"]);
    let a = f.stopped("path_outside_write_set");
    assert!(
        a["stop_detail"].as_str().unwrap().contains("notes.md"),
        "{a}"
    );
    assert!(f.packets().is_empty());
    // The checks see the merge base and the head; one that writes leaves the worktree dirty.
    let check = "test \"$AP_HEAD\" = \"$(git rev-parse HEAD)\" && git merge-base --is-ancestor \"$AP_BASE\" \"$AP_HEAD\" && test \"$AP_BASE\" != \"$AP_HEAD\"";
    let mut f = F::new();
    f.set_unit(json!({"acceptance": [{"check": check, "kind": "fails_on_base"}, {"check": "touch written", "kind": "invariant"}]}));
    f.steps(&["implement"]);
    f.stopped("worktree_dirty");
    for (check, cause) in [
        ("exit 124", "acceptance_timeout"),
        ("exit 3", "acceptance_failed"),
    ] {
        let mut f = F::new();
        f.set_unit(json!({"acceptance": [{"check": check, "kind": "invariant"}]}));
        f.steps(&["implement"]);
        f.stopped(cause);
    }
}

// spec: specs/ap/unit-run.md#R17
#[test]
fn r17_the_required_check_and_the_review_threads() {
    let mut f = F::new();
    f.check = "FAILURE";
    f.steps(&["implement"]);
    f.stopped("check_failed");
    let mut f = F::new();
    f.threads.push(json!({"id": "T1", "isResolved": false, "path": "src/widgets.rs", "line": 1,
        "first": {"totalCount": 1, "nodes": [{"author": {"login": "someone"}, "originalCommit": {"oid": "x"}}]},
        "last": {"nodes": [{"author": {"login": "someone"}}]}}));
    f.steps(&["implement"]);
    assert!(
        f.stopped("unresolved_threads")["stop_detail"]
            .as_str()
            .unwrap()
            .contains("T1")
    );
    assert!(f.packets().is_empty());
}

// spec: specs/ap/unit-run.md#R18
// spec: specs/ap/unit-run.md#R21
#[test]
fn r18_two_repairs_inside_one_call_and_the_third_is_the_limit() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "fix", "repair", "fix", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (
            a["state"].as_str(),
            status,
            a["rounds"].as_u64(),
            a["repairs"].as_u64()
        ),
        (Some("accepted"), 0, Some(3), Some(2)),
        "{a}"
    );
    let rounds: Vec<u64> = a["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["round"].as_u64().unwrap())
        .collect();
    assert_eq!(rounds, [1, 2, 3]);
    let accepted = f.bodies("accept_unit");
    assert_eq!(accepted[0]["parameters"]["head"], json!(f.origin_head()));
    let mut f = F::new();
    f.steps(&[
        "implement",
        "repair",
        "fix",
        "repair",
        "fix",
        "repair",
        "fix",
    ]);
    let a = f.stopped("repair_limit");
    assert_eq!((a["repairs"].as_u64(), f.launches), (Some(3), 6), "{a}");
}

// spec: specs/ap/unit-run.md#R19
#[test]
fn r19_a_report_without_the_form_abandons_the_review() {
    let mut f = F::new();
    f.steps(&["implement", "noform", "accept"]);
    f.stopped("verifier_failed");
    assert_eq!(f.packets()[0].1["verdict"], "abandoned");
    f.set_unit(json!({"state": "stopped"}));
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted");
    // The abandoned review's session is not asked again.
    assert!(f.launch(3).0.contains("--session-id"));
    let start = f.bodies("start_unit_review");
    let p = &start[1]["parameters"];
    assert_eq!(
        (
            &p["kind"],
            &p["round"],
            &p["pull_request"],
            &p["repository"]
        ),
        (
            &json!("packet"),
            &json!(1),
            &json!(7),
            &json!("invented/widgets")
        )
    );
    assert_eq!(p["task"]["native_key"], "invented-task");
}

// spec: specs/ap/unit-run.md#R20
// spec: specs/ap/unit-run.md#R23
#[test]
fn r20_a_verifier_that_pushes_becomes_a_writer_and_never_accepts() {
    let mut f = F::new();
    f.steps(&["implement", "push"]);
    let a = f.stopped("verifier_wrote");
    let verifier = f.session_of(2);
    assert!(
        f.unit()["writers"]
            .as_array()
            .unwrap()
            .contains(&json!(verifier))
    );
    assert_eq!(f.unit()["head"], json!(f.origin_head()));
    assert_eq!(f.packets()[0].1["verdict"], "abandoned", "{a}");
    let order: Vec<String> = f
        .actions()
        .into_iter()
        .filter(|a| a != "start_run" && a != "close_run")
        .collect();
    assert_eq!(
        order[order.len() - 3..],
        ["close_unit_review", "record_head", "stop_unit"]
    );
    // One that writes in the worktree without a push.
    let mut f = F::new();
    f.steps(&["implement", "write"]);
    f.stopped("verifier_wrote");
    // R23: nothing approved, merged or pushed by the run itself.
    for a in f.actions() {
        assert!(!["approve_unit", "merge_unit"].contains(&a.as_str()));
    }
    assert!(
        !f.calls
            .iter()
            .any(|c| c.0 == "git" && c.1.iter().any(|a| a == "push"))
    );
    assert!(!f.calls.iter().any(|c| c.0 == "gh" && c.1[1] == "merge"));
}

// spec: specs/ap/unit-run.md#R20
#[test]
fn r20_a_fixer_head_that_never_shows_on_the_hosting_service() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "unpushed"]);
    f.stopped("head_not_seen");
    assert!(f.clock >= Duration::from_secs(120));
}

// spec: specs/ap/unit-run.md#R22
#[test]
fn r22_a_refusal_of_the_store_in_the_middle_ends_the_call() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    f.refuse = Some(("record_head", "unit_not_running"));
    let e = f.refusal();
    assert_eq!((e.code.as_str(), e.status), ("store_refused", 3), "{e:?}");
    assert_eq!(e.detail["failed"], json!(["unit_not_running"]));
    assert!(!f.actions().contains(&"stop_unit".to_owned()));
    assert_eq!(f.launches, 1);
}

// spec: specs/ap/unit-run.md#R25
#[test]
fn r25_the_answer() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(status, 0);
    let keys: BTreeSet<&str> = a.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "unit",
            "state",
            "head",
            "pull_request",
            "rounds",
            "repairs",
            "reviews",
            "waits_for"
        ])
    );
    assert_eq!(
        (a["waits_for"].as_str(), a["head"].clone()),
        (Some("merge"), json!(f.origin_head()))
    );
    let r = &a["reviews"][0];
    assert_eq!(
        (&r["round"], &r["head"], &r["verdict"]),
        (&json!(1), &a["head"], &json!("accept"))
    );
    assert_eq!(f.unit()["state"], "accepted");
    // The binary prints a refusal with the key.
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["unit", "run", "Bad"])
        .env_remove("ONT_ROLE")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (out.status.code(), &e["error"]["code"], &e["error"]["unit"]),
        (Some(2), &json!("invalid_input"), &json!("Bad"))
    );
}

// spec: specs/ap/unit-run.md#R23
#[test]
fn r23_the_run_stops_only_units_of_a_stage_its_records_name() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    f.die_at_launch = Some(2);
    assert_eq!(f.refusal().status, 3);
    let (key, _) = f.packets()[0].clone();
    f.cut(&f.t.join(format!("reviews/{key}.md.run.json")));
    let names = f.alive.clone();
    let before = f.calls.len();
    f.steps(&["implement", "accept", "accept"]);
    assert_eq!(f.ok().0["state"], "accepted");
    let stopped: Vec<&String> = f.calls[before..]
        .iter()
        .filter(|c| c.0 == "systemctl" && c.1[0] == "stop")
        .map(|c| &c.1[1])
        .collect();
    // The cut verifier's unit, the units `ap run` stops at each stage's end and a check's own unit
    // stopped by its runner: each of the form its runner gives.
    assert!(stopped.iter().any(|s| names.contains(*s)), "{stopped:?}");
    for s in stopped {
        let stage = s.starts_with("ap-run-") && s.ends_with(".scope");
        assert!(stage || s.starts_with("ap-check-"), "{s}");
    }
    // The run removes nothing: every prompt and record of every stage is still there.
    assert!(
        f.home().join("1.implementer.md").exists()
            && f.home().join("2.verifier.prompt.md").exists()
    );
}
