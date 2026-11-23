//! `ap watch` (`specs/ap/watch.md`): one pass of the ten detectors under the lock `pass.lock` of
//! the watch directory. Each detector reads its sources and yields signals, or is `unreadable`; per
//! signal the pass opens, sees again or reopens the finding of its key, and per detector that read
//! it closes each live finding of that detector it did not yield (`open_finding`,
//! `see_finding_again`, `reopen_finding`, `close_finding`, agent `ap-watch`). A finding whose
//! detector does not begin with `watch:` is never read for closing and never changed.
//! `ap watch --record-login` records the login reading the detector `login` compares with.
pub mod accounts;
pub mod checks;
pub mod credit;
pub mod detect;
pub mod drift;
pub mod login;
pub mod runtimes;

use crate::host::{Failure, Host, LIMIT, System, digest, fail, id, new_session_id};
use crate::run::stamp;
use crate::store::{self, Actor};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::File,
    io::{ErrorKind, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    process::ExitCode,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The watch directory of the host (R1); a test passes its own.
pub const DIRECTORY: &str = "/var/lib/agent-platform/watch";
/// The detectors in the order of a pass (R4, R20).
pub const DETECTORS: [&str; 10] = [
    "delivery", "login", "services", "host", "records", "accounts", "checks", "runtimes", "drift",
    "credit",
];
/// Fixed CLI subcommands and flags `ap run` invokes per runtime (R26).
pub struct RuntimeSpec {
    pub name: &'static str,
    pub subcommands: &'static [&'static str],
    pub flags: &'static [&'static str],
}
/// The runtime CLIs `runtimes` checks (never `codex`).
pub const RUNTIMES: [RuntimeSpec; 3] = [
    RuntimeSpec {
        name: "claude",
        subcommands: &[],
        flags: &[
            "--print",
            "--output-format",
            "--verbose",
            "--model",
            "--effort",
            "--permission-mode",
            "--permission-prompts",
            "--setting-sources",
            "--settings",
            "--disallowedTools",
            "--strict-mcp-config",
            "--mcp-config",
            "--disable-slash-commands",
            "--json-schema",
            "--resume",
            "--session-id",
        ],
    },
    RuntimeSpec {
        name: "muse",
        subcommands: &["exec"],
        flags: &[
            "--json",
            "--model",
            "--reasoning-effort",
            "--contributor-mode",
            "--disable-reminders",
            "--session-id",
            "--workspace",
            "--prompt-file",
            "--yolo",
        ],
    },
    RuntimeSpec {
        name: "agy",
        subcommands: &[],
        flags: &[
            "--print",
            "--output-format",
            "--model",
            "--dangerously-skip-permissions",
            "--print-timeout",
            "--conversation",
        ],
    },
];
/// A live finding is seen again when its `last_seen_at` is older than this (R4).
const SEEN_AFTER: u64 = 57 * 60;
/// The bounds of a summary and of an evidence (R9).
const SUMMARY: usize = 600;
const EVIDENCE: usize = 2_000;

/// One signal of a detector: its signature (unique within the detector), severity, summary (which
/// begins with the signature) and evidence.
#[derive(Debug, Clone)]
pub(crate) struct Signal {
    pub(crate) signature: String,
    pub(crate) severity: &'static str,
    pub(crate) summary: String,
    pub(crate) evidence: String,
}
impl Signal {
    pub(crate) fn new(
        signature: String,
        severity: &'static str,
        what: &str,
        evidence: String,
    ) -> Self {
        Signal {
            summary: format!("{signature}: {what}"),
            signature,
            severity,
            evidence,
        }
    }
}
/// What a detector that read its sources yields: its signals, the families its sources did not hold
/// (their findings are neither seen nor closed) and its member of `state.json` after the pass.
#[derive(Debug, Default)]
pub(crate) struct Read {
    pub(crate) signals: Vec<Signal>,
    pub(crate) unknown: Vec<String>,
    pub(crate) state: Option<Value>,
}
/// A detector's reading, or the cause its sources could not be read.
pub(crate) type Reading = Result<Read, String>;

/// The families of a detector (R5): the signature prefixes `specs/ap/watch.md` lists, with
/// `unreadable`.
pub fn families(detector: &str) -> Vec<String> {
    let fixed: &[&str] = match detector {
        "delivery" => &["stale", "end", "checkout"],
        "login" => &["no baseline"],
        "services" => &["failed", "timer"],
        "host" => &[
            "disk floor",
            "slice memory high",
            "apt",
            "old cli",
            "pc down",
        ],
        "records" => &["backup stale", "backup copy stale"],
        "accounts" => &["sign-in", "limit"],
        "checks" => &["main red", "dependabot", "settings"],
        "runtimes" => &["cli"],
        "drift" => &["drift", "inventory"],
        "credit" => &credit::FAMILIES,
        _ => &[],
    };
    let mut all: Vec<String> = fixed.iter().map(|f| (*f).to_owned()).collect();
    if detector == "login" {
        all.extend(login::PARTS.iter().map(|p| format!("login {p}")));
    }
    all.push("unreadable".to_owned());
    all
}
/// The family of a finding of `detector`: the longest family that is a prefix of its summary.
pub fn family(detector: &str, summary: &str) -> Option<String> {
    let all = families(detector).into_iter();
    all.filter(|f| summary.starts_with(f.as_str()))
        .max_by_key(String::len)
}
/// The native key of a signal's finding: `watch-<detector>-<16 hex of the SHA-256 of the signature>`.
pub fn key(detector: &str, signature: &str) -> String {
    format!("watch-{detector}-{}", &digest(signature.as_bytes())[..16])
}
/// The first `max` bytes of `t`, cut back to a character boundary.
fn cut(t: &str, max: usize) -> String {
    let mut end = t.len().min(max);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_owned()
}
/// Seconds since 1970 as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc(secs: u64) -> String {
    stamp(UNIX_EPOCH + Duration::from_secs(secs))
}
/// The seconds since 1970 of `YYYY-MM-DDTHH:MM:SSZ`; None for another form.
pub fn epoch(t: &str) -> Option<u64> {
    let b = t.as_bytes();
    let shape = b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !shape {
        return None;
    }
    let n = |a: usize, z: usize| t[a..z].parse::<i64>().ok();
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let secs = n(11, 13)? * 3_600 + n(14, 16)? * 60 + n(17, 19)?;
    let y = y - i64::from(m <= 2);
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    let total = u64::try_from(days * 86_400 + secs).ok()?;
    (utc(total) == t).then_some(total)
}

/// What the arguments ask (R1, R2).
enum Mode {
    Pass,
    Record { totp: String, replace: bool },
}
/// No argument, or `--record-login --totp-file <absolute path> [--replace]` in any order, each once.
fn arguments(argv: &[OsString]) -> Option<Mode> {
    let words: Vec<&str> = argv.iter().map(|a| a.to_str()).collect::<Option<_>>()?;
    let (mut record, mut totp, mut replace) = (false, None, false);
    let mut words = words.into_iter();
    while let Some(w) = words.next() {
        match w {
            "--record-login" if !record => record = true,
            "--replace" if !replace => replace = true,
            "--totp-file" if totp.is_none() => {
                totp = Some(words.next().filter(|p| p.starts_with('/'))?.to_owned())
            }
            _ => return None,
        }
    }
    match (record, totp) {
        (false, None) if !replace => Some(Mode::Pass),
        (true, Some(totp)) => Some(Mode::Record { totp, replace }),
        _ => None,
    }
}
fn refusal(code: &str, field: &str, detail: Value, exit: u8) -> (Value, u8) {
    (
        json!({"error": {"code": code, "field": field, "detail": detail}}),
        exit,
    )
}

/// The lock of a pass, let go by an explicit `LOCK_UN` before its descriptor closes (as `ap work`'s).
struct Held(File);
impl Drop for Held {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
/// The watch directory made (mode 0700), then `pass.lock` taken without waiting: None when another
/// pass holds it (R1).
fn lock(dir: &Path) -> Result<Option<Held>, Failure> {
    let io = |what: &str| fail("io_error", "directory", json!(what));
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    dirs.create(dir)
        .map_err(|_| io("the watch directory cannot be made"))?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let file = options
        .open(dir.join("pass.lock"))
        .map_err(|_| io("the lock"))?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(Held(file))),
        Err(e) if e == rustix::io::Errno::WOULDBLOCK => Ok(None),
        Err(_) => Err(io("the lock")),
    }
}
/// `bytes` written to `dir/name` by a rename of a temporary file of mode 0600.
fn replace_file(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = dir.join(format!(".{name}.tmp"));
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&tmp, dir.join(name))
}
fn seconds(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// One call (R1, R2): the answer and its exit. An answer with the member `error` belongs on standard
/// error.
pub fn execute(s: &mut impl System, dir: &Path, argv: &[OsString]) -> (Value, u8) {
    let Some(mode) = arguments(argv) else {
        return refusal("invalid_usage", "args", Value::Null, 2);
    };
    let held = match lock(dir) {
        Ok(Some(held)) => held,
        Ok(None) => return (json!({"status": "watch_busy"}), 0),
        Err(f) => return refusal(f.code, &f.field, f.detail, 3),
    };
    let answer = match mode {
        Mode::Pass => pass(s, dir),
        Mode::Record { totp, replace } => record(s, dir, &totp, replace),
    };
    drop(held);
    answer
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let (value, exit) = execute(&mut Host, Path::new(DIRECTORY), argv);
    let written = match value.get("error") {
        Some(_) => writeln!(std::io::stderr(), "{value}"),
        None => writeln!(std::io::stdout(), "{value}"),
    };
    ExitCode::from(if written.is_ok() { exit } else { 4 })
}

/// `--record-login` (R2): `login.json` from the reading of now and the TOTP file's path.
fn record(s: &mut impl System, dir: &Path, totp: &str, replace: bool) -> (Value, u8) {
    let path = dir.join("login.json");
    if std::fs::symlink_metadata(&path).is_ok() && !replace {
        return refusal("login_recorded", "login.json", Value::Null, 2);
    }
    match detect::kind(s, totp) {
        Ok(Some(t)) if detect::regular(&t) => {}
        Ok(_) => return refusal("totp_file_absent", "totp_file", Value::Null, 2),
        Err(cause) => return refusal("login_unreadable", "totp_file", json!(cause), 3),
    }
    let parts = match login::reading(s, totp) {
        Ok(parts) => parts,
        Err(cause) => return refusal("login_unreadable", "login", json!(cause), 3),
    };
    let v = json!({"recorded_at": stamp(s.wall()), "totp_file": totp, "parts": parts});
    match replace_file(dir, "login.json", format!("{v}\n").as_bytes()) {
        Ok(()) => (v, 0),
        Err(_) => refusal("io_error", "login.json", Value::Null, 3),
    }
}

/// The actions and the answer of one pass.
struct Pass {
    session: String,
    prefix: String,
    count: u32,
    now: u64,
    at: String,
    opened: Vec<String>,
    seen: Vec<String>,
    reopened: Vec<String>,
    closed: Vec<String>,
    failed: Vec<Value>,
}
/// Which list an applied action joins.
#[derive(Clone, Copy)]
enum Done {
    Opened,
    Seen,
    Reopened,
    Closed,
}
impl Pass {
    fn apply(&mut self, s: &mut impl System, action: &str, key: &str, p: Value, done: Done) {
        self.count += 1;
        let rid = format!("{}-{}", self.prefix, self.count);
        let actor = Actor {
            agent: "ap-watch",
            session: &self.session,
            role: "watch",
        };
        match store::unit_action(s, None, &actor, action, rid, p) {
            Ok(_) => match done {
                Done::Opened => self.opened.push(key.to_owned()),
                Done::Seen => self.seen.push(key.to_owned()),
                Done::Reopened => self.reopened.push(key.to_owned()),
                Done::Closed => self.closed.push(key.to_owned()),
            },
            Err(f) => self
                .failed
                .push(json!({"key": key, "code": store::code(&f)})),
        }
    }
    /// The actions of one detector (R4, R5): per signal by its finding's state, then the closes.
    fn act(&mut self, s: &mut impl System, d: &str, reading: &Reading) {
        let detector = format!("watch:{d}");
        let kind = match d {
            "login" => "security",
            "drift" => "drift",
            _ => "incident",
        };
        let all = vec![json!({"eq": ["detector", detector]})];
        let found: BTreeMap<String, Value> = match store::search(s, None, "finding", all) {
            Ok(found) => found
                .into_iter()
                .filter(|(_, f)| f["detector"] == detector.as_str())
                .collect(),
            Err(f) => {
                let code = store::code(&f);
                return self.failed.push(json!({"key": detector, "code": code}));
            }
        };
        let own;
        let (signals, unknown) = match reading {
            Ok(r) => (&r.signals, Some(&r.unknown)),
            Err(cause) => {
                let what = format!("ap watch could not read the sources of the detector {d}");
                own = vec![Signal::new(
                    "unreadable".into(),
                    "act",
                    &what,
                    cause.clone(),
                )];
                (&own, None)
            }
        };
        let mut yielded = BTreeSet::new();
        for sig in signals {
            let key = key(d, &sig.signature);
            yielded.insert(key.clone());
            let finding = id("finding", &key);
            let evidence = cut(&sig.evidence, EVIDENCE);
            let f = found.get(&key);
            match f.and_then(|f| f["state"].as_str()) {
                None => {
                    let p = json!({"finding": finding, "kind": kind, "detector": detector,
                        "summary": cut(&sig.summary, SUMMARY), "evidence": evidence,
                        "severity": sig.severity});
                    self.apply(s, "open_finding", &key, p, Done::Opened);
                }
                Some("open" | "planned") => {
                    let f = f.unwrap_or(&Value::Null);
                    let differs = f["severity"] != sig.severity;
                    let last = f["last_seen_at"].as_str().and_then(epoch);
                    let old = last.is_none_or(|t| self.now.saturating_sub(t) > SEEN_AFTER);
                    if differs || old {
                        let mut p = json!({"finding": finding, "evidence": evidence});
                        if differs {
                            p["severity"] = json!(sig.severity);
                        }
                        self.apply(s, "see_finding_again", &key, p, Done::Seen);
                    }
                }
                Some("closed") => {
                    let p = json!({"finding": finding, "evidence": evidence});
                    self.apply(s, "reopen_finding", &key, p, Done::Reopened);
                }
                Some(_) => {}
            }
        }
        let Some(unknown) = unknown else {
            return;
        };
        for (key, f) in &found {
            let live = matches!(f["state"].as_str(), Some("open" | "planned"));
            let summary = f["summary"].as_str().unwrap_or_default();
            let known = family(d, summary).is_none_or(|fam| !unknown.contains(&fam));
            if live && known && !yielded.contains(key) {
                let reason = format!("not seen by ap watch {d} at {}", self.at);
                let p = json!({"finding": id("finding", key), "reason": reason});
                self.apply(s, "close_finding", key, p, Done::Closed);
            }
        }
    }
}

/// The host's policy file as one JSON object, or None when absent.
fn read_policy(s: &impl System) -> Result<Option<Value>, String> {
    let Some(file) = s.policy_file() else {
        return Ok(None);
    };
    let Some(path) = file.to_str() else {
        let missing = std::fs::metadata(&file).is_err_and(|e| e.kind() == ErrorKind::NotFound);
        return if missing {
            Ok(None)
        } else {
            Err("policy path is not valid UTF-8".to_owned())
        };
    };
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("policy file cannot be read".to_owned()),
    };
    if bytes.len() as u64 > LIMIT {
        return Err("policy file is too large".to_owned());
    }
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| "policy file is not one JSON object".to_owned())
        .map(Some)
}

/// One pass (R3 to R9, R20 to R31).
fn pass(s: &mut impl System, dir: &Path) -> (Value, u8) {
    let wall = s.wall();
    let (now, at) = (seconds(wall), stamp(wall));
    let drawn = match new_session_id(s) {
        Ok(id) => id.replace('-', ""),
        Err(f) => return refusal(f.code, &f.field, f.detail, 3),
    };
    let prior: Option<Value> = std::fs::read(dir.join("state.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(Value::is_object);
    let member = |d: &str| {
        prior
            .as_ref()
            .and_then(|p| p.get(d))
            .filter(|v| v.is_object())
    };
    let policy = read_policy(s);
    let mut readings: Vec<(&str, Reading)> = vec![];
    for d in DETECTORS {
        let reading = match d {
            "delivery" => detect::delivery(s, now, member(d)),
            "login" => login::detect(s, dir),
            "services" => detect::services(s, now),
            "host" => detect::host(s, now, member(d), &policy),
            "records" => detect::records(s, now, member(d)),
            "accounts" => accounts::detect(s, now, &policy),
            "checks" => checks::detect(s, &policy),
            "runtimes" => runtimes::detect(s, member(d)),
            "drift" => drift::detect(s),
            _ => credit::detect(s, now, &policy),
        };
        readings.push((d, reading));
    }
    let mut p = Pass {
        session: format!("ww-{}", &drawn[..16]),
        prefix: format!("ww-{}", &drawn[..16]),
        count: 0,
        now,
        at: at.clone(),
        opened: vec![],
        seen: vec![],
        reopened: vec![],
        closed: vec![],
        failed: vec![],
    };
    for (d, reading) in &readings {
        p.act(s, d, reading);
    }
    let mut next = Map::new();
    let mut detectors = Map::new();
    for (d, reading) in &readings {
        let (state, signals, member) = match reading {
            Ok(r) => ("ok", r.signals.len(), r.state.as_ref()),
            Err(_) => ("unreadable", 1, member(d)),
        };
        if let Some(m) = member {
            next.insert((*d).to_owned(), m.clone());
        }
        detectors.insert((*d).to_owned(), json!({"state": state, "signals": signals}));
    }
    let bytes = format!("{}\n", Value::Object(next));
    if replace_file(dir, "state.json", bytes.as_bytes()).is_err() {
        p.failed
            .push(json!({"key": "state.json", "code": "io_error"}));
    }
    let exit = u8::from(!p.failed.is_empty());
    let v = json!({"at": at, "detectors": detectors, "opened": p.opened, "seen": p.seen,
        "reopened": p.reopened, "closed": p.closed, "failed": p.failed});
    (v, exit)
}
