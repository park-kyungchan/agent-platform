//! The pass `ap measure sessions` (`specs/ap/measure.md` R2 to R8): the session records of each account, the stage
//! records and the door's record are read as the read-out `evals/readout.py` reads them; each run of route
//! `engineering` is matched to the store's runs, recorded when it has none, extended when it grew, and measured by
//! UTC day. Nothing is written but the three actions; no path, cwd, text or input leaves this file but a run's
//! `record`.
use super::count::{Counts, of_tool, refusals};
use crate::host::{Failure, System, error, id};
use crate::store::{self, Actor};
use crate::watch::{epoch, utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// The prices `ap` was built with (R2).
const PRICES: &str = include_str!("../../../../evals/prices.tsv");
const STAGE_DIRS: [&str; 2] = [
    "/var/lib/agent-platform/units",
    "/var/lib/agent-platform/reviews",
];
const DOOR_LOG: &str = "/var/lib/agent-platform/door.jsonl";
const TEMP_ROOTS: [&str; 2] = ["/tmp", "/var/tmp"];
/// The word `crates/ap/src/run.rs` gives a Claude stage's `runtime` (`Runtime::word`), also the pass's.
const RUNTIME: &str = "claude";
const DAY: u64 = 86_400;

/// Whether a walk skips a directory of this name (R3): a name that begins with `edu-`, and, for a project directory
/// (one directly in a `projects` directory), a name that holds `-edu-` or `-education-platform`.
pub fn academy(name: &str, project: bool) -> bool {
    name.starts_with("edu-")
        || (project && (name.contains("-edu-") || name.contains("-education-platform")))
}

// The options (R2).

type Rates = BTreeMap<String, BTreeMap<String, f64>>;
struct Options {
    since: String,
    accounts: Vec<PathBuf>,
    work_root: String,
    platforms_root: String,
    stage_dirs: Vec<String>,
    rates: Rates,
    store: Option<String>,
}

/// The rows of a price file as the read-out's `load_prices` reads them: `model`, `kind`, `rate` by a tab.
fn load_prices(text: &str) -> Option<Rates> {
    let mut rates = Rates::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let [model, kind, value] = cols.as_slice() else {
            return None;
        };
        let value: f64 = value.trim().parse().ok().filter(|v: &f64| v.is_finite())?;
        rates
            .entry((*model).to_owned())
            .or_default()
            .insert((*kind).to_owned(), value);
    }
    Some(rates)
}

/// A UTC day `YYYY-MM-DD` of the calendar, as seconds since 1970 at its start.
pub(super) fn day_start(day: &str) -> Option<u64> {
    let t = epoch(&format!("{day}T00:00:00Z"))?;
    (utc(t).get(..10) == Some(day)).then_some(t)
}

fn options(s: &impl System, argv: &[OsString]) -> Result<Options, Failure> {
    let usage = || error("usage", "args");
    let mut words = vec![];
    for a in argv {
        words.push(a.to_str().ok_or_else(usage)?.to_owned());
    }
    let mut once: BTreeMap<&str, String> = BTreeMap::new();
    let (mut accounts, mut stage_dirs) = (vec![], vec![]);
    let mut it = words.iter();
    while let Some(w) = it.next() {
        let name = match w.as_str() {
            n @ ("--since" | "--accounts" | "--work-root" | "--platforms-root" | "--stage-dir"
            | "--prices" | "--store") => n,
            _ => return Err(usage()),
        };
        let value = it.next().ok_or_else(usage)?.clone();
        match name {
            "--accounts" => accounts.push(value),
            "--stage-dir" => stage_dirs.push(value),
            _ => {
                if once.insert(name, value).is_some() {
                    return Err(usage());
                }
            }
        }
    }
    let invalid = |f: &str| error("invalid_input", f);
    let absolute = |v: &String| v.starts_with('/');
    if !accounts.iter().all(absolute) {
        return Err(invalid("accounts"));
    }
    if !stage_dirs.iter().all(absolute) {
        return Err(invalid("stage-dir"));
    }
    for (option, field) in [
        ("--work-root", "work-root"),
        ("--platforms-root", "platforms-root"),
        ("--prices", "prices"),
        ("--store", "store"),
    ] {
        if once.get(option).is_some_and(|v| !absolute(v)) {
            return Err(invalid(field));
        }
    }
    let today = s
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        / DAY
        * DAY;
    let since = match once.get("--since") {
        Some(d) => {
            day_start(d).ok_or_else(|| invalid("since"))?;
            d.clone()
        }
        None => utc(today.saturating_sub(2 * DAY))[..10].to_owned(),
    };
    let rates = match once.get("--prices") {
        Some(p) => std::fs::read_to_string(p)
            .ok()
            .and_then(|t| load_prices(&t))
            .ok_or_else(|| invalid("prices"))?,
        None => load_prices(PRICES).ok_or_else(|| invalid("prices"))?,
    };
    let accounts = if accounts.is_empty() {
        default_accounts(s)
    } else {
        accounts.into_iter().map(PathBuf::from).collect()
    };
    Ok(Options {
        since,
        accounts,
        work_root: once
            .remove("--work-root")
            .unwrap_or_else(|| "/home/palantirkc/work".into()),
        platforms_root: once
            .remove("--platforms-root")
            .unwrap_or_else(|| "/home/palantirkc/platforms".into()),
        stage_dirs: if stage_dirs.is_empty() {
            STAGE_DIRS.iter().map(|d| (*d).to_owned()).collect()
        } else {
            stage_dirs
        },
        rates,
        store: once.remove("--store"),
    })
}

fn variable(s: &impl System, name: &str) -> Option<String> {
    s.environment()
        .into_iter()
        .find(|(k, v)| k == name && !v.is_empty())
        .map(|(_, v)| v)
}

/// Each directory `$HOME/.claude*` that holds a directory `projects`, by name.
fn default_accounts(s: &impl System) -> Vec<PathBuf> {
    let Some(home) = variable(s, "HOME") else {
        return vec![];
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(&home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".claude"))
        .map(|e| e.path())
        .filter(|p| p.join("projects").is_dir())
        .collect();
    found.sort();
    found
}

// The walks (R3).

#[derive(Default)]
struct Walked {
    files: Vec<(PathBuf, String)>,
    skipped: u64,
}
/// Every `*.jsonl` under `dir`, not through a link; a directory the Academy's is skipped and counted.
fn walk(dir: &Path, project: bool, account: &str, w: &mut Walked) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let Ok(kind) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if academy(&name, project) {
                w.skipped += 1;
            } else {
                walk(&e.path(), false, account, w);
            }
        } else if kind.is_file() && name.ends_with(".jsonl") {
            w.files.push((e.path(), account.to_owned()));
        }
    }
}
/// The session files of the accounts: `<account>/projects`, then `<account>/homes/<name>/projects` by name.
fn session_files(accounts: &[PathBuf]) -> Walked {
    let mut w = Walked::default();
    for account in accounts {
        let name = account
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        walk(&account.join("projects"), true, &name, &mut w);
        let mut homes: Vec<_> = std::fs::read_dir(account.join("homes"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|k| k.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        homes.sort();
        for h in homes {
            if academy(&h, false) {
                w.skipped += 1;
                continue;
            }
            walk(
                &account.join("homes").join(h).join("projects"),
                true,
                &name,
                &mut w,
            );
        }
    }
    let mut seen = BTreeSet::new();
    w.files
        .retain(|(p, _)| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())));
    w.files.sort_by(|a, b| a.0.cmp(&b.0));
    w
}

struct Stage {
    path: String,
    session: String,
    role: String,
    start: String,
    from_store: bool,
}
fn stage_walk(
    dir: &Path,
    depth: usize,
    max: Option<usize>,
    out: &mut Vec<PathBuf>,
    skipped: &mut u64,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if academy(&name, false) {
                *skipped += 1;
            } else if max.is_none_or(|m| depth < m) {
                stage_walk(&e.path(), depth + 1, max, out, skipped);
            }
        } else if kind.is_file() && name.ends_with(".run.json") {
            out.push(e.path());
        }
    }
}
/// The stage records as the read-out's `read_stages` reads them: the work root's `*/**/*.run.json` and each stage
/// directory's `*.run.json` and `*/*.run.json` (marked as read from a stage directory); a file reached both ways is
/// read once.
fn read_stages(work_root: &str, stage_dirs: &[String], skipped: &mut u64) -> Vec<Stage> {
    let mut found: BTreeMap<PathBuf, (PathBuf, bool)> = BTreeMap::new();
    let mut work = vec![];
    stage_walk(Path::new(work_root), 0, None, &mut work, skipped);
    for f in work {
        // `*/**/*.run.json`: not a file directly in the work root.
        if f.parent() != Some(Path::new(work_root)) {
            found.insert(
                std::fs::canonicalize(&f).unwrap_or_else(|_| f.clone()),
                (f, false),
            );
        }
    }
    for d in stage_dirs {
        let mut files = vec![];
        stage_walk(Path::new(d), 0, Some(1), &mut files, skipped);
        for f in files {
            found.insert(
                std::fs::canonicalize(&f).unwrap_or_else(|_| f.clone()),
                (f, true),
            );
        }
    }
    let mut records: Vec<(PathBuf, bool)> = found.into_values().collect();
    records.sort();
    let mut stages = vec![];
    for (f, from_store) in records {
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes))
        else {
            continue;
        };
        let Some(session) = o.get("session").and_then(Value::as_str) else {
            continue;
        };
        let text = |k: &str, default: &str| match o.get(k) {
            Some(Value::String(v)) if !v.is_empty() => v.clone(),
            _ => default.to_owned(),
        };
        stages.push(Stage {
            path: f.to_string_lossy().into_owned(),
            session: session.to_owned(),
            role: text("role", "unknown"),
            start: text("start", ""),
            from_store,
        });
    }
    stages
}

// The session records (R4, R7).

const KINDS: usize = 5; // input, output, cache_read, cache_write_5m, cache_write_1h
struct Message {
    ts: String,
    model: Option<String>,
    sessions: BTreeSet<String>,
    tok: [u64; KINDS],
    cwd: String,
}
struct Call {
    name: String,
    counts: Counts,
    ts: String,
    message: Option<String>,
    session: String,
}
/// The refusals of one tool result, by code and field.
type Refusals = BTreeMap<(String, String), u64>;
struct Shown {
    use_id: String,
    ts: String,
    session: String,
    refusals: Refusals,
}
#[derive(Default)]
struct Read {
    messages: BTreeMap<String, Message>,
    calls: BTreeMap<String, Call>,
    shown: BTreeMap<String, Shown>,
    start: BTreeMap<String, String>,
    first_cwd: BTreeMap<String, (String, String)>,
    account: BTreeMap<String, String>,
    files: u64,
    unreadable: u64,
}
fn num(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}
fn text_of(content: &Value) -> String {
    match content {
        Value::String(t) => t.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
impl Read {
    fn line(&mut self, o: &serde_json::Map<String, Value>, account: &str, old: bool, since: &str) {
        let sid = o
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let ts = o
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !sid.is_empty() && !ts.is_empty() {
            let start = self
                .start
                .entry(sid.to_owned())
                .or_insert_with(|| ts.to_owned());
            if ts < start.as_str() {
                *start = ts.to_owned();
            }
            if let Some(cwd) = o
                .get("cwd")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
            {
                let first = self.first_cwd.get(sid);
                if first.is_none_or(|(t, c)| (ts, cwd) < (t.as_str(), c.as_str())) {
                    self.first_cwd
                        .insert(sid.to_owned(), (ts.to_owned(), cwd.to_owned()));
                }
            }
        }
        if !sid.is_empty() {
            self.account
                .entry(sid.to_owned())
                .or_insert_with(|| account.to_owned());
        }
        if old || sid.is_empty() {
            return;
        }
        let m = o.get("message").unwrap_or(&Value::Null);
        let day_ok = ts.get(..10).is_some_and(|d| d >= since);
        match o.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let mid = m["id"].as_str();
                if let (Some(mid), Some(u)) = (mid, m["usage"].as_object()) {
                    let cc = &u.get("cache_creation").cloned().unwrap_or(Value::Null);
                    let (w5, w1) = match cc.is_object() {
                        true => (
                            num(&cc["ephemeral_5m_input_tokens"]),
                            num(&cc["ephemeral_1h_input_tokens"]),
                        ),
                        false => (
                            num(u.get("cache_creation_input_tokens").unwrap_or(&Value::Null)),
                            0,
                        ),
                    };
                    let get = |k: &str| num(u.get(k).unwrap_or(&Value::Null));
                    let tok = [
                        get("input_tokens"),
                        get("output_tokens"),
                        get("cache_read_input_tokens"),
                        w5,
                        w1,
                    ];
                    let cwd = o.get("cwd").and_then(Value::as_str).unwrap_or_default();
                    match self.messages.get_mut(mid) {
                        None => {
                            self.messages.insert(
                                mid.to_owned(),
                                Message {
                                    ts: ts.to_owned(),
                                    model: m["model"].as_str().map(str::to_owned),
                                    sessions: BTreeSet::from([sid.to_owned()]),
                                    tok,
                                    cwd: cwd.to_owned(),
                                },
                            );
                        }
                        Some(rec) => {
                            for (k, v) in tok.iter().enumerate() {
                                rec.tok[k] = rec.tok[k].max(*v);
                            }
                            rec.sessions.insert(sid.to_owned());
                            if !ts.is_empty() && ts < rec.ts.as_str() {
                                rec.ts = ts.to_owned();
                            }
                        }
                    }
                }
                for b in m["content"].as_array().into_iter().flatten() {
                    let (Some("tool_use"), Some(use_id), Some(name)) =
                        (b["type"].as_str(), b["id"].as_str(), b["name"].as_str())
                    else {
                        continue;
                    };
                    if self.calls.contains_key(use_id) {
                        continue;
                    }
                    let counts = if day_ok {
                        of_tool(name, &b["input"])
                    } else {
                        Counts::default()
                    };
                    self.calls.insert(
                        use_id.to_owned(),
                        Call {
                            name: name.to_owned(),
                            counts,
                            ts: ts.to_owned(),
                            message: mid.map(str::to_owned),
                            session: sid.to_owned(),
                        },
                    );
                }
            }
            Some("user") => {
                for b in m["content"].as_array().into_iter().flatten() {
                    let (Some("tool_result"), Some(use_id)) =
                        (b["type"].as_str(), b["tool_use_id"].as_str())
                    else {
                        continue;
                    };
                    if self.shown.contains_key(use_id) || !day_ok {
                        continue;
                    }
                    let found = refusals(&text_of(&b["content"]));
                    self.shown.insert(
                        use_id.to_owned(),
                        Shown {
                            use_id: use_id.to_owned(),
                            ts: ts.to_owned(),
                            session: sid.to_owned(),
                            refusals: found,
                        },
                    );
                }
            }
            _ => {}
        }
    }
}
fn read_sessions(files: &[(PathBuf, String)], since_epoch: u64, since: &str) -> Read {
    let mut r = Read::default();
    for (f, account) in files {
        let Ok(meta) = std::fs::metadata(f) else {
            continue;
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let old = modified < since_epoch;
        let Ok(bytes) = std::fs::read(f) else {
            continue;
        };
        r.files += 1;
        for raw in String::from_utf8_lossy(&bytes).lines() {
            match serde_json::from_str::<Value>(raw) {
                Ok(Value::Object(o)) => r.line(&o, account, old, since),
                _ if old => {}
                _ => r.unreadable += 1,
            }
        }
    }
    r
}

// The runs (R5, R6).

/// A run of the pass: one stage record of a session, or a session without one.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Record(String),
    Session(String),
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Day {
    messages: u64,
    usd_micros: i64,
    refusals: u64,
    counts: Counts,
}
#[derive(Default)]
struct Run {
    session: String,
    role: String,
    cwd: String,
    account: String,
    stage_read: bool,
    messages: Vec<(String, [u64; KINDS])>,
    models: BTreeMap<String, u64>,
    days: BTreeMap<String, Day>,
}

fn under(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root || path.starts_with(&format!("{root}/"))
}
/// The read-out's `route_of` (R5): lesson, a record of a stage directory, the two roots, a temporary directory.
fn route_of(cwd: &str, o: &Options, stage: Option<&Stage>) -> &'static str {
    let lesson = format!("{}/edu-lessons", o.work_root.trim_end_matches('/'));
    if under(cwd, &lesson) {
        return "lesson";
    }
    if stage.is_some_and(|s| s.from_store)
        || under(cwd, &o.work_root)
        || under(cwd, &o.platforms_root)
    {
        return "engineering";
    }
    if stage.is_none() && TEMP_ROOTS.iter().any(|t| under(cwd, t)) {
        "temporary"
    } else {
        "other"
    }
}
/// The read-out's `role_without_stage` with no `--lead-cwd`.
fn role_without_stage(cwd: &str, o: &Options) -> &'static str {
    let work = format!("{}/", o.work_root.trim_end_matches('/'));
    let platforms = format!("{}/", o.platforms_root.trim_end_matches('/'));
    let lesson = format!("{}/edu-lessons/", o.work_root.trim_end_matches('/'));
    let here = format!("{cwd}/");
    let in_worktree = here.starts_with(&work) && here[work.len() - 1..].contains("/wt/");
    if in_worktree || here.starts_with(&platforms) {
        return "worker, no record";
    }
    if here.starts_with(&work) && here != work && !here.starts_with(&lesson) {
        "lead"
    } else {
        "interactive"
    }
}

/// USD at list price in millionths (the read-out's `cost`): the `+long` rate past `long_over_tokens`, 0 without a price.
fn micros(model: Option<&str>, tok: &[u64; KINDS], rates: &Rates) -> i64 {
    let Some(mut r) = model.and_then(|m| rates.get(m)) else {
        return 0;
    };
    let prompt = tok[0] + tok[2] + tok[3] + tok[4];
    if r.get("long_over_tokens")
        .is_some_and(|over| prompt as f64 > *over)
        && let Some(long) = model.and_then(|m| rates.get(&format!("{m}+long")))
    {
        r = long;
    }
    let kinds = [
        "input",
        "output",
        "cache_read",
        "cache_write_5m",
        "cache_write_1h",
    ];
    let sum: f64 = kinds
        .iter()
        .zip(tok)
        .map(|(k, n)| *n as f64 * r.get(*k).copied().unwrap_or(0.0))
        .sum();
    sum.round() as i64
}

struct Pass<'a> {
    o: &'a Options,
    read: Read,
    stages: Vec<Stage>,
    by_session: BTreeMap<String, Vec<usize>>,
    owner: BTreeMap<String, String>,
    runs: BTreeMap<Key, Run>,
    skipped_route: BTreeSet<Key>,
}
impl Pass<'_> {
    fn cwd(&self, session: &str, fallback: &str) -> String {
        self.read
            .first_cwd
            .get(session)
            .map_or_else(|| fallback.to_owned(), |(_, c)| c.clone())
    }
    /// The key of the run of a line of `session` at `ts` when it is of route `engineering` (R5), its run made
    /// when absent; a run of another route is noted in `skipped_route` and has none.
    fn key_of(&mut self, session: &str, ts: &str, fallback_cwd: &str) -> Option<Key> {
        let cwd = self.cwd(session, fallback_cwd);
        let stage = self.by_session.get(session).map(|ss| {
            let at = ss.partition_point(|i| self.stages[*i].start.as_str() <= ts);
            ss[at.saturating_sub(1)]
        });
        let (key, role, route) = match stage {
            Some(i) => {
                let st = &self.stages[i];
                (
                    Key::Record(st.path.clone()),
                    st.role.clone(),
                    route_of(&cwd, self.o, Some(st)),
                )
            }
            None => (
                Key::Session(session.to_owned()),
                role_without_stage(&cwd, self.o).to_owned(),
                route_of(&cwd, self.o, None),
            ),
        };
        if route != "engineering" {
            self.skipped_route.insert(key);
            return None;
        }
        let account = self.read.account.get(session).cloned().unwrap_or_default();
        self.runs.entry(key.clone()).or_insert_with(|| Run {
            session: session.to_owned(),
            role,
            cwd,
            account,
            stage_read: stage.is_some(),
            ..Run::default()
        });
        Some(key)
    }
    fn run_of(&mut self, session: &str, ts: &str, fallback_cwd: &str) -> Option<&mut Run> {
        let key = self.key_of(session, ts, fallback_cwd)?;
        self.runs.get_mut(&key)
    }
    fn day(&mut self, session: &str, ts: &str) -> Option<&mut Day> {
        let day = ts.get(..10)?.to_owned();
        if day.as_str() < self.o.since.as_str() {
            return None;
        }
        Some(self.run_of(session, ts, "")?.days.entry(day).or_default())
    }
}

/// Assigns messages, calls and refusals to their runs.
fn assemble(o: &Options, read: Read, stages: Vec<Stage>) -> Pass<'_> {
    let mut by_session: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut order: Vec<usize> = (0..stages.len()).collect();
    order.sort_by(|a, b| stages[*a].start.cmp(&stages[*b].start));
    for i in order {
        by_session
            .entry(stages[i].session.clone())
            .or_default()
            .push(i);
    }
    let mut owner = BTreeMap::new();
    for (mid, m) in &read.messages {
        let start = |s: &String| {
            (
                read.start.get(s).cloned().unwrap_or_else(|| "~".into()),
                s.clone(),
            )
        };
        if let Some(first) = m.sessions.iter().min_by_key(|s| start(s)) {
            owner.insert(mid.clone(), first.clone());
        }
    }
    let mut p = Pass {
        o,
        read,
        stages,
        by_session,
        owner,
        runs: BTreeMap::new(),
        skipped_route: BTreeSet::new(),
    };
    let mids: Vec<String> = p
        .read
        .messages
        .iter()
        .filter(|(_, m)| m.ts.get(..10).is_some_and(|d| d >= o.since.as_str()))
        .map(|(mid, _)| mid.clone())
        .collect();
    for mid in mids {
        let Some(session) = p.owner.get(&mid).cloned() else {
            continue;
        };
        let Some(m) = p.read.messages.get(&mid) else {
            continue;
        };
        let (ts, model, tok, cwd) = (m.ts.clone(), m.model.clone(), m.tok, m.cwd.clone());
        let usd = micros(model.as_deref(), &tok, &o.rates);
        let Some(run) = p.run_of(&session, &ts, &cwd) else {
            continue;
        };
        run.messages.push((ts.clone(), tok));
        *run.models
            .entry(model.unwrap_or_else(|| "unknown".into()))
            .or_default() += 1;
        let d = run.days.entry(ts[..10].to_owned()).or_default();
        d.messages += 1;
        d.usd_micros += usd;
    }
    let calls: Vec<(String, String, Counts)> = p
        .read
        .calls
        .values()
        .filter(|c| c.counts != Counts::default())
        .map(|c| {
            let session = c
                .message
                .as_ref()
                .and_then(|m| p.owner.get(m))
                .unwrap_or(&c.session);
            (session.clone(), c.ts.clone(), c.counts)
        })
        .collect();
    for (session, ts, counts) in calls {
        if let Some(d) = p.day(&session, &ts) {
            d.counts.add(counts);
        }
    }
    // Refusals: per run, second, code and field, the larger of a call's count and its carrier's.
    let mut shown: BTreeMap<(Key, String, String, String), [u64; 2]> = BTreeMap::new();
    let blocks: Vec<(String, String, bool, Refusals)> = p
        .read
        .shown
        .values()
        .filter(|b| !b.refusals.is_empty())
        .map(|b| {
            let call = p.read.calls.get(&b.use_id);
            let session = call
                .and_then(|c| c.message.as_ref())
                .and_then(|m| p.owner.get(m))
                .or(call.map(|c| &c.session))
                .unwrap_or(&b.session);
            let carrier = call.is_some_and(|c| c.name == "REPL" || c.name == "Poll");
            (session.clone(), b.ts.clone(), carrier, b.refusals.clone())
        })
        .collect();
    for (session, ts, carrier, found) in blocks {
        let second = ts.get(..19).unwrap_or(&ts).to_owned();
        if ts.get(..10).is_none_or(|d| d < o.since.as_str()) {
            continue;
        }
        let Some(key) = p.key_of(&session, &ts, "") else {
            continue;
        };
        for ((code, field), n) in found {
            shown
                .entry((key.clone(), second.clone(), code, field))
                .or_default()[usize::from(carrier)] += n;
        }
    }
    for ((key, second, _, _), [direct, carried]) in shown {
        if let Some(run) = p.runs.get_mut(&key) {
            run.days
                .entry(second[..10].to_owned())
                .or_default()
                .refusals += direct.max(carried);
        }
    }
    p
}

/// The door's record (R7): per run record and day, the `fn` lines that answered exit 0.
fn door_lines(s: &impl System, since: &str) -> BTreeMap<String, BTreeMap<String, u64>> {
    let path = variable(s, "AP_DOOR_LOG").unwrap_or_else(|| DOOR_LOG.into());
    let mut out: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let Ok(bytes) = std::fs::read(path) else {
        return out;
    };
    for raw in String::from_utf8_lossy(&bytes).lines() {
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let (Some(run), Some(day)) = (
            v["run"].as_str(),
            v["at"].as_str().and_then(|a| a.get(..10)),
        ) else {
            continue;
        };
        if v["verb"] == "fn" && v["exit"] == 0 && day >= since {
            *out.entry(run.to_owned())
                .or_default()
                .entry(day.to_owned())
                .or_default() += 1;
        }
    }
    out
}

fn hex32(runtime: &str, session: &str, record: &str) -> String {
    let d = Sha256::digest(format!("{runtime}:{session}:{record}").as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect::<String>()[..32].to_owned()
}
fn hex16(p: &Value) -> String {
    let d = Sha256::digest(p.to_string().as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect::<String>()[..16].to_owned()
}
/// A `file_path` of the schema: `/` and 1 to 399 printable ASCII characters without a space.
fn file_path(p: &str) -> bool {
    p.len() <= 400
        && p.len() >= 2
        && p.starts_with('/')
        && p.bytes().all(|b| (b'!'..=b'~').contains(&b))
}
fn cut(t: &str, max: usize) -> String {
    let mut end = t.len().min(max);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_owned()
}
fn second(ts: &str) -> String {
    format!("{}Z", ts.get(..19).unwrap_or(ts))
}
fn day_value(day: &str, d: &Day) -> Value {
    json!({"day": day, "messages": d.messages, "usd_micros": d.usd_micros, "refusals": d.refusals,
        "file_reads": d.counts.file_reads, "file_writes": d.counts.file_writes,
        "store_reads": d.counts.store_reads, "store_writes": d.counts.store_writes})
}
/// The intent of a run the pass makes (R5): the intent whose `directory` is the cwd or its longest parent, else the
/// intent whose `lead_session` is the session.
fn intent_of(intents: &[store::Found], cwd: &str, session: &str) -> Option<String> {
    let by_dir = intents
        .iter()
        .filter_map(|(k, f)| Some((k, f["directory"].as_str()?)))
        .filter(|(_, d)| !d.is_empty() && under(cwd, d))
        .max_by_key(|(k, d)| {
            (
                d.trim_end_matches('/').len(),
                std::cmp::Reverse((*k).clone()),
            )
        });
    if let Some((k, _)) = by_dir {
        return Some(k.clone());
    }
    intents
        .iter()
        .find(|(_, f)| f["lead_session"] == session)
        .map(|(k, _)| k.clone())
}

#[derive(Default)]
struct Tally {
    recorded: u64,
    extended: u64,
    measured: u64,
    failed: BTreeMap<String, u64>,
}

/// The pass (R2 to R8): the answer line and its exit status.
pub(super) fn pass(s: &mut impl System, argv: &[OsString]) -> Result<(Value, u8), Failure> {
    let o = options(s, argv)?;
    let now = s
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let since_epoch = day_start(&o.since).unwrap_or(0);
    let st = o.store.as_deref();
    // The reads of the store come before any effect: a failed one ends the pass with nothing applied.
    let found = store::search(s, st, "run", vec![json!({"eq": ["runtime", RUNTIME]})])?;
    let intents = store::search_every(s, st, "intent", store::PAGE)?.0;
    let walked = session_files(&o.accounts);
    let mut skipped = walked.skipped;
    let stages = read_stages(&o.work_root, &o.stage_dirs, &mut skipped);
    let read = read_sessions(&walked.files, since_epoch, &o.since);
    let (files, unreadable) = (read.files, read.unreadable);
    let stage_paths: BTreeSet<String> = stages.iter().map(|s| s.path.clone()).collect();
    let mut p = assemble(&o, read, stages);
    let doors = door_lines(s, &o.since);
    // The store's runs by the key of the pass: `record` first, then the session of a run of state `session`.
    let mut by_key: BTreeMap<Key, (String, Value)> = BTreeMap::new();
    for (k, f) in found.iter().rev() {
        let key = match (
            f["record"].as_str(),
            f["state"] == "session",
            f["session"].as_str(),
        ) {
            (Some(r), _, _) => Key::Record(r.to_owned()),
            (None, true, Some(session)) => Key::Session(session.to_owned()),
            _ => continue,
        };
        by_key.insert(key, (k.clone(), f.clone()));
    }
    let actor_session = format!("measure-{now}");
    let actor = Actor {
        agent: "ap-measure",
        session: &actor_session,
        role: "measure",
    };
    let mut t = Tally::default();
    // The runs found that an action changed; every other one is `unchanged`.
    let mut changed: BTreeSet<String> = BTreeSet::new();
    let mut keys: BTreeSet<Key> = p.runs.keys().cloned().collect();
    keys.extend(doors.keys().map(|r| Key::Record(r.clone())));
    keys.extend(by_key.keys().cloned());
    for key in keys {
        let mine = p.runs.remove(&key);
        if mine.is_none() && p.skipped_route.contains(&key) {
            continue;
        }
        let existing = by_key.get(&key);
        let record = match &key {
            Key::Record(r) => r.clone(),
            Key::Session(_) => String::new(),
        };
        let session = mine
            .as_ref()
            .map(|r| r.session.clone())
            .or_else(|| existing.and_then(|(_, f)| f["session"].as_str().map(str::to_owned)))
            .unwrap_or_default();
        let base = hex32(RUNTIME, &session, &record);
        let mut days: BTreeMap<String, Day> =
            mine.as_ref().map(|r| r.days.clone()).unwrap_or_default();
        for (day, n) in doors.get(&record).into_iter().flatten() {
            days.entry(day.clone()).or_default().counts.store_reads += n;
        }
        let holds = mine.as_ref().is_some_and(|r| !r.messages.is_empty()) || !days.is_empty();
        let target: Value = match existing {
            Some((k, f)) => {
                let started = f["started_at"].as_str().unwrap_or_default();
                let fresh = f["measured_at"].is_null()
                    && started.get(..10).is_some_and(|d| d >= o.since.as_str())
                    && stage_paths.contains(&record);
                if !holds && !fresh {
                    continue;
                }
                // A session's run that grew: the later messages extend it.
                if let Some(run) = mine.as_ref().filter(|_| f["state"] == "session") {
                    let ended = f["ended_at"].as_str().unwrap_or_default().to_owned();
                    let later: Vec<&(String, [u64; KINDS])> = run
                        .messages
                        .iter()
                        .filter(|(ts, _)| second(ts) > ended)
                        .collect();
                    if let Some(last) = later.iter().map(|(ts, _)| second(ts)).max() {
                        let sum = |i: &[usize]| -> u64 {
                            later
                                .iter()
                                .map(|(_, tok)| i.iter().map(|k| tok[*k]).sum::<u64>())
                                .sum()
                        };
                        let stored = |name: &str| f[name].as_u64().unwrap_or(0);
                        let x = json!({"run": id("run", k), "ended_at": last,
                            "input_tokens": stored("input_tokens") + sum(&[0]),
                            "cache_read_tokens": stored("cache_read_tokens") + sum(&[2]),
                            "cache_write_tokens": stored("cache_write_tokens") + sum(&[3, 4]),
                            "output_tokens": stored("output_tokens") + sum(&[1])});
                        let rid = format!("measure-{base}-x-{}", hex16(&x));
                        match store::unit_action(s, st, &actor, "extend_session", rid, x) {
                            Ok(a) if a.outcome == "applied" => {
                                t.extended += 1;
                                changed.insert(k.clone());
                            }
                            Ok(_) => {}
                            Err(_) => *t.failed.entry("extend_session".into()).or_default() += 1,
                        }
                    }
                }
                id("run", k)
            }
            None => {
                let Some(run) = mine.as_ref().filter(|r| !r.messages.is_empty()) else {
                    continue;
                };
                let model = run
                    .models
                    .iter()
                    .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                    .map_or_else(|| "unknown".to_owned(), |(m, _)| m.clone());
                let first = run
                    .messages
                    .iter()
                    .map(|(ts, _)| second(ts))
                    .min()
                    .unwrap_or_default();
                let last = run
                    .messages
                    .iter()
                    .map(|(ts, _)| second(ts))
                    .max()
                    .unwrap_or_default();
                let sum = |i: &[usize]| -> u64 {
                    run.messages
                        .iter()
                        .map(|(_, tok)| i.iter().map(|k| tok[*k]).sum::<u64>())
                        .sum()
                };
                let mut x = json!({"session": cut(&run.session, 200), "role": cut(&run.role, 40),
                    "runtime": RUNTIME, "model": cut(&model, 80), "started_at": first, "ended_at": last,
                    "host": id("host", "local"),
                    "input_tokens": sum(&[0]), "cache_read_tokens": sum(&[2]),
                    "cache_write_tokens": sum(&[3, 4]), "output_tokens": sum(&[1])});
                if !run.account.is_empty() {
                    x["account"] = json!(cut(&run.account, 200));
                    x["uses"] = id("account", &run.account);
                }
                if file_path(&record) && run.stage_read {
                    x["record"] = json!(record);
                }
                if let Some(i) = intent_of(&intents, &run.cwd, &run.session) {
                    x["intent"] = id("intent", &i);
                }
                match store::unit_action(
                    s,
                    st,
                    &actor,
                    "record_session",
                    format!("measure-{base}"),
                    x,
                ) {
                    Ok(a) => match a.made("run") {
                        Some(made) => {
                            t.recorded += u64::from(a.outcome == "applied");
                            made.clone()
                        }
                        None => {
                            *t.failed.entry("record_session".into()).or_default() += 1;
                            continue;
                        }
                    },
                    Err(_) => {
                        *t.failed.entry("record_session".into()).or_default() += 1;
                        continue;
                    }
                }
            }
        };
        // The days: the stored entries before `--since`'s day kept, those from it on the pass's.
        let mut entries: Vec<Value> = existing
            .and_then(|(_, f)| f["days"].as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d["day"].as_str().is_some_and(|d| d < o.since.as_str()))
            .collect();
        entries.extend(days.iter().map(|(day, d)| day_value(day, d)));
        entries.sort_by(|a, b| a["day"].as_str().cmp(&b["day"].as_str()));
        let refusals: u64 = entries
            .iter()
            .map(|d| d["refusals"].as_u64().unwrap_or(0))
            .sum();
        let x = json!({"run": target, "days": entries, "refusals": refusals});
        let rid = format!("measure-{base}-{}", hex16(&x));
        match store::unit_action(s, st, &actor, "measure_run", rid, x) {
            Ok(a) if a.outcome == "applied" => {
                t.measured += 1;
                if let Some((k, _)) = existing {
                    changed.insert(k.clone());
                }
            }
            Ok(_) => {}
            Err(_) => *t.failed.entry("measure_run".into()).or_default() += 1,
        }
    }
    let unchanged = found.iter().filter(|(k, _)| !changed.contains(k)).count();
    let line = json!({"at": utc(now), "since": o.since,
        "files": {"read": files, "skipped_academy": skipped, "unreadable_lines": unreadable},
        "skipped_route": p.skipped_route.len(),
        "runs": {"recorded": t.recorded, "extended": t.extended, "measured": t.measured, "unchanged": unchanged},
        "failed": t.failed});
    let status = if t.failed.is_empty() { 0 } else { 3 };
    Ok((line, status))
}
