//! Native bounded commands; raw stdout/stderr never appear in diagnostics.
use crate::{Error, Result};
use std::io::{ErrorKind, Read};
use std::os::fd::AsFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
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
// Keep each read nonblocking and check the same absolute deadline even while
// draining continuously writable pipes. No reader thread can outlive this call.
struct Pipe<R> {
    reader: R,
    bytes: Vec<u8>,
    eof: bool,
}
impl<R: Read + AsFd> Pipe<R> {
    fn new(reader: R) -> Result<Self> {
        let flags = rustix::fs::fcntl_getfl(&reader).map_err(|_| Error("process_pipe"))?;
        rustix::fs::fcntl_setfl(&reader, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(|_| Error("process_pipe"))?;
        Ok(Self {
            reader,
            bytes: Vec::new(),
            eof: false,
        })
    }
    fn drain(&mut self, limit: usize, deadline: Instant) -> Result<()> {
        let mut buf = [0; 8192];
        while !self.eof {
            if Instant::now() >= deadline {
                return Err(Error("process_timeout"));
            }
            match self.reader.read(&mut buf) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    if self.bytes.len().saturating_add(n) > limit {
                        return Err(Error("process_output_limit"));
                    }
                    self.bytes.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error("process_pipe")),
            }
        }
        Ok(())
    }
}
fn local_root(root: &str) -> bool {
    root.starts_with('/') && !root.contains('\0') && !root.split('/').any(|s| s == "." || s == "..")
}
fn approved_route(route: &str) -> bool {
    let parts = route.split('/').collect::<Vec<_>>();
    let ["repos", owner, repo, tail @ ..] = parts.as_slice() else {
        return false;
    };
    let name = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    if !name(owner) || !name(repo) {
        return false;
    }
    match tail {
        [] | ["git", "ref", "heads", "main"] => true,
        ["git", "commits", oid] => crate::hex(oid, 40),
        ["git", "trees", query] => query
            .strip_suffix("?recursive=1")
            .is_some_and(|oid| crate::hex(oid, 40)),
        _ => false,
    }
}
fn allowed(call: &Call) -> bool {
    let args = call.args.iter().map(String::as_str).collect::<Vec<_>>();
    match (call.program.as_str(), args.as_slice()) {
        ("git" | "gh", ["--version"]) => true,
        ("gh", ["api", "--method", "GET", route]) => approved_route(route),
        (
            "git",
            [
                "--no-replace-objects",
                "-C",
                root,
                "rev-parse",
                "--verify",
                object,
            ],
        ) => {
            local_root(root)
                && object
                    .strip_suffix("^{tree}")
                    .is_some_and(|oid| crate::hex(oid, 40))
        }
        (
            "git",
            [
                "--no-replace-objects",
                "-C",
                root,
                "ls-tree",
                "-r",
                "-z",
                "--full-tree",
                "--name-only",
                oid,
            ],
        ) => local_root(root) && crate::hex(oid, 40),
        _ => false,
    }
}
impl Runner for NativeRunner {
    fn run(&mut self, call: &Call) -> Result<Output> {
        if !matches!(call.program.as_str(), "git" | "gh") {
            return Err(Error("invalid_program"));
        }
        if !allowed(call) {
            return Err(Error("invalid_command"));
        }
        self.execute(Command::new(&call.program).args(&call.args))
    }
}
impl NativeRunner {
    // Private mechanics boundary lets module tests supply isolated executables;
    // production callers must pass the exact read-only allowlist above.
    fn execute(&self, command: &mut Command) -> Result<Output> {
        if Instant::now() >= self.deadline {
            return Err(Error("capture_timeout"));
        }
        let deadline = self.deadline.min(Instant::now() + self.per_process);
        let mut child = command
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
        let mut group_cleaned = false;
        let result = (|| {
            let mut out = Pipe::new(child.stdout.take().ok_or(Error("process_pipe"))?)?;
            let mut err = Pipe::new(child.stderr.take().ok_or(Error("process_pipe"))?)?;
            let mut status = None;
            loop {
                if Instant::now() >= deadline {
                    return Err(Error("process_timeout"));
                }
                out.drain(self.output_limit, deadline)?;
                err.drain(self.output_limit, deadline)?;
                if status.is_none() {
                    status = child.try_wait().map_err(|_| Error("process_wait"))?;
                    if status.is_some() {
                        // Descendants in this owned group must release inherited pipes.
                        let _ =
                            rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
                        group_cleaned = true;
                    }
                }
                if let Some(s) = status.filter(|_| out.eof && err.eof) {
                    if Instant::now() >= deadline {
                        return Err(Error("process_timeout"));
                    }
                    return Ok(Output {
                        stdout: out.bytes,
                        exit_code: s.code().and_then(|n| u32::try_from(n).ok()),
                        signal: s.signal().and_then(|n| u32::try_from(n).ok()),
                    });
                }
                std::thread::sleep(
                    Duration::from_millis(5)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        })();
        if !group_cleaned {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = child.wait();
        result
    }
}
pub fn checked(runner: &mut impl Runner, call: &Call) -> Result<Vec<u8>> {
    let o = runner.run(call)?;
    if !o.success() {
        return Err(Error("command_exit_nonzero"));
    }
    Ok(o.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(script: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "kpi-process-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&dir).unwrap();
            std::fs::write(dir.join("command.sh"), script).unwrap();
            Self(dir)
        }
        fn command(&self) -> Command {
            let mut c = Command::new("/bin/sh");
            c.arg(self.0.join("command.sh")).current_dir(&self.0);
            c
        }
        fn ready(&self) -> bool {
            self.0.join("ready").exists()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only the explicitly recorded synthetic detached descendant, never
            // a process discovered by name or a foreign group, is cleaned up.
            if let Ok(pid) = std::fs::read_to_string(self.0.join("detached-pid"))
                && let Some(pid) = pid
                    .trim()
                    .parse::<i32>()
                    .ok()
                    .and_then(rustix::process::Pid::from_raw)
            {
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
            }
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn runner(seconds: u64) -> NativeRunner {
        NativeRunner {
            per_process: Duration::from_secs(seconds),
            deadline: Instant::now() + Duration::from_secs(seconds),
            output_limit: 32768,
        }
    }
    #[test]
    fn detached_ready_descendant_cannot_extend_pipe_deadline() {
        // The leader waits for the setsid child to record readiness before exit.
        // Its inherited pipe remains open for 30s unless the reader is bounded.
        let f = Fixture::new(
            "setsid sh -c 'echo $$ > detached-pid; touch ready; exec sleep 30' &\nwhile ! test -f ready; do sleep 0.01; done\n",
        );
        let mut r = runner(10);
        r.deadline = Instant::now() + Duration::from_secs(5);
        let start = Instant::now();
        let result = r.execute(&mut f.command());
        assert!(
            f.ready(),
            "fixture startup did not complete; no deadline conclusion"
        );
        assert_eq!(result.err(), Some(Error("process_timeout")));
        assert!(start.elapsed() < Duration::from_secs(6));
    }
    #[test]
    fn ready_native_timeout_output_caps_and_group_cleanup_are_distinct() {
        let mut unrelated = Command::new("sleep").arg("60").spawn().unwrap();
        for (script, expected) in [
            ("touch ready; exec sleep 30", Some("process_timeout")),
            (
                "touch ready; while :; do printf synthetic; done",
                Some("process_output_limit"),
            ),
            (
                "touch ready; while :; do printf synthetic-private-marker >&2; done",
                Some("process_output_limit"),
            ),
            ("sleep 30 &\ntouch ready\n", None),
        ] {
            let f = Fixture::new(script);
            let r = runner(5);
            let start = Instant::now();
            let result = r.execute(&mut f.command());
            assert!(f.ready(), "fixture startup did not complete");
            if let Some(code) = expected {
                assert_eq!(result.err(), Some(Error(code)));
            } else {
                assert!(result.unwrap().success());
            }
            assert!(start.elapsed() < Duration::from_secs(6));
            assert!(unrelated.try_wait().unwrap().is_none());
        }
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
    }
    #[test]
    fn exact_read_only_shapes_are_allowed_and_every_other_shape_is_rejected() {
        let oid = "a".repeat(40);
        let tree = format!("{oid}^{{tree}}");
        for (program, args) in [
            ("git", vec!["--version"]),
            ("gh", vec!["--version"]),
            (
                "git",
                vec![
                    "--no-replace-objects",
                    "-C",
                    "/synthetic/repo",
                    "rev-parse",
                    "--verify",
                    &tree,
                ],
            ),
            (
                "git",
                vec![
                    "--no-replace-objects",
                    "-C",
                    "/synthetic/repo",
                    "ls-tree",
                    "-r",
                    "-z",
                    "--full-tree",
                    "--name-only",
                    &oid,
                ],
            ),
        ] {
            assert!(allowed(&Call::new(
                program,
                args.into_iter().map(String::from).collect()
            )));
        }
        for route in [
            "repos/owner/repo".into(),
            "repos/owner/repo/git/ref/heads/main".into(),
            format!("repos/owner/repo/git/commits/{oid}"),
            format!("repos/owner/repo/git/trees/{oid}?recursive=1"),
        ] {
            assert!(allowed(&crate::git::api_call(&route)));
        }
        let dir = Fixture::new("");
        let path = dir.0.to_str().unwrap();
        let mut r = runner(5);
        let bad = [
            ("git", vec!["-C", path, "init", "--quiet"]),
            ("git", vec!["fetch"]),
            ("git", vec!["clone", "example"]),
            ("git", vec!["checkout", "main"]),
            ("git", vec!["-c", "alias.fixture=!touch invoked", "fixture"]),
            ("git", vec!["--version", "--help"]),
            (
                "git",
                vec![
                    "--no-replace-objects",
                    "-C",
                    path,
                    "rev-parse",
                    "--verify",
                    "HEAD^{tree}",
                ],
            ),
            (
                "git",
                vec![
                    "--no-replace-objects",
                    "-C",
                    "/a/../b",
                    "ls-tree",
                    "-r",
                    "-z",
                    "--full-tree",
                    "--name-only",
                    &oid,
                ],
            ),
            ("gh", vec!["api", "--method", "POST", "repos/owner/repo"]),
            ("gh", vec!["api", "--method", "DELETE", "repos/owner/repo"]),
            ("gh", vec!["api", "--method", "PATCH", "repos/owner/repo"]),
            (
                "gh",
                vec![
                    "api",
                    "--method",
                    "GET",
                    "repos/owner/repo",
                    "--input",
                    "payload",
                ],
            ),
            (
                "gh",
                vec![
                    "api",
                    "--method",
                    "GET",
                    "https://foreign.example/repos/owner/repo",
                ],
            ),
            (
                "gh",
                vec!["api", "--method", "GET", "repos/owner/repo/issues"],
            ),
            (
                "gh",
                vec![
                    "api",
                    "--method",
                    "GET",
                    "repos/owner/repo/git/commits/main",
                ],
            ),
            ("gh", vec!["api", "--method", "GET", "repos/../repo"]),
            (
                "gh",
                vec!["api", "--method", "GET", "repos/owner/repo?extra=1"],
            ),
        ];
        // An expired execution deadline makes a guard regression fail safely
        // as capture_timeout, before any real Git/gh invocation can occur.
        r.deadline = Instant::now();
        for (program, args) in bad {
            let call = Call::new(program, args.into_iter().map(String::from).collect());
            assert!(!allowed(&call));
            assert_eq!(r.run(&call).err(), Some(Error("invalid_command")));
        }
        assert!(!dir.0.join(".git").exists());
        assert!(!dir.0.join("invoked").exists());
        // Rejection precedes even the expired deadline check (and hence spawn).
        r.deadline = Instant::now();
        assert_eq!(
            r.run(&Call::new("git", vec!["init".into()])).err(),
            Some(Error("invalid_command"))
        );
    }
}
