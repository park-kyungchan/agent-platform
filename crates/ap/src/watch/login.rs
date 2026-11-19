//! The detector `login` of `ap watch` (`specs/ap/watch.md` R14 to R16): the reading of the login path,
//! part by part, compared with the one `ap watch --record-login` wrote to `login.json`. The TOTP secret
//! file is read by `stat` alone (its presence); no byte of it is read.
use super::detect::{file_digest, kind, output, regular};
use super::{Read, Reading, Signal};
use crate::host::{System, digest};
use serde_json::{Map, Value, json};
use std::path::Path;

/// The parts, in the order of a reading: each is the `<part>` of a signal `login <part>`.
pub const PARTS: [&str; 13] = [
    "sshd -T",
    "/etc/ssh/sshd_config.d/",
    "/etc/pam.d/sshd",
    "/etc/systemd/system/ssh.socket.d/",
    "/etc/sudoers",
    "/etc/sudoers.d/",
    "/etc/systemd/system/ssh.service.d/",
    "/etc/systemd/system/ssh.service.requires/",
    "ssh.service drop-ins",
    "ssh.socket drop-ins",
    "ssh.service fragment",
    "ssh.socket fragment",
    "totp file",
];
/// `sshd -T` with its exit code on the last line: `exit <code>`.
const SSHD: &str = "sshd -T 2>/dev/null; echo \"exit $?\"";

/// A directory part: its entries by name, type and, for a link, its target text; a regular file also
/// by its SHA-256. A directory `stat` does not find is the empty set; another type has no entries.
fn directory(s: &mut impl System, part: &str) -> Result<Value, String> {
    let path = part.trim_end_matches('/');
    match kind(s, path)? {
        None => return Ok(json!({"entries": []})),
        Some(t) if t != "directory" => return Ok(json!({"type": t, "entries": []})),
        Some(_) => {}
    }
    let words = [
        path,
        "-mindepth",
        "1",
        "-maxdepth",
        "1",
        "-printf",
        "%f\\t%y\\t%l\\n",
    ];
    let listed = output(s, "find", &words)?;
    let mut entries = vec![];
    for line in listed.lines() {
        let [name, y, target] = line.split('\t').collect::<Vec<_>>()[..] else {
            return Err(format!("find {path}: an entry cannot be read"));
        };
        let mut e = json!({"name": name, "type": y});
        match y {
            "l" => e["target"] = json!(target),
            "f" => e["sha256"] = json!(file_digest(s, &format!("{path}/{name}"))?),
            _ => {}
        }
        entries.push(e);
    }
    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!({"entries": entries}))
}
/// A file's part: the path as printed, its type (`empty` for an empty path, `absent` when `stat` does
/// not find it) and, for a regular file, its SHA-256.
fn file(s: &mut impl System, path: &str) -> Result<Value, String> {
    if path.is_empty() {
        return Ok(json!({"path": "", "type": "empty"}));
    }
    let Some(t) = kind(s, path)? else {
        return Ok(json!({"path": path, "type": "absent"}));
    };
    let mut v = json!({"path": path, "type": t});
    if regular(&t) {
        v["sha256"] = json!(file_digest(s, path)?);
    }
    Ok(v)
}
/// `sshd -T`'s output with its lines sorted, by its SHA-256; `exit <code>` when it exits non-zero.
fn sshd(s: &mut impl System) -> Result<Value, String> {
    let out = output(s, "sh", &["-c", SSHD])?;
    let mut lines: Vec<&str> = out.lines().collect();
    let code = lines
        .pop()
        .and_then(|l| l.strip_prefix("exit "))
        .ok_or("sshd -T: no exit code")?;
    if code != "0" {
        return Ok(json!(format!("exit {code}")));
    }
    lines.sort_unstable();
    Ok(json!(format!(
        "sha256 {}",
        digest(lines.join("\n").as_bytes())
    )))
}
/// The `FragmentPath` and the files of `DropInPaths` of a unit, as `systemctl show` prints them.
fn unit(s: &mut impl System, name: &str) -> Result<(Value, Value), String> {
    let shown = output(
        s,
        "systemctl",
        &["show", "-p", "FragmentPath", "-p", "DropInPaths", name],
    )?;
    let prop = |p: &str| {
        shown
            .lines()
            .find_map(|l| l.strip_prefix(p)?.strip_prefix('='))
            .map(str::to_owned)
    };
    let (Some(fragment), Some(drop_ins)) = (prop("FragmentPath"), prop("DropInPaths")) else {
        return Err(format!(
            "systemctl show {name}: no FragmentPath or DropInPaths"
        ));
    };
    let mut files = vec![];
    for path in drop_ins.split_whitespace() {
        files.push(file(s, path)?);
    }
    Ok((file(s, &fragment)?, json!(files)))
}
/// The reading of every part, with the TOTP file at `totp` (R14, R15).
pub(crate) fn reading(s: &mut impl System, totp: &str) -> Result<Map<String, Value>, String> {
    let mut parts = Map::new();
    parts.insert(PARTS[0].into(), sshd(s)?);
    for part in &PARTS[1..8] {
        let v = match part.ends_with('/') {
            true => directory(s, part)?,
            false => {
                let mut v = file(s, part)?;
                v.as_object_mut().map(|o| o.remove("path"));
                v
            }
        };
        parts.insert((*part).into(), v);
    }
    let (service_fragment, service_drop_ins) = unit(s, "ssh.service")?;
    let (socket_fragment, socket_drop_ins) = unit(s, "ssh.socket")?;
    parts.insert(PARTS[8].into(), service_drop_ins);
    parts.insert(PARTS[9].into(), socket_drop_ins);
    parts.insert(PARTS[10].into(), service_fragment);
    parts.insert(PARTS[11].into(), socket_fragment);
    let presence = kind(s, totp)?.unwrap_or_else(|| "absent".into());
    parts.insert(PARTS[12].into(), json!({"type": presence}));
    Ok(parts)
}
/// The SHA-256 of a part's value as `login.json` holds it.
fn value_digest(v: &Value) -> String {
    digest(v.to_string().as_bytes())
}
/// `login` (R16): no `login.json` is `no baseline`; else one `login <part>` per part that differs.
pub(crate) fn detect(s: &mut impl System, dir: &Path) -> Reading {
    let mut r = Read::default();
    let recorded = match std::fs::read(dir.join("login.json")) {
        Ok(b) => {
            serde_json::from_slice::<Value>(&b).map_err(|_| "login.json is not JSON".to_owned())?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let what = "no login reading is recorded: the Lead runs ap watch --record-login";
            let evidence = format!("{} is absent", dir.join("login.json").display());
            r.signals
                .push(Signal::new("no baseline".into(), "act", what, evidence));
            r.unknown = PARTS.iter().map(|p| format!("login {p}")).collect();
            return Ok(r);
        }
        Err(_) => return Err("login.json cannot be read".into()),
    };
    let (Some(totp), Some(parts)) = (
        recorded["totp_file"].as_str(),
        recorded["parts"].as_object(),
    ) else {
        return Err("login.json has no totp_file or parts".into());
    };
    let now = reading(s, totp)?;
    for part in PARTS {
        let (was, is) = (parts.get(part).unwrap_or(&Value::Null), &now[part]);
        if was != is {
            let what = "the login path differs from the reading login.json recorded";
            let evidence = format!(
                "part {part}: recorded {}, now {} (SHA-256 of the part's value as login.json holds it)",
                value_digest(was),
                value_digest(is)
            );
            r.signals.push(Signal::new(
                format!("login {part}"),
                "urgent",
                what,
                evidence,
            ));
        }
    }
    Ok(r)
}
