//! Caller-pane resolver. Focus and inherited pane variables are never caller evidence.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

type Pids = BTreeSet<i64>;
pub const HELP: &str = "Usage: pane [--exports | --focused | --resume-candidate]\n  default    Print the caller pane; warn for an unverified session match.\n  --exports  Print shell exports for the containing pane.\n  --focused  Print the UI-focused pane (not caller identity).\n  --resume-candidate  Print a unique Codex launch-time resume match (unverified current session).\n";

#[derive(Debug, PartialEq, Eq)]
pub struct Failure(pub &'static str, pub String);
impl Failure {
    pub fn stderr(&self) -> String {
        format!("{}@pane:1: {}\n", self.0, self.1)
    }
}
fn failure(code: &'static str, message: impl Into<String>) -> Failure {
    Failure(code, message.into())
}
/// I/O boundary permits fake proc trees and literal Herdr JSON without native calls.
pub trait System {
    fn query(&mut self, arguments: &[&str]) -> Result<Value, Failure>;
    fn proc_stat(&self, pid: i64) -> Option<String>;
    fn pid(&self) -> i64;
    fn session_id(&self) -> String;
    fn origin(&self, session: &str) -> Option<(i64, String)>;
    fn boot_and_ticks(&self) -> Option<(i64, u64)>;
    fn caller_ttys(&self) -> BTreeSet<u64>;
    fn shell_tty(&self, pid: i64) -> Option<u64>;
    fn realpath(&self, path: &str) -> PathBuf;
}
fn processes(info: &Value) -> &[Value] {
    info["foreground_processes"]
        .as_array()
        .map_or(&[], Vec::as_slice)
}
fn roots(info: &Value) -> Pids {
    std::iter::once(info["shell_pid"].as_i64())
        .chain(processes(info).iter().map(|p| p["pid"].as_i64()))
        .flatten()
        .collect()
}
fn fields(stat: &str) -> Option<Vec<&str>> {
    Some(
        stat.get(stat.rfind(')')? + 2..)?
            .split_whitespace()
            .collect(),
    )
}
fn valid_session(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn reported(p: &Value, session: &str) -> bool {
    !session.is_empty()
        && p["agent"] == "codex"
        && p["agent_session"]["source"] == "herdr:codex"
        && p["agent_session"]["kind"] == "id"
        && p["agent_session"]["value"] == session
}
fn has_session(p: &Value) -> bool {
    match &p["agent_session"] {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Object(o) => !o.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
    }
}
fn is_codex(value: &Value) -> bool {
    // Match os.path.basename literally; Path::file_name normalizes trailing / and /.
    value.as_str().and_then(|s| s.rsplit('/').next()) == Some("codex")
}

fn resumed(info: &Value, session: &str) -> Pids {
    if session.is_empty() {
        return Pids::new();
    }
    processes(info)
        .iter()
        .filter_map(|p| {
            let argv = p["argv"].as_array()?;
            (argv.len() >= 3 && is_codex(&argv[0]) && argv[1] == "resume" && argv[2] == session)
                .then(|| p["pid"].as_i64())
                .flatten()
        })
        .collect()
}
fn fresh<S: System>(system: &S, info: &Value, origin: &Option<(i64, String)>) -> Pids {
    let Some((created, cwd)) = origin else {
        return Pids::new();
    };
    let Some((boot, ticks)) = system.boot_and_ticks().filter(|(_, t)| *t > 0) else {
        return Pids::new();
    };
    processes(info)
        .iter()
        .filter_map(|p| {
            let argv = p["argv"].as_array()?;
            let pid = p["pid"].as_i64()?;
            if argv.len() != 1
                || !is_codex(&argv[0])
                || system.realpath(p["cwd"].as_str()?) != system.realpath(cwd)
            {
                return None;
            }
            let stat = system.proc_stat(pid)?;
            let start = fields(&stat)?.get(19)?.parse::<u64>().ok()?;
            // Match Python's float division and int truncation at millisecond resolution.
            let started = ((boot as f64 + start as f64 / ticks as f64) * 1000.0) as i64;
            created
                .checked_sub(started)
                .filter(|d| (0..=15000).contains(d))
                .map(|_| pid)
        })
        .collect()
}
fn terminal<S: System>(system: &S, info: &Value, ttys: &BTreeSet<u64>) -> bool {
    info["shell_pid"]
        .as_i64()
        .filter(|p| *p > 1)
        .and_then(|p| system.shell_tty(p))
        .is_some_and(|d| ttys.contains(&d))
}
fn panes<S: System>(system: &mut S) -> Result<Vec<Value>, Failure> {
    Ok(system.query(&["pane", "list"])?["panes"]
        .as_array()
        .cloned()
        .unwrap_or_default())
}
fn info<S: System>(system: &mut S, p: &Value) -> Result<Value, Failure> {
    let id = p["pane_id"]
        .as_str()
        .ok_or_else(|| failure("pane_metadata_invalid", "pane_id"))?;
    Ok(system.query(&["pane", "process-info", "--pane", id])?["process_info"].clone())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ancestry,
    Terminal,
    Reported,
    Resume,
    Fresh,
    Focused,
    Unresolved,
}
/// Returns exact stdout/stderr. Only the system boundary can execute Herdr.
pub fn resolve<S: System>(
    system: &mut S,
    args: &[String],
    herdr_env: bool,
) -> Result<(String, String), Failure> {
    if args == ["-h"] || args == ["--help"] {
        return Ok((HELP.into(), String::new()));
    }
    if !(args.is_empty()
        || args == ["--exports"]
        || args == ["--focused"]
        || args == ["--resume-candidate"])
    {
        return Err(failure("invalid_arguments", "use pane --help"));
    }
    if !herdr_env {
        return Err(failure(
            "herdr_environment_missing",
            "run inside a Herdr-managed environment",
        ));
    }
    let initial = panes(system)?;
    let focused = args == ["--focused"];
    let mut ancestors = Pids::new();
    let mut session = system.session_id();
    if !valid_session(&session) {
        session.clear();
    }
    let mut ttys = BTreeSet::new();
    let mut origin = None;
    let mut first_pids = Pids::new();
    let (candidates, mode) = if focused {
        (
            initial
                .iter()
                .filter(|p| p["focused"] == true)
                .cloned()
                .collect::<Vec<_>>(),
            Mode::Focused,
        )
    } else {
        let mut pid = system.pid();
        while pid > 1 && ancestors.insert(pid) {
            let Some(raw) = system.proc_stat(pid) else {
                break;
            };
            let Some(parent) = fields(&raw).and_then(|f| f.get(1).and_then(|s| s.parse().ok()))
            else {
                break;
            };
            pid = parent;
        }
        if !session.is_empty() {
            origin = system.origin(&session);
        }
        ttys = system.caller_ttys();
        let (mut a, mut t, mut r, mut s, mut n) = (vec![], vec![], vec![], vec![], vec![]);
        for p in &initial {
            let current = info(system, p)?;
            if !ancestors.is_disjoint(&roots(&current)) {
                a.push(p.clone());
            }
            if terminal(system, &current, &ttys) {
                t.push(p.clone());
            }
            if reported(p, &session) {
                r.push(p.clone());
            }
            let pids = resumed(&current, &session);
            if !pids.is_empty() && !has_session(p) {
                s.push((p.clone(), pids));
            }
            let pids = fresh(system, &current, &origin);
            if !pids.is_empty() && !has_session(p) {
                n.push((p.clone(), pids));
            }
        }
        if args == ["--resume-candidate"]
            || (a.is_empty() && t.is_empty() && r.is_empty() && args.is_empty() && !s.is_empty())
        {
            if s.len() == 1 {
                first_pids = s[0].1.clone();
            }
            (s.into_iter().map(|(p, _)| p).collect(), Mode::Resume)
        } else if !a.is_empty() {
            (a, Mode::Ancestry)
        } else if !t.is_empty() {
            (t, Mode::Terminal)
        } else if !r.is_empty() {
            (r, Mode::Reported)
        } else if args.is_empty() && !n.is_empty() {
            if n.len() == 1 {
                first_pids = n[0].1.clone();
            }
            (n.into_iter().map(|(p, _)| p).collect(), Mode::Fresh)
        } else {
            (vec![], Mode::Unresolved)
        }
    };
    if candidates.len() != 1 {
        return Err(if focused {
            failure(
                "focused_pane_unresolved",
                format!("matches={}", candidates.len()),
            )
        } else {
            failure(
                "caller_pane_unresolved",
                format!("matches={}; no verified caller binding", candidates.len()),
            )
        });
    }
    let pane = &candidates[0];
    if !focused {
        let live = panes(system)?;
        let same: Vec<_> = live
            .iter()
            .filter(|p| p["pane_id"] == pane["pane_id"])
            .collect();
        if same.len() != 1
            || ["workspace_id", "tab_id"]
                .iter()
                .any(|k| same[0][k] != pane[k])
        {
            return Err(failure(
                "pane_changed_during_lookup",
                "pane moved or closed",
            ));
        }
        let current = info(system, pane)?;
        let mut current_matches = vec![];
        if matches!(mode, Mode::Resume | Mode::Fresh | Mode::Terminal) {
            for item in &live {
                let other = if item["pane_id"] == pane["pane_id"] {
                    current.clone()
                } else {
                    info(system, item)?
                };
                if match mode {
                    Mode::Resume => !resumed(&other, &session).is_empty() && !has_session(item),
                    Mode::Fresh => !fresh(system, &other, &origin).is_empty() && !has_session(item),
                    _ => terminal(system, &other, &ttys),
                } {
                    current_matches.push(item["pane_id"].clone());
                }
            }
        }
        let unique = current_matches == [pane["pane_id"].clone()];
        let changed = match mode {
            Mode::Ancestry => ancestors.is_disjoint(&roots(&current)),
            Mode::Resume => !unique || first_pids.is_disjoint(&resumed(&current, &session)),
            Mode::Fresh => !unique || first_pids.is_disjoint(&fresh(system, &current, &origin)),
            Mode::Reported => {
                live.iter()
                    .filter(|p| reported(p, &session))
                    .map(|p| p["pane_id"].clone())
                    .collect::<Vec<_>>()
                    != [pane["pane_id"].clone()]
            }
            Mode::Terminal => !unique,
            _ => false,
        };
        if changed {
            return Err(failure(
                "pane_changed_during_lookup",
                "process association changed",
            ));
        }
    }
    let id = pane["pane_id"]
        .as_str()
        .ok_or_else(|| failure("pane_metadata_invalid", "pane_id"))?;
    if args == ["--exports"] {
        let workspace = pane["workspace_id"].as_str().unwrap_or("");
        let suffix = |s: &str, prefix: &str| {
            s.strip_prefix(prefix)
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric()))
        };
        if !suffix(workspace, "w") {
            return Err(failure("pane_metadata_invalid", "workspace_id"));
        }
        if !suffix(id, &format!("{workspace}:p")) {
            return Err(failure("pane_metadata_invalid", "pane_id"));
        }
        let tab = pane["tab_id"].as_str().unwrap_or("");
        if !suffix(tab, &format!("{workspace}:t")) {
            return Err(failure("pane_metadata_invalid", "tab_id"));
        }
        Ok((
            format!(
                "export HERDR_PANE_ID={id}\nexport HERDR_WORKSPACE_ID={workspace}\nexport HERDR_TAB_ID={tab}\n"
            ),
            String::new(),
        ))
    } else {
        let warning = if matches!(mode, Mode::Resume | Mode::Fresh) {
            "unverified_session_candidate@pane:1: frontend may have changed sessions\n"
        } else {
            ""
        };
        Ok((format!("{id}\n"), warning.into()))
    }
}

pub struct Host;
impl System for Host {
    fn query(&mut self, arguments: &[&str]) -> Result<Value, Failure> {
        let query_error = || {
            failure(
                "herdr_query_failed",
                "could not query the current Herdr session",
            )
        };
        let parse_error = || {
            failure(
                "herdr_query_failed",
                "could not parse the current Herdr session",
            )
        };
        let mut child = Command::new("herdr")
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| parse_error())?;
        let mut stdout = child.stdout.take().ok_or_else(parse_error)?;
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = vec![];
            let result = stdout.read_to_end(&mut bytes).map(|_| bytes);
            let _ = send.send(result);
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(parse_error());
                }
            }
        };
        if !status.success() {
            return Err(query_error());
        }
        let bytes = recv
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| parse_error())?
            .map_err(|_| parse_error())?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| parse_error())?;
        value.get("result").cloned().ok_or_else(parse_error)
    }
    fn proc_stat(&self, pid: i64) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()
    }
    fn pid(&self) -> i64 {
        std::process::id().into()
    }
    fn session_id(&self) -> String {
        std::env::var("CODEX_THREAD_ID").unwrap_or_default()
    }
    fn origin(&self, session: &str) -> Option<(i64, String)> {
        let home = std::env::var_os("CODEX_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".codex")))?;
        let db = rusqlite::Connection::open_with_flags(
            home.join("state_5.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .ok()?;
        db.busy_timeout(Duration::from_millis(200)).ok()?;
        db.query_row(
            "SELECT created_at_ms,cwd FROM threads WHERE id=?",
            [session],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()
    }
    fn boot_and_ticks(&self) -> Option<(i64, u64)> {
        let raw = std::fs::read_to_string("/proc/stat").ok()?;
        let boot = raw
            .lines()
            .find_map(|s| s.strip_prefix("btime "))?
            .trim()
            .parse()
            .ok()?;
        Some((boot, rustix::param::clock_ticks_per_second()))
    }
    fn caller_ttys(&self) -> BTreeSet<u64> {
        [
            (1, std::io::stdout().is_terminal()),
            (2, std::io::stderr().is_terminal()),
        ]
        .into_iter()
        .filter(|(_, is_tty)| *is_tty)
        .filter_map(|(fd, _)| std::fs::metadata(format!("/proc/self/fd/{fd}")).ok())
        .filter(|m| m.file_type().is_char_device())
        .map(|m| m.rdev())
        .collect()
    }
    fn shell_tty(&self, pid: i64) -> Option<u64> {
        let m = std::fs::metadata(format!("/proc/{pid}/fd/0")).ok()?;
        m.file_type().is_char_device().then(|| m.rdev())
    }
    fn realpath(&self, path: &str) -> PathBuf {
        // realpath(strict=False): resolve existing prefixes, then normalize missing suffixes.
        let input = Path::new(path);
        let mut result = if input.is_absolute() {
            PathBuf::from("/")
        } else {
            std::env::current_dir().unwrap_or_default()
        };
        for component in input.components() {
            match component {
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::Normal(part) => {
                    result.push(part);
                    if let Ok(real) = std::fs::canonicalize(&result) {
                        result = real;
                    }
                }
                _ => {}
            }
        }
        result
    }
}
pub fn cli(args: &[OsString]) -> ExitCode {
    let args: Option<Vec<_>> = args.iter().map(|s| s.to_str().map(String::from)).collect();
    let result = args
        .ok_or_else(|| failure("invalid_arguments", "use pane --help"))
        .and_then(|a| {
            resolve(
                &mut Host,
                &a,
                std::env::var("HERDR_ENV").as_deref() == Ok("1"),
            )
        });
    let (out, err, status) = match result {
        Ok((out, err)) => (out, err, 0),
        Err(e) => (String::new(), e.stderr(), 1),
    };
    if std::io::stdout().lock().write_all(out.as_bytes()).is_err()
        || std::io::stderr().lock().write_all(err.as_bytes()).is_err()
    {
        return ExitCode::from(1);
    }
    ExitCode::from(status)
}
