//! `ap resource`: one pass that observes this host and the hosts of the policy, writes one sample line per host,
//! writes the lines of `ap check` as records, observes each account's newest usage-limit reading and indexes the two
//! record sources; and the functions `capacity` and `gate` that answer from those objects (`specs/ap/resource.md`).
use crate::check::{self, Ctx, Io, Pc};
use crate::host::{Failure, Host, LIMIT, System, digest, error, fail, id};
use crate::route;
use crate::status;
use crate::store::{self, Actor, Found};
use crate::watch::{epoch, utc};
use crate::work;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

/// The directory the host store's sources `host_samples` and `checks` read (R1).
pub const DIRECTORY: &str = "/var/lib/agent-platform/resource";
/// The checks log of `specs/ap/check.md` R15 without `AP_CHECKS_LOG`.
const CHECKS_LOG: &str = "/var/lib/agent-platform/checks.jsonl";
/// The file of the checks' records in the directory (R7).
pub const CHECKS_FILE: &str = "log.checks.jsonl";
/// The slots of the dispatch pass without `work.slots` (`specs/ap/work.md` R3).
const SLOTS: u64 = 3;
const DAY: u64 = 86_400;
/// A samples file older than this many days is removed (R6).
const KEEP_DAYS: u64 = 14;
/// A check line whose `end` is older than this many days is not a record (R7).
const CHECK_DAYS: u64 = 30;
/// The window of the runs of an account without a `limit_at` (R8).
const RUN_DAYS: u64 = 7;
/// The newest records of the checks log that are written (R7).
pub const CHECKS_KEPT: usize = 50_000;
/// The longest record line the engine reads (`specs/ont/index.md` R19).
const RECORD_BYTES: usize = 65_536;
const COMMAND_BYTES: usize = 1_000;
const REASON_BYTES: usize = 40;
const NODE_BYTES: usize = 200;
/// A host observed longer ago than this is stale (R12).
const STALE_S: u64 = 600;
/// The probe of R4: the CPU count, the one-minute load and `MemAvailable` in kB, on one line, and nothing else.
const PROBE: &str = r#"printf '%s %s %s\n' "$(nproc)" "$(cut -d ' ' -f 1 /proc/loadavg)" "$(sed -n 's/^MemAvailable: *\([0-9][0-9]*\) kB$/\1/p' /proc/meminfo)""#;
const PROBE_LIMIT: Duration = Duration::from_secs(30);
/// How long the pass waits for a second later than the last sample line (R6).
const SECOND_WAIT: Duration = Duration::from_millis(100);
const SECOND_TRIES: usize = 20;
const INDEX_MS: u64 = 600_000;
/// One page of a search of the pass, and of each function (R11).
const PASS_MS: u64 = 30_000;
const CAPACITY_MS: u64 = 3_000;
const GATE_MS: u64 = 60_000;
/// Objects a page: a head of `host`, `account` or `host_sample` holds short strings and integers only (at most
/// about 4 KB with the field wrappers), so 200 fit an answer of 1 MiB; a `run` or a `check` head may be larger, so
/// they are read by the store's own page.
const SMALL_PAGE: u64 = 200;
const TEMPORARY_PREFIX: &str = ".ap-resource.";
const SAMPLES_SUFFIX: &str = ".samples.jsonl";
/// The thresholds of the gates (R13 to R16).
const MEMORY_MB: u64 = 2_500;
const LOAD_PER_CPU: f64 = 1.5;
const WAIT_SHARE: f64 = 0.10;
const ACCEPT_DROP: f64 = 0.15;
const UTILIZATION: f64 = 0.80;
const PC_RATIO: f64 = 0.50;
const MIN_SAMPLES: usize = 12;

/// The arguments of `ap resource` (R1).
#[derive(Debug, Default, Clone)]
pub struct Options {
    pub directory: Option<String>,
    pub store: Option<String>,
}

/// `[--directory <dir>] [--store <path>]` (R1); every refusal comes before any effect.
pub fn options(argv: &[OsString]) -> Result<Options, Failure> {
    let usage = || error("usage", "args");
    let mut o = Options::default();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].to_str().ok_or_else(usage)?;
        let (slot, field) = match flag {
            "--directory" => (&mut o.directory, "directory"),
            "--store" => (&mut o.store, "store"),
            _ => return Err(usage()),
        };
        let value = argv.get(i + 1).ok_or_else(usage)?;
        let value = value
            .to_str()
            .filter(|v| route::absolute(v))
            .ok_or_else(|| route::invalid(field, "is not an absolute path"))?;
        if slot.replace(value.to_owned()).is_some() {
            return Err(usage());
        }
        i += 2;
    }
    let default = o
        .directory
        .as_deref()
        .is_none_or(|d| d.trim_end_matches('/') == DIRECTORY);
    match (default, o.store.is_some()) {
        (true, true) => Err(route::invalid(
            "directory",
            "a scratch store needs a directory other than the host's",
        )),
        (false, false) => Err(route::invalid(
            "directory",
            "a directory other than the host's needs a scratch store",
        )),
        _ => Ok(o),
    }
}

/// What the pass reads of the policy (R1): the PC, the slots and the accounts (key, runtime).
#[derive(Default)]
struct Policy {
    pc: Option<Pc>,
    slots: u64,
    accounts: BTreeMap<String, String>,
}
fn policy_invalid(why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(why))
}
/// The last path component of an account directory, a final `/` taken off; None for `/`.
fn last_component(dir: &str) -> Option<String> {
    let bare = dir.trim_end_matches('/');
    bare.rsplit('/')
        .next()
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
}
/// The accounts of the policy's `accounts` (R1, R8): each distinct last path component with the runtime of the
/// first key, in byte order, whose list names it.
fn accounts(policy: &Value) -> Result<BTreeMap<String, String>, Failure> {
    let mut out = BTreeMap::new();
    let Some(a) = policy.get("accounts") else {
        return Ok(out);
    };
    let why = "accounts is not an object whose every value is an array of absolute paths";
    let map = a.as_object().ok_or_else(|| policy_invalid(why))?;
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    for key in keys {
        let list = map[key].as_array().ok_or_else(|| policy_invalid(why))?;
        for dir in list {
            let dir = dir
                .as_str()
                .filter(|d| route::absolute(d))
                .ok_or_else(|| policy_invalid(why))?;
            if let Some(name) = last_component(dir) {
                let runtime = key.split('/').next().unwrap_or(key);
                out.entry(name).or_insert_with(|| runtime.to_owned());
            }
        }
    }
    Ok(out)
}
fn policy(s: &impl System) -> Result<Policy, Failure> {
    let Some(v) = work::policy(s)? else {
        return Ok(Policy {
            slots: SLOTS,
            ..Policy::default()
        });
    };
    let pc = check::hosts(&v)?;
    work::work(&v)?;
    Ok(Policy {
        pc,
        slots: v["work"]["slots"].as_u64().unwrap_or(SLOTS),
        accounts: accounts(&v)?,
    })
}

/// The lock of the pass, let go by an explicit `LOCK_UN` before its descriptor closes (as `ap work`'s).
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
/// `<dir>/resource.lock`, taken without waiting (R2): None when another pass holds it.
fn lock(dir: &Path) -> Result<Option<Held>, Failure> {
    let io = || fail("io_error", "directory", json!("the lock"));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let file = options.open(dir.join("resource.lock")).map_err(|_| io())?;
    if file.metadata().map_err(|_| io())?.permissions().mode() & 0o7777 != 0o600 {
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| io())?;
    }
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(Held(file))),
        Err(e) if e == rustix::io::Errno::WOULDBLOCK => Ok(None),
        Err(_) => Err(io()),
    }
}

/// The three readings of a host (R3, R4).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Reading {
    cpus: u64,
    load_x100: u64,
    available_mb: u64,
}
fn load_x100(load: f64) -> u64 {
    (load * 100.0).round() as u64
}
/// This host's readings, each file read through the port (R3).
fn local_reading(s: &impl System) -> Result<Reading, Failure> {
    let read = |path: &str| {
        s.read(path)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| fail("reading_failed", path, Value::Null))
    };
    let bad = |path: &str| fail("reading_failed", path, Value::Null);
    let stat = read("/proc/stat")?;
    let cpus = stat
        .lines()
        .filter(|l| {
            l.strip_prefix("cpu")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        })
        .count() as u64;
    if cpus == 0 {
        return Err(bad("/proc/stat"));
    }
    let load = read("/proc/loadavg")?
        .split_whitespace()
        .next()
        .and_then(|w| w.parse::<f64>().ok())
        .filter(|l| l.is_finite() && *l >= 0.0)
        .ok_or_else(|| bad("/proc/loadavg"))?;
    let kb = read("/proc/meminfo")?
        .lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| bad("/proc/meminfo"))?;
    Ok(Reading {
        cpus,
        load_x100: load_x100(load),
        available_mb: kb / 1024,
    })
}

/// The output of the `tailscale` children of the pass goes nowhere: the pass reads their answers only.
struct Quiet;
impl Io for Quiet {
    fn out(&self, _: &[u8]) {}
    fn err(&self, _: &[u8]) {}
}
/// A host of the policy (R4): whether it answers, whether it is free, and its readings.
fn pc_reading(s: &impl System, pc: &Pc) -> (bool, bool, Option<Reading>) {
    let ctx = Ctx::of(Arc::new(s.environment()), Arc::new(Quiet));
    if !check::ping(&ctx, pc) {
        return (false, false, None);
    }
    let text = format!("bash -c {} ap-resource-probe", check::quote(PROBE));
    let Some(out) = check::ssh(&ctx, pc, &text, PROBE_LIMIT) else {
        return (false, false, None);
    };
    let text = String::from_utf8_lossy(&out);
    let Some(probe) = check::probe_line(text.lines().next().unwrap_or("")) else {
        return (false, false, None);
    };
    let free = check::placement(pc, &probe).is_none();
    let load = probe.load.parse::<f64>().unwrap_or(0.0);
    let reading = Reading {
        cpus: probe.cpus,
        load_x100: load_x100(load),
        available_mb: probe.wsl_mb,
    };
    (true, free, Some(reading))
}

/// The counts of the units of R3.
#[derive(Clone, Copy)]
struct Counts {
    running: u64,
    ready: u64,
    waits: u64,
}
fn counts(units: &[Found]) -> Counts {
    let state = |f: &Value, s: &str| f["state"] == s;
    let waits = |f: &Value| {
        f["waits_on"]
            .as_str()
            .is_some_and(|w| w == "cpu outside" || w.starts_with("cpu over "))
    };
    Counts {
        running: units.iter().filter(|(_, f)| state(f, "running")).count() as u64,
        ready: units.iter().filter(|(_, f)| state(f, "ready")).count() as u64,
        waits: units
            .iter()
            .filter(|(_, f)| state(f, "ready") && waits(f))
            .count() as u64,
    }
}

/// One host of the pass (R5, R6).
struct HostLine {
    key: String,
    node: String,
    answers: bool,
    free: bool,
    reading: Option<Reading>,
    /// For `local`: the slots and, when the search of R3 answered, the counts.
    local: Option<(u64, Option<Counts>)>,
}
impl HostLine {
    fn sample(&self, t: u64) -> Value {
        let mut line = json!({"type": "host_sample", "key": format!("{}@{t}", self.key),
            "observed_at": t, "at": utc(t), "host": self.key, "answers": self.answers, "free": self.free});
        if let Some(r) = self.reading {
            line["cpus"] = json!(r.cpus);
            line["load_x100"] = json!(r.load_x100);
            line["available_mb"] = json!(r.available_mb);
        }
        if let Some((slots, Some(c))) = self.local {
            line["slots"] = json!(slots);
            line["units_running"] = json!(c.running);
            line["units_ready"] = json!(c.ready);
            line["cpu_waits"] = json!(c.waits);
        }
        line
    }
}

/// A text cut at `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
/// The name of the samples file of the UTC day of `t` (R6).
fn samples_file(t: u64) -> String {
    let day: String = utc(t)[..10].chars().filter(|c| *c != '-').collect();
    format!("{day}{SAMPLES_SUFFIX}")
}
/// The day (days since 1970) of a samples file's name of the form `<YYYYMMDD>.samples.jsonl`; None for any other.
fn samples_day(name: &str) -> Option<u64> {
    let d = name.strip_suffix(SAMPLES_SUFFIX)?;
    if d.len() != 8 || !d.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    epoch(&format!("{}-{}-{}T00:00:00Z", &d[..4], &d[4..6], &d[6..])).map(|s| s / DAY)
}
/// The `observed_at` of the last line of a samples file, when it has one.
fn last_observed(path: &Path) -> Option<u64> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let line = text.lines().rev().find(|l| !l.trim().is_empty())?;
    serde_json::from_str::<Value>(line).ok()?["observed_at"].as_u64()
}
/// The pass's time (R6): a second later than the last line of the day's samples file, waited for at most 2 seconds;
/// with the second whether lines may be appended.
fn pass_time(s: &mut impl System, dir: &Path) -> (u64, bool) {
    let mut tries = 0;
    loop {
        let t = route::seconds(s);
        if last_observed(&dir.join(samples_file(t))).is_none_or(|last| last < t) {
            return (t, true);
        }
        if tries == SECOND_TRIES {
            return (t, false);
        }
        tries += 1;
        s.pause(SECOND_WAIT);
    }
}
/// The lines of the hosts appended by one write to the day's samples file, mode 0600, never through a link (R6).
fn append_samples(dir: &Path, t: u64, lines: &[Value]) -> std::io::Result<()> {
    let path = dir.join(samples_file(t));
    let mut f = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(&path)?;
    let meta = f.metadata()?;
    if !meta.is_file() {
        return Err(ErrorKind::InvalidInput.into());
    }
    if meta.permissions().mode() & 0o7777 != 0o600 {
        f.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut text = String::new();
    // A last line cut short by a write that failed is ended, so this pass's lines stand on their own.
    let mut bytes = vec![];
    File::open(&path)?.read_to_end(&mut bytes)?;
    if bytes.last().is_some_and(|b| *b != b'\n') {
        text.push('\n');
    }
    for l in lines {
        text.push_str(&l.to_string());
        text.push('\n');
    }
    f.write_all(text.as_bytes())
}
/// The samples files more than 14 days before the pass's day, removed (R6): their names.
fn remove_old(dir: &Path, t: u64) -> Vec<String> {
    let today = t / DAY;
    let mut removed = vec![];
    let Ok(entries) = fs::read_dir(dir) else {
        return removed;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in names {
        let old = samples_day(&name).is_some_and(|d| today.saturating_sub(d) > KEEP_DAYS);
        let regular = fs::symlink_metadata(dir.join(&name)).is_ok_and(|m| m.is_file());
        if old && regular && fs::remove_file(dir.join(&name)).is_ok() {
            removed.push(name);
        }
    }
    removed
}

/// The record of one line of the checks log (R7), with its `end` in seconds; None for a line the schema does not
/// take.
pub fn check_record(line: &[u8], t: u64) -> Option<(u64, Value)> {
    let v: Value = serde_json::from_slice(line).ok()?;
    let o = v.as_object()?;
    let time = |k: &str| o.get(k).and_then(Value::as_str).and_then(epoch);
    let end = time("end")?;
    if end > t || end + CHECK_DAYS * DAY < t {
        return None;
    }
    let host = o
        .get("host")
        .and_then(Value::as_str)
        .filter(|h| *h == "pc" || *h == "local")?;
    let text = |k: &str| o.get(k).and_then(Value::as_str);
    let (repository, head, dir) = (text("repository")?, text("head")?, text("dir")?);
    let words = o
        .get("command")?
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    let seconds = o
        .get("seconds")?
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.0)?;
    let ms = (seconds * 1000.0).round();
    if ms >= i64::MAX as f64 {
        return None;
    }
    let exit = o.get("exit")?.as_i64()?;
    let start = match o.get("start") {
        None | Some(Value::Null) => None,
        Some(s) => Some(s.as_str().filter(|s| epoch(s).is_some())?),
    };
    let reason = match o.get("reason") {
        None | Some(Value::Null) => None,
        Some(r) => Some(r.as_str().filter(|r| r.len() <= REASON_BYTES)?),
    };
    let key = digest(line)[..32].to_owned();
    let mut record = json!({"type": "check", "key": key, "observed_at": end, "host": host,
        "repository": repository, "head": head, "dir": dir, "command": cut(&words.join(" "), COMMAND_BYTES),
        "end": utc(end), "exit": exit, "milliseconds": ms as i64});
    if let Some(start) = start {
        record["start"] = json!(start);
    }
    if let Some(reason) = reason {
        record["reason"] = json!(reason);
    }
    (record.to_string().len() <= RECORD_BYTES).then_some((end, record))
}
/// The records of the checks log (R7): the kept records, newest `CHECKS_KEPT`, in the order of the log, and the
/// lines skipped.
pub fn check_records(log: &[u8], t: u64) -> (Vec<Value>, u64) {
    let mut seen = HashSet::new();
    let mut kept = vec![];
    let mut skipped = 0;
    let mut lines: Vec<&[u8]> = log.split(|b| *b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    for (i, line) in lines.into_iter().enumerate() {
        if !seen.insert(line) {
            continue;
        }
        match check_record(line, t) {
            Some((end, record)) => kept.push((end, i, record)),
            None => skipped += 1,
        }
    }
    if kept.len() > CHECKS_KEPT {
        kept.sort_by_key(|(end, i, _)| (*end, *i));
        kept.drain(..kept.len() - CHECKS_KEPT);
        kept.sort_by_key(|(_, i, _)| *i);
    }
    (kept.into_iter().map(|(_, _, r)| r).collect(), skipped)
}
/// The checks log's path (`specs/ap/check.md` R15), from the environment the port gives.
fn checks_log(s: &impl System) -> String {
    s.environment()
        .into_iter()
        .find(|(k, v)| k == "AP_CHECKS_LOG" && !v.is_empty())
        .map_or_else(|| CHECKS_LOG.to_owned(), |(_, v)| v)
}
/// R7: the records written to the directory, `{records, skipped}`; None when the log could not be read or the file
/// not written.
fn write_checks(s: &impl System, dir: &Path, t: u64) -> Option<Value> {
    let path = checks_log(s);
    let bytes = match s.read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => vec![],
        // A device such as /dev/null, which `ap check` may append to, holds no line.
        Err(_) if fs::metadata(&path).is_ok_and(|m| !m.is_dir() && !m.is_file()) => vec![],
        Err(_) => return None,
    };
    let (records, skipped) = check_records(&bytes, t);
    route::write_file(dir, TEMPORARY_PREFIX, CHECKS_FILE, &records).ok()?;
    Some(json!({"records": records.len(), "skipped": skipped}))
}

/// A usage-limit reading as `specs/ap/watch.md` R22 reads one: a JSON object with a `windows` object or a
/// `utilization` number.
fn limit_reading(limit: &Value) -> Option<Map<String, Value>> {
    let v: Value = serde_json::from_str(limit.as_str()?).ok()?;
    let o = v.as_object()?;
    (o.get("windows").is_some_and(Value::is_object)
        || o.get("utilization").is_some_and(Value::is_number))
    .then(|| o.clone())
}

/// One request id of the pass: its time, the action and the first 16 hexadecimal digits of the SHA-256 of the key.
fn rid(t: u64, action: &str, key: &str) -> String {
    format!("resource-{t}-{action}-{}", &digest(key.as_bytes())[..16])
}

/// One pass (R2 to R10): the answer line and whether no step failed.
pub fn pass(s: &mut impl System, o: &Options) -> Result<(Value, bool), Failure> {
    let policy = policy(s)?;
    let dir = PathBuf::from(o.directory.as_deref().unwrap_or(DIRECTORY));
    route::directory(&dir)?;
    let Some(_held) = lock(&dir)? else {
        return Ok((json!({"skipped": "pass_running"}), true));
    };
    let st = o.store.as_deref();
    let local = local_reading(s)?;
    let (t, appendable) = pass_time(s, &dir);
    let mut failed: Vec<String> = vec![];
    let units = store::search(
        s,
        st,
        "unit",
        vec![json!({"in": ["state", ["ready", "running"]]})],
    );
    let c = match units {
        Ok(units) => Some(counts(&units)),
        Err(_) => {
            failed.push("units".into());
            None
        }
    };
    let node = s
        .read("/proc/sys/kernel/hostname")
        .ok()
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "local".into());
    let mut hosts = vec![HostLine {
        key: "local".into(),
        node: cut(&node, NODE_BYTES).to_owned(),
        answers: true,
        free: c.is_some_and(|c| c.running < policy.slots),
        reading: Some(local),
        local: Some((policy.slots, c)),
    }];
    if let Some(pc) = &policy.pc {
        let (answers, free, reading) = pc_reading(s, pc);
        let node = pc.ssh.split_once('@').map_or("pc", |(_, h)| h);
        hosts.push(HostLine {
            key: "pc".into(),
            node: cut(node, NODE_BYTES).to_owned(),
            answers,
            free,
            reading,
            local: None,
        });
    }
    let session = format!("resource-{t}");
    let actor = Actor {
        agent: "ap-resource",
        session: &session,
        role: "measure",
    };
    let mut declared = vec![];
    for h in &hosts {
        let key = h.key.as_str();
        // Some(true) declared by this pass, Some(false) the store has it, None its read or its apply failed.
        let made = match store::get(s, st, "host", key) {
            Ok(Some(_)) => Some(false),
            Ok(None) => {
                let p = json!({"host": id("host", key), "node": h.node, "machine": key});
                let applied = store::declare_host(s, st, &actor, rid(t, "declare_host", key), p);
                applied.is_ok().then_some(true)
            }
            Err(_) => None,
        };
        if made.is_none() {
            failed.push(format!("declare_host:{key}"));
        }
        declared.push(made == Some(true));
        observe(s, st, &actor, h, t, &mut failed);
    }
    let samples: Vec<Value> = hosts.iter().map(|h| h.sample(t)).collect();
    if !appendable || append_samples(&dir, t, &samples).is_err() {
        failed.push("samples".into());
    }
    let removed = remove_old(&dir, t);
    let checks = write_checks(s, &dir, t);
    if checks.is_none() {
        failed.push("checks".into());
    }
    let mut accounts = vec![];
    for (key, runtime) in &policy.accounts {
        let observed = observe_account(s, st, &actor, (key, runtime), t);
        if observed.is_err() {
            failed.push(format!("observe_account:{key}"));
        }
        accounts.push(json!({"account": key, "observed": observed == Ok(true)}));
    }
    let names = ["host_samples", "checks"];
    let mut sources = Map::new();
    match store::index(s, st, &names, INDEX_MS) {
        Ok(v) => {
            let mut held = v["status"] == "ok";
            for name in names {
                let one = v["result"]["sources"]
                    .as_array()
                    .and_then(|a| a.iter().find(|x| x["source"] == name));
                held &= one.is_some_and(|x| x["status"] == "ok");
                sources.insert(
                    name.into(),
                    one.map_or(Value::Null, |x| {
                        json!({"status": x["status"], "code": x.get("code"), "appended": x["appended"],
                            "renewed": x["renewed"], "unchanged": x["unchanged"]})
                    }),
                );
            }
            if !held {
                failed.push("index".into());
            }
        }
        Err(_) => {
            for name in names {
                sources.insert(name.into(), Value::Null);
            }
            failed.push("index".into());
        }
    }
    let hosts: Vec<Value> = samples
        .into_iter()
        .zip(declared)
        .map(|(mut line, made)| {
            if let Some(m) = line.as_object_mut() {
                m.remove("type");
                m.insert("declared".into(), json!(made));
            }
            line
        })
        .collect();
    let ok = failed.is_empty();
    Ok((
        json!({"at": utc(t), "directory": dir.to_string_lossy(), "hosts": hosts, "accounts": accounts,
            "checks": checks.unwrap_or(json!({"records": null, "skipped": null})), "removed": removed,
            "sources": sources, "failed": failed}),
        ok,
    ))
}
/// `observe_host` of one host (R5).
fn observe(
    s: &mut impl System,
    st: Option<&str>,
    actor: &Actor,
    h: &HostLine,
    t: u64,
    failed: &mut Vec<String>,
) {
    let mut p = json!({"host": id("host", &h.key), "answers": h.answers, "free": h.free});
    if h.answers {
        p["seen_at"] = json!(utc(t));
    }
    if let Some(r) = h.reading {
        p["cpus"] = json!(r.cpus);
        p["load_x100"] = json!(r.load_x100);
        p["available_mb"] = json!(r.available_mb);
    }
    if store::observe_host(s, st, actor, rid(t, "observe_host", &h.key), p).is_err() {
        failed.push(format!("observe_host:{}", h.key));
    }
}
/// One account (R8): whether `observe_account` was applied; an error when a read or the apply failed.
fn observe_account(
    s: &mut impl System,
    st: Option<&str>,
    actor: &Actor,
    (key, runtime): (&str, &str),
    t: u64,
) -> Result<bool, ()> {
    let existing = store::get(s, st, "account", key).map_err(|_| ())?;
    let since = existing
        .as_ref()
        .and_then(|f| f["limit_at"].as_str())
        .filter(|a| epoch(a).is_some())
        .map_or_else(|| utc(t.saturating_sub(RUN_DAYS * DAY)), str::to_owned);
    let filter = json!({"all": [{"eq": ["account", key]}, {"gt": ["ended_at", since]}]});
    let (runs, _) =
        store::search_pages(s, st, ("run", Some(filter)), store::PAGE, PASS_MS).map_err(|_| ())?;
    let newest = runs
        .iter()
        .filter(|(_, f)| f["account"] == key && limit_reading(&f["limit"]).is_some())
        .filter_map(|(k, f)| {
            let ended = f["ended_at"].as_str().filter(|e| epoch(e).is_some())?;
            (ended > since.as_str()).then_some((ended, k, f))
        })
        .max_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    if existing.is_some() && newest.is_none() {
        return Ok(false);
    }
    let mut p = json!({"account": id("account", key), "runtime": runtime});
    if let Some((ended, run, f)) = newest {
        p["limit"] = f["limit"].clone();
        p["limit_at"] = json!(ended);
        p["limit_run"] = id("run", run);
    }
    store::observe_account(s, st, actor, rid(t, "observe_account", key), p).map_err(|_| ())?;
    Ok(true)
}

/// `ap resource <arguments>` (R1, R9): the answer line, exit 0 when no step failed, else 3; a refusal exit 2.
pub fn cli(argv: &[OsString]) -> ExitCode {
    let o = match options(argv) {
        Ok(o) => o,
        Err(e) => return route::print(&route::refusal(&e), false, 2),
    };
    match pass(&mut Host, &o) {
        Ok((v, ok)) => route::print(&v, true, if ok { 0 } else { 3 }),
        Err(e) if matches!(e.code, "usage" | "invalid_input" | "policy_invalid") => {
            route::print(&route::refusal(&e), false, 2)
        }
        Err(e) => route::print(&route::refusal(&e), false, 3),
    }
}

/// The parameters as one object; a member not in `allowed` is `invalid_input` with its name (R11).
fn parameters<'a>(p: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>, Failure> {
    let o = p
        .as_object()
        .ok_or_else(|| route::invalid("parameters", "is not an object"))?;
    if let Some(k) = o.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(route::invalid(k, "is not a parameter of this function"));
    }
    Ok(o)
}
fn seconds_of(v: &Value) -> Option<u64> {
    v.as_str().and_then(epoch)
}

/// The function `capacity` (R12) from the store at `store`.
pub fn capacity(s: &mut impl System, store: &str, p: &Value) -> Result<Value, Failure> {
    parameters(p, &[])?;
    let (hosts, now) =
        store::search_pages(s, Some(store), ("host", None), SMALL_PAGE, CAPACITY_MS)?;
    let (accounts, _) =
        store::search_pages(s, Some(store), ("account", None), SMALL_PAGE, CAPACITY_MS)?;
    // Schema version 24 on: one search of the active holds; a host that is the subject of one is held.
    let active = json!({"eq": ["status", "active"]});
    let (holds, _) = store::search_pages(
        s,
        Some(store),
        ("hold", Some(active)),
        SMALL_PAGE,
        CAPACITY_MS,
    )?;
    let held_hosts: BTreeSet<&str> = holds
        .iter()
        .filter(|(_, f)| f["subject"]["object_type"] == "host")
        .filter_map(|(_, f)| f["subject"]["native_key"].as_str())
        .collect();
    let at = now.ok_or_else(|| error("store_answer_unreadable", "now"))?;
    let mut hosts = hosts;
    hosts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut rows = vec![];
    let mut machines: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, (key, f)) in hosts.iter().enumerate() {
        let observed = seconds_of(&f["observed_at"]);
        let stale = observed.is_some_and(|o| o + STALE_S < at);
        let answers = match observed {
            Some(_) => f["answers"].clone(),
            None => Value::Null,
        };
        let held = held_hosts.contains(key.as_str());
        let free = if held {
            json!(false)
        } else {
            f["free"].clone()
        };
        rows.push(json!({"host": key, "node": f["node"], "machine": f["machine"], "answers": answers,
            "stale": stale, "observed_at": f["observed_at"], "seen_at": f["seen_at"], "cpus": f["cpus"],
            "load_x100": f["load_x100"], "available_mb": f["available_mb"], "free": free, "held": held}));
        if let Some(m) = f["machine"].as_str() {
            machines.entry(m.to_owned()).or_default().push(i);
        }
    }
    let mut out_machines = vec![];
    let mut can_start = vec![];
    for (machine, members) in machines {
        let live = members
            .iter()
            .copied()
            .filter(|i| rows[*i]["answers"] == true && rows[*i]["stale"] == false);
        // The latest seen_at, then the least key: one host's readings, so the machine is counted once.
        let seen = |i: usize| rows[i]["seen_at"].as_str().unwrap_or("").to_owned();
        let chosen = live.min_by(|a, b| {
            seen(*b)
                .cmp(&seen(*a))
                .then_with(|| hosts[*a].0.cmp(&hosts[*b].0))
        });
        let names: Vec<&str> = members.iter().map(|i| hosts[*i].0.as_str()).collect();
        let held = members.iter().any(|i| rows[*i]["held"] == true);
        let (up, free) = (
            chosen.is_some(),
            !held && chosen.is_some_and(|i| rows[i]["free"] == true),
        );
        let reading = |k: &str| chosen.map_or(Value::Null, |i| rows[i][k].clone());
        if free {
            can_start.push(machine.clone());
        }
        out_machines.push(json!({"machine": machine, "hosts": names, "up": up, "cpus": reading("cpus"),
            "load_x100": reading("load_x100"), "available_mb": reading("available_mb"), "free": free}));
    }
    let mut accounts = accounts;
    accounts.sort_by(|a, b| a.0.cmp(&b.0));
    let accounts: Vec<Value> = accounts
        .iter()
        .map(|(key, f)| {
            let (allowance, source, at) = match (f["limit"].as_str(), f["allowance"].as_str()) {
                (Some(limit), _) => (
                    serde_json::from_str(limit).unwrap_or_else(|_| json!(limit)),
                    json!("run"),
                    f["limit_at"].clone(),
                ),
                (None, Some(text)) => (json!(text), json!("stated"), f["allowance_at"].clone()),
                (None, None) => (Value::Null, Value::Null, Value::Null),
            };
            json!({"account": key, "runtime": f["runtime"], "allowance": allowance, "source": source, "at": at})
        })
        .collect();
    Ok(
        json!({"at": utc(at), "hosts": rows, "machines": out_machines, "accounts": accounts,
        "can_start": can_start}),
    )
}

/// One row of `gate`.
fn row(gate: &str, verdict: &str, text: String) -> Value {
    json!({"gate": gate, "verdict": verdict, "text": text})
}
fn verdict(pass: bool) -> &'static str {
    if pass { "PASS" } else { "FAIL" }
}
/// A row as the script printed it (R13): the verdict left-aligned in six columns, a space and the text.
pub fn row_line(row: &Value) -> String {
    format!(
        "{:<6} {}",
        row["verdict"].as_str().unwrap_or(""),
        row["text"].as_str().unwrap_or("")
    )
}
/// The median of `specs/ap/status.md` R40: the mean of the two middle values for an even count.
fn median(mut v: Vec<f64>) -> Option<f64> {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(v[n / 2]),
        _ => Some((v[n / 2 - 1] + v[n / 2]) / 2.0),
    }
}
/// G1 to G3 and INFO (R13) over the samples of `local`.
fn gates_samples(samples: &[&Value], since: &str, rows: &mut Vec<Value>) {
    let n = samples.len();
    if n < MIN_SAMPLES {
        rows.push(row(
            "G1-G3",
            "N.A.",
            format!("G1-G3 need >= {MIN_SAMPLES} samples since {since} (have {n})"),
        ));
        return;
    }
    let m = samples
        .iter()
        .filter_map(|f| f["available_mb"].as_u64())
        .min()
        .unwrap_or(0);
    rows.push(row(
        "G1",
        verdict(m >= MEMORY_MB),
        format!("G1 free memory min {m} MB >= {MEMORY_MB}"),
    ));
    let mut loads: Vec<f64> = samples
        .iter()
        .filter_map(|f| {
            let cpus = f["cpus"].as_u64().filter(|c| *c > 0)?;
            Some(f["load_x100"].as_u64()? as f64 / (100.0 * cpus as f64))
        })
        .collect();
    loads.sort_by(f64::total_cmp);
    let x = match loads.len() {
        0 => 0.0,
        k => loads[(0.9 * (k - 1) as f64).floor() as usize],
    };
    rows.push(row(
        "G2",
        verdict(x <= LOAD_PER_CPU),
        format!("G2 load1 p90 per CPU {x:.2} <= {LOAD_PER_CPU:.2}"),
    ));
    // The samples with the counts of R3: (slots, units running, units ready, cpu waits).
    type Counted = (u64, u64, u64, u64);
    let counted: Vec<Counted> = samples
        .iter()
        .filter_map(|f| {
            Some((
                f["slots"].as_u64()?,
                f["units_running"].as_u64()?,
                f["units_ready"].as_u64()?,
                f["cpu_waits"].as_u64()?,
            ))
        })
        .collect();
    if counted.is_empty() {
        rows.push(row("G3", "N.A.", "G3 needs a sample with counts".into()));
        return;
    }
    let share = |test: &dyn Fn(&Counted) -> bool| {
        counted.iter().filter(|c| test(c)).count() as f64 / counted.len() as f64
    };
    let p = share(&|c| c.3 > 0);
    rows.push(row(
        "G3",
        verdict(p <= WAIT_SHARE),
        format!(
            "G3 samples with a cpu wait {:.0}% <= {:.0}%",
            100.0 * p,
            100.0 * WAIT_SHARE
        ),
    ));
    let full = share(&|c| c.1 >= c.0);
    let idle = share(&|c| c.1 < c.0 && c.2 == 0);
    rows.push(row(
        "INFO",
        "INFO",
        format!(
            "INFO slots full in {:.0}% of samples; a slot idle with nothing ready in {:.0}%",
            100.0 * full,
            100.0 * idle
        ),
    ));
}
/// G4 and G5 (R14) from the rows of `ap status --indicators`.
fn gates_units(units: &[Value], since: u64, since_text: &str, rows: &mut Vec<Value>) {
    let merged = |u: &&Value| seconds_of(&u["merged_at"]);
    let a: Vec<&Value> = units
        .iter()
        .filter(|u| merged(u).is_some_and(|m| m >= since))
        .collect();
    let b: Vec<&Value> = units
        .iter()
        .filter(|u| merged(u).is_some_and(|m| m < since))
        .collect();
    let share = |v: &[&Value]| {
        v.iter().filter(|u| u["first_verdict"] == "accept").count() as f64 / v.len() as f64
    };
    if !a.is_empty() && !b.is_empty() {
        let (sa, sb) = (share(&a), share(&b));
        rows.push(row(
            "G4",
            verdict(sb - sa <= ACCEPT_DROP),
            format!(
                "G4 first-accept share {sa:.2} vs {sb:.2} before ({} / {} units), drop <= {ACCEPT_DROP:.2}",
                a.len(),
                b.len()
            ),
        ));
    } else {
        rows.push(row(
            "G4",
            "N.A.",
            format!("G4 first-accept share needs merged units after {since_text}"),
        ));
    }
    if !a.is_empty() {
        let k = a
            .iter()
            .flat_map(|u| u["stops"].as_array().into_iter().flatten())
            .filter(|s| *s == "check_failed")
            .count();
        rows.push(row(
            "G5",
            "INFO",
            format!(
                "G5 units merged in the window: {}; check_failed stops: {k}",
                a.len()
            ),
        ));
    }
}
/// G6 (R15) from the runs that ended in the window.
fn gate_limits(runs: &[Found], since: u64, rows: &mut Vec<Value>) {
    let mut bad = 0;
    let mut u: f64 = 0.0;
    for (_, f) in runs {
        if seconds_of(&f["ended_at"]).is_none_or(|e| e < since) {
            continue;
        }
        let Some(reading) = limit_reading(&f["limit"]) else {
            continue;
        };
        let windows: Vec<&Value> = reading
            .get("windows")
            .and_then(Value::as_object)
            .map(|w| w.values().collect())
            .unwrap_or_default();
        let own = Value::Object(reading.clone());
        let all: Vec<&Value> = std::iter::once(&own).chain(windows).collect();
        let refused = all.iter().any(|r| {
            r["status"]
                .as_str()
                .is_some_and(|s| s != "allowed" && s != "allowed_warning")
        });
        bad += usize::from(refused);
        for r in all {
            if let Some(x) = r["utilization"].as_f64().filter(|x| x.is_finite()) {
                u = u.max(x);
            }
        }
    }
    rows.push(row(
        "G6",
        verdict(bad == 0 && u <= UTILIZATION),
        format!(
            "G6 account rate limit: refused/limited runs {bad} == 0, utilization {u:.2} <= {UTILIZATION:.2}"
        ),
    ));
}
/// G7 (R16) from the checks of the window that exited 0.
fn gate_checks(checks: &[Found], since: u64, since_text: &str, rows: &mut Vec<Value>) {
    let mut pairs: BTreeMap<(String, String), (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for (_, f) in checks {
        if seconds_of(&f["end"]).is_none_or(|e| e < since) || f["exit"] != 0 {
            continue;
        }
        let (Some(dir), Some(command), Some(ms)) = (
            f["dir"].as_str(),
            f["command"].as_str(),
            f["milliseconds"].as_f64(),
        ) else {
            continue;
        };
        let host = f["host"]["native_key"].as_str().or(f["host"].as_str());
        let entry = pairs
            .entry((dir.to_owned(), command.to_owned()))
            .or_default();
        match host {
            Some("pc") => entry.0.push(ms),
            Some("local") => entry.1.push(ms),
            _ => {}
        }
    }
    let ratios: Vec<f64> = pairs
        .into_values()
        .filter_map(|(pc, here)| Some(median(pc)? / median(here)?))
        .collect();
    let n = ratios.len();
    match median(ratios) {
        Some(r) => rows.push(row(
            "G7",
            verdict(r <= PC_RATIO),
            format!(
                "G7 check seconds on the PC / on this host: median ratio {r:.2} over {n} worktrees <= {PC_RATIO:.2}"
            ),
        )),
        None => rows.push(row(
            "G7",
            "N.A.",
            format!("G7 needs a worktree checked on both hosts since {since_text}"),
        )),
    }
}

/// The function `gate` (R13 to R16) from the store at `store`.
pub fn gate(s: &mut impl System, store: &str, p: &Value) -> Result<Value, Failure> {
    let o = parameters(p, &["since", "intent"])?;
    let since_text = o
        .get("since")
        .and_then(Value::as_str)
        .filter(|t| epoch(t).is_some())
        .ok_or_else(|| route::invalid("since", "is not a UTC time YYYY-MM-DDTHH:MM:SSZ"))?
        .to_owned();
    let since = epoch(&since_text).unwrap_or(0);
    let intent = match o.get("intent") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_str()
                .filter(|i| (1..=512).contains(&i.len()))
                .ok_or_else(|| route::invalid("intent", "is not a key of 1 to 512 bytes"))?
                .to_owned(),
        ),
    };
    let local = id("host", "local");
    let filter = json!({"all": [{"eq": ["host", local]}, {"ge": ["at", since_text]}]});
    let (samples, now) = store::search_pages(
        s,
        Some(store),
        ("host_sample", Some(filter)),
        SMALL_PAGE,
        GATE_MS,
    )?;
    let at = now.ok_or_else(|| error("store_answer_unreadable", "now"))?;
    let mut samples: Vec<&Found> = samples
        .iter()
        .filter(|(_, f)| f["host"]["native_key"] == "local")
        .filter(|(_, f)| seconds_of(&f["at"]).is_some_and(|a| a >= since))
        .collect();
    samples.sort_by(|a, b| a.0.cmp(&b.0));
    let fields: Vec<&Value> = samples.iter().map(|(_, f)| f).collect();
    let mut rows = vec![];
    gates_samples(&fields, &since_text, &mut rows);
    let indicators = status::execute(s, Some(store), &status::Call::Indicators(intent))?;
    let units = indicators["units"].as_array().cloned().unwrap_or_default();
    gates_units(&units, since, &since_text, &mut rows);
    let filter = json!({"ge": ["ended_at", since_text]});
    let (runs, _) =
        store::search_pages(s, Some(store), ("run", Some(filter)), store::PAGE, GATE_MS)?;
    gate_limits(&runs, since, &mut rows);
    let filter = json!({"all": [{"ge": ["end", since_text]}, {"eq": ["exit", 0]}]});
    let (checks, _) = store::search_pages(
        s,
        Some(store),
        ("check", Some(filter)),
        store::PAGE,
        GATE_MS,
    )?;
    gate_checks(&checks, since, &since_text, &mut rows);
    Ok(json!({"since": since_text, "at": utc(at), "samples": fields.len(), "rows": rows}))
}

/// `ap` with no argument and `ONT_FN_NAME` `capacity` or `gate` (R11): the parameters on standard input, the answer
/// on standard output; a refusal on standard error with exit 2, a store it cannot read exit 3.
pub fn function(name: &str) -> ExitCode {
    let result = (|| {
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error("io_error", "stdin"))?;
        if bytes.len() as u64 > LIMIT {
            return Err(error("input_too_large", "stdin"));
        }
        let p: Value =
            serde_json::from_slice(&bytes).map_err(|_| route::invalid("stdin", "is not JSON"))?;
        let store = std::env::var("ONT_FN_STORE")
            .ok()
            .filter(|v| route::absolute(v))
            .ok_or_else(|| route::invalid("ONT_FN_STORE", "is not an absolute path"))?;
        match name {
            "capacity" => capacity(&mut Host, &store, &p),
            _ => gate(&mut Host, &store, &p),
        }
    })();
    match result {
        Ok(v) => route::print(&v, true, 0),
        Err(e)
            if matches!(
                e.code,
                "invalid_input" | "input_too_large" | "unknown_intent"
            ) =>
        {
            route::print(&route::refusal(&e), false, 2)
        }
        Err(e) => route::print(&route::refusal(&e), false, 3),
    }
}

/// The names of the samples files a directory holds, for a reader of the pass (tests among them).
pub fn samples_files(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| samples_day(n).is_some())
        .collect()
}
