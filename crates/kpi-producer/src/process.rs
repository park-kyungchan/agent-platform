//! Native bounded commands; raw stdout/stderr never appear in diagnostics.
use crate::{Error, Result};
use std::io::Read;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Call {
    pub program: String,
    pub args: Vec<String>,
}
impl Call {
    pub fn new(program: &str, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub exit_code: Option<u32>,
    pub signal: Option<u32>,
}
impl Output {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && self.signal.is_none()
    }
}
pub trait Runner {
    fn run(&mut self, call: &Call) -> Result<Output>;
}
pub struct NativeRunner {
    pub per_process: Duration,
    pub deadline: Instant,
    pub output_limit: usize,
}
fn drain(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0; 8192];
        loop {
            let n = pipe.read(&mut buf).map_err(|_| Error("process_pipe"))?;
            if n == 0 {
                return Ok(bytes);
            }
            if bytes.len().saturating_add(n) > limit {
                overflow.store(true, Ordering::SeqCst);
                return Err(Error("process_output_limit"));
            }
            bytes.extend_from_slice(&buf[..n]);
        }
    })
}
impl Runner for NativeRunner {
    fn run(&mut self, call: &Call) -> Result<Output> {
        if Instant::now() >= self.deadline {
            return Err(Error("capture_timeout"));
        }
        if !matches!(call.program.as_str(), "git" | "gh") {
            return Err(Error("invalid_program"));
        }
        let mut child = Command::new(&call.program)
            .args(&call.args)
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env_remove("GIT_TRACE")
            .env_remove("GIT_TRACE_CURL")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|_| Error("process_spawn"))?;
        let pid = rustix::process::Pid::from_child(&child);
        let overflow = Arc::new(AtomicBool::new(false));
        let out = drain(
            child.stdout.take().ok_or(Error("process_pipe"))?,
            self.output_limit,
            overflow.clone(),
        );
        let err = drain(
            child.stderr.take().ok_or(Error("process_pipe"))?,
            self.output_limit,
            overflow.clone(),
        );
        let deadline = self.deadline.min(Instant::now() + self.per_process);
        let status = loop {
            if overflow.load(Ordering::SeqCst) || Instant::now() >= deadline {
                break Err(Error(if overflow.load(Ordering::SeqCst) {
                    "process_output_limit"
                } else {
                    "process_timeout"
                }));
            }
            match child.try_wait() {
                Ok(Some(s)) => break Ok(s),
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => break Err(Error("process_wait")),
            }
        };
        // Kill this command's private process group even after leader exit: a descendant
        // must not keep an inherited pipe open forever. No foreign group is selected.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        let _ = child.wait();
        let stdout = out.join().map_err(|_| Error("process_pipe"))?;
        let stderr = err.join().map_err(|_| Error("process_pipe"))?;
        let s = status?;
        let stdout = stdout?;
        let _ = stderr?;
        Ok(Output {
            stdout,
            exit_code: s.code().and_then(|n| u32::try_from(n).ok()),
            signal: s.signal().and_then(|n| u32::try_from(n).ok()),
        })
    }
}
pub fn checked(runner: &mut impl Runner, call: &Call) -> Result<Vec<u8>> {
    let o = runner.run(call)?;
    if !o.success() {
        return Err(Error("command_exit_nonzero"));
    }
    Ok(o.stdout)
}
