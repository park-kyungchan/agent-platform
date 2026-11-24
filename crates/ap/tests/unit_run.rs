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
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
#[path = "fixtures/work/forks.rs"]
mod forks;
use ap::host::{Answer, Failure, Host, Reply, System};
use ap::unit::drive::{Refusal, arguments, decide, execute};
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

mod roles_file;
static NEXT: AtomicU32 = AtomicU32::new(0);
const FAKE: &str = include_str!("fixtures/fake-claude.sh");
const STAGE: &str = include_str!("fixtures/unit_run/stage.sh");
const FIXER: &str = include_str!("../src/unit/frames/fixer.md");
const SECOND_LOOK: &str = include_str!("../src/unit/frames/second-look.md");
const CONTINUE: &str = include_str!("../src/unit/frames/continue.md");
const STORE: &str = "/invented/scratch/store";
const BWRAP: &str = "/usr/bin/bwrap";
/// The words of the driver's Git calls before the command word (R39).
const SAFE: [&str; 4] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
];
const KEY: &str = "widgets-list";
/// The fixed script of the App's signature (R42).
const SIGN: &str = r#"printf %s "$1" | openssl dgst -sha256 -sign "$2" -binary"#;
/// What the fake `openssl` signs with, the installation the fake service names and the installation
/// token it makes: all invented.
const SIGNATURE: &[u8] = b"invented-signature-bytes";
const INSTALLATION: u64 = 77;
const INSTALLATION_TOKEN: &str = "ghs_InventedInstallation42";
/// The App id of the tests' policy: invented.
const APP_ID: u64 = 424_242;

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://invented","account":"root"},"native_key":key})
}
/// A failed call of `gh` that says `text` on its standard error.
fn said(text: &str) -> Answer {
    Ok(Reply {
        success: false,
        bytes: text.as_bytes().to_vec(),
        stdout: vec![],
    })
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
    let out = forks::forking(|| {
        Command::new("git")
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
    })
    .unwrap();
    assert!(out.status.success(), "git {words:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn kill_group(pgid: u32) {
    let _ = forks::forking(|| {
        Command::new("kill")
            .args(["-KILL", "--", &format!("-{pgid}")])
            .stderr(std::process::Stdio::null())
            .status()
    });
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
    /// The answer of the function `capacity` (`None`: the call fails), and the number of its calls.
    capacity: Option<Value>,
    capacity_calls: usize,
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
    /// The walk: the real engine on a scratch store, instead of the fake store.
    engine: Option<String>,
    /// The merge of the pull request on the hosting service: its head and its merge commit.
    merged: Option<(String, String)>,
    /// What `gh pr merge` does: `ok`, `fail` (nothing merged, the call fails), `unconfirmed` (the call
    /// succeeds, nothing merged), `cut` (merged, then the store dies).
    merge_mode: &'static str,
    /// Every status posted: the head, the state, the description.
    statuses: Vec<(String, String, String)>,
    /// The head branch the pull request names.
    head_ref: String,
    /// The base branch the pull request names.
    base_ref: String,
    /// At the run's push, the store dies and the push fails.
    die_at_push: bool,
    /// After the validate of `merge_unit`, the unit is dropped by another call.
    drop_after_validate: bool,
    /// The next fetches of `main` fail, each with the next message of Git.
    fetch_fails: Vec<&'static str>,
    /// What a failed `gh` call of the fake says (`pr view` with `gh-down`, `pr merge`).
    gh_says: &'static str,
    /// The state `lead-review` reads on the head, in place of the newest status posted on it.
    lead_review: Option<&'static str>,
    /// The `bwrap … --` of each launch, dropped before the fake CLI starts (R38).
    wraps: Vec<Vec<String>>,
    /// The environment of each launch.
    launch_envs: Vec<Vec<(String, String)>>,
    /// The `bwrap … --` of each check's command, dropped before it runs (`run-limits.md` R12).
    check_wraps: Vec<Vec<String>>,
    /// From this launch on, the host has no units (a check runs by plain `sh`).
    no_units_after: Option<usize>,
    /// The body of the pull request (`pr view --json body`); `None`: an answer without `body`.
    body: Option<String>,
    /// The read of the body fails.
    body_fails: bool,
    /// Every `gh pr comment`: its words.
    comments: Vec<Vec<String>>,
    /// Every `gh pr comment` fails.
    comment_fails: bool,
    /// The step of the App's status that fails (R42): `openssl`, `installation`, `access_tokens`,
    /// `status`; with `-missing` after it, the answer lacks its member.
    app_fails: Option<&'static str>,
    /// The `Authorization` header of each status posted, when it had one.
    status_headers: Vec<Option<String>>,
    /// The port's wall clock, when the test fixes it.
    wall: Option<SystemTime>,
}
impl F {
    fn new() -> Self {
        forks::gap();
        let t = std::env::temp_dir().join(format!(
            "ap-unit-run-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "a3", "seed", "secrets"] {
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
            capacity: None,
            capacity_calls: 0,
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
            engine: None,
            merged: None,
            merge_mode: "ok",
            statuses: vec![],
            head_ref: format!("unit/{KEY}"),
            base_ref: "main".into(),
            die_at_push: false,
            drop_after_validate: false,
            fetch_fails: vec![],
            gh_says: "{}",
            lead_review: None,
            wraps: vec![],
            launch_envs: vec![],
            check_wraps: vec![],
            no_units_after: None,
            body: Some("An invented description of the change.\n".into()),
            body_fails: false,
            comments: vec![],
            comment_fails: false,
            app_fails: None,
            status_headers: vec![],
            wall: None,
        };
        f.policy(json!({}));
        f.put("task", "invented-task", json!({"status": "open"}));
        f.put("intent", "widgets", json!({"status": "confirmed", "lead_session": "lead-1",
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
            "units": {"directory": self.p("units"), "github": self.github(None)}});
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
    /// `units.github`: the Lead's App (an invented id and key path), and `stage_tokens` when given.
    fn github(&self, tokens: Option<Value>) -> Value {
        let mut g = json!({"lead_app": {"id": APP_ID, "key": self.p("secrets/app.pem")}});
        if let Some(t) = tokens {
            g["stage_tokens"] = t;
        }
        g
    }
    /// The stage token of `invented/widgets` in its own file, named by `units.github.stage_tokens`.
    fn token(&self, text: &str) -> String {
        let path = self.p("secrets/widgets.token");
        std::fs::write(&path, text).unwrap();
        let tokens = json!({"invented/widgets": path});
        let units = json!({"directory": self.p("units"), "github": self.github(Some(tokens))});
        self.policy(json!({"units": units}));
        path
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
        if self.engine.is_some() {
            return self.fields("unit", KEY);
        }
        self.objects[&k("unit", KEY)].clone()
    }
    fn set_unit(&mut self, set: Value) {
        self.modify("unit", KEY, set);
    }
    fn run(&mut self) -> Result<(Value, u8), Box<Refusal>> {
        forks::settled();
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
        if self.engine.is_some() {
            let view = self.engine_call(
                &["render"],
                &json!({"view": "reviews_of_unit", "args": {"unit": KEY}}),
            );
            let keys = view["markdown"]
                .as_str()
                .unwrap()
                .lines()
                .skip_while(|l| !l.starts_with("| --- |"))
                .skip(1)
                .take_while(|l| l.starts_with("| "));
            let keys: Vec<String> = keys
                .map(|l| l[2..].split(' ').next().unwrap().to_owned())
                .collect();
            let mut list: Vec<(String, Value)> = keys
                .into_iter()
                .map(|k| {
                    let v = self.fields("review", &k);
                    (k, v)
                })
                .collect();
            list.retain(|(_, v)| v["kind"] == "packet");
            list.sort_by_key(|(k, v)| (v["started_at"].as_str().unwrap().to_owned(), k.clone()));
            return list;
        }
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
        let out = forks::forking(|| {
            Command::new("git")
                .arg("-C")
                .arg(self.t.join("origin.git"))
                .args([
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/unit/{KEY}"),
                ])
                .output()
        })
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
        let known = forks::forking(|| {
            Command::new("git")
                .arg("-C")
                .arg(&other)
                .args(["rev-parse", "--verify", "--quiet", &theirs])
                .output()
        })
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
        let child =
            forks::forking(|| Command::new("sleep").arg("600").process_group(0).spawn()).unwrap();
        self.groups.insert(unit.clone(), child.id());
        self.sleepers.push(child);
        self.alive.insert(unit);
        (self.dead, self.die_at_launch, self.die_after) = (false, None, None);
    }
    /// `ap unit approve` or `reject` by the session `session`: the answer, or the refusal and `after`.
    fn decide(
        &mut self,
        op: &str,
        session: &str,
        mut r: Value,
    ) -> Result<Value, (Failure, Option<String>)> {
        for (k, v) in [
            ("agent", json!("lead")),
            ("session", json!(session)),
            ("unit", json!(KEY)),
        ] {
            if r.get(k).is_none() {
                r[k] = v;
            }
        }
        let mut after = None;
        forks::settled();
        decide(self, Some(STORE), op, &r, &mut after).map_err(|f| (f, after))
    }
    /// The index of the first call that `found` matches.
    fn first(&self, found: impl Fn(&(String, Vec<String>, Value)) -> bool) -> Option<usize> {
        self.calls.iter().position(found)
    }
    /// The merges: `gh pr merge` calls, and the update's own merges.
    fn merges(&self) -> (usize, usize) {
        let gh = self
            .calls
            .iter()
            .filter(|c| c.0 == "gh" && c.1[..2] == ["pr", "merge"]);
        let own = self
            .calls
            .iter()
            .filter(|c| c.0 == "env" && c.1.iter().any(|a| a == "merge"));
        (gh.count(), own.count())
    }
    fn origin(&self, words: &[&str]) -> String {
        git(&self.t.join("origin.git"), words)
    }
    /// The run made a stage's check after call `from`.
    fn checks_since(&self, from: usize) -> usize {
        self.calls[from..]
            .iter()
            .filter(|c| c.0 == "systemd-run")
            .count()
    }
    fn record(&self, rel: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.home().join(rel)).unwrap()).unwrap()
    }

    /// The walk's fixture: the store of `AP_UNIT_RUN_TEMPLATE` copied, its task in the policy.
    fn engine() -> Self {
        let mut f = Self::new();
        let template =
            PathBuf::from(std::env::var("AP_UNIT_RUN_TEMPLATE").expect("AP_UNIT_RUN_TEMPLATE"));
        std::fs::create_dir(f.t.join("store")).unwrap();
        for name in ["store.sqlite", "edits.jsonl"] {
            std::fs::copy(template.join(name), f.t.join("store").join(name)).unwrap();
        }
        // The roles file beside the copied store: this uid may act as every role of the grants.
        roles_file::put(
            &f.t.join("store"),
            &serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap(),
        );
        f.engine = Some(f.p("store/store.sqlite"));
        f.policy(
            json!({"reviews": {"task": "ontology-operational-base", "directory": f.p("reviews")}}),
        );
        f
    }
    fn engine_call(&self, args: &[&str], body: &Value) -> Value {
        let store = self.engine.as_deref().unwrap();
        let out = forks::forking(|| {
            Command::new("ont")
                .arg("--store")
                .arg(store)
                .args(args)
                .env_remove("ONT_ROLE")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
        })
        .unwrap();
        let mut out = out;
        std::io::Write::write_all(out.stdin.as_mut().unwrap(), body.to_string().as_bytes())
            .unwrap();
        let out = out.wait_with_output().unwrap();
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        v.get("result").cloned().unwrap_or(v)
    }
    /// An id of the walk's store: its namespace is the engine's.
    fn eid(&self, kind: &str, key: &str) -> Value {
        let units = json!({"object_type": "unit", "where": {"all": [{"in": ["lane", ["small", "objective"]]}]}, "limit": 1});
        let found = self.engine_call(&["search"], &units);
        json!({"object_type": kind, "namespace": found["objects"][0]["id"]["namespace"], "native_key": key})
    }
    fn fields(&self, kind: &str, key: &str) -> Value {
        let v = self.engine_call(&["get"], &json!({"id": self.eid(kind, key)}));
        let o = &v["objects"][0]["observations"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["fields"];
        o.as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v["value"].clone()))
            .collect()
    }
    /// An action of the Lead's side of the walk (another agent and session than the run's), applied.
    fn engine_apply(&mut self, action: &str, mut p: Value) -> Value {
        for k in ["unit", "task", "subject"] {
            if let Some(key) = p[k]["native_key"].as_str().map(str::to_owned) {
                // A subject carries its own type.
                let kind = p[k]["object_type"].as_str().filter(|_| k == "subject").unwrap_or(k);
                p[k] = self.eid(kind, &key);
            }
        }
        let rid = format!("walk-{}", NEXT.fetch_add(1, Ordering::SeqCst));
        let body = json!({"action_type": action, "request_id": rid, "agent": "walk", "session": "walk-1", "role": "lead", "parameters": p});
        let v = self.engine_call(&["action", "apply", "--client", "walk"], &body);
        assert_eq!(v["outcome"], "applied", "{action}: {v}");
        v
    }
    /// `ready_unit` on the walk's unit, with its intent when it has one.
    fn engine_ready(&mut self) {
        let mut p = json!({"unit": oid("unit", KEY)});
        if let Some(i) = self.unit().get("intent").filter(|i| !i.is_null()) {
            p["intent"] = i.clone();
        }
        self.engine_apply("ready_unit", p);
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
        if let Some(store) = &self.engine {
            let mut a = args.to_vec();
            *a.last_mut().unwrap() = store.clone();
            return forks::forking(|| Host.call("ont", &a, input, 60_000));
        }
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        match args[0].as_str() {
            "fn" if args[1] == "capacity" => {
                self.capacity_calls += 1;
                match self.capacity.clone() {
                    Some(answer) => reply(
                        true,
                        &json!({"function": "capacity", "domain": "work", "answer": answer, "elapsed_ms": 1}),
                    ),
                    None => reply(false, &json!({"status": "refused", "code": "not_granted"})),
                }
            }
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
            // Schema 24: the take-over reads the active holds by one search of `hold` by `status`.
            "search" if input["object_type"] == "hold" => {
                assert_eq!(input["where"], json!({"all": [{"eq": ["status", "active"]}]}));
                let found: Vec<Value> = (self.objects.iter())
                    .filter(|((t, _), v)| t == "hold" && v["status"] == "active")
                    .map(|((_, key), h)| json!({"id": oid("hold", key), "head": {"fields": Self::wrap(h)}}))
                    .collect();
                reply(
                    true,
                    &json!({"status": "ok", "now": epoch, "result": {"basis": {"qualified": true}, "objects": found, "continuation": null}}),
                )
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
        if body["action_type"] == "merge_unit" {
            let failed = self.merge_failed(&body["parameters"]);
            let criteria: Vec<Value> = [
                "unit_not_accepted",
                "head_differs",
                "head_not_reviewed",
                "intent_parameter_required",
                "held",
                "not_approved",
            ]
            .iter()
            .map(|c| json!({"code": c, "passed": !failed.contains(c)}))
            .collect();
            if self.drop_after_validate {
                self.set_unit(json!({"state": "dropped", "drop_reason": "an invented drop"}));
            }
            return reply(
                true,
                &json!({"status": "ok", "result": {"valid": failed.is_empty(), "criteria": criteria, "edits": []}}),
            );
        }
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
    /// The criteria of `merge_unit` that fail.
    fn merge_failed(&self, p: &Value) -> Vec<&'static str> {
        let u = self.unit();
        let mut failed = vec![];
        if u["state"] != "accepted" {
            failed.push("unit_not_accepted");
        }
        if p["head"] != u["head"] {
            failed.push("head_differs");
        }
        if !self.reviewed(&p["head"], &u) {
            failed.push("head_not_reviewed");
        }
        if u["lane"] == "objective" && p["intent"].is_null() {
            failed.push("intent_parameter_required");
        }
        if self.held(&u) {
            failed.push("held");
        }
        if (u["class"] == "contract" || u["class"] == "effect") && u["approved_head"] != p["head"] {
            failed.push("not_approved");
        }
        failed
    }
    fn reviewed(&self, head: &Value, u: &Value) -> bool {
        self.packets().iter().any(|(_, r)| {
            r["head"] == *head
                && r["status"] == "closed"
                && r["verdict"] == "accept"
                && r["started_at"].as_str() > u["amended_at"].as_str()
        })
    }
    fn held(&self, u: &Value) -> bool {
        self.objects.iter().any(|((t, _), h)| {
            t == "hold"
                && h["status"] == "active"
                && (h["subject"] == oid("unit", KEY) || h["subject"] == u["intent"])
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
                    if !self.reviewed(&p["head"], &u) {
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
            "reopen_unit" => {
                if state != "accepted" {
                    failed.push("unit_not_accepted");
                }
                if p["intent"] != u["intent"] {
                    failed.push("intent_differs");
                }
                if self.held(&u) {
                    failed.push("held");
                }
                if failed.is_empty() {
                    self.set_unit(json!({"state": "running"}));
                }
                json!([{"change": "update", "id": oid("unit", KEY)}])
            }
            "merge_unit" => {
                failed = self.merge_failed(p);
                if failed.is_empty() {
                    self.set_unit(
                        json!({"state": "merged", "merged_commit": p["merged_commit"],
                        "ended_at": now}),
                    );
                }
                json!([{"change": "update", "id": oid("unit", KEY)}])
            }
            "approve_unit" => {
                if state != "accepted" {
                    failed.push("unit_not_accepted");
                }
                if body["role"] != "lead" {
                    failed.push("not_the_lead");
                }
                if p["head"] != u["head"] {
                    failed.push("head_differs");
                }
                let expected = &body["expected"][0];
                let intent = k(
                    "intent",
                    expected["id"]["native_key"].as_str().unwrap_or_default(),
                );
                let revision = self.revisions.get(&intent).map(|r| format!("r#{r}"));
                if revision.as_deref() != expected["revision"].as_str() {
                    return reply(
                        false,
                        &json!({"status": "refused", "code": "revision_conflict"}),
                    );
                }
                if failed.is_empty() {
                    self.set_unit(json!({"approved_head": p["head"]}));
                }
                json!([{"change": "update", "id": oid("unit", KEY)}])
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
            "carry_review" => {
                let from = p["review"]["native_key"].as_str().unwrap().to_owned();
                let r = self.objects[&k("review", &from)].clone();
                let accepting =
                    r["status"] == "closed" && r["verdict"] == "accept" && r["kind"] == "packet";
                if !accepting {
                    failed.push("review_not_accepting");
                }
                let members = ["repository", "unit", "report", "report_sha256"];
                if members.iter().any(|m| r[*m].is_null()) {
                    failed.push("review_incomplete");
                }
                if r["started_at"].as_str() <= u["amended_at"].as_str() {
                    failed.push("review_older_than_amendment");
                }
                if r["unit"] != p["unit"] {
                    failed.push("unit_differs");
                }
                if r["head"] == p["head"] {
                    failed.push("head_unchanged");
                }
                let key = Self::made_key(
                    body["agent"].as_str().unwrap(),
                    body["request_id"].as_str().unwrap(),
                );
                if failed.is_empty() {
                    let mut c = json!({"kind": "packet", "head": p["head"], "status": "closed",
                        "verdict": "accept", "started_at": now, "closed_at": now, "carried_from": p["review"]});
                    for m in [
                        "task",
                        "model",
                        "effort",
                        "prompt",
                        "prompt_sha256",
                        "repository",
                        "unit",
                        "report",
                        "report_sha256",
                    ] {
                        c[m] = r[m].clone();
                    }
                    self.put("review", &key, c);
                }
                json!([{"change": "initial", "id": oid("review", &key)}])
            }
            "start_run" => {
                assert_eq!(body["agent"], "ap-run");
                let key = format!(
                    "run-{}",
                    self.objects.keys().filter(|(t, _)| t == "run").count() + 1
                );
                let mut r = p.clone();
                (r["status"], r["started_at"], r["role"]) =
                    (json!("running"), json!(now), p["role"].clone());
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
                if self.t.join("bin/gh-down").exists() {
                    return said(self.gh_says);
                }
                // The description a verifier round reads (R19).
                if a[3..] == ["--repo", "invented/widgets", "--json", "body"] {
                    if self.body_fails {
                        return said(self.gh_says);
                    }
                    return ok(match &self.body {
                        Some(b) => json!({"body": b}),
                        None => json!({"title": "An invented title."}),
                    });
                }
                let (state, head, commit) = match &self.merged {
                    Some((h, c)) => ("MERGED", json!(h), json!({"oid": c})),
                    None if self.t.join("bin/pr-closed").exists() => {
                        ("CLOSED", json!(head), Value::Null)
                    }
                    None => (self.pr_state, json!(head), Value::Null),
                };
                ok(
                    json!({"state": state, "baseRefName": self.base_ref, "headRefOid": head, "headRefName": self.head_ref,
                    "isCrossRepository": false, "headRepositoryOwner": {"login": "invented"},
                    "headRepository": {"name": "widgets"}, "mergeCommit": commit}),
                )
            }
            ("pr", "merge") => {
                let pinned = a
                    .windows(2)
                    .find(|w| w[0] == "--match-head-commit")
                    .unwrap()[1]
                    .clone();
                match self.merge_mode {
                    "fail" => return said(self.gh_says),
                    "fail_once" => {
                        self.merge_mode = "ok";
                        return said(self.gh_says);
                    }
                    "unconfirmed" => return ok(json!({})),
                    _ => {}
                }
                let origin = self.t.join("origin.git");
                let main = git(&origin, &["rev-parse", "refs/heads/main"]);
                let behind = !forks::forking(|| {
                    Command::new("git")
                        .arg("-C")
                        .arg(&origin)
                        .args(["merge-base", "--is-ancestor", &main, &pinned])
                        .status()
                })
                .unwrap()
                .success();
                if head.as_deref() != Some(pinned.as_str()) || behind || self.merged.is_some() {
                    return reply(false, &json!({}));
                }
                let tree = format!("{pinned}^{{tree}}");
                let commit = git(
                    &origin,
                    &[
                        "commit-tree",
                        &tree,
                        "-p",
                        &main,
                        "-p",
                        &pinned,
                        "-m",
                        "Merge pull request #7",
                    ],
                );
                git(&origin, &["update-ref", "refs/heads/main", &commit]);
                self.merged = Some((pinned, commit));
                if self.merge_mode == "cut" {
                    self.dead = true;
                }
                ok(json!({}))
            }
            // The read of the head's statuses: the newest `lead-review` posted on it.
            ("api", path) if path.ends_with("/status") => {
                assert_eq!(a.len(), 2, "a read posts nothing: {a:?}");
                let head = path.split('/').nth(4).unwrap();
                let posted = self.statuses.iter().rev().find(|st| st.0 == head);
                let state = self.lead_review.or(posted.map(|st| st.1.as_str()));
                let list: Vec<Value> = state
                    .map(|st| json!({"context": "lead-review", "state": st}))
                    .into_iter()
                    .collect();
                ok(json!({"state": "success", "statuses": list}))
            }
            // The report as a comment (R43).
            ("pr", "comment") => {
                self.comments.push(a.to_vec());
                match self.comment_fails {
                    true => said("HTTP 502: an invented failure"),
                    false => ok(json!({})),
                }
            }
            // The App's installation, its token and a status, by the path the call names (R42).
            ("api", "--method" | "-H") if !a.iter().any(|w| w == "graphql") => {
                let path = a
                    .iter()
                    .find(|w| w.starts_with("repos/") || w.starts_with("app/"))
                    .unwrap()
                    .clone();
                let header = a.windows(2).find(|w| w[0] == "-H").map(|w| w[1].clone());
                let fails = |step: &str| self.app_fails == Some(step);
                let missing = |step: &str| {
                    self.app_fails
                        .is_some_and(|f| f == format!("{step}-missing"))
                };
                if path.ends_with("/installation") {
                    return match () {
                        _ if fails("installation") => said(self.gh_says),
                        _ if missing("installation") => ok(json!({"message": "invented"})),
                        _ => ok(json!({"id": INSTALLATION, "app_id": APP_ID})),
                    };
                }
                if path.starts_with("app/installations/") {
                    return match () {
                        _ if fails("access_tokens") => said(self.gh_says),
                        _ if missing("access_tokens") => ok(json!({"token": ""})),
                        _ => ok(
                            json!({"token": INSTALLATION_TOKEN, "expires_at": "2099-01-01T00:00:00Z"}),
                        ),
                    };
                }
                if fails("status") {
                    return said(self.gh_says);
                }
                let head = path.rsplit('/').next().unwrap().to_owned();
                let field = |k: &str| {
                    let f = a.iter().find_map(|w| w.strip_prefix(&format!("{k}=")));
                    f.unwrap().to_owned()
                };
                self.statuses
                    .push((head, field("state"), field("description")));
                self.status_headers.push(header);
                ok(json!({}))
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
            "git"
                if !self.fetch_fails.is_empty()
                    && args.iter().any(|a| a == "fetch")
                    && args.last().is_some_and(|a| a == "main") =>
            {
                let says = self.fetch_fails.remove(0).as_bytes().to_vec();
                Ok(Reply {
                    success: false,
                    bytes: says,
                    stdout: vec![],
                })
            }
            "git" if self.die_at_push && args.iter().any(|a| a == "push") => {
                self.dead = true;
                Ok(Reply {
                    success: false,
                    bytes: vec![],
                    stdout: vec![],
                })
            }
            // The App's signature (R42): the fake `openssl` signs with invented bytes.
            "sh" if args.get(1).map(String::as_str) == Some(SIGN) => {
                let fails = self.app_fails == Some("openssl");
                let empty = self.app_fails == Some("openssl-missing");
                let stdout = match fails || empty {
                    true => vec![],
                    false => SIGNATURE.to_vec(),
                };
                let bytes = match fails {
                    true => b"invented: unable to load key".to_vec(),
                    false => stdout.clone(),
                };
                Ok(Reply {
                    success: !fails,
                    bytes,
                    stdout,
                })
            }
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
            // A check's unit: the command after `--`, run here (no unit is started), its view
            // (`bwrap … --`) kept for the tests and dropped.
            "systemd-run" => {
                let at = args.iter().position(|a| a == "--").unwrap();
                let mut command = args[at + 1..].to_vec();
                let mut wrap = vec![];
                if let Some(from) = command.iter().position(|a| a == BWRAP) {
                    let to = from + command[from..].iter().position(|a| a == "--").unwrap();
                    wrap = command.drain(from..=to).collect();
                }
                self.check_wraps.push(wrap);
                forks::forking(|| Host.call(&command[0], &command[1..], input, ms))
            }
            // The probe of `limits`: no `bwrap` of the host is called.
            BWRAP => {
                assert_eq!(args, ["--version"]);
                Ok(Reply {
                    success: true,
                    bytes: vec![],
                    stdout: b"bubblewrap 0.0.0\n".to_vec(),
                })
            }
            _ => forks::forking(|| Host.call(program, args, input, ms)),
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
        // `bwrap … --` in front of the CLI is kept for the tests and dropped here (R38).
        let mut at = at;
        let mut wrap = vec![];
        if argv[at + 1] == BWRAP {
            let end = at + 1 + argv[at + 1..].iter().position(|a| a == "--").unwrap();
            wrap = argv[at + 1..=end].to_vec();
            at = end;
        }
        self.wraps.push(wrap);
        self.launch_envs.push(env.to_vec());
        let (stdin, stdout, stderr) = (
            files[0].try_clone()?,
            files[1].try_clone()?,
            files[2].try_clone()?,
        );
        let child = forks::forking(|| {
            Command::new(self.t.join("bin").join(&argv[at + 1]))
                .args(&argv[at + 2..])
                .env_clear()
                .envs(env.iter().map(|(k, v)| (k, v)))
                // What the fake stage runs itself is told apart from what the run runs (R39).
                .env("AP_FAKE_STAGE", "1")
                .current_dir(cwd)
                .stdin(stdin)
                .stdout(stdout)
                .stderr(stderr)
                .process_group(0)
                .spawn()
        })?;
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
        self.no_units_after.is_none_or(|n| self.launches < n)
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(self.t.join("policy.json"))
    }
    fn wall(&self) -> SystemTime {
        self.wall.unwrap_or_else(SystemTime::now)
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}
/// The code of a refusal of `decide`: the store's first failed criterion, else the code itself.
fn store_code(f: &Failure) -> String {
    match f.code {
        "store_refused" => f.detail["failed"][0]
            .as_str()
            .or(f.detail["code"].as_str())
            .unwrap()
            .to_owned(),
        code => code.to_owned(),
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
        forks::forking(|| {
            Command::new(env!("CARGO_BIN_EXE_ap"))
                .args(args)
                .env_remove("ONT_ROLE")
                .stdin(std::process::Stdio::null())
                .output()
        })
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
    assert_eq!(f.ok().0["state"], "merged");
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
            (&json!(format!("u-{KEY}")), Some(&json!("dispatch"))),
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
        forks::settled();
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
    f.policy(
        json!({"units": {"directory": f.p("units-link"), "timeouts": {"verifier": 600},
        "github": f.github(None)}}),
    );
    f.steps(&["implement", "repair", "fix", "accept"]);
    assert_eq!(f.ok().0["state"], "merged");
    let home = f.t.join("real-units").join(KEY);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&home), 0o700);
    let entries: BTreeSet<String> = std::fs::read_dir(&home)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    for name in [
        "lock",
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
    // The worktree was there, and is removed after the merge (R32).
    assert!(!entries.contains("wt"), "{entries:?}");
    let worktrees = git(&f.t.join("clone"), &["worktree", "list", "--porcelain"]);
    assert!(!worktrees.contains(home.to_str().unwrap()), "{worktrees}");
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
    assert_eq!(f.ok().0["state"], "merged");
    // The account of a stage's home of the launch (`specs/ap/run-limits.md` R14).
    let account = |field: &str| field.split("/homes/").next().unwrap().to_owned();
    let dir = |n: usize| account(f.launch(n).0.split('|').next().unwrap());
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
    assert_eq!(f.ok().0["state"], "merged");
    assert_eq!(account(f.launch(1).0.split('|').next().unwrap()), f.p("a3"));
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
    assert_eq!(f.ok().0["state"], "merged");
}

// spec: specs/ap/unit-run.md#R6
#[test]
fn r6_a_hold_on_the_unit_refuses_the_take_over() {
    // A cut run (`running`) is taken over without an apply; the active holds are read by one search of `hold`.
    let mut f = F::new();
    f.set_unit(json!({"state": "running"}));
    let hold = |f: &mut F, key: &str, status: &str, subject: Value| {
        let mut h = json!({"status": status, "effect": "no work", "task": oid("task", "walk")});
        if !subject.is_null() {
            h["subject"] = subject;
        }
        f.put("hold", key, h);
    };
    // The subject is the unit: refused `held`, the detail the hold's key; the intent of the unit: the same.
    hold(&mut f, "h-unit", "active", oid("unit", KEY));
    let e = f.refusal();
    assert_eq!((code(&e), &e.detail), (("held", "unit", 2), &json!("h-unit")));
    let searches: Vec<_> = f.calls.iter().filter(|c| c.1[0] == "search").collect();
    assert_eq!(searches.len(), 1);
    assert_eq!(searches[0].2["object_type"], "hold");
    assert_eq!(searches[0].2["where"], json!({"all": [{"eq": ["status", "active"]}]}));
    f.objects.remove(&k("hold", "h-unit"));
    let intent = f.unit()["intent"].clone();
    hold(&mut f, "h-intent", "active", intent);
    let e = f.refusal();
    assert_eq!((code(&e), &e.detail), (("held", "unit", 2), &json!("h-intent")));
    f.objects.remove(&k("hold", "h-intent"));
    assert!(f.actions().is_empty(), "the refusals applied nothing");
    // A hold on another unit, on a host or a finding, a lifted one, one on a task alone, and one written before
    // schema 24 (`unit` and no `subject`) refuse nothing.
    hold(&mut f, "h-other", "active", oid("unit", "another-unit"));
    hold(&mut f, "h-host", "active", oid("host", "pc"));
    hold(&mut f, "h-finding", "active", oid("finding", "f"));
    hold(&mut f, "h-lifted", "lifted", oid("unit", KEY));
    hold(&mut f, "h-task", "active", Value::Null);
    f.put("hold", "h-old", json!({"status": "active", "effect": "old", "unit": oid("unit", KEY)}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_ne!(a["refused"], "held", "{a}");
}
// spec: specs/ap/unit-run.md#R6
#[test]
fn r6_what_the_state_read_decides() {
    let mut f = F::new();
    f.put(
        "hold",
        "h1",
        json!({"status": "active", "effect": "no starts", "subject": oid("intent", "widgets")}),
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
        json!({"status": "active", "effect": "no work", "subject": oid("unit", KEY)}),
    );
    let e = f.refusal();
    assert_eq!(code(&e), ("held", "unit", 2));
    assert_eq!(e.detail, json!("h2"));
    f.objects.remove(&k("hold", "h2"));
    for state in ["draft", "done"] {
        f.set_unit(json!({"state": state}));
        assert_eq!(code(&f.refusal()), ("unit_not_runnable", "unit", 2));
    }
    // Stopped by the Lead's rejection: answered as it is, nothing applied (exit 3).
    f.set_unit(json!({"state": "stopped", "stop_cause": "lead_rejected", "stop_detail": "an invented reason"}));
    let calls = f.actions().len();
    let a = f.stopped("lead_rejected");
    assert_eq!(a["stop_detail"], "an invented reason");
    assert_eq!((f.actions().len(), f.launches), (calls, 0));
    // A merged or dropped unit: its cleanup only, nothing applied.
    for state in ["merged", "dropped"] {
        f.set_unit(json!({"state": state}));
        let (a, status) = f.ok();
        assert_eq!((a["state"].as_str(), status), (Some(state), 0), "{a}");
        assert_eq!(
            a["cleanup"],
            json!({"worktree": "nothing", "branch": "nothing"})
        );
        assert_eq!(f.actions().len(), calls);
    }
    // An accepted unit of class `contract` waits for the Lead's approval: nothing applied.
    f.set_unit(json!({"state": "ready", "class": "contract", "stop_cause": null}));
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["waits_for"].as_str(), status),
        (Some("accepted"), Some("approve"), 0),
        "{a}"
    );
    let calls = f.actions().len();
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["waits_for"].as_str(), status),
        (Some("accepted"), Some("approve"), 0)
    );
    assert_eq!(f.actions().len(), calls);
    let mut f = F::new();
    f.set_unit(json!({"state": "stopped", "head": null}));
    f.steps(&["implement", "accept"]);
    assert_eq!(f.ok().0["state"], "merged");
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
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert!(
        f.calls
            .iter()
            .any(|c| c.0 == "systemctl" && c.1 == ["stop", &scope])
    );
    let run = &f.objects[&k("run", "run-1")];
    assert_eq!(
        (&run["status"], &run["state"], &run["closed_by"]),
        (&json!("closed"), &json!("killed"), &json!("dispatch"))
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
    assert_eq!(a["state"], "merged", "{a}");
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
        (Some("merged"), Some(1), Some(2)),
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
    assert_eq!(a["state"], "merged", "{a}");
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
    assert_eq!(a["state"], "merged", "{a}");
    assert_eq!(
        f.objects[&k("review", &key)]["session"],
        format!("u-{KEY}:recovery")
    );
}

// spec: specs/ap/unit-run.md#R7
#[test]
fn r7_a_call_killed_between_two_applies_goes_on() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    f.die_after = Some("record_head");
    assert_eq!(f.refusal().status, 3);
    (f.dead, f.die_after) = (false, None);
    assert_eq!(f.unit()["state"], "running");
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), f.launches),
        (Some("merged"), 2),
        "{a}"
    );
    assert_eq!(f.bodies("record_head").len(), 1);
}

// spec: specs/ap/unit-run.md#R8
#[test]
fn r8_the_base_is_one_commit_and_the_worktree_is_guarded() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged");
    // The base of the round is the commit `main` was when the run merged into it.
    let merged = a["merged_commit"].as_str().unwrap();
    let base = git(
        &f.t.join("origin.git"),
        &["rev-parse", &format!("{merged}^1")],
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
        (Some("merged"), Some(pushed.as_str())),
        "{a}"
    );
    let merged = a["merged_commit"].as_str().unwrap();
    assert_eq!(
        git(
            &f.t.join("origin.git"),
            &["rev-parse", &format!("{merged}^2")]
        ),
        pushed
    );
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
        (Some("merged"), Some(1)),
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
    assert_eq!(a["state"], "merged", "{a}");
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
// spec: specs/ap/unit-run.md#R26
// spec: specs/ap/unit-run.md#R28
// spec: specs/ap/unit-run.md#R23
#[test]
fn r15_a_head_behind_main_is_updated_by_the_runs_own_merge() {
    let mut f = F::new();
    f.steps(&["implementmain", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["updates"].as_u64(), status),
        (Some("merged"), Some(1), 0),
        "{a}"
    );
    let head = a["head"].as_str().unwrap();
    // The update's merge: the implementer's head first, the base second, by the run's own name.
    let implemented = f.bodies("record_head")[0]["parameters"]["head"].clone();
    assert_eq!(
        json!(f.origin(&["rev-parse", &format!("{head}^1")])),
        implemented
    );
    let shown = f.origin(&["log", "-1", "--format=%s|%an <%ae>|%cn <%ce>", head]);
    assert_eq!(
        shown,
        format!(
            "Merge origin/main into unit/{KEY} (ap unit run)|ap unit run <ap-unit@agents.invalid>|ap unit run <ap-unit@agents.invalid>"
        )
    );
    // R28: the writers gain the update's word before the push, recorded with the head it merged.
    let word = json!(format!("u-{KEY}:update"));
    let worded = |b: &Value| {
        b["action_type"] == "record_head"
            && b["parameters"]["writers"]
                .as_array()
                .unwrap()
                .contains(&word)
    };
    let heads = f.bodies("record_head");
    let with = heads.iter().find(|b| worded(b)).expect("the update's word");
    assert_eq!(with["parameters"]["head"], implemented);
    let recorded = f.first(|c| c.0 == "ont" && worded(&c.2)).unwrap();
    let push = f
        .first(|c| c.0 == "git" && c.1.iter().any(|w| w == "push"))
        .unwrap();
    assert!(recorded < push);
    let to = format!("HEAD:refs/heads/unit/{KEY}");
    assert_eq!(f.calls[push].1[3..7], SAFE);
    assert_eq!(
        f.calls[push].1[7..],
        ["push", "--quiet", "origin", to.as_str()]
    );
    // The new head has no verdict: a round in a new session.
    assert!(f.launch(2).0.contains("--session-id"));
    assert_eq!(f.merges(), (1, 1));
}

// spec: specs/ap/unit-run.md#R26
// spec: specs/ap/unit-run.md#R30
#[test]
fn r26_a_conflict_with_main_stops_and_the_updates_are_bounded() {
    let mut f = F::new();
    f.steps(&["implementclash", "accept"]);
    let a = f.stopped("merge_conflict");
    assert!(
        a["stop_detail"]
            .as_str()
            .unwrap()
            .contains("src/widgets.rs"),
        "{a}"
    );
    assert_eq!(git(&f.wt(), &["status", "--porcelain"]), "");
    assert!(f.packets().is_empty());
    // The same stop on the next call, until the Lead amends the unit or drops it.
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("merge_conflict");
    assert_eq!(f.launches, 1);
    // `main` moved under each look: five updates in one call, then `update_limit`.
    let mut f = F::new();
    let mut steps = vec!["implement"];
    steps.extend(["acceptmain"; 6]);
    f.steps(&steps);
    let a = f.stopped("update_limit");
    assert_eq!((a["updates"].as_u64(), f.launches), (Some(5), 7), "{a}");
    let causes: Vec<Value> = f
        .bodies("reopen_unit")
        .iter()
        .map(|b| b["parameters"]["cause"].clone())
        .collect();
    assert_eq!(causes, vec![json!("main_moved"); 6]);
    // Each look after an update is the same verifier's, the round naming the merge.
    for n in 3..=7 {
        let (line, prompt) = f.launch(n);
        assert!(
            line.contains(&format!("--resume {}", f.session_of(2))),
            "{n}: {line}"
        );
        assert!(
            prompt.contains("What moved it: a merge of origin/main\n"),
            "{prompt}"
        );
    }
    assert_eq!(f.merges(), (0, 5));
}

// spec: specs/ap/unit-run.md#R12
// spec: specs/ap/unit-run.md#R27
#[test]
fn r27_a_call_cut_between_its_merge_and_its_push_is_continued() {
    let mut f = F::new();
    f.steps(&["implementmain", "accept"]);
    f.die_at_push = true;
    assert_eq!(f.refusal().status, 3);
    (f.dead, f.die_at_push) = (false, false);
    assert_eq!(f.unit()["state"], "running");
    let left = git(&f.wt(), &["rev-parse", "HEAD"]);
    assert_ne!(json!(left), json!(f.origin_head()));
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["updates"].as_u64()),
        (Some("merged"), Some(1)),
        "{a}"
    );
    // The merge the cut call left is pushed, not made again.
    assert_eq!(a["head"], left);
    assert_eq!(f.merges(), (1, 1));
    // A merge commit of another message (one made by hand) is its author's to settle.
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing", "settle", "accept"]);
    f.stopped("repair_refused");
    f.push_from_elsewhere("main", "src/main-moved.rs");
    git(&f.t.join("clone"), &["fetch", "--quiet", "origin"]);
    git(
        &f.wt(),
        &[
            "merge",
            "--no-ff",
            "--quiet",
            "-m",
            "A merge by hand",
            "refs/remotes/origin/main",
        ],
    );
    f.set_unit(json!({"state": "stopped"}));
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["updates"].as_u64()),
        (Some("merged"), Some(0)),
        "{a}"
    );
    let (line, prompt) = f.launch(4);
    assert!(
        line.contains("--resume") && prompt.ends_with(CONTINUE),
        "{line}"
    );
    assert_eq!(f.merges(), (1, 0));
}

// spec: specs/ap/unit-run.md#R29
// spec: specs/ap/unit-run.md#R19
#[test]
fn r29_a_head_judged_repair_goes_to_its_author_before_the_update() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing", "fix", "accept"]);
    f.stopped("repair_refused");
    f.push_from_elsewhere("main", "src/main-moved.rs");
    f.set_unit(json!({"state": "stopped"}));
    let from = f.calls.len();
    let (a, _) = f.ok();
    assert_eq!(
        (
            a["state"].as_str(),
            a["updates"].as_u64(),
            a["rounds"].as_u64()
        ),
        (Some("merged"), Some(1), Some(2)),
        "{a}"
    );
    // The fixer first; the update merges the fixer's head; the check ran only on the new head.
    let (line, prompt) = f.launch(4);
    assert!(
        line.contains("--resume") && prompt.contains(FIXER),
        "{line}"
    );
    let head = a["head"].as_str().unwrap();
    assert_eq!(
        f.origin(&["log", "-1", "--format=%s", &format!("{head}^1")]),
        "fix 4"
    );
    assert_eq!(f.checks_since(from), 1);
    // The verifier who asked for the repair looks again, the round naming both moves.
    let (line, prompt) = f.launch(5);
    assert!(
        line.contains(&format!("--resume {}", f.session_of(2))),
        "{line}"
    );
    assert!(
        prompt.contains("What moved it: the author's repair and a merge of origin/main\n"),
        "{prompt}"
    );
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

impl F {
    /// A commit on the origin's `main` that adds the file `rel`, before any call.
    fn seed_main(&self, rel: &str) {
        let seed = self.t.join("seed");
        std::fs::create_dir_all(seed.join(rel).parent().unwrap()).unwrap();
        std::fs::write(seed.join(rel), "invented\n").unwrap();
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "--quiet", "-m", "invented seed"]);
        let origin = self.t.join("origin.git");
        git(
            &seed,
            &["push", "--quiet", origin.to_str().unwrap(), "main"],
        );
    }
    /// One call of a unit whose author runs the shell lines `edit` in the worktree and commits.
    fn edit(&mut self, class: &str, write_set: Value, seed: &[&str], edit: &str) -> (Value, u8) {
        self.set_unit(json!({"class": class, "write_set": write_set, "acceptance": []}));
        for rel in seed {
            self.seed_main(rel);
        }
        std::fs::write(self.t.join("bin/edit.sh"), edit).unwrap();
        self.steps(&["edit", "accept"]);
        self.ok()
    }
}

// spec: specs/ap/unit-run.md#R36
#[test]
fn r36_a_plain_unit_stops_on_a_protected_path_and_a_contract_unit_goes_on() {
    let stops: [(&str, &str, &[&str], &str); 13] = [
        ("ci/**", "ci/x", &[], "mkdir ci; echo x > ci/x"),
        // A name compared without case, and a root directory of the host's files.
        ("CI/**", "CI/x", &[], "mkdir CI; echo x > CI/x"),
        (
            "deploy/**",
            "deploy/x.service",
            &[],
            "mkdir deploy; echo x > deploy/x.service",
        ),
        (
            "specs/**",
            "specs/ap/unit.md",
            &[],
            "mkdir -p specs/ap; echo x > specs/ap/unit.md",
        ),
        (
            ".github/**",
            ".github/workflows/rust.yml",
            &[],
            "mkdir -p .github/workflows; echo x > .github/workflows/rust.yml",
        ),
        ("check", "check", &[], "echo x > check"),
        (
            "crates/**",
            "crates/x/clippy.toml",
            &[],
            "mkdir -p crates/x; echo x > crates/x/clippy.toml",
        ),
        (
            "rust-toolchain",
            "rust-toolchain",
            &[],
            "echo x > rust-toolchain",
        ),
        (".cargo", ".cargo", &[], "ln -s elsewhere .cargo"),
        (
            "ontology/**",
            "ontology/ENGINE-PIN",
            &[],
            "mkdir ontology; echo x > ontology/ENGINE-PIN",
        ),
        // Trees that delivery runs or installs as root: a changed file, and a file added beside
        // `ontology/migrate` that Python would load in its place.
        (
            "content/**",
            "content/SHA256SUMS",
            &[],
            "mkdir content; echo x > content/SHA256SUMS",
        ),
        (
            "ontology/**",
            "ontology/json.py",
            &[],
            "mkdir ontology; echo x > ontology/json.py",
        ),
        // Without rename detection the old path of a file moved away is a changed path.
        (
            "tools/**",
            "ci/checker.py",
            &["ci/checker.py"],
            "mkdir tools; git mv ci/checker.py tools/checker.py",
        ),
    ];
    for (set, path, seed, edit) in stops {
        let mut f = F::new();
        let write_set = if path == "ci/checker.py" {
            json!(["ci/**", "tools/**"])
        } else {
            json!([set])
        };
        let (a, status) = f.edit("plain", write_set, seed, edit);
        assert_eq!(
            (a["state"].as_str(), status),
            (Some("stopped"), 3),
            "{path}: {a}"
        );
        assert_eq!(a["stop_cause"], "protected_path_changed", "{path}: {a}");
        assert!(
            a["stop_detail"].as_str().unwrap().contains(path),
            "{path}: {a}"
        );
        assert!(f.packets().is_empty(), "{path}");
        assert_eq!(f.merges(), (0, 0), "{path}");
    }
    // Neighbours by name are not protected: the unit is judged, accepted and merged.
    for (set, edit) in [
        ("checks/**", "mkdir checks; echo x > checks/x"),
        ("docs/**", "mkdir -p docs/specs; echo x > docs/specs/x"),
    ] {
        let mut f = F::new();
        let (a, status) = f.edit("plain", json!([set]), &[], edit);
        assert_eq!(
            (a["state"].as_str(), status),
            (Some("merged"), 0),
            "{set}: {a}"
        );
    }
    // The same change in a unit of class `contract` whose write set names the path goes on.
    let mut f = F::new();
    let (a, status) = f.edit("contract", json!(["ci/**"]), &[], "mkdir ci; echo x > ci/x");
    assert_eq!((a["state"].as_str(), status), (Some("accepted"), 0), "{a}");
    assert_eq!(a["waits_for"], "approve", "{a}");
}

// spec: specs/ap/unit-run.md#R16
#[test]
fn r16_a_stored_text_outside_the_grammar_is_read_as_on_the_base() {
    // A unit stored with `src/*` (the verbs refuse it, another client of the store may write it): the
    // text is the path named `src/*` and nothing else, so a change of `src/a.rs` is outside the set.
    let mut f = F::new();
    let (a, status) = f.edit(
        "plain",
        json!(["src/*"]),
        &[],
        "mkdir src; echo x > src/a.rs",
    );
    assert_eq!((a["state"].as_str(), status), (Some("stopped"), 3), "{a}");
    assert_eq!(a["stop_cause"], "path_outside_write_set", "{a}");
    assert!(
        a["stop_detail"].as_str().unwrap().contains("src/a.rs"),
        "{a}"
    );
    // A final `/**` covers the descendants of the literal text before it, a bracket included.
    let mut f = F::new();
    let edit = "mkdir -p 'a/[b]'; echo x > 'a/[b]/x.rs'";
    let (a, status) = f.edit("plain", json!(["a/[b]/**"]), &[], edit);
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    // And the same text is no cover for a path of another name.
    let mut f = F::new();
    let edit = "mkdir -p a/b; echo x > a/b/x.rs";
    let (a, status) = f.edit("plain", json!(["a/[b]/**"]), &[], edit);
    assert_eq!((a["state"].as_str(), status), (Some("stopped"), 3), "{a}");
    assert_eq!(a["stop_cause"], "path_outside_write_set", "{a}");
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
        (Some("merged"), 0, Some(3), Some(2)),
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
    // The verifier answers without the form, and again when `ap run` asks it again once.
    f.steps(&["implement", "noform", "noform", "accept"]);
    f.stopped("verifier_failed");
    assert_eq!(f.packets()[0].1["verdict"], "abandoned");
    f.set_unit(json!({"state": "stopped"}));
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged");
    // The abandoned review's session is not asked again: launch 4 is a new session.
    assert!(f.launch(4).0.contains("--session-id"));
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
            "updates",
            "merged_commit",
            "cleanup",
            "credentials"
        ])
    );
    let host = json!([{"stage": 1, "role": "implementer", "credential": "host"}]);
    assert_eq!(a["credentials"], host);
    assert_eq!(
        (
            a["state"].as_str(),
            a["head"].clone(),
            a["updates"].as_u64()
        ),
        (Some("merged"), json!(f.origin_head()), Some(0))
    );
    assert_eq!(
        a["merged_commit"],
        f.origin(&["rev-parse", "refs/heads/main"])
    );
    let r = &a["reviews"][0];
    assert_eq!(
        (&r["round"], &r["head"], &r["verdict"]),
        (&json!(1), &a["head"], &json!("accept"))
    );
    assert_eq!(f.unit()["state"], "merged");
    // A unit of class `effect` waits for the approval: `waits_for`, exit 0.
    let mut f = F::new();
    f.set_unit(json!({"class": "effect"}));
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["waits_for"].as_str(), status),
        (Some("accepted"), Some("approve"), 0)
    );
    assert!(a.get("cleanup").is_none() && a.get("refused").is_none());
    assert!(a["merged_commit"].is_null());
    // A refusal that leaves the unit accepted: `refused`, exit 3, no `waits_for`.
    f.pr_state = "CLOSED";
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["refused"].as_str(), status),
        (Some("accepted"), Some("pull_request_not_open"), 3)
    );
    assert!(a.get("waits_for").is_none());
    // The binary prints a refusal with the key.
    let out = forks::forking(|| {
        Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(["unit", "run", "Bad"])
            .env_remove("ONT_ROLE")
            .stdin(std::process::Stdio::null())
            .output()
    })
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
    assert_eq!(f.ok().0["state"], "merged");
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

// spec: specs/ap/unit-run.md#R18
// spec: specs/ap/unit-run.md#R30
// spec: specs/ap/unit-run.md#R31
// spec: specs/ap/unit-run.md#R32
#[test]
fn r30_a_plain_unit_is_merged_in_the_call_that_accepts_it() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    let (head, merged) = (
        a["head"].as_str().unwrap(),
        a["merged_commit"].as_str().unwrap(),
    );
    assert_eq!(f.unit()["merged_commit"], merged);
    // What delivery needs: two parents, the tree of the second, a closed accepting review of it
    // with the repository and a report whose digest it holds.
    let parents = f.origin(&["rev-list", "--parents", "-n", "1", merged]);
    assert_eq!(parents.split(' ').count(), 3);
    assert_eq!(f.origin(&["rev-parse", &format!("{merged}^2")]), head);
    assert_eq!(
        f.origin(&["rev-parse", &format!("{merged}^{{tree}}")]),
        f.origin(&["rev-parse", &format!("{head}^{{tree}}")])
    );
    let (key, r) = f.packets().pop().unwrap();
    assert_eq!(
        (&r["status"], &r["verdict"], &r["head"], &r["repository"]),
        (
            &json!("closed"),
            &json!("accept"),
            &json!(head),
            &json!("invented/widgets")
        )
    );
    let report = std::fs::read(r["report"].as_str().unwrap()).unwrap();
    assert_eq!(r["report_sha256"], format!("{:x}", Sha256::digest(&report)));
    // R31: validated, then the status on the head, then the merge pinned to the head, then applied.
    let validate = f
        .first(|c| c.0 == "ont" && c.1[1] == "validate" && c.2["action_type"] == "merge_unit")
        .unwrap();
    let posted = f.first(|c| c.0 == "gh" && c.1[1] == "--method").unwrap();
    let merge = f.first(|c| c.0 == "gh" && c.1[1] == "merge").unwrap();
    let applied = f
        .first(|c| c.0 == "ont" && c.1[1] == "apply" && c.2["action_type"] == "merge_unit")
        .unwrap();
    assert!(validate < posted && posted < merge && merge < applied);
    assert_eq!(
        f.calls[validate].2["parameters"]["merged_commit"],
        "0".repeat(40)
    );
    assert_eq!(
        f.calls[validate].2["request_id"],
        f.calls[applied].2["request_id"]
    );
    assert_eq!(
        f.calls[applied].2["parameters"]["intent"],
        oid("intent", "widgets")
    );
    assert!(
        f.calls[merge]
            .1
            .ends_with(&["--match-head-commit".into(), head.to_owned()])
    );
    let want = (
        head.to_owned(),
        "success".into(),
        format!("review {key} accept"),
    );
    assert_eq!(f.statuses, [want]);
    // R32: the one worktree and the one branch.
    assert_eq!(
        a["cleanup"],
        json!({"worktree": "removed", "branch": "removed"})
    );
    assert!(!f.wt().exists() && f.home().join("1.implementer.md").exists());
    let branches = git(&f.t.join("clone"), &["branch", "--list"]);
    assert!(!branches.contains(KEY), "{branches}");
    assert!(!f.actions().contains(&"approve_unit".to_owned()));
    // A second call on the merged unit finds nothing more to remove.
    let (a, status) = f.ok();
    assert_eq!(
        (a["cleanup"].clone(), status),
        (json!({"worktree": "nothing", "branch": "nothing"}), 0)
    );
}

// spec: specs/ap/unit-run.md#R30
// spec: specs/ap/unit-run.md#R33
#[test]
fn r30_a_contract_unit_waits_for_the_approval_of_the_lead() {
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["waits_for"], "approve", "{a}");
    let head = a["head"].clone();
    // Another session than the intent's Lead: refused before any apply.
    let calls = f.actions().len();
    let r = json!({"request_id": "ap-1", "head": head, "reason": "Read the diff."});
    let (e, after) = f.decide("approve", "someone-else", r.clone()).unwrap_err();
    assert_eq!(
        (e.code, e.field.as_str(), &e.detail, after),
        ("not_the_lead", "session", &json!("lead-1"), None)
    );
    assert_eq!(f.actions().len(), calls);
    let answer = f.decide("approve", "lead-1", r).unwrap();
    assert_eq!(
        (&answer["outcome"], &answer["approved_head"]),
        (&json!("applied"), &head)
    );
    let body = f.bodies("approve_unit")[0].clone();
    assert_eq!(
        (&body["role"], &body["request_id"], &body["session"]),
        (&json!("lead"), &json!("ap-1-approve"), &json!("lead-1"))
    );
    assert_eq!(body["expected"][0]["id"]["native_key"], "widgets");
    // The merge by the next call.
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert_eq!(f.merges(), (1, 0));
}

// spec: specs/ap/unit-run.md#R30
#[test]
fn r30_a_call_cut_between_the_merge_and_merge_unit_merges_once() {
    let mut f = F::new();
    f.merge_mode = "cut";
    f.steps(&["implement", "accept"]);
    assert_eq!(f.refusal().status, 3);
    (f.dead, f.merge_mode) = (false, "ok");
    assert_eq!(f.unit()["state"], "accepted");
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert_eq!(a["merged_commit"], json!(f.merged.as_ref().unwrap().1));
    assert_eq!(a["cleanup"]["worktree"], "removed");
    assert_eq!(f.merges(), (1, 0));
    // Merged by hand on another head, or closed: nothing applied, the unit stays accepted.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    let calls = f.actions().len();
    f.merged = Some(("c".repeat(40), "d".repeat(40)));
    let (b, status) = f.ok();
    assert_eq!(
        (b["state"].as_str(), b["refused"].as_str(), status),
        (Some("accepted"), Some("pull_request_not_open"), 3)
    );
    // Merged by hand with the head, without the approval: `merge_unit` is refused.
    f.merged = Some((a["head"].as_str().unwrap().into(), "d".repeat(40)));
    let (b, status) = f.ok();
    assert_eq!(
        (b["state"].as_str(), b["refused"].as_str(), status),
        (Some("accepted"), Some("not_approved"), 3),
        "{b}"
    );
    assert_eq!(f.actions()[calls..], ["merge_unit"]);
    assert!(f.statuses.is_empty() && f.wt().exists());
}

// spec: specs/ap/unit-run.md#R30
// spec: specs/ap/unit-run.md#R31
#[test]
fn r30_a_pull_request_moved_off_main_is_neither_merged_nor_recorded_merged() {
    // An approved unit whose pull request's base is then set to another branch.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    let r = json!({"request_id": "ap-1", "head": a["head"], "reason": "Read."});
    f.decide("approve", "lead-1", r).unwrap();
    f.base_ref = "release".into();
    let calls = f.actions().len();
    let (b, status) = f.ok();
    assert_eq!(
        (b["state"].as_str(), b["refused"].as_str(), status),
        (Some("accepted"), Some("pull_request_not_open"), 3),
        "{b}"
    );
    assert!(f.statuses.is_empty() && f.merges() == (0, 0));
    assert_eq!(f.actions().len(), calls);
    // A call cut after a merge the read shows made into another branch: nothing is recorded.
    let mut f = F::new();
    f.merge_mode = "cut";
    f.steps(&["implement", "accept"]);
    assert_eq!(f.refusal().status, 3);
    (f.dead, f.merge_mode) = (false, "ok");
    f.base_ref = "release".into();
    let calls = f.actions().len();
    let (b, status) = f.ok();
    assert_eq!(
        (b["state"].as_str(), b["refused"].as_str(), status),
        (Some("accepted"), Some("pull_request_not_open"), 3),
        "{b}"
    );
    assert_eq!(f.actions().len(), calls);
    assert!(b.get("cleanup").is_none() && f.wt().exists());
}

// spec: specs/ap/unit-run.md#R30
// spec: specs/ap/unit-run.md#R19
#[test]
fn r30_a_head_or_main_moved_under_an_accepted_unit() {
    // A head pushed from outside: reopened `head_moved`, judged anew by a new session.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept", "accept"]);
    assert_eq!(f.ok().0["waits_for"], "approve");
    let pushed = f.push_from_elsewhere(&format!("unit/{KEY}"), "src/widgets.rs");
    let (a, _) = f.ok();
    assert_eq!(
        (
            a["state"].as_str(),
            a["head"].as_str(),
            a["rounds"].as_u64()
        ),
        (Some("accepted"), Some(pushed.as_str()), Some(2)),
        "{a}"
    );
    assert_eq!(
        f.bodies("reopen_unit")[0]["parameters"]["cause"],
        "head_moved"
    );
    assert!(f.launch(3).0.contains("--session-id"));
    // `main` moved: reopened `main_moved`, updated, and the same verifier looks again.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept", "accept"]);
    let (a, _) = f.ok();
    f.push_from_elsewhere("main", "src/main-moved.rs");
    let (b, _) = f.ok();
    assert_eq!(
        (b["state"].as_str(), b["updates"].as_u64()),
        (Some("accepted"), Some(1)),
        "{b}"
    );
    assert_eq!(
        f.bodies("reopen_unit")[0]["parameters"]["cause"],
        "main_moved"
    );
    let (line, prompt) = f.launch(3);
    assert!(
        line.contains(&format!("--resume {}", f.session_of(2))),
        "{line}"
    );
    let round = format!(
        "Head judged before: {}\nWhat moved it: a merge of origin/main\n",
        a["head"].as_str().unwrap()
    );
    assert!(prompt.contains(&round), "{prompt}");
    // `main` moved again and a hold on the unit: the reopen is refused, the unit stays accepted.
    f.push_from_elsewhere("main", "src/main-moved-again.rs");
    f.put(
        "hold",
        "h1",
        json!({"status": "active", "effect": "no work", "subject": oid("unit", KEY)}),
    );
    let (c, status) = f.ok();
    assert_eq!(
        (c["state"].as_str(), c["refused"].as_str(), status),
        (Some("accepted"), Some("held"), 3)
    );
}

// spec: specs/ap/unit-run.md#R31
// spec: specs/ap/unit-run.md#R22
#[test]
fn r31_a_merge_refused_at_the_validate_or_not_made() {
    // A hold set after the approval: refused at the validate, nothing posted or merged.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    let r = json!({"request_id": "ap-2", "head": a["head"], "reason": "Read."});
    f.decide("approve", "lead-1", r).unwrap();
    f.put(
        "hold",
        "h1",
        json!({"status": "active", "effect": "no merge", "subject": oid("intent", "widgets")}),
    );
    let (b, status) = f.ok();
    assert_eq!(
        (b["state"].as_str(), b["refused"].as_str(), status),
        (Some("accepted"), Some("held"), 3)
    );
    assert!(f.statuses.is_empty() && f.merges() == (0, 0));
    assert!(!f.actions().contains(&"merge_unit".to_owned()));
    // A merge that failed: `lead-review` posted again as `failure`; the next call merges.
    let mut f = F::new();
    f.merge_mode = "fail";
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["refused"].as_str(), status),
        (Some("accepted"), Some("merge_failed"), 3)
    );
    let states: Vec<&str> = f.statuses.iter().map(|s| s.1.as_str()).collect();
    assert_eq!(states, ["success", "failure"]);
    assert!(f.statuses.iter().all(|s| json!(s.0) == a["head"]));
    f.merge_mode = "ok";
    assert_eq!(f.ok().0["state"], "merged");
    // A merge not confirmed: the unit stays accepted, no failure posted.
    let mut f = F::new();
    f.merge_mode = "unconfirmed";
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["refused"].as_str(), status),
        (Some("merge_unconfirmed"), 3)
    );
    assert_eq!(f.statuses.len(), 1);
}

// spec: specs/ap/unit-run.md#R35
// spec: specs/ap/unit-run.md#R32
#[test]
fn r35_a_drop_between_the_validate_and_merge_unit() {
    let mut f = F::new();
    f.drop_after_validate = true;
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["refused"].as_str(), status),
        (Some("dropped"), Some("unit_not_accepted"), 3),
        "{a}"
    );
    assert_eq!(a["merged_commit"], json!(f.merged.as_ref().unwrap().1));
    assert!(a.get("cleanup").is_none() && f.wt().exists());
    // The next call does the cleanup of the dropped unit: its branch is in `main`.
    f.drop_after_validate = false;
    let (a, status) = f.ok();
    assert_eq!(
        (a["cleanup"].clone(), status),
        (json!({"worktree": "removed", "branch": "removed"}), 0)
    );
}

// spec: specs/ap/unit-run.md#R32
#[test]
fn r32_the_cleanup_of_a_dropped_unit_and_of_a_worktree_that_is_not_clean() {
    // Dropped with uncommitted work: the worktree goes with the drop; the branch is not in `main`.
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    std::fs::write(f.wt().join("stray.txt"), "x").unwrap();
    f.set_unit(json!({"state": "dropped"}));
    let calls = f.actions().len();
    let (a, status) = f.ok();
    assert_eq!(status, 0);
    assert_eq!(
        a["cleanup"],
        json!({"worktree": "removed", "branch": "kept", "branch_reason": "not merged into main",
            "pull_request_open": 7})
    );
    assert_eq!(f.actions().len(), calls);
    let branches = git(&f.t.join("clone"), &["branch", "--list"]);
    assert!(branches.contains(KEY), "{branches}");
    assert!(
        f.calls
            .iter()
            .any(|c| c.0 == "git" && c.1.contains(&"--force".to_owned()))
    );
    // Merged, with a worktree that is not clean: it stays, and so does its branch.
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    std::fs::write(f.wt().join("stray.txt"), "x").unwrap();
    f.set_unit(json!({"state": "merged"}));
    let (a, status) = f.ok();
    assert_eq!(
        (a["cleanup"]["worktree"].as_str(), status),
        (Some("worktree_kept"), 0)
    );
    assert_eq!(a["cleanup"]["branch"], "kept");
    assert!(a["cleanup"].get("pull_request_open").is_none());
    assert!(f.wt().join("stray.txt").exists());
    // A link at the worktree's path is not followed: nothing behind it is removed.
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    std::fs::rename(f.wt(), f.t.join("moved")).unwrap();
    std::os::unix::fs::symlink(f.t.join("moved"), f.wt()).unwrap();
    f.set_unit(json!({"state": "dropped"}));
    let (a, _) = f.ok();
    assert_eq!(
        (&a["cleanup"]["worktree"], &a["cleanup"]["worktree_reason"]),
        (&json!("worktree_kept"), &json!("not a directory"))
    );
    assert!(f.t.join("moved/src/widgets.rs").exists());
}

// spec: specs/ap/unit-run.md#R33
// spec: specs/ap/unit-run.md#R35
#[test]
fn r33_approve_checks_its_request_the_lead_and_the_lock() {
    let mut f = F::new();
    let head = "a".repeat(40);
    for (r, field) in [
        (
            json!({"request_id": "x", "head": head, "reason": "r", "extra": 1}),
            "extra",
        ),
        (
            json!({"request_id": "x", "head": "HEAD", "reason": "r"}),
            "head",
        ),
        (
            json!({"request_id": "x", "head": head, "reason": "r", "unit": "../b"}),
            "unit",
        ),
        (
            json!({"request_id": "x y", "head": head, "reason": "r"}),
            "request_id",
        ),
        (
            json!({"request_id": "x", "head": head, "reason": "r".repeat(601)}),
            "reason",
        ),
    ] {
        let (e, _) = f.decide("approve", "lead-1", r).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", field));
    }
    assert!(f.calls.is_empty() && !f.t.join("units").exists());
    // The lock of a run refuses it; a unit of the lane `small` has no intent.
    std::fs::create_dir_all(f.home()).unwrap();
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(f.home().join("lock"))
        .unwrap();
    held.try_lock().unwrap();
    let r = json!({"request_id": "x", "head": head, "reason": "r"});
    let (e, _) = f.decide("approve", "lead-1", r.clone()).unwrap_err();
    assert_eq!(e.code, "unit_run_active");
    drop(held);
    f.set_unit(json!({"lane": "small", "intent": null}));
    let (e, _) = f.decide("approve", "lead-1", r.clone()).unwrap_err();
    assert_eq!(e.code, "no_intent");
    let (e, _) = f
        .decide(
            "reject",
            "lead-1",
            json!({"request_id": "x", "reason": "r"}),
        )
        .unwrap_err();
    assert_eq!(e.code, "no_intent");
    assert!(f.actions().is_empty());
    // The store's refusal passes on: a unit that is not accepted.
    f.set_unit(json!({"lane": "objective", "intent": oid("intent", "widgets")}));
    let (e, after) = f.decide("approve", "lead-1", r).unwrap_err();
    assert_eq!((store_code(&e), after), ("unit_not_accepted".into(), None));
    // The binary: `usage` for another argument, the refusal with `after` and `unit`.
    let out = forks::forking(|| {
        Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(["unit", "approve", "x"])
            .env_remove("ONT_ROLE")
            .stdin(std::process::Stdio::null())
            .output()
    })
    .unwrap();
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (out.status.code(), &e["error"]["code"]),
        (Some(2), &json!("usage"))
    );
}

// spec: specs/ap/unit-run.md#R34
// spec: specs/ap/unit-run.md#R6
#[test]
fn r34_reject_reopens_and_stops_the_unit_and_revokes_the_status() {
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    let reason = "The list shows a widget twice.";
    // Checked before anything: a reason over 1,000 bytes; the role in the environment.
    let long = json!({"request_id": "rj", "reason": "x".repeat(1001)});
    let (e, _) = f.decide("reject", "lead-1", long).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "reason"));
    f.env.push(("ONT_ROLE".into(), "lead".into()));
    let r = json!({"request_id": "rj", "reason": reason});
    let (e, _) = f.decide("reject", "lead-1", r.clone()).unwrap_err();
    assert_eq!(e.code, "role_in_environment");
    f.env.pop();
    let (e, _) = f.decide("reject", "lead-2", r.clone()).unwrap_err();
    assert_eq!(e.code, "not_the_lead");
    // A call cut between its two applies is completed by its repeat.
    f.refuse = Some(("stop_unit", "unit_not_running"));
    let (e, after) = f.decide("reject", "lead-1", r.clone()).unwrap_err();
    assert_eq!(
        (store_code(&e), after.as_deref()),
        ("unit_not_running".into(), Some("reopen"))
    );
    assert_eq!(f.unit()["state"], "running");
    f.refuse = None;
    let answer = f.decide("reject", "lead-1", r).unwrap();
    assert_eq!(
        (&answer["state"], &answer["lead_review"]),
        (&json!("stopped"), &json!("failure"))
    );
    let reopen = f.bodies("reopen_unit");
    assert_eq!(reopen.len(), 2);
    assert_eq!(
        (&reopen[0]["request_id"], &reopen[0]["parameters"]["cause"]),
        (&json!("rj-reopen"), &json!("rejected by the Lead"))
    );
    let stop = f.bodies("stop_unit").pop().unwrap();
    assert_eq!(
        (
            &stop["request_id"],
            &stop["parameters"]["cause"],
            &stop["parameters"]["detail"]
        ),
        (&json!("rj-stop"), &json!("lead_rejected"), &json!(reason))
    );
    assert_eq!(stop["role"], "lead");
    let last = f.statuses.last().unwrap();
    assert_eq!(
        (json!(last.0), last.1.as_str()),
        (a["head"].clone(), "failure")
    );
    // R6: the run takes no rejected unit up again.
    let (calls, launches) = (f.actions().len(), f.launches);
    let b = f.stopped("lead_rejected");
    assert_eq!(b["stop_detail"], reason);
    assert_eq!((f.actions().len(), f.launches), (calls, launches));
    // An approved head is not rejected.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    let r = json!({"request_id": "ok", "head": a["head"], "reason": "Read."});
    f.decide("approve", "lead-1", r).unwrap();
    let (e, _) = f
        .decide(
            "reject",
            "lead-1",
            json!({"request_id": "rj", "reason": "No."}),
        )
        .unwrap_err();
    assert_eq!(e.code, "approved");
    assert!(f.bodies("reopen_unit").is_empty());
}

// spec: specs/ap/unit-run.md#R10
#[test]
fn r10_a_branch_taken_by_its_pull_request_must_be_that_ones_head_branch() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing", "fix", "accept"]);
    f.stopped("repair_refused");
    let wt = f.p(&format!("units/{KEY}/wt"));
    git(&f.t.join("clone"), &["worktree", "remove", "--force", &wt]);
    f.head_ref = "someone/else".into();
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("branch_in_use");
    assert!(!f.wt().exists());
    f.head_ref = format!("unit/{KEY}");
    f.set_unit(json!({"state": "stopped"}));
    assert_eq!(f.ok().0["state"], "merged");
}

// spec: specs/ap/unit-run.md#R20
#[test]
fn r20_the_read_after_a_verifier_includes_the_state_and_its_failure_closes_the_review() {
    let mut f = F::new();
    f.steps(&["implement", "close"]);
    f.stopped("verifier_wrote");
    assert_eq!(f.packets()[0].1["verdict"], "abandoned");
    let mut f = F::new();
    f.steps(&["implement", "ghdown"]);
    f.stopped("github_unreadable");
    let r = &f.packets()[0].1;
    assert_eq!(
        (&r["verdict"], &r["session"]),
        (&json!("abandoned"), &json!(f.session_of(2)))
    );
}

/// One walk by the real engine of `ontology/ENGINE-PIN` on a scratch store, the fake CLI and the fakes of
/// the hosting service above: `AP_UNIT_RUN_TEMPLATE` names a directory whose `store.sqlite` holds the
/// schema of this repository, installed from a copy redirected as `ontology/rehearsal.sh` redirects it,
/// and the `ready` unit `widgets-list` (of the lane `objective`, class `plain`, under a confirmed intent
/// whose plan is reviewed and whose Lead session is `lead-1`). Each walk prints one line.
#[test]
#[ignore = "the walk: needs the pinned engine and a scratch store named by AP_UNIT_RUN_TEMPLATE"]
fn walk_with_the_real_engine() {
    let line = |name: &str, f: &F, a: &Value| {
        let stopped: Vec<String> = f
            .calls
            .iter()
            .filter(|c| c.0 == "systemctl" && c.1[0] == "stop")
            .map(|c| c.1[1].clone())
            .collect();
        println!(
            "walk {name}: state={} cause={} rounds={} repairs={} launches={} pull_request={} reviews={} stops={}",
            a["state"],
            a["stop_cause"],
            a["rounds"],
            a["repairs"],
            f.launches,
            a["pull_request"],
            a["reviews"].as_array().map_or(0, Vec::len),
            stopped.len()
        );
    };
    let walk = |name: &str, steps: &[&str]| {
        let mut f = F::engine();
        f.steps(steps);
        let (a, _) = f.ok();
        line(name, &f, &a);
        (f, a)
    };
    let (_, a) = walk("one-repair", &["implement", "repair", "fix", "accept"]);
    assert_eq!(
        (a["state"].as_str(), a["repairs"].as_u64()),
        (Some("merged"), Some(1)),
        "{a}"
    );
    let (_, a) = walk(
        "two-repairs",
        &["implement", "repair", "fix", "repair", "fix", "accept"],
    );
    assert_eq!(
        (a["state"].as_str(), a["repairs"].as_u64()),
        (Some("merged"), Some(2)),
        "{a}"
    );
    let (_, a) = walk(
        "third-repair",
        &["implement", "repair", "fix", "repair", "fix", "repair"],
    );
    assert_eq!(a["stop_cause"], "repair_limit", "{a}");
    let (_, a) = walk("no-pull-request", &["nopr"]);
    assert_eq!(a["stop_cause"], "no_pull_request", "{a}");
    // A fixer that changes nothing, then a head pushed from another clone.
    let (mut f, a) = walk(
        "fixer-changes-nothing",
        &["implement", "repair", "nothing", "accept"],
    );
    assert_eq!(a["stop_cause"], "repair_refused", "{a}");
    f.push_from_elsewhere(&format!("unit/{KEY}"), "src/widgets.rs");
    let (a, _) = f.ok();
    line("pushed-elsewhere", &f, &a);
    assert_eq!(a["state"], "merged", "{a}");
    assert!(f.launch(4).0.contains("--session-id"));
    // Stopped `acceptance_failed`, amended, made ready and run again. The plan review of the template's intent
    // started after its unit was added, so it counts for the unit (schema version 16, `plan_not_reviewed`); an
    // amendment gives the digest of its page (`page_digest`, required), which a plain unit is not judged by.
    let mut f = F::engine();
    let failing = json!([{"check": "grep -q amended src/widgets.rs", "kind": "fails_on_base"}]);
    f.engine_apply(
        "amend_unit",
        json!({"unit": oid("unit", KEY), "acceptance": failing, "page_digest": "d".repeat(64)}),
    );
    f.engine_ready();
    f.steps(&["implement", "fix", "accept"]);
    let (a, _) = f.ok();
    line("acceptance-failed", &f, &a);
    assert_eq!(a["stop_cause"], "acceptance_failed", "{a}");
    std::thread::sleep(Duration::from_millis(1100));
    let passing = json!([{"check": "test -f src/widgets.rs", "kind": "fails_on_base"}]);
    f.engine_apply(
        "amend_unit",
        json!({"unit": oid("unit", KEY), "acceptance": passing, "page_digest": "d".repeat(64)}),
    );
    f.engine_ready();
    let (a, _) = f.ok();
    line("amended-and-run-again", &f, &a);
    assert_eq!(a["state"], "merged", "{a}");
    assert!(f.launch(2).1.ends_with(CONTINUE));
    // Calls killed during a stage, each followed by a call that goes on.
    for (name, steps, at, record) in [
        (
            "killed-in-implementer",
            vec!["local", "settle", "accept"],
            1,
            "1.implementer.md.run.json",
        ),
        (
            "killed-after-pull-request",
            vec!["dirty", "settle", "accept"],
            1,
            "1.implementer.md.run.json",
        ),
        (
            "killed-in-fixer",
            vec!["implement", "repair", "local", "settle", "accept"],
            3,
            "3.fixer.md.run.json",
        ),
        (
            "killed-in-verifier",
            vec!["implement", "accept", "accept"],
            2,
            "",
        ),
    ] {
        let mut f = F::engine();
        f.steps(&steps);
        f.die_at_launch = Some(at);
        let first = f.refusal();
        let path = match record {
            "" => {
                f.t.join(format!("reviews/{}.md.run.json", f.packets()[0].0))
            }
            r => f.home().join(r),
        };
        f.cut(&path);
        let (a, _) = f.ok();
        line(name, &f, &a);
        let runs = f.engine_call(&["search"], &json!({"object_type": "run", "where": {"all": [{"eq": ["state", "killed"]}]}, "limit": 15}));
        println!(
            "walk {name}: first call {} {}; killed runs {}; reviews {:?}",
            first.code,
            first.status,
            runs["objects"].as_array().map_or(0, Vec::len),
            f.packets()
                .iter()
                .map(|(_, r)| (r["verdict"].clone(), r["session"].clone()))
                .collect::<Vec<_>>()
        );
        assert_eq!(a["state"], "merged", "{name} {a}");
    }
    // A take-over refused by a hold; a branch named `main`; a pull request closed under a running unit.
    let mut f = F::engine();
    f.steps(&["local"]);
    f.die_at_launch = Some(1);
    let _ = f.refusal();
    f.cut(&f.home().join("1.implementer.md.run.json"));
    let hold = f.engine_apply(
        "set_hold",
        json!({"task": oid("task", "ontology-operational-base"),
        "effect": "no walk", "until": "the walk ends", "subject": oid("unit", KEY)}),
    );
    let e = f.refusal();
    println!(
        "walk held: hold {} refusal {} {}",
        hold["outcome"], e.code, e.status
    );
    assert_eq!(e.code, "held");
    let mut f = F::engine();
    f.engine_apply(
        "amend_unit",
        json!({"unit": oid("unit", KEY), "branch": "main", "page_digest": "d".repeat(64)}),
    );
    f.engine_ready();
    let e = f.refusal();
    println!(
        "walk branch-main: refusal {} {} store state {}",
        e.code,
        e.status,
        f.unit()["state"]
    );
    assert_eq!(e.code, "invalid_branch");
    let (mut f, _) = walk("before-close", &["implement", "repair", "nothing"]);
    f.pr_state = "CLOSED";
    let launches = f.launches;
    let (a, _) = f.ok();
    line("pull-request-closed", &f, &a);
    assert_eq!(
        (a["stop_cause"].as_str(), f.launches),
        (Some("pull_request_not_open"), launches)
    );
}

/// The walk of the update, the merge, `approve`, `reject` and the cleanup by the real engine on the
/// scratch store of `walk_with_the_real_engine` (its unit of the lane `objective`, class `plain`, under
/// an intent whose Lead session is `lead-1`). Each walk prints one line.
#[test]
#[ignore = "the walk: needs the pinned engine and a scratch store named by AP_UNIT_RUN_TEMPLATE"]
fn walk_the_merge_with_the_real_engine() {
    let line = |name: &str, f: &F, a: &Value, status: u8| {
        println!(
            "walk {name}: state={} exit={status} updates={} merged_commit={} refused={} waits_for={} cause={} cleanup={} launches={} merges={:?}",
            a["state"],
            a["updates"],
            a["merged_commit"].as_str().map_or("null", |c| &c[..7]),
            a["refused"],
            a["waits_for"],
            a["stop_cause"],
            a["cleanup"],
            f.launches,
            f.merges()
        );
    };
    let run = |name: &str, f: &mut F| {
        let (a, status) = f.ok();
        line(name, f, &a, status);
        a
    };
    // A unit of class `contract` made ready again: amended, its page reviewed, ready.
    let contract = |f: &mut F| {
        f.engine_apply(
            "amend_unit",
            json!({"unit": oid("unit", KEY), "class": "contract", "page_digest": "e".repeat(64)}),
        );
        std::thread::sleep(Duration::from_millis(1100));
        let p = json!({"task": oid("task", "ontology-operational-base"), "kind": "page", "head": "e".repeat(64),
            "model": "walk", "effort": "high", "prompt": f.p("PROMPT.md"), "prompt_sha256": "b".repeat(64),
            "unit": oid("unit", KEY)});
        let started = f.engine_apply("start_unit_review", p);
        let review = started["edits"][0]["id"].clone();
        f.engine_apply("close_unit_review", json!({"review": review, "verdict": "accept",
            "session": "walk-page-reviewer", "report": f.p("REPORT.md"), "unit": oid("unit", KEY), "report_sha256": "c".repeat(64)}));
        let intent = f.unit()["intent"].clone();
        f.engine_apply(
            "ready_unit",
            json!({"unit": oid("unit", KEY), "intent": intent}),
        );
    };
    // A plain unit merged in one call, its worktree gone.
    let mut f = F::engine();
    f.steps(&["implement", "accept"]);
    let a = run("plain-merged", &mut f);
    assert_eq!(
        (a["state"].as_str(), f.wt().exists()),
        (Some("merged"), false)
    );
    let m = a["merged_commit"].as_str().unwrap();
    let head = a["head"].as_str().unwrap();
    assert_eq!(f.origin(&["rev-parse", &format!("{m}^2")]), head);
    assert_eq!(
        f.origin(&["rev-parse", &format!("{m}^{{tree}}")]),
        f.origin(&["rev-parse", &format!("{head}^{{tree}}")])
    );
    // A contract unit that waits, an approval by another session refused, the Lead's, the merge.
    let mut f = F::engine();
    contract(&mut f);
    f.steps(&["implement", "accept"]);
    let a = run("contract-waits", &mut f);
    assert_eq!(a["waits_for"], "approve");
    let r = json!({"request_id": "walk-approve", "head": a["head"], "reason": "Read the diff."});
    let (e, _) = f.decide("approve", "someone-else", r.clone()).unwrap_err();
    println!("walk approve-by-another: {} {}", e.code, e.detail);
    assert_eq!(e.code, "not_the_lead");
    let answer = f.decide("approve", "lead-1", r).unwrap();
    println!("walk approve-by-the-lead: {answer}");
    let a = run("contract-merged", &mut f);
    assert_eq!(a["state"], "merged");
    // `main` moved under a running unit.
    let mut f = F::engine();
    f.steps(&["implementmain", "accept"]);
    let a = run("main-moved-running", &mut f);
    assert_eq!(
        (a["state"].as_str(), a["updates"].as_u64()),
        (Some("merged"), Some(1))
    );
    // `main` moved, then a head pushed from outside, under an accepted unit.
    let mut f = F::engine();
    contract(&mut f);
    f.steps(&["implement", "accept", "accept", "accept"]);
    run("before-main-moves", &mut f);
    f.push_from_elsewhere("main", "src/main-moved.rs");
    let a = run("main-moved-accepted", &mut f);
    assert_eq!(
        (a["state"].as_str(), a["updates"].as_u64()),
        (Some("accepted"), Some(1))
    );
    assert!(f.launch(3).0.contains("--resume"));
    f.push_from_elsewhere(&format!("unit/{KEY}"), "src/widgets.rs");
    let a = run("head-pushed-accepted", &mut f);
    assert_eq!(a["state"], "accepted");
    assert!(f.launch(4).0.contains("--session-id"));
    // A call cut between the merge on the hosting service and `merge_unit`.
    let mut f = F::engine();
    f.merge_mode = "cut";
    f.steps(&["implement", "accept"]);
    let e = f.refusal();
    println!("walk cut-after-merge: first call {} {}", e.code, e.status);
    (f.dead, f.merge_mode) = (false, "ok");
    let a = run("cut-after-merge", &mut f);
    assert_eq!((a["state"].as_str(), f.merges()), (Some("merged"), (1, 0)));
    // A call cut between the run's merge and its push.
    let mut f = F::engine();
    f.die_at_push = true;
    f.steps(&["implementmain", "accept"]);
    let e = f.refusal();
    println!("walk cut-before-push: first call {} {}", e.code, e.status);
    (f.dead, f.die_at_push) = (false, false);
    let a = run("cut-before-push", &mut f);
    assert_eq!((a["state"].as_str(), f.merges()), (Some("merged"), (1, 1)));
    // A merge refused by a hold at the validate.
    let mut f = F::engine();
    contract(&mut f);
    f.steps(&["implement", "accept"]);
    let a = run("hold-before", &mut f);
    let r = json!({"request_id": "walk-approve-2", "head": a["head"], "reason": "Read."});
    f.decide("approve", "lead-1", r).unwrap();
    f.engine_apply(
        "set_hold",
        json!({"task": oid("task", "ontology-operational-base"),
        "effect": "no merge", "until": "the walk ends", "subject": oid("unit", KEY)}),
    );
    let a = run("hold-at-validate", &mut f);
    assert_eq!(a["refused"], "held");
    assert!(f.statuses.is_empty() && f.merges() == (0, 0));
    // `reject`, then a run that takes nothing up.
    let mut f = F::engine();
    contract(&mut f);
    f.steps(&["implement", "accept"]);
    run("reject-before", &mut f);
    let r = json!({"request_id": "walk-reject", "reason": "The list shows a widget twice."});
    let answer = f.decide("reject", "lead-1", r).unwrap();
    println!("walk reject: {answer}");
    let a = run("rejected-run", &mut f);
    assert_eq!(a["stop_cause"], "lead_rejected");
    // The cleanup of a dropped unit with uncommitted work, and of a worktree that is not clean.
    let mut f = F::engine();
    f.steps(&["implement", "repair", "nothing"]);
    run("drop-before", &mut f);
    std::fs::write(f.wt().join("stray.txt"), "x").unwrap();
    f.engine_apply(
        "drop_unit",
        json!({"unit": oid("unit", KEY), "reason": "The walk drops it."}),
    );
    let a = run("dropped-cleanup", &mut f);
    assert_eq!(a["cleanup"]["worktree"], "removed");
    let mut f = F::engine();
    f.merge_mode = "cut";
    f.steps(&["implement", "accept"]);
    let _ = f.refusal();
    (f.dead, f.merge_mode) = (false, "ok");
    std::fs::write(f.wt().join("stray.txt"), "x").unwrap();
    let a = run("not-clean-cleanup", &mut f);
    assert_eq!(
        (a["state"].as_str(), a["cleanup"]["worktree"].as_str()),
        (Some("merged"), Some("worktree_kept"))
    );
}

/// A failed `gh` call that names credentials: each must leave a detail redacted.
const LEAKY: &str = "HTTP 401: Bad credentials GH_TOKEN=ghp_Secret0001 \"token\": \"tok-secret-2\" Authorization: Bearer brr-secret-3 https://someone:pw-secret-4@github.com/x github_pat_11Secret_5 gho_Secret6";
const SECRETS: [&str; 6] = [
    "Secret0001",
    "tok-secret-2",
    "brr-secret-3",
    "pw-secret-4",
    "github_pat_11Secret_5",
    "gho_Secret6",
];
fn redacted(detail: &str) {
    assert!(detail.contains("<redacted>"), "{detail}");
    for secret in SECRETS {
        assert!(!detail.contains(secret), "{secret} in {detail}");
    }
}

// spec: specs/ap/unit-run.md#R22
#[test]
fn r22_a_failed_gh_call_of_a_running_unit_keeps_its_reply_redacted_in_the_stop() {
    let mut f = F::new();
    f.gh_says = LEAKY;
    f.steps(&["implement", "ghdown"]);
    let a = f.stopped("github_unreadable");
    let detail = a["stop_detail"].as_str().unwrap();
    redacted(detail);
    let d: Value = serde_json::from_str(detail).unwrap();
    assert_eq!(d["call"], "pr view");
    assert!(
        d["reply"]
            .as_str()
            .unwrap()
            .starts_with("HTTP 401: Bad credentials")
    );
    assert_eq!(f.unit()["stop_detail"], a["stop_detail"]);
    // A long reply keeps its last 200 bytes, cut at a character boundary.
    let mut f = F::new();
    let long: &'static str = Box::leak(format!("{}é end", "é".repeat(300)).into_boxed_str());
    f.gh_says = long;
    f.steps(&["implement", "ghdown"]);
    let a = f.stopped("github_unreadable");
    let d: Value = serde_json::from_str(a["stop_detail"].as_str().unwrap()).unwrap();
    let reply = d["reply"].as_str().unwrap();
    assert!(
        reply.len() <= 200 && reply.len() >= 199 && reply.ends_with("é end"),
        "{reply}"
    );
}

// spec: specs/ap/unit-run.md#R31
#[test]
fn r31_a_merge_refused_for_a_status_not_yet_taken_is_tried_once_more() {
    let gh = |f: &F| -> Vec<String> {
        let calls = f.calls.iter().filter(|c| c.0 == "gh");
        calls.map(|c| c.1[..2].join(" ")).collect()
    };
    // The first merge fails, the head's `lead-review` reads `success`: once more, and it merges.
    let mut f = F::new();
    f.merge_mode = "fail_once";
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert_eq!(f.merges().0, 2);
    let states: Vec<&str> = f.statuses.iter().map(|s| s.1.as_str()).collect();
    assert_eq!(states, ["success"]);
    let calls = gh(&f);
    let first = calls.iter().position(|c| c == "pr merge").unwrap();
    let read = calls.iter().position(|c| c.ends_with("/status")).unwrap();
    assert_eq!(
        calls[first..=first + 4]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["pr merge", "pr view", &calls[read], "pr merge", "pr view"]
    );
    assert!(calls[read].ends_with(&format!("{}/status", a["head"].as_str().unwrap())));
    // Both fail: the reply, redacted, beside `refused`, and `failure` posted after the second.
    let mut f = F::new();
    f.merge_mode = "fail";
    f.gh_says = LEAKY;
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["refused"].as_str(), status),
        (Some("accepted"), Some("merge_failed"), 3)
    );
    redacted(a["detail"].as_str().unwrap());
    assert_eq!(f.merges().0, 2);
    let states: Vec<&str> = f.statuses.iter().map(|s| s.1.as_str()).collect();
    assert_eq!(states, ["success", "failure"]);
    let calls = gh(&f);
    let last_merge = calls.iter().rposition(|c| c == "pr merge").unwrap();
    let failure = calls.iter().rposition(|c| c == "api --method").unwrap();
    assert!(last_merge < failure);
    // A head whose `lead-review` does not read `success`: no second merge.
    let mut f = F::new();
    f.merge_mode = "fail";
    f.lead_review = Some("pending");
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["refused"], "merge_failed", "{a}");
    assert_eq!(f.merges().0, 1);
    assert!(a["detail"].is_string());
    // A refusal that no `gh` reply caused has no `detail`.
    let mut f = F::new();
    f.merge_mode = "unconfirmed";
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["refused"], "merge_unconfirmed", "{a}");
    assert!(a.get("detail").is_none(), "{a}");
}

// spec: specs/ap/unit-run.md#R8
#[test]
fn r8_a_fetch_that_raced_another_is_tried_once_more() {
    let racing = "error: cannot lock ref 'refs/remotes/origin/main': is at 1 but expected 2";
    let fetches = |f: &F| {
        let calls = f.calls.iter().filter(|c| c.0 == "git");
        calls
            .filter(|c| c.1.iter().any(|a| a == "fetch") && c.1.last().unwrap() == "main")
            .count()
    };
    let mut f = F::new();
    f.fetch_fails = vec![racing];
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert!(f.fetch_fails.is_empty());
    // Twice in a row: once more only, then `fetch_failed`.
    let mut f = F::new();
    f.fetch_fails = vec![racing, racing];
    f.stopped("fetch_failed");
    assert_eq!(fetches(&f), 2);
    // Any other failure stops at once.
    let mut f = F::new();
    f.fetch_fails = vec!["fatal: unable to access the remote"];
    f.stopped("fetch_failed");
    assert_eq!(fetches(&f), 1);
}

impl F {
    /// The files `<n>.update.md` under the unit's directory.
    fn update_files(&self) -> Vec<String> {
        let entries = std::fs::read_dir(self.home()).unwrap().flatten();
        let names = entries.map(|e| e.file_name().to_string_lossy().into_owned());
        let mut names: Vec<String> = names.filter(|n| n.ends_with(".update.md")).collect();
        names.sort();
        names
    }
}

// spec: specs/ap/unit-run.md#R37
// spec: specs/ap/unit-run.md#R18
// spec: specs/ap/unit-run.md#R25
#[test]
fn an_update_outside_the_write_set_keeps_the_accept() {
    // `main` moves outside the write set while the verifier looks: the update's head is checked and
    // the accept carried to it, with no second verifier stage.
    let mut f = F::new();
    f.steps(&["implement", "acceptoutside"]);
    let (a, status) = f.ok();
    assert_eq!(
        (
            a["state"].as_str(),
            a["updates"].as_u64(),
            a["rounds"].as_u64(),
            status
        ),
        (Some("merged"), Some(1), Some(1), 0),
        "{a}"
    );
    assert_eq!(f.launches, 2, "no verifier stage after the update");
    assert_eq!(f.bodies("start_unit_review").len(), 1);
    let packets = f.packets();
    assert_eq!(packets.len(), 2);
    let (first, judged) = (packets[0].0.clone(), packets[0].1["head"].clone());
    let head = a["head"].as_str().unwrap().to_owned();
    assert_eq!(
        json!(f.origin(&["rev-parse", &format!("{head}^1")])),
        judged
    );
    // `carry_review` with the accepting review of the head moved from and the new head, then the accept.
    let carried = f.bodies("carry_review");
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0]["parameters"]["review"]["native_key"], *first);
    assert_eq!(carried[0]["parameters"]["head"], *head);
    let actions = f.actions();
    let at = actions.iter().position(|x| x == "carry_review").unwrap();
    assert_eq!(actions[at + 1], "accept_unit");
    let (made, r) = &packets[1];
    assert_eq!(
        (r["head"].as_str(), r["verdict"].as_str()),
        (Some(head.as_str()), Some("accept"))
    );
    assert_eq!(r["carried_from"]["native_key"], *first);
    assert!(r["round"].is_null(), "{r}");
    // The answer names the carry; the review list holds both, the carried one without a round.
    assert_eq!(a["carried"]["review"], *made, "{a}");
    assert_eq!(a["carried"]["from"], *first, "{a}");
    assert_eq!(a["reviews"].as_array().unwrap().len(), 2);
    assert_eq!(f.update_files().len(), 1);
    let file = a["carried"]["file"].as_str().unwrap();
    assert_eq!(
        Path::new(file),
        f.home().join(&f.update_files()[0]).as_path()
    );
    let text = std::fs::read_to_string(file).unwrap();
    for part in [
        judged.as_str().unwrap(),
        head.as_str(),
        first.as_str(),
        made.as_str(),
        "- docs/other.md\n",
        "None of these paths meets the unit's write set.",
        "The unit's patch is unchanged",
        &format!("R16 on {head}: passed"),
        &format!("R17 on {head}: passed"),
    ] {
        assert!(text.contains(part), "{part}: {text}");
    }
    assert_eq!(f.merges(), (1, 1));

    // A carry the store refuses stops nothing: the head goes to a verifier round instead.
    let mut f = F::new();
    f.refuse = Some(("carry_review", "head_unchanged"));
    f.steps(&["implement", "acceptoutside", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["rounds"].as_u64(), f.launches),
        (Some("merged"), Some(2), 3),
        "{a}"
    );
    assert!(a.get("carried").is_none(), "{a}");
    assert!(f.update_files().is_empty());
    let (line, prompt) = f.launch(3);
    assert!(
        line.contains(&format!("--resume {}", f.session_of(2))),
        "{line}"
    );
    assert!(prompt.contains("What moved it: a merge of origin/main\n"));

    // A carried accept is carried again on the next update; a later verifier counts no carried round
    // and starts a new session, as a carried review names none.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "acceptoutside", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["waits_for"], "approve", "{a}");
    let (b, _) = f.ok();
    assert_eq!(
        (
            b["state"].as_str(),
            b["updates"].as_u64(),
            b["rounds"].as_u64()
        ),
        (Some("accepted"), Some(1), Some(1)),
        "{b}"
    );
    assert_eq!(b["carried"]["from"], a["reviews"][0]["key"], "{b}");
    f.push_from_elsewhere("main", "notes-main.md");
    let (c, _) = f.ok();
    assert_eq!(
        (
            c["state"].as_str(),
            c["updates"].as_u64(),
            c["rounds"].as_u64()
        ),
        (Some("accepted"), Some(1), Some(1)),
        "{c}"
    );
    assert_eq!(c["carried"]["from"], b["carried"]["review"], "{c}");
    assert_eq!((f.launches, f.update_files().len()), (2, 2));
    assert!(f.bodies("approve_unit").is_empty());
    f.push_from_elsewhere(&format!("unit/{KEY}"), "src/widgets.rs");
    let (d, _) = f.ok();
    assert_eq!(
        (d["state"].as_str(), d["rounds"].as_u64()),
        (Some("accepted"), Some(2)),
        "{d}"
    );
    assert!(f.launch(3).0.contains("--session-id"));
    let starts = f.bodies("start_unit_review");
    assert_eq!(starts.last().unwrap()["parameters"]["round"], 2);
    assert_eq!(f.bodies("carry_review").len(), 2);
}

// spec: specs/ap/unit-run.md#R37
// spec: specs/ap/unit-run.md#R19
#[test]
fn an_update_inside_the_write_set_goes_to_the_verifier() {
    // `main` changed a path of the write set: the same verifier looks again, nothing is carried.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "acceptmain", "accept"]);
    f.ok();
    let (b, _) = f.ok();
    assert_eq!(
        (
            b["state"].as_str(),
            b["updates"].as_u64(),
            b["rounds"].as_u64()
        ),
        (Some("accepted"), Some(1), Some(2)),
        "{b}"
    );
    assert!(f.bodies("carry_review").is_empty());
    assert!(b.get("carried").is_none() && f.update_files().is_empty());
    let (line, prompt) = f.launch(3);
    assert!(
        line.contains(&format!("--resume {}", f.session_of(2))),
        "{line}"
    );
    assert!(prompt.contains("What moved it: a merge of origin/main\n"));
    // The path is matched against the unit's own write set, not a fixed tree.
    let mut f = F::new();
    f.set_unit(json!({"write_set": ["src/**", "docs/**"]}));
    f.steps(&["implement", "acceptoutside", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["rounds"].as_u64(), f.launches),
        (Some("merged"), Some(2), 3),
        "{a}"
    );
    assert!(f.bodies("carry_review").is_empty());
}

// spec: specs/ap/unit-run.md#R37
// spec: specs/ap/unit-run.md#R12
#[test]
fn an_author_commit_and_an_update_go_to_the_verifier() {
    // An author commit on the branch and `main` moved outside the write set: judged as at the base.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept", "accept"]);
    f.ok();
    let pushed = f.push_from_elsewhere(&format!("unit/{KEY}"), "src/widgets.rs");
    f.push_from_elsewhere("main", "notes-main.md");
    let (b, _) = f.ok();
    assert_eq!(
        (
            b["state"].as_str(),
            b["updates"].as_u64(),
            b["rounds"].as_u64()
        ),
        (Some("accepted"), Some(1), Some(2)),
        "{b}"
    );
    let head = b["head"].as_str().unwrap();
    assert_eq!(f.origin(&["rev-parse", &format!("{head}^1")]), pushed);
    assert!(f.bodies("carry_review").is_empty());
    assert!(f.launch(3).0.contains("--session-id"));
    // A merge with the update's message that also changes the patch: the patch differs, so judged anew.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept", "accept"]);
    f.ok();
    f.push_from_elsewhere("main", "notes-main.md");
    let wt = f.wt();
    git(&wt, &["fetch", "--quiet", "origin"]);
    git(
        &wt,
        &[
            "merge",
            "--no-ff",
            "--no-commit",
            "--quiet",
            "refs/remotes/origin/main",
        ],
    );
    std::fs::write(wt.join("src/extra.rs"), "extra\n").unwrap();
    git(&wt, &["add", "-A"]);
    let message = format!("Merge origin/main into unit/{KEY} (ap unit run)");
    git(&wt, &["commit", "--quiet", "-m", &message]);
    git(
        &wt,
        &[
            "push",
            "--quiet",
            "origin",
            &format!("HEAD:refs/heads/unit/{KEY}"),
        ],
    );
    let (c, _) = f.ok();
    assert_eq!(
        (c["state"].as_str(), c["rounds"].as_u64()),
        (Some("accepted"), Some(2)),
        "{c}"
    );
    assert!(f.bodies("carry_review").is_empty());
    assert_eq!(f.launches, 3);
}

// ---- Every stage under `limits`, the worktree's own Git directory, the driver's Git calls ------

impl F {
    /// The clone's Git directory and the worktree's administrative directory.
    fn git_dirs(&self) -> (PathBuf, PathBuf) {
        let g = self.t.join("clone/.git");
        let admin = g.join("worktrees/wt");
        (g, admin)
    }
    /// The words a stage of account `account` and scratch directory `<home>/scratch/<n>` gets in
    /// front of the hidden paths, from `--dev /dev` on (the view of `specs/ap/run-limits.md` R16
    /// before them depends on the host): an author's with its worktree and its refs, then the
    /// files its prompt names, read-only (R38).
    fn stage_words(&self, account: &str, n: usize, author: bool, files: &[PathBuf]) -> Vec<String> {
        let text = |p: &Path| p.to_str().unwrap().to_owned();
        let (g, admin) = self.git_dirs();
        let (home, wt) = (self.home(), self.wt());
        let (account, scratch) = (self.p(account), text(&home.join(format!("scratch/{n}"))));
        let mut words: Vec<String> = [
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
            &scratch,
            &scratch,
        ]
        .map(str::to_owned)
        .to_vec();
        let mut read_only = vec![];
        if author {
            let pairs = [
                (wt.clone(), wt.clone()),
                (g.join("objects"), g.join("objects")),
                (admin.clone(), admin.clone()),
                (g.join("refs/heads/unit"), g.join("refs/heads/unit")),
                (
                    g.join("logs/refs/heads/unit"),
                    g.join("logs/refs/heads/unit"),
                ),
                (home.join("remotes/refs"), g.join("refs/remotes/origin")),
                (
                    home.join("remotes/logs"),
                    g.join("logs/refs/remotes/origin"),
                ),
                (home.join("remotes/tags"), g.join("refs/tags")),
            ];
            for (from, to) in pairs {
                words.extend(["--bind".to_owned(), text(&from), text(&to)]);
            }
            read_only.extend([
                wt.join(".git"),
                admin.join("commondir"),
                admin.join("gitdir"),
                admin.join("config.worktree"),
            ]);
        }
        read_only.extend(files.iter().cloned());
        for p in read_only {
            words.extend(["--ro-bind".to_owned(), text(&p), text(&p)]);
        }
        words
    }
    /// Where the words of `stage_words` begin in launch `i`, after the three flags and the view.
    fn dev(&self, i: usize) -> usize {
        let wrap = &self.wraps[i];
        assert_eq!(
            wrap[..4],
            [BWRAP, "--unshare-user", "--disable-userns", "--unshare-pid"].map(str::to_owned)
        );
        wrap.iter().position(|w| w == "--dev").unwrap()
    }
    /// The words of launch `i` (from 0) between those of `stage_words`, which end at `words`, and
    /// `--remount-ro /`: the hidden paths, after the words of the home of the launch (`--tmpfs <a>`
    /// through the last `--remount-ro <a>`, `specs/ap/run-limits.md` R5), which are left out.
    fn stage_hidden(&self, i: usize, words: usize) -> Vec<String> {
        let wrap = &self.wraps[i];
        let mut at = words;
        let mut accounts = vec![];
        while wrap[at] == "--tmpfs" && wrap[at + 2] != "--remount-ro" {
            accounts.push(wrap[at + 1].clone());
            at += 2;
        }
        let ro: Vec<String> = (accounts.iter())
            .flat_map(|a| ["--remount-ro".to_owned(), a.clone()])
            .collect();
        if !ro.is_empty() {
            let end = (at..wrap.len())
                .find(|&j| wrap[j..].starts_with(&ro))
                .unwrap();
            at = end + ro.len();
        }
        assert_eq!(wrap[wrap.len() - 7..wrap.len() - 5], ["--remount-ro", "/"]);
        wrap[at..wrap.len() - 7].to_vec()
    }
    /// The files the prompt of the verifier's stage `n` names: its description file, then the
    /// author's answer its round names (R19, R38).
    fn verifier_files(&self, n: usize) -> Vec<PathBuf> {
        let home = self.home();
        let prompt = std::fs::read_to_string(home.join(format!("{n}.verifier.prompt.md"))).unwrap();
        let mut files = vec![home.join(format!("{n}.description.md"))];
        let answer = prompt
            .lines()
            .find_map(|l| l.strip_prefix("Answer of the author: "))
            .filter(|a| *a != "none");
        files.extend(answer.map(PathBuf::from));
        files
    }
}
/// The hidden paths of a run of hiding words (`specs/ap/run-limits.md` R10), each with its words.
fn hidden_groups(words: &[String]) -> Vec<(String, Vec<String>)> {
    let mut groups = vec![];
    let mut i = 0;
    while i < words.len() {
        let n = if words[i] == "--tmpfs" { 4 } else { 3 };
        let p = words[i + n - 1].clone();
        groups.push((p, words[i..i + n].to_vec()));
        i += n;
    }
    groups
}
/// The mode bits of a path.
fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

// spec: specs/ap/unit-run.md#R38
#[test]
fn r38_every_stage_runs_under_limits_and_an_author_writes_its_worktree_and_its_own_refs() {
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    // The implementer replaces the name of its view's `origin/main` by a link to a file outside.
    let outside = f.t.join("outside");
    std::fs::write(&outside, "outside\n").unwrap();
    let edit = format!(
        "mkdir -p src; echo widget >> src/widgets.rs; rm -f ../remotes/refs/main; ln -s {} ../remotes/refs/main\n",
        outside.display()
    );
    std::fs::write(f.t.join("bin/edit.sh"), edit).unwrap();
    f.steps(&["edit", "repair", "fix", "accept"]);
    let (a, status) = f.ok();
    assert_eq!(
        (a["state"].as_str(), a["waits_for"].as_str(), status),
        (Some("accepted"), Some("approve"), 0),
        "{a}"
    );
    let home = f.home();
    let wt = f.wt();
    let reviews: Vec<String> = f.packets().into_iter().map(|(k, _)| k).collect();
    let stages = [
        ("implementer", "a1", true),
        ("verifier", "a2", false),
        ("fixer", "a1", true),
        ("verifier", "a2", false),
    ];
    assert_eq!(f.wraps.len(), 4);
    // The files the prompts name: the verifier's report for the fixer, the description (and, for
    // a second look, the author's answer) for a verifier.
    let report = f.t.join(format!("reviews/{}.md", by_round(&f)[0].0));
    for (i, (role, account, author)) in stages.into_iter().enumerate() {
        let n = i + 1;
        assert!(home.join(format!("{n}.{role}.prompt.md")).exists(), "{n}");
        let files = match role {
            "fixer" => vec![report.clone()],
            "verifier" => f.verifier_files(n),
            _ => vec![],
        };
        let want = f.stage_words(account, n, author, &files);
        let wrap = &f.wraps[i];
        let dev = f.dev(i);
        assert_eq!(wrap[dev..dev + want.len()], want[..], "{role}: {wrap:?}");
        let tail = ["--cap-drop", "ALL", "--chdir", wt.to_str().unwrap(), "--"];
        assert_eq!(wrap[wrap.len() - 5..], tail.map(str::to_owned), "{role}");
        // Only hidden paths after the named ones, and the state of `gh` never hidden from an
        // author with the host's login: its view shows it where the host has it (R16 (j)); a
        // verifier's view does not hold it (R41).
        let hidden = f.stage_hidden(i, dev + want.len());
        assert!(!hidden.iter().any(|w| w == "--bind"), "{hidden:?}");
        let gh = wrap.iter().filter(|w| w.ends_with("/.config/gh")).count();
        assert!(
            !hidden.iter().any(|w| w.ends_with("/.config/gh")),
            "{wrap:?}"
        );
        assert!(gh == 0 || (author && gh == 2), "{role}: {wrap:?}");
        // A Cargo home in the stage's scratch directory, never root's.
        let env = &f.launch_envs[i];
        let cargo: Vec<&String> = env
            .iter()
            .filter(|(k, _)| k == "CARGO_HOME")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            cargo,
            [&format!("{}/scratch/{n}/cargo", home.display())],
            "{role}"
        );
        // The record names the limits of the request.
        let read =
            |p: PathBuf| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
        let prompt = format!("/{n}.{role}.prompt.md");
        let rec = match author {
            true => read(home.join(format!("{n}.{role}.md.run.json"))),
            false => (reviews.iter())
                .map(|k| read(f.t.join(format!("reviews/{k}.md.run.json"))))
                .find(|r| r["prompt"]["path"].as_str().unwrap().ends_with(&prompt))
                .unwrap(),
        };
        let limits = &rec["limits"];
        let show = author.then(|| json!(["~/.config/gh"]));
        assert_eq!(limits.get("show").cloned(), show, "{role}: {rec}");
        assert_eq!(
            limits["scratch"],
            json!(format!("{}/scratch/{n}", home.display()))
        );
        assert_eq!(limits.get("over").is_some(), author, "{role}: {limits}");
        let mut read_only: Vec<Value> = match author {
            true => [
                wt.join(".git"),
                f.git_dirs().1.join("commondir"),
                f.git_dirs().1.join("gitdir"),
                f.git_dirs().1.join("config.worktree"),
            ]
            .iter()
            .map(|p| json!(p))
            .collect(),
            false => vec![],
        };
        read_only.extend(files.iter().map(|p| json!(p)));
        assert_eq!(
            limits.get("read_only").cloned(),
            (!read_only.is_empty()).then(|| json!(read_only)),
            "{role}: {limits}"
        );
        let writable = limits["writable"].as_array().unwrap();
        assert_eq!(writable.len(), if author { 10 } else { 2 }, "{limits}");
    }
    // What the run made for the authors: the ref directories and the targets in the clone, its own
    // remote-tracking directories (mode 0700) and an empty `config.worktree`.
    let (g, admin) = f.git_dirs();
    for d in [
        "refs/heads/unit",
        "logs/refs/heads/unit",
        "refs/remotes/origin",
        "logs/refs/remotes/origin",
        "refs/tags",
    ] {
        assert!(g.join(d).is_dir(), "{d}");
    }
    for d in [
        "scratch",
        "remotes",
        "remotes/refs",
        "remotes/logs",
        "remotes/tags",
    ] {
        assert_eq!(mode(&home.join(d)), 0o700, "{d}");
    }
    let config = std::fs::symlink_metadata(admin.join("config.worktree")).unwrap();
    assert!(config.is_file() && config.len() == 0);
    // Each `ap run` removed its scratch directory.
    assert_eq!(std::fs::read_dir(home.join("scratch")).unwrap().count(), 0);
    // Before the fixer the link was replaced by the base, its target unchanged.
    let main = home.join("remotes/refs/main");
    assert!(std::fs::symlink_metadata(&main).unwrap().is_file());
    let base = f.origin(&["rev-parse", "refs/heads/main"]);
    assert_eq!(std::fs::read_to_string(&main).unwrap(), format!("{base}\n"));
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "outside\n");
    // A settling stage is an author's too.
    let mut f = F::new();
    f.steps(&["dirty", "settle", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let want = f.stage_words("a1", 2, true, &[]);
    let dev = f.dev(1);
    assert_eq!(
        f.wraps[1][dev..dev + want.len()],
        want[..],
        "{:?}",
        f.wraps[1]
    );
    assert!(f.launch(2).1.ends_with(CONTINUE));
}

// spec: specs/ap/unit-run.md#R38
#[test]
fn r38_a_fixer_and_a_verifier_read_the_files_their_prompts_name() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "fix", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let home = f.home();
    let records = four_records(&f);
    let (key, _) = by_round(&f).remove(0);
    let report = f.t.join(format!("reviews/{key}.md"));
    // The fixer's round names the verifier's report; the second verifier's names the author's
    // answer, beside each verifier's description file.
    let fixer = std::fs::read_to_string(home.join("3.fixer.prompt.md")).unwrap();
    assert!(fixer.contains(&format!("Verifier report: {}\n", report.display())));
    let second = std::fs::read_to_string(home.join("4.verifier.prompt.md")).unwrap();
    let answer = home.join("3.fixer.md");
    assert!(second.contains(&format!("Answer of the author: {}\n", answer.display())));
    let named: [Vec<PathBuf>; 4] = [
        vec![],
        vec![home.join("2.description.md")],
        vec![report],
        vec![home.join("4.description.md"), answer],
    ];
    for (i, files) in named.iter().enumerate() {
        let read_only = records[i]["limits"]["read_only"].as_array().cloned();
        let read_only = read_only.unwrap_or_default();
        let tail = &read_only[read_only.len() - files.len()..];
        let want: Vec<Value> = files.iter().map(|p| json!(p)).collect();
        assert_eq!(tail, &want[..], "{i}: {}", records[i]["limits"]);
        // The fixer's five, a verifier's one or two (`specs/ap/run-limits.md` R1: at most 8).
        let count = [4, 1, 5, 2][i];
        assert_eq!(read_only.len(), count, "{i}");
        // Each a read-only bind of the stage's launch.
        for p in files {
            let p = p.to_str().unwrap().to_owned();
            let bind = ["--ro-bind".to_owned(), p.clone(), p];
            assert!(
                f.wraps[i].windows(3).any(|w| w == bind),
                "{i}: {:?}",
                f.wraps[i]
            );
        }
    }
}

// spec: specs/ap/unit-run.md#R38
#[test]
fn r38_a_directory_at_the_name_of_the_views_main_is_removed_and_a_failure_starts_no_stage() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    f.set_unit(json!({"class": "contract"}));
    // A directory with a link in it where the run writes `main`: removed, the link not followed.
    let home = f.home();
    std::fs::create_dir_all(home.join("remotes/refs/main")).unwrap();
    let outside = f.t.join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("kept"), "kept\n").unwrap();
    std::os::unix::fs::symlink(&outside, home.join("remotes/refs/main/link")).unwrap();
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted", "{a}");
    assert!(
        std::fs::symlink_metadata(home.join("remotes/refs/main"))
            .unwrap()
            .is_file()
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("kept")).unwrap(),
        "kept\n"
    );
    // A name the run cannot write: the unit stops `worktree_failed` and no stage starts.
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    std::fs::create_dir_all(f.home()).unwrap();
    std::fs::write(f.home().join("remotes"), "a file").unwrap();
    let a = f.stopped("worktree_failed");
    assert_eq!(a["stop_detail"], "remotes");
    assert_eq!(f.launches, 0);
}

// spec: specs/ap/unit-run.md#R8
#[test]
fn r8_the_worktree_names_its_own_git_directory() {
    use std::os::unix::fs::symlink;
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    let launches = f.launches;
    let (g, admin) = f.git_dirs();
    let dotgit = f.wt().join(".git");
    let kept = std::fs::read_to_string(&dotgit).unwrap();
    assert_eq!(kept, format!("gitdir: {}\n", admin.display()));
    let refused = |f: &mut F| {
        f.set_unit(json!({"state": "stopped"}));
        assert_eq!(f.stopped("worktree_failed")["stop_detail"], "git directory");
    };
    // A `.git` that is a link, though to a file of the same text.
    let copy = f.t.join("dotgit");
    std::fs::write(&copy, &kept).unwrap();
    std::fs::remove_file(&dotgit).unwrap();
    symlink(&copy, &dotgit).unwrap();
    refused(&mut f);
    std::fs::remove_file(&dotgit).unwrap();
    // A `.git` that names another directory: the clone's own, and a copy of the worktree's
    // administrative directory under another repository.
    std::fs::write(&dotgit, format!("gitdir: {}\n", g.display())).unwrap();
    refused(&mut f);
    let other = f.t.join("other.git/worktrees/wt");
    std::fs::create_dir_all(&other).unwrap();
    for name in ["gitdir", "commondir", "HEAD"] {
        std::fs::copy(admin.join(name), other.join(name)).unwrap();
    }
    std::fs::write(&dotgit, format!("gitdir: {}\n", other.display())).unwrap();
    refused(&mut f);
    std::fs::write(&dotgit, &kept).unwrap();
    // A `commondir` that names another directory.
    std::fs::write(admin.join("commondir"), "../../..\n").unwrap();
    refused(&mut f);
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    assert_eq!(f.launches, launches, "no stage started");
    // Restored, the worktree is used again.
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("repair_refused");
}

// spec: specs/ap/unit-run.md#R39
#[test]
fn r39_every_git_call_of_the_driver_runs_no_hook_and_enters_no_submodule() {
    // A run with an update that merges and pushes, and one whose update conflicts.
    let mut f = F::new();
    f.steps(&["implementmain", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let mut g = F::new();
    g.steps(&["implementclash"]);
    g.stopped("merge_conflict");
    let mut seen = BTreeSet::new();
    for f in [&f, &g] {
        for (program, args, _) in &f.calls {
            let words: &[String] = match program.as_str() {
                "git" => args,
                // The update's merge and `ap run`'s reads of an implementer's worktree.
                "env" => &args[args.iter().position(|a| a == "git").unwrap() + 1..],
                _ => continue,
            };
            // `check-ref-format` reads no repository.
            if words[0] == "check-ref-format" {
                continue;
            }
            let at = words.iter().position(|w| w == "-C").unwrap() + 2;
            let command = &words[at..];
            // The page's reads of the clone (`ls-tree`, `cat-file`, its `rev-parse` of
            // `origin/main`; `render.rs`) are not the driver's and run no hook.
            let page = command == ["rev-parse", "--verify", "origin/main^{commit}"];
            if page || ["ls-tree", "cat-file"].contains(&command[0].as_str()) {
                continue;
            }
            assert!(command.len() > 4 && command[..4] == SAFE, "{words:?}");
            let word = command[4].as_str();
            let reads_worktree = word == "status"
                || (word == "diff" && command.iter().any(|w| w == "--diff-filter=U"));
            if reads_worktree {
                assert!(
                    command.contains(&"--ignore-submodules=all".to_owned()),
                    "{words:?}"
                );
            }
            seen.insert(word.to_owned());
        }
    }
    for word in [
        "fetch",
        "status",
        "rev-parse",
        "worktree",
        "merge",
        "push",
        "diff",
        "merge-base",
        "branch",
    ] {
        assert!(seen.contains(word), "{word}: {seen:?}");
    }
    // A diff between two commits keeps its options.
    let between = f
        .calls
        .iter()
        .find(|c| c.0 == "git" && c.1.iter().any(|w| w == "diff") && c.1.iter().any(|w| w == "-z"));
    let between = &between.unwrap().1;
    assert!(
        between.contains(&"--ignore-submodules=none".to_owned()),
        "{between:?}"
    );
}

/// A program at `<t>/bin/mark-<name>` that leaves the file `<t>/marks/<name>` when the run runs it,
/// and nothing when a fake stage does (which in a real run would run in its own view).
fn marker(f: &F, name: &str, body: &str) -> String {
    let marks = f.t.join("marks");
    std::fs::create_dir_all(&marks).unwrap();
    let path = f.t.join(format!("bin/mark-{name}"));
    let text = format!(
        "#!/bin/sh\n[ -n \"$AP_FAKE_STAGE\" ] || : > '{}/{name}'\n{body}\n",
        marks.display()
    );
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_owned()
}
/// The clone with hooks, the stage's edit with a committed submodule whose own configuration names
/// a filter, a `core.fsmonitor` line in the worktree's `config.worktree`, and `main` moved by one
/// commit that sets `moved` (R39).
fn planted(f: &F, moved: &str) {
    git(
        &f.t.join("clone"),
        &["config", "extensions.worktreeConfig", "true"],
    );
    let hooks = f.t.join("clone/.git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for hook in [
        "post-merge",
        "pre-push",
        "post-checkout",
        "reference-transaction",
    ] {
        let program = marker(f, hook, "cat > /dev/null 2>&1; exit 0");
        std::fs::copy(&program, hooks.join(hook)).unwrap();
    }
    let filter = marker(f, "filter", "cat");
    let fsmonitor = marker(f, "fsmonitor", "exit 1");
    let edit = format!(
        r#"set -e
mkdir -p src; echo "widget" >> src/widgets.rs
git init -q src/sub
( cd src/sub && printf 'a\n' > f && printf 'f filter=mark\n' > .gitattributes \
  && git config filter.mark.clean '{filter}' && git add -A \
  && git -c user.name=Invented -c user.email=invented@example.invalid commit -qm sub && printf 'a\n' > f )
printf '[core]\n\tfsmonitor = {fsmonitor}\n' >> "$(git rev-parse --git-path config.worktree)"
git fetch -q origin main; base=$(git rev-parse FETCH_HEAD); idx=$(mktemp -u)
GIT_INDEX_FILE=$idx git read-tree "$base"
blob=$(printf 'main\n' | git hash-object -w --stdin)
GIT_INDEX_FILE=$idx git update-index --add --cacheinfo "100644,$blob,{moved}"
tree=$(GIT_INDEX_FILE=$idx git write-tree); rm -f "$idx"
c=$(git -c user.name=Invented -c user.email=invented@example.invalid commit-tree "$tree" -p "$base" -m "main moved")
git push -q origin "$c:refs/heads/main"
"#
    );
    std::fs::write(f.t.join("bin/edit.sh"), edit).unwrap();
}
fn marks(f: &F) -> Vec<String> {
    let entries = std::fs::read_dir(f.t.join("marks")).unwrap().flatten();
    entries
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

// spec: specs/ap/unit-run.md#R39
// spec: specs/ap/unit-run.md#R32
#[test]
fn r39_with_the_real_git_no_hook_fsmonitor_or_filter_of_a_stage_runs_in_the_drivers_calls() {
    // The update merges and pushes, the checks run, the unit is merged and cleaned up.
    let mut f = F::new();
    planted(&f, "docs/other.md");
    f.steps(&["edit", "accept"]);
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert_eq!(a["updates"], 1, "{a}");
    assert!(f.merges().1 == 1 && f.checks_since(0) >= 1);
    assert_eq!(marks(&f), Vec::<String>::new());
    // What the stage left was there: a submodule merged into `main`, the fsmonitor line.
    let sub = f.origin(&["ls-tree", "refs/heads/main", "src/sub"]);
    assert!(sub.starts_with("160000 commit "), "{sub}");
    assert!(
        std::fs::read_to_string(f.git_dirs().1.join("config.worktree"))
            .unwrap()
            .contains("fsmonitor")
    );
    // Git refuses to remove a worktree with a committed submodule.
    assert_eq!(a["cleanup"]["worktree"], "worktree_kept", "{a}");
    // The update conflicts: the merge, the conflicting paths, the abort.
    let mut f = F::new();
    planted(&f, "src/widgets.rs");
    f.steps(&["edit"]);
    let a = f.stopped("merge_conflict");
    assert!(
        a["stop_detail"]
            .as_str()
            .unwrap()
            .contains("src/widgets.rs"),
        "{a}"
    );
    assert_eq!(marks(&f), Vec::<String>::new());
}

// spec: specs/ap/run-limits.md#R12
// spec: specs/ap/unit-run.md#R16
#[test]
fn r12_a_check_on_the_head_runs_in_its_view_and_without_units_as_on_the_base() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let wt = f.wt().to_str().unwrap().to_owned();
    let dotgit = format!("{wt}/.git");
    let run = f
        .calls
        .iter()
        .find(|c| c.0 == "systemd-run")
        .unwrap()
        .1
        .clone();
    let at = run.iter().position(|a| a == BWRAP).unwrap();
    assert_eq!(run[..16][13..], ["--", "sh", "-c"].map(str::to_owned));
    assert_eq!(
        run[19..23],
        ["env", "-i", "-C", wt.as_str()].map(str::to_owned)
    );
    assert!(run[at - 1].starts_with("AP_HEAD=") && run[at - 2].starts_with("AP_BASE="));
    let head = [
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
        "--tmpfs",
        "/tmp",
        "--bind",
        &wt,
        &wt,
        "--ro-bind",
        &dotgit,
        &dotgit,
    ];
    assert_eq!(run[at..at + head.len()], head.map(str::to_owned), "{run:?}");
    let end = at + run[at..].iter().position(|a| a == "--").unwrap();
    assert_eq!(
        run[end - 4..=end],
        ["--cap-drop", "ALL", "--chdir", &wt, "--"].map(str::to_owned)
    );
    assert_eq!(
        run[end + 1..],
        [
            "timeout",
            "-k",
            "10",
            "300",
            "bash",
            "-c",
            "test -f src/widgets.rs"
        ]
        .map(str::to_owned)
    );
    assert_eq!(f.check_wraps[0], run[at..=end]);
    // The check hides every existing path of the list, the state of `gh` too, nothing shown; a
    // stage hides those of them a bind of its launch holds or lies in, the state of `gh` not
    // (`specs/ap/run-limits.md` R10), with the same words.
    let hidden = run[at + head.len()..end - 4].to_vec();
    let dev = f.dev(0);
    let stage = f.stage_hidden(0, dev + f.stage_words("a1", 1, true, &[]).len());
    let (check, stage) = (hidden_groups(&hidden), hidden_groups(&stage));
    let wrap = &f.wraps[0];
    let mut targets = vec![];
    for (i, w) in wrap.iter().enumerate() {
        if (w == "--bind" || w == "--ro-bind") && wrap[i + 1] != "/dev/null" {
            targets.push(Path::new(&wrap[i + 2]).to_path_buf());
        }
    }
    let related =
        |p: &str| (targets.iter()).any(|t| Path::new(p).starts_with(t) || t.starts_with(p));
    let want: Vec<(String, Vec<String>)> = (check.iter())
        .filter(|(p, _)| related(p) && !p.ends_with("/.config/gh"))
        .cloned()
        .collect();
    assert_eq!(stage, want, "{wrap:?}");
    assert!(!hidden.iter().any(|w| w == "--bind"), "{hidden:?}");
    // Without units: the plain `sh` call of the base, no view.
    let mut f = F::new();
    f.no_units_after = Some(1);
    f.steps(&["implement", "accept"]);
    let a = f.stopped("stage_refused");
    let wt = f.wt().to_str().unwrap().to_owned();
    assert!(
        a["stop_detail"]
            .as_str()
            .unwrap()
            .contains("systemd_absent"),
        "{a}"
    );
    let sh = &f.calls.iter().find(|c| c.0 == "sh").unwrap().1;
    assert!(!sh.contains(&BWRAP.to_owned()), "{sh:?}");
    assert_eq!(sh.len(), 19, "{sh:?}");
    assert_eq!(
        sh[4..8],
        ["env", "-i", "-C", wt.as_str()].map(str::to_owned)
    );
    assert_eq!(
        sh[12..18],
        ["timeout", "-k", "10", "300", "bash", "-c"].map(str::to_owned)
    );
    assert!(f.check_wraps.is_empty());
}

// spec: specs/ap/unit-run.md#R7
// spec: specs/ap/unit-run.md#R32
#[test]
fn r7_the_scratch_directories_go_after_the_leftovers_and_r32_the_scratch_and_remotes_go() {
    use std::os::unix::fs::symlink;
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["local", "settle", "accept"]);
    f.die_at_launch = Some(1);
    assert_eq!(f.refusal().status, 3);
    let record = f.home().join("1.implementer.md.run.json");
    f.cut(&record);
    // What a killed `ap run` leaves: its scratch directory, with a link out of it; and a stray file.
    let scratch = f.home().join("scratch");
    let outside = f.t.join("outside");
    std::fs::create_dir_all(scratch.join("1/tmp")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("kept"), "kept\n").unwrap();
    symlink(&outside, scratch.join("1/tmp/out")).unwrap();
    std::fs::write(scratch.join("stray"), "s").unwrap();
    let (a, _) = f.ok();
    assert_eq!(a["state"], "accepted", "{a}");
    assert!(!scratch.join("1").exists() && !scratch.join("stray").exists());
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_to_string(outside.join("kept")).unwrap(),
        "kept\n"
    );
    // The record of the killed stage still says `running`; its run is closed in the store.
    assert_eq!(f.record("1.implementer.md.run.json")["state"], "running");
    assert_eq!(f.objects[&k("run", "run-1")]["state"], "killed");
    assert!(f.home().join("remotes/refs/main").is_file());
    // The cleanup of a merged unit removes both directories.
    f.set_unit(json!({"state": "merged"}));
    let (a, _) = f.ok();
    assert_eq!(a["cleanup"]["worktree"], "removed", "{a}");
    assert!(!scratch.exists() && !f.home().join("remotes").exists());
    assert!(f.home().join("1.implementer.md.run.json").exists());
    // Links at both names are not followed, by the leftovers nor by the cleanup.
    symlink(&outside, &scratch).unwrap();
    symlink(&outside, f.home().join("remotes")).unwrap();
    f.ok();
    assert_eq!(
        std::fs::read_to_string(outside.join("kept")).unwrap(),
        "kept\n"
    );
    assert!(std::fs::symlink_metadata(&scratch).unwrap().is_symlink());
}

// ---- A link a stage left in the worktree's Git directory or at the unit's ref (R40) ------------

/// The file a test's link points at, outside the worktree and the Git directory: `name` under the
/// test's directory, holding one line.
fn target(f: &F, name: &str) -> PathBuf {
    let path = f.t.join(format!("outside-{name}"));
    std::fs::write(&path, format!("{name} kept\n")).unwrap();
    path
}
/// `link` made a link to `to`, whatever stood there.
fn link_to(link: &Path, to: &Path) {
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(to, link).unwrap();
}
/// A contract unit accepted, its worktree in place and `main` moved: the next call updates the branch.
fn accepted_and_moved() -> F {
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["waits_for"], "approve", "{a}");
    f.push_from_elsewhere("main", "src/main-moved.rs");
    f
}
/// The calls of `git` and `env` made after the `from`-th whose words hold `word` exactly.
fn calls_with(f: &F, from: usize, word: &str) -> usize {
    let words = |c: &&(String, Vec<String>, Value)| c.1.iter().any(|a| a == word);
    let git_or_env = |c: &&(String, Vec<String>, Value)| c.0 == "git" || c.0 == "env";
    f.calls[from..]
        .iter()
        .filter(git_or_env)
        .filter(words)
        .count()
}
fn detail(a: &Value) -> Value {
    serde_json::from_str(a["stop_detail"].as_str().unwrap()).unwrap()
}
fn text(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

// spec: specs/ap/unit-run.md#R40
#[test]
fn r40_a_link_at_the_reflog_of_head_stops_the_update_before_its_merge() {
    let mut f = accepted_and_moved();
    let (_, admin) = f.git_dirs();
    let outside = target(&f, "head");
    link_to(&admin.join("logs/HEAD"), &outside);
    let from = f.calls.len();
    let a = f.stopped("git_dir_link");
    assert_eq!(
        detail(&a),
        json!({"paths": [text(&admin.join("logs/HEAD"))], "count": 1}),
        "{a}"
    );
    assert_eq!(a["updates"], 0);
    assert_eq!(calls_with(&f, from, "merge"), 0);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "head kept\n");
    // The stop repeats while the link stays.
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("git_dir_link");
    assert_eq!(calls_with(&f, from, "merge"), 0);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "head kept\n");
}

// spec: specs/ap/unit-run.md#R40
#[test]
fn r40_two_links_are_named_in_the_order_of_their_bytes() {
    let mut f = accepted_and_moved();
    let (g, admin) = f.git_dirs();
    let (merge_msg, reflog) = (target(&f, "msg"), target(&f, "reflog"));
    link_to(&admin.join("MERGE_MSG"), &merge_msg);
    let branch_log = g.join("logs/refs/heads/unit/widgets-list");
    assert!(branch_log.is_file(), "the branch has a reflog");
    link_to(&branch_log, &reflog);
    let from = f.calls.len();
    let a = f.stopped("git_dir_link");
    assert_eq!(
        detail(&a),
        json!({"paths": [text(&branch_log), text(&admin.join("MERGE_MSG"))], "count": 2}),
        "{a}"
    );
    assert_eq!(calls_with(&f, from, "merge"), 0);
    assert_eq!(std::fs::read_to_string(&merge_msg).unwrap(), "msg kept\n");
    assert_eq!(std::fs::read_to_string(&reflog).unwrap(), "reflog kept\n");
}

// spec: specs/ap/unit-run.md#R40
#[test]
fn r40_a_link_at_the_reflog_of_the_branch_stops_before_the_worktree_is_made() {
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    let (g, _) = f.git_dirs();
    let outside = target(&f, "branch");
    let branch_log = g.join("logs/refs/heads/unit/widgets-list");
    link_to(&branch_log, &outside);
    let a = f.stopped("git_dir_link");
    assert_eq!(
        detail(&a),
        json!({"paths": [text(&branch_log)], "count": 1}),
        "{a}"
    );
    assert_eq!(calls_with(&f, 0, "add"), 0, "no `git worktree add`");
    assert!(!f.wt().exists());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "branch kept\n");
    // Without a link the Git directory is asked for once, immediately before the add.
    let mut f = F::new();
    f.steps(&["implement", "accept"]);
    f.ok();
    let add = f
        .calls
        .iter()
        .position(|c| {
            c.0 == "git" && c.1.iter().any(|w| w == "add") && c.1.contains(&"worktree".into())
        })
        .unwrap();
    let before = &f.calls[add - 1];
    let clone = f.p("clone");
    let words = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    let at = before.1.iter().position(|w| w == "-C").unwrap();
    assert_eq!(before.0, "git");
    assert_eq!(before.1[at + 1], clone, "{:?}", before.1);
    assert_eq!(before.1[at + 2..at + 6], SAFE, "{:?}", before.1);
    assert_eq!(before.1[at + 6..], words, "{:?}", before.1);
    let asked = f.calls[..add]
        .iter()
        .filter(|c| c.1.iter().any(|w| w == "--git-common-dir"))
        .count();
    assert_eq!(asked, 1);
}

// spec: specs/ap/unit-run.md#R40
// spec: specs/ap/unit-run.md#R32
#[test]
fn r40_the_cleanup_keeps_a_worktree_whose_git_directory_holds_a_link() {
    for state in ["merged", "dropped"] {
        let mut f = F::new();
        f.steps(&["implement", "repair", "nothing"]);
        f.stopped("repair_refused");
        let (_, admin) = f.git_dirs();
        let (dir, file) = (f.t.join("outside-dir"), f.t.join("outside-dir/inner"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "inner kept\n").unwrap();
        let link = admin.join("a-link");
        link_to(&link, &dir);
        f.set_unit(json!({"state": state}));
        let from = f.calls.len();
        let (a, status) = f.ok();
        assert_eq!(status, 0, "{a}");
        assert_eq!(a["cleanup"]["worktree"], "worktree_kept", "{state}: {a}");
        assert_eq!(
            a["cleanup"]["worktree_reason"],
            format!("link in the git directory: {}", link.display()),
            "{a}"
        );
        assert_eq!(
            calls_with(&f, from, "remove"),
            0,
            "no `git worktree remove`"
        );
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(std::fs::read_link(&link).unwrap(), dir);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "inner kept\n");
        assert!(f.wt().join(".git").is_file());
    }
}

// spec: specs/ap/unit-run.md#R40
#[test]
fn r40_a_link_at_the_ref_of_the_branch_is_not_followed() {
    let mut f = accepted_and_moved();
    let (g, _) = f.git_dirs();
    let main = git(&f.t.join("clone"), &["rev-parse", "refs/heads/main"]);
    let reference = g.join("refs/heads/unit/widgets-list");
    std::fs::remove_file(&reference).unwrap();
    std::os::unix::fs::symlink("refs/heads/main", &reference).unwrap();
    let from = f.calls.len();
    let a = f.stopped("git_dir_link");
    assert_eq!(
        detail(&a),
        json!({"paths": [text(&reference)], "count": 1}),
        "{a}"
    );
    assert_eq!(calls_with(&f, from, "merge"), 0);
    assert_eq!(
        git(&f.t.join("clone"), &["rev-parse", "refs/heads/main"]),
        main
    );
}

// spec: specs/ap/unit-run.md#R40
// spec: specs/ap/unit-run.md#R32
#[test]
fn r40_a_git_directory_of_more_than_ten_thousand_entries_is_unreadable_and_no_link() {
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    let (_, admin) = f.git_dirs();
    let many = admin.join("many");
    std::fs::create_dir_all(&many).unwrap();
    for n in 0..10_001 {
        std::fs::File::create(many.join(format!("e{n}"))).unwrap();
    }
    let unreadable = format!("git directory unreadable: {}", admin.display());
    f.set_unit(json!({"state": "stopped"}));
    let a = f.stopped("worktree_failed");
    assert_eq!(a["stop_detail"], unreadable.as_str(), "{a}");
    // The bound comes before a link, which would answer `git_dir_link` after a whole walk.
    let outside = target(&f, "bound");
    link_to(&admin.join("logs/HEAD"), &outside);
    f.set_unit(json!({"state": "stopped"}));
    let a = f.stopped("worktree_failed");
    assert_eq!(a["stop_detail"], unreadable.as_str(), "{a}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "bound kept\n");
    // The cleanup of a merged unit keeps the worktree with the same reason.
    f.set_unit(json!({"state": "merged"}));
    let from = f.calls.len();
    let (a, _) = f.ok();
    assert_eq!(a["cleanup"]["worktree"], "worktree_kept", "{a}");
    assert_eq!(a["cleanup"]["worktree_reason"], unreadable.as_str(), "{a}");
    assert_eq!(calls_with(&f, from, "remove"), 0);
    // Exactly the bound is read: 10,000 entries below the Git directory pass.
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    let (_, admin) = f.git_dirs();
    let before = walk_count(&admin);
    let many = admin.join("many");
    std::fs::create_dir_all(&many).unwrap();
    for n in 0..(10_000 - before - 1) {
        std::fs::File::create(many.join(format!("e{n}"))).unwrap();
    }
    assert_eq!(walk_count(&admin), 10_000);
    f.set_unit(json!({"state": "stopped"}));
    f.stopped("repair_refused");
}
/// The entries below `dir`, every level.
fn walk_count(dir: &Path) -> usize {
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        n += 1;
        if e.file_type().unwrap().is_dir() {
            n += walk_count(&e.path());
        }
    }
    n
}

// spec: specs/ap/unit-run.md#R40
#[test]
fn r40_the_path_of_a_link_is_written_with_the_replacement_character_for_a_byte_that_is_no_utf8() {
    use std::os::unix::ffi::OsStrExt;
    let mut f = F::new();
    f.steps(&["implement", "repair", "nothing"]);
    f.stopped("repair_refused");
    let (_, admin) = f.git_dirs();
    let outside = target(&f, "ff");
    let name = std::ffi::OsStr::from_bytes(b"bad\xff");
    link_to(&admin.join(name), &outside);
    f.set_unit(json!({"state": "stopped"}));
    let a = f.stopped("git_dir_link");
    assert_eq!(
        detail(&a),
        json!({"paths": [format!("{}/bad\u{FFFD}", admin.display())], "count": 1}),
        "{a}"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "ff kept\n");
}

// ---- The credential of a stage, the description, the comment, the status as the Lead's App -----

/// Every regular file under `dir`, read; no link followed.
fn files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let m = std::fs::symlink_metadata(&p).unwrap();
            if m.is_dir() {
                todo.push(p);
            } else if m.is_file() {
                let bytes = std::fs::read(&p).unwrap();
                out.push((p, bytes));
            }
        }
    }
    out
}
/// No byte of `secret` in the answer, a call of `ont` (an action's body, a read), a file under the
/// units directory (prompts, records, answers, the credential and description files) or the reviews.
fn nowhere(f: &F, a: &Value, secret: &str) {
    assert!(!a.to_string().contains(secret), "{a}");
    for c in f.calls.iter().filter(|c| c.0 == "ont") {
        let words = c.1.join(" ") + &c.2.to_string();
        assert!(!words.contains(secret), "{secret} in {c:?}");
    }
    for dir in [f.t.join("units"), f.t.join("reviews")] {
        for (p, bytes) in files(&dir) {
            let found = bytes.windows(secret.len()).any(|w| w == secret.as_bytes());
            assert!(!found, "{secret} in {}", p.display());
        }
    }
}
/// The value of `name` in the environment of launch `i` (from 0).
fn env_of(f: &F, i: usize, name: &str) -> Option<String> {
    let env = &f.launch_envs[i];
    env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}
/// The reviews of kind `packet` by their round (two may start in one second).
fn by_round(f: &F) -> Vec<(String, Value)> {
    let mut list = f.packets();
    list.sort_by_key(|(_, r)| r["round"].as_u64());
    list
}
/// The records of the stages 1 to 4 of `implement, repair, fix, accept`, in order.
fn four_records(f: &F) -> Vec<Value> {
    let read =
        |p: PathBuf| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    let reviews: Vec<Value> = (by_round(f).into_iter())
        .map(|(k, _)| read(f.t.join(format!("reviews/{k}.md.run.json"))))
        .collect();
    vec![
        read(f.home().join("1.implementer.md.run.json")),
        reviews[0].clone(),
        read(f.home().join("3.fixer.md.run.json")),
        reviews[1].clone(),
    ]
}
/// Base64url without padding, decoded.
fn unb64url(t: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let (mut bits, mut n, mut out) = (0u32, 0, vec![]);
    for c in t.bytes() {
        let v = A
            .iter()
            .position(|a| *a == c)
            .unwrap_or_else(|| panic!("{t}")) as u32;
        (bits, n) = ((bits << 6) | v, n + 6);
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    out
}
/// The words of the status call of before (R42): no header.
fn status_words(head: &str, state: &str, description: &str) -> Vec<String> {
    [
        "api",
        "--method",
        "POST",
        &format!("repos/invented/widgets/statuses/{head}"),
        "-f",
        &format!("state={state}"),
        "-f",
        "context=lead-review",
        "-f",
        &format!("description={description}"),
    ]
    .map(str::to_owned)
    .to_vec()
}
/// The status calls of `gh`, by their words.
fn status_calls(f: &F) -> Vec<Vec<String>> {
    let posts = f.calls.iter().filter(|c| c.0 == "gh");
    let posts = posts.filter(|c| c.1.iter().any(|w| w.contains("/statuses/")));
    posts.map(|c| c.1.clone()).collect()
}

// spec: specs/ap/unit-run.md#R41
// spec: specs/ap/unit-run.md#R3
#[test]
fn r41_a_policy_without_the_leads_app_is_refused_before_anything() {
    let units = |f: &F, g: Option<Value>| {
        let mut u = json!({"directory": f.p("units")});
        if let Some(g) = g {
            u["github"] = g;
        }
        json!({"units": u})
    };
    let tokens = json!({"invented/widgets": "/invented/widgets.token"});
    for g in [None, Some(json!({})), Some(json!({"stage_tokens": tokens}))] {
        let mut f = F::new();
        f.policy(units(&f, g.clone()));
        let e = f.refusal();
        assert_eq!(code(&e), ("github_not_mapped", "lead_app", 2), "{g:?}");
        assert!(
            f.actions().is_empty() && !f.t.join("units").exists(),
            "{g:?}"
        );
        assert_eq!(f.launches, 0);
    }
    // Each malformed member, named by the reader of `units` (R3), before the check of R41.
    let app = json!({"id": 1, "key": "/invented/app.pem"});
    let many: serde_json::Map<String, Value> = (0..33)
        .map(|i| (format!("invented/r{i}"), json!("/invented/t")))
        .collect();
    let lead = "units.github.lead_app";
    let st = "units.github.stage_tokens";
    for (g, member) in [
        (json!([]), "units.github"),
        (json!({"other": 1}), "units.github.other"),
        (json!({"lead_app": {"id": 0, "key": "/k"}}), lead),
        (
            json!({"lead_app": {"id": 4_294_967_296_u64, "key": "/k"}}),
            lead,
        ),
        (json!({"lead_app": {"id": -1, "key": "/k"}}), lead),
        (json!({"lead_app": {"id": 1.5, "key": "/k"}}), lead),
        (json!({"lead_app": {"id": "1", "key": "/k"}}), lead),
        (json!({"lead_app": {"id": 1}}), lead),
        (json!({"lead_app": {"key": "/k"}}), lead),
        (json!({"lead_app": {"id": 1, "key": "relative"}}), lead),
        (json!({"lead_app": {"id": 1, "key": "/a b"}}), lead),
        (
            json!({"lead_app": {"id": 1, "key": "/k", "other": 1}}),
            lead,
        ),
        (json!({"lead_app": [1, "/k"]}), lead),
        (json!({"lead_app": app, "stage_tokens": {}}), st),
        (json!({"lead_app": app, "stage_tokens": many}), st),
        (json!({"lead_app": app, "stage_tokens": []}), st),
        (
            json!({"lead_app": app, "stage_tokens": {"widgets": "/t"}}),
            st,
        ),
        (
            json!({"lead_app": app, "stage_tokens": {"invented/..": "/t"}}),
            st,
        ),
        (
            json!({"lead_app": app, "stage_tokens": {"in vented/w": "/t"}}),
            st,
        ),
        (
            json!({"lead_app": app, "stage_tokens": {"invented/widgets": "t"}}),
            st,
        ),
        (
            json!({"lead_app": app, "stage_tokens": {"invented/widgets": 1}}),
            st,
        ),
    ] {
        let mut f = F::new();
        f.policy(units(&f, Some(g.clone())));
        let e = f.refusal();
        assert_eq!(code(&e), ("policy_invalid", "policy", 2), "{g}");
        let detail = e.detail.as_str().unwrap();
        assert!(detail.starts_with(member), "{g}: {detail}");
        assert!(f.actions().is_empty() && !f.t.join("units").exists(), "{g}");
    }
    // The bounds themselves pass: the largest id, 32 repositories; no file the member names opened.
    let mut f = F::new();
    let mut tokens: serde_json::Map<String, Value> = (0..31)
        .map(|i| (format!("invented/r{i}"), json!("/invented/absent/t")))
        .collect();
    tokens.insert("in-vented/w.x_y".into(), json!("/invented/absent/u"));
    let g = json!({"lead_app": {"id": 4_294_967_295_u64, "key": "/invented/absent/app.pem"},
        "stage_tokens": tokens});
    f.policy(units(&f, Some(g)));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
}

// spec: specs/ap/unit-run.md#R41
// spec: specs/ap/unit-run.md#R38
// spec: specs/ap/unit-run.md#R25
// spec: specs/ap/unit-run.md#R42
#[test]
fn r41_an_author_of_a_repository_without_a_stage_token_keeps_the_hosts_login() {
    for other in [false, true] {
        let mut f = F::new();
        if other {
            let tokens = json!({"invented/other": f.p("secrets/other.token")});
            let units = json!({"directory": f.p("units"), "github": f.github(Some(tokens))});
            f.policy(json!({"units": units}));
        }
        f.steps(&["implement", "repair", "fix", "accept"]);
        let (a, _) = f.ok();
        assert_eq!(a["state"], "merged", "{a}");
        let want = json!([
            {"stage": 1, "role": "implementer", "credential": "host"},
            {"stage": 3, "role": "fixer", "credential": "host"}
        ]);
        assert_eq!(a["credentials"], want, "{a}");
        let records = four_records(&f);
        for (i, author) in [true, false, true, false].into_iter().enumerate() {
            let n = i + 1;
            assert_eq!(env_of(&f, i, "GH_TOKEN"), None, "{n}");
            let file = f.home().join(format!("{n}.credential"));
            match author {
                true => {
                    assert_eq!(std::fs::read_to_string(&file).unwrap(), "host\n");
                    assert_eq!(mode(&file), 0o600);
                }
                false => assert!(!file.exists(), "{n}"),
            }
            let show = author.then(|| json!(["~/.config/gh"]));
            assert_eq!(records[i]["limits"].get("show").cloned(), show, "{n}");
        }
        // R42: the base's one call, word for word, and no signature.
        let (key, _) = by_round(&f).pop().unwrap();
        let head = a["head"].as_str().unwrap();
        let description = format!("review {key} accept");
        assert_eq!(
            status_calls(&f),
            [status_words(head, "success", &description)]
        );
        assert_eq!(f.status_headers, [None]);
        assert!(!f.calls.iter().any(|c| c.0 == "sh" && c.1[1] == SIGN));
    }
}

// spec: specs/ap/unit-run.md#R41
// spec: specs/ap/unit-run.md#R38
// spec: specs/ap/unit-run.md#R25
#[test]
fn r41_a_stage_token_reaches_each_author_stage_alone_and_is_read_anew() {
    let mut f = F::new();
    let (first, second) = (
        "github_pat_InventedStageToken01",
        "github_pat_InventedStageToken02",
    );
    // A final carriage return and line feed are dropped.
    let path = f.token(&format!("{first}\r\n"));
    // The implementer's stage replaces the token: the fixer reads the new one.
    let edit =
        format!("mkdir -p src; echo widget >> src/widgets.rs; printf '%s\\n' {second} > {path}\n");
    std::fs::write(f.t.join("bin/edit.sh"), edit).unwrap();
    f.steps(&["edit", "repair", "fix", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let want = json!([
        {"stage": 1, "role": "implementer", "credential": "token"},
        {"stage": 3, "role": "fixer", "credential": "token"}
    ]);
    assert_eq!(a["credentials"], want, "{a}");
    let tokens = [Some(first), None, Some(second), None];
    let records = four_records(&f);
    for (i, token) in tokens.into_iter().enumerate() {
        let n = i + 1;
        assert_eq!(env_of(&f, i, "GH_TOKEN").as_deref(), token, "{n}");
        // No stage with a token, and no verifier, is shown the state of `gh`.
        assert!(records[i]["limits"].get("show").is_none(), "{n}");
        assert!(records[i]["limits"]["scratch"].is_string(), "{n}");
        let file = f.home().join(format!("{n}.credential"));
        match token {
            Some(_) => assert_eq!(std::fs::read_to_string(&file).unwrap(), "token\n"),
            None => assert!(!file.exists(), "{n}"),
        }
    }
    // The verifier's view: no `GH_TOKEN`, and the Cargo home still there.
    assert!(env_of(&f, 1, "CARGO_HOME").is_some());
    nowhere(&f, &a, first);
    nowhere(&f, &a, second);
    // A settling stage is an author's: it gets the token too.
    let mut f = F::new();
    let token = "github_pat_InventedStageToken03";
    f.token(token);
    f.steps(&["dirty", "settle", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    assert_eq!(
        [0, 1, 2].map(|i| env_of(&f, i, "GH_TOKEN")),
        [Some(token.into()), Some(token.into()), None]
    );
    let want = json!([
        {"stage": 1, "role": "implementer", "credential": "token"},
        {"stage": 2, "role": "implementer", "credential": "token"}
    ]);
    assert_eq!(a["credentials"], want, "{a}");
    nowhere(&f, &a, token);
}

// spec: specs/ap/unit-run.md#R41
// spec: specs/ap/unit-run.md#R22
#[test]
fn r41_a_stage_token_that_cannot_be_read_or_has_another_form_stops_before_any_launch() {
    let long = format!("github_pat_{}", "a".repeat(256));
    let cases: [(Option<&str>, &str); 7] = [
        (None, "read"),
        (Some("gho_InventedLogin1\n"), "form"),
        (Some(""), "form"),
        (Some("\n"), "form"),
        (Some("github_pat_InventedA\ngithub_pat_InventedB\n"), "form"),
        (Some("github_pat_Invented-Dash"), "form"),
        (Some(&long), "form"),
    ];
    for (text, reason) in cases {
        let mut f = F::new();
        let path = f.token(text.unwrap_or_default());
        if text.is_none() {
            std::fs::remove_file(&path).unwrap();
        }
        f.steps(&["implement", "accept"]);
        let a = f.stopped("stage_token_unreadable");
        let detail = json!({"reason": reason}).to_string();
        assert_eq!(a["stop_detail"], detail, "{text:?}");
        assert_eq!(f.unit()["stop_detail"], detail);
        assert_eq!(f.launches, 0, "{text:?}");
        assert!(!f.home().join("1.credential").exists());
        assert!(a.get("credentials").is_none(), "{a}");
        if let Some(t) = text.map(str::trim_end).filter(|t| !t.is_empty()) {
            nowhere(&f, &a, t.lines().next().unwrap());
        }
    }
}

// spec: specs/ap/unit-run.md#R42
// spec: specs/ap/unit-run.md#R31
#[test]
fn r42_the_status_is_posted_as_the_leads_app_with_a_fresh_installation_token() {
    let mut f = F::new();
    let token = "github_pat_InventedStageToken04";
    f.token(token);
    let wall = SystemTime::now();
    f.wall = Some(wall);
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let w = wall.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let at = f.first(|c| c.0 == "sh" && c.1[1] == SIGN).unwrap();
    let calls = &f.calls[at..at + 5];
    // (b) The signature: the fixed script, the input and the key's path as arguments, no input.
    let b = &calls[0];
    assert_eq!(b.1.len(), 5, "{b:?}");
    assert_eq!(b.1[..3], ["-c", SIGN, "sh"].map(str::to_owned));
    assert_eq!(b.1[4], f.p("secrets/app.pem"));
    assert!(b.2.is_null());
    // (a) The signing input: the header and the claims, base64url without padding.
    let input = &b.1[3];
    let (header, claims) = input.split_once('.').unwrap();
    assert_eq!(unb64url(header), br#"{"alg":"RS256","typ":"JWT"}"#);
    let want = format!(r#"{{"iat":{},"exp":{},"iss":"{APP_ID}"}}"#, w - 60, w + 540);
    assert_eq!(String::from_utf8(unb64url(claims)).unwrap(), want);
    assert!(!input.contains(['=', '+', '/']), "{input}");
    // (c) The installation, with the JWT: the input, a `.` and the signature.
    let c = &calls[1];
    let bearer = c.1[2].clone();
    let jwt = bearer.strip_prefix("Authorization: Bearer ").unwrap();
    let (signed, signature) = jwt.rsplit_once('.').unwrap();
    assert_eq!(
        (signed, unb64url(signature).as_slice()),
        (input.as_str(), SIGNATURE)
    );
    assert_eq!(
        (c.0.as_str(), c.1.clone(), c.2.clone()),
        (
            "gh",
            ["api", "-H", &bearer, "repos/invented/widgets/installation"]
                .map(str::to_owned)
                .to_vec(),
            Value::Null
        )
    );
    // (d) The installation token for this repository and the statuses alone.
    let d = &calls[2];
    let path = format!("app/installations/{INSTALLATION}/access_tokens");
    let words = [
        "api", "--method", "POST", "-H", &bearer, &path, "--input", "-",
    ];
    assert_eq!(
        (d.0.as_str(), d.1.clone()),
        ("gh", words.map(str::to_owned).to_vec())
    );
    assert_eq!(
        d.2,
        json!({"repositories": ["widgets"], "permissions": {"statuses": "write"}})
    );
    // (e) The status call of before, with the installation token's header after `api`.
    let (key, _) = f.packets().pop().unwrap();
    let head = a["head"].as_str().unwrap();
    let mut words = status_words(head, "success", &format!("review {key} accept"));
    let header = format!("Authorization: Bearer {INSTALLATION_TOKEN}");
    words.splice(1..1, ["-H".to_owned(), header.clone()]);
    assert_eq!((calls[3].0.as_str(), &calls[3].1), ("gh", &words));
    assert_eq!(f.status_headers, [Some(header)]);
    assert!(!calls[3].1.iter().any(|w| w.contains(jwt)));
    // Then the merge pinned to the head.
    assert_eq!(calls[4].1[..2], ["pr", "merge"].map(str::to_owned));
    // The JWT and the installation token are words of (c) to (e) alone, and nowhere else.
    let with = |t: &str| {
        let calls = f.calls.iter();
        calls
            .filter(|c| c.1.iter().any(|w| w.contains(t)) || c.2.to_string().contains(t))
            .count()
    };
    assert_eq!((with(jwt), with(INSTALLATION_TOKEN)), (2, 1));
    for secret in [jwt, INSTALLATION_TOKEN, token, signature] {
        nowhere(&f, &a, secret);
    }
    // A failure at each step: `status_failed`, nothing called or merged after it.
    let steps = [
        ("openssl", "sh", Some("invented: unable to load key")),
        ("openssl-missing", "sh", Some("")),
        ("installation", "/installation", None),
        (
            "installation-missing",
            "/installation",
            Some("installation"),
        ),
        ("access_tokens", "/access_tokens", None),
        (
            "access_tokens-missing",
            "/access_tokens",
            Some("access_tokens"),
        ),
        ("status", "/statuses/", None),
    ];
    for (step, call, detail) in steps {
        let mut f = F::new();
        f.token(token);
        f.gh_says = LEAKY;
        f.app_fails = Some(step);
        f.steps(&["implement", "accept"]);
        let (a, status) = f.ok();
        assert_eq!(
            (a["state"].as_str(), a["refused"].as_str(), status),
            (Some("accepted"), Some("status_failed"), 3),
            "{step}: {a}"
        );
        assert_eq!(f.merges(), (0, 0), "{step}");
        assert!(f.statuses.is_empty(), "{step}");
        let failed = f.calls.iter().rposition(|c| match call {
            "sh" => c.0 == "sh" && c.1[1] == SIGN,
            _ => c.0 == "gh" && c.1.iter().any(|w| w.contains(call)),
        });
        let failed = failed.unwrap_or_else(|| panic!("{step}"));
        let later = f.calls[failed + 1..].iter();
        assert!(
            !later.clone().any(|c| c.0 == "gh" || c.0 == "sh"),
            "{step}: {:?}",
            later.collect::<Vec<_>>()
        );
        let said = a["detail"].as_str().unwrap();
        match detail {
            Some(d) => assert_eq!(said, d, "{step}"),
            None => redacted(said),
        }
        nowhere(&f, &a, token);
    }
}

// spec: specs/ap/unit-run.md#R42
// spec: specs/ap/unit-run.md#R34
#[test]
fn r42_reject_posts_as_the_app_where_a_stage_token_exists_and_else_as_before() {
    let accepted = |tokens: bool| {
        let mut f = F::new();
        if tokens {
            f.token("github_pat_InventedStageToken05");
        }
        f.set_unit(json!({"class": "contract"}));
        f.steps(&["implement", "accept"]);
        let (a, _) = f.ok();
        assert_eq!(a["waits_for"], "approve", "{a}");
        (f, a["head"].as_str().unwrap().to_owned())
    };
    let reject = |f: &mut F, id: &str| {
        let r = json!({"request_id": id, "reason": "The list shows a widget twice."});
        f.decide("reject", "lead-1", r).unwrap()["lead_review"].clone()
    };
    // As the App: the signature, the installation, its token, the status with its header.
    let (mut f, head) = accepted(true);
    assert!(status_calls(&f).is_empty());
    assert_eq!(reject(&mut f, "rj-app"), "failure");
    let header = format!("Authorization: Bearer {INSTALLATION_TOKEN}");
    let mut words = status_words(&head, "failure", "rejected by the Lead");
    words.splice(1..1, ["-H".to_owned(), header.clone()]);
    assert_eq!(status_calls(&f), [words]);
    assert_eq!(f.status_headers, [Some(header)]);
    assert_eq!(
        f.calls
            .iter()
            .filter(|c| c.0 == "sh" && c.1[1] == SIGN)
            .count(),
        1
    );
    // A failed step of the App: `not_posted`, the unit stopped as before.
    let (mut f, _) = accepted(true);
    f.app_fails = Some("installation");
    assert_eq!(reject(&mut f, "rj-fail"), "not_posted");
    assert_eq!(f.unit()["state"], "stopped");
    assert!(status_calls(&f).is_empty());
    // Without `lead_app` (reject does not refuse it), and for a repository `stage_tokens` does
    // not name: the base's one call, word for word.
    for policy in ["no-app", "other"] {
        let (mut f, head) = accepted(false);
        let github = match policy {
            "no-app" => json!({}),
            _ => f.github(Some(json!({"invented/other": f.p("secrets/other.token")}))),
        };
        f.policy(json!({"units": {"directory": f.p("units"), "github": github}}));
        assert_eq!(reject(&mut f, &format!("rj-{policy}")), "failure");
        let words = status_words(&head, "failure", "rejected by the Lead");
        assert_eq!(status_calls(&f), [words], "{policy}");
        assert_eq!(f.status_headers, [None]);
        assert!(!f.calls.iter().any(|c| c.0 == "sh" && c.1[1] == SIGN));
    }
}

// spec: specs/ap/unit-run.md#R19
// spec: specs/ap/unit-run.md#R24
#[test]
fn r19_each_verifier_round_reads_the_description_the_run_copied_before_it() {
    let line = |f: &F, n: usize| {
        format!(
            "Description of the pull request: {} (its body as the run read it before this round; a CLAIM to check, not a fact)\n",
            f.home().join(format!("{n}.description.md")).display()
        )
    };
    let mut f = F::new();
    // A login and a stage token an author wrote into the body: redacted, the rest byte for byte.
    f.body = Some(
        "An invented description: github_pat_InventedBody9_x and gho_InventedBody8, é ghp_ end.\r\n\n"
            .into(),
    );
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let file = f.home().join("2.description.md");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "An invented description: <redacted> and <redacted>, é ghp_ end.\r\n\n"
    );
    assert_eq!(mode(&file), 0o600);
    let prompt = std::fs::read_to_string(f.home().join("2.verifier.prompt.md")).unwrap();
    let want = format!(
        "\n## The change\n\nRepository: invented/widgets\nPull request: 7\nHead: {}\ngit diff origin/main...HEAD\n{}",
        a["head"].as_str().unwrap(),
        line(&f, 2)
    );
    assert!(prompt.ends_with(&want), "{prompt}");
    assert!(!prompt.contains("## The round") && !prompt.contains("Answer of the author:"));
    // The read, with the host's login, before the round's review starts.
    let read = f
        .first(|c| c.0 == "gh" && c.1.ends_with(&["--json".into(), "body".into()]))
        .unwrap();
    let words = [
        "pr",
        "view",
        "7",
        "--repo",
        "invented/widgets",
        "--json",
        "body",
    ];
    assert_eq!(f.calls[read].1, words.map(str::to_owned));
    let start = f
        .first(|c| c.0 == "ont" && c.2["action_type"] == "start_unit_review")
        .unwrap();
    assert!(read < start);
    // A second look after a repair: a new description file, and the fixer's answer named.
    let mut f = F::new();
    f.steps(&["implement", "repair", "fix", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let packets = f.packets();
    let repair = packets
        .iter()
        .find(|(_, r)| r["verdict"] == "repair")
        .unwrap();
    let judged = repair.1["head"].as_str().unwrap().to_owned();
    let prompt = std::fs::read_to_string(f.home().join("4.verifier.prompt.md")).unwrap();
    let round = format!(
        "{}\n{SECOND_LOOK}\n## The round\n\nHead judged before: {judged}\nWhat moved it: the author's repair\nAnswer of the author: {}\n",
        line(&f, 4),
        f.home().join("3.fixer.md").display()
    );
    assert!(prompt.ends_with(&round), "{prompt}");
    for n in [2, 4] {
        assert!(
            f.home().join(format!("{n}.description.md")).is_file(),
            "{n}"
        );
    }
    let reads = f.calls.iter().filter(|c| c.1.ends_with(&["body".into()]));
    assert_eq!(reads.count(), 2);
    // A second look after an update alone: no answer of the author.
    let mut f = F::new();
    f.set_unit(json!({"class": "contract"}));
    f.steps(&["implement", "acceptmain", "accept"]);
    f.ok();
    f.ok();
    let (_, prompt) = f.launch(3);
    assert!(prompt.contains("## The round"), "{prompt}");
    assert!(
        prompt.contains("What moved it: a merge of origin/main\n"),
        "{prompt}"
    );
    assert!(!prompt.contains("Answer of the author:"), "{prompt}");
    assert!(
        prompt.contains("Description of the pull request: "),
        "{prompt}"
    );
    // A read that fails, and an answer without `body`: no review started, no verifier launched.
    for broken in ["fails", "absent"] {
        let mut f = F::new();
        f.gh_says = LEAKY;
        match broken {
            "fails" => f.body_fails = true,
            _ => f.body = None,
        }
        f.steps(&["implement", "accept"]);
        let a = f.stopped("github_unreadable");
        let d: Value = serde_json::from_str(a["stop_detail"].as_str().unwrap()).unwrap();
        assert_eq!(d["call"], "pr view", "{broken}: {d}");
        if broken == "fails" {
            redacted(d["reply"].as_str().unwrap());
        }
        let validated = f.first(|c| c.0 == "ont" && c.2["action_type"] == "start_unit_review");
        assert!(validated.is_none(), "{broken}");
        assert!(f.packets().is_empty() && f.launches == 1, "{broken}");
        assert!(!f.home().join("2.description.md").exists());
    }
}

// spec: specs/ap/unit-run.md#R43
// spec: specs/ap/unit-run.md#R23
#[test]
fn r43_the_report_of_each_closed_round_is_posted_as_one_comment() {
    let comment = |f: &F, key: &str| -> Vec<String> {
        let report = f.p(&format!("reviews/{key}.md"));
        let words = [
            "pr",
            "comment",
            "7",
            "--repo",
            "invented/widgets",
            "--body-file",
        ];
        let mut w = words.map(str::to_owned).to_vec();
        w.push(report);
        w
    };
    let mut f = F::new();
    f.steps(&["implement", "repair", "fix", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let packets = by_round(&f);
    let want: Vec<Vec<String>> = packets.iter().map(|(k, _)| comment(&f, k)).collect();
    assert_eq!(f.comments, want);
    // Each after its round closed, before the next stage.
    for (k, _) in &packets {
        let closed = f
            .first(|c| {
                c.0 == "ont"
                    && c.2["action_type"] == "close_unit_review"
                    && c.2["parameters"]["review"]["native_key"] == k.as_str()
            })
            .unwrap();
        let posted = f.first(|c| c.0 == "gh" && c.1 == comment(&f, k)).unwrap();
        assert!(closed < posted, "{k}");
    }
    // An abandoned review: no comment.
    let mut f = F::new();
    f.steps(&["implement", "noform", "noform"]);
    f.stopped("verifier_failed");
    assert!(f.comments.is_empty());
    // A carried accept: the one verifier round's comment alone.
    let mut f = F::new();
    f.steps(&["implement", "acceptoutside"]);
    let (a, _) = f.ok();
    assert!(a["carried"].is_object(), "{a}");
    let judged: Vec<String> = (f.packets().into_iter())
        .filter(|(_, r)| r["carried_from"].is_null())
        .map(|(k, _)| k)
        .collect();
    assert_eq!(judged.len(), 1);
    assert_eq!(f.comments, [comment(&f, &judged[0])]);
    // A comment that fails changes nothing: no stop, no second call, no member of the answer.
    let mut f = F::new();
    f.comment_fails = true;
    f.steps(&["implement", "accept"]);
    let (a, status) = f.ok();
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
    assert_eq!(f.comments.len(), 1);
    assert!(
        a.get("detail").is_none() && a.get("refused").is_none(),
        "{a}"
    );
}

// spec: specs/ap/unit-run.md#R44
#[test]
fn r44_a_line_of_a_token_form_stops_the_unit_before_its_verifier_and_an_admission_takes_it() {
    // The token is built at run time, so that no line of this file holds the form.
    let token = format!("{}{}_{}", "gh", "p", "aB3dE5".repeat(6));
    let edit = format!("mkdir docs; printf '%s\\n' '{token}' > docs/fixture.txt");
    let mut f = F::new();
    let (a, status) = f.edit("plain", json!(["docs/**"]), &[], &edit);
    assert_eq!((a["state"].as_str(), status), (Some("stopped"), 3), "{a}");
    assert_eq!(a["stop_cause"], "secret_in_diff", "{a}");
    assert_eq!(
        detail(&a),
        json!({"lines": [{"path": "docs/fixture.txt", "line": 1, "form": "github_token"}], "count": 1}),
        "{a}"
    );
    for piece in token.as_bytes().windows(8) {
        let piece = std::str::from_utf8(piece).unwrap();
        assert!(!a.to_string().contains(piece), "{piece}");
    }
    assert!(f.packets().is_empty());
    assert_eq!(f.merges(), (0, 0));
    // The same edit with the admission of that path and that line's digest is merged.
    let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
    let mut f = F::new();
    f.set_unit(json!({"notes": format!("Admitted line: docs/fixture.txt {digest}")}));
    let (a, status) = f.edit("plain", json!(["docs/**"]), &[], &edit);
    assert_eq!((a["state"].as_str(), status), (Some("merged"), 0), "{a}");
}

// ---- A look on the PC (specs/ap/unit-run.md R45) ----------------------------------------------

/// The policy with `hosts.pc.stages`, and a `tailscale` that never answers: a look that asks for the
/// PC runs here with `pc_unreachable`, so its record's `host_reason` shows the request had `host`.
fn pc_world(f: &mut F, capacity: Option<Value>) {
    f.policy(
        json!({"hosts": {"pc": {"ssh": "u@pc-1", "work": f.p("pcchecks"), "busy_load": 0.5,
        "stages": {"ssh": "stage@pc-1", "work": f.p("pcwork")}}}}),
    );
    let stand_in = f.t.join("bin/tailscale");
    std::fs::write(&stand_in, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
    forks::settled();
    let path = format!(
        "{}:{}",
        f.p("bin"),
        std::env::var("PATH").unwrap_or_default()
    );
    f.env.retain(|(k, _)| k != "PATH");
    f.env.push(("PATH".into(), path));
    f.capacity = capacity;
}
fn capacity_of(pc_free: bool, local_load_x100: u64) -> Value {
    json!({"hosts": [
        {"host": "local", "machine": "vps", "cpus": 2, "load_x100": local_load_x100, "available_mb": 1000, "free": true},
        {"host": "pc", "machine": "desk", "cpus": 8, "load_x100": 10, "available_mb": 9000, "free": pc_free}],
        "machines": [], "accounts": [], "can_start": if pc_free { json!(["desk"]) } else { json!([]) }})
}
fn look_records(f: &F) -> Vec<Value> {
    let mut packets = f.packets();
    packets.sort_by_key(|(_, v)| v["round"].as_u64());
    packets
        .iter()
        .map(|(key, _)| {
            serde_json::from_slice(
                &std::fs::read(f.p(&format!("reviews/{key}.md.run.json"))).unwrap(),
            )
            .unwrap()
        })
        .collect()
}

// spec: specs/ap/unit-run.md#R45
#[test]
fn r45_a_free_pc_with_a_loaded_vps_gives_the_look_host_pc_and_the_description_to_read() {
    let mut f = F::new();
    pc_world(&mut f, Some(capacity_of(true, 200)));
    f.steps(&["implement", "accept"]);
    let (a, _) = f.ok();
    assert_eq!(a["state"], "merged", "{a}");
    let records = look_records(&f);
    assert_eq!(
        records[0]["host_reason"], "pc_unreachable",
        "{}",
        records[0]
    );
    assert_eq!(f.capacity_calls, 1);
    let wanted = f.home().join("2.description.md");
    assert_eq!(
        records[0]["limits"]["read_only"],
        json!([wanted.to_str().unwrap()])
    );
}

// spec: specs/ap/unit-run.md#R45
#[test]
fn r45_either_condition_alone_gives_no_host() {
    for (free, load) in [(false, 400), (true, 199), (true, 0)] {
        let mut f = F::new();
        pc_world(&mut f, Some(capacity_of(free, load)));
        f.steps(&["implement", "accept"]);
        let (a, _) = f.ok();
        assert_eq!(a["state"], "merged", "{free} {load} {a}");
        assert!(look_records(&f)[0].get("host").is_none(), "{free} {load}");
    }
}

// spec: specs/ap/unit-run.md#R45
#[test]
fn r45_a_capacity_that_fails_or_is_not_an_object_gives_no_host_and_no_stop() {
    for answer in [
        None,
        Some(json!("free")),
        Some(json!({})),
        Some(json!({"can_start": ["desk"]})),
        Some(json!({"can_start": ["desk"], "hosts": [{"host": "pc"}]})),
    ] {
        let mut f = F::new();
        pc_world(&mut f, answer.clone());
        f.steps(&["implement", "accept"]);
        let (a, _) = f.ok();
        assert_eq!(a["state"], "merged", "{answer:?} {a}");
        assert!(look_records(&f)[0].get("host").is_none(), "{answer:?}");
        assert_eq!(f.capacity_calls, 1);
        // No write of the store comes of it.
        assert!(f.bodies("capacity").is_empty());
    }
    // No `hosts.pc.stages`: `capacity` is not read at all.
    let mut f = F::new();
    f.capacity = Some(capacity_of(true, 400));
    f.steps(&["implement", "accept"]);
    f.ok();
    assert_eq!(f.capacity_calls, 0);
}

// spec: specs/ap/unit-run.md#R45
#[test]
fn r45_a_second_look_after_one_on_the_pc_starts_a_fresh_session() {
    let two_calls = |f: &mut F, pc: bool| {
        f.set_unit(json!({"class": "contract"}));
        f.steps(&["implement", "acceptmain", "accept"]);
        f.ok();
        if pc {
            // The earlier look ran on the PC: its record says so (its session lives there).
            for e in std::fs::read_dir(f.t.join("reviews")).unwrap().flatten() {
                if e.file_name().to_string_lossy().ends_with(".run.json") {
                    let mut record: Value =
                        serde_json::from_slice(&std::fs::read(e.path()).unwrap()).unwrap();
                    assert_eq!(record["host"], "local", "{record}");
                    record["host"] = json!("pc");
                    std::fs::write(e.path(), record.to_string()).unwrap();
                }
            }
        }
        f.ok();
    };
    // Baseline: the earlier look ran here, so the second look resumes its session.
    let mut f = F::new();
    pc_world(&mut f, None);
    two_calls(&mut f, false);
    let (_, prompt) = f.launch(3);
    assert!(prompt.contains("## The round"), "{prompt}");
    assert!(!look_records(&f)[1]["resumes"].is_null());
    // On the PC the second look starts a fresh session as a first round does, placed by the rule.
    let mut f = F::new();
    pc_world(&mut f, Some(capacity_of(true, 300)));
    two_calls(&mut f, true);
    let (_, prompt) = f.launch(3);
    assert!(!prompt.contains("## The round"), "{prompt}");
    let records = look_records(&f);
    assert!(records[1]["resumes"].is_null(), "{}", records[1]);
    assert_eq!(records[1]["host_reason"], "pc_unreachable");
    assert_eq!(f.capacity_calls, 2);
}
