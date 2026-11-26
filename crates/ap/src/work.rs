//! `ap work`: the dispatch pass (`specs/ap/work.md`). One pass, under the lock `work.lock` of the units
//! directory, reads the units of the store and the run units of the service manager once, and starts
//! each unit that may start as `ap unit run <key>` in a transient unit of its own; a `ready` unit that
//! does not start gets what it waits on as its `waits_on`. When the policy has a writer, the pass also
//! starts the writer of each draft unit of a horizon that has no page (R23 to R25).
use crate::host::{Failure, Host, LIMIT, System, error, fail, id, new_session_id};
use crate::quota;
use crate::run::KEEP;
use crate::status;
use crate::store::{self, Actor};
use crate::unit::drive::{self, key_form};
use crate::unit::{object, page_write, render, routing};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::File,
    io::{ErrorKind, Read as _, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

/// The longest wait for the pass's lock, and the step of that wait (R4).
const LOCK_WAIT: Duration = Duration::from_secs(120);
const LOCK_STEP: Duration = Duration::from_secs(1);
/// The schema's bound of `waits_on`.
const WAITS_ON: usize = 300;
/// The states of a run unit that is live (R5).
const LIVE: [&str; 4] = ["active", "activating", "deactivating", "reloading"];
/// The states of the units the pass reads (R5).
const READ: [&str; 3] = ["ready", "running", "accepted"];

/// The member `work` of the policy file (R3).
pub(crate) struct Work {
    slots: usize,
    slice: String,
    pub(crate) prefix: String,
    /// The writers in flight a pass may reach (R24), and the prefix of their units.
    pages: usize,
    page_prefix: String,
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
fn prefix_ok(p: &str) -> bool {
    (1..=16).contains(&p.len())
        && p.starts_with(|c: char| c.is_ascii_lowercase())
        && !p.ends_with('-')
        && p.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
/// The member `work` as R3 reads it; None when the policy has none.
pub(crate) fn work(policy: &Value) -> Result<Option<Work>, Failure> {
    let Some(w) = policy.get("work") else {
        return Ok(None);
    };
    let w = w
        .as_object()
        .ok_or_else(|| invalid("work", "is not an object"))?;
    if let Some(k) = w
        .keys()
        .find(|k| !["slots", "slice", "prefix", "pages", "page_prefix"].contains(&k.as_str()))
    {
        return Err(invalid(
            "work",
            &format!("holds the member {k}, which is not read"),
        ));
    }
    let slots = match w.get("slots") {
        None => 3,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=16).contains(n))
            .ok_or_else(|| invalid("work.slots", "is not an integer from 1 to 16"))?
            as usize,
    };
    let text = |k: &str, ok: fn(&str) -> bool, default: &str, why: &str| match w.get(k) {
        None => Ok(default.to_owned()),
        Some(v) => v
            .as_str()
            .filter(|t| ok(t))
            .map(str::to_owned)
            .ok_or_else(|| invalid(&format!("work.{k}"), why)),
    };
    let form =
        "is not 1 to 16 characters of a-z0-9- that begin with a letter and do not end with -";
    let slice = text(
        "slice",
        quota::slice_ok,
        "agents.slice",
        "is not a slice name of a-z0-9 and .slice",
    )?;
    let prefix = text("prefix", prefix_ok, "ap-unit", form)?;
    let pages = match w.get("pages") {
        None => 2,
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= 8)
            .ok_or_else(|| invalid("work.pages", "is not an integer from 0 to 8"))?
            as usize,
    };
    let page_prefix = text("page_prefix", prefix_ok, "ap-page", form)?;
    // The writers' units and the runs' units are told apart by their names (R3).
    let meets = |a: &str, b: &str| a.strip_prefix(b).is_some_and(|r| r.starts_with('-'));
    let clash =
        page_prefix == prefix || meets(&page_prefix, &prefix) || meets(&prefix, &page_prefix);
    if policy.get("writer").is_some() && clash {
        return Err(invalid(
            "work.page_prefix",
            "is the prefix, or one of the two begins with the other and -",
        ));
    }
    Ok(Some(Work {
        slots,
        slice,
        prefix,
        pages,
        page_prefix,
    }))
}
/// The policy file as one JSON object; None when the host names none or it does not exist (R2).
pub(crate) fn policy(s: &impl System) -> Result<Option<Value>, Failure> {
    let Some(file) = s.policy_file() else {
        return Ok(None);
    };
    let Some(path) = file.to_str() else {
        return Err(invalid("file", "path is not valid UTF-8"));
    };
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("file", "is unreadable")),
    };
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("file", "is not JSON"))?;
    match v.is_object() {
        true => Ok(Some(v)),
        false => Err(invalid("file", "is not one JSON object")),
    }
}
/// A variable of this process, when set and not empty.
pub(crate) fn variable(s: &impl System, name: &str) -> Option<String> {
    let env = s.environment().into_iter();
    env.filter(|(k, v)| k == name && !v.is_empty())
        .map(|(_, v)| v)
        .next()
}

/// `--store <absolute path>`, or nothing (R1).
pub fn arguments(argv: &[OsString]) -> Result<Option<String>, Failure> {
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    match words.as_deref() {
        Some([]) => Ok(None),
        Some(["--store", p]) if p.starts_with('/') => Ok(Some((*p).to_owned())),
        _ => Err(error("usage", "args")),
    }
}
/// A failure of a pass, with the starts it made before it.
#[derive(Debug)]
pub struct Refusal {
    pub failure: Failure,
    pub status: u8,
    /// None before the lock was taken.
    pub started: Option<Vec<String>>,
}
fn before(f: Failure) -> Box<Refusal> {
    let status = match f.code {
        "usage" | "role_in_environment" | "invalid_input" | "policy_invalid" => 2,
        _ => 3,
    };
    Box::new(Refusal {
        failure: f,
        status,
        started: None,
    })
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = arguments(argv)
        .map_err(before)
        .and_then(|store| execute(&mut Host, store.as_deref()));
    let (written, status) = match result {
        Ok(v) => (writeln!(std::io::stdout(), "{v}"), 0),
        Err(e) => {
            let f = &e.failure;
            let mut v = json!({"code": f.code, "field": f.field, "detail": f.detail});
            if let Some(started) = &e.started {
                v["started"] = json!(started);
            }
            (
                writeln!(std::io::stderr(), "{}", json!({ "error": v })),
                e.status,
            )
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}

/// With `--store` the policy names the scratch units directory, slice and prefix, and with a writer
/// the prefix of the writers' units and the pages directory (R3).
pub(crate) fn scratch(policy: &Value, store: Option<&str>) -> Result<(), Failure> {
    let named = |k: &str| policy["work"].get(k).is_some();
    let scratch = policy["units"].get("directory").is_some() && named("slice") && named("prefix");
    let pages = policy.get("writer").is_none()
        || (named("page_prefix") && policy["writer"].get("directory").is_some());
    if store.is_some() && !(scratch && pages) {
        return Err(error("invalid_input", "policy"));
    }
    Ok(())
}

/// One pass (R1 to R19): the answer, or the refusal.
pub fn execute(s: &mut impl System, store: Option<&str>) -> Result<Value, Box<Refusal>> {
    if object::lead_in_environment(s) {
        return Err(before(error("role_in_environment", "ONT_ROLE")));
    }
    match (store.is_some(), variable(s, "AP_POLICY_FILE").is_some()) {
        (false, true) => return Err(before(error("invalid_input", "store"))),
        (true, false) => return Err(before(error("invalid_input", "policy"))),
        _ => {}
    }
    let Some(policy) = policy(s).map_err(before)? else {
        return Ok(json!({"skipped": "no policy file"}));
    };
    let Some(w) = work(&policy).map_err(before)? else {
        return Ok(json!({"skipped": "no work"}));
    };
    // A writer of a wrong form refuses the pass before the lock, whatever `pages` (R24).
    let writer = page_write::writer(&policy).map_err(before)?;
    scratch(&policy, store).map_err(before)?;
    // A routing map of a wrong form refuses the pass before the lock (R27).
    let routing = routing::routing(&policy).map_err(before)?;
    let (directory, _) = drive::units(&policy).map_err(before)?;
    let cpu = quota::config(s).map_err(before)?;
    if !s.units() {
        return Ok(json!({"skipped": "no service manager"}));
    }
    let Some((root, _lock)) = pass_lock(s, &directory).map_err(before)? else {
        return Ok(json!({"skipped": "pass_running"}));
    };
    let drawn = new_session_id(s).map_err(before)?.replace('-', "");
    // Only with a writer and `pages` above 0 does the pass reach the writers (R24).
    let pages = writer
        .filter(|_| w.pages > 0)
        .map(|wr| PathBuf::from(wr.directory));
    let mut pass = Pass {
        store: store.map(str::to_owned),
        root,
        work: w,
        cpu,
        session: format!("w-{}", &drawn[..16]),
        prefix: format!("wk-{}", &drawn[..16]),
        count: 0,
        started: vec![],
        load: None,
        pages,
        delivery: None,
        routing,
        bars: None,
    };
    pass.run(s).map_err(|f| {
        Box::new(Refusal {
            failure: f,
            status: 3,
            started: Some(pass.started.clone()),
        })
    })
}
/// The lock of the pass, let go by an explicit `LOCK_UN` before its descriptor closes: a child another
/// thread of the process forked holds a copy of the descriptor until its `exec`, and a close alone would
/// leave the lock held through that copy, so the next pass would wait for it.
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
/// The units directory made (mode 0700), then the lock `work.lock` taken within 120 seconds (R4):
/// the resolved directory and the open lock, or None when another pass held it all that time.
fn pass_lock(s: &mut impl System, directory: &str) -> Result<Option<(PathBuf, Held)>, Failure> {
    let io = |what: &str| fail("io_error", "directory", json!(what));
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    dirs.create(directory)
        .map_err(|_| io("the units directory cannot be made"))?;
    let root = std::fs::canonicalize(directory).map_err(|_| io("the units directory"))?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let lock = options
        .open(root.join("work.lock"))
        .map_err(|_| io("the lock"))?;
    let began = s.now();
    loop {
        match rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(Some((root, Held(lock)))),
            Err(e) if e == rustix::io::Errno::WOULDBLOCK => {}
            Err(_) => return Err(io("the lock")),
        }
        if s.now().duration_since(began) >= LOCK_WAIT {
            return Ok(None);
        }
        s.pause(LOCK_STEP);
    }
}

/// The live run units of the service manager: each one's name, and its key when the rest of the name
/// has a key's form (R5, R6).
fn live_units(s: &mut impl System, prefix: &str) -> Result<Vec<(String, Option<String>)>, Failure> {
    let pattern = format!("{prefix}-*");
    let words = ["list-units", "--all", "--plain", "--no-legend", &pattern];
    let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let reply = s.call("systemctl", &words, &Value::Null, 30_000);
    let reply = reply
        .ok()
        .filter(|r| r.success)
        .ok_or_else(|| error("reading_failed", "list_units"))?;
    let text =
        String::from_utf8(reply.stdout).map_err(|_| error("reading_failed", "list_units"))?;
    let mut live = vec![];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let columns: Vec<&str> = line.split_whitespace().take(3).collect();
        let [name, _, active] = columns[..] else {
            return Err(fail("reading_failed", "list_units", json!(line)));
        };
        let rest = name
            .strip_prefix(prefix)
            .and_then(|r| r.strip_prefix('-'))
            .and_then(|r| r.strip_suffix(".service"));
        if let Some(rest) = rest.filter(|_| LIVE.contains(&active)) {
            let key = key_form(rest).then(|| rest.to_owned());
            live.push((name.to_owned(), key));
        }
    }
    Ok(live)
}
/// Whether another process holds the lock of the unit `key` (of a checked form): the file, when it
/// exists, opened without create and without following a link, and one shared attempt that does not
/// wait, released at once (R6).
fn lock_held(root: &Path, key: &str) -> bool {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let Ok(file) = options.open(root.join(key).join("lock")) else {
        return false;
    };
    let shared = rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockShared);
    match shared {
        Ok(()) => {
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
            false
        }
        Err(e) => e == rustix::io::Errno::WOULDBLOCK,
    }
}
/// Whether the file `started` of the unit `key` (of a checked form) holds exactly `revision` (R9).
fn started_at(root: &Path, key: &str, revision: &str) -> bool {
    let path = root.join(key).join("started");
    let is_file = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file());
    is_file && std::fs::read(&path).is_ok_and(|b| b == revision.as_bytes())
}
/// What `observe` reads of given units.
#[derive(Debug, Default, PartialEq)]
pub struct Observed {
    /// The keys of the given units that are in flight.
    pub in_flight: Vec<String>,
    /// The keys of the given units whose file `started` equals their revision.
    pub started: Vec<String>,
    /// The names of the live run units whose keys are not among the given units.
    pub others: Vec<String>,
}
/// The reading `ap status --wait` needs (R20): of the units given as (key, revision), which are in
/// flight and which have a file `started` equal to their revision, under the units directory `root`
/// and the run units of `prefix`. It writes nothing and takes no `work.lock`.
pub fn observe(
    s: &mut impl System,
    root: &Path,
    prefix: &str,
    units: &[(String, String)],
) -> Result<Observed, Failure> {
    let live = live_units(s, prefix)?;
    let mut seen = Observed::default();
    for (key, revision) in units.iter().filter(|(k, _)| key_form(k)) {
        let running = live.iter().any(|(_, k)| k.as_deref() == Some(key.as_str()));
        if running || lock_held(root, key) {
            seen.in_flight.push(key.clone());
        }
        if started_at(root, key, revision) {
            seen.started.push(key.clone());
        }
    }
    let given = |k: &Option<String>| units.iter().any(|(u, _)| k.as_deref() == Some(u.as_str()));
    seen.others = live
        .into_iter()
        .filter(|(_, k)| !given(k))
        .map(|(n, _)| n)
        .collect();
    Ok(seen)
}

/// Two write-set patterns overlap (R11): the literal beginning of one, up to its first `*`, is a
/// prefix of the other, compared as path components.
pub fn overlap(a: &str, b: &str) -> bool {
    let within = |p: &str, other: &str| match p.split_once('*') {
        // The component a `*` cuts is a prefix of the other's component; whole components before it.
        Some((literal, _)) => other.starts_with(literal),
        None => other == p || other.strip_prefix(p).is_some_and(|r| r.starts_with('/')),
    };
    within(a, b) || within(b, a)
}
/// The first `max` bytes of `t`, cut back to a character boundary.
fn cut_to(t: &str, max: usize) -> String {
    let mut end = t.len().min(max);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_owned()
}
fn strings(v: &Value) -> Vec<String> {
    let all = v.as_array().into_iter().flatten();
    all.filter_map(Value::as_str).map(str::to_owned).collect()
}

/// One unit as the pass read it.
struct Read {
    key: String,
    f: Value,
    in_flight: bool,
}
/// One pass with the lock held.
struct Pass {
    store: Option<String>,
    root: PathBuf,
    work: Work,
    cpu: Option<quota::Config>,
    session: String,
    prefix: String,
    count: u32,
    started: Vec<String>,
    /// The one reading of the load: the busy share and `outside` (R13).
    load: Option<(i64, i64, i64)>,
    /// The pages directory, in a pass that reaches the writers (R24).
    pages: Option<PathBuf>,
    /// The delivery's files, read once by the first candidate with `depends_delivered` (R26).
    delivery: Option<status::DeliveryFiles>,
    /// The policy's routing map, and the live bars read once by the first candidate it tests (R27).
    routing: Option<routing::Routing>,
    bars: Option<BTreeSet<String>>,
}
/// A candidate's end in this pass.
enum End {
    Started,
    Waits(String),
    /// A wait after this pass cleared the stored `waits_on` (a start that failed): the store now
    /// holds the empty string.
    WaitsCleared(String),
    /// Nothing to note: a unit that moved meanwhile.
    Skipped,
}
impl Pass {
    fn store(&self) -> Option<&str> {
        self.store.as_deref()
    }
    /// A new request id `wk-<16 hex>-<counter>` (R17).
    fn rid(&mut self) -> String {
        self.count += 1;
        format!("{}-{}", self.prefix, self.count)
    }
    fn body(&mut self, action: &str, p: Value) -> Value {
        let rid = self.rid();
        let actor = Actor {
            agent: "ap-work",
            session: &self.session,
            role: "dispatch",
        };
        store::body(&actor, action, rid, p)
    }
    fn apply(
        &mut self,
        s: &mut impl System,
        action: &str,
        p: Value,
    ) -> Result<store::Applied, Failure> {
        let rid = self.rid();
        let actor = Actor {
            agent: "ap-work",
            session: &self.session,
            role: "dispatch",
        };
        store::unit_action(s, self.store.as_deref(), &actor, action, rid, p)
    }
    /// The codes an `ont action validate` failed; empty when it is valid.
    fn validate(
        &mut self,
        s: &mut impl System,
        action: &str,
        p: Value,
    ) -> Result<Vec<String>, Failure> {
        let body = self.body(action, p);
        let v = store::at(store::validate(s, self.store(), &body), action)?;
        if v["result"]["valid"] == true {
            return Ok(vec![]);
        }
        let criteria = v["result"]["criteria"].as_array().into_iter().flatten();
        let failed: Vec<String> = criteria
            .filter(|c| c["passed"] == false)
            .filter_map(|c| c["code"].as_str().map(str::to_owned))
            .collect();
        match failed.is_empty() {
            true => Err(fail("store_answer_unreadable", action, Value::Null)),
            false => Ok(failed),
        }
    }
    /// `note_unit_wait`: the unit's revision after it, or None when the unit is no longer `ready`
    /// (R14, R16).
    fn note(
        &mut self,
        s: &mut impl System,
        key: &str,
        waits_on: &str,
    ) -> Result<Option<String>, Failure> {
        let p = json!({"unit": id("unit", key), "waits_on": waits_on});
        let applied = match self.apply(s, "note_unit_wait", p) {
            Err(f) if store::code(&f) == "unit_not_ready" => return Ok(None),
            other => store::at(other, "note_unit_wait")?,
        };
        let edit = applied
            .edits
            .iter()
            .find(|e| e["id"]["object_type"] == "unit" && e["id"]["native_key"] == key);
        let revision = edit.and_then(|e| e["revision"].as_str());
        let revision =
            revision.ok_or_else(|| error("store_answer_unreadable", "note_unit_wait"))?;
        Ok(Some(revision.to_owned()))
    }

    fn run(&mut self, s: &mut impl System) -> Result<Value, Failure> {
        let all = vec![json!({"in": ["state", READ]})];
        let found = store::at(store::search(s, self.store(), "unit", all), "units")?;
        let live = live_units(s, &self.work.prefix)?;
        let mut units: Vec<Read> = vec![];
        for (key, f) in found.into_iter().filter(|(k, _)| key_form(k)) {
            let running = live.iter().any(|(_, k)| k.as_deref() == Some(key.as_str()));
            let in_flight = running || lock_held(&self.root, &key);
            units.push(Read { key, f, in_flight });
        }
        let read: BTreeSet<&str> = units.iter().map(|u| u.key.as_str()).collect();
        let others: Vec<String> = live
            .iter()
            .filter(|(_, k)| !k.as_deref().is_some_and(|k| read.contains(k)))
            .map(|(n, _)| n.clone())
            .collect();
        let mut in_flight: Vec<String> = units
            .iter()
            .filter(|u| u.in_flight)
            .map(|u| u.key.clone())
            .collect();
        let flying = in_flight.len() + others.len();
        in_flight.extend(others);
        let cut: Vec<String> = units
            .iter()
            .filter(|u| !u.in_flight && u.f["state"] == "running")
            .map(|u| u.key.clone())
            .collect();
        let state = |u: &&Read, st: &str| u.f["state"] == st && !u.in_flight;
        let mut accepted: Vec<&Read> = units
            .iter()
            .filter(|u| state(u, "accepted") && merge_only(&u.f))
            .collect();
        accepted.sort_by(|a, b| a.key.cmp(&b.key));
        let mut ready: Vec<&Read> = units.iter().filter(|u| state(u, "ready")).collect();
        ready.sort_by_key(|u| {
            (
                u.f["ready_at"].as_str().unwrap_or_default().to_owned(),
                u.key.clone(),
            )
        });
        let (mut not_restarted, mut waiting) = (vec![], vec![]);
        for u in accepted.into_iter().chain(ready) {
            let Some((revision, f)) =
                store::at(store::head(s, self.store(), "unit", &u.key), "unit")?
            else {
                continue;
            };
            let is_ready = f["state"] == "ready";
            let merge = f["state"] == "accepted" && merge_only(&f);
            if !is_ready && !merge {
                continue;
            }
            if started_at(&self.root, &u.key, &revision) {
                not_restarted.push(u.key.clone());
                continue;
            }
            let end = match self.constraints(s, &u.key, &f, &units, flying)? {
                Some(why) => End::Waits(why),
                None => self.start(s, &u.key, &f, revision)?,
            };
            let end_cleared = matches!(end, End::WaitsCleared(_));
            match end {
                End::Started | End::Skipped => {}
                End::Waits(why) | End::WaitsCleared(why) if is_ready => {
                    let why = cut_to(&why, WAITS_ON);
                    let stored = match end_cleared {
                        true => "",
                        false => f["waits_on"].as_str().unwrap_or_default(),
                    };
                    if stored != why {
                        self.note(s, &u.key, &why)?;
                    }
                    waiting.push(json!({"unit": u.key, "waits_on": why}));
                }
                End::Waits(why) | End::WaitsCleared(why) => {
                    waiting.push(json!({"unit": u.key, "waits_on": cut_to(&why, WAITS_ON)}))
                }
            }
        }
        let (busy, outside) = match self.load {
            Some((busy, _, outside)) => (json!(busy), json!(outside)),
            None => (Value::Null, Value::Null),
        };
        let mut answer = json!({"started": self.started, "cut": cut, "not_restarted": not_restarted,
            "waiting": waiting, "in_flight": in_flight, "busy": busy, "outside": outside});
        if let Some(pages) = self.pages.clone() {
            answer["pages"] = self.writers(s, &pages)?;
        }
        Ok(answer)
    }

    /// The writers (R24): every draft unit of a horizon that has no page and a write set, by
    /// `added_at`, then key, started up to `pages` in flight; the answer's member `pages` (R25).
    fn writers(&mut self, s: &mut impl System, pages: &Path) -> Result<Value, Failure> {
        let all = vec![json!({"in": ["state", ["draft"]]})];
        let drafts = store::at(store::search(s, self.store(), "unit", all), "units")?;
        let mut intents: Vec<String> = vec![];
        for (key, f) in &drafts {
            let intent = f["intent"]["native_key"].as_str();
            if let Some(i) = intent.filter(|_| f["lane"] == "objective" && key_form(key))
                && !intents.iter().any(|k| k == i)
            {
                intents.push(i.to_owned());
            }
        }
        let mut candidates: Vec<(String, String)> = vec![];
        for intent in &intents {
            for h in horizon_of(s, self.store(), intent, &drafts)? {
                let f = drafts.iter().find(|(k, _)| *k == h.key).map(|(_, f)| f);
                let written =
                    f.is_some_and(|f| f["write_set"].as_array().is_some_and(|w| !w.is_empty()));
                let objective = f.is_some_and(|f| f["lane"] == "objective");
                if h.page == "none" && written && objective && key_form(&h.key) {
                    candidates.push((h.added_at, h.key));
                }
            }
        }
        candidates.sort();
        candidates.dedup();
        let live = live_units(s, &self.work.page_prefix)?;
        let named = |k: &str| candidates.iter().any(|(_, c)| c == k);
        let others: Vec<String> = live
            .iter()
            .filter(|(_, k)| !k.as_deref().is_some_and(named))
            .map(|(n, _)| n.clone())
            .collect();
        let mut in_flight: Vec<String> = vec![];
        for (_, key) in &candidates {
            let running = live.iter().any(|(_, k)| k.as_deref() == Some(key.as_str()));
            if running || lock_held(pages, key) {
                in_flight.push(key.clone());
            }
        }
        let flying = in_flight.len() + others.len();
        let (mut started, mut not_restarted, mut waiting) = (vec![], vec![], vec![]);
        for (_, key) in &candidates {
            if in_flight.contains(key) {
                continue;
            }
            let home = pages.join(key);
            if home.join("started").symlink_metadata().is_ok() {
                not_restarted.push(key.clone());
                continue;
            }
            let n = flying + started.len();
            if n >= self.work.pages {
                waiting.push(
                    json!({"unit": key, "waits_on": format!("pages {n}/{}", self.work.pages)}),
                );
                continue;
            }
            let mut dirs = std::fs::DirBuilder::new();
            dirs.recursive(true).mode(0o700);
            let made =
                dirs.create(&home).is_ok() && home.symlink_metadata().is_ok_and(|m| m.is_dir());
            if !made {
                return Err(fail("io_error", "started", json!(key)));
            }
            let path = home.join("started");
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true).mode(0o600);
            options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
            options
                .open(&path)
                .map_err(|_| fail("io_error", "started", json!(key)))?;
            let argv = self.command(s, key, true);
            let called = s.call("systemd-run", &argv, &Value::Null, 60_000);
            if called.is_ok_and(|r| r.success) {
                started.push(key.clone());
            } else {
                let _ = std::fs::remove_file(&path);
                waiting.push(json!({"unit": key, "waits_on": "start failed"}));
            }
        }
        in_flight.extend(others);
        Ok(
            json!({"started": started, "not_restarted": not_restarted, "waiting": waiting,
            "in_flight": in_flight}),
        )
    }

    /// The first constraint that fails, as its `waits_on`; None when the unit may start. A `ready` unit
    /// meets R10 to R13; a merge candidate meets R10 only (R22): once the validate of `merge_unit`
    /// passes it starts, with no count against `slots` and no reading of the load.
    /// `flying` is the count of the units in flight before this pass's starts.
    fn constraints(
        &mut self,
        s: &mut impl System,
        key: &str,
        f: &Value,
        units: &[Read],
        flying: usize,
    ) -> Result<Option<String>, Failure> {
        let is_ready = f["state"] == "ready";
        let Some(repository) = f["repository"].as_str().filter(|r| !r.is_empty()) else {
            return Ok(Some("no repository".into()));
        };
        if let Err(refused) = drive::checked(s, key, f) {
            return Ok(Some(format!("run refused: {}", refused.code)));
        }
        let mut p = json!({"unit": id("unit", key)});
        if f["lane"] == "objective" {
            p["intent"] = f["intent"].clone();
        }
        if is_ready {
            let failed = self.validate(s, "start_unit", p)?;
            if failed.iter().any(|c| c == "dependency_not_merged") {
                for d in f["depends_on"].as_array().into_iter().flatten() {
                    let Some(d) = d["native_key"].as_str() else {
                        continue;
                    };
                    let dep = store::at(store::get(s, self.store(), "unit", d), "depends_on")?;
                    if dep.is_some_and(|dep| dep["state"] == "dropped") {
                        return Ok(Some(format!("dependency dropped: {d}")));
                    }
                }
            }
            if !failed.is_empty() {
                return Ok(Some(failed.join(", ")));
            }
            // R26: right after the validate of `start_unit`, the delivered dependencies.
            if let Some(why) = self.undelivered(s, f)? {
                return Ok(Some(why));
            }
            // R27: right after the delivered dependencies, the routing map's tests.
            if let Some(why) = self.routed(s, f)? {
                return Ok(Some(why));
            }
        } else {
            p["head"] = f["head"].clone();
            p["merged_commit"] = json!("0".repeat(40));
            let failed = self.validate(s, "merge_unit", p)?;
            if !failed.is_empty() {
                return Ok(Some(failed.join(", ")));
            }
            // R22: nothing after the validate of `merge_unit` holds a merge candidate.
            return Ok(None);
        }
        let mine = strings(&f["write_set"]);
        let others = units.iter().filter(|o| {
            let busy = o.in_flight
                || self.started.contains(&o.key)
                || o.f["state"] == "running"
                || o.f["state"] == "accepted";
            o.key != key && busy && o.f["repository"] == repository
        });
        for o in others {
            let theirs = strings(&o.f["write_set"]);
            if mine.iter().any(|a| theirs.iter().any(|b| overlap(a, b))) {
                return Ok(Some(format!("write set overlaps {}", o.key)));
            }
        }
        let n = flying + self.started.len();
        if n >= self.work.slots {
            return Ok(Some(format!("slots {n}/{}", self.work.slots)));
        }
        let Some(c) = &self.cpu else {
            return Ok(None);
        };
        let (limit, floor, slice) = (c.busy_limit, c.floor, c.slice.clone());
        if self.load.is_none() {
            let l = quota::load(s, &slice)?;
            self.load = Some((l.busy, l.cpus, l.outside));
        }
        let (busy, cpus, outside) = self.load.unwrap_or_default();
        if busy > limit {
            return Ok(Some(format!("cpu over {limit}%")));
        }
        if limit * cpus - outside < floor {
            return Ok(Some("cpu outside".into()));
        }
        Ok(None)
    }

    /// The routing map's tests of a `ready` candidate (R27), as its `waits_on`: nothing without the member
    /// `routing` or for a unit without a kind and both tiers; the live bars come from one search a pass.
    fn routed(&mut self, s: &mut impl System, f: &Value) -> Result<Option<String>, Failure> {
        let Some(r) = self.routing.clone() else {
            return Ok(None);
        };
        if !routing::checked(f) {
            return Ok(None);
        }
        if self.bars.is_none() {
            let r = self.store.clone();
            self.bars = Some(routing::live_bars(s, r.as_deref())?);
        }
        let live = self.bars.clone().unwrap_or_default();
        Ok(routing::judge(&r, &live, f)
            .err()
            .map(|e| routing::wait_text(&e)))
    }

    /// The first unit of `depends_delivered` that is not delivered, as its `waits_on` (R26). The
    /// delivery's files are read and Git runs only for a candidate that names one.
    fn undelivered(&mut self, s: &mut impl System, f: &Value) -> Result<Option<String>, Failure> {
        let all = f["depends_delivered"].as_array().into_iter().flatten();
        let keys: Vec<&str> = all.filter_map(|d| d["native_key"].as_str()).collect();
        if keys.is_empty() {
            return Ok(None);
        }
        let files = match self.delivery.take() {
            Some(d) => d,
            None => status::delivery_files(s),
        };
        let mut why = None;
        for key in keys {
            let dep = store::get(s, self.store(), "unit", key);
            let dep = match store::at(dep, "depends_delivered") {
                Ok(dep) => dep,
                Err(e) => {
                    self.delivery = Some(files);
                    return Err(e);
                }
            };
            if dep.as_ref().is_some_and(|d| d["state"] == "dropped") {
                why = Some(format!("dependency dropped: {key}"));
                break;
            }
            if !dep.is_some_and(|d| delivered(s, &files, &d)) {
                why = Some(format!("not delivered: {key}"));
                break;
            }
        }
        self.delivery = Some(files);
        Ok(why)
    }

    /// The start of one unit (R14, R15).
    fn start(
        &mut self,
        s: &mut impl System,
        key: &str,
        f: &Value,
        revision: String,
    ) -> Result<End, Failure> {
        let (mut revision, mut cleared) = (revision, false);
        let stored = f["waits_on"].as_str().unwrap_or_default();
        if f["state"] == "ready" && !stored.is_empty() {
            match self.note(s, key, "")? {
                Some(r) => (revision, cleared) = (r, true),
                None => return Ok(End::Skipped),
            }
        }
        let home = self.root.join(key);
        let mut dirs = std::fs::DirBuilder::new();
        dirs.mode(0o700);
        match dirs.create(&home) {
            Err(e) if e.kind() != ErrorKind::AlreadyExists => {
                return Err(fail("io_error", "started", json!(key)));
            }
            _ if !std::fs::symlink_metadata(&home).is_ok_and(|m| m.is_dir()) => {
                return Err(fail("io_error", "started", json!(key)));
            }
            _ => {}
        }
        let path = home.join("started");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true).mode(0o600);
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        let written = options
            .open(&path)
            .and_then(|mut file| file.write_all(revision.as_bytes()));
        written.map_err(|_| fail("io_error", "started", json!(key)))?;
        let argv = self.command(s, key, false);
        let called = s.call("systemd-run", &argv, &Value::Null, 60_000);
        if called.is_ok_and(|r| r.success) {
            self.started.push(key.to_owned());
            return Ok(End::Started);
        }
        // No start: the file this pass wrote goes, and the next pass tries again.
        let _ = std::fs::remove_file(&path);
        let why = "start failed".to_owned();
        Ok(if cleared {
            End::WaitsCleared(why)
        } else {
            End::Waits(why)
        })
    }
    /// The arguments of `systemd-run` for the unit `key` (R14): its run, or with `writer` its page's
    /// writer (R24).
    fn command(&self, s: &impl System, key: &str, writer: bool) -> Vec<String> {
        let prefix = match writer {
            true => &self.work.page_prefix,
            false => &self.work.prefix,
        };
        let unit = format!("{prefix}-{key}");
        let mut argv: Vec<String> = [
            "--unit",
            &unit,
            "--slice",
            &self.work.slice,
            "--collect",
            "--quiet",
            "--expand-environment=no",
        ]
        .iter()
        .map(|w| (*w).to_owned())
        .collect();
        for (k, v) in s.environment() {
            if KEEP.contains(&k.as_str()) || (k == "AP_POLICY_FILE" && !v.is_empty()) {
                argv.push("--setenv".into());
                argv.push(format!("{k}={v}"));
            }
        }
        // A run's function calls claim `dispatch`, whatever this process's own environment holds
        // (`specs/ap/roles.md` R3).
        argv.extend(["--setenv".into(), "ONT_ROLE=dispatch".into()]);
        argv.extend([
            "--".into(),
            this_ap(),
            "unit".into(),
            if writer { "write" } else { "run" }.into(),
            key.to_owned(),
        ]);
        if let Some(store) = self.store() {
            argv.extend(["--store".into(), store.to_owned()]);
        }
        argv
    }
}
/// An `accepted` unit that waits only for its merge run: class `plain`, or approved at its head (R8).
/// Whether the unit `d` of a `depends_delivered` is delivered (R26). In a repository the request of
/// delivery covers (its `repository`, or `etc.repository`): `merged` or `done`, the newest record
/// ended `current` or `installed` with the target of `ap` (and for the `etc` repository its member
/// `etc` ended `same` or `installed`), and the merged commit an ancestor of or equal to that record's
/// tip in delivery's own clone. Elsewhere its merge releases it, and without a repository its state
/// `done`. A request that cannot be read, and an ancestry that cannot be read, is not delivered.
fn delivered(s: &mut impl System, files: &status::DeliveryFiles, d: &Value) -> bool {
    let merged = d["state"] == "merged" || d["state"] == "done";
    let Some(repository) = d["repository"].as_str().filter(|r| !r.is_empty()) else {
        return d["state"] == "done";
    };
    let Some(request) = &files.request else {
        return false;
    };
    // An old name of the regroup reads as its new one, in the request and in the unit (R26).
    let named = |v: &Value| {
        v.as_str()
            .is_some_and(|r| object::same_repository(r, repository))
    };
    let main = named(&request["repository"]);
    let etc = !main && named(&request["etc"]["repository"]);
    if !main && !etc {
        return merged;
    }
    let Some(record) = files.record.as_ref().filter(|_| merged) else {
        return false;
    };
    let ended = matches!(record["end"].as_str(), Some("current" | "installed"));
    if !ended || record["target"] != status::AP_TARGET {
        return false;
    }
    let tip = match etc {
        false => record["tip"].as_str(),
        true => {
            let e = &record["etc"];
            let done = e.is_object() && matches!(e["result"].as_str(), Some("same" | "installed"));
            e["tip"].as_str().filter(|_| done)
        }
    };
    let hex = |t: &&str| t.len() == 40 && t.bytes().all(|c| c.is_ascii_hexdigit());
    let (Some(tip), Some(commit)) = (tip.filter(hex), d["merged_commit"].as_str().filter(hex))
    else {
        return false;
    };
    let name = repository.split_once('/').map(|(_, n)| n);
    let name = name.filter(|n| !n.is_empty() && !n.contains('/') && *n != "." && *n != "..");
    let Some(name) = name else {
        return false;
    };
    let clone = format!("{}/{name}", files.state);
    let words = ["merge-base", "--is-ancestor", commit, tip];
    crate::review::git(s, &clone, &words).is_ok()
}
fn merge_only(f: &Value) -> bool {
    f["class"] == "plain" || (!f["head"].is_null() && f["approved_head"] == f["head"])
}
/// This program's path: `/proc/self/exe` as a link, a trailing ` (deleted)` cut (delivery replaces
/// the binary by a rename).
fn this_ap() -> String {
    let path = std::fs::read_link("/proc/self/exe").unwrap_or_default();
    let path = path.to_string_lossy();
    path.strip_suffix(" (deleted)").unwrap_or(&path).to_owned()
}

/// One unit of a horizon (R23).
pub struct Horizon {
    pub key: String,
    pub class: Value,
    pub page: &'static str,
    added_at: String,
}
/// Whether the unit `key`, read as `u`, has a review of kind `page`, `closed` with the verdict
/// `accept`, whose `head` is its page digest as read (R23).
fn page_accepted(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    u: &Value,
) -> Result<bool, Failure> {
    let digest = render::page_digest(key, u);
    let keys = store::at(
        store::view_keys(s, store, "reviews_of_unit", json!({ "unit": key })),
        "reviews",
    )?;
    for k in keys {
        let Some(v) = store::at(store::get(s, store, "review", &k), "reviews")? else {
            continue;
        };
        if v["kind"] == "page"
            && v["status"] == "closed"
            && v["verdict"] == "accept"
            && v["head"] == digest
        {
            return Ok(true);
        }
    }
    Ok(false)
}
/// The horizon of the intent `intent` among the draft units `drafts` (R23): the drafts of that intent
/// whose every dependency counts, by `added_at`, then key, each with the state of its page.
pub fn horizon_of(
    s: &mut impl System,
    store: Option<&str>,
    intent: &str,
    drafts: &[store::Found],
) -> Result<Vec<Horizon>, Failure> {
    let mut found = vec![];
    for (key, f) in drafts {
        if f["state"] != "draft" || f["intent"]["native_key"] != intent {
            continue;
        }
        let mut counts = true;
        for d in f["depends_on"].as_array().into_iter().flatten() {
            let Some(d) = d["native_key"].as_str() else {
                counts = false;
                break;
            };
            let dep = store::at(store::get(s, store, "unit", d), "depends_on")?;
            let ok = match dep {
                None => false,
                Some(dep) => match dep["state"].as_str().unwrap_or_default() {
                    "merged" | "done" | "ready" | "running" | "accepted" | "stopped" => true,
                    "draft" => page_accepted(s, store, d, &dep)?,
                    _ => false,
                },
            };
            if !ok {
                counts = false;
                break;
            }
        }
        if !counts {
            continue;
        }
        let page = if page_accepted(s, store, key, f)? {
            "accepted"
        } else if f["acceptance"].as_array().is_some_and(|a| !a.is_empty()) {
            "written"
        } else {
            "none"
        };
        found.push(Horizon {
            key: key.clone(),
            class: f["class"].clone(),
            page,
            added_at: f["added_at"].as_str().unwrap_or_default().to_owned(),
        });
    }
    found.sort_by(|a, b| (&a.added_at, &a.key).cmp(&(&b.added_at, &b.key)));
    Ok(found)
}
/// The function `horizon` (R23): `{"intent": <key>}` against the store `store`; the answer, or the
/// refusal with its exit (2 a refusal, 3 a read that failed).
pub fn horizon(s: &mut impl System, store: &str, p: &Value) -> Result<Value, (Failure, u8)> {
    let refuse = |f: Failure| (f, 2);
    let failed = |f: Failure| (f, 3);
    let one = p.as_object().is_some_and(|o| o.len() == 1);
    let key = p["intent"]
        .as_str()
        .filter(|k| one && (1..=512).contains(&k.len()));
    let key = key.ok_or_else(|| refuse(error("invalid_input", "intent")))?;
    let read = store::at(store::get(s, Some(store), "intent", key), "intent").map_err(failed)?;
    if read.is_none() {
        return Err(refuse(error("unknown_intent", "intent")));
    }
    let all = vec![json!({"in": ["state", ["draft"]]})];
    let drafts = store::at(store::search(s, Some(store), "unit", all), "units").map_err(failed)?;
    let units = horizon_of(s, Some(store), key, &drafts).map_err(failed)?;
    let units: Vec<Value> = units
        .iter()
        .map(|h| json!({"unit": h.key, "class": h.class, "page": h.page}))
        .collect();
    Ok(json!({"intent": key, "units": units}))
}
/// `ap` with no argument and `ONT_FN_NAME=horizon` (R23): the parameters on standard input, the store
/// at `ONT_FN_STORE`; the answer on standard output, or the error object on standard error.
pub fn function() -> ExitCode {
    let result = (|| {
        let mut bytes = vec![];
        let read = std::io::stdin().take(LIMIT + 1).read_to_end(&mut bytes);
        if read.is_err() || bytes.len() as u64 > LIMIT {
            return Err((error("invalid_input", "stdin"), 2));
        }
        let p: Value =
            serde_json::from_slice(&bytes).map_err(|_| (error("invalid_input", "stdin"), 2))?;
        let store = std::env::var("ONT_FN_STORE")
            .ok()
            .filter(|v| v.starts_with('/'));
        let store = store.ok_or_else(|| (error("invalid_input", "ONT_FN_STORE"), 2))?;
        horizon(&mut Host, &store, &p)
    })();
    let (written, status) = match result {
        Ok(v) => (writeln!(std::io::stdout(), "{v}"), 0),
        Err((f, status)) => {
            let v =
                json!({"error": {"code": store::code(&f), "field": f.field, "detail": f.detail}});
            (writeln!(std::io::stderr(), "{v}"), status)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
