//! The detector `drift` of `ap watch` (`specs/ap/watch.md` R20, R27 to R29, R33, R34 to R36): at
//! the commit K the installed `ap` names, read from delivery's clone of this repository, it compares
//! host files named in `host/deployments/host-artifacts.json` against their recorded SHA-256 digests
//! (R27), the unit files of `deploy/` (R28) and the files of `host/etc/` (R29) against the host's
//! copies, and when the policy names `hosts.pc.ssh` (`apcheck@...`) and the PC is reachable, the
//! checks account's `ont` binary against `/usr/local/bin/ont` (R33). It only reads: no clone or
//! checkout is fetched, checked out or written.
use super::detect::output;
use super::{Read, Reading, Signal};
use crate::host::{System, digest};
use serde_json::Value;
use std::io::ErrorKind;

/// The tree of the host's configuration in this repository; every path of the inventory and of `etc/`
/// is read under it (R34).
pub const HOST_TREE: &str = "host/";
pub const INVENTORY_PATH: &str = "host/deployments/host-artifacts.json";
/// The installed `ap`, whose `--version` names the commit delivery installed (R28).
pub const INSTALLED_AP: &str = "/usr/local/bin/ap";
/// The installed `ont`, whose digest and `--version` the PC checks binary is compared against (R33).
pub const INSTALLED_ONT: &str = "/usr/local/bin/ont";
/// Default remote binary path on the PC checks account when `deploy/deliver.json` names none (R33).
pub const DEFAULT_PC_ONT: &str = "/home/apcheck/.cargo/bin/ont";
/// Delivery's clone of lifecycle-platform: `<state>/<name>` of `deploy/deliver.json`, its `repository`
/// (R27 to R29).
pub const DELIVER_CLONE: &str = "/var/lib/agent-platform/deliver/lifecycle-platform";
/// The host's directory of the unit files delivery installs (`specs/ap/deliver.md` R48).
pub const UNIT_DIR: &str = "/etc/systemd/system";
const MS: u64 = 10_000;
const PC_PROBE_MS: u64 = 15_000;
const PC_DIGEST_SCRIPT: &str = r#"p=$1; if [ ! -e "$p" ] && [ ! -L "$p" ]; then printf 'ABSENT\n'; elif [ -f "$p" ] && [ ! -L "$p" ]; then sha256sum -- "$p" | awk '{print $1}'; else printf 'NOT_REGULAR\n'; fi"#;

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
    let spec = format!("{commit}:{HOST_TREE}{dest}");
    let args = [
        "-C".to_owned(),
        DELIVER_CLONE.to_owned(),
        "cat-file".to_owned(),
        "-t".to_owned(),
        spec,
    ];
    let reply = s
        .call("git", &args, &Value::Null, MS)
        .map_err(|_| "git cat-file could not run".to_owned())?;
    Ok(reply.success && String::from_utf8_lossy(&reply.stdout).trim() == "blob")
}

/// `drift` (R20, R27 to R29, R33): compares host files against the inventory, unit files and
/// configuration files at the installed commit K, and the PC checks account's `ont` binary.
pub(crate) fn detect(s: &mut impl System, policy: &Result<Option<Value>, String>) -> Reading {
    let k = installed_commit(s)?;
    let mut r = Read::default();
    inventory(s, &mut r, &k)?;
    units(s, &mut r, &k)?;
    configuration(s, &mut r, &k)?;
    pc_ont(s, &mut r, policy, &k)?;
    Ok(r)
}
/// R27: the rows of `host/deployments/host-artifacts.json` at K, each host file against its digest;
/// a row's `destination` is relative to `host/`.
fn inventory(s: &mut impl System, r: &mut Read, commit: &str) -> Result<(), String> {
    let spec = format!("{commit}:{INVENTORY_PATH}");
    let raw = output(s, "git", &["-C", DELIVER_CLONE, "show", &spec])?;
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
        if !is_blob(s, commit, destination)? {
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
    Ok(())
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
/// K, the commit the installed `ap` names in the first line of its `--version` (R28).
fn installed_commit(s: &mut impl System) -> Result<String, String> {
    let version = output(s, INSTALLED_AP, &["--version"])?;
    let words: Vec<&str> = version
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    match words.as_slice() {
        ["ap", _, k] if hex40(k) => Ok((*k).to_owned()),
        _ => Err(format!("{INSTALLED_AP} --version names no commit")),
    }
}
/// R28: the unit files of K, read from delivery's clone, against their copies under
/// `/etc/systemd/system`.
fn units(s: &mut impl System, r: &mut Read, k: &str) -> Result<(), String> {
    let listed = git_bytes(s, DELIVER_CLONE, &["ls-tree", k, "deploy/"])?;
    let listed = String::from_utf8(listed).map_err(|_| "git ls-tree: not UTF-8".to_owned())?;
    for name in crate::deliver::unit_entries(&listed) {
        let spec = format!("{k}:deploy/{name}");
        let want = git_bytes(s, DELIVER_CLONE, &["show", &spec])?;
        let host = format!("{UNIT_DIR}/{name}");
        compare(s, r, &format!("deploy/{name}"), &host, &want, k)?;
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
/// R29: the files of `host/etc/` at K, read from delivery's clone, against the host's files of the
/// path without `host/` under `/` (`host/etc/x` is `/etc/x`); the signature names that path.
fn configuration(s: &mut impl System, r: &mut Read, k: &str) -> Result<(), String> {
    let tree = format!("{HOST_TREE}etc/");
    let listed = git_bytes(s, DELIVER_CLONE, &["ls-tree", "-r", "-z", k, &tree])?;
    for entry in listed.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let (meta, full) = entry.split_once('\t').unwrap_or(("", &entry));
        let path = full.strip_prefix(HOST_TREE).unwrap_or(full);
        let meta: Vec<&str> = meta.split(' ').collect();
        let file = meta.len() == 3
            && meta[1] == "blob"
            && (meta[0] == "100644" || meta[0] == "100755")
            && full.starts_with(&tree)
            && plain_path(path);
        if !file {
            r.signals.push(Signal::new(
                format!("inventory {path}"),
                "act",
                &format!("entry {full} of host/etc/ is not a file the watch compares"),
                format!("commit {k}, entry {full} is not a plain file"),
            ));
            continue;
        }
        let want = git_bytes(s, DELIVER_CLONE, &["show", &format!("{k}:{full}")])?;
        compare(s, r, path, &format!("/{path}"), &want, k)?;
    }
    Ok(())
}

fn hex64(t: &str) -> bool {
    t.len() == 64 && t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// R33: read-only SHA-256 drift check of the checks account's `ont` binary when the pass's policy
/// reading holds a valid `apcheck@<node>` `hosts.pc.ssh` and the PC answers within 15 seconds.
fn pc_ont(
    s: &mut impl System,
    r: &mut Read,
    policy: &Result<Option<Value>, String>,
    k: &str,
) -> Result<(), String> {
    let Ok(Some(policy)) = policy else {
        return Ok(());
    };
    let Ok(Some(pc)) = crate::check::hosts(policy) else {
        return Ok(());
    };
    if !crate::deliver::valid_pc_ssh(&pc.ssh) {
        return Ok(());
    }
    let bin = git_bytes(
        s,
        DELIVER_CLONE,
        &["show", &format!("{k}:deploy/deliver.json")],
    )
    .ok()
    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    .and_then(|v| {
        v.pointer("/pc/bin")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
    .filter(|b| crate::deliver::valid_pc_bin(b))
    .unwrap_or_else(|| DEFAULT_PC_ONT.to_owned());
    let cmd = format!(
        "sh -c {} ap-watch-pc {}",
        crate::check::quote(PC_DIGEST_SCRIPT),
        crate::check::quote(&bin)
    );
    let env = s.environment();
    let scratch = std::env::temp_dir();
    let Some((true, out, _)) = crate::deliver::tailscale_ssh(
        s,
        &crate::deliver::SshCall {
            env: &env,
            cwd: "/",
            ssh: &pc.ssh,
            cmd: &cmd,
            stdin: None,
            ms: PC_PROBE_MS,
            scratch: &scratch,
        },
    ) else {
        return Ok(());
    };
    let answer = out.lines().next().map(str::trim).unwrap_or_default();
    let absent = answer == "ABSENT";
    let actual = if absent {
        None
    } else if hex64(answer) {
        Some(answer.to_owned())
    } else if answer == "NOT_REGULAR" {
        Some("not_regular".to_owned())
    } else {
        return Ok(());
    };
    let local_bytes = s
        .read(INSTALLED_ONT)
        .map_err(|_| format!("{INSTALLED_ONT} cannot be read"))?;
    let expected = digest(&local_bytes);
    let version = output(s, INSTALLED_ONT, &["--version"])?;
    let words: Vec<&str> = version
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let commit = match words.as_slice() {
        ["ont", _, c] if hex40(c) => (*c).to_owned(),
        _ => return Err(format!("{INSTALLED_ONT} --version names no commit")),
    };
    if absent {
        r.signals.push(Signal::new(
            "drift pc ont absent".to_owned(),
            "act",
            &format!("PC checks binary {bin} is absent"),
            format!("expected {expected}, absent, commit {commit}"),
        ));
    } else if let Some(actual) = actual
        && actual != expected
    {
        r.signals.push(Signal::new(
            "drift pc ont".to_owned(),
            "act",
            &format!("PC checks binary {bin} differs from {commit}"),
            format!("expected {expected}, actual {actual}, commit {commit}"),
        ));
    }
    Ok(())
}
