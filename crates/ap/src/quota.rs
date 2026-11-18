//! `ap quota`: one turn that sets the runtime CPU quota of a slice (`specs/ap/quota.md`).
use crate::host::{Failure, Host, System, args, error, fail, require};
use crate::run::stamp;
use serde_json::{Map, Value, json};
use std::{ffi::OsString, io::Write, process::ExitCode, time::Duration};

const GAP: Duration = Duration::from_secs(10);

/// The member `quota` of the policy file, validated (R2).
struct Config {
    slice: String,
    busy_limit: i64,
    margin: i64,
    ceiling: i64,
    floor: i64,
}
/// What one reading of a slice and the host says (R4 to R6), per cent.
pub struct Load {
    pub cpus: i64,
    pub busy: i64,
    pub slice_use: i64,
    pub outside: i64,
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
fn slice_ok(name: &str) -> bool {
    name.strip_suffix(".slice").is_some_and(|n| {
        (1..=32).contains(&n.len())
            && n.bytes()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
    })
}
fn number(
    o: &Map<String, Value>,
    key: &str,
    default: i64,
    range: (i64, i64),
) -> Result<i64, Failure> {
    let bad = || {
        invalid(
            &format!("quota.{key}"),
            &format!("is not an integer from {} to {}", range.0, range.1),
        )
    };
    o.get(key).map_or(Ok(default), |v| {
        v.as_i64()
            .filter(|n| (range.0..=range.1).contains(n))
            .ok_or_else(bad)
    })
}
/// The policy's `quota`: None without a file or a member.
fn config(s: &impl System) -> Result<Option<Config>, Failure> {
    let Some(file) = s.policy_file() else {
        return Ok(None);
    };
    let path = file
        .to_str()
        .ok_or_else(|| invalid("file", "path is not valid UTF-8"))?;
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("file", "is unreadable")),
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("file", "is not JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("file", "is not one JSON object"))?;
    let Some(q) = object.get("quota") else {
        return Ok(None);
    };
    let q = q
        .as_object()
        .ok_or_else(|| invalid("quota", "is not an object"))?;
    if let Some(key) = q
        .keys()
        .find(|k| !["slice", "busy_limit", "margin", "ceiling", "floor"].contains(&k.as_str()))
    {
        return Err(invalid(
            "quota",
            &format!("holds the member {key}, which is not read"),
        ));
    }
    let slice = q
        .get("slice")
        .and_then(Value::as_str)
        .filter(|n| slice_ok(n));
    let slice =
        slice.ok_or_else(|| invalid("quota.slice", "is not a slice name of a-z0-9 and .slice"))?;
    let ceiling = number(q, "ceiling", 170, (10, 6_400))?;
    Ok(Some(Config {
        slice: slice.to_owned(),
        busy_limit: number(q, "busy_limit", 85, (10, 100))?,
        margin: number(q, "margin", 5, (0, 50))?,
        ceiling,
        floor: number(q, "floor", 60.min(ceiling), (10, ceiling))?,
    }))
}

fn reading(field: &str) -> Failure {
    error("reading_failed", field)
}
/// `part` of `whole` times `scale`, rounded to the nearest.
fn ratio(part: u64, whole: u64, scale: u64) -> i64 {
    (part as f64 * scale as f64 / whole as f64).round() as i64
}
/// One moment: the `cpu ` line's idle and total, the CPUs, the slice's `usage_usec` (None: no use to read).
fn sample(s: &mut impl System, slice: &str) -> Result<(u64, u64, i64, Option<u64>), Failure> {
    let stat = s.read("/proc/stat").map_err(|_| reading("proc_stat"))?;
    let stat = String::from_utf8(stat).map_err(|_| reading("proc_stat"))?;
    let line = stat
        .lines()
        .find(|l| l.starts_with("cpu "))
        .ok_or_else(|| reading("proc_stat"))?;
    let numbers: Option<Vec<u64>> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|n| n.parse().ok())
        .collect();
    let numbers = numbers
        .filter(|n| n.len() == 8)
        .ok_or_else(|| reading("proc_stat"))?;
    let cpus = stat
        .lines()
        .filter(|l| {
            l.strip_prefix("cpu")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        })
        .count();
    require!(cpus > 0 => "reading_failed", "proc_stat");
    let reply = s.call(
        "systemctl",
        &args(&["show", "-p", "ControlGroup", "--value", slice]),
        &Value::Null,
        10_000,
    );
    let reply = reply
        .ok()
        .filter(|r| r.success)
        .ok_or_else(|| reading("control_group"))?;
    // Only the form the service manager gives this slice is made into a path: `/cpu.stat` is the host's.
    let usage = if reply.stdout == format!("/{slice}\n").as_bytes() {
        match s.read(&format!("/sys/fs/cgroup/{slice}/cpu.stat")) {
            Ok(file) => {
                let file = String::from_utf8_lossy(&file);
                let count = file
                    .lines()
                    .find_map(|l| l.strip_prefix("usage_usec "))
                    .and_then(|n| n.trim().parse().ok());
                Some(count.ok_or_else(|| reading("cpu_stat"))?)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(reading("cpu_stat")),
        }
    } else {
        None
    };
    Ok((numbers[3], numbers.iter().sum(), cpus as i64, usage))
}
/// The two readings of R4 to R6, ten seconds apart. `slice` is a name `slice_ok` accepts.
pub fn load(s: &mut impl System, slice: &str) -> Result<Load, Failure> {
    require!(slice_ok(slice) => "reading_failed", "slice");
    let first = sample(s, slice)?;
    let began = s.now();
    s.pause(GAP);
    let second = sample(s, slice)?;
    let micros = s.now().duration_since(began).as_micros() as u64;
    let total = second
        .1
        .checked_sub(first.1)
        .filter(|t| *t > 0)
        .ok_or_else(|| reading("proc_stat"))?;
    let idle = second.0.saturating_sub(first.0).min(total);
    let (cpus, busy) = (second.2, ratio(total - idle, total, 100));
    let slice_use = match (first.3, second.3) {
        (Some(a), Some(b)) if b >= a && micros > 0 => ratio(b - a, micros, 100),
        _ => 0,
    };
    Ok(Load {
        cpus,
        busy,
        slice_use,
        outside: (busy * cpus - slice_use).max(0),
    })
}

/// `CPUQuotaPerSecUSec` in microseconds per second: None is `infinity` (R7).
fn quota_now(s: &mut impl System, slice: &str) -> Result<Option<u64>, Failure> {
    let reply = s.call(
        "systemctl",
        &args(&["show", "-p", "CPUQuotaPerSecUSec", "--value", slice]),
        &Value::Null,
        10_000,
    );
    let reply = reply
        .ok()
        .filter(|r| r.success)
        .ok_or_else(|| reading("quota_now"))?;
    let text = String::from_utf8(reply.stdout).map_err(|_| reading("quota_now"))?;
    let text = text.trim();
    if text == "infinity" {
        return Ok(None);
    }
    let term = |t: &str| -> Option<u64> {
        let (digits, unit) = [("min", 60_000_000), ("ms", 1_000), ("s", 1_000_000)]
            .iter()
            .find_map(|(u, m)| t.strip_suffix(u).map(|d| (d, *m)))?;
        let (whole, fraction) = match digits.split_once('.') {
            Some((w, f)) if unit == 1_000_000 && (1..=6).contains(&f.len()) => (w, f),
            Some(_) => return None,
            None => (digits, ""),
        };
        if whole.is_empty()
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let micros = if fraction.is_empty() {
            0
        } else {
            format!("{fraction:0<6}").parse().ok()?
        };
        whole
            .parse::<u64>()
            .ok()?
            .checked_mul(unit)?
            .checked_add(micros)
    };
    let terms: Option<Vec<u64>> = text.split_whitespace().map(term).collect();
    let terms = terms
        .filter(|t| !t.is_empty())
        .ok_or_else(|| reading("quota_now"))?;
    terms
        .iter()
        .try_fold(0u64, |a, t| a.checked_add(*t))
        .map(Some)
        .ok_or_else(|| reading("quota_now"))
}
/// The per cent a value read means (cut to a whole number: a value that is not one differs anyway).
fn percent(micros: Option<u64>) -> Value {
    micros.map_or(Value::Null, |m| json!(m / 10_000))
}

/// One turn (R1 to R12).
pub fn execute(s: &mut impl System) -> Result<Value, Failure> {
    let Some(c) = config(s)? else {
        return Ok(json!({"skipped": "no quota"}));
    };
    if !s.units() {
        return Ok(json!({"skipped": "no service manager"}));
    }
    let l = load(s, &c.slice)?;
    let before = quota_now(s, &c.slice)?;
    let quota = ((c.busy_limit - c.margin) * l.cpus - l.outside).clamp(c.floor, c.ceiling);
    let wanted = Some(quota as u64 * 10_000);
    let set = before != wanted;
    if set {
        let property = format!("CPUQuota={quota}%");
        let call = s.call(
            "systemctl",
            &args(&["set-property", "--runtime", &c.slice, &property]),
            &Value::Null,
            30_000,
        );
        require!(call.is_ok_and(|r| r.success) => "set_failed", "set_property");
        let after = quota_now(s, &c.slice)?;
        if after != wanted {
            return Err(fail("set_failed", "quota", percent(after)));
        }
    }
    Ok(
        json!({"at": stamp(s.wall()), "slice": c.slice, "cpus": l.cpus, "busy": l.busy, "slice_use": l.slice_use,
        "outside": l.outside, "before": percent(before), "quota": quota, "set": set}),
    )
}

/// The exit of a failure: 2 for a refusal before any reading, else 3.
pub fn exit_status(code: &str) -> u8 {
    if matches!(code, "usage" | "policy_invalid") {
        2
    } else {
        3
    }
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = if argv.is_empty() {
        execute(&mut Host)
    } else {
        Err(error("usage", "args"))
    };
    let (value, status) = match result {
        Ok(v) => (v, 0),
        Err(e) => (
            json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}}),
            exit_status(e.code),
        ),
    };
    let written = if status == 0 {
        writeln!(std::io::stdout(), "{value}")
    } else {
        writeln!(std::io::stderr(), "{value}")
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
