//! The detector `accounts` of `ap watch` (`specs/ap/watch.md` R20 to R22): Claude sign-in
//! expiry from `<dir>/.credentials.json` (the one exception to reading no credential file) and
//! recent `run` usage limits from the store.
use super::{Read, Reading, Signal, epoch, utc};
use crate::host::System;
use crate::store;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::ErrorKind,
    path::Path,
};

const SIGN_IN_WINDOW: u64 = 72 * 3_600;
const LIMIT_WINDOW: u64 = 6 * 3_600;
const LIMIT_FLOOR: f64 = 0.9;

enum Expiry {
    NotJson,
    Missing,
    Ms(u64),
}

fn bare(path: &str) -> String {
    if path.len() > 1 {
        path.trim_end_matches('/').to_owned()
    } else {
        path.to_owned()
    }
}

fn claude_dirs(accounts: &Value) -> Result<BTreeSet<String>, String> {
    let map = accounts
        .as_object()
        .ok_or_else(|| "policy accounts is not an object".to_owned())?;
    let mut dirs = BTreeSet::new();
    for (key, value) in map {
        if !key.starts_with("claude") {
            continue;
        }
        let list = value
            .as_array()
            .ok_or_else(|| "policy accounts entry is not an array".to_owned())?;
        for entry in list {
            let dir = entry
                .as_str()
                .filter(|p| Path::new(p).is_absolute() && !p.contains('\0'))
                .ok_or_else(|| "policy accounts directory is invalid".to_owned())?;
            dirs.insert(bare(dir));
        }
    }
    Ok(dirs)
}

fn read_expiry(s: &impl System, path: &str) -> Result<Option<Expiry>, String> {
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("credentials_unreadable".to_owned()),
    };
    let parsed = match serde_json::from_slice::<Value>(&bytes) {
        Err(_) => Expiry::NotJson,
        Ok(v) => match v
            .get("claudeAiOauth")
            .and_then(|o| o.get("refreshTokenExpiresAt"))
            .and_then(Value::as_u64)
        {
            Some(ms) => Expiry::Ms(ms),
            None => Expiry::Missing,
        },
    };
    drop(bytes);
    Ok(Some(parsed))
}

fn collect_sign_in(
    s: &impl System,
    now: u64,
    dirs: &BTreeSet<String>,
    out: &mut Vec<Signal>,
) -> Result<(), String> {
    for dir in dirs {
        let path = if dir == "/" {
            "/.credentials.json".to_owned()
        } else {
            format!("{dir}/.credentials.json")
        };
        match read_expiry(s, &path)? {
            None => out.push(Signal::new(
                format!("sign-in {dir} absent"),
                "act",
                "Claude sign-in credentials are absent",
                "credentials_absent".into(),
            )),
            Some(Expiry::NotJson) => out.push(Signal::new(
                format!("sign-in {dir} absent"),
                "act",
                "Claude sign-in credentials could not be parsed",
                "credentials_not_json".into(),
            )),
            Some(Expiry::Missing) => out.push(Signal::new(
                format!("sign-in {dir} absent"),
                "act",
                "Claude sign-in credentials lack refreshTokenExpiresAt",
                "credentials_expiry_missing".into(),
            )),
            Some(Expiry::Ms(ms)) => {
                let end = ms / 1_000;
                if end <= now {
                    out.push(Signal::new(
                        format!("sign-in {dir}"),
                        "urgent",
                        "Claude sign-in has expired",
                        format!("expires_at {}", utc(end)),
                    ));
                } else if end <= now.saturating_add(SIGN_IN_WINDOW) {
                    out.push(Signal::new(
                        format!("sign-in {dir}"),
                        "act",
                        "Claude sign-in expires within 72 hours",
                        format!("expires_at {}", utc(end)),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn record_window(
    windows: &mut BTreeMap<String, (u64, String, f64)>,
    name: &str,
    started: u64,
    key: &str,
    utilization: f64,
) {
    let replace = match windows.get(name) {
        None => true,
        Some((prev_at, prev_key, _)) => (started, key) > (*prev_at, prev_key.as_str()),
    };
    if replace {
        windows.insert(name.to_owned(), (started, key.to_owned(), utilization));
    }
}

fn collect_limits(s: &mut impl System, now: u64, out: &mut Vec<Signal>) -> Result<(), String> {
    let cutoff = now.saturating_sub(LIMIT_WINDOW);
    let since = utc(cutoff);
    let runs = store::search(s, None, "run", vec![json!({"ge": ["started_at", since]})])
        .map_err(|f| format!("ont search run failed: {}", store::code(&f)))?;
    let mut by_account: BTreeMap<String, BTreeMap<String, (u64, String, f64)>> = BTreeMap::new();
    for (key, fields) in runs {
        let Some(started) = fields["started_at"]
            .as_str()
            .and_then(epoch)
            .filter(|t| *t >= cutoff)
        else {
            continue;
        };
        let Some(account) = fields["account"].as_str().filter(|a| !a.is_empty()) else {
            continue;
        };
        let Some(limit_text) = fields["limit"].as_str() else {
            continue;
        };
        let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(limit_text) else {
            continue;
        };
        if let Some(windows) = obj.get("windows").and_then(Value::as_object) {
            let entry = by_account.entry(account.to_owned()).or_default();
            for (wname, wval) in windows {
                if let Some(u) = wval
                    .get("utilization")
                    .and_then(Value::as_f64)
                    .filter(|u| u.is_finite())
                {
                    record_window(entry, wname, started, &key, u);
                }
            }
        } else if let Some(u) = obj
            .get("utilization")
            .and_then(Value::as_f64)
            .filter(|u| u.is_finite())
        {
            let wname = obj
                .get("window")
                .and_then(Value::as_str)
                .filter(|w| !w.is_empty())
                .unwrap_or("window");
            let entry = by_account.entry(account.to_owned()).or_default();
            record_window(entry, wname, started, &key, u);
        }
    }
    for (account, windows) in by_account {
        let high = windows
            .into_iter()
            .filter(|(_, (_, _, u))| *u >= LIMIT_FLOOR)
            .max_by(|(wa, (_, _, ua)), (wb, (_, _, ub))| ua.total_cmp(ub).then_with(|| wb.cmp(wa)));
        if let Some((wname, (_, run_key, u))) = high {
            out.push(Signal::new(
                format!("limit {account}"),
                "info",
                &format!("account {account} usage limit {wname} reached {u}"),
                format!("window {wname}, utilization {u}, run {run_key}"),
            ));
        }
    }
    Ok(())
}

/// `accounts` (R20 to R22): Claude sign-in expiry and recent `run` usage limits.
pub(crate) fn detect(
    s: &mut impl System,
    now: u64,
    policy: &Result<Option<Value>, String>,
) -> Reading {
    let policy = match policy {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Ok(Read {
                signals: vec![],
                unknown: vec!["sign-in".into(), "limit".into()],
                state: None,
            });
        }
        Err(cause) => return Err(cause.clone()),
    };
    let mut r = Read::default();
    match policy.get("accounts") {
        None => r.unknown.push("sign-in".into()),
        Some(accounts) => {
            let dirs = claude_dirs(accounts)?;
            collect_sign_in(s, now, &dirs, &mut r.signals)?;
        }
    }
    collect_limits(s, now, &mut r.signals)?;
    Ok(r)
}
