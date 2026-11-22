//! The detectors `delivery`, `services`, `host` and `records` of `ap watch` (`specs/ap/watch.md` R10 to
//! R13), and the reads every detector shares. Each reads through `System` only and changes nothing.
use super::{Read, Reading, Signal, epoch, utc};
use crate::host::{System, digest};
use serde_json::{Value, json};

/// Delivery's record (`deploy/deliver.json`'s `state`).
pub const DELIVERY: &str = "/var/lib/agent-platform/deliver/last.json";
/// The backup's record (`scripts/backup-store.sh`'s `state`).
pub const BACKUP: &str = "/var/lib/agent-platform/backup/last.json";
/// The directory whose file system `disk floor` reads.
pub const DISK: &str = "/var/lib/agent-platform";
pub const MEMORY_EVENTS: &str = "/sys/fs/cgroup/agents.slice/memory.events";
pub const APT: &str = "/var/log/apt/history.log";
/// The checks log (`specs/ap/check.md` R15) and the door's record (`specs/ap/door.md` R6) `pc down`
/// reads (R30), at these default paths only.
pub const CHECKS_LOG: &str = "/var/lib/agent-platform/checks.jsonl";
pub const DOOR_LOG: &str = "/var/lib/agent-platform/door.jsonl";
/// How far back a reading of the PC counts (R30), in seconds.
const PC_WINDOW: u64 = 3_600;
/// The units `services` leaves to `delivery` and `records`.
const COVERED: [&str; 2] = ["ap-deliver.service", "ont-backup.service"];
/// The timers `services` reads, each with its interval in seconds (R11); `ap-work` only once installed.
pub const TIMERS: [(&str, u64); 5] = [
    ("ap-deliver", 600),
    ("ap-quota", 30),
    ("ont-backup", 3_600),
    ("ap-watch", 300),
    ("ap-work", 60),
];
/// The command names of the agent CLIs `old cli` lists.
const CLIS: [&str; 4] = ["claude", "muse", "agy", "codex"];
/// Delivery's refusals that signal only after 30 minutes of the same end and code.
const PASSING: [&str; 3] = ["build_busy", "busy", "attempt_running"];
/// Delivery's refusals that yield no signal while an `end failed` is kept.
const AFTER_FAILED: [&str; 3] = ["pair_differs", "suppressed", "held_by_rollback"];
const MS: u64 = 10_000;

/// The standard output of one call that ran and succeeded, as text; else the cause.
pub(crate) fn output(s: &mut impl System, program: &str, words: &[&str]) -> Result<String, String> {
    let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let shown = format!("{program} {}", words.join(" "));
    match s.call(program, &words, &Value::Null, MS) {
        Ok(r) if r.success => {
            String::from_utf8(r.stdout).map_err(|_| format!("{shown}: not UTF-8"))
        }
        Ok(_) => Err(format!("{shown} failed")),
        Err(_) => Err(format!("{shown} could not run")),
    }
}
/// The type `stat -c %F` prints for `path` (a link is not followed); None when `stat` does not find
/// the path; the cause when the call cannot run or fails otherwise.
pub(crate) fn kind(s: &mut impl System, path: &str) -> Result<Option<String>, String> {
    let words = ["-c".to_owned(), "%F".to_owned(), path.to_owned()];
    match s.call("stat", &words, &Value::Null, MS) {
        Ok(r) if r.success => match String::from_utf8(r.stdout) {
            Ok(t) if !t.trim().is_empty() => Ok(Some(t.trim().to_owned())),
            _ => Err(format!("stat {path}: no type")),
        },
        Ok(r) => {
            let said = String::from_utf8_lossy(&r.bytes);
            let absent = ["No such file or directory", "Not a directory"];
            match absent.iter().any(|a| said.contains(a)) {
                true => Ok(None),
                false => Err(format!("stat {path} failed")),
            }
        }
        Err(_) => Err(format!("stat {path} could not run")),
    }
}
/// Whether a type `stat` printed is a regular file (`regular empty file` included).
pub(crate) fn regular(t: &str) -> bool {
    t == "regular file" || t == "regular empty file"
}
/// The SHA-256 of a regular file's bytes; the cause when it cannot be read.
pub(crate) fn file_digest(s: &impl System, path: &str) -> Result<String, String> {
    s.read(path)
        .map(|b| digest(&b))
        .map_err(|_| format!("{path} cannot be read"))
}
/// A file's bytes as text: None when it does not exist; the cause when it cannot be read.
fn text(s: &impl System, path: &str) -> Result<Option<String>, String> {
    match s.read(path) {
        Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(format!("{path} cannot be read")),
    }
}
/// A JSON record's object; the cause when it is absent, unreadable or not one object.
fn record(s: &impl System, path: &str) -> Result<Value, String> {
    let t = text(s, path)?.ok_or_else(|| format!("{path} is absent"))?;
    serde_json::from_str::<Value>(&t)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| format!("{path} is not one JSON object"))
}
fn word(v: &Value) -> String {
    v.as_str().map_or_else(|| "-".to_owned(), str::to_owned)
}

/// `delivery` (R10): `last.json`'s age, its `end` and its `checkouts`.
pub(crate) fn delivery(s: &mut impl System, now: u64, prior: Option<&Value>) -> Reading {
    let mtime = output(s, "stat", &["-c", "%Y", DELIVERY])?;
    let mtime: u64 = mtime
        .trim()
        .parse()
        .map_err(|_| "stat -c %Y: not a time".to_owned())?;
    let last = record(s, DELIVERY)?;
    let mut r = Read::default();
    let at = word(&last["at"]);
    if now.saturating_sub(mtime) > 30 * 60 {
        let what = format!(
            "delivery's last.json is {} minutes old (the limit is 30)",
            (now - mtime) / 60
        );
        let evidence = format!(
            "{DELIVERY} modified at {}; the pass at {}",
            utc(mtime),
            utc(now)
        );
        r.signals
            .push(Signal::new("stale".into(), "act", &what, evidence));
    }
    let (end, code) = (word(&last["end"]), word(&last["code"]));
    let evidence = format!(
        "last.json at {at}: end {end}, code {code}, outcome {}, tip {}, from {}, to {}",
        word(&last["outcome"]),
        word(&last["tip"]),
        word(&last["from"]),
        word(&last["to"])
    );
    let ended = end == "installed" || end == "current";
    // An `end failed` is kept until an end `installed` or `current`. Whether one is kept is unknown
    // (None) after a baseline that read another end, until one of those three ends is read: the
    // member `failed` is then left out of `state.json`.
    let known: Option<Option<String>> = match end.as_str() {
        "failed" => Some(Some(code.clone())),
        _ if ended => Some(None),
        _ => prior
            .and_then(|p| p.get("failed"))
            .map(|v| v.as_str().map(str::to_owned)),
    };
    let kept = known.clone().flatten();
    if known.is_none() {
        r.unknown.push("end".into());
    }
    if let Some(k) = &kept {
        let what = "delivery ended failed: a rollback that did not restore; a person acts";
        r.signals.push(Signal::new(
            format!("end failed {k}"),
            "urgent",
            what,
            evidence.clone(),
        ));
    }
    let mut refusal = Value::Null;
    if !ended && end != "failed" {
        let passing = PASSING.contains(&code.as_str());
        if passing {
            let same = prior
                .map(|p| &p["refusal"])
                .filter(|f| f["end"] == end.as_str() && f["code"] == code.as_str());
            let since = same.and_then(|f| f["since"].as_u64()).unwrap_or(now);
            refusal = json!({"end": end, "code": code, "since": since});
            if same.is_some() && now.saturating_sub(since) > 30 * 60 {
                let what = format!("delivery has ended {end} with {code} since {}", utc(since));
                r.signals.push(Signal::new(
                    format!("end {end} {code}"),
                    "act",
                    &what,
                    evidence.clone(),
                ));
            } else {
                r.unknown.push("end".into());
            }
        } else if known == Some(None) || !AFTER_FAILED.contains(&code.as_str()) {
            let what = format!("the last delivery ended {end} with {code}");
            r.signals.push(Signal::new(
                format!("end {end} {code}"),
                "act",
                &what,
                evidence,
            ));
        }
    }
    match last["checkouts"].as_array() {
        None => r.unknown.push("checkout".into()),
        Some(rows) => {
            for c in rows {
                let (path, result) = (word(&c["path"]), word(&c["result"]));
                if result != "forwarded" && result != "current" {
                    let what =
                        format!("delivery did not bring the checkout {path} to main ({result})");
                    let evidence = format!(
                        "last.json at {at}: checkout {path} from {} to {} result {result}",
                        word(&c["from"]),
                        word(&c["to"])
                    );
                    r.signals.push(Signal::new(
                        format!("checkout {path} {result}"),
                        "act",
                        &what,
                        evidence,
                    ));
                }
            }
        }
    }
    let mut state = json!({"refusal": refusal});
    if let Some(kept) = known {
        state["failed"] = json!(kept);
    }
    r.state = Some(state);
    Ok(r)
}

/// The properties `systemctl show` prints for each unit, one block per unit, keyed by `Id`.
fn show(s: &mut impl System, units: &[&str], props: &[&str]) -> Result<Vec<Value>, String> {
    let mut words = vec!["show"];
    for p in props {
        words.extend(["-p", p]);
    }
    words.push("--");
    words.extend(units);
    let out = output(s, "systemctl", &words)?;
    let blocks: Vec<Value> = out
        .split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|b| {
            b.lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.to_owned(), json!(v)))
                .collect()
        })
        .collect();
    let all = units.iter().all(|u| blocks.iter().any(|b| b["Id"] == *u));
    all.then_some(blocks)
        .ok_or_else(|| "systemctl show: a unit's block is missing".into())
}
/// A timestamp of `systemctl show` (`Thu 2026-11-19 02:03:01 UTC`); None when empty or `n/a`.
fn timestamp(t: &str) -> Result<Option<u64>, String> {
    let words: Vec<&str> = t.split_whitespace().collect();
    match words[..] {
        [] | ["n/a"] => Ok(None),
        [_, date, time, "UTC"] => epoch(&format!("{date}T{time}Z"))
            .map(Some)
            .ok_or_else(|| format!("{t}: not a time")),
        _ => Err(format!("{t}: not a UTC time")),
    }
}
/// `services` (R11): the failed units and the timers that did not fire.
pub(crate) fn services(s: &mut impl System, now: u64) -> Reading {
    let mut r = Read::default();
    let failed = output(
        s,
        "systemctl",
        &["--failed", "--plain", "--no-legend", "--no-pager"],
    )?;
    for unit in failed.lines().filter_map(|l| l.split_whitespace().next()) {
        if COVERED.contains(&unit) {
            continue;
        }
        let props = [
            "Id",
            "Result",
            "ExecMainStatus",
            "StateChangeTimestamp",
            "InactiveEnterTimestamp",
        ];
        let b = show(s, &[unit], &props)?.remove(0);
        let evidence = props[1..]
            .iter()
            .map(|p| format!("{p}={}", word(&b[*p])))
            .collect::<Vec<_>>()
            .join("; ");
        let what = format!("the unit {unit} is failed");
        r.signals.push(Signal::new(
            format!("failed {unit}"),
            "act",
            &what,
            evidence,
        ));
    }
    let uptime = text(s, "/proc/uptime")?.ok_or("/proc/uptime is absent")?;
    let uptime: f64 = uptime
        .split_whitespace()
        .next()
        .and_then(|u| u.parse().ok())
        .ok_or("/proc/uptime: no number")?;
    let mut units = vec![];
    let names: Vec<(String, String)> = TIMERS
        .iter()
        .map(|(n, _)| (format!("{n}.timer"), format!("{n}.service")))
        .collect();
    for (t, u) in &names {
        units.extend([t.as_str(), u.as_str()]);
    }
    let blocks = show(
        s,
        &units,
        &["Id", "LoadState", "ActiveState", "LastTriggerUSec"],
    )?;
    let of = |u: &str| {
        blocks
            .iter()
            .find(|b| b["Id"] == u)
            .cloned()
            .unwrap_or_default()
    };
    for ((name, interval), (t, u)) in TIMERS.iter().zip(&names) {
        let (timer, service) = (of(t), of(u));
        if *name == "ap-work" && timer["LoadState"] == "not-found" {
            continue;
        }
        let last = timestamp(timer["LastTriggerUSec"].as_str().unwrap_or_default())?;
        let active = ["active", "activating", "deactivating", "reloading"]
            .contains(&service["ActiveState"].as_str().unwrap_or_default());
        let late = match last {
            None => uptime as u64 > 3 * interval,
            Some(at) => now.saturating_sub(at) > 3 * interval && !active,
        };
        if late {
            let what =
                format!("the timer {t} did not fire within three intervals of {interval} seconds");
            let evidence = format!(
                "{t} LoadState={} LastTriggerUSec={}; {u} ActiveState={}; uptime {} seconds",
                word(&timer["LoadState"]),
                word(&timer["LastTriggerUSec"]),
                word(&service["ActiveState"]),
                uptime as u64
            );
            r.signals
                .push(Signal::new(format!("timer {name}"), "act", &what, evidence));
        }
    }
    r.state = None;
    Ok(r)
}

/// `host` (R12): the disk floor, the slice's `memory.events`, the apt history and old agent CLIs.
pub(crate) fn host(
    s: &mut impl System,
    now: u64,
    prior: Option<&Value>,
    policy: &Result<Option<Value>, String>,
) -> Reading {
    let mut r = Read::default();
    let mut state = prior.cloned().unwrap_or_else(|| json!({}));
    let df = output(s, "df", &["-BM", "--output=avail", DISK])?;
    let free: u64 = df
        .lines()
        .last()
        .map(|l| l.trim().trim_end_matches('M'))
        .and_then(|n| n.parse().ok())
        .ok_or("df: no number")?;
    if free < 10_240 {
        let what = format!("the file system of {DISK} has {free} MiB free (the floor is 10240)");
        r.signals.push(Signal::new(
            "disk floor".into(),
            "act",
            &what,
            format!("df {DISK}: {free} MiB available"),
        ));
    }
    match text(s, MEMORY_EVENTS)? {
        None => r.unknown.push("slice memory high".into()),
        Some(events) => {
            let high: u64 = events
                .lines()
                .find_map(|l| l.strip_prefix("high "))
                .and_then(|n| n.trim().parse().ok())
                .ok_or("memory.events: no high")?;
            let before = prior.and_then(|p| p["memory_high"].as_u64());
            if before.is_none() {
                r.unknown.push("slice memory high".into());
            }
            let mut rose = prior
                .and_then(|p| p["memory_rose_at"].as_str())
                .and_then(epoch);
            if before.is_some_and(|b| high > b) {
                rose = Some(now);
            }
            if let Some(t) = rose.filter(|t| now.saturating_sub(*t) < 3_600) {
                let what = "agents.slice reached MemoryHigh within the last 60 minutes";
                let evidence = format!(
                    "{MEMORY_EVENTS}: high {high}, before {}, last rose at {}",
                    before.map_or("-".into(), |b| b.to_string()),
                    utc(t)
                );
                r.signals.push(Signal::new(
                    "slice memory high".into(),
                    "info",
                    what,
                    evidence,
                ));
            }
            (state["memory_high"], state["memory_rose_at"]) = (json!(high), json!(rose.map(utc)));
        }
    }
    match text(s, APT)? {
        None => r.unknown.push("apt".into()),
        Some(log) => {
            let starts: Vec<String> = log
                .lines()
                .filter_map(|l| l.strip_prefix("Start-Date:"))
                .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect();
            let before = prior
                .and_then(|p| p.get("apt_start"))
                .map(|v| v.as_str().unwrap_or_default().to_owned());
            if before.is_none() {
                r.unknown.push("apt".into());
            }
            if let Some(before) = &before {
                for start in starts.iter().filter(|st| st.as_str() > before.as_str()) {
                    let what = "a package change in the apt history";
                    let evidence = format!(
                        "Start-Date {start} in {APT}; the newest of the pass before: {}",
                        if before.is_empty() { "-" } else { before }
                    );
                    r.signals
                        .push(Signal::new(format!("apt {start}"), "info", what, evidence));
                }
            }
            let newest = starts.into_iter().chain(before).max().unwrap_or_default();
            state["apt_start"] = json!(newest);
        }
    }
    let ps = output(s, "ps", &["-eo", "pid=,etimes=,lstart=,comm="])?;
    for line in ps.lines().filter(|l| !l.trim().is_empty()) {
        let w: Vec<&str> = line.split_whitespace().collect();
        let (Some(pid), Some(elapsed), Some(start)) = (
            w.first(),
            w.get(1).and_then(|e| e.parse::<u64>().ok()),
            w.get(2..7),
        ) else {
            return Err("ps: a line cannot be read".into());
        };
        let command = w[7..].join(" ");
        if CLIS.contains(&command.as_str()) && elapsed > 7 * 86_400 {
            let start = start.join(" ");
            let what = format!(
                "the agent CLI {command} (pid {pid}) is older than seven days; listed, not signalled"
            );
            let evidence =
                format!("pid {pid}, command {command}, started {start}, elapsed {elapsed} seconds");
            r.signals.push(Signal::new(
                format!("old cli {pid} {start}"),
                "info",
                &what,
                evidence,
            ));
        }
    }
    pc_down(s, now, policy, &mut r)?;
    r.state = Some(state);
    Ok(r)
}

/// One reading of the PC (R30): when it was made, and whether the PC took the call.
struct PcReading {
    at: u64,
    reached: bool,
    file: &'static str,
    /// The checks-log line's `host` (`pc` or `local`, else `-`) and the reason, a fixed word.
    host: &'static str,
    reason: &'static str,
}
/// The reasons of a call the probe of the PC answered (`specs/ap/check.md` R4, R19) and those of a
/// call it did not take.
const ANSWERED: [&str; 3] = ["pc_busy", "pc_memory_low", "pc_engine_differs"];
const DOWN: [&str; 2] = ["pc_unreachable", "pc_lost"];
fn fixed(set: &[&'static str], v: &Value) -> Option<&'static str> {
    set.iter().copied().find(|w| v.as_str() == Some(*w))
}
/// The reading one line of a record holds, or None (R30).
fn pc_line(file: &'static str, line: &str) -> Option<PcReading> {
    let v: Value = serde_json::from_str(line).ok().filter(Value::is_object)?;
    let door = file == "door.jsonl";
    let (at, reached) = if door {
        if v["verb"] != "check" {
            return None;
        }
        let start = epoch(v["at"].as_str()?)?;
        let at = start.saturating_add(v["ms"].as_u64()? / 1_000);
        (at, v["exit"].is_i64() || v["exit"].is_u64())
    } else {
        (epoch(v["end"].as_str()?)?, v["host"] == "pc")
    };
    let answered = fixed(&ANSWERED, &v["reason"]);
    let down = fixed(&DOWN, &v["reason"]);
    let reached = reached || answered.is_some();
    if !reached && down.is_none() {
        return None;
    }
    let host = match v["host"].as_str() {
        Some("pc") if !door => "pc",
        Some("local") if !door => "local",
        _ => "-",
    };
    Some(PcReading {
        at,
        reached,
        file,
        host,
        reason: answered.or(down).unwrap_or("-"),
    })
}
/// The part of `hosts.pc.ssh` after its `@`, when it is a host name.
fn pc_host(pc: &Value) -> String {
    let name = |h: &&str| {
        !h.is_empty()
            && h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    };
    pc["ssh"]
        .as_str()
        .and_then(|s| s.split_once('@'))
        .map(|(_, h)| h)
        .filter(name)
        .map_or_else(|| "<host of hosts.pc.ssh>".to_owned(), str::to_owned)
}
/// `pc down` (R30): the newest reading of the PC of the last 60 minutes in the checks log and the
/// door's record, when the policy names `hosts.pc`. Nothing of it is kept in `state.json`.
fn pc_down(
    s: &impl System,
    now: u64,
    policy: &Result<Option<Value>, String>,
    r: &mut Read,
) -> Result<(), String> {
    let Ok(Some(policy)) = policy else {
        r.unknown.push("pc down".into());
        return Ok(());
    };
    let pc = &policy["hosts"]["pc"];
    if !pc.is_object() {
        return Ok(());
    }
    let mut newest: Option<PcReading> = None;
    for (path, file) in [(CHECKS_LOG, "checks.jsonl"), (DOOR_LOG, "door.jsonl")] {
        let Some(log) = text(s, path)? else {
            continue;
        };
        for reading in log.lines().filter_map(|l| pc_line(file, l)) {
            let counts = reading.at.saturating_add(PC_WINDOW) >= now;
            let better = newest
                .as_ref()
                .is_none_or(|n| (reading.at, reading.reached) > (n.at, n.reached));
            if counts && better {
                newest = Some(reading);
            }
        }
    }
    match newest {
        None => r.unknown.push("pc down".into()),
        Some(n) if n.reached => {}
        Some(n) => {
            let host = pc_host(pc);
            let what = format!(
                "the local PC took no check; ask the user to run `Start-ScheduledTask -TaskName \
                 ap-checks-keepalive` in PowerShell on the PC, then read `tailscale ping {host}` \
                 on this host"
            );
            let evidence = match n.file {
                "checks.jsonl" => format!(
                    "checks.jsonl at {}: host {}, reason {}",
                    utc(n.at),
                    n.host,
                    n.reason
                ),
                file => format!("{file} at {}: reason {}", utc(n.at), n.reason),
            };
            r.signals
                .push(Signal::new("pc down".into(), "act", &what, evidence));
        }
    }
    Ok(())
}

/// `records` (R13): the store's newest backup and the copy off the host.
pub(crate) fn records(s: &mut impl System, now: u64, prior: Option<&Value>) -> Reading {
    let mut r = Read::default();
    let last = record(s, BACKUP)?;
    let at = last["at"]
        .as_str()
        .and_then(epoch)
        .ok_or("backup last.json: no at")?;
    let kept = |k: &str| prior.and_then(|p| p[k].as_str()).and_then(epoch);
    let backup_at = match (last["backup"].is_string(), kept("backup_at")) {
        (true, _) => at,
        (false, Some(t)) => t,
        (false, None) => now,
    };
    let copied = matches!(last["remote"].as_str(), Some("copied" | "unchanged"));
    let copied_at = match (copied, kept("copied_at")) {
        (true, _) => at,
        (false, Some(t)) => t,
        (false, None) => now,
    };
    if now.saturating_sub(backup_at) > 2 * 3_600 {
        let since = utc(backup_at + 1);
        let body = json!({"outcome": "applied", "since": since, "limit": 1});
        let reply = crate::host::ont_within(s, &json!({}), &["log"], &body, 30_000)
            .map_err(|_| "ont log could not run".to_owned())?;
        let v: Value =
            serde_json::from_slice(&reply.bytes).map_err(|_| "ont log: no answer".to_owned())?;
        let rows = v["result"]["rows"]
            .as_array()
            .filter(|_| reply.success && v["status"] == "ok")
            .ok_or("ont log failed")?;
        if !rows.is_empty() || v["result"]["withheld"].as_u64().unwrap_or(0) > 0 {
            let what = format!(
                "the store's newest backup is from {}, over two hours ago, and actions were applied since",
                utc(backup_at)
            );
            let evidence = format!(
                "{BACKUP}: at {}, backup {}, code {}; an applied action since {since}",
                word(&last["at"]),
                word(&last["backup"]),
                word(&last["code"])
            );
            r.signals
                .push(Signal::new("backup stale".into(), "act", &what, evidence));
        }
    }
    if now.saturating_sub(copied_at) > 86_400 {
        let what = format!(
            "the last copy off the host is from {}, over a day ago",
            utc(copied_at)
        );
        let evidence = format!(
            "copied_at {}; {BACKUP}: at {}, remote {}, code {}",
            utc(copied_at),
            word(&last["at"]),
            word(&last["remote"]),
            word(&last["code"])
        );
        r.signals.push(Signal::new(
            "backup copy stale".into(),
            "act",
            &what,
            evidence,
        ));
    }
    r.state = Some(json!({"backup_at": utc(backup_at), "copied_at": utc(copied_at)}));
    Ok(r)
}
