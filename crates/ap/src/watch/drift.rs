//! The detector `drift` of `ap watch` (`specs/ap/watch.md` R20, R27 to R29): compares host files
//! named in `deployments/host-artifacts.json` at `origin/main` of `agent-vps-operations` against their
//! recorded SHA-256 digests (R27), the unit files of `deploy/` at the commit the installed `ap` names
//! (R28), and the files of `etc/` at the tip delivery's last fetch of the operations repository read
//! (R29). It only reads: no clone or checkout is fetched, checked out or written.
use super::detect::output;
use super::{Read, Reading, Signal};
use crate::host::{System, digest};
use serde_json::Value;
use std::io::ErrorKind;

pub const VPS_OPS_REPO: &str = "/home/palantirkc/platforms/agent-vps-operations";
pub const INVENTORY_PATH: &str = "deployments/host-artifacts.json";
/// The installed `ap`, whose `--version` names the commit delivery installed (R28).
pub const INSTALLED_AP: &str = "/usr/local/bin/ap";
/// Delivery's clone of agent-platform and of the operations repository: `<state>/<name>` of
/// `deploy/deliver.json`, its `repository` and its `etc.repository` (R28, R29).
pub const DELIVER_CLONE: &str = "/var/lib/agent-platform/deliver/agent-platform";
pub const ETC_CLONE: &str = "/var/lib/agent-platform/deliver/agent-vps-operations";
/// The host's directory of the unit files delivery installs (`specs/ap/deliver.md` R48).
pub const UNIT_DIR: &str = "/etc/systemd/system";
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
    units(s, &mut r)?;
    configuration(s, &mut r)?;
    Ok(r)
}
/// The standard output of one `git -C <clone>` call that ran and succeeded, as bytes; else the cause.
fn git_bytes(s: &mut impl System, clone: &str, words: &[&str]) -> Result<Vec<u8>, String> {
    let mut argv = vec!["-C".to_owned(), clone.to_owned()];
    argv.extend(words.iter().map(|w| (*w).to_owned()));
    let shown = format!(
        "git -C {clone} {}",
        words.first().copied().unwrap_or_default()
    );
    match s.call("git", &argv, &Value::Null, MS) {
        Ok(r) if r.success => Ok(r.stdout),
        Ok(_) => Err(format!("{shown} failed")),
        Err(_) => Err(format!("{shown} could not run")),
    }
}
fn hex40(t: &str) -> bool {
    t.len() == 40 && t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
/// The host's copy at `host` against the bytes `want` of `commit`: `drift <label>` when its bytes
/// differ, `drift <label> absent` when it is not found. The evidence holds the two digests and the
/// commit only.
fn compare(
    s: &mut impl System,
    r: &mut Read,
    label: &str,
    host: &str,
    want: &[u8],
    commit: &str,
) -> Result<(), String> {
    let expected = digest(want);
    match s.read(host) {
        Ok(bytes) => {
            let actual = digest(&bytes);
            drop(bytes);
            if actual != expected {
                r.signals.push(Signal::new(
                    format!("drift {label}"),
                    "act",
                    &format!("host file {host} differs from {label} at {commit}"),
                    format!("expected {expected}, actual {actual}, commit {commit}"),
                ));
            }
            Ok(())
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            r.signals.push(Signal::new(
                format!("drift {label} absent"),
                "act",
                &format!("host file {host} of {label} at {commit} is absent"),
                format!("expected {expected}, absent, commit {commit}"),
            ));
            Ok(())
        }
        Err(_) => Err(format!("host file {host} cannot be read")),
    }
}
/// R28: the unit files of the commit K the installed `ap` names, read from delivery's clone, against
/// their copies under `/etc/systemd/system`.
fn units(s: &mut impl System, r: &mut Read) -> Result<(), String> {
    let version = output(s, INSTALLED_AP, &["--version"])?;
    let words: Vec<&str> = version
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let k = match words.as_slice() {
        ["ap", _, k] if hex40(k) => (*k).to_owned(),
        _ => return Err(format!("{INSTALLED_AP} --version names no commit")),
    };
    let listed = git_bytes(s, DELIVER_CLONE, &["ls-tree", &k, "deploy/"])?;
    let listed = String::from_utf8(listed).map_err(|_| "git ls-tree: not UTF-8".to_owned())?;
    for name in crate::deliver::unit_entries(&listed) {
        let spec = format!("{k}:deploy/{name}");
        let want = git_bytes(s, DELIVER_CLONE, &["show", &spec])?;
        let host = format!("{UNIT_DIR}/{name}");
        compare(s, r, &format!("deploy/{name}"), &host, &want, &k)?;
    }
    Ok(())
}
/// A path of `etc/` the watch compares: ASCII letters, digits, `/`, `.`, `_`, `@` and `-`, with no
/// empty, `.` or `..` component.
fn plain_path(p: &str) -> bool {
    p.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/._@-".contains(&b))
        && !p.split('/').any(|c| c.is_empty() || c == "." || c == "..")
}
/// R29: the files of `etc/` at the tip E of delivery's last fetch of the operations repository,
/// against the host's files of the same path under `/`. No clone, or no fetched tip: nothing.
fn configuration(s: &mut impl System, r: &mut Read) -> Result<(), String> {
    match s.read(&format!("{ETC_CLONE}/.git/HEAD")) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(format!("{ETC_CLONE}/.git/HEAD cannot be read")),
    }
    let refs = git_bytes(
        s,
        ETC_CLONE,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            "refs/remotes/origin/main",
        ],
    )?;
    let refs = String::from_utf8_lossy(&refs);
    let lines: Vec<&str> = refs.lines().collect();
    let tip = match lines.as_slice() {
        [] => return Ok(()),
        [one] if hex40(one) => (*one).to_owned(),
        _ => return Err("git for-each-ref did not answer one commit".into()),
    };
    let listed = git_bytes(s, ETC_CLONE, &["ls-tree", "-r", "-z", &tip, "etc/"])?;
    for entry in listed.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let (meta, path) = entry.split_once('\t').unwrap_or(("", &entry));
        let meta: Vec<&str> = meta.split(' ').collect();
        let file = meta.len() == 3
            && meta[1] == "blob"
            && (meta[0] == "100644" || meta[0] == "100755")
            && plain_path(path);
        if !file {
            r.signals.push(Signal::new(
                format!("inventory {path}"),
                "act",
                &format!(
                    "entry {path} of the operations repository is not a file the watch compares"
                ),
                format!("commit {tip}, entry {path} is not a plain file"),
            ));
            continue;
        }
        let want = git_bytes(s, ETC_CLONE, &["show", &format!("{tip}:{path}")])?;
        compare(s, r, path, &format!("/{path}"), &want, &tip)?;
    }
    Ok(())
}
