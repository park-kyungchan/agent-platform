//! The detector `drift` of `ap watch` (`specs/ap/watch.md` R20, R27): compares host files named
//! in `deployments/host-artifacts.json` at `origin/main` of `agent-vps-operations` (without fetching
//! or writing in that checkout) against their recorded SHA-256 digests.
use super::detect::output;
use super::{Read, Reading, Signal};
use crate::host::{System, digest};
use serde_json::Value;
use std::io::ErrorKind;

pub const VPS_OPS_REPO: &str = "/home/palantirkc/platforms/agent-vps-operations";
pub const INVENTORY_PATH: &str = "deployments/host-artifacts.json";
const MS: u64 = 10_000;

fn valid_destination(dest: &str) -> bool {
    dest.starts_with("deployments/")
        && !dest.ends_with('/')
        && !dest.contains('\0')
        && !dest
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
}

fn valid_source(source: &str) -> bool {
    source.starts_with('/') && !source.contains('\0')
}

fn is_blob(s: &mut impl System, commit: &str, dest: &str) -> Result<bool, String> {
    let spec = format!("{commit}:{dest}");
    let args = [
        "-C".to_owned(),
        VPS_OPS_REPO.to_owned(),
        "cat-file".to_owned(),
        "-t".to_owned(),
        spec,
    ];
    let reply = s
        .call("git", &args, &Value::Null, MS)
        .map_err(|_| "git cat-file could not run".to_owned())?;
    Ok(reply.success && String::from_utf8_lossy(&reply.stdout).trim() == "blob")
}

/// `drift` (R20, R27): compares host files against the inventory at `origin/main`.
pub(crate) fn detect(s: &mut impl System) -> Reading {
    let commit = output(
        s,
        "git",
        &[
            "-C",
            VPS_OPS_REPO,
            "rev-parse",
            "--verify",
            "origin/main^{commit}",
        ],
    )?;
    let commit = commit.trim().to_owned();
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("git rev-parse origin/main did not return a commit".into());
    }
    let spec = format!("{commit}:{INVENTORY_PATH}");
    let raw = output(s, "git", &["-C", VPS_OPS_REPO, "show", &spec])?;
    let rows: Value =
        serde_json::from_str(&raw).map_err(|_| format!("{INVENTORY_PATH}: not JSON"))?;
    let rows = rows
        .as_array()
        .ok_or_else(|| format!("{INVENTORY_PATH}: not a JSON array"))?;
    let mut parsed = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(obj) = row.as_object() else {
            return Err(format!("{INVENTORY_PATH}: row is not an object"));
        };
        let (Some(source), Some(destination), Some(sha256)) = (
            obj.get("source").and_then(Value::as_str),
            obj.get("destination").and_then(Value::as_str),
            obj.get("sha256").and_then(Value::as_str),
        ) else {
            return Err(format!(
                "{INVENTORY_PATH}: row lacks source, destination or sha256"
            ));
        };
        parsed.push((source, destination, sha256));
    }
    let mut r = Read::default();
    for (source, destination, expected) in parsed {
        if !valid_destination(destination) || !valid_source(source) {
            r.signals.push(Signal::new(
                format!("inventory {destination}"),
                "act",
                &format!("inventory row {destination} has an invalid path"),
                format!("commit {commit}, destination {destination}, source {source}"),
            ));
            continue;
        }
        if !is_blob(s, &commit, destination)? {
            r.signals.push(Signal::new(
                format!("inventory {destination}"),
                "act",
                &format!("inventory row {destination} is not a blob at {commit}"),
                format!("commit {commit}, destination {destination} is not a blob"),
            ));
            continue;
        }
        match s.read(source) {
            Ok(bytes) => {
                let actual = digest(&bytes);
                drop(bytes);
                if actual != expected {
                    r.signals.push(Signal::new(
                        format!("drift {destination}"),
                        "act",
                        &format!("host artifact {destination} differs from {commit}"),
                        format!("expected {expected}, actual {actual}, commit {commit}"),
                    ));
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                r.signals.push(Signal::new(
                    format!("drift {destination} absent"),
                    "act",
                    &format!("host artifact {destination} is absent"),
                    format!("expected {expected}, absent, commit {commit}"),
                ));
            }
            Err(_) => {
                return Err(format!("host artifact {source} cannot be read"));
            }
        }
    }
    Ok(r)
}
