//! `ap unit run <key>`: one unit of the store driven from `ready`, `stopped` or a cut run to
//! `accepted` or `stopped`, its repairs inside (`specs/ap/unit-run.md`). The stages are `ap run`'s
//! code, the gate is `ap gate`'s, the acceptance checks run as `ap unit ready` runs them, and the
//! review of a head is recorded as `ap unit review` records one.
use super::object::{self, branch_ok, clone_of};
use super::page_review::{self, key_ok, stage_unit, unit_ended};
use super::render::{self, Role};
use crate::host::{Failure, Host, System, args, digest, error, fail, id, new_session_id};
use crate::review::{self, DIFF_OPTIONS};
use crate::store::{self, Actor, Applied, at};
use serde_json::{Map, Value, json};
use std::{
    ffi::OsString,
    fs::File,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, UNIX_EPOCH},
};

const FIXER: &str = include_str!("frames/fixer.md");
const SECOND_LOOK: &str = include_str!("frames/second-look.md");
const CONTINUE: &str = include_str!("frames/continue.md");
const UNITS: &str = "/var/lib/agent-platform/units";
const REVIEWS: &str = "/var/lib/agent-platform/reviews";
/// The bound of each stage by role, in seconds (R3).
const TIMEOUTS: [(&str, u64); 3] = [
    ("implementer", 10_800),
    ("verifier", 7_200),
    ("fixer", 7_200),
];
/// The most bytes of a stop's detail (R22).
const DETAIL: usize = 1000;

/// A refusal (R25): exit 2 before the run applied anything, else 3.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub status: u8,
}
/// What ends the loop early: a stop the store records, or a refusal or failure that ends the call (R22).
enum Halt {
    Stop(String, String),
    Fail(Failure),
}
impl From<Failure> for Halt {
    fn from(f: Failure) -> Self {
        Self::Fail(f)
    }
}
type Step<T = ()> = Result<T, Halt>;
fn stop<T>(cause: &str, detail: impl ToString) -> Step<T> {
    Err(Halt::Stop(cause.to_owned(), detail.to_string()))
}
fn failed<T>(code: &'static str, field: &str, detail: Value) -> Step<T> {
    Err(Halt::Fail(fail(code, field, detail)))
}
/// A stage `ap run` refused (R11).
fn refused(f: Failure) -> Halt {
    let detail = json!({"code": f.code, "field": f.field});
    Halt::Stop("stage_refused".into(), detail.to_string())
}
/// The first `max` bytes of `t`, cut back to a character boundary.
fn cut(t: &str, max: usize) -> String {
    let mut end = t.len().min(max);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_owned()
}
fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v[k].as_str().filter(|t| !t.is_empty())
}
/// The session a record names: 1 to 200 bytes.
fn session(rec: &Value) -> Option<&str> {
    text(rec, "session").filter(|t| t.len() <= 200)
}
fn hex40(h: &str) -> bool {
    h.len() == 40
        && h.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
/// A directory, and not a link.
fn real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}
fn json_file(path: &str) -> Option<Value> {
    let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    v.is_object().then_some(v)
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// Words without spaces (a validated repository, branch and number), separated by one space.
fn w(words: &str) -> Vec<&str> {
    words.split(' ').collect()
}

/// The arguments after `run`: `<key> [--store <absolute path>]` (R1).
pub fn arguments(argv: &[OsString]) -> Result<(String, Option<String>), Failure> {
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    match words.as_deref() {
        Some([key]) if !key.starts_with('-') => Ok(((*key).to_owned(), None)),
        Some([key, "--store", p]) if !key.starts_with('-') && p.starts_with('/') => {
            Ok(((*key).to_owned(), Some((*p).to_owned())))
        }
        _ => Err(error("usage", "args")),
    }
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = arguments(argv)
        .map_err(|f| refusal(f, 2, false))
        .and_then(|(key, store)| execute(&mut Host, store.as_deref(), &key));
    let key = argv.first().and_then(|k| k.to_str());
    let (written, status) = match result {
        Ok((v, status)) => (writeln!(std::io::stdout(), "{v}"), status),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail, "unit": key}});
            (writeln!(std::io::stderr(), "{v}"), e.status)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// Before the run a refusal of the store keeps the store's code (R6); after its first apply it is
/// `store_refused` with the store's refusal as `detail` (R22).
fn refusal(f: Failure, status: u8, running: bool) -> Box<Refusal> {
    let code = if running {
        f.code.to_owned()
    } else {
        store::code(&f)
    };
    let (field, detail) = (f.field, f.detail);
    Box::new(Refusal {
        code,
        field,
        detail,
        status,
    })
}
/// One call: the answer and its exit, or the refusal.
pub fn execute(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
) -> Result<(Value, u8), Box<Refusal>> {
    let (mut run, _lock) = prepare(s, store, key).map_err(|f| refusal(f, 2, false))?;
    let begun = match run.u["state"].as_str().unwrap_or_default() {
        "accepted" => return run.answer(s, None).map_err(|f| refusal(f, 3, true)),
        "ready" | "stopped" => run.begin(s),
        "running" => run.held(s),
        other => Err(fail("unit_not_runnable", "unit", json!(other))),
    };
    begun.map_err(|f| refusal(f, 2, false))?;
    let answered = match run.leftovers(s).and_then(|()| run.drive(s)) {
        Ok(()) => run.answer(s, None),
        Err(Halt::Stop(cause, detail)) => {
            let detail = cut(&detail, DETAIL);
            let p = json!({"unit": id("unit", key), "cause": cause, "detail": detail});
            let stopped = run.apply(s, "stop_unit", p);
            stopped.and_then(|_| run.answer(s, Some((&cause, &detail))))
        }
        Err(Halt::Fail(f)) => Err(f),
    };
    let status = if run.applied { 3 } else { 2 };
    answered.map_err(|f| refusal(f, status, true))
}

/// An author record under `<directory>/<key>/`: `<n>.implementer.md.run.json` or `<n>.fixer.md.run.json`.
struct Author {
    role: String,
    path: String,
    rec: Value,
}
/// The worktree as read: clean, its HEAD, on the unit's branch, its commits beyond the base.
struct Tree {
    clean: bool,
    head: String,
    on_branch: bool,
    ahead: String,
    changed: usize,
}
impl Tree {
    fn shown(&self) -> String {
        let (changed, commits, on_branch) = (self.changed, &self.ahead, self.on_branch);
        json!({"changed": changed, "commits": commits, "on_branch": on_branch, "head": self.head})
            .to_string()
    }
}
/// The reviews of kind `packet` of the unit, by `started_at`, and what counts of them (R18).
struct Verdicts {
    list: Vec<(String, Value)>,
    amended: String,
}
impl Verdicts {
    fn counted(&self) -> impl Iterator<Item = &(String, Value)> {
        let counts = |v: &Value| {
            let verdict = v["verdict"] == "accept" || v["verdict"] == "repair";
            v["status"] == "closed" && verdict && self.later(v)
        };
        self.list.iter().filter(move |(_, v)| counts(v))
    }
    fn later(&self, v: &Value) -> bool {
        v["started_at"]
            .as_str()
            .is_some_and(|t| t > self.amended.as_str())
    }
    fn rounds(&self) -> usize {
        self.counted().count()
    }
    fn repairs(&self) -> usize {
        self.counted()
            .filter(|(_, v)| v["verdict"] == "repair")
            .count()
    }
    fn newest(&self) -> Option<&(String, Value)> {
        self.counted().last()
    }
    fn of(&self, head: &str) -> Option<&(String, Value)> {
        self.counted().filter(|(_, v)| v["head"] == head).last()
    }
}

/// What this call did so far: its request ids, the steps it takes once, the head its own fixer moved
/// from (R12, R13, R19).
#[derive(Default)]
struct Done {
    count: u32,
    settled: bool,
    turned: bool,
    moved: bool,
    fixed: Option<String>,
    checks: u32,
}
/// One call's unit and its places.
struct Run {
    store: Option<String>,
    key: String,
    session: String,
    prefix: String,
    u: Value,
    repo: String,
    branch: String,
    clone: String,
    home: PathBuf,
    wt: String,
    reviews: PathBuf,
    task: String,
    implementer: Value,
    verifier: Value,
    timeouts: Map<String, Value>,
    base: String,
    applied: bool,
    done: Done,
}
/// The member `units` (R3): the directory and the timeouts by role.
fn units(policy: &Value) -> Result<(String, Map<String, Value>), Failure> {
    let mut timeouts: Map<String, Value> = TIMEOUTS
        .iter()
        .map(|(k, t)| ((*k).to_owned(), json!(t)))
        .collect();
    let Some(m) = policy.get("units") else {
        return Ok((UNITS.to_owned(), timeouts));
    };
    let m = m
        .as_object()
        .ok_or_else(|| invalid("units", "is not an object"))?;
    let mut directory = UNITS.to_owned();
    for (k, v) in m {
        match (k.as_str(), v) {
            ("directory", Value::String(d))
                if d.starts_with('/')
                    && (2..=400).contains(&d.len())
                    && d.bytes().all(|c| (b'!'..=b'~').contains(&c)) =>
            {
                directory = d.clone();
            }
            ("timeouts", Value::Object(t)) => {
                for (name, n) in t {
                    let seconds = n.as_u64().filter(|n| (1..=86_400).contains(n));
                    if !timeouts.contains_key(name) || seconds.is_none() {
                        let member = format!("units.timeouts.{name}");
                        return Err(invalid(&member, "is not a timeout of 1 to 86,400 seconds"));
                    }
                    timeouts.insert(name.clone(), n.clone());
                }
            }
            _ => {
                return Err(invalid(
                    &format!("units.{k}"),
                    "is not a member of this form",
                ));
            }
        }
    }
    Ok((directory, timeouts))
}
/// A tier as members of an `ap run` request, its accounts by `specs/ap/policy.md` R6 (R4).
fn tier(u: &Value, which: &str, role: &str, policy: &Value) -> Result<Value, Failure> {
    let t = &u[which];
    let (runtime, model) = (text(t, "runtime").unwrap_or_default(), text(t, "model"));
    if runtime != "claude" {
        return Err(fail("tier_not_supported", which, json!(runtime)));
    }
    let model = model.unwrap_or_default();
    let keys = [
        format!("{runtime}/{role}"),
        format!("{runtime}/{model}"),
        runtime.into(),
    ];
    let accounts = &policy["accounts"];
    let key = keys.iter().find(|k| accounts.get(k.as_str()).is_some());
    let key = key.ok_or_else(|| error("account_not_mapped", which))?;
    let list = accounts[key.as_str()]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let paths: Vec<&str> = list.iter().filter_map(Value::as_str).collect();
    if paths.is_empty() || paths.len() > 5 || paths.len() != list.len() {
        return Err(invalid(&format!("accounts.{key}"), "is not 1 to 5 paths"));
    }
    let mut req = json!({"runtime": runtime, "model": model, "effort": t["effort"],
        "account": paths[0], "permission_mode": "auto"});
    if paths.len() > 1 {
        req["next_accounts"] = json!(paths[1..]);
    }
    Ok(req)
}
/// The checks before any effect (R1 to R4), then the directories and the lock (R3, R5).
fn prepare(s: &mut impl System, store: Option<&str>, key: &str) -> Result<(Run, File), Failure> {
    if object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    let byte = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-';
    let first = key.starts_with(|c: char| c.is_ascii_lowercase());
    if !(1..=59).contains(&key.len()) || !first || !key.bytes().all(byte) {
        return Err(error("invalid_input", "unit"));
    }
    let u = at(store::get(s, store, "unit", key), "unit")?;
    let u = u.ok_or_else(|| error("unknown_unit", "unit"))?;
    let repo = text(&u, "repository").ok_or_else(|| error("no_repository", "unit"))?;
    let branch = text(&u, "branch").map_or_else(|| format!("unit/{key}"), str::to_owned);
    // `git check-ref-format` only for a name of R10's form, which never begins with `-`.
    let format = args(&["check-ref-format", "--branch", &branch]);
    let named = branch_ok(&branch)
        && (s.call("git", &format, &Value::Null, 10_000)).is_ok_and(|r| r.success);
    if !named {
        return Err(fail("invalid_branch", "branch", json!(branch)));
    }
    let clone = clone_of(s, repo)?;
    // `clone_of` read the file as one JSON object.
    let file = s
        .policy_file()
        .and_then(|p| s.read(&p.to_string_lossy()).ok());
    let policy: Value = file
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let reviews = page_review::policy(s)?;
    let task = reviews.get("task").and_then(Value::as_str);
    let task = task
        .ok_or_else(|| error("invalid_input", "task"))?
        .to_owned();
    let (directory, timeouts) = units(&policy)?;
    let implementer = tier(&u, "implementer", "implementer", &policy)?;
    let verifier = tier(&u, "verifier", "reviewer", &policy)?;
    // The directory is resolved once; `<key>/wt` is appended to it unresolved (R3, R8).
    let io = |what: &str| fail("io_error", "directory", json!(what));
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    dirs.create(&directory)
        .map_err(|_| io("the units directory cannot be made"))?;
    let root = std::fs::canonicalize(&directory).map_err(|_| io("the units directory"))?;
    let home = root.join(key);
    match dirs.recursive(false).create(&home) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(io("the unit's")),
        _ if !real_dir(&home) => return Err(fail("worktree_failed", "directory", json!(home))),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let lock = options
        .open(home.join("lock"))
        .map_err(|_| io("the lock"))?;
    let held = rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive);
    held.map_err(|_| error("unit_run_active", "unit"))?;
    let drawn = new_session_id(s)?.replace('-', "");
    let dir = reviews.get("directory").and_then(Value::as_str);
    let run = Run {
        store: store.map(str::to_owned),
        key: key.to_owned(),
        session: format!("u-{key}"),
        prefix: format!("ur-{}", &drawn[..16]),
        repo: repo.to_owned(),
        branch,
        clone,
        wt: home.join("wt").to_string_lossy().into_owned(),
        home,
        reviews: PathBuf::from(dir.unwrap_or(REVIEWS)),
        task,
        implementer,
        verifier,
        timeouts,
        u,
        base: String::new(),
        applied: false,
        done: Done::default(),
    };
    Ok((run, lock))
}

impl Run {
    fn store(&self) -> Option<&str> {
        self.store.as_deref()
    }
    /// One apply as the client `ap`, the agent `ap-unit`, the session `u-<key>` and no role (R1).
    fn apply(&mut self, s: &mut impl System, action: &str, p: Value) -> Result<Applied, Failure> {
        let rid = self.rid();
        let actor = Actor {
            agent: "ap-unit",
            session: &self.session,
        };
        let applied = store::unit_action(s, self.store.as_deref(), &actor, action, rid, p);
        self.applied |= applied.is_ok();
        at(applied, action)
    }
    /// A new request id `ur-<16 hex>-<counter>` (R1).
    fn rid(&mut self) -> String {
        self.done.count += 1;
        format!("{}-{}", self.prefix, self.done.count)
    }
    fn unit_id(&self) -> Value {
        id("unit", &self.key)
    }
    fn reread(&mut self, s: &mut impl System) -> Result<(), Failure> {
        let u = at(store::get(s, self.store(), "unit", &self.key), "unit")?;
        self.u = u.ok_or_else(|| error("store_answer_unreadable", "unit"))?;
        Ok(())
    }
    /// `start_unit` from `ready`, `resume_unit` from `stopped` (R6).
    fn begin(&mut self, s: &mut impl System) -> Result<(), Failure> {
        let action = if self.u["state"] == "ready" {
            "start_unit"
        } else {
            "resume_unit"
        };
        let mut p = json!({"unit": self.unit_id()});
        if self.u["lane"] == "objective" {
            p["intent"] = self.u["intent"].clone();
        }
        self.apply(s, action, p).map(|_| ())
    }
    /// The take-over of a cut run applies nothing: an active hold on the unit or its intent refuses it (R6).
    fn held(&mut self, s: &mut impl System) -> Result<(), Failure> {
        let rows = at(
            store::view_rows(s, self.store(), "holds_on_work", json!({})),
            "holds",
        )?;
        let intent = self.u["intent"]["native_key"].as_str();
        let cell = |r: &Map<String, Value>, k: &str| {
            let c = r.get(k).and_then(Value::as_str).unwrap_or_default();
            c.trim_end_matches(" (stale)").to_owned()
        };
        let named = |r: &&Map<String, Value>| {
            cell(r, "Unit") == self.key || intent.is_some_and(|i| cell(r, "Intent") == i)
        };
        match rows.iter().find(named) {
            Some(r) => Err(fail("held", "unit", json!(r))),
            None => Ok(()),
        }
    }

    /// The author records by `<n>` (R3).
    fn authors(&self) -> Vec<Author> {
        let mut list = vec![];
        for e in std::fs::read_dir(&self.home)
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((n, rest)) = name.split_once('.') else {
                continue;
            };
            let role = rest
                .strip_suffix(".md.run.json")
                .filter(|r| ["implementer", "fixer"].contains(r));
            let path = self.home.join(&name).to_string_lossy().into_owned();
            if let (Ok(n), Some(role), Some(rec)) = (n.parse::<u64>(), role, json_file(&path)) {
                list.push((
                    n,
                    Author {
                        role: role.to_owned(),
                        path,
                        rec,
                    },
                ));
            }
        }
        list.sort_by_key(|(n, _)| *n);
        list.into_iter().map(|(_, a)| a).collect()
    }
    /// The next stage's `<n>`: one more than any `<n>.` of the unit's directory.
    fn next(&self) -> u64 {
        let entries = std::fs::read_dir(&self.home)
            .into_iter()
            .flatten()
            .flatten();
        let numbers = entries.filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.split_once('.')
                .and_then(|(n, _)| n.parse::<u64>().ok())
        });
        numbers.max().unwrap_or(0) + 1
    }
    /// A new prompt file of mode 0600 under `<directory>/<key>/`.
    fn prompt(&self, name: &str, text: &str) -> Step<String> {
        let path = self.home.join(name);
        let mut options = std::fs::OpenOptions::new();
        let file = options.write(true).create_new(true).mode(0o600).open(&path);
        let written = file.and_then(|mut f| f.write_all(text.as_bytes()));
        written.map_err(|_| Halt::Stop("io_error".into(), name.into()))?;
        Ok(path.to_string_lossy().into_owned())
    }
    /// A stage's end state and the first bytes of its answer.
    fn answered(out: &str, rec: &Value) -> String {
        let answer = std::fs::read(out).unwrap_or_default();
        let state = rec["state"].as_str().unwrap_or("failed");
        cut(
            &format!("{state}: {}", String::from_utf8_lossy(&answer)),
            DETAIL,
        )
    }
    /// The `ap run` request of a stage of `tier` in the worktree (R4, R11, R19).
    fn stage(&self, tier: &Value, role: &str, timeout: &str, (prompt, out): (&str, &str)) -> Value {
        let mut req = tier.clone();
        (req["role"], req["cwd"], req["prompt"]) = (json!(role), json!(self.wt), json!(prompt));
        (req["out"], req["unit"]) = (json!(out), json!(self.key));
        req["timeout_seconds"] = self.timeouts[timeout].clone();
        if let Some(store) = &self.store {
            req["store"] = json!(store);
        }
        req
    }

    /// The reviews of kind `packet` of the unit (R18).
    fn verdicts(&mut self, s: &mut impl System) -> Step<Verdicts> {
        let args = json!({"unit": self.key});
        let keys = at(
            store::view_keys(s, self.store(), "reviews_of_unit", args),
            "reviews",
        )?;
        let mut list = vec![];
        for k in keys {
            let v = at(store::get(s, self.store(), "review", &k), "reviews")?;
            if let Some(v) = v.filter(|v| v["kind"] == "packet") {
                list.push((k, v));
            }
        }
        let started = |v: &Value| v["started_at"].as_str().map(str::to_owned);
        list.sort_by_key(|(k, v)| (started(v), v["round"].as_u64(), k.clone()));
        let amended = self.u["amended_at"].as_str().unwrap_or_default().to_owned();
        Ok(Verdicts { list, amended })
    }
    /// A review's report path, by its key (checked) and never by a member of the store (R3).
    fn report(&self, key: &str) -> Option<String> {
        let path = self.reviews.join(format!("{key}.md"));
        key_ok(key).then(|| path.to_string_lossy().into_owned())
    }
    /// `close_unit_review`; a session the store refuses as a writer closes `abandoned` with the
    /// session `u-<key>:recovery` (R7). True when the review closed as asked.
    fn close(&mut self, s: &mut impl System, review: &str, mut p: Value) -> Step<bool> {
        (p["review"], p["unit"]) = (id("review", review), self.unit_id());
        let recovery = format!("{}:recovery", self.session);
        if p["session"].is_null() {
            p["session"] = json!(recovery);
        }
        match self.apply(s, "close_unit_review", p.clone()) {
            Ok(_) => Ok(true),
            Err(f) if store::code(&f) == "writer_cannot_review" && p["session"] != *recovery => {
                let p = json!({"verdict": "abandoned", "session": recovery});
                self.close(s, review, p).map(|_| false)
            }
            Err(f) => Err(f.into()),
        }
    }

    /// The leftovers of a cut run, before the loop (R7).
    fn leftovers(&mut self, s: &mut impl System) -> Step {
        for a in self.authors() {
            self.end_stage(s, &a.path, &a.rec, &a.role)?;
        }
        let v = self.verdicts(s)?;
        for (k, _) in v.list.iter().filter(|(_, r)| r["status"] == "running") {
            let record = self.report(k).map(|out| format!("{out}.run.json"));
            let rec = record.as_deref().and_then(json_file);
            if let (Some(path), Some(rec)) = (&record, &rec) {
                self.end_stage(s, path, rec, "verifier")?;
            }
            let session = rec.as_ref().and_then(session);
            self.close(s, k, json!({"verdict": "abandoned", "session": session}))?;
        }
        Ok(())
    }
    /// A stage of a cut run: its unit stopped and read as ended, its run closed (R7).
    fn end_stage(&mut self, s: &mut impl System, path: &str, rec: &Value, role: &str) -> Step {
        let running = rec["state"] == "running";
        if let Some(name) = rec["unit"].as_str().filter(|_| running)
            && !(stage_unit(name) && unit_ended(s, name, true))
        {
            return failed("run_still_active", "record", json!(name));
        }
        let closed = ["applied", "already_applied"].map(|c| rec["run_object"]["close"] == c);
        let run = rec["run_object"]["key"]
            .as_str()
            .filter(|_| closed == [false, false]);
        let Some(run) = run else { return Ok(()) };
        let found = store::get(s, self.store(), "run", run).ok().flatten();
        if !found.is_some_and(|r| r["record"] == path && r["status"] == "running") {
            return Ok(());
        }
        let state = if running {
            "killed"
        } else {
            rec["state"].as_str().unwrap_or("failed")
        };
        let mut p = json!({"run": id("run", run), "state": state});
        if let Some(t) = session(rec) {
            p["session"] = json!(t);
        }
        let out = path.strip_suffix(".run.json").unwrap_or(path);
        let rid = format!("run-{}-close", &digest(out.as_bytes())[..32]);
        // The stage's own request id; the store's answer is the run's, and a later call tries again.
        let _ = store::close_run(s, self.store(), (&rid, role, &p));
        Ok(())
    }

    fn git(&self, s: &mut impl System, dir: &str, words: &[&str]) -> Step<String> {
        let out = review::git(s, dir, words);
        let out = out.map_err(|_| Halt::Stop("git_failed".into(), words.join(" ")))?;
        Ok(out.trim_end().to_owned())
    }
    fn git_ok(&self, s: &mut impl System, dir: &str, words: &[&str]) -> bool {
        review::git(s, dir, words).is_ok()
    }
    /// `git fetch origin <what>` in the clone within 120 seconds (R8, R12).
    fn fetch(&self, s: &mut impl System, what: &str) -> Step {
        let words = [
            "--no-replace-objects",
            "-C",
            &self.clone,
            "fetch",
            "--quiet",
            "origin",
            what,
        ];
        let fetched = s.call("git", &args(&words), &Value::Null, 120_000);
        match fetched.is_ok_and(|r| r.success) {
            true => Ok(()),
            false => stop("fetch_failed", what),
        }
    }
    fn gh(&self, s: &mut impl System, words: &str) -> Step<Value> {
        let reply = s.call("gh", &args(&w(words)), &Value::Null, 60_000);
        let v = reply.ok().filter(|r| r.success);
        let v = v.and_then(|r| serde_json::from_slice::<Value>(&r.stdout).ok());
        v.map_or_else(|| stop("github_unreadable", &words[..7]), Ok)
    }
    /// The open pull request of the branch against `main` from the repository itself (R9).
    fn list(&self, s: &mut impl System) -> Step<Option<(u64, String)>> {
        let fields = "number,headRefOid,headRefName,baseRefName,isCrossRepository,headRepositoryOwner,headRepository";
        let (repo, branch) = (&self.repo, &self.branch);
        let words = format!(
            "pr list --repo {repo} --head {branch} --base main --state open --json {fields}"
        );
        let v = self.gh(s, &words)?;
        let (owner, name) = repo.split_once('/').unwrap_or_default();
        let same = |a: &Value, b: &str| a.as_str().is_some_and(|a| a.eq_ignore_ascii_case(b));
        let own = |p: &&Value| {
            p["headRefName"] == branch.as_str()
                && p["baseRefName"] == "main"
                && p["isCrossRepository"] == false
                && same(&p["headRepositoryOwner"]["login"], owner)
                && same(&p["headRepository"]["name"], name)
        };
        let list: Vec<&Value> = v.as_array().into_iter().flatten().filter(own).collect();
        let [p] = list.as_slice() else {
            return match list.len() {
                0 => Ok(None),
                n => stop("pull_request_ambiguous", json!({"count": n})),
            };
        };
        let n = p["number"].as_u64().filter(|n| *n > 0);
        let head = p["headRefOid"]
            .as_str()
            .filter(|h| hex40(h))
            .map(str::to_owned);
        n.zip(head)
            .map_or_else(|| stop("github_unreadable", "pr list"), |p| Ok(Some(p)))
    }
    /// The state and the head of pull request `n`.
    fn view(&self, s: &mut impl System, n: u64) -> Step<(String, String)> {
        let v = self.gh(
            s,
            &format!("pr view {n} --repo {} --json state,headRefOid", self.repo),
        )?;
        let head = v["headRefOid"].as_str().filter(|h| hex40(h));
        match (v["state"].as_str(), head) {
            (Some(state), Some(head)) => Ok((state.to_owned(), head.to_owned())),
            _ => stop("github_unreadable", "pr view"),
        }
    }
    /// The read of the hosting service at the start of a pass (R9).
    fn read(&mut self, s: &mut impl System) -> Step<Option<(u64, String)>> {
        if let Some(n) = self.u["pull_request"].as_u64() {
            let (state, head) = self.view(s, n)?;
            if state != "OPEN" {
                return stop(
                    "pull_request_not_open",
                    json!({"pull_request": n, "state": state}),
                );
            }
            return Ok(Some((n, head)));
        }
        let found = self.list(s)?;
        if let (Some((n, _)), true) = (&found, self.authors().is_empty()) {
            return stop("branch_in_use", json!({"pull_request": n}));
        }
        Ok(found)
    }
    /// After an author stage, its local head seen as the pull request's head within 120 seconds
    /// (R20); without a pull request, until one shows or the bound passes.
    fn wait(&mut self, s: &mut impl System) -> Step {
        let local = self.git(s, &self.wt, &["rev-parse", "HEAD"])?;
        let deadline = s.now() + Duration::from_secs(120);
        loop {
            let seen = match self.u["pull_request"].as_u64() {
                Some(n) => self.view(s, n).ok().map(|(_, h)| h),
                None => self.list(s).ok().flatten().map(|(_, h)| h),
            };
            match seen {
                Some(h) if h == local => return Ok(()),
                None if s.now() >= deadline && self.u["pull_request"].is_null() => return Ok(()),
                _ if s.now() >= deadline => return stop("head_not_seen", json!({"head": local})),
                _ => s.pause(Duration::from_secs(5)),
            }
        }
    }

    /// The clone's worktree entry whose path is exactly `wt` (R8).
    fn entry(&self, s: &mut impl System) -> Step<Option<String>> {
        let list = self.git(s, &self.clone, &["worktree", "list", "--porcelain"])?;
        let first = format!("worktree {}", self.wt);
        let found = list
            .split("\n\n")
            .find(|e| e.lines().next() == Some(first.as_str()));
        Ok(found.map(str::to_owned))
    }
    /// No worktree: a new branch from the base, or the unit's own branch as it is (R10).
    fn make_worktree(&mut self, s: &mut impl System) -> Step {
        let (clone, wt, branch) = (&self.clone, &self.wt, &self.branch);
        let reference = format!("refs/heads/{branch}");
        let exists = self.git_ok(s, clone, &["rev-parse", "--verify", "--quiet", &reference]);
        let added = if exists {
            let entry = self.entry(s)?;
            let own = entry.is_some_and(|e| e.lines().any(|l| l == format!("branch {reference}")));
            if !own && self.u["pull_request"].is_null() {
                return stop("branch_in_use", json!({"branch": branch}));
            }
            self.git(s, clone, &["worktree", "add", "--quiet", wt, branch])
        } else {
            self.git(
                s,
                clone,
                &["worktree", "add", "--quiet", "-b", branch, wt, &self.base],
            )
        };
        added
            .map(|_| ())
            .or_else(|_| stop("worktree_failed", "git worktree add"))
    }
    /// The worktree as read, after its guards (R8).
    fn tree(&self, s: &mut impl System) -> Step<Tree> {
        if !real_dir(&self.home) || !real_dir(Path::new(&self.wt)) {
            return stop("worktree_failed", "not a directory");
        }
        let Some(entry) = self.entry(s)? else {
            return stop("worktree_failed", "no worktree of the clone at this path");
        };
        let on_branch = entry
            .lines()
            .any(|l| l == format!("branch refs/heads/{}", self.branch));
        let wt = self.wt.as_str();
        let status = self.git(
            s,
            wt,
            &["status", "--porcelain", "--untracked-files=normal"],
        )?;
        let head = self.git(s, wt, &["rev-parse", "HEAD"])?;
        let ahead = self.git(
            s,
            wt,
            &["rev-list", "--count", &format!("{}..HEAD", self.base)],
        )?;
        let changed = status.lines().count();
        Ok(Tree {
            clean: changed == 0,
            head,
            on_branch,
            ahead,
            changed,
        })
    }

    fn page(&mut self, s: &mut impl System, role: Role) -> Step<String> {
        let u = self.u.clone();
        Ok(render::page_text(s, self.store(), &self.key, &u, Some(role))?.0)
    }
    /// One author stage of `role` with `git_author`, resuming `from` when given: its record and its
    /// answer's path (R11 to R13, R21).
    fn author(
        &mut self,
        s: &mut impl System,
        role: &str,
        from: Option<&Author>,
        text: &str,
    ) -> Step<(Value, String)> {
        let n = self.next();
        let prompt = self.prompt(&format!("{n}.{role}.prompt.md"), text)?;
        let out = self
            .home
            .join(format!("{n}.{role}.md"))
            .to_string_lossy()
            .into_owned();
        let mut req = self.stage(&self.implementer, role, role, (&prompt, &out));
        req["git_author"] = json!(true);
        if let Some(a) = from {
            for k in ["model", "effort", "runtime"]
                .iter()
                .filter(|k| a.rec[**k].is_string())
            {
                req[*k] = a.rec[*k].clone();
            }
            req["resume"] = json!({"record": a.path});
        }
        let (rec, _) = crate::run::execute(s, &req).map_err(refused)?;
        Ok((rec, out))
    }
    /// No pull request: the implementer, fresh or resumed (R11).
    fn implement(&mut self, s: &mut impl System, t: &Tree) -> Step {
        let authors = self.authors();
        let named = |a: &&Author| a.role == "implementer" && session(&a.rec).is_some();
        let from = authors.iter().rev().find(named);
        let mut text = self.page(s, Role::Implementer)?;
        match from {
            Some(_) => text.push_str(&format!("\n{CONTINUE}")),
            None if !t.clean || !t.on_branch || t.ahead != "0" => {
                return stop("worktree_dirty", t.shown());
            }
            None => {}
        }
        let (rec, out) = self.author(s, "implementer", from, &text)?;
        self.wait(s)?;
        match self.list(s)? {
            None => stop("no_pull_request", Self::answered(&out, &rec)),
            Some(_) => Ok(()),
        }
    }
    /// A worktree a cut author stage left: its author settles it, once in a call (R12).
    fn settle(&mut self, s: &mut impl System, t: &Tree) -> Step {
        if self.done.settled {
            return stop("worktree_dirty", t.shown());
        }
        self.done.settled = true;
        let authors = self.authors();
        let v = self.verdicts(s)?;
        let newest = authors.last();
        if let (Some((k, r)), Some(a)) = (v.newest(), newest)
            && r["verdict"] == "repair"
            && a.role == "fixer"
            && a.rec["state"] != "ok"
        {
            let judged = r["head"].as_str().unwrap_or_default();
            return self.repair(s, k, judged, &v);
        }
        match newest.filter(|a| session(&a.rec).is_some()) {
            Some(a) => self.turn(s, a),
            None => stop("worktree_dirty", t.shown()),
        }
    }
    /// The settling stage: the session of `a` resumed under its role with `continue.md` (R12, R13).
    fn turn(&mut self, s: &mut impl System, a: &Author) -> Step {
        let text = format!("{}\n{CONTINUE}", self.page(s, Role::Implementer)?);
        self.author(s, &a.role, Some(a), &text)?;
        self.wait(s)
    }
    /// The newest author record, when the author's one turn on an amended page is due (R13).
    fn amended_turn(&self) -> Option<Author> {
        let amended = self.u["amended_at"].as_str().unwrap_or_default();
        let authors = self.authors();
        let started = |a: &Author| a.rec["start"].as_str().unwrap_or_default().to_owned();
        let done = authors
            .iter()
            .any(|a| started(a).as_str() > amended && a.rec["state"] == "ok");
        let newest = authors.into_iter().last()?;
        let due = amended > started(&newest).as_str() && session(&newest.rec).is_some();
        (due && !done && !self.done.turned).then_some(newest)
    }
    /// `record_head` with the union of the unit's writers and every author session (R14), and `extra` (R20).
    fn record_head(
        &mut self,
        s: &mut impl System,
        n: u64,
        head: &str,
        extra: Option<&str>,
    ) -> Step {
        let held = self.u["writers"].as_array().into_iter().flatten();
        let mut writers: Vec<String> = held.filter_map(|w| w.as_str().map(str::to_owned)).collect();
        let mut sessions = vec![];
        for a in self.authors() {
            let attempts = a.rec["attempts"].as_array().cloned().unwrap_or_default();
            let all = attempts.iter().chain([&a.rec]).filter_map(session);
            sessions.extend(all.map(str::to_owned));
        }
        for w in sessions.into_iter().chain(extra.map(str::to_owned)) {
            if !writers.contains(&w) {
                writers.push(w);
            }
        }
        let rounds = self.verdicts(s)?.rounds();
        let p = json!({"unit": self.unit_id(), "head": head, "pull_request": n, "rounds": rounds,
            "writers": writers});
        self.apply(s, "record_head", p)?;
        Ok(self.reread(s)?)
    }

    fn clean_at(&self, s: &mut impl System, head: &str) -> Step {
        let t = self.tree(s)?;
        match t.clean && t.head == head && t.on_branch {
            true => Ok(()),
            false => stop("worktree_dirty", t.shown()),
        }
    }
    /// The deterministic checks on the head (R16).
    fn checks(&mut self, s: &mut impl System, head: &str) -> Step {
        self.clean_at(s, head)?;
        let (wt, base) = (self.wt.as_str(), self.base.as_str());
        let mut words = vec!["diff"];
        words.extend(DIFF_OPTIONS.into_iter().chain([base, head]));
        let diff = self.git(s, wt, &words)?;
        let paths: Vec<&str> = diff.split_terminator('\0').collect();
        let set = self.u["write_set"].as_array().into_iter().flatten();
        let set: Vec<String> = set.filter_map(|p| p.as_str().map(str::to_owned)).collect();
        let outside = review::outside(&set, &paths);
        if !outside.is_empty() {
            let shown: Vec<&&str> = outside.iter().take(10).collect();
            return stop(
                "path_outside_write_set",
                json!({"paths": shown, "count": outside.len()}),
            );
        }
        let merge_base = self.git(s, wt, &["merge-base", base, head])?;
        self.done.checks += 1;
        let list = self.u["acceptance"].as_array().cloned().unwrap_or_default();
        for (i, c) in list.iter().enumerate().map(|(i, c)| (i + 1, c)) {
            let name = format!("{}-{}-{i}.check.out", self.prefix, self.done.checks);
            let output = self.home.join(name).to_string_lossy().into_owned();
            let check = c["check"].as_str().unwrap_or_default();
            let status = object::run_check(s, &self.wt, (&merge_base, head), &output, check);
            let status = status.map_err(|f| Halt::Stop(f.code.into(), f.detail.to_string()))?;
            let cause = match status {
                Some(0) => continue,
                None => "acceptance_not_runnable",
                Some(124 | 137) => "acceptance_timeout",
                Some(_) => "acceptance_failed",
            };
            let bytes = std::fs::read(&output).unwrap_or_default();
            let mut from = bytes.len().saturating_sub(600);
            while from < bytes.len() && (bytes[from] & 0xC0) == 0x80 {
                from += 1;
            }
            let tail = String::from_utf8_lossy(&bytes[from..]);
            return stop(cause, json!({"index": i, "status": status, "output": tail}));
        }
        self.clean_at(s, head)
    }
    /// The required check on the hosting service, without the automatic reviewer (R17).
    fn gate(&mut self, s: &mut impl System, number: u64, head: &str) -> Step {
        let req = json!({"repository": self.repo, "pull_request": number, "head": head});
        let gated = crate::gate::execute(s, &req);
        let (a, _) = gated.map_err(|f| Halt::Stop("github_unreadable".into(), f.code.into()))?;
        let end = a["end"].as_str().unwrap_or("github_unreadable");
        if end != "passed" {
            return stop(
                end,
                json!({"head": a["head"], "failed_check": a["failed_check"]}),
            );
        }
        let threads = a["threads"].as_array();
        if !threads.is_some_and(Vec::is_empty) {
            let ids: Vec<&Value> = threads.into_iter().flatten().map(|t| &t["id"]).collect();
            return stop("unresolved_threads", json!({"threads": ids}));
        }
        Ok(())
    }

    /// The record of the newest verdict's session and the head it judged, when the round resumes it
    /// (R19): only after this call's own fixer moved that head, and no move of another since.
    fn second_look(&self, v: &Verdicts) -> Option<(String, String)> {
        let since = v.list.iter().rfind(|(_, r)| v.later(r));
        let abandoned = since.is_some_and(|(_, r)| r["verdict"] == "abandoned");
        let (k, r) = v.newest()?;
        let (judged, session) = (r["head"].as_str()?, r["session"].as_str()?);
        let writers = self.u["writers"].as_array();
        let writer = writers.is_some_and(|w| w.iter().any(|w| w == session));
        let own = self.done.fixed.as_deref() == Some(judged) && !self.done.moved;
        let record = format!("{}.run.json", self.report(k)?);
        let fit = own && !abandoned && !writer && json_file(&record).is_some();
        fit.then(|| (record, judged.to_owned()))
    }
    /// One validate naming the review's key, the admission of the stage, the start (R19).
    fn start_review(&mut self, s: &mut impl System, body: &Value, req: &mut Value) -> Step<String> {
        let checked = at(store::validate(s, self.store(), body), "start")?;
        if checked["result"]["valid"] != true {
            let criteria = checked["result"]["criteria"]
                .as_array()
                .into_iter()
                .flatten();
            let no: Vec<&Value> = criteria
                .filter(|c| c["passed"] == false)
                .map(|c| &c["code"])
                .collect();
            let detail = json!({"status": "refused", "code": "submission_criteria", "failed": no});
            return failed(store::REFUSED, "start", detail);
        }
        let edits = checked["result"]["edits"].as_array().into_iter().flatten();
        let mut made =
            edits.filter(|e| e["change"] == "initial" && e["id"]["object_type"] == "review");
        let key = made.next().and_then(|e| e["id"]["native_key"].as_str());
        let key = key.filter(|k| key_ok(k)).map(str::to_owned);
        let review = key.ok_or_else(|| error("store_answer_unreadable", "start"))?;
        let out = self.report(&review).unwrap_or_default();
        if Path::new(&out).symlink_metadata().is_ok() {
            return failed("request_id_reused", "start", json!(review));
        }
        req["out"] = json!(out);
        let mut dirs = std::fs::DirBuilder::new();
        let made = dirs.recursive(true).mode(0o700).create(&self.reviews);
        made.map_err(|_| Halt::Stop("io_error".into(), "the reviews directory".into()))?;
        crate::run::admit_stage(s, req).map_err(refused)?;
        // The store compares strictly: a review in the second of the amendment would not count.
        let read = at(store::head_now(s, self.store(), "unit", &self.key), "unit")?;
        let now = read.and_then(|(_, _, n)| n);
        let read_at = now.map(|n| crate::run::stamp(UNIX_EPOCH + Duration::from_secs(n)));
        if read_at.is_none_or(|t| t.as_str() <= self.u["amended_at"].as_str().unwrap_or_default()) {
            s.pause(Duration::from_secs(1));
        }
        let started = at(store::start_unit_review(s, self.store(), body), "start")?;
        self.applied = true;
        let key = started
            .made("review")
            .and_then(|i| i["native_key"].as_str());
        if started.outcome != "applied" || key != Some(review.as_str()) {
            return failed("request_id_reused", "start", json!(review));
        }
        Ok(review)
    }
    /// A verifier round on `head`: its verdict and its review (R19, R20).
    fn verify(
        &mut self,
        s: &mut impl System,
        number: u64,
        head: &str,
        v: &Verdicts,
    ) -> Step<(String, String)> {
        let resumed = self.second_look(v);
        let mut text = self.page(s, Role::Verifier)?;
        if let Some((_, judged)) = &resumed {
            let round =
                format!("Head judged before: {judged}\nWhat moved it: the author's repair\n");
            text.push_str(&format!("\n{SECOND_LOOK}\n## The round\n\n{round}"));
        }
        let n = self.next();
        let prompt = self.prompt(&format!("{n}.verifier.prompt.md"), &text)?;
        let t = &self.verifier;
        let p = json!({"task": id("task", &self.task), "kind": "packet", "head": head,
            "base": self.base, "model": t["model"], "effort": t["effort"], "prompt": prompt,
            "prompt_sha256": digest(text.as_bytes()), "unit": self.unit_id(), "round": v.rounds() + 1,
            "repository": self.repo, "pull_request": number});
        let rid = self.rid();
        let body = store::body(
            &Actor {
                agent: "ap-unit",
                session: &self.session,
            },
            "start_unit_review",
            rid,
            p,
        );
        let mut req = self.stage(&self.verifier, "reviewer", "verifier", (&prompt, ""));
        (req["ont_role"], req["expect"]) = (json!("verifier"), json!({"first_line": "Verdict: *"}));
        if let Some((record, _)) = &resumed {
            req["resume"] = json!({"record": record});
        }
        let review = self.start_review(s, &body, &mut req)?;
        let out = req["out"].as_str().unwrap_or_default().to_owned();
        let rec = match crate::run::execute(s, &req) {
            Ok((rec, _)) => rec,
            Err(f) => {
                self.close(s, &review, json!({"verdict": "abandoned"}))?;
                return Err(refused(f));
            }
        };
        let session = session(&rec).map(str::to_owned);
        let abandon = json!({"verdict": "abandoned", "session": session});
        // A verifier that wrote or pushed becomes a writer before the unit stops (R20).
        let t = self.tree(s)?;
        let (_, now) = self.view(s, number)?;
        if !t.clean || t.head != head || now != head || !t.on_branch {
            self.close(s, &review, abandon)?;
            self.record_head(s, number, &now, session.as_deref())?;
            let detail = json!({"review": review, "head": now, "worktree": t.shown()});
            return stop("verifier_wrote", detail);
        }
        let bytes = std::fs::read(&out).unwrap_or_default();
        let sha = digest(&bytes);
        let named = rec["answer"]["path"] == *out && rec["answer"]["sha256"] == *sha;
        let report = String::from_utf8_lossy(&bytes);
        let mut lines = report.split('\n').map(|l| l.trim_end_matches('\r'));
        let verdict = match lines.next() {
            Some("Verdict: accept") => Some("accept"),
            Some("Verdict: repair") => Some("repair"),
            _ => None,
        };
        let head_line = super::names_head(lines.next(), head);
        let Some(verdict) = verdict.filter(|_| rec["state"] == "ok" && named && head_line) else {
            self.close(s, &review, abandon)?;
            return stop(
                "verifier_failed",
                json!({"review": review, "state": rec["state"]}),
            );
        };
        let account = rec["account"]
            .as_str()
            .and_then(|a| Path::new(a).file_name()?.to_str());
        let mut p =
            json!({"verdict": verdict, "session": session, "report": out, "report_sha256": sha});
        if let Some(a) = account.filter(|a| a.len() <= 40) {
            p["account"] = json!(a);
        }
        if !self.close(s, &review, p)? {
            let detail = json!({"review": review, "cause": "writer_cannot_review"});
            return stop("verifier_failed", detail);
        }
        Ok((verdict.to_owned(), review))
    }
    /// The repair step (R21).
    fn repair(&mut self, s: &mut impl System, review: &str, judged: &str, v: &Verdicts) -> Step {
        if v.repairs() >= 3 {
            return stop("repair_limit", json!({"repairs": v.repairs()}));
        }
        let report = self.report(review).unwrap_or_default();
        let authors = self.authors();
        let Some(from) = authors.last().filter(|a| session(&a.rec).is_some()) else {
            return stop("stage_refused", json!({"code": "no_author_session"}));
        };
        let page = self.page(s, Role::Implementer)?;
        let round = format!("Head judged: {judged}\nVerifier report: {report}\n");
        let text = format!("{page}\n{FIXER}\n## The round\n\n{round}");
        let (rec, out) = self.author(s, "fixer", Some(from), &text)?;
        if self.git(s, &self.wt, &["rev-parse", "HEAD"])? == judged {
            return stop("repair_refused", Self::answered(&out, &rec));
        }
        self.done.fixed = Some(judged.to_owned());
        self.wait(s)
    }

    /// The loop: each pass reads, then takes the first step that applies (R8 to R21).
    fn drive(&mut self, s: &mut impl System) -> Step {
        loop {
            self.fetch(s, "main")?;
            let main = ["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"];
            self.base = self.git(s, &self.clone, &main)?;
            if !hex40(&self.base) {
                return stop("fetch_failed", "refs/remotes/origin/main");
            }
            let pr = self.read(s)?;
            if Path::new(&self.wt).symlink_metadata().is_err() {
                self.make_worktree(s)?;
                continue;
            }
            let t = self.tree(s)?;
            let Some((number, head)) = pr else {
                self.implement(s, &t)?;
                continue;
            };
            if t.head != head || !t.clean || !t.on_branch {
                let forward = t.clean && t.on_branch && {
                    self.fetch(s, &format!("refs/heads/{}", self.branch))?;
                    self.git_ok(
                        s,
                        &self.wt,
                        &["merge-base", "--is-ancestor", &t.head, &head],
                    )
                };
                if forward {
                    self.git(s, &self.wt, &["merge", "--ff-only", "--quiet", &head])?;
                    self.done.moved = true;
                } else {
                    self.settle(s, &t)?;
                }
                continue;
            }
            if let Some(a) = self.amended_turn() {
                self.done.turned = true;
                self.turn(s, &a)?;
                continue;
            }
            if self.u["head"] != *head || self.u["pull_request"] != number {
                self.record_head(s, number, &head, None)?;
            }
            if !self.git_ok(
                s,
                &self.clone,
                &["merge-base", "--is-ancestor", &self.base, &head],
            ) {
                return stop("behind_main", json!({"base": self.base, "head": head}));
            }
            self.checks(s, &head)?;
            self.gate(s, number, &head)?;
            let v = self.verdicts(s)?;
            let (verdict, review) = match v.of(&head) {
                Some((k, r)) => (
                    r["verdict"].as_str().unwrap_or_default().to_owned(),
                    k.clone(),
                ),
                None => self.verify(s, number, &head, &v)?,
            };
            if verdict == "accept" {
                self.apply(
                    s,
                    "accept_unit",
                    json!({"unit": self.unit_id(), "head": head}),
                )?;
                return Ok(());
            }
            let v = self.verdicts(s)?;
            self.repair(s, &review, &head, &v)?;
        }
    }
    /// The answer (R25).
    fn answer(
        &mut self,
        s: &mut impl System,
        stopped: Option<(&str, &str)>,
    ) -> Result<(Value, u8), Failure> {
        self.reread(s)?;
        let v = match self.verdicts(s) {
            Ok(v) => v,
            Err(Halt::Fail(f)) => return Err(f),
            Err(Halt::Stop(..)) => return Err(error("store_answer_unreadable", "reviews")),
        };
        let one = |(k, r): &(String, Value)| json!({"key": k, "round": r["round"], "head": r["head"], "verdict": r["verdict"]});
        let u = &self.u;
        let mut a = json!({"unit": self.key, "state": u["state"], "head": u["head"],
            "pull_request": u["pull_request"], "rounds": v.rounds(), "repairs": v.repairs(),
            "reviews": v.list.iter().map(one).collect::<Vec<_>>()});
        let Some((cause, detail)) = stopped else {
            let approve = u["class"] == "contract" || u["class"] == "effect";
            a["waits_for"] = json!(if approve { "approve" } else { "merge" });
            return Ok((a, 0));
        };
        (a["stop_cause"], a["stop_detail"]) = (json!(cause), json!(detail));
        Ok((a, 3))
    }
}
