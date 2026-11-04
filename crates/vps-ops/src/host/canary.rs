use crate::{
    Result,
    runner::{CommandSpec, Runner},
    util,
};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
pub const HOST: &str = "srv1873899";
pub const UNIT: &str = "hermes-management-canary.service";
pub const UNIT_PATH: &str = "/etc/systemd/system/hermes-management-canary.service";
pub const STATE_DIR: &str = "/var/lib/hermes-hostctl-canary";
pub const TARGET: &str = "host:srv1873899/systemd:hermes-management-canary.service";
pub const UNIT_TEXT: &str =
    include_str!("../../../../deployments/hermes-canary/hermes-management-canary.service");
pub fn unit_sha() -> String {
    util::hash(UNIT_TEXT)
}
const PROPERTIES: [&str; 10] = [
    "LoadState",
    "ActiveState",
    "SubState",
    "MainPID",
    "InvocationID",
    "ExecMainStartTimestampMonotonic",
    "InactiveEnterTimestampMonotonic",
    "FragmentPath",
    "NeedDaemonReload",
    "DropInPaths",
];
pub trait Backend {
    fn observe(&mut self) -> Result<Value>;
    fn change(&mut self, action: &str) -> Result<Value>;
}
fn root_host() -> Result<()> {
    let s = fs::read_to_string("/proc/self/status")?;
    let uid = s
        .lines()
        .find(|s| s.starts_with("Uid:"))
        .and_then(|s| s.split_whitespace().nth(2))
        .unwrap_or("");
    if uid != "0" || fs::read_to_string("/proc/sys/kernel/hostname")?.trim() != HOST {
        return Err("HOST_BOUNDARY_REQUIRED".into());
    }
    Ok(())
}
fn owned_directory(p: &Path) -> Result<()> {
    for p in p.ancestors() {
        let m = fs::symlink_metadata(p)?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err("UNSAFE_INSTALL_DIRECTORY".into());
        }
    }
    Ok(())
}
fn owned_file(p: &Path, max: u64) -> Result<Vec<u8>> {
    owned_directory(p.parent().ok_or("UNSAFE_INSTALL_DIRECTORY")?)?;
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(0x20000 | 0x800)
        .open(p)?;
    let m = f.metadata()?;
    if !m.is_file() || m.uid() != 0 || m.nlink() != 1 || m.mode() & 0o022 != 0 || m.len() > max {
        return Err("UNSAFE_INSTALL_FILE".into());
    }
    use std::io::Read;
    let mut b = Vec::new();
    f.take(max + 1).read_to_end(&mut b)?;
    if b.len() > max as usize || b.len() as u64 != m.len() {
        return Err("UNSAFE_INSTALL_FILE".into());
    }
    Ok(b)
}
fn host_lock() -> Result<File> {
    root_host()?;
    owned_directory(Path::new(STATE_DIR))?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(0x20000 | 0x800)
        .open(Path::new(STATE_DIR).join("control.lock"))?;
    let m = f.metadata()?;
    if !m.is_file() || m.uid() != 0 || m.nlink() != 1 || m.mode() & 0o777 != 0o600 {
        return Err("UNSAFE_LOCK".into());
    }
    f.try_lock().map_err(|_| "BUSY")?;
    Ok(f)
}
pub struct SystemdBackend<'a> {
    pub runner: &'a mut dyn Runner,
}
impl SystemdBackend<'_> {
    fn run(&mut self, args: Vec<String>) -> Result<crate::runner::CommandOutput> {
        let mut a = vec!["--no-pager".into(), "--no-ask-password".into()];
        a.extend(args);
        let mut s = CommandSpec::new("/usr/bin/systemctl", a);
        s.timeout_ms = 20_000;
        s.max_stdout_bytes = 16384;
        s.clear_environment = true;
        s.cwd = Some("/".into());
        s.environment = [
            ("PATH", "/usr/sbin:/usr/bin:/sbin:/bin"),
            ("LANG", "C"),
            ("LC_ALL", "C"),
            ("SYSTEMD_COLORS", "0"),
            ("SYSTEMD_PAGERSECURE", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        self.runner.run(s)
    }
}
impl Backend for SystemdBackend<'_> {
    fn observe(&mut self) -> Result<Value> {
        let raw = owned_file(Path::new(UNIT_PATH), 16384)?;
        if util::hash(raw) != unit_sha() {
            return Err("TARGET_IDENTITY_UNVERIFIED".into());
        }
        let mut args = vec!["show".into(), UNIT.into()];
        args.extend(PROPERTIES.iter().map(|p| format!("--property={p}")));
        let r = self.run(args)?;
        if r.exit_code != 0 || r.stdout.len() > 16384 {
            return Err("SYSTEMD_READ_FAILED".into());
        }
        let mut v = std::collections::BTreeMap::new();
        for l in r.stdout.lines() {
            let (k, x) = l.split_once('=').ok_or("SYSTEMD_OUTPUT_INVALID")?;
            if !PROPERTIES.contains(&k) || v.insert(k, x).is_some() {
                return Err("SYSTEMD_OUTPUT_INVALID".into());
            }
        }
        if v.len() != PROPERTIES.len() {
            return Err("SYSTEMD_OUTPUT_INVALID".into());
        }
        if v["FragmentPath"] != UNIT_PATH
            || v["NeedDaemonReload"] != "no"
            || !v["DropInPaths"].is_empty()
        {
            return Err("TARGET_IDENTITY_UNVERIFIED".into());
        }
        if !util::matches("^[0-9]+$", v["MainPID"]) {
            return Err("SYSTEMD_OUTPUT_INVALID".into());
        }
        Ok(
            json!({"load":v["LoadState"],"active":v["ActiveState"],"sub":v["SubState"],"pid":v["MainPID"].parse::<u64>()?,"invocation_id":v["InvocationID"],"generation_us":v["ExecMainStartTimestampMonotonic"],"inactive_us":v["InactiveEnterTimestampMonotonic"],"boot_id":fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim(),"unit_sha256":unit_sha()}),
        )
    }
    fn change(&mut self, action: &str) -> Result<Value> {
        if !["start", "stop"].contains(&action) {
            return Err("UNSUPPORTED_ACTION".into());
        }
        let r = self.run(vec![action.into(), UNIT.into()])?;
        Ok(
            json!({"returncode":if r.reason=="deadline-exceeded"{None}else{Some(r.exit_code)},"timed_out":r.reason=="deadline-exceeded"}),
        )
    }
}
pub fn state(v: &Value) -> Result<Value> {
    let keys = [
        "load",
        "active",
        "sub",
        "pid",
        "invocation_id",
        "generation_us",
        "inactive_us",
        "boot_id",
        "unit_sha256",
    ];
    let Some(o) = v.as_object() else {
        return Err("INVALID_OBSERVATION".into());
    };
    if o.len() != keys.len()
        || keys.iter().any(|k| !o.contains_key(*k))
        || v["pid"].as_u64().is_none_or(|n| n > 2147483647)
        || keys
            .iter()
            .filter(|k| **k != "pid")
            .any(|k| v[k].as_str().is_none_or(|s| s.chars().count() > 128))
        || !util::matches(
            "^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$",
            v["boot_id"].as_str().unwrap_or(""),
        )
        || !util::matches(
            "^(?:[0-9a-f]{32})?$",
            v["invocation_id"].as_str().unwrap_or(""),
        )
        || ["generation_us", "inactive_us"]
            .iter()
            .any(|k| !util::matches("^[0-9]{1,24}$", v[k].as_str().unwrap_or("")))
        || ["active", "sub"]
            .iter()
            .any(|k| !util::matches("^[a-z-]{1,32}$", v[k].as_str().unwrap_or("")))
    {
        return Err("INVALID_OBSERVATION".into());
    }
    if v["load"] != "loaded" || v["unit_sha256"] != unit_sha() {
        return Err("TARGET_IDENTITY_UNVERIFIED".into());
    }
    if v["active"] == "active" && (v["pid"] == 0 || v["invocation_id"] == "")
        || v["active"] == "inactive" && v["pid"] != 0
    {
        return Err("INVALID_OBSERVATION".into());
    }
    Ok(v.clone())
}
pub fn observation(v: &Value) -> Value {
    let mut canonical = json!({"schema":1,"target":TARGET,"state":v});
    canonical.sort_all_objects();
    json!({"state":v,"sha256":util::hash(serde_json::to_vec(&canonical).expect("canary state JSON"))})
}
fn reason(base: &Value, status: &str, why: &str, code: i32) -> (Value, i32) {
    let mut v = base.clone();
    v["status"] = json!(status);
    v["reason"] = json!(why);
    (v, code)
}
pub fn dispatch(
    argv: &[String],
    backend: &mut dyn Backend,
    stamp: &str,
    execution_id: &str,
) -> (Value, i32) {
    let mut base = json!({"schema":"hermes.host-canary.v1","target":TARGET,"observed_at_utc":stamp,"execution_id":execution_id,"invocation_attempted":false});
    if !((argv.len() == 1 && ["capabilities", "status"].contains(&argv[0].as_str()))
        || (argv.len() == 2
            && ["start", "stop"].contains(&argv[0].as_str())
            && util::digest(&argv[1])))
    {
        return reason(&base, "REJECTED", "UNSUPPORTED_INGRESS", 2);
    }
    if argv[0] == "capabilities" {
        return match std::env::current_exe().and_then(fs::read) {
            Ok(b) => {
                base["status"] = json!("DECLARED_CAPABILITIES");
                base["installed"] = Value::Null;
                base["commands"] = json!(["capabilities", "status", "start", "stop"]);
                base["production_controls"] = json!(false);
                base["unit_sha256"] = json!(unit_sha());
                base["controller_sha256"] = json!(util::hash(b));
                (base, 0)
            }
            Err(_) => reason(&base, "UNKNOWN", "CODE_IDENTITY_UNAVAILABLE", 3),
        };
    }
    let action = &argv[0];
    let work = (|| -> Result<(Value, i32)> {
        let before = state(&backend.observe()?)?;
        let obs = observation(&before);
        if action == "status" {
            base["status"] = json!("OBSERVED");
            base["observation"] = obs;
            return Ok((base.clone(), 0));
        }
        if argv[1] != obs["sha256"] {
            let (mut v, c) = reason(&base, "REJECTED", "PRESTATE_CHANGED", 2);
            v["observation"] = obs;
            return Ok((v, c));
        }
        if !((before["active"] == "active" && before["sub"] == "running")
            || (before["active"] == "inactive" && before["sub"] == "dead"))
        {
            let (mut v, c) = reason(&base, "REJECTED", "STATE_NOT_QUIESCENT", 2);
            v["observation"] = obs;
            return Ok((v, c));
        }
        let desired = if action == "start" {
            "active"
        } else {
            "inactive"
        };
        if before["active"] == desired {
            base["status"] = json!("NO_CHANGE");
            base["observation"] = obs;
            return Ok((base.clone(), 0));
        }
        base["invocation_attempted"] = json!(true);
        let command = backend.change(action)?;
        let after = state(&backend.observe()?)?;
        base["action"] = json!(action);
        base["before"] = before;
        base["after"] = after.clone();
        base["observation"] = observation(&after);
        if command.as_object().is_none_or(|o| {
            o.len() != 2 || !o.contains_key("returncode") || !o.contains_key("timed_out")
        }) || !command["timed_out"].is_boolean()
            || (!command["returncode"].is_null() && command["returncode"].as_i64().is_none())
        {
            return Ok(reason(&base, "UNKNOWN", "COMMAND_UNCONFIRMED", 3));
        }
        base["command"] = command.clone();
        if command["returncode"] != 0 || command["timed_out"] == true {
            return Ok(reason(&base, "UNKNOWN", "COMMAND_UNCONFIRMED", 3));
        }
        if after["active"] != desired
            || after["sub"]
                != if desired == "active" {
                    "running"
                } else {
                    "dead"
                }
        {
            return Ok(reason(&base, "UNKNOWN", "POSTSTATE_UNCONFIRMED", 3));
        }
        base["status"] = json!("APPLIED");
        Ok((base.clone(), 0))
    })();
    match work {
        Ok(v) => v,
        Err(e) => observation_failure(&base, &e.to_string()),
    }
}
fn observation_failure(base: &Value, error: &str) -> (Value, i32) {
    let allowed = [
        "HOST_BOUNDARY_REQUIRED",
        "UNSAFE_INSTALL_DIRECTORY",
        "UNSAFE_INSTALL_FILE",
        "UNSAFE_LOCK",
        "TARGET_IDENTITY_UNVERIFIED",
        "SYSTEMD_READ_FAILED",
        "SYSTEMD_OUTPUT_INVALID",
        "UNSUPPORTED_ACTION",
        "INVALID_OBSERVATION",
    ];
    reason(
        base,
        "UNKNOWN",
        if allowed.contains(&error) {
            error
        } else {
            "OBSERVATION_OR_EXECUTION_FAILED"
        },
        3,
    )
}
fn python_timestamp(time: chrono::DateTime<chrono::Utc>) -> String {
    let precision = if time.timestamp_subsec_micros() == 0 {
        chrono::SecondsFormat::Secs
    } else {
        chrono::SecondsFormat::Micros
    };
    time.to_rfc3339_opts(precision, false)
}
pub fn live(r: &mut dyn Runner, args: &[String]) -> (Value, i32) {
    let time = python_timestamp(chrono::Utc::now());
    let execution = util::id().replace('-', "");
    let valid = (args.len() == 1 && ["capabilities", "status"].contains(&args[0].as_str()))
        || (args.len() == 2
            && ["start", "stop"].contains(&args[0].as_str())
            && util::digest(&args[1]));
    if !valid || args.first().is_some_and(|s| s == "capabilities") {
        return dispatch(args, &mut SystemdBackend { runner: r }, &time, &execution);
    }
    let base = json!({"schema":"hermes.host-canary.v1","target":TARGET,"observed_at_utc":time,"execution_id":execution,"invocation_attempted":false});
    let lock = match host_lock() {
        Ok(l) => l,
        Err(e) => {
            let s = e.to_string();
            return reason(
                &base,
                if s == "BUSY" { "REJECTED" } else { "UNKNOWN" },
                if [
                    "HOST_BOUNDARY_REQUIRED",
                    "UNSAFE_INSTALL_DIRECTORY",
                    "UNSAFE_LOCK",
                    "BUSY",
                ]
                .contains(&s.as_str())
                {
                    &s
                } else {
                    "OBSERVATION_OR_EXECUTION_FAILED"
                },
                if s == "BUSY" { 2 } else { 3 },
            );
        }
    };
    let setup = (|| -> Result<()> {
        root_host()?;
        owned_file(Path::new("/usr/bin/systemctl"), 4 * 1024 * 1024)?;
        Ok(())
    })();
    let result = match setup {
        Ok(()) => dispatch(args, &mut SystemdBackend { runner: r }, &time, &execution),
        Err(e) => observation_failure(&base, &e.to_string()),
    };
    drop(lock);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_setup_preserves_python_error_reasons() {
        for expected in [
            "HOST_BOUNDARY_REQUIRED",
            "UNSAFE_INSTALL_DIRECTORY",
            "UNSAFE_INSTALL_FILE",
        ] {
            let (value, code) =
                observation_failure(&json!({"invocation_attempted":false}), expected);
            assert_eq!(value["reason"], expected);
            assert_eq!(value["status"], "UNKNOWN");
            assert_eq!(value["invocation_attempted"], false);
            assert_eq!(code, 3);
        }
        assert_eq!(
            observation_failure(&json!({}), "private OS detail").0["reason"],
            "OBSERVATION_OR_EXECUTION_FAILED"
        );
    }
    #[test]
    fn live_timestamp_matches_python_isoformat() {
        for (input, expected) in [
            ("2026-11-04T01:02:03Z", "2026-11-04T01:02:03+00:00"),
            (
                "2026-11-04T01:02:03.123456789Z",
                "2026-11-04T01:02:03.123456+00:00",
            ),
        ] {
            assert_eq!(python_timestamp(input.parse().unwrap()), expected);
        }
    }
}
