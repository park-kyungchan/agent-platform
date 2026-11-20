//! `ap unit run <key>`: one unit of the store driven from `ready`, `stopped` or a cut run to
//! `accepted` or `stopped`, its repairs and the updates of its branch inside, then a `plain` or
//! approved unit merged and its worktree removed (`specs/ap/unit-run.md`); `ap unit approve|reject`,
//! the Lead's word on an accepted unit. The stages are `ap run`'s code, the gate is `ap gate`'s, the
//! acceptance checks run as `ap unit ready` runs them, and the review of a head is recorded as
//! `ap unit review` records one.
use super::object::{self, branch_ok, clone_of};
use super::page_review::{self, key_ok, stage_unit, unit_ended};
use super::render::{self, Role};
use crate::host::{Failure, Host, System, args, digest, error, fail, id, new_session_id};
use crate::review::{self, DIFF_OPTIONS};
use crate::store::{self, Actor, Applied, Lead, at};
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
/// The most updates of a branch in one call (R26).
const UPDATES: u32 = 5;
/// The author and committer of the update's merge (R26).
const MERGER: [&str; 4] = [
    "GIT_AUTHOR_NAME=ap unit run",
    "GIT_AUTHOR_EMAIL=ap-unit@agents.invalid",
    "GIT_COMMITTER_NAME=ap unit run",
    "GIT_COMMITTER_EMAIL=ap-unit@agents.invalid",
];

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
    /// A stop of a failed `gh` call: the cause, the detail and the call's reply, redacted (R22, R25).
    Said(String, String, String),
    Fail(Failure),
}
impl From<Failure> for Halt {
    fn from(f: Failure) -> Self {
        Self::Fail(f)
    }
}
type Step<T = ()> = Result<T, Halt>;
/// The paths a unit of class `plain` may not change (R36), every name lower-case: exactly these
/// whole paths.
const PROTECTED_EXACT: [&str; 2] = ["check", "review.md"];
/// The last component, in any directory: Cargo, rustfmt and rustup read the configuration files
/// below the root too, and a build script or a manifest of any package runs or steers a build that
/// delivery makes as root.
const PROTECTED_NAMES: [&str; 9] = [
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    "build.rs",
    "cargo.toml",
    "cargo.lock",
];
/// The last two components, in any directory: Cargo's configuration below the root.
const PROTECTED_CONFIGS: [&str; 2] = [".cargo/config.toml", ".cargo/config"];
/// The first component: the path itself or anything under it.
const PROTECTED_TREES: [&str; 10] = [
    "ci", ".github", "specs", "scripts", ".cargo", "deploy", "etc", "content", "github", "ontology",
];
/// A path a unit of class `plain` may not change (R36): `path` is repository-relative, and every
/// name is compared without case (`CI/x`, `.GitHub/…`).
fn protected(path: &str) -> bool {
    let path = path.to_lowercase();
    let last = path.rsplit('/').next().unwrap_or(&path);
    let first = path.split('/').next().unwrap_or(&path);
    let config = PROTECTED_CONFIGS
        .iter()
        .any(|c| path == *c || path.ends_with(&format!("/{c}")));
    PROTECTED_EXACT.contains(&path.as_str())
        || PROTECTED_NAMES.contains(&last)
        || PROTECTED_TREES.contains(&first)
        || config
}
/// `gh pr merge` with `words`: `Err` with the redacted reply when it failed (no reply: empty).
fn merge_call(s: &mut impl System, words: &str) -> Result<(), String> {
    match s.call("gh", &args(&w(words)), &Value::Null, 120_000) {
        Ok(r) if r.success => Ok(()),
        Ok(r) => Err(crate::unit::redact(&r.bytes)),
        Err(_) => Err(String::new()),
    }
}
/// Whether Git's message says a ref was locked by another process: two fetches into one clone at
/// once (two units of one repository), the one cause of a failed fetch that a second try removes.
pub(crate) fn racing(message: &[u8]) -> bool {
    message.windows(15).any(|w| w == b"cannot lock ref")
}
/// A stop that keeps the redacted reply of the failed `gh` call that caused it.
fn said<T>(cause: &str, detail: impl ToString, reply: String) -> Step<T> {
    Err(Halt::Said(cause.to_owned(), detail.to_string(), reply))
}
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
/// The key of a unit as `ap unit add` makes one (R2).
pub(crate) fn key_form(key: &str) -> bool {
    let byte = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-';
    let first = key.starts_with(|c: char| c.is_ascii_lowercase());
    (1..=59).contains(&key.len()) && first && key.bytes().all(byte)
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

/// The refusal an `ont action validate` answers, in the form of the store's refusal of an apply.
fn not_valid(checked: &Value) -> Option<Value> {
    let criteria = checked["result"]["criteria"].as_array().into_iter();
    let no: Vec<&Value> = criteria
        .flatten()
        .filter(|c| c["passed"] == false)
        .map(|c| &c["code"])
        .collect();
    let detail = json!({"status": "refused", "code": "submission_criteria", "failed": no});
    (checked["result"]["valid"] != true).then_some(detail)
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
    let state = run.u["state"].as_str().unwrap_or_default().to_owned();
    let ended = state == "merged" || state == "dropped";
    let begun = match state.as_str() {
        // The Lead's rejection is undone by an amendment or a drop, never by a run (R6).
        "stopped" if run.u["stop_cause"] == "lead_rejected" => {
            let detail = run.u["stop_detail"].as_str().unwrap_or_default().to_owned();
            let answered = run.answer(s, Some(("lead_rejected", &detail)), &Outcome::default());
            return answered.map_err(|f| refusal(f, 3, true));
        }
        "ready" | "stopped" => run.begin(s),
        "running" => run.held(s),
        "accepted" | "merged" | "dropped" => Ok(()),
        other => Err(fail("unit_not_runnable", "unit", json!(other))),
    };
    begun.map_err(|f| refusal(f, 2, false))?;
    let driven = run.leftovers(s).and_then(|()| match ended {
        true => Ok(Outcome {
            cleanup: Some(run.cleanup(s)),
            ..Outcome::default()
        }),
        false => run.drive(s),
    });
    let driven = driven.map_err(|h| match h {
        Halt::Said(cause, detail, reply) => (Halt::Stop(cause, detail), Some(reply)),
        other => (other, None),
    });
    let answered = match driven {
        Ok(out) => run.answer(s, None, &out),
        // A unit that is not running is not stopped: the answer names the refusal, and the reply
        // of a failed `gh` call that caused it as `detail` (R22, R25, R30).
        Err((Halt::Stop(cause, _), reply)) if run.u["state"] != "running" => {
            let out = Outcome {
                refused: Some(cause),
                detail: reply,
                ..Outcome::default()
            };
            run.answer(s, None, &out)
        }
        Err((Halt::Stop(cause, detail), _)) => {
            let detail = cut(&detail, DETAIL);
            let p = json!({"unit": id("unit", key), "cause": cause, "detail": detail});
            let stopped = run.apply(s, "stop_unit", p);
            stopped.and_then(|_| run.answer(s, Some((&cause, &detail)), &Outcome::default()))
        }
        Err((Halt::Fail(f), _)) => Err(f),
        Err((Halt::Said(..), _)) => unreachable!("a stop with a reply was taken apart above"),
    };
    let status = if run.applied || ended { 3 } else { 2 };
    answered.map_err(|f| refusal(f, status, true))
}
/// How a call that did not stop ended: the merge, the cleanup, the refusal that left the unit as
/// it is (R25).
#[derive(Default)]
struct Outcome {
    merged_commit: Option<String>,
    cleanup: Option<Value>,
    refused: Option<String>,
    /// The redacted reply of the failed `gh` call that caused `refused`.
    detail: Option<String>,
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
    own: Option<Own>,
    checks: u32,
    updates: u32,
}
/// The heads this call's own fixer and updates moved, one after the other, from the head `from`
/// (R19).
struct Own {
    from: String,
    to: String,
    repair: bool,
    update: bool,
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
pub(crate) fn units(policy: &Value) -> Result<(String, Map<String, Value>), Failure> {
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
/// The policy file as one JSON object, or null.
fn policy_of(s: &impl System) -> Value {
    let file = s
        .policy_file()
        .and_then(|p| s.read(&p.to_string_lossy()).ok());
    file.and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
/// The directories and the lock of the unit `key`, of a checked form (R3, R5, R35): the units
/// directory is resolved once; `<key>/wt` is appended to it unresolved (R8).
fn locked(directory: &str, key: &str) -> Result<(PathBuf, File), Failure> {
    let io = |what: &str| fail("io_error", "directory", json!(what));
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    dirs.create(directory)
        .map_err(|_| io("the units directory cannot be made"))?;
    let root = std::fs::canonicalize(directory).map_err(|_| io("the units directory"))?;
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
    Ok((home, lock))
}
/// What the checks of a unit read before any action give (R2 to R4): its places and its tiers.
pub(crate) struct Checked {
    repo: String,
    branch: String,
    clone: String,
    reviews: Map<String, Value>,
    task: String,
    directory: String,
    timeouts: Map<String, Value>,
    implementer: Value,
    verifier: Value,
}
/// The checks `ap unit run` makes on the unit `u` of the key `key` before any action (R2 to R4): the
/// key, the repository, the branch, the policy's entries and the tiers. The dispatch pass
/// (`specs/ap/work.md` R10) calls the same function, so a unit the run refuses is not started.
pub(crate) fn checked(s: &mut impl System, key: &str, u: &Value) -> Result<Checked, Failure> {
    if !key_form(key) {
        return Err(error("invalid_input", "unit"));
    }
    let repo = text(u, "repository").ok_or_else(|| error("no_repository", "unit"))?;
    let branch = text(u, "branch").map_or_else(|| format!("unit/{key}"), str::to_owned);
    // `git check-ref-format` only for a name of R10's form, which never begins with `-`.
    let format = args(&["check-ref-format", "--branch", &branch]);
    let named = branch_ok(&branch)
        && (s.call("git", &format, &Value::Null, 10_000)).is_ok_and(|r| r.success);
    if !named {
        return Err(fail("invalid_branch", "branch", json!(branch)));
    }
    let clone = clone_of(s, repo)?;
    // `clone_of` read the file as one JSON object.
    let policy = policy_of(s);
    let reviews = page_review::policy(s)?;
    let task = reviews.get("task").and_then(Value::as_str);
    let task = task
        .ok_or_else(|| error("invalid_input", "task"))?
        .to_owned();
    let (directory, timeouts) = units(&policy)?;
    let implementer = tier(u, "implementer", "implementer", &policy)?;
    let verifier = tier(u, "verifier", "reviewer", &policy)?;
    Ok(Checked {
        repo: repo.to_owned(),
        branch,
        clone,
        reviews,
        task,
        directory,
        timeouts,
        implementer,
        verifier,
    })
}
/// The checks before any effect (R1 to R4), then the directories and the lock (R3, R5).
fn prepare(s: &mut impl System, store: Option<&str>, key: &str) -> Result<(Run, File), Failure> {
    if object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    if !key_form(key) {
        return Err(error("invalid_input", "unit"));
    }
    let u = at(store::get(s, store, "unit", key), "unit")?;
    let u = u.ok_or_else(|| error("unknown_unit", "unit"))?;
    let c = checked(s, key, &u)?;
    let (home, lock) = locked(&c.directory, key)?;
    let drawn = new_session_id(s)?.replace('-', "");
    let dir = c.reviews.get("directory").and_then(Value::as_str);
    let run = Run {
        store: store.map(str::to_owned),
        key: key.to_owned(),
        session: format!("u-{key}"),
        prefix: format!("ur-{}", &drawn[..16]),
        repo: c.repo,
        branch: c.branch,
        clone: c.clone,
        wt: home.join("wt").to_string_lossy().into_owned(),
        home,
        reviews: PathBuf::from(dir.unwrap_or(REVIEWS)),
        task: c.task,
        implementer: c.implementer,
        verifier: c.verifier,
        timeouts: c.timeouts,
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
        self.apply_as(s, action, rid, p)
    }
    fn apply_as(
        &mut self,
        s: &mut impl System,
        action: &str,
        rid: String,
        p: Value,
    ) -> Result<Applied, Failure> {
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
        self.apply(s, action, p)?;
        self.u["state"] = json!("running");
        Ok(())
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
        let mut fetched = s.call("git", &args(&words), &Value::Null, 120_000);
        // A fetch of `main` that raced another one on the same clone is tried once more (R8).
        if what == "main"
            && fetched
                .as_ref()
                .is_ok_and(|r| !r.success && racing(&r.bytes))
        {
            s.pause(Duration::from_secs(1));
            fetched = s.call("git", &args(&words), &Value::Null, 120_000);
        }
        match fetched.is_ok_and(|r| r.success) {
            true => Ok(()),
            false => stop("fetch_failed", what),
        }
    }
    fn gh(&self, s: &mut impl System, words: &str) -> Step<Value> {
        let reply = s.call("gh", &args(&w(words)), &Value::Null, 60_000);
        // A call that ran and failed keeps its reply, redacted, in the detail (R22, R25).
        if let Some(r) = reply.as_ref().ok().filter(|r| !r.success) {
            let reply = crate::unit::redact(&r.bytes);
            let detail = json!({"call": &words[..7], "reply": reply});
            return said("github_unreadable", detail, reply);
        }
        let v = reply.ok();
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
        let own = |p: &&Value| p["baseRefName"] == "main" && self.of_branch(p);
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
    /// A pull request whose head branch is the unit's branch of the repository itself (R9, R10).
    fn of_branch(&self, p: &Value) -> bool {
        let (owner, name) = self.repo.split_once('/').unwrap_or_default();
        let same = |a: &Value, b: &str| a.as_str().is_some_and(|a| a.eq_ignore_ascii_case(b));
        p["headRefName"] == self.branch.as_str()
            && p["isCrossRepository"] == false
            && same(&p["headRepositoryOwner"]["login"], owner)
            && same(&p["headRepository"]["name"], name)
    }
    /// Pull request `n` as the hosting service shows it: its state and head (checked), its base, its
    /// head branch and repository, its merge commit.
    fn pull(&self, s: &mut impl System, n: u64) -> Step<Value> {
        let fields = "state,baseRefName,headRefOid,headRefName,isCrossRepository,headRepositoryOwner,headRepository,mergeCommit";
        let v = self.gh(
            s,
            &format!("pr view {n} --repo {} --json {fields}", self.repo),
        )?;
        let head = v["headRefOid"].as_str().is_some_and(hex40);
        match (v["state"].is_string(), head) {
            (true, true) => Ok(v),
            _ => stop("github_unreadable", "pr view"),
        }
    }
    /// The state and the head of pull request `n`.
    fn view(&self, s: &mut impl System, n: u64) -> Step<(String, String)> {
        let v = self.pull(s, n)?;
        let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Ok((text("state"), text("headRefOid")))
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
            // A pull request the unit holds makes the branch its own only as that one's head branch.
            let held = match self.u["pull_request"].as_u64().filter(|_| !own) {
                Some(n) => self.of_branch(&self.pull(s, n)?),
                None => own,
            };
            if !held {
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
        if self.u["class"] == "plain" {
            let hit: Vec<&&str> = paths.iter().filter(|p| protected(p)).collect();
            if !hit.is_empty() {
                let shown: Vec<&&&str> = hit.iter().take(10).collect();
                return stop(
                    "protected_path_changed",
                    json!({"paths": shown, "count": hit.len()}),
                );
            }
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

    /// This call's own fixer or update moved the head `from` to `to` (R19).
    fn moved(&mut self, from: &str, to: &str, repair: bool) {
        let own = match self.done.own.take() {
            Some(o) if o.to == from => o,
            _ => Own {
                from: from.to_owned(),
                to: String::new(),
                repair: false,
                update: false,
            },
        };
        self.done.own = Some(Own {
            to: to.to_owned(),
            repair: own.repair || repair,
            update: own.update || !repair,
            ..own
        });
    }
    /// The record of the newest verdict's session, the head it judged and what moved it, when the
    /// round on `head` resumes it (R19): only when this call's own fixer and updates moved the judged
    /// head to `head`, and no move of another since.
    fn second_look(&self, v: &Verdicts, head: &str) -> Option<(String, String, &'static str)> {
        let since = v.list.iter().rfind(|(_, r)| v.later(r));
        let abandoned = since.is_some_and(|(_, r)| r["verdict"] == "abandoned");
        let (k, r) = v.newest()?;
        let (judged, session) = (r["head"].as_str()?, r["session"].as_str()?);
        let writers = self.u["writers"].as_array();
        let writer = writers.is_some_and(|w| w.iter().any(|w| w == session));
        let own = self
            .done
            .own
            .as_ref()
            .filter(|o| o.from == judged && o.to == head);
        let what = match own.map(|o| (o.repair, o.update)) {
            Some((true, true)) => "the author's repair and a merge of origin/main",
            Some((true, false)) => "the author's repair",
            _ => "a merge of origin/main",
        };
        let record = format!("{}.run.json", self.report(k)?);
        let fit = own.is_some() && !self.done.moved && !abandoned && !writer;
        let fit = fit && json_file(&record).is_some();
        fit.then(|| (record, judged.to_owned(), what))
    }
    /// One validate naming the review's key, the admission of the stage, the start (R19).
    fn start_review(&mut self, s: &mut impl System, body: &Value, req: &mut Value) -> Step<String> {
        let checked = at(store::validate(s, self.store(), body), "start")?;
        if let Some(detail) = not_valid(&checked) {
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
        let resumed = self.second_look(v, head);
        let mut text = self.page(s, Role::Verifier)?;
        if let Some((_, judged, what)) = &resumed {
            let round = format!("Head judged before: {judged}\nWhat moved it: {what}\n");
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
        if let Some((record, ..)) = &resumed {
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
        // A verifier that wrote or pushed becomes a writer before the unit stops; a read that fails
        // closes the review first (R20).
        let read = self
            .tree(s)
            .and_then(|t| self.view(s, number).map(|v| (t, v)));
        let (t, (state, now)) = match read {
            Ok(read) => read,
            Err(h) => {
                self.close(s, &review, abandon)?;
                return Err(h);
            }
        };
        if !t.clean || t.head != head || now != head || !t.on_branch || state != "OPEN" {
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
        // The moves of this call are counted again from this verdict.
        self.done.own = None;
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
        let now = self.git(s, &self.wt, &["rev-parse", "HEAD"])?;
        if now == judged {
            return stop("repair_refused", Self::answered(&out, &rec));
        }
        self.moved(judged, &now, true);
        self.wait(s)
    }

    /// One base commit: `refs/remotes/origin/main` after a fetch (R8, R30).
    fn take_base(&mut self, s: &mut impl System) -> Step {
        self.fetch(s, "main")?;
        let main = ["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"];
        self.base = self.git(s, &self.clone, &main)?;
        if !hex40(&self.base) {
            return stop("fetch_failed", "refs/remotes/origin/main");
        }
        Ok(())
    }
    fn contains_base(&self, s: &mut impl System, head: &str) -> bool {
        let words = ["merge-base", "--is-ancestor", &self.base, head];
        self.git_ok(s, &self.clone, &words)
    }
    /// The loop: each pass reads, then takes the first step that applies (R8 to R21, R26 to R30).
    fn drive(&mut self, s: &mut impl System) -> Step<Outcome> {
        loop {
            if self.u["state"] == "accepted" {
                match self.land(s)? {
                    Some(out) => return Ok(out),
                    None => continue,
                }
            }
            self.take_base(s)?;
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
            let pending = self.own_merge(s, &t, &head);
            if pending.is_none() && (t.head != head || !t.clean || !t.on_branch) {
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
            if let Some(a) = self.amended_turn().filter(|_| pending.is_none()) {
                self.done.turned = true;
                self.turn(s, &a)?;
                continue;
            }
            if self.u["head"] != *head || self.u["pull_request"] != number {
                self.record_head(s, number, &head, None)?;
            }
            // A head judged `repair` goes to its author before any update (R29).
            let v = self.verdicts(s)?;
            if let Some((k, r)) = v.of(&head).filter(|_| pending.is_none())
                && r["verdict"] == "repair"
            {
                let k = k.clone();
                self.repair(s, &k, &head, &v)?;
                continue;
            }
            if !self.contains_base(s, &head) {
                self.update(s, number, &head, pending)?;
                continue;
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
                let p = json!({"unit": self.unit_id(), "head": head});
                self.apply(s, "accept_unit", p)?;
                self.reread(s)?;
                // A `plain` unit goes on to its merge in this call (R18, R30).
                if self.u["class"] != "plain" {
                    return Ok(Outcome::default());
                }
                continue;
            }
            let v = self.verdicts(s)?;
            self.repair(s, &review, &head, &v)?;
        }
    }

    /// The message of the update's own merges (R26, R27).
    fn merge_message(&self) -> String {
        format!("Merge origin/main into {} (ap unit run)", self.branch)
    }
    /// A clean worktree on the branch whose HEAD is the update's own merge of `head` with the base or
    /// an ancestor of it, made by a call cut before its push (R27).
    fn own_merge(&self, s: &mut impl System, t: &Tree, head: &str) -> Option<String> {
        if !t.clean || !t.on_branch || t.head == head {
            return None;
        }
        let parents = review::git(s, &self.wt, &["rev-list", "--parents", "-n", "1", &t.head]);
        let parents = parents.ok()?;
        let [_, first, second] = parents.split_whitespace().collect::<Vec<_>>()[..] else {
            return None;
        };
        let message = review::git(s, &self.wt, &["log", "-1", "--format=%B", &t.head]).ok()?;
        let ours = first == head && message.trim_end() == self.merge_message();
        let ancestor = ["merge-base", "--is-ancestor", second, &self.base];
        (ours && self.git_ok(s, &self.wt, &ancestor)).then(|| t.head.clone())
    }
    /// The update of a head behind the base: the base merged into the branch in the worktree (or the
    /// run's own unpushed merge taken), the writers recorded, the push, the wait (R26 to R28).
    fn update(
        &mut self,
        s: &mut impl System,
        number: u64,
        head: &str,
        pending: Option<String>,
    ) -> Step {
        if self.done.updates >= UPDATES {
            return stop("update_limit", json!({"updates": self.done.updates}));
        }
        let merged = match pending {
            Some(m) => m,
            None => self.merge_base(s)?,
        };
        let word = format!("{}:update", self.session);
        let writers = self.u["writers"].as_array();
        let known = writers.is_some_and(|w| w.iter().any(|w| *w == *word));
        if self.u["head"] != *head || !known {
            self.record_head(s, number, head, Some(&word))?;
        }
        let to = format!("HEAD:refs/heads/{}", self.branch);
        let push = [
            "--no-replace-objects",
            "-C",
            &self.wt,
            "push",
            "--quiet",
            "origin",
            &to,
        ];
        let pushed = s.call("git", &args(&push), &Value::Null, 120_000);
        if !pushed.is_ok_and(|r| r.success) {
            return stop("push_failed", json!({"head": merged}));
        }
        self.done.updates += 1;
        self.moved(head, &merged, false);
        self.wait(s)
    }
    /// The base merged into the worktree's HEAD by the run's own author; a conflict is aborted (R26).
    fn merge_base(&self, s: &mut impl System) -> Step<String> {
        let message = self.merge_message();
        let mut words = MERGER.to_vec();
        words.extend([
            "git",
            "--no-replace-objects",
            "-C",
            &self.wt,
            "merge",
            "--no-ff",
        ]);
        words.extend(["--no-edit", "--quiet", "-m", &message, &self.base]);
        let merged = s.call("env", &args(&words), &Value::Null, 120_000);
        if merged.is_ok_and(|r| r.success) {
            return self.git(s, &self.wt, &["rev-parse", "HEAD"]);
        }
        let paths = ["diff", "--name-only", "--diff-filter=U"];
        let paths = review::git(s, &self.wt, &paths).unwrap_or_default();
        self.git(s, &self.wt, &["merge", "--abort"])?;
        let paths: Vec<&str> = paths.lines().take(10).collect();
        stop("merge_conflict", json!({"base": self.base, "paths": paths}))
    }

    /// An accepted unit: the pull request read against a new base, then the merge, the reopen or the
    /// wait for the Lead's approval (R30). `None` when the unit runs again.
    fn land(&mut self, s: &mut impl System) -> Step<Option<Outcome>> {
        self.take_base(s)?;
        let head = self.u["head"].as_str().unwrap_or_default().to_owned();
        let Some(n) = self.u["pull_request"].as_u64() else {
            return stop("pull_request_not_open", json!({"pull_request": null}));
        };
        let p = self.pull(s, n)?;
        let (state, now) = (p["state"].as_str().unwrap_or_default(), &p["headRefOid"]);
        // Only a pull request against `main` is merged or recorded merged; one moved off it is not.
        if p["baseRefName"] != "main" {
            let detail = json!({"pull_request": n, "state": state, "base": p["baseRefName"]});
            return stop("pull_request_not_open", detail);
        }
        match state {
            "MERGED" if *now == *head => {
                let commit = p["mergeCommit"]["oid"].as_str().filter(|c| hex40(c));
                let Some(commit) = commit else {
                    return stop("github_unreadable", "mergeCommit");
                };
                let rid = self.rid();
                let p = self.merge_parameters(&head, commit);
                self.record_merge(s, rid, p).map(Some)
            }
            "OPEN" if *now != *head => self.reopen(s, "head_moved").map(|()| None),
            "OPEN" if !self.contains_base(s, &head) => self.reopen(s, "main_moved").map(|()| None),
            "OPEN" if self.u["class"] == "plain" || self.u["approved_head"] == *head => {
                self.merge(s, n, &head).map(Some)
            }
            "OPEN" => Ok(Some(Outcome::default())),
            _ => stop(
                "pull_request_not_open",
                json!({"pull_request": n, "state": state, "head": now}),
            ),
        }
    }
    /// `reopen_unit` with `cause`; the store's refusal ends the call with the unit `accepted` (R30).
    fn reopen(&mut self, s: &mut impl System, cause: &str) -> Step {
        let mut p = json!({"unit": self.unit_id(), "cause": cause});
        if self.u["lane"] == "objective" {
            p["intent"] = self.u["intent"].clone();
        }
        match self.apply(s, "reopen_unit", p) {
            Err(f) if f.code == store::REFUSED => stop(&store::code(&f), f.detail),
            other => {
                other?;
                Ok(self.reread(s)?)
            }
        }
    }
    fn merge_parameters(&self, head: &str, commit: &str) -> Value {
        let mut p = json!({"unit": self.unit_id(), "head": head, "merged_commit": commit});
        if self.u["lane"] == "objective" {
            p["intent"] = self.u["intent"].clone();
        }
        p
    }
    /// The merge: `merge_unit` validated, the status `lead-review`, the merge with the head pinned,
    /// read back, then `merge_unit` applied with the merge commit (R31).
    fn merge(&mut self, s: &mut impl System, n: u64, head: &str) -> Step<Outcome> {
        let rid = self.rid();
        let p = self.merge_parameters(head, &"0".repeat(40));
        let actor = Actor {
            agent: "ap-unit",
            session: &self.session,
        };
        let body = store::body(&actor, "merge_unit", rid.clone(), p);
        let checked = at(store::validate(s, self.store(), &body), "merge_unit")?;
        if let Some(detail) = not_valid(&checked) {
            let code = detail["failed"][0]
                .as_str()
                .unwrap_or(store::REFUSED)
                .to_owned();
            return stop(&code, detail);
        }
        let v = self.verdicts(s)?;
        let review = v.of(head).map(|(k, _)| k.clone()).unwrap_or_default();
        let target = json!({"repository": self.repo, "head": head});
        let posted = review::post(s, &target, "success", &format!("review {review} accept"));
        if let Err(f) = posted {
            let reply = f.detail.as_str().unwrap_or_default().to_owned();
            return said("status_failed", json!({"head": head}), reply);
        }
        let words = format!(
            "pr merge {n} --repo {} --merge --match-head-commit {head}",
            self.repo
        );
        let mut called = merge_call(s, &words);
        // The read decides, also after a call that failed: it may have merged.
        let mut read = self.pull(s, n).ok();
        // A merge refused for a status GitHub had not yet taken: once more, when the head's
        // `lead-review` reads `success`, after a pause (R31).
        let open = read.as_ref().is_some_and(|p| p["state"] != "MERGED");
        if called.is_err()
            && open
            && review::lead_review(s, &self.repo, head).as_deref() == Some("success")
        {
            s.pause(Duration::from_secs(5));
            called = merge_call(s, &words);
            read = self.pull(s, n).ok();
        }
        let merged = read.filter(|p| {
            p["state"] == "MERGED" && p["headRefOid"] == head && p["baseRefName"] == "main"
        });
        let commit = merged.and_then(|p| p["mergeCommit"]["oid"].as_str().map(str::to_owned));
        let Some(commit) = commit.filter(|c| hex40(c)) else {
            let Err(reply) = called else {
                return stop("merge_unconfirmed", json!({"head": head}));
            };
            let _ = review::post(s, &target, "failure", "merge failed");
            return said("merge_failed", json!({"head": head}), reply);
        };
        let p = self.merge_parameters(head, &commit);
        self.record_merge(s, rid, p)
    }
    /// `merge_unit` applied, then the cleanup; the store's refusal leaves the unit as it reads (R30,
    /// R31, R35).
    fn record_merge(&mut self, s: &mut impl System, rid: String, p: Value) -> Step<Outcome> {
        let mut out = Outcome {
            merged_commit: p["merged_commit"].as_str().map(str::to_owned),
            ..Outcome::default()
        };
        match self.apply_as(s, "merge_unit", rid, p) {
            Err(f) if f.code == store::REFUSED => out.refused = Some(store::code(&f)),
            Err(f) => return Err(f.into()),
            Ok(_) => {
                self.reread(s)?;
                out.cleanup = Some(self.cleanup(s));
            }
        }
        self.reread(s)?;
        Ok(out)
    }
    /// The cleanup of a merged or dropped unit: the one worktree, the one branch, and for a dropped
    /// unit its pull request named when it is open (R32).
    fn cleanup(&mut self, s: &mut impl System) -> Value {
        let dropped = self.u["state"] == "dropped";
        let mut c = json!({});
        let wt = Path::new(&self.wt);
        let worktree = if wt.symlink_metadata().is_err() {
            Ok("nothing")
        } else if !real_dir(&self.home) || !real_dir(wt) {
            Err("not a directory")
        } else {
            match self.entry(s) {
                Ok(Some(_)) => {
                    let mut words = vec!["worktree", "remove"];
                    words.extend(dropped.then_some("--force"));
                    words.push(&self.wt);
                    match self.git_ok(s, &self.clone, &words) {
                        true => Ok("removed"),
                        false => Err("git worktree remove refused: the worktree is not clean"),
                    }
                }
                Ok(None) => Err("no worktree of the clone at this path"),
                Err(_) => Err("git worktree list failed"),
            }
        };
        match worktree {
            Ok(w) => c["worktree"] = json!(w),
            Err(why) => {
                (c["worktree"], c["worktree_reason"]) = (json!("worktree_kept"), json!(why))
            }
        }
        let made = !self.u["pull_request"].is_null() || !self.authors().is_empty();
        let branch = if made {
            self.drop_branch(s)
        } else {
            Ok("nothing")
        };
        match branch {
            Ok(b) => c["branch"] = json!(b),
            Err(why) => (c["branch"], c["branch_reason"]) = (json!("kept"), json!(why)),
        }
        if let Some(n) = self.u["pull_request"].as_u64().filter(|_| dropped)
            && self.view(s, n).is_ok_and(|(state, _)| state == "OPEN")
        {
            c["pull_request_open"] = json!(n);
        }
        c
    }
    /// The unit's branch deleted only when a fetch now shows its tip in `main` (R32).
    fn drop_branch(&mut self, s: &mut impl System) -> Result<&'static str, &'static str> {
        let (clone, branch) = (self.clone.clone(), self.branch.clone());
        let reference = format!("refs/heads/{branch}");
        let tip = review::git(s, &clone, &["rev-parse", "--verify", "--quiet", &reference]);
        let Ok(tip) = tip.map(|t| t.trim().to_owned()) else {
            return Ok("nothing");
        };
        if self.fetch(s, "main").is_err() {
            return Err("fetch_failed");
        }
        let merged = [
            "merge-base",
            "--is-ancestor",
            &tip,
            "refs/remotes/origin/main",
        ];
        if !self.git_ok(s, &clone, &merged) {
            return Err("not merged into main");
        }
        let deleted = self.git_ok(s, &clone, &["branch", "-d", &branch])
            || self.git_ok(s, &clone, &["branch", "-D", &branch]);
        deleted
            .then_some("removed")
            .ok_or("git branch -D refused: the branch is checked out")
    }

    /// The answer (R25).
    fn answer(
        &mut self,
        s: &mut impl System,
        stopped: Option<(&str, &str)>,
        out: &Outcome,
    ) -> Result<(Value, u8), Failure> {
        self.reread(s)?;
        let v = match self.verdicts(s) {
            Ok(v) => v,
            Err(Halt::Fail(f)) => return Err(f),
            Err(Halt::Stop(..) | Halt::Said(..)) => {
                return Err(error("store_answer_unreadable", "reviews"));
            }
        };
        let one = |(k, r): &(String, Value)| json!({"key": k, "round": r["round"], "head": r["head"], "verdict": r["verdict"]});
        let u = &self.u;
        let merged = out
            .merged_commit
            .as_ref()
            .map_or(u["merged_commit"].clone(), |c| json!(c));
        let mut a = json!({"unit": self.key, "state": u["state"], "head": u["head"],
            "pull_request": u["pull_request"], "rounds": v.rounds(), "repairs": v.repairs(),
            "reviews": v.list.iter().map(one).collect::<Vec<_>>(), "updates": self.done.updates,
            "merged_commit": merged});
        if let Some(c) = &out.cleanup {
            a["cleanup"] = c.clone();
        }
        if let Some((cause, detail)) = stopped {
            (a["stop_cause"], a["stop_detail"]) = (json!(cause), json!(detail));
            return Ok((a, 3));
        }
        if let Some(code) = &out.refused {
            a["refused"] = json!(code);
            if let Some(reply) = &out.detail {
                a["detail"] = json!(reply);
            }
            return Ok((a, 3));
        }
        if u["state"] == "accepted" {
            let approve = u["class"] == "contract" || u["class"] == "effect";
            a["waits_for"] = json!(if approve { "approve" } else { "merge" });
        }
        Ok((a, 0))
    }
}

/// The arguments after `approve` or `reject`: none, or `--store <absolute path>` (R33).
fn store_argument(argv: &[OsString]) -> Result<Option<String>, Failure> {
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    match words.as_deref() {
        Some([]) => Ok(None),
        Some(["--store", p]) if p.starts_with('/') => Ok(Some((*p).to_owned())),
        _ => Err(error("usage", "args")),
    }
}
/// `ap unit approve|reject [--store <path>] < request.json`: the answer, or the refusal on standard
/// error with `after` and `unit`, exit 2 (R33, R34).
pub fn decide_cli(op: &str, argv: &[OsString]) -> ExitCode {
    let mut after = None;
    let mut unit = None;
    let result = store_argument(argv).and_then(|store| {
        let r = object::request()?;
        unit = r["unit"].as_str().map(str::to_owned);
        decide(&mut Host, store.as_deref(), op, &r, &mut after)
    });
    let (written, status) = match result {
        Ok(v) => (writeln!(std::io::stdout(), "{v}"), 0),
        Err(f) => {
            let code = store::code(&f);
            let v = json!({"error": {"code": code, "field": f.field, "detail": f.detail,
                "after": after, "unit": unit}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// One `approve` or `reject` (R33 to R35): every check before the lock, the lock, the Lead read, the
/// applies. `after` is the suffix of the last action applied.
pub fn decide(
    s: &mut impl System,
    store: Option<&str>,
    op: &str,
    r: &Value,
    after: &mut Option<String>,
) -> Result<Value, Failure> {
    let approve = op == "approve";
    let members = match approve {
        true => "request_id agent session unit head reason",
        false => "request_id agent session unit reason",
    };
    if !r.is_object() {
        return Err(store::bad("stdin"));
    }
    store::only(r, members, "")?;
    let rid = store::shaped(r, "request_id", 44, "._-")?;
    let actor = Actor {
        agent: store::shaped(r, "agent", 64, "._:-")?,
        session: store::shaped(r, "session", 64, "._:-")?,
    };
    let key = r["unit"].as_str().filter(|k| key_form(k));
    let key = key.ok_or_else(|| store::bad("unit"))?;
    let limit = if approve { 600 } else { DETAIL };
    let reason = r["reason"].as_str().ok_or_else(|| store::bad("reason"))?;
    if reason.len() > limit {
        let detail = json!({"bytes": reason.len(), "limit": limit});
        return Err(fail("invalid_input", "reason", detail));
    }
    let head = r["head"].as_str().filter(|h| hex40(h));
    if approve && head.is_none() {
        return Err(store::bad("head"));
    }
    // `reject` sends no role: the engine would give it the role of the environment.
    if !approve && object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    let (directory, _) = units(&policy_of(s))?;
    let (_, _lock) = locked(&directory, key)?;
    let u = at(store::get(s, store, "unit", key), "unit")?;
    let u = u.ok_or_else(|| error("unknown_unit", "unit"))?;
    let intent = u["intent"]["native_key"].as_str();
    let intent = intent.ok_or_else(|| error("no_intent", "unit"))?;
    let read = at(store::head(s, store, "intent", intent), "intent")?;
    let (revision, i) = read.ok_or_else(|| error("unknown_intent", "intent"))?;
    if i["lead_session"].as_str() != Some(actor.session) {
        return Err(fail("not_the_lead", "session", i["lead_session"].clone()));
    }
    let unit = id("unit", key);
    if approve {
        let lead = Lead {
            intent: u["intent"].clone(),
            revision,
        };
        let p = json!({"unit": unit, "head": head, "reason": reason});
        let rid = format!("{rid}-approve");
        let applied = at(
            store::approve_unit(s, store, &actor, rid, p, &lead),
            "approve",
        )?;
        *after = Some("approve".into());
        let back = at(store::get(s, store, "unit", key), "unit")?.unwrap_or_default();
        return Ok(
            json!({"unit": key, "outcome": applied.outcome, "state": back["state"],
            "approved_head": back["approved_head"]}),
        );
    }
    // No action clears an approval: an approved head is not rejected (R34).
    if u["approved_head"].is_string() && u["approved_head"] == u["head"] {
        return Err(fail("approved", "unit", u["head"].clone()));
    }
    let mut p = json!({"unit": unit, "cause": "rejected by the Lead"});
    if u["lane"] == "objective" {
        p["intent"] = u["intent"].clone();
    }
    let reopened = store::unit_action(s, store, &actor, "reopen_unit", format!("{rid}-reopen"), p);
    at(reopened, "reopen")?;
    *after = Some("reopen".into());
    let p = json!({"unit": unit, "cause": "lead_rejected", "detail": reason});
    let stopped = store::unit_action(s, store, &actor, "stop_unit", format!("{rid}-stop"), p);
    let applied = at(stopped, "stop")?;
    *after = Some("stop".into());
    let mut posted = Value::Null;
    if let (Some(_), Some(head), Some(repo)) = (
        u["pull_request"].as_u64(),
        u["head"].as_str(),
        u["repository"].as_str(),
    ) {
        let target = json!({"repository": repo, "head": head});
        let sent = review::post(s, &target, "failure", "rejected by the Lead");
        posted = json!(if sent.is_ok() {
            "failure"
        } else {
            "not_posted"
        });
    }
    let back = at(store::get(s, store, "unit", key), "unit")?.unwrap_or_default();
    Ok(
        json!({"unit": key, "outcome": applied.outcome, "state": back["state"],
        "lead_review": posted}),
    )
}

#[cfg(test)]
mod tests {
    use super::{PROTECTED_CONFIGS, PROTECTED_EXACT, PROTECTED_NAMES, PROTECTED_TREES, protected};

    /// Every protected path of R36, and the neighbours that are not (`specs/ap/unit-run.md` R36).
    #[test]
    fn r36_the_protected_paths_are_pinned() {
        // The lists themselves, so that a path added or taken away fails here without this change.
        assert_eq!(PROTECTED_EXACT, ["check", "review.md"]);
        assert_eq!(
            PROTECTED_NAMES,
            [
                "clippy.toml",
                ".clippy.toml",
                "rustfmt.toml",
                ".rustfmt.toml",
                "rust-toolchain",
                "rust-toolchain.toml",
                "build.rs",
                "cargo.toml",
                "cargo.lock",
            ]
        );
        assert_eq!(PROTECTED_CONFIGS, [".cargo/config.toml", ".cargo/config"]);
        assert_eq!(
            PROTECTED_TREES,
            [
                "ci", ".github", "specs", "scripts", ".cargo", "deploy", "etc", "content",
                "github", "ontology",
            ]
        );
        let held = [
            "content/SHA256SUMS",
            "Content/AGENTS.md",
            "content/skills/lead/SKILL.md",
            "github/apply",
            "github/ruleset-main.json",
            "ontology",
            "ontology/migrate",
            "ontology/json.py",
            "ontology/work.schema.json",
            "Ontology/README.md",
            "build.rs",
            "crates/ap/build.rs",
            "crates/x/Build.rs",
            "Cargo.toml",
            "crates/ap/Cargo.toml",
            "crates/x/CARGO.TOML",
            "Cargo.lock",
            "sub/Cargo.lock",
            "check",
            "REVIEW.md",
            "review.md",
            "rust-toolchain",
            "rust-toolchain.toml",
            "crates/x/rust-toolchain.toml",
            "sub/rust-toolchain",
            "ontology/rehearsal.sh",
            "ontology/ENGINE-PIN",
            "clippy.toml",
            ".clippy.toml",
            "crates/x/clippy.toml",
            "rustfmt.toml",
            ".rustfmt.toml",
            "crates/x/.rustfmt.toml",
            "ci",
            "ci/x",
            "CI/x",
            ".github/workflows/rust.yml",
            ".GitHub/workflows/rust.yml",
            "specs/ap/unit.md",
            "scripts/",
            "scripts/install-ap.sh",
            ".cargo",
            ".cargo/config.toml",
            "crates/x/.cargo/config.toml",
            "crates/x/.cargo/config",
            "crates/x/.Cargo/Config.toml",
            "deploy",
            "deploy/ap-deliver.timer",
            "Deploy/x",
            "etc",
            "etc/agent-platform/policy.json",
        ];
        for path in held {
            assert!(protected(path), "{path}");
        }
        let free = [
            "checks/x",
            "docs/specs/x",
            "crates/ci/x",
            "crates/x/deploy/x",
            "src/etc/x",
            "REVIEW.md.bak",
            "crates/x/cargo/config.toml",
            "crates/x/.cargo/other.toml",
            "rust-toolchain.txt",
            "deployment/x",
            "etcetera",
            "contents/x",
            "github.md",
            "githubs/x",
            "ontologies/x",
            "docs/content/x",
            "crates/ap/src/main.rs",
            "crates/x/build.rs.bak",
            "crates/x/cargo.lock.txt",
            "crates/x/mybuild.rs",
        ];
        for path in free {
            assert!(!protected(path), "{path}");
        }
    }
}
