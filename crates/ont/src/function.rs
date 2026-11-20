//! `ont fn`: call a function a domain declares (`specs/ont/fn.md`).
//!
//! The engine knows only the declaration: the parameters, the program's
//! path, the bound on the answer's bytes and the time limit. It finds the
//! function among the domains `describe` lists for the caller, checks the
//! parameters as `action validate` checks an action's, checks the program
//! file, starts it in its own process group with a cleared environment and
//! the parameters on standard input, and reads its answer within the bounds.
//!
//! No listener and no threads: the program's three pipes are served by one
//! `poll` loop in the calling thread. A program over its time or its answer
//! bound has its process group killed before it is reaped.
use crate::action::{DeclaredParameter, check_parameter_values};
use crate::principal::Principal;
use crate::refusal::{open_store, st};
use crate::{Answer, Context, Failure, READ_ONLY_ENV, Refusal, internal};
use ont_core::schema::FunctionDef;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// `PATH` of the program when the caller has none.
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// A call: the function's name, the domain when several declare it, and the
/// parameters (the arguments of the MCP tool `ont_fn`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FnRequest {
    pub name: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub parameters: Option<Value>,
}
impl FnRequest {
    /// Parse the arguments of `ont_fn`: `{"name", "domain"?, "parameters"?}`.
    pub fn parse(bytes: &[u8]) -> Result<Self, Refusal> {
        object(bytes)?;
        crate::cli::parse_request(bytes)
    }
    /// Parse the standard input of `ont fn <name>`:
    /// `{"parameters"?, "domain"?}`.
    pub fn parse_stdin(name: &str, bytes: &[u8]) -> Result<Self, Refusal> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Stdin {
            #[serde(default)]
            domain: Option<String>,
            #[serde(default)]
            parameters: Option<Value>,
        }
        object(bytes)?;
        let input: Stdin = crate::cli::parse_request(bytes)?;
        Ok(Self {
            name: name.to_string(),
            domain: input.domain,
            parameters: input.parameters,
        })
    }
}

/// A request is one JSON object (serde would also read a struct from an
/// array).
fn object(bytes: &[u8]) -> Result<(), Refusal> {
    match crate::cli::parse_request::<Value>(bytes)? {
        Value::Object(_) => Ok(()),
        _ => Err(Refusal::input("invalid_request", "", "one JSON object")),
    }
}

/// How the program ended, as the pump saw it.
enum Ended {
    Exited {
        status: ExitStatus,
        answer: Vec<u8>,
        stderr_bytes: u64,
    },
    TooLarge,
    TimedOut,
}

/// The function `req` names among the domains `describe` lists for the
/// caller: `(domain, declaration)`.
fn resolve(ctx: &Context, req: &FnRequest) -> Result<(String, FunctionDef, Value), Failure> {
    let mut store = open_store(&ctx.store, false)?;
    let set = st!(store.schema_set()).clone();
    drop(store);
    let principal = Principal::bind(&set)?;
    let declaring: Vec<String> = principal
        .view()
        .domains()
        .into_iter()
        .filter(|d| {
            set.schema(d)
                .is_some_and(|s| s.functions.contains_key(&req.name))
        })
        .collect();
    let domain = match &req.domain {
        Some(d) if declaring.contains(d) => d.clone(),
        Some(_) => return Err(unknown_function()),
        None => match declaring.as_slice() {
            [] => return Err(unknown_function()),
            [one] => one.clone(),
            _ => {
                return Err(Refusal::input(
                    "function_ambiguous",
                    "/domain",
                    "one of the domains that declare the function",
                )
                .with("domains", json!(declaring))
                .into());
            }
        },
    };
    let Some(schema) = set.schema(&domain) else {
        return Err(unknown_function());
    };
    let Some(def) = schema.functions.get(&req.name).cloned() else {
        return Err(unknown_function());
    };
    let declared: Vec<DeclaredParameter<'_>> = def
        .parameters
        .iter()
        .map(|(name, p)| DeclaredParameter {
            name,
            ty: &p.ty,
            required: p.required,
            max_bytes: None,
        })
        .collect();
    let supplied = req.parameters.clone().unwrap_or_else(|| json!({}));
    let checked = check_parameter_values(schema, &declared, &supplied, "describe")?;
    Ok((domain, def, json!(checked)))
}

fn unknown_function() -> Failure {
    Refusal::input(
        "unknown_function",
        "/name",
        "a function of a domain listed by describe",
    )
    .into()
}

/// The first condition on the program file that fails, else `Ok`.
pub fn program_condition(program: &str) -> Result<(), &'static str> {
    let path = Path::new(program);
    if !path.is_absolute() {
        return Err("not_absolute");
    }
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Err("not_found"),
        Err(_) => return Err("not_accessible"),
    };
    if !meta.is_file() {
        return Err("not_regular_file");
    }
    if meta.permissions().mode() & 0o111 == 0
        || rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_err()
    {
        return Err("not_executable");
    }
    let real_uid = rustix::process::getuid().as_raw();
    if meta.uid() != 0 && meta.uid() != real_uid {
        return Err("owner_not_trusted");
    }
    if meta.mode() & 0o022 != 0 {
        return Err("writable_by_group_or_others");
    }
    Ok(())
}

/// The program's whole environment: `PATH`, `HOME` and `LANG` (the caller's
/// values, else defaults), the store, the function's name, read only.
fn environment(store: &Path, name: &str) -> Vec<(&'static str, std::ffi::OsString)> {
    let inherited = |key: &str, default: &str| {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| default.into())
    };
    vec![
        ("PATH", inherited("PATH", DEFAULT_PATH)),
        ("HOME", inherited("HOME", "/")),
        ("LANG", inherited("LANG", "C.UTF-8")),
        ("ONT_FN_STORE", store.as_os_str().to_owned()),
        ("ONT_FN_NAME", name.into()),
        (READ_ONLY_ENV, "1".into()),
    ]
}

fn set_nonblocking(fd: impl AsFd) -> std::io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(&fd)?;
    rustix::fs::fcntl_setfl(&fd, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}

/// Kill the program's process group (the program is not reaped yet, so its
/// id still names the group) and reap the program.
fn kill_group(child: &mut Child) {
    let _ = rustix::process::kill_process_group(
        rustix::process::Pid::from_child(child),
        rustix::process::Signal::KILL,
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// Read what is ready on `pipe` into `sink` (`None`: discard, count only).
/// Returns `Ok(true)` at end of file.
fn drain(
    pipe: &mut impl Read,
    buf: &mut [u8],
    sink: Option<&mut Vec<u8>>,
    count: &mut u64,
) -> std::io::Result<bool> {
    let mut sink = sink;
    loop {
        match pipe.read(buf) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                *count += n as u64;
                if let Some(s) = sink.as_deref_mut() {
                    s.extend_from_slice(&buf[..n]);
                }
                // One read per readiness keeps the bound check timely.
                return Ok(false);
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(e) => return Err(e),
        }
    }
}

/// Serve the program's pipes until it ends, its answer passes
/// `answer_bytes`, or `deadline` passes.
fn pump(
    child: &mut Child,
    input: &[u8],
    answer_bytes: u64,
    deadline: Instant,
) -> Result<Ended, Failure> {
    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    for result in [
        stdin.as_ref().map(set_nonblocking),
        stdout.as_ref().map(set_nonblocking),
        stderr.as_ref().map(set_nonblocking),
    ]
    .into_iter()
    .flatten()
    {
        if result.is_err() {
            kill_group(child);
            return Err(Failure::Internal(internal!("function_pipe_failed")));
        }
    }
    let mut pending = input;
    let mut answer = Vec::new();
    let (mut answer_count, mut stderr_bytes) = (0u64, 0u64);
    let mut buf = vec![0u8; 65_536];
    while stdout.is_some() || stderr.is_some() {
        if pending.is_empty() {
            stdin = None;
        }
        let now = Instant::now();
        if now >= deadline {
            kill_group(child);
            return Ok(Ended::TimedOut);
        }
        let left = deadline - now;
        let timeout = Timespec {
            tv_sec: left.as_secs() as _,
            tv_nsec: left.subsec_nanos() as _,
        };
        let (write_ready, out_ready, err_ready) = {
            let mut fds = Vec::with_capacity(3);
            let mut slots = [usize::MAX; 3];
            if let Some(fd) = &stdin {
                slots[0] = fds.len();
                fds.push(PollFd::new(fd, PollFlags::OUT));
            }
            if let Some(fd) = &stdout {
                slots[1] = fds.len();
                fds.push(PollFd::new(fd, PollFlags::IN));
            }
            if let Some(fd) = &stderr {
                slots[2] = fds.len();
                fds.push(PollFd::new(fd, PollFlags::IN));
            }
            match poll(&mut fds, Some(&timeout)) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => {
                    drop(fds);
                    kill_group(child);
                    return Err(Failure::Internal(internal!("function_poll_failed")));
                }
            }
            let ready = |slot: usize| slot != usize::MAX && !fds[slot].revents().is_empty();
            (ready(slots[0]), ready(slots[1]), ready(slots[2]))
        };
        if write_ready && let Some(pipe) = stdin.as_mut() {
            match pipe.write(pending) {
                Ok(n) => pending = &pending[n..],
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                // The program closed its standard input: nothing more to give.
                Err(_) => pending = &[],
            }
        }
        if out_ready && let Some(pipe) = stdout.as_mut() {
            match drain(pipe, &mut buf, Some(&mut answer), &mut answer_count) {
                Ok(true) => stdout = None,
                Ok(false) => {}
                Err(_) => stdout = None,
            }
            if answer_count > answer_bytes {
                kill_group(child);
                return Ok(Ended::TooLarge);
            }
        }
        if err_ready && let Some(pipe) = stderr.as_mut() {
            match drain(pipe, &mut buf, None, &mut stderr_bytes) {
                Ok(true) | Err(_) => stderr = None,
                Ok(false) => {}
            }
        }
    }
    drop(stdin);
    // Both outputs are closed: wait for the exit within the time left.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Ended::Exited {
                    status,
                    answer,
                    stderr_bytes,
                });
            }
            Ok(None) => {}
            Err(_) => {
                kill_group(child);
                return Err(Failure::Internal(internal!("function_wait_failed")));
            }
        }
        let now = Instant::now();
        if now >= deadline {
            kill_group(child);
            return Ok(Ended::TimedOut);
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(2)));
    }
}

/// Run the function `req` names and return its answer.
pub fn call(ctx: &Context, req: &FnRequest) -> Result<Answer, Failure> {
    let (domain, def, parameters) = resolve(ctx, req)?;
    let named = |r: Refusal| -> Failure {
        r.with("function", json!(req.name))
            .with("domain", json!(domain))
            .into()
    };
    if let Err(condition) = program_condition(&def.program) {
        return Err(named(
            Refusal::new("function_program_unusable").with("detail", json!(condition)),
        ));
    }
    let store: PathBuf = std::path::absolute(&ctx.store).unwrap_or_else(|_| ctx.store.clone());
    let input = parameters.to_string().into_bytes();
    let start = Instant::now();
    let deadline = start + Duration::from_millis(def.timeout_ms);
    let mut child = match Command::new(&def.program)
        .env_clear()
        .envs(environment(&store, &req.name))
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return Err(named(
                Refusal::new("function_program_unusable").with("detail", json!("start_failed")),
            ));
        }
    };
    let ended = pump(&mut child, &input, def.answer_bytes, deadline)?;
    let elapsed_ms = start.elapsed().as_millis() as u64;
    match ended {
        Ended::TimedOut => Err(named(
            Refusal::new("function_timeout").with("timeout_ms", json!(def.timeout_ms)),
        )),
        Ended::TooLarge => Err(named(
            Refusal::new("function_answer_too_large").with("answer_bytes", json!(def.answer_bytes)),
        )),
        Ended::Exited {
            status,
            stderr_bytes,
            ..
        } if !status.success() => Err(named(Refusal::new("function_failed").with(
            "detail",
            json!({"exit":status.code(),"stderr_bytes":stderr_bytes}),
        ))),
        Ended::Exited { answer, .. } => match serde_json::from_slice::<Value>(&answer) {
            Ok(answer) => Ok(Answer::Plain(json!({
                "function":req.name,
                "domain":domain,
                "answer":answer,
                "elapsed_ms":elapsed_ms
            }))),
            Err(_) => Err(named(
                Refusal::new("function_answer_not_json").with("bytes", json!(answer.len())),
            )),
        },
    }
}
