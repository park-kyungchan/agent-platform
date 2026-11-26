//! `ap api batch` and the function `api_spend` (`specs/ap/api.md`): the one door of the Claude API credit. A
//! request file is admitted by its worst case against the caps of the policy's member `api`, reserved as a `run` of
//! role `batch`, submitted as one Message Batch through `curl` with the key in a header file, polled, and settled
//! by the results' `usage`. The prices are the rows of `evals/prices.tsv`, included when `ap` is built.
use crate::host::{Failure, Host, System, digest, error, fail, id};
use crate::store;
use crate::watch::{epoch, utc};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, UNIX_EPOCH},
};

/// The price file of the commit `ap` is built from (R10, R14): read at build time, never copied.
const PRICES: &str = include_str!("../../../evals/prices.tsv");
pub const HELP: &str = "Usage: ap api batch <job> --requests <file> --out <dir> [--queue standard|soak] [--store <path>]\nAdmits a request file against the job's cap, the day's cap and the month's stop of the policy's member api by its worst case at the batch price (the token-counting endpoint's input count, max_tokens at the output price, half of evals/prices.tsv), reserves it as a run of role batch, submits one Message Batch through curl with the key in a header file, polls it, writes <out>/results.jsonl and settles the run; one JSON line. ap with no argument and ONT_FN_NAME=api_spend answers the function api_spend of the store (specs/ap/api.md).\n";
const BASE: &str = "https://api.anthropic.com/v1/messages/";
const CREDENTIALS: &str = "/etc/agent-platform/credentials/";
const RUN_DIRECTORY: &str = "/run/agent-platform";
const MAX_REQUEST_BYTES: u64 = 200_000_000;
const LOCK_WAIT: Duration = Duration::from_secs(300);
const LOCK_STEP: Duration = Duration::from_secs(1);
const POLL_EVERY: Duration = Duration::from_secs(60);
const POLL_FOR: Duration = Duration::from_secs(25 * 3_600);
/// A rate as read: micro-USD per token times `SCALE`, so that a rate of up to six decimals is an integer.
const SCALE: u128 = 1_000_000;
const MICROS: i64 = 1_000_000;
const PARAMS: [&str; 8] = [
    "model",
    "max_tokens",
    "system",
    "messages",
    "output_config",
    "tools",
    "tool_choice",
    "thinking",
];

/// A path examined without following a link (R4).
#[derive(Clone, Copy, Debug)]
pub struct Examined {
    pub link: bool,
    pub regular: bool,
    pub uid: u32,
    pub mode: u32,
    /// The effective uid of this process, as `/proc/self/status` gives it.
    pub euid: Option<u32>,
}
/// The second number of the line `Uid:` of a `/proc/<pid>/status` text: the effective uid.
pub fn effective_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}
/// The real examination: `lstat` of the path and the effective uid of `/proc/self/status`.
pub fn examine_path(path: &str) -> std::io::Result<Examined> {
    let m = fs::symlink_metadata(path)?;
    let status = fs::read_to_string("/proc/self/status").ok();
    Ok(Examined {
        link: m.file_type().is_symlink(),
        regular: m.file_type().is_file(),
        uid: m.uid(),
        mode: m.mode() & 0o7777,
        euid: status.as_deref().and_then(effective_uid),
    })
}
/// The port of the credit (R4): the host's port with the one examination of the key file.
pub trait Api: System {
    fn examine(&self, path: &str) -> std::io::Result<Examined> {
        examine_path(path)
    }
}
impl Api for Host {}

/// A failure and the exit status it ends the call with.
#[derive(Debug)]
pub struct Stop {
    pub failure: Failure,
    pub status: u8,
}
fn refuse(f: Failure) -> Stop {
    Stop {
        failure: f,
        status: 2,
    }
}
fn broken(f: Failure) -> Stop {
    Stop {
        failure: f,
        status: 3,
    }
}

// The prices (R10, R14).

/// A rate of the file in `SCALE` units; None for another form.
fn scaled(text: &str) -> Option<u128> {
    let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || frac.len() > 6 || !digits(whole) || !digits(frac) {
        return None;
    }
    let frac = format!("{frac:0<6}");
    Some(whole.parse::<u128>().ok()? * SCALE + frac.parse::<u128>().ok()?)
}
/// The value of the row `model`, `kind`.
fn row(model: &str, kind: &str) -> Option<&'static str> {
    PRICES.lines().find_map(|l| {
        let mut f = l.split('\t');
        (f.next() == Some(model) && f.next() == Some(kind))
            .then(|| f.next())
            .flatten()
    })
}
fn rate(model: &str, kind: &str) -> Option<u128> {
    row(model, kind).and_then(scaled)
}
/// The rates of a model, `+long` when the prompt passes its `long_over_tokens` row.
#[derive(Clone, Copy)]
struct Rates {
    input: u128,
    output: u128,
    cache_read: u128,
    write_5m: u128,
    write_1h: u128,
}
fn rates(model: &str, prompt: u64) -> Option<Rates> {
    let over = row(model, "long_over_tokens").and_then(|t| t.parse::<u64>().ok());
    let long = format!("{model}+long");
    let name = match over {
        Some(t) if prompt > t && rate(&long, "input").is_some() => long.as_str(),
        _ => model,
    };
    let (input, output) = (rate(name, "input")?, rate(name, "output")?);
    // A kind without a row is priced at the model's output rate, its highest.
    let or = |kind: &str| rate(name, kind).unwrap_or(output);
    Some(Rates {
        input,
        output,
        cache_read: or("cache_read"),
        write_5m: or("cache_write_5m"),
        write_1h: or("cache_write_1h"),
    })
}
/// Half of a sum of tokens times rates, in whole micro-USD rounded up.
fn half_up(sum: u128) -> i64 {
    let micros = sum.div_ceil(2 * SCALE);
    i64::try_from(micros).unwrap_or(i64::MAX)
}
/// Whether the model has an `input` and an `output` row (R3).
pub fn priced(model: &str) -> bool {
    rate(model, "input").is_some() && rate(model, "output").is_some()
}

// The policy's member `api` (R3).

#[derive(Clone, Debug)]
pub struct Job {
    pub cap_usd: u64,
    pub models: Vec<String>,
    pub max_tokens: u64,
    pub max_requests: u64,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub key_file: String,
    pub account: String,
    pub standard: u64,
    pub soak: u64,
    pub day_cap_usd: u64,
    pub jobs: BTreeMap<String, Job>,
}
fn invalid_policy(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
fn job_form(j: &str) -> bool {
    (1..=40).contains(&j.len())
        && j.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
/// A Claude API model identifier: `claude-` and 1 to 57 of `[a-z0-9.-]`.
fn model_form(m: &str) -> bool {
    m.strip_prefix("claude-").is_some_and(|rest| {
        (1..=57).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
    })
}
/// An object with exactly the members `names`.
fn exactly<'a>(v: &'a Value, at: &str, names: &[&str]) -> Result<&'a Map<String, Value>, Failure> {
    let o = v
        .as_object()
        .ok_or_else(|| invalid_policy(at, "is not an object"))?;
    if let Some(k) = o.keys().find(|k| !names.contains(&k.as_str())) {
        return Err(invalid_policy(
            at,
            &format!("holds the member {k}, which is not read"),
        ));
    }
    if let Some(k) = names.iter().find(|k| !o.contains_key(**k)) {
        return Err(invalid_policy(&format!("{at}.{k}"), "is missing"));
    }
    Ok(o)
}
fn whole(v: &Value, at: &str, low: u64, high: u64) -> Result<u64, Failure> {
    v.as_u64()
        .filter(|n| (low..=high).contains(n))
        .ok_or_else(|| invalid_policy(at, &format!("is not an integer from {low} to {high}")))
}
/// The member `api` of the parsed policy (R3), None when the policy has none. The reader `api` of
/// `ap policy check` calls it; it makes no file system call.
pub fn config(policy: &Value) -> Result<Option<Config>, Failure> {
    let Some(api) = policy.get("api") else {
        return Ok(None);
    };
    let o = exactly(
        api,
        "api",
        &["key_file", "stops_usd", "day_cap_usd", "jobs"],
    )?;
    let key_file = o["key_file"].as_str().unwrap_or_default();
    let name = key_file.strip_prefix(CREDENTIALS).unwrap_or_default();
    if !credentials_file(key_file) {
        return Err(invalid_policy(
            "api.key_file",
            "is not a path directly under /etc/agent-platform/credentials/",
        ));
    }
    let stops = exactly(&o["stops_usd"], "api.stops_usd", &["standard", "soak"])?;
    let standard = whole(&stops["standard"], "api.stops_usd.standard", 1, 1000)?;
    let soak = whole(&stops["soak"], "api.stops_usd.soak", 1, 1000)?;
    if standard > soak {
        return Err(invalid_policy("api.stops_usd", "has standard above soak"));
    }
    let day_cap_usd = whole(&o["day_cap_usd"], "api.day_cap_usd", 1, 1000)?;
    let all = o["jobs"]
        .as_object()
        .filter(|j| (1..=32).contains(&j.len()))
        .ok_or_else(|| invalid_policy("api.jobs", "is not an object of 1 to 32 jobs"))?;
    let mut jobs = BTreeMap::new();
    for (key, v) in all {
        if !job_form(key) {
            return Err(invalid_policy(
                "api.jobs",
                &format!("names the job {key}, which is not 1 to 40 of a-z0-9_-"),
            ));
        }
        let at = format!("api.jobs.{key}");
        let j = exactly(v, &at, &["cap_usd", "models", "max_tokens", "max_requests"])?;
        let cap_usd = whole(&j["cap_usd"], &format!("{at}.cap_usd"), 1, 1000)?;
        let bad_models = || {
            invalid_policy(
                &format!("{at}.models"),
                "is not 1 to 8 distinct Claude API model identifiers with an input and an output price",
            )
        };
        let list = j["models"]
            .as_array()
            .filter(|m| (1..=8).contains(&m.len()))
            .ok_or_else(bad_models)?;
        let mut models: Vec<String> = vec![];
        for m in list {
            let m = m.as_str().filter(|m| model_form(m) && priced(m));
            match m {
                Some(m) if !models.iter().any(|x| x == m) => models.push(m.to_owned()),
                _ => return Err(bad_models()),
            }
        }
        let max_tokens = whole(&j["max_tokens"], &format!("{at}.max_tokens"), 1, 128_000)?;
        let max_requests = whole(
            &j["max_requests"],
            &format!("{at}.max_requests"),
            1,
            100_000,
        )?;
        jobs.insert(
            key.clone(),
            Job {
                cap_usd,
                models,
                max_tokens,
                max_requests,
            },
        );
    }
    Ok(Some(Config {
        key_file: key_file.to_owned(),
        account: name.to_owned(),
        standard,
        soak,
        day_cap_usd,
        jobs,
    }))
}

// The arguments (R1, R2).

#[derive(Debug)]
struct Args {
    job: String,
    requests: String,
    out: String,
    soak: bool,
    store: Option<String>,
}
fn usage() -> Failure {
    error("usage", "args")
}
fn arguments(argv: &[OsString]) -> Result<Args, Failure> {
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    let words = words.ok_or_else(usage)?;
    let Some((&"batch", rest)) = words.split_first() else {
        return Err(usage());
    };
    let mut job = None;
    let mut options: BTreeMap<&str, &str> = BTreeMap::new();
    let mut i = 0;
    while i < rest.len() {
        let w = rest[i];
        if let Some(name) = w.strip_prefix("--") {
            if !["requests", "out", "queue", "store"].contains(&name) {
                return Err(usage());
            }
            let value = rest.get(i + 1).ok_or_else(usage)?;
            if options.insert(name, value).is_some() {
                return Err(usage());
            }
            i += 2;
        } else {
            if job.replace(w).is_some() {
                return Err(usage());
            }
            i += 1;
        }
    }
    let job = job.ok_or_else(usage)?;
    let (Some(requests), Some(out)) = (options.get("requests"), options.get("out")) else {
        return Err(usage());
    };
    if !job_form(job) {
        return Err(error("invalid_input", "job"));
    }
    let absolute = |p: &str| Path::new(p).is_absolute();
    for (name, value) in &options {
        let ok = match *name {
            "queue" => ["standard", "soak"].contains(value),
            _ => absolute(value),
        };
        if !ok {
            return Err(error("invalid_input", name));
        }
    }
    Ok(Args {
        job: job.to_owned(),
        requests: (*requests).to_owned(),
        out: (*out).to_owned(),
        soak: options.get("queue") == Some(&"soak"),
        store: options.get("store").map(|s| (*s).to_owned()),
    })
}
/// A path of the Academy (R2): a component that begins with `education-`, `edu-` or `academy-`, or `evals` then `lesson`.
pub fn academy(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts
        .iter()
        .any(|c| c.starts_with("education-") || c.starts_with("edu-") || c.starts_with("academy-"))
        || parts.windows(2).any(|w| w == ["evals", "lesson"])
}

// The key (R4).

fn key_unusable(why: &str) -> Failure {
    fail("key_unusable", "key_file", json!(why))
}
/// Whether `path` has the form of a key file (R3): an absolute path directly under
/// `/etc/agent-platform/credentials/`, exactly one component more, not `.` or `..`, no NUL byte.
pub fn credentials_file(path: &str) -> bool {
    let name = path.strip_prefix(CREDENTIALS).unwrap_or_default();
    !(name.is_empty() || name.contains('/') || name == "." || name == ".." || name.contains('\0'))
}
fn key(s: &impl Api, path: &str) -> Result<String, Failure> {
    key_by(s, path, |p| s.examine(p))
}
/// The one rule of R4 for a key file, with the examination the caller's port gives: `ap api batch`
/// reads its key with `Api::examine`, a key launch of `ap run` (`specs/ap/run.md` R54) with
/// `System::examine_key`. The read is the port's `read`.
pub fn key_by(
    s: &impl System,
    path: &str,
    examine: impl FnOnce(&str) -> std::io::Result<Examined>,
) -> Result<String, Failure> {
    let e = match examine(path) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(key_unusable("not_found")),
        Err(_) => return Err(key_unusable("not_regular_file")),
    };
    if e.link || !e.regular {
        return Err(key_unusable("not_regular_file"));
    }
    if e.uid != 0 && Some(e.uid) != e.euid {
        return Err(key_unusable("owner_not_trusted"));
    }
    if e.mode & 0o077 != 0 {
        return Err(key_unusable("group_or_other_access"));
    }
    let bytes = s.read(path).map_err(|_| key_unusable("not_found"))?;
    let line = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
    let fits = (1..=256).contains(&line.len()) && line.iter().all(|b| (0x21..=0x7e).contains(b));
    match fits {
        true => String::from_utf8(line.to_vec()).map_err(|_| key_unusable("not_one_line")),
        false => Err(key_unusable("not_one_line")),
    }
}

// The request file (R5).

struct Request {
    custom_id: String,
    line: Value,
    params: Value,
    max_tokens: u64,
}
fn line_invalid(line: usize, reason: &str) -> Failure {
    fail(
        "request_invalid",
        "requests",
        json!({"line": line, "reason": reason}),
    )
}
fn read_requests(path: &str) -> Result<Vec<u8>, Failure> {
    let unreadable = || fail("invalid_input", "requests", json!("cannot be read"));
    let file = File::open(path).map_err(|_| unreadable())?;
    let m = file.metadata().map_err(|_| unreadable())?;
    if !m.is_file() {
        return Err(unreadable());
    }
    if m.len() > MAX_REQUEST_BYTES {
        return Err(error("requests_too_large", "requests"));
    }
    let mut bytes = vec![];
    file.take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err(error("requests_too_large", "requests"));
    }
    Ok(bytes)
}
fn custom_id_form(c: &str) -> bool {
    (1..=64).contains(&c.len())
        && c.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn requests(bytes: &[u8]) -> Result<Vec<Request>, Failure> {
    let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut seen = BTreeSet::new();
    let mut all = vec![];
    for (i, text) in lines.iter().enumerate() {
        let n = i + 1;
        let v: Value = serde_json::from_slice(text).map_err(|_| line_invalid(n, "not_json"))?;
        let o = v
            .as_object()
            .ok_or_else(|| line_invalid(n, "not_an_object"))?;
        if o.len() != 2 || !o.contains_key("custom_id") || !o.contains_key("params") {
            return Err(line_invalid(n, "members"));
        }
        let custom_id = o["custom_id"]
            .as_str()
            .filter(|c| custom_id_form(c))
            .ok_or_else(|| line_invalid(n, "custom_id"))?;
        if !seen.insert(custom_id.to_owned()) {
            return Err(line_invalid(n, "custom_id_repeated"));
        }
        let p = o["params"]
            .as_object()
            .ok_or_else(|| line_invalid(n, "params"))?;
        if p.keys().any(|k| !PARAMS.contains(&k.as_str())) {
            return Err(line_invalid(n, "params_member"));
        }
        if !p.get("model").is_some_and(Value::is_string) {
            return Err(line_invalid(n, "model"));
        }
        let max_tokens = p
            .get("max_tokens")
            .and_then(Value::as_u64)
            .filter(|m| *m >= 1)
            .ok_or_else(|| line_invalid(n, "max_tokens"))?;
        if !p.get("messages").is_some_and(Value::is_array) {
            return Err(line_invalid(n, "messages"));
        }
        all.push(Request {
            custom_id: custom_id.to_owned(),
            params: o["params"].clone(),
            line: v.clone(),
            max_tokens,
        });
    }
    Ok(all)
}
fn model_of(r: &Request) -> &str {
    r.params["model"].as_str().unwrap_or_default()
}
/// Whether a value holds a member `cache_control` at any depth (R10).
fn cached(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.contains_key("cache_control") || o.values().any(cached),
        Value::Array(a) => a.iter().any(cached),
        _ => false,
    }
}

// The spend (R9).

/// What a period counts by job: settled, reserved, runs and running runs.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Tally {
    pub runs: u64,
    pub running: u64,
    pub settled: i64,
    pub reserved: i64,
}
impl Tally {
    pub fn counted(&self) -> i64 {
        self.settled.saturating_add(self.reserved)
    }
    fn add(&mut self, fields: &Value) {
        self.runs += 1;
        if fields["status"] != "closed" {
            self.running += 1;
        }
        match (
            fields["status"] == "closed",
            fields["settled_usd_micros"].as_i64(),
        ) {
            (true, Some(settled)) => self.settled = self.settled.saturating_add(settled),
            _ => {
                let reserved = fields["reserved_usd_micros"].as_i64().unwrap_or(0);
                self.reserved = self.reserved.saturating_add(reserved);
            }
        }
    }
}
/// The fields of every run of role `batch` started at `from` or later (seconds since 1970): one search whose
/// filter names only `role` and `started_at`, every page read through `store::search` (R9).
pub(crate) fn batch_runs(
    s: &mut impl System,
    store: Option<&str>,
    from: u64,
) -> Result<Vec<Value>, Failure> {
    let filter = vec![
        json!({"eq": ["role", "batch"]}),
        json!({"gt": ["started_at", utc(from.saturating_sub(1))]}),
    ];
    Ok(store::search(s, store, "run", filter)?
        .into_iter()
        .map(|(_, fields)| fields)
        .collect())
}
/// The runs started in `[from, to)`, counted by job (R9): one rule for the admission and `api_spend`.
pub fn tally(runs: &[Value], (from, to): (u64, u64)) -> BTreeMap<String, Tally> {
    let mut by_job: BTreeMap<String, Tally> = BTreeMap::new();
    for fields in runs {
        let at = fields["started_at"].as_str().and_then(epoch);
        if !at.is_some_and(|t| from <= t && t < to) {
            continue;
        }
        let job = fields["job"].as_str().unwrap_or_default().to_owned();
        by_job.entry(job).or_default().add(fields);
    }
    by_job
}
/// The UTC calendar month `[start, end)` of a month `YYYY-MM`.
fn month_bounds(year: i64, month: i64) -> Option<(u64, u64)> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let start = epoch(&format!("{year:04}-{month:02}-01T00:00:00Z"))?;
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = epoch(&format!("{ny:04}-{nm:02}-01T00:00:00Z"))?;
    Some((start, end))
}
pub(crate) fn month_of(secs: u64) -> Option<(u64, u64)> {
    let t = utc(secs);
    month_bounds(t.get(0..4)?.parse().ok()?, t.get(5..7)?.parse().ok()?)
}
pub(crate) fn sum(by_job: &BTreeMap<String, Tally>) -> i64 {
    by_job
        .values()
        .map(Tally::counted)
        .fold(0, i64::saturating_add)
}

// The lock and the run directory (R8).

/// The lock, let go by an explicit `LOCK_UN` before its descriptor closes (as `ap work`'s).
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
fn run_directory(s: &impl System) -> Result<PathBuf, Stop> {
    let named = s
        .environment()
        .into_iter()
        .find(|(k, v)| k == "AP_API_RUN" && !v.is_empty())
        .map(|(_, v)| v);
    let dir = PathBuf::from(named.unwrap_or_else(|| RUN_DIRECTORY.to_owned()));
    let invalid = || broken(error("invalid_input", "run"));
    if !dir.is_absolute() {
        return Err(invalid());
    }
    match fs::symlink_metadata(&dir) {
        Ok(m) if m.file_type().is_dir() => Ok(dir),
        Ok(_) => Err(invalid()),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&dir)
                .map_err(|_| broken(fail("io_error", "run", json!("cannot be made"))))?;
            Ok(dir)
        }
        Err(_) => Err(invalid()),
    }
}
fn lock(s: &mut impl System, dir: &Path) -> Result<Held, Stop> {
    let io = || broken(fail("io_error", "run", json!("the lock")));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let file = options.open(dir.join("api.lock")).map_err(|_| io())?;
    let began = s.now();
    loop {
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(Held(file)),
            Err(e) if e == rustix::io::Errno::WOULDBLOCK => {}
            Err(_) => return Err(io()),
        }
        if s.now().duration_since(began) >= LOCK_WAIT {
            return Err(broken(error("lock_busy", "run")));
        }
        s.pause(LOCK_STEP);
    }
}
/// The pid of a key directory's name `key.<pid>.<n>`.
fn key_pid(name: &str) -> Option<u32> {
    let (pid, n) = name.strip_prefix("key.")?.split_once('.')?;
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    (digits(pid) && digits(n))
        .then(|| pid.parse().ok())
        .flatten()
}
/// Under the lock: the key directories of processes that are gone (R8).
fn sweep(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().and_then(key_pid) else {
            continue;
        };
        if !Path::new(&format!("/proc/{pid}")).exists() {
            let path = e.path();
            let _ = fs::remove_file(path.join("header"));
            let _ = fs::remove_dir(&path);
        }
    }
}

// The calls (R11).

/// A key directory and its header file, removed when dropped: on every end of its call.
struct KeyDir(PathBuf);
impl Drop for KeyDir {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("header"));
        let _ = fs::remove_dir(&self.0);
    }
}
fn key_dir(run: &Path, key: &str, n: &mut u64) -> Option<KeyDir> {
    let pid = std::process::id();
    for _ in 0..1_000 {
        let path = run.join(format!("key.{pid}.{n}"));
        *n += 1;
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
        let held = KeyDir(path);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(held.0.join("header"))
            .ok()?;
        file.write_all(format!("x-api-key: {key}\n").as_bytes())
            .ok()?;
        return Some(held);
    }
    None
}
/// One call of the API: its URL after the base, its body (null for a GET), a file for the answer's body, and
/// its bound in seconds.
struct Call<'a> {
    path: String,
    body: Value,
    output: Option<&'a Path>,
    seconds: u64,
}
/// The HTTP status and the body of an answer; None for no answer.
type Heard = Option<(u16, Vec<u8>)>;
struct Line<'a> {
    run: &'a Path,
    key: &'a str,
    n: u64,
}
impl Line<'_> {
    fn call(&mut self, s: &mut impl System, c: Call) -> Heard {
        let held = key_dir(self.run, self.key, &mut self.n)?;
        let header = held.0.join("header");
        let mut a: Vec<String> = [
            "-q",
            "-sS",
            "--proto",
            "=https",
            "--max-time",
            &c.seconds.to_string(),
            "-H",
            "anthropic-version: 2026-10-21",
            "-H",
            "content-type: application/json",
        ]
        .iter()
        .map(|w| (*w).to_owned())
        .collect();
        a.push("-H".into());
        a.push(format!("@{}", header.to_str()?));
        if !c.body.is_null() {
            a.extend(["-X", "POST", "--data-binary", "@-"].map(str::to_owned));
        }
        if let Some(out) = c.output {
            a.push("-o".into());
            a.push(out.to_str()?.to_owned());
        }
        a.extend(["--write-out", "\n%{http_code}"].map(str::to_owned));
        a.push(format!("{BASE}{}", c.path));
        let reply = s.call("curl", &a, &c.body, c.seconds * 1_000 + 10_000);
        drop(held);
        let reply = reply.ok()?;
        let out = reply.stdout;
        let at = out.iter().rposition(|b| *b == b'\n')?;
        let status: u16 = std::str::from_utf8(&out[at + 1..])
            .ok()?
            .trim()
            .parse()
            .ok()?;
        (status != 0).then(|| (status, out[..at].to_vec()))
    }
}
fn batch_id_form(i: &str) -> bool {
    (1..=128).contains(&i.len()) && i.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

// The call (R1 to R16).

/// `<out>/batch.json` (R10): written whole, mode 0600, at each step.
struct Record {
    path: PathBuf,
    value: Value,
}
impl Record {
    fn state(&mut self, state: &str) -> Result<(), Stop> {
        self.value["state"] = json!(state);
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true).mode(0o600);
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        let written = options
            .open(&self.path)
            .and_then(|mut f| f.write_all(format!("{}\n", self.value).as_bytes()));
        written.map_err(|_| broken(fail("io_error", "out", json!("batch.json"))))
    }
}
/// One `close_run` with its one repeat when it has no answer (R12, R14).
fn close(s: &mut impl System, store: Option<&str>, rid: &str, p: &Value) -> bool {
    for _ in 0..2 {
        match store::close_run_api(s, store, (rid, "batch", p)) {
            Ok(_) => return true,
            Err(f) if f.code == store::REFUSED => return false,
            Err(_) => {}
        }
    }
    false
}
/// The start (R10): the run's key, or None for `store_failed`.
fn start(s: &mut impl System, store: Option<&str>, rid: &str, p: &Value) -> Option<String> {
    let key = |a: store::Applied| {
        a.made("run")
            .and_then(|i| i["native_key"].as_str())
            .map(str::to_owned)
    };
    match store::start_run_api(s, store, (rid, "batch", p)) {
        Ok(a) if a.outcome == "applied" => key(a),
        Ok(_) => None,
        Err(f) if f.code == store::REFUSED => None,
        Err(_) => store::start_run_api(s, store, (rid, "batch", p))
            .ok()
            .and_then(key),
    }
}

/// `ap api batch` with the arguments after `api`: the answer of R15, or the failure and its exit.
pub fn batch(s: &mut impl Api, argv: &[OsString]) -> Result<Value, Stop> {
    // R1, R2: the arguments, then the Academy, before any file system call.
    let a = arguments(argv).map_err(refuse)?;
    for (name, path) in [
        ("requests", Some(&a.requests)),
        ("out", Some(&a.out)),
        ("store", a.store.as_ref()),
    ] {
        if path.is_some_and(|p| academy(p)) {
            return Err(refuse(error("academy_path", name)));
        }
    }
    // R3: the policy's member.
    let not_configured = || refuse(error("api_not_configured", "policy"));
    let policy = crate::work::policy(&*s)
        .map_err(refuse)?
        .ok_or_else(not_configured)?;
    let config = config(&policy)
        .map_err(refuse)?
        .ok_or_else(not_configured)?;
    let job = config
        .jobs
        .get(&a.job)
        .ok_or_else(|| refuse(error("unknown_job", "job")))?
        .clone();
    // R4, R5, R6.
    let key = key(&*s, &config.key_file).map_err(refuse)?;
    let all = requests(&read_requests(&a.requests).map_err(refuse)?).map_err(refuse)?;
    if all.is_empty() {
        return Err(refuse(error("no_requests", "requests")));
    }
    if all.len() as u64 > job.max_requests {
        return Err(refuse(error("over_max_requests", "requests")));
    }
    let models: BTreeSet<&str> = all.iter().map(model_of).collect();
    if models.len() > 1 {
        return Err(refuse(error("models_mixed", "model")));
    }
    let model = model_of(&all[0]).to_owned();
    if !job.models.contains(&model) {
        return Err(refuse(error("model_not_allowed", "model")));
    }
    if all.iter().any(|r| r.max_tokens > job.max_tokens) {
        return Err(refuse(error("over_max_tokens", "max_tokens")));
    }
    let out = PathBuf::from(&a.out);
    if fs::symlink_metadata(&out).is_ok() {
        return Err(refuse(error("out_exists", "out")));
    }
    if !out.parent().is_some_and(|p| p.is_dir()) {
        return Err(refuse(error("invalid_input", "out")));
    }
    let base = rates(&model, 0).ok_or_else(|| refuse(error("model_not_allowed", "model")))?;
    let store = a.store.as_deref();
    let began = s.wall().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = began.as_secs();
    // R8: the run directory, the lock, the key directories of gone processes.
    let run = run_directory(&*s)?;
    let held = lock(s, &run)?;
    sweep(&run);
    // R9: the month and the day.
    let failed = |f: Failure| broken(fail("store_failed", "store", json!(store::code_of(&f))));
    let month = month_of(secs).ok_or_else(|| broken(error("io_error", "clock")))?;
    let runs = batch_runs(s, store, month.0).map_err(failed)?;
    let by_job = tally(&runs, month);
    let day_from = secs - secs % 86_400;
    let day = sum(&tally(&runs, (day_from, day_from + 86_400)));
    let job_month = by_job.get(&a.job).map_or(0, Tally::counted);
    let month_spent = sum(&by_job);
    let stop = if a.soak { config.soak } else { config.standard };
    let caps = [
        ("job", job.cap_usd, job_month),
        ("day", config.day_cap_usd, day),
        ("month", stop, month_spent),
    ];
    let admit = |amount: i64| -> Result<(), Stop> {
        for (name, cap, counted) in caps {
            let cap = i64::try_from(cap)
                .unwrap_or(i64::MAX)
                .saturating_mul(MICROS);
            if counted.saturating_add(amount) > cap {
                return Err(refuse(fail(
                    "budget_exceeded",
                    name,
                    json!({"cap_usd_micros": cap, "counted_usd_micros": counted, "worst_usd_micros": amount}),
                )));
            }
        }
        Ok(())
    };
    // R10: the output part, before any call; then one count per request; then the whole.
    let worst_out = all
        .iter()
        .map(|r| half_up(u128::from(r.max_tokens) * base.output))
        .fold(0, i64::saturating_add);
    admit(worst_out)?;
    let mut line = Line {
        run: &run,
        key: &key,
        n: 0,
    };
    let mut worst = 0i64;
    for (i, r) in all.iter().enumerate() {
        let mut body = r.params.clone();
        if let Some(o) = body.as_object_mut() {
            o.remove("max_tokens");
        }
        let call = Call {
            path: "count_tokens".into(),
            body,
            output: None,
            seconds: 60,
        };
        let count_failed = || broken(fail("count_failed", "requests", json!({"line": i + 1})));
        let n = match line.call(s, call) {
            Some((200, bytes)) => serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v["input_tokens"].as_u64())
                .ok_or_else(count_failed)?,
            _ => return Err(count_failed()),
        };
        let rt = rates(&model, n).ok_or_else(count_failed)?;
        let r_in = if cached(&r.params) {
            rt.write_1h
        } else {
            rt.input
        };
        let amount = half_up(u128::from(n) * r_in + u128::from(r.max_tokens) * rt.output);
        worst = worst.saturating_add(amount);
    }
    admit(worst)?;
    // The reservation.
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&out)
        .map_err(|_| broken(fail("io_error", "out", json!("cannot be made"))))?;
    let rid = format!(
        "api-{}",
        &digest(format!("{}\n{}\n{}", a.out, began.as_nanos(), std::process::id()).as_bytes())
            [..32]
    );
    let record_path = out.join("batch.json");
    let mut record = Record {
        path: record_path.clone(),
        value: json!({"job": a.job, "run": null, "model": model, "requests": all.len(),
            "reserved_usd_micros": worst, "batch": null, "state": null}),
    };
    let mut params = json!({"role": "batch", "runtime": "api", "model": model,
        "account": config.account, "uses": id("account", &config.account),
        "host": id("host", "local"), "job": a.job,
        "reserved_usd_micros": worst});
    let record_text = record_path.to_str().unwrap_or_default();
    if record_text.len() <= 400
        && record_text
            .bytes()
            .skip(1)
            .all(|b| (0x21..=0x7e).contains(&b))
    {
        params["record"] = json!(record_text);
    }
    let started = start(s, store, &format!("{rid}-start"), &params);
    let Some(run_key) = started else {
        record.state("store_failed")?;
        return Err(broken(error("store_failed", "store")));
    };
    record.value["run"] = json!(run_key);
    record.state("reserved")?;
    drop(held);
    let close_id = format!("{rid}-close");
    let run_id = id("run", &run_key);
    // R12: the submit, once.
    let lines: Vec<Value> = all.iter().map(|r| r.line.clone()).collect();
    let submit = Call {
        path: "batches".into(),
        body: json!({ "requests": lines }),
        output: None,
        seconds: 600,
    };
    let batch = match line.call(s, submit) {
        Some((200, bytes)) => serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
            v["id"]
                .as_str()
                .filter(|i| batch_id_form(i))
                .map(str::to_owned)
        }),
        Some((status @ 400..=499, _)) => {
            let p = json!({"run": run_id, "state": "submit_refused", "settled_usd_micros": 0});
            if !close(s, store, &close_id, &p) {
                record.state("close_failed")?;
                return Err(broken(error("close_failed", "store")));
            }
            record.state("submit_refused")?;
            return Err(broken(fail(
                "submit_refused",
                "requests",
                json!({"status": status}),
            )));
        }
        _ => None,
    };
    let Some(batch) = batch else {
        record.state("submit_uncertain")?;
        return Err(broken(error("submit_uncertain", "requests")));
    };
    record.value["batch"] = json!(batch);
    record.state("submitted")?;
    // R13: the poll, then the results.
    let submitted = s.now();
    loop {
        s.pause(POLL_EVERY);
        let poll = Call {
            path: format!("batches/{batch}"),
            body: Value::Null,
            output: None,
            seconds: 60,
        };
        let ended = matches!(line.call(s, poll), Some((200, bytes))
            if serde_json::from_slice::<Value>(&bytes).is_ok_and(|v| v["processing_status"] == "ended"));
        if ended {
            break;
        }
        if s.now().duration_since(submitted) >= POLL_FOR {
            record.state("poll_timeout")?;
            return Err(broken(error("poll_timeout", "batch")));
        }
    }
    let results_path = out.join("results.jsonl");
    let unreadable = |record: &mut Record| -> Stop {
        match record.state("results_unreadable") {
            Ok(()) => broken(error("results_unreadable", "results")),
            Err(stop) => stop,
        }
    };
    let made = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&results_path);
    if made.is_err() {
        return Err(unreadable(&mut record));
    }
    let get = Call {
        path: format!("batches/{batch}/results"),
        body: Value::Null,
        output: Some(&results_path),
        seconds: 600,
    };
    if !matches!(line.call(s, get), Some((200, _))) {
        return Err(unreadable(&mut record));
    }
    let Some(results_text) = results_path.to_str() else {
        return Err(unreadable(&mut record));
    };
    let Ok(bytes) = s.read(results_text) else {
        return Err(unreadable(&mut record));
    };
    let Some(settled) = settle(&bytes, &all, &model) else {
        return Err(unreadable(&mut record));
    };
    // R14: the close.
    let p = json!({"run": run_id, "state": "ended", "session": batch,
        "input_tokens": settled.tokens[0], "cache_read_tokens": settled.tokens[1],
        "cache_write_tokens": settled.tokens[2], "output_tokens": settled.tokens[3],
        "settled_usd_micros": settled.amount});
    if !close(s, store, &close_id, &p) {
        record.state("close_failed")?;
        return Err(broken(error("close_failed", "store")));
    }
    record.state("settled")?;
    Ok(
        json!({"job": a.job, "run": run_key, "batch": batch, "requests": all.len(),
        "results": settled.results, "stop_reasons": settled.stop_reasons,
        "reserved_usd_micros": worst, "settled_usd_micros": settled.amount, "out": a.out}),
    )
}
/// The results of a batch, settled (R13, R14).
struct Settled {
    results: Value,
    stop_reasons: Map<String, Value>,
    tokens: [u64; 4],
    amount: i64,
}
fn settle(bytes: &[u8], all: &[Request], model: &str) -> Option<Settled> {
    let ids: BTreeSet<&str> = all.iter().map(|r| r.custom_id.as_str()).collect();
    let mut seen = BTreeSet::new();
    let mut counts: BTreeMap<&str, u64> = ["succeeded", "errored", "canceled", "expired"]
        .into_iter()
        .map(|k| (k, 0))
        .collect();
    let mut stop_reasons = Map::new();
    let mut tokens = [0u64; 4];
    let mut amount = 0i64;
    for text in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let v: Value = serde_json::from_slice(text).ok()?;
        let custom = v["custom_id"].as_str().filter(|c| ids.contains(c))?;
        if !seen.insert(custom.to_owned()) {
            return None;
        }
        let kind = v["result"]["type"].as_str()?;
        *counts.get_mut(kind)? += 1;
        if kind != "succeeded" {
            continue;
        }
        let message = &v["result"]["message"];
        if let Some(reason) = message["stop_reason"].as_str() {
            let c = stop_reasons.entry(reason).or_insert(json!(0));
            *c = json!(c.as_u64().unwrap_or(0) + 1);
        }
        let u = &message["usage"];
        let n = |k: &str| u[k].as_u64().unwrap_or(0);
        let (input, read, write, output) = (
            n("input_tokens"),
            n("cache_read_input_tokens"),
            n("cache_creation_input_tokens"),
            n("output_tokens"),
        );
        let c1 = u["cache_creation"]["ephemeral_1h_input_tokens"]
            .as_u64()
            .unwrap_or(0);
        let c5 = write.saturating_sub(c1);
        let r = rates(model, input.saturating_add(read).saturating_add(write))?;
        let sum = u128::from(input) * r.input
            + u128::from(read) * r.cache_read
            + u128::from(c5) * r.write_5m
            + u128::from(c1) * r.write_1h
            + u128::from(output) * r.output;
        amount = amount.saturating_add(half_up(sum));
        for (t, add) in tokens.iter_mut().zip([input, read, write, output]) {
            *t = t.saturating_add(add);
        }
    }
    if seen.len() != ids.len() {
        return None;
    }
    Some(Settled {
        results: json!(counts),
        stop_reasons,
        tokens,
        amount,
    })
}

/// `ap api <arguments>`.
pub fn cli(argv: &[OsString]) -> ExitCode {
    let help = matches!(argv, [w] if w == "--help")
        || matches!(argv, [b, h] if b == "batch" && h == "--help");
    if help {
        return match std::io::stdout().write_all(HELP.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(4),
        };
    }
    match batch(&mut Host, argv) {
        Ok(v) => crate::route::print(&v, true, 0),
        Err(stop) => crate::route::print(&crate::route::refusal(&stop.failure), false, stop.status),
    }
}

// The function (R17).

fn invalid(field: &str) -> Failure {
    error("invalid_input", field)
}
/// `api_spend` over the store `store` with the parameters `p`.
pub fn spend(s: &mut impl System, store: &str, p: &Value) -> Result<Value, Failure> {
    let o = p.as_object().ok_or_else(|| invalid("stdin"))?;
    if let Some(k) = o.keys().find(|k| *k != "month") {
        return Err(invalid(k));
    }
    let month = o.get("month").and_then(Value::as_str).unwrap_or_default();
    let b = month.as_bytes();
    let shape = b.len() == 7
        && b[4] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || c.is_ascii_digit());
    let bounds = shape
        .then(|| month_bounds(month[0..4].parse().ok()?, month[5..7].parse().ok()?))
        .flatten()
        .ok_or_else(|| invalid("month"))?;
    let by_job = tally(&batch_runs(s, Some(store), bounds.0)?, bounds);
    let mut total = Tally::default();
    let jobs: Vec<Value> = by_job
        .iter()
        .map(|(job, t)| {
            total.settled = total.settled.saturating_add(t.settled);
            total.reserved = total.reserved.saturating_add(t.reserved);
            json!({"job": job, "runs": t.runs, "running": t.running,
                "settled_usd_micros": t.settled, "reserved_usd_micros": t.reserved,
                "counted_usd_micros": t.counted()})
        })
        .collect();
    Ok(
        json!({"month": month, "jobs": jobs, "settled_usd_micros": total.settled,
        "reserved_usd_micros": total.reserved, "counted_usd_micros": total.counted()}),
    )
}
/// `ap` with no argument and `ONT_FN_NAME=api_spend`.
pub fn function() -> ExitCode {
    crate::route::function_of(spend)
}
