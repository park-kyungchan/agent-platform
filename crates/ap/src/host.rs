//! The host the verbs run on: the port every subprocess and clock crosses (`System`), its real form (`Host`),
//! the failure and answer types, the argument, digest, id and store-engine helpers, and the JSON command line.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Duration, Instant},
};

pub(crate) const LIMIT: u64 = 1_048_576;
pub(crate) const ROOT: &str = "/home/palantirkc/work/";
#[derive(Debug)]
pub struct Failure {
    pub code: &'static str,
    pub field: String,
    pub detail: Value,
}
pub(crate) fn fail(code: &'static str, field: &str, detail: Value) -> Failure {
    Failure {
        code,
        field: field.into(),
        detail,
    }
}
pub(crate) fn error(code: &'static str, field: &str) -> Failure {
    fail(code, field, Value::Null)
}
pub struct Reply {
    pub success: bool,
    pub bytes: Vec<u8>,
    pub stdout: Vec<u8>,
}
pub type Answer = Result<Reply, Failure>;
/// All subprocesses cross this boundary; tests supply literal replies and record calls.
pub trait System {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer;
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if !std::fs::metadata(path)?.is_file() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        std::fs::read(path)
    }
    /// The work root (`/home/palantirkc/work/`); a test replaces it by a temporary directory.
    fn work_root(&self) -> PathBuf {
        PathBuf::from(ROOT)
    }
    /// The home directory of the passwd entry of the user this process runs as (its effective
    /// user id), not `HOME`; None when no entry of `/etc/passwd` names that id. A test replaces it.
    fn home(&self) -> Option<PathBuf> {
        passwd_home(
            &std::fs::read_to_string("/proc/self/status").ok()?,
            &std::fs::read_to_string("/etc/passwd").ok()?,
        )
    }
    fn cwd(&self) -> std::io::Result<PathBuf> {
        std::env::current_dir()
    }
    /// The resolved path of an existing directory.
    fn directory(&self, path: &str) -> std::io::Result<PathBuf> {
        let resolved = std::fs::canonicalize(path)?;
        if !resolved.is_dir() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        Ok(resolved)
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    /// The wall-clock time, for a stamp (`now` is monotonic and has no date).
    fn wall(&self) -> std::time::SystemTime {
        std::time::SystemTime::now()
    }
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
    /// Starts one long program without a pipe: `argv[0]` with the rest of `argv`, exactly the
    /// environment `env`, in `cwd`, with `files` as stdin, stdout and stderr (files the caller
    /// opened). `ap run` passes the whole `systemd-run` command line here; a test replaces the
    /// unit launcher by overriding this method.
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<std::process::Child> {
        let (program, rest) = argv.split_first().ok_or(std::io::ErrorKind::InvalidInput)?;
        let [stdin, stdout, stderr] = files;
        Command::new(program)
            .args(rest)
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(cwd)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
    }
    /// Whether systemd is this host's init (sd_booted: `/run/systemd/system` is a directory).
    fn units(&self) -> bool {
        Path::new("/run/systemd/system").is_dir()
    }
    /// The host's policy file (`specs/ap/policy.md`): none unless the real host names one, so a
    /// test fake that reads the real file system is never subject to the host's file.
    fn policy_file(&self) -> Option<PathBuf> {
        None
    }
    /// This process's environment; `ap run` keeps only a fixed short list of it.
    fn environment(&self) -> Vec<(String, String)> {
        std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .collect()
    }
}
/// The home of the passwd entry whose user id is the effective one of a `/proc/<pid>/status` text
/// (its line `Uid:`, the second number): the sixth field of the first such line of the passwd text.
pub fn passwd_home(status: &str, passwd: &str) -> Option<PathBuf> {
    let uid = status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?;
    passwd.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.len() == 7 && f[2] == uid && f[5].starts_with('/')).then(|| PathBuf::from(f[5]))
    })
}
pub(crate) fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).into()).collect()
}
pub(crate) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Failure> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error("invalid_input", key))
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn id(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
pub(crate) fn parse(reply: Reply, code: &'static str) -> Result<Value, Failure> {
    serde_json::from_slice(&reply.bytes).map_err(|_| error(code, "answer"))
}
pub(crate) fn codes(v: &Value) -> Value {
    let mut found: Vec<String> = v["result"]["criteria"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["passed"] == false)
        .filter_map(|c| c["code"].as_str().map(str::to_owned))
        .collect();
    found.extend(
        v["failed"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    if let Some(code) = v["code"].as_str() {
        found.push(code.to_owned());
    }
    json!({"codes":found})
}
pub(crate) fn ont(
    s: &mut impl System,
    r: &Value,
    command: &[&str],
    body: &Value,
) -> Result<Reply, Failure> {
    ont_within(s, r, command, body, 30_000)
}
/// One call of `ont` bound to `ms` milliseconds.
pub(crate) fn ont_within(
    s: &mut impl System,
    r: &Value,
    command: &[&str],
    body: &Value,
    ms: u64,
) -> Result<Reply, Failure> {
    let mut a = args(command);
    if let Some(store) = r["store"].as_str() {
        a.extend(args(&["--store", store]));
    }
    s.call("ont", &a, body, ms)
}
macro_rules! require {
    ($condition:expr => $code:expr, $field:expr) => {
        if !$condition {
            return Err(fail($code, $field, Value::Null));
        }
    };
}
pub(crate) use require;
pub(crate) fn apply_answer(reply: Reply, code: &'static str) -> Result<Value, Failure> {
    let success = reply.success;
    let v = parse(reply, code)?;
    require!(success || refused(&v) => code, "apply");
    Ok(v)
}
/// A fresh UUID from the kernel; Claude Code takes it as `--session-id` and Herdr reports it back.
pub(crate) fn new_session_id(s: &impl System) -> Result<String, Failure> {
    let bytes = s
        .read("/proc/sys/kernel/random/uuid")
        .map_err(|_| error("io_error", "session_id"))?;
    let id = String::from_utf8_lossy(&bytes).trim().to_owned();
    let shape = id.len() == 36
        && id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        });
    require!(shape => "io_error", "session_id");
    Ok(id)
}
pub(crate) fn complete_id(v: &Value) -> bool {
    [
        "/object_type",
        "/native_key",
        "/namespace/instance",
        "/namespace/account",
    ]
    .iter()
    .all(|p| {
        v.pointer(p)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    })
}
pub(crate) fn refused(v: &Value) -> bool {
    v.is_object() && (v["status"].as_str().is_some_and(|s| s != "ok") || v["code"].is_string())
}
pub struct Host;
/// The policy file's path: the value of `AP_POLICY_FILE` when set and not empty, else the default.
fn policy_path(value: Option<OsString>) -> PathBuf {
    value.filter(|v| !v.is_empty()).map_or_else(
        || PathBuf::from("/etc/agent-platform/policy.json"),
        PathBuf::from,
    )
}
impl System for Host {
    fn policy_file(&self) -> Option<PathBuf> {
        Some(policy_path(std::env::var_os("AP_POLICY_FILE")))
    }
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        let error = || fail("process_failed", program, Value::Null);
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| error())?;
        let mut stdin = child.stdin.take().ok_or_else(error)?;
        let body = if input.is_null() {
            vec![]
        } else {
            serde_json::to_vec(input).map_err(|_| error())?
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(stdin.write_all(&body));
        });
        let (out_tx, out_rx) = std::sync::mpsc::channel();
        let stdout = child.stdout.take().ok_or_else(error)?;
        let stderr = child.stderr.take().ok_or_else(error)?;
        for (which, pipe) in [
            (true, Box::new(stdout) as Box<dyn Read + Send>),
            (false, Box::new(stderr) as Box<dyn Read + Send>),
        ] {
            let tx = out_tx.clone();
            std::thread::spawn(move || {
                let mut bytes = vec![];
                let result = pipe
                    .take(LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .map(|_| (which, bytes));
                let _ = tx.send(result);
            });
        }
        let deadline = Instant::now() + Duration::from_millis(ms);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    child.kill().map_err(|_| error())?;
                    child.wait().map_err(|_| error())?;
                    return Err(error());
                }
            }
        };
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| error())?
            .map_err(|_| error())?;
        let mut out = vec![];
        let mut err = vec![];
        for _ in 0..2 {
            let (which, bytes) = out_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| error())?
                .map_err(|_| error())?;
            if bytes.len() as u64 > LIMIT {
                return Err(error());
            }
            if which {
                out = bytes;
            } else {
                err = bytes;
            }
        }
        Ok(Reply {
            success: status.success(),
            bytes: if out.is_empty() { err } else { out.clone() },
            stdout: out,
        })
    }
}
pub(crate) fn json_cli(
    args: &[OsString],
    execute: impl FnOnce(&mut Host, &str, &Value) -> Result<(Value, u8), Failure>,
) -> ExitCode {
    let result = (|| {
        if args.len() != 1 {
            return Err(fail("usage", "args", Value::Null));
        }
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail("io_error", "stdin", Value::Null))?;
        if bytes.len() as u64 > LIMIT {
            return Err(fail("input_too_large", "stdin", Value::Null));
        }
        let r = serde_json::from_slice(&bytes)
            .map_err(|_| fail("invalid_input", "stdin", Value::Null))?;
        execute(&mut Host, args[0].to_str().unwrap_or(""), &r)
    })();
    let (value, status, error) = match result {
        Ok((v, status)) => (v, status, false),
        Err(e) => (
            json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}}),
            3,
            true,
        ),
    };
    let mut stream: Box<dyn Write> = if error {
        Box::new(std::io::stderr())
    } else {
        Box::new(std::io::stdout())
    };
    if writeln!(stream, "{value}").is_err() {
        return ExitCode::from(4);
    }
    ExitCode::from(status)
}

#[cfg(test)]
mod tests {
    use super::policy_path;
    use std::path::PathBuf;
    // spec: specs/ap/policy.md#R1
    #[test]
    fn the_policy_path_is_the_variable_when_set_and_not_empty() {
        let default = PathBuf::from("/etc/agent-platform/policy.json");
        assert_eq!(policy_path(None), default);
        assert_eq!(policy_path(Some("".into())), default);
        assert_eq!(
            policy_path(Some("/tmp/p.json".into())),
            PathBuf::from("/tmp/p.json")
        );
    }
}
