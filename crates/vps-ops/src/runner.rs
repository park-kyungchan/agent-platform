//! All external processes cross this injectable boundary. Tests use fixture runners.
use crate::{Result, util};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    pub timeout_ms: u64,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub termination_grace_ms: u64,
    pub reap_timeout_ms: u64,
    pub environment: BTreeMap<String, String>,
    pub clear_environment: bool,
    pub cwd: Option<String>,
}
impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            stdin: vec![],
            timeout_ms: 30_000,
            max_stdout_bytes: 8 * 1024 * 1024,
            max_stderr_bytes: 65536,
            termination_grace_ms: 1000,
            reap_timeout_ms: 1000,
            environment: BTreeMap::new(),
            clear_environment: false,
            cwd: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub reason: String,
    pub created: bool,
    pub reaped: bool,
    pub term_sent: bool,
    pub kill_sent: bool,
    pub signal_failed: bool,
    pub stdout_bytes_observed: usize,
    pub stderr_bytes_observed: usize,
    pub stdout_complete: bool,
    pub stderr_complete: bool,
    pub transport_exit_code: Option<i32>,
    pub transport_signal: Option<String>,
}
pub trait Runner {
    fn command_available(&mut self, _program: &str) -> Result<bool> {
        Err("Command availability probe unavailable in this runner".into())
    }
    fn start_native(&mut self, _spec: CommandSpec) -> Result<Box<dyn NativeJob>> {
        Err("Native launch unavailable in this runner".into())
    }
    fn run(&mut self, spec: CommandSpec) -> Result<CommandOutput>;
    fn pause(&mut self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
}
#[derive(Default)]
pub struct ProcessRunner;
impl Runner for ProcessRunner {
    fn command_available(&mut self, program: &str) -> Result<bool> {
        use std::os::unix::fs::PermissionsExt;
        Ok(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|directory| {
                std::fs::metadata(directory.join(program))
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            }),
        )
    }
    fn start_native(&mut self, spec: CommandSpec) -> Result<Box<dyn NativeJob>> {
        start_native_process(spec)
    }
    fn run(&mut self, spec: CommandSpec) -> Result<CommandOutput> {
        let mut out = CommandOutput {
            exit_code: 127,
            stdout: String::new(),
            stderr: String::new(),
            reason: "spawn-failed".into(),
            created: false,
            reaped: false,
            term_sent: false,
            kill_sent: false,
            signal_failed: false,
            stdout_bytes_observed: 0,
            stderr_bytes_observed: 0,
            stdout_complete: false,
            stderr_complete: false,
            transport_exit_code: None,
            transport_signal: None,
        };
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if spec.clear_environment {
            command.env_clear();
        }
        command.envs(&spec.environment);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(_) => return Ok(out),
        };
        out.created = true;
        let mut input = child.stdin.take();
        let mut stdout = child.stdout.take().ok_or("Missing stdout pipe")?;
        let mut stderr = child.stderr.take().ok_or("Missing stderr pipe")?;
        fn nonblocking(fd: impl std::os::fd::AsFd) -> Result<()> {
            let flags = rustix::fs::fcntl_getfl(&fd)?;
            rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)?;
            Ok(())
        }
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        if let Some(f) = &input {
            nonblocking(f)?;
        }
        let started = Instant::now();
        let mut retained = [Vec::new(), Vec::new()];
        let mut sent = 0;
        let mut failure = None;
        loop {
            if !out.reaped {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        use std::os::unix::process::ExitStatusExt;
                        out.reaped = true;
                        out.transport_exit_code = status.code();
                        out.transport_signal = status.signal().map(|n| format!("SIG{n}"));
                        out.exit_code = status.code().unwrap_or(128 + status.signal().unwrap_or(0));
                    }
                    Ok(None) => {}
                    Err(_) => {
                        failure = Some("reap-failed");
                    }
                }
            }
            if failure.is_some() {
                break;
            }
            if let Some(f) = input.as_mut() {
                if sent == spec.stdin.len() {
                    input = None;
                } else {
                    match f.write(&spec.stdin[sent..spec.stdin.len().min(sent + 8192)]) {
                        Ok(n) => sent += n,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => input = None,
                    }
                }
            }
            for (index, buffer) in retained.iter_mut().enumerate() {
                let (reader, count, complete, maximum): (
                    &mut dyn Read,
                    &mut usize,
                    &mut bool,
                    usize,
                ) = if index == 0 {
                    (
                        &mut stdout,
                        &mut out.stdout_bytes_observed,
                        &mut out.stdout_complete,
                        spec.max_stdout_bytes,
                    )
                } else {
                    (
                        &mut stderr,
                        &mut out.stderr_bytes_observed,
                        &mut out.stderr_complete,
                        spec.max_stderr_bytes,
                    )
                };
                if *complete {
                    continue;
                }
                let mut b = [0u8; 8192];
                match reader.read(&mut b) {
                    Ok(0) => *complete = true,
                    Ok(n) => {
                        *count += n;
                        if *count > maximum {
                            failure = Some(if index == 0 {
                                "stdout-limit-exceeded"
                            } else {
                                "stderr-limit-exceeded"
                            });
                            break;
                        }
                        buffer.extend_from_slice(&b[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        failure = Some("stream-read-failed");
                        break;
                    }
                }
            }
            if failure.is_some() || out.reaped && out.stdout_complete && out.stderr_complete {
                break;
            }
            if started.elapsed().as_millis() >= u128::from(spec.timeout_ms) {
                failure = Some("deadline-exceeded");
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        if let Some(reason) = failure {
            out.reason = reason.into();
            out.exit_code = if reason == "deadline-exceeded" {
                124
            } else {
                125
            };
            if !out.reaped {
                out.term_sent = Command::new("/bin/kill")
                    .args(["-TERM", &child.id().to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                out.signal_failed = !out.term_sent;
                let until = Instant::now() + Duration::from_millis(spec.termination_grace_ms);
                while Instant::now() < until {
                    if let Ok(Some(status)) = child.try_wait() {
                        out.reaped = true;
                        out.transport_exit_code = status.code();
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                if !out.reaped {
                    out.kill_sent = child.kill().is_ok();
                    let until = Instant::now() + Duration::from_millis(spec.reap_timeout_ms);
                    while Instant::now() < until {
                        if let Ok(Some(status)) = child.try_wait() {
                            out.reaped = true;
                            out.transport_exit_code = status.code();
                            break;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        } else {
            out.reason = if out.exit_code == 0 {
                "completed"
            } else {
                "nonzero-exit"
            }
            .into();
            out.stdout = String::from_utf8_lossy(&retained[0]).into_owned();
            out.stderr = String::from_utf8_lossy(&retained[1]).into_owned();
        }
        // Drop only our nonblocking pipe handles; no descendant or remote process is signalled.
        drop(input);
        drop(stdout);
        drop(stderr);
        Ok(out)
    }
}
pub fn checked(r: &mut dyn Runner, program: &str, args: Vec<String>) -> Result<String> {
    let o = r.run(CommandSpec::new(program, args))?;
    if o.exit_code != 0 {
        return Err(format!(
            "Command failed ({}); stderr sha256 {}",
            o.exit_code,
            util::hash(o.stderr)
        )
        .into());
    }
    Ok(o.stdout.trim().into())
}

pub trait NativeJob {
    fn poll(&mut self) -> Result<Option<CommandOutput>>;
}
pub struct ThreadJob {
    receiver: mpsc::Receiver<Result<CommandOutput>>,
}
impl NativeJob for ThreadJob {
    fn poll(&mut self) -> Result<Option<CommandOutput>> {
        match self.receiver.try_recv() {
            Ok(o) => o.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(_) => Err("Native coordinator observation failed".into()),
        }
    }
}
pub fn start_native_process(spec: CommandSpec) -> Result<Box<dyn NativeJob>> {
    let mut c = Command::new(&spec.program);
    c.args(&spec.args)
        .envs(&spec.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn()?;
    let stdout = child.stdout.take().ok_or("Missing native stdout")?;
    let stderr = child.stderr.take().ok_or("Missing native stderr")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let capture = |mut reader: Box<dyn Read + Send>| -> Result<String> {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            let mut total = 0;
            let mut retained = Vec::new();
            let mut buf = [0; 8192];
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                total += n;
                let remaining = (1024 * 1024usize).saturating_sub(retained.len());
                retained.extend_from_slice(&buf[..n.min(remaining)])
            }
            Ok(serde_json::json!({"sha256":format!("{:x}",hasher.finalize()),"byteLength":total,"truncated":total>1024*1024,"sanitizedText":crate::adapters::desktop::sanitize_output(&String::from_utf8_lossy(&retained))}).to_string())
        };
        let a = thread::spawn(move || capture(Box::new(stdout)));
        let b = thread::spawn(move || capture(Box::new(stderr)));
        let result = (|| -> Result<CommandOutput> {
            let s = child.wait()?;
            let stdout = a.join().map_err(|_| "Native stdout observation failed")??;
            let stderr = b.join().map_err(|_| "Native stderr observation failed")??;
            Ok(CommandOutput {
                exit_code: s.code().unwrap_or(1),
                stdout,
                stderr,
                reason: "completed".into(),
                created: true,
                reaped: true,
                term_sent: false,
                kill_sent: false,
                signal_failed: false,
                stdout_bytes_observed: 0,
                stderr_bytes_observed: 0,
                stdout_complete: true,
                stderr_complete: true,
                transport_exit_code: s.code(),
                transport_signal: None,
            })
        })();
        let _ = tx.send(result);
    });
    Ok(Box::new(ThreadJob { receiver: rx }))
}
