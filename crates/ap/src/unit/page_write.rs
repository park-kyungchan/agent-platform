//! `ap unit write <key>`: the writer of a unit's page. One session of `ap run` (role `writer`, under
//! `limits` with its scratch directory alone) is given the page as the Lead added it, the pages of its
//! dependencies and the repository at `origin/main`; its one output is an amendment, refused when it
//! names a member the Lead keeps or a path outside the write set the call read, applied by the code of
//! `ap unit amend`, then looked at by the code of `ap unit review`. A refusal or a repair goes back to
//! the same session, at most three launches a call (`specs/ap/unit-write.md`).
use super::drive::{key_form, racing};
use super::object::{self, Call, clone_of};
use super::page_review::{self, stage_unit, unit_ended};
use super::render;
use crate::host::{Failure, Host, System, digest, error, fail, id, new_session_id};
use crate::review::{Reading, write_set_pattern};
use crate::store;
use serde_json::{Map, Value, json};
use std::{
    ffi::OsString,
    fs::File,
    io::{ErrorKind, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

/// The frame of the writer, compiled in (R13).
const FRAME: &str = include_str!("frames/writer.md");
/// The defaults of the policy's member `writer` (R2).
const DIRECTORY: &str = "/var/lib/agent-platform/pages";
const TIMEOUT: u64 = 5_400;
/// The members an amendment may hold (R7): every other one is the Lead's.
pub const MEMBERS: [&str; 12] = [
    "objective",
    "interface",
    "invariants",
    "acceptance",
    "not_built",
    "write_set",
    "read_first",
    "specs",
    "verifier_questions",
    "implementer",
    "verifier",
    "estimate_lines",
];
/// The most launches of one call (R10).
const LAUNCHES: usize = 3;
/// The largest text of a first launch, in bytes (R5).
const PROMPT_MAX: usize = 1_000_000;
const FETCH_MS: u64 = 120_000;
const FETCH_TRIES: u32 = 5;
/// The pause before a fetch or a look is tried again (R5, R9).
const AGAIN: Duration = Duration::from_secs(2);
/// The most repeats of a look refused `fetch_failed` (R9).
const LOOK_REPEATS: u32 = 2;
/// The sentence that goes back to the writer before a refusal or a report (R10).
const BACK: &str = "Your amendment was not taken as it stands: the next line is either the refusal of `ap` as one JSON object or the path of the report of the look at the page it made; read it, then give the whole amendment again as your final message, changed so that it holds.";
/// The codes of `specs/ap/unit-review.md` R23 by which a look refuses the page it was to look at: they
/// go back to the writer (R10).
const PAGE_REFUSED: [&str; 9] = [
    "write_set_missing",
    "acceptance_missing",
    "tiers_missing",
    "write_set_form",
    "plain_writes_protected",
    "acceptance_not_runnable",
    "acceptance_timeout",
    "acceptance_passes_on_base",
    "invariant_fails_on_base",
];

/// The member `writer` of the policy file (R2).
#[derive(Debug, Clone, PartialEq)]
pub struct Writer {
    pub model: String,
    pub effort: String,
    pub permission_mode: String,
    pub compact_window: Option<u64>,
    pub timeout: u64,
    pub directory: String,
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// A path of the schema's `file_path`: `/` then 1 to 399 printable ASCII bytes, no space.
fn file_path(d: &str) -> bool {
    d.starts_with('/')
        && (2..=400).contains(&d.len())
        && d.bytes().all(|c| (b'!'..=b'~').contains(&c))
}
/// The member `writer` of the policy `policy` (R2); None when the policy has none. `ap unit write`,
/// the reader `writer` of `ap policy check` and the dispatch pass call it.
pub fn writer(policy: &Value) -> Result<Option<Writer>, Failure> {
    let Some(w) = policy.get("writer") else {
        return Ok(None);
    };
    let w = w
        .as_object()
        .ok_or_else(|| invalid("writer", "is not an object"))?;
    let known = [
        "model",
        "effort",
        "permission_mode",
        "compact_window",
        "timeout_seconds",
        "directory",
    ];
    if let Some(k) = w.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(invalid(
            "writer",
            &format!("holds the member {k}, which is not read"),
        ));
    }
    let word = |k: &str, max: usize| {
        let v = w.get(k).and_then(Value::as_str);
        v.filter(|v| !v.is_empty() && crate::run::word_ok(v, max))
            .map(str::to_owned)
            .ok_or_else(|| invalid(&format!("writer.{k}"), "is not a word ap run takes"))
    };
    let (model, effort) = (word("model", 40)?, word("effort", 16)?);
    let mode = w.get("permission_mode").and_then(Value::as_str);
    let mode = mode.filter(|m| crate::run::MODES.contains(m));
    let permission_mode = mode
        .ok_or_else(|| invalid("writer.permission_mode", "is not a mode ap run takes"))?
        .to_owned();
    let compact_window = match w.get("compact_window") {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .filter(|n| (100_000..=1_000_000).contains(n))
                .ok_or_else(|| {
                    invalid(
                        "writer.compact_window",
                        "is not an integer from 100000 to 1000000",
                    )
                })?,
        ),
    };
    let timeout = match w.get("timeout_seconds") {
        None => TIMEOUT,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=86_400).contains(n))
            .ok_or_else(|| invalid("writer.timeout_seconds", "is not 1 to 86,400 seconds"))?,
    };
    let directory = match w.get("directory") {
        None => DIRECTORY.to_owned(),
        Some(v) => v
            .as_str()
            .filter(|d| file_path(d))
            .ok_or_else(|| invalid("writer.directory", "is not a file_path of the schema"))?
            .to_owned(),
    };
    Ok(Some(Writer {
        model,
        effort,
        permission_mode,
        compact_window,
        timeout,
        directory,
    }))
}
/// The accounts of the writer's stage: the list of the first key present among `claude/writer`,
/// `claude/<model>` and `claude` of `accounts` (R2).
fn accounts(policy: &Value, model: &str) -> Result<Vec<String>, Failure> {
    let keys = [
        "claude/writer".to_owned(),
        format!("claude/{model}"),
        "claude".to_owned(),
    ];
    let all = &policy["accounts"];
    let key = keys.iter().find(|k| all.get(k.as_str()).is_some());
    let key = key.ok_or_else(|| error("account_not_mapped", "writer"))?;
    let list = all[key.as_str()].as_array().cloned().unwrap_or_default();
    let paths: Vec<String> = list
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if paths.is_empty() || paths.len() > 5 || paths.len() != list.len() {
        return Err(invalid(&format!("accounts.{key}"), "is not 1 to 5 paths"));
    }
    Ok(paths)
}

/// A refusal of the call (R1): `after` is `amend` once an amendment was applied.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<&'static str>,
    pub unit: String,
}
/// What ends a call as a refusal, with the store's code where the store refused.
struct Stop {
    code: String,
    field: String,
    detail: Value,
}
impl From<Failure> for Stop {
    fn from(f: Failure) -> Self {
        Stop {
            code: store::code(&f),
            field: f.field,
            detail: f.detail,
        }
    }
}
/// The arguments after `write`: the key and `--store <absolute path>` (R1).
pub fn arguments(argv: &[OsString]) -> Result<(String, Option<String>), Failure> {
    let usage = || error("usage", "args");
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    match words.ok_or_else(usage)?.as_slice() {
        [key] if *key != "--store" => Ok(((*key).to_owned(), None)),
        [key, "--store", p] if *key != "--store" && p.starts_with('/') => {
            Ok(((*key).to_owned(), Some((*p).to_owned())))
        }
        _ => Err(usage()),
    }
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = match arguments(argv) {
        Ok((key, store)) => execute(&mut Host, &key, store.as_deref()),
        Err(f) => Err(Box::new(Refusal {
            code: f.code.into(),
            field: f.field,
            detail: f.detail,
            after: None,
            unit: String::new(),
        })),
    };
    let (written, status) = match result {
        Ok((v, status)) => (writeln!(std::io::stdout(), "{v}"), status),
        Err(e) => {
            let unit = if e.unit.is_empty() {
                Value::Null
            } else {
                json!(e.unit)
            };
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail,
                "after": e.after, "unit": unit}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}

/// One call (R1 to R12): the answer and its exit (0 `accept`, 3 every other end), or the refusal.
pub fn execute(
    s: &mut impl System,
    key: &str,
    store: Option<&str>,
) -> Result<(Value, u8), Box<Refusal>> {
    let mut amendments = 0;
    let result = call(s, key, store, &mut amendments);
    result.map_err(|e| {
        Box::new(Refusal {
            code: e.code,
            field: e.field,
            detail: e.detail,
            after: (amendments > 0).then_some("amend"),
            unit: key.to_owned(),
        })
    })
}

/// The lock of the call on `<home>/lock`, let go by an explicit `LOCK_UN` before its descriptor
/// closes (a child forked meanwhile holds a copy of the descriptor until its `exec`).
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
fn io(what: &str) -> Failure {
    fail("io_error", "home", json!(what))
}
/// A directory that is no link.
fn real_dir(p: &Path) -> bool {
    p.symlink_metadata().is_ok_and(|m| m.is_dir())
}
/// `dir` made with mode 0700 when absent; a directory and no link (R4).
fn made(dir: &Path, recursive: bool) -> Result<(), Failure> {
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(recursive).mode(0o700);
    match dirs.create(dir) {
        Err(e) if e.kind() != ErrorKind::AlreadyExists => Err(io("a directory cannot be made")),
        _ if !real_dir(dir) => Err(io("a directory is no directory or is a link")),
        _ => Ok(()),
    }
}
/// A new file of mode 0600 with `bytes`, never written through a link; an existing one fails.
fn new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    options.open(path)?.write_all(bytes)
}
/// Every entry of `dir`, when it is a directory and no link, removed without following a link.
fn empty(dir: &Path) {
    if !real_dir(dir) {
        return;
    }
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let _ = match real_dir(&p) {
            true => std::fs::remove_dir_all(&p),
            false => std::fs::remove_file(&p),
        };
    }
}
/// A path removed whatever it is, a link as a link.
fn remove(p: &Path) {
    let _ = match real_dir(p) {
        true => std::fs::remove_dir_all(p),
        false => std::fs::remove_file(p),
    };
}
/// The number of the next launch: one above every `<m>.` that begins a name of `home` (R4, `<n>`).
fn next_launch(home: &Path) -> u64 {
    let names = std::fs::read_dir(home).into_iter().flatten().flatten();
    let numbers = names.filter_map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let (m, _) = name.split_once('.')?;
        (!m.is_empty() && m.bytes().all(|c| c.is_ascii_digit())).then(|| m.parse::<u64>().ok())?
    });
    numbers.max().unwrap_or(0) + 1
}
/// The records a killed call left `running`: each one's scope stopped and read as ended, its `run`
/// object closed `killed`; then the working and scratch directories emptied (R4).
fn leftovers(s: &mut impl System, store: Option<&str>, home: &Path) -> Result<(), Stop> {
    let mut records: Vec<PathBuf> = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.strip_suffix(".amend.json.run.json")
                .is_some_and(|m| !m.is_empty() && m.bytes().all(|c| c.is_ascii_digit()))
        })
        .collect();
    records.sort();
    for path in records {
        let Some(rec) = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            continue;
        };
        if rec["state"] != "running" {
            continue;
        }
        if let Some(name) = rec["unit"].as_str()
            && !(stage_unit(name) && unit_ended(s, name, true))
        {
            return Err(fail("run_still_active", "record", json!(name)).into());
        }
        let path = path.to_string_lossy().into_owned();
        let closed = ["applied", "already_applied"].map(|c| rec["run_object"]["close"] == c);
        let run = rec["run_object"]["key"]
            .as_str()
            .filter(|_| closed == [false, false]);
        let Some(run) = run else { continue };
        let found = store::get(s, store, "run", run).ok().flatten();
        if !found.is_some_and(|r| r["record"] == path && r["status"] == "running") {
            continue;
        }
        let mut p = json!({"run": id("run", run), "state": "killed"});
        if let Some(t) = rec["session"].as_str().filter(|t| !t.is_empty()) {
            p["session"] = json!(t);
        }
        let out = path.strip_suffix(".run.json").unwrap_or(&path);
        let rid = format!("run-{}-close", &digest(out.as_bytes())[..32]);
        // The stage's own request id; the store's answer is the run's.
        let _ = store::close_run(s, store, (&rid, "writer", &p));
    }
    empty(&home.join("work"));
    empty(&home.join("scratch"));
    Ok(())
}
/// `git -C <clone> fetch --quiet origin main` within 120 seconds; a fetch that raced another on the
/// same clone is tried again after 2 seconds, five tries at most (R5).
fn fetch(s: &mut impl System, clone: &str) -> Result<(), Failure> {
    let words: Vec<String> = ["-C", clone, "fetch", "--quiet", "origin", "main"]
        .iter()
        .map(|w| (*w).to_owned())
        .collect();
    for n in 1..=FETCH_TRIES {
        match s.call("git", &words, &Value::Null, FETCH_MS) {
            Ok(r) if r.success => return Ok(()),
            Ok(r) if racing(&r.bytes) && n < FETCH_TRIES => s.pause(AGAIN),
            _ => break,
        }
    }
    Err(error("fetch_failed", "clone"))
}
/// The text of the first launch (R5): the page without `--for`, the pages of the dependencies and
/// the frame; and the commit its repository part names.
fn text(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    u: &Value,
) -> Result<(String, Option<String>), Failure> {
    let (mut out, commit) = render::page_text(s, store, key, u, None)?;
    out.push_str("\n## Pages of the dependencies\n");
    let deps: Vec<&str> = u["depends_on"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["native_key"].as_str().or(d.as_str()))
        .collect();
    if deps.is_empty() {
        out.push_str("\nThis unit depends on no unit.\n");
    }
    for d in deps {
        match store::at(store::get(s, store, "unit", d), "depends_on")? {
            Some(dep) => {
                let state = dep["state"].as_str().unwrap_or_default();
                out.push_str(&format!("\n### {d}: {state}\n\n"));
                out.push_str(&render::page_part(d, &dep, false));
            }
            None => out.push_str(&format!("\n### {d}\n\n(not in the store)\n")),
        }
    }
    out.push('\n');
    out.push_str(FRAME);
    Ok((out, commit))
}
/// The schema of the answer: an object of the members of R7 and no other (R6).
fn answer_schema() -> Value {
    let text = json!({"type": "string"});
    let list = json!({"type": "array", "items": {"type": "string"}});
    let tier = json!({"type": "object", "additionalProperties": false,
        "properties": {"runtime": text, "model": text, "effort": text}});
    let check = json!({"type": "object", "additionalProperties": false,
        "properties": {"check": text, "kind": {"type": "string", "enum": ["fails_on_base", "invariant"]}}});
    let mut properties = Map::new();
    for m in MEMBERS {
        let v = match m {
            "acceptance" => json!({"type": "array", "items": check}),
            "write_set" | "read_first" | "specs" => list.clone(),
            "implementer" | "verifier" => tier.clone(),
            "estimate_lines" => json!({"type": "integer", "minimum": 0}),
            _ => text.clone(),
        };
        properties.insert(m.to_owned(), v);
    }
    json!({"type": "object", "additionalProperties": false, "properties": properties})
}
/// Whether the pattern `p` lies inside the bound (R7): an exact path of the bound itself; or, for a
/// `<dir>/**` of the bound, an exact path below `<dir>` or a `<e>/**` whose `<e>` is `<dir>` or lies
/// below it, compared as path components. A text of neither form is outside.
pub fn inside(p: &str, bound: &[String]) -> bool {
    let pattern = write_set_pattern(p);
    if !pattern.grammar {
        return false;
    }
    let below = |path: &str, dir: &str| {
        path.strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    bound.iter().any(|b| {
        let of_bound = write_set_pattern(b);
        if !of_bound.grammar {
            return false;
        }
        match (of_bound.reading, pattern.reading) {
            (Reading::Exact(e), Reading::Exact(x)) => e == x,
            (Reading::Below(dir), Reading::Exact(x)) => below(x, dir),
            (Reading::Below(dir), Reading::Below(e)) => e == dir || below(e, dir),
            (Reading::Exact(_), Reading::Below(_)) => false,
        }
    })
}
/// The amendment of a launch that ended `ok`, read once, judged before any other check (R7).
fn judged(out: &str, bound: &[String]) -> Result<Map<String, Value>, Value> {
    let read = std::fs::read(out)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let Some(Value::Object(a)) = read else {
        return Err(
            json!({"code": "amendment_not_admitted", "field": "out", "detail": "the answer is not one JSON object"}),
        );
    };
    if let Some(k) = a.keys().find(|k| !MEMBERS.contains(&k.as_str())) {
        return Err(json!({"code": "amendment_not_admitted", "field": k, "detail": null}));
    }
    if let Some(Value::Array(list)) = a.get("write_set") {
        for (i, p) in list.iter().enumerate() {
            if !p.as_str().is_some_and(|p| inside(p, bound)) {
                return Err(
                    json!({"code": "write_set_outside", "field": format!("write_set.{i}"),
                    "detail": {"pattern": p, "bound": bound}}),
                );
            }
        }
    }
    Ok(a)
}
/// A refusal of `ap unit amend` as it goes back to the writer (R10): its code, field, detail and
/// `fields`; None for one that ends the call.
fn amend_back(r: &object::Refusal) -> Option<Value> {
    let store_refused = r.detail["status"] == "refused";
    let ours = ["invalid_input", "page_too_large", "plain_writes_protected"];
    if !(ours.contains(&r.code.as_str()) || store_refused) {
        return None;
    }
    let mut v = json!({"code": r.code, "field": r.field, "detail": r.detail});
    if let Some(fields) = &r.fields {
        v["fields"] = fields.clone();
    }
    Some(v)
}
/// What goes back to the writer after a launch.
enum Back {
    /// A refusal of the amendment, the apply or the look at the page, as one JSON line.
    Refusal(Value),
    /// The path of the report of a look that said `repair`.
    Report(String),
}
/// How a call ends (R11).
enum End {
    Accept,
    Repair,
    Refused,
    Stage,
    LookAbandoned,
    LookRefused,
}
impl End {
    fn word(&self) -> &'static str {
        match self {
            End::Accept => "accept",
            End::Repair => "repair",
            End::Refused => "refused",
            End::Stage => "stage",
            End::LookAbandoned => "look_abandoned",
            End::LookRefused => "look_refused",
        }
    }
}
/// The parts of a call that its launches and looks share.
struct Work<'a> {
    store: Option<&'a str>,
    key: &'a str,
    drawn: String,
    count: u32,
    /// The revision and the fields of the unit as this call last read it.
    revision: String,
    unit: Value,
}
impl Work<'_> {
    /// A new request id `pw-<16 hex>-<counter>` (R8).
    fn rid(&mut self) -> String {
        self.count += 1;
        format!("pw-{}-{}", self.drawn, self.count)
    }
    fn read(&mut self, s: &mut impl System) -> Result<(), Stop> {
        let read = store::at(store::head(s, self.store, "unit", self.key), "unit")?;
        let (revision, unit) = read.ok_or_else(|| error("unknown_unit", "unit"))?;
        (self.revision, self.unit) = (revision, unit);
        Ok(())
    }
}

fn call(
    s: &mut impl System,
    key: &str,
    store: Option<&str>,
    amendments: &mut u32,
) -> Result<(Value, u8), Stop> {
    // R1: the role, then the key.
    if object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE").into());
    }
    if !key_form(key) {
        return Err(error("invalid_input", "unit").into());
    }
    // R2: the policy's member `writer` and the accounts of the stage.
    let policy = crate::work::policy(s)?.unwrap_or_default();
    let w = writer(&policy)?.ok_or_else(|| error("writer_not_configured", "policy"))?;
    if store.is_some() && policy["writer"].get("directory").is_none() {
        return Err(error("invalid_input", "policy").into());
    }
    let accounts = accounts(&policy, &w.model)?;
    // R3: the unit; its write set is the bound of the call.
    let drawn = new_session_id(s)?.replace('-', "");
    let mut work = Work {
        store,
        key,
        drawn: drawn[..16].to_owned(),
        count: 0,
        revision: String::new(),
        unit: Value::Null,
    };
    work.read(s)?;
    if work.unit["state"] != "draft" {
        return Err(error("unit_not_draft", "unit").into());
    }
    let bound: Vec<String> = work.unit["write_set"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if work.unit["write_set"].as_array().is_none_or(Vec::is_empty) {
        return Err(error("write_set_missing", "unit").into());
    }
    // R4: the home, the lock, the leftovers, the file `started`.
    let pages = PathBuf::from(&w.directory);
    made(&pages, true)?;
    let home = pages.join(key);
    made(&home, false)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let lock = options
        .open(home.join("lock"))
        .map_err(|_| io("the lock"))?;
    let taken = rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive);
    taken.map_err(|_| error("write_active", "unit"))?;
    let _held = Held(lock);
    leftovers(s, store, &home)?;
    match new_file(&home.join("started"), b"") {
        Err(e) if e.kind() != ErrorKind::AlreadyExists => return Err(io("the file started").into()),
        _ => {}
    }
    // R5: the fetch and the text.
    let clone = match work.unit["repository"].as_str().filter(|r| !r.is_empty()) {
        Some(r) => Some(clone_of(s, r)?),
        None => None,
    };
    if let Some(clone) = &clone {
        fetch(s, clone)?;
    }
    let (text, commit) = text(s, store, key, &work.unit)?;
    if text.len() > PROMPT_MAX {
        return Err(fail(
            "prompt_too_large",
            "unit",
            json!({"bytes": text.len(), "limit": PROMPT_MAX}),
        )
        .into());
    }
    let mut n = next_launch(&home);
    let prompt = home.join(format!("{n}.prompt.md"));
    new_file(&prompt, text.as_bytes()).map_err(|_| io("the prompt"))?;
    // R6: the schema, the working directory of every launch, the stages.
    let schema = home.join("amendment.schema.json");
    remove(&schema);
    let bytes = serde_json::to_vec_pretty(&answer_schema()).unwrap_or_default();
    new_file(&schema, &bytes).map_err(|_| io("the schema"))?;
    for sub in ["work", "scratch"] {
        made(&home.join(sub), false)?;
    }
    let wd = home.join("work").join(n.to_string());
    let copied = match (&clone, &commit) {
        (Some(clone), Some(commit)) => {
            object::copy(s, clone, &wd.to_string_lossy(), commit).map(|()| {
                let mode = std::fs::Permissions::from_mode(0o700);
                std::fs::set_permissions(&wd, mode).unwrap_or_default()
            })
        }
        _ => made(&wd, false),
    };
    let ended = copied.map_err(Stop::from).and_then(|()| {
        let stages = Stages {
            w: &w,
            accounts: &accounts,
            home: &home,
            wd: &wd,
            schema: &schema,
        };
        launches(s, &mut work, &stages, &bound, (&mut n, prompt), amendments)
    });
    remove(&wd);
    ended
}
/// What every launch of a call has in common (R6).
struct Stages<'a> {
    w: &'a Writer,
    accounts: &'a [String],
    home: &'a Path,
    wd: &'a Path,
    schema: &'a Path,
}
impl Stages<'_> {
    /// The request of `ap run` for the launch `n` with its prompt, resuming `resume` when given.
    fn request(&self, work: &Work, n: u64, prompt: &Path, resume: Option<&str>) -> Value {
        let p = |p: &Path| p.to_string_lossy().into_owned();
        let out = self.home.join(format!("{n}.amend.json"));
        let scratch = self.home.join("scratch").join(n.to_string());
        let mut r = json!({"role": "writer", "runtime": "claude", "account": self.accounts[0],
            "model": self.w.model, "effort": self.w.effort, "permission_mode": self.w.permission_mode,
            "timeout_seconds": self.w.timeout, "cwd": p(self.wd), "prompt": p(prompt), "out": p(&out),
            "answer_schema": p(self.schema), "limits": {"scratch": p(&scratch)}, "unit": work.key});
        if self.accounts.len() > 1 {
            r["next_accounts"] = json!(self.accounts[1..]);
        }
        if let Some(c) = self.w.compact_window {
            r["compact_window"] = json!(c);
        }
        if let Some(store) = work.store {
            r["store"] = json!(store);
        }
        if let Some(record) = resume {
            r["resume"] = json!({ "record": record });
        }
        r
    }
}
/// The launches of a call, each followed by the bound, the apply and the look (R6 to R11).
fn launches(
    s: &mut impl System,
    work: &mut Work,
    stages: &Stages,
    bound: &[String],
    (n, mut prompt): (&mut u64, PathBuf),
    amendments: &mut u32,
) -> Result<(Value, u8), Stop> {
    let mut launched: Vec<Value> = vec![];
    let mut reviews: Vec<String> = vec![];
    let mut last_look: Option<String> = None;
    let mut record: Option<String> = None;
    let mut refusal = Value::Null;
    let end = loop {
        let req = stages.request(work, *n, &prompt, record.as_deref());
        let out = req["out"].as_str().unwrap_or_default().to_owned();
        let (rec, _) = crate::run::execute(s, &req)?;
        let this = format!("{out}.run.json");
        launched.push(json!({"n": *n, "record": this, "state": rec["state"],
            "run": rec["run_object"]["key"]}));
        record = Some(this);
        if rec["state"] != "ok" {
            break End::Stage;
        }
        let back = match judged(&out, bound) {
            Err(r) => Back::Refusal(r),
            Ok(a) => {
                // R8: the unit as this call last read it, then the apply by `ap unit amend`'s code.
                let last = work.revision.clone();
                work.read(s)?;
                if work.revision != last {
                    return Err(error("unit_changed", "unit").into());
                }
                if work.unit["state"] != "draft" {
                    return Err(error("unit_not_draft", "unit").into());
                }
                let mut r = Value::Object(a);
                r["request_id"] = json!(work.rid());
                r["agent"] = json!("ap-write");
                r["session"] = json!(format!("p-{}", work.key));
                r["unit"] = json!(work.key);
                let amend = Call {
                    op: "amend",
                    store: work.store.map(str::to_owned),
                    key: None,
                    role: None,
                    digest: false,
                };
                match object::execute(s, &amend, &r) {
                    Err(e) => match amend_back(&e) {
                        Some(v) => Back::Refusal(v),
                        None => {
                            return Err(Stop {
                                code: e.code,
                                field: e.field,
                                detail: e.detail,
                            });
                        }
                    },
                    Ok(_) => {
                        *amendments += 1;
                        work.read(s)?;
                        // R9: the look at the page.
                        match look(s, work, last_look.as_deref())? {
                            Look::Accept(k) => {
                                reviews.push(k);
                                refusal = Value::Null;
                                break End::Accept;
                            }
                            Look::Repair(k, report) => {
                                reviews.push(k.clone());
                                last_look = Some(k);
                                Back::Report(report)
                            }
                            Look::Abandoned(k) => {
                                reviews.push(k);
                                refusal = Value::Null;
                                break End::LookAbandoned;
                            }
                            Look::PageRefused(v) => Back::Refusal(v),
                            Look::Refused(v) => {
                                refusal = v;
                                break End::LookRefused;
                            }
                        }
                    }
                }
            }
        };
        // R10: the way back to the same session, three launches at most.
        let line = match &back {
            Back::Refusal(v) => {
                refusal = json!({"code": v["code"], "field": v["field"], "detail": v["detail"]});
                v.to_string()
            }
            Back::Report(path) => {
                refusal = Value::Null;
                path.clone()
            }
        };
        if launched.len() >= LAUNCHES {
            break match back {
                Back::Refusal(_) => End::Refused,
                Back::Report(_) => End::Repair,
            };
        }
        *n += 1;
        prompt = stages.home.join(format!("{n}.prompt.md"));
        new_file(&prompt, format!("{BACK}\n{line}\n").as_bytes()).map_err(|_| io("the prompt"))?;
    };
    let answer = json!({"unit": work.key, "end": end.word(), "launches": launched,
        "amendments": *amendments, "reviews": reviews,
        "digest": render::page_digest(work.key, &work.unit), "refusal": refusal});
    let status = if matches!(end, End::Accept) { 0 } else { 3 };
    Ok((answer, status))
}
/// What a look at the page gave (R9).
enum Look {
    Accept(String),
    Repair(String, String),
    Abandoned(String),
    /// A refusal of `specs/ap/unit-review.md` R23: it goes back to the writer.
    PageRefused(Value),
    /// Any other refusal: it ends the call.
    Refused(Value),
}
/// One look by the code of `ap unit review`, repeated after a refusal `fetch_failed` (R9).
fn look(s: &mut impl System, work: &mut Work, resume: Option<&str>) -> Result<Look, Stop> {
    let mut repeats = 0;
    loop {
        let mut r = json!({"request_id": work.rid(), "agent": "ap-write",
            "session": format!("p-{}", work.key), "unit": work.key});
        if let Some(k) = resume {
            r["resume"] = json!(k);
        }
        let e = match page_review::execute(s, work.store, &r) {
            Ok((v, _)) => {
                let k = v["review"].as_str().unwrap_or_default().to_owned();
                return Ok(match v["verdict"].as_str() {
                    Some("accept") => Look::Accept(k),
                    Some("repair") => {
                        Look::Repair(k, v["report"].as_str().unwrap_or_default().to_owned())
                    }
                    _ => Look::Abandoned(k),
                });
            }
            Err(e) => e,
        };
        let v = json!({"code": e.code, "field": e.field, "detail": e.detail});
        if e.code == "fetch_failed" && e.after.is_none() && repeats < LOOK_REPEATS {
            repeats += 1;
            s.pause(AGAIN);
            continue;
        }
        if PAGE_REFUSED.contains(&e.code.as_str()) && e.after.is_none() {
            return Ok(Look::PageRefused(v));
        }
        return Ok(Look::Refused(v));
    }
}
