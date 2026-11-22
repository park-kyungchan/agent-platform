//! The door of a stage with `limits` (`specs/ap/door.md`): one Unix socket that `ap run` binds
//! beside the stage's scratch directory, at a path the stage's view holds and cannot write, and
//! serves outside the stage, in its own process, until the stage has ended. It answers two
//! requests and nothing else: `check`, the call of `ap check` for the stage's own worktree on the
//! local PC (it never runs a command here), and `fn`, a function of the store on its list, asked
//! as the stage. Every request leaves one line of the door's record. `ap fn <name>` is the client
//! of `fn`, `ap check` under `AP_DOOR` the client of `check`. The bytes between the two ends are
//! this crate's own: the request is one line of JSON; the answer is frames of one tag byte and a
//! four-byte big-endian length, the two output streams as they are produced, then one closing
//! message, a JSON object.
use crate::check::{self, Call, Ctx, Ended, Io};
use crate::host::System;
use crate::run::stamp;
use serde_json::{Value, json};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

/// The door's list: the functions of the store a stage may ask for (R5).
pub(crate) const FUNCTIONS: [&str; 2] = ["route", "clause"];
/// The roles whose `check` the door serves (R3).
const CHECKERS: [&str; 3] = ["implementer", "fixer", "verifier"];
/// The longest request line, and the time a connection has to send it (R2).
const REQUEST_MAX: usize = 65_536;
const READ_WITHIN: Duration = Duration::from_secs(10);
/// The requests served at once (R2).
const AT_ONCE: usize = 4;
/// The door's own bounds of a function's call: its time and each of its two streams (R5).
const FN_BOUND: Duration = Duration::from_secs(630);
const FN_BYTES: usize = 2 * 1024 * 1024;
const MAX_TIMEOUT: u64 = 14_400;
const DEFAULT_LOG: &str = "/var/lib/agent-platform/door.jsonl";
/// The longest `name` of a record line, in bytes (R6).
const NAME_MAX: usize = 200;
/// The longest input of `ap fn` (R8).
const INPUT_MAX: usize = 60_000;
/// The longest path a socket address holds, without its NUL.
const ADDRESS_MAX: usize = 107;
/// The tags of the answer's frames.
const TAG_OUT: u8 = 1;
const TAG_ERR: u8 = 2;
const TAG_END: u8 = 3;
/// The longest frame a client takes.
const FRAME_MAX: usize = 1 << 24;
/// How long the closing waits for the requests it ended to answer on their own, and in all (R7).
const CLOSE_GRACE: Duration = Duration::from_secs(3);
const CLOSE_WITHIN: Duration = Duration::from_secs(11);

/// The path of the door of a scratch directory (R1): in its parent, its last component and `.door`.
pub(crate) fn path_of(scratch: &str) -> String {
    let p = Path::new(scratch);
    let name = p.file_name().unwrap_or_default().to_string_lossy();
    let parent = p.parent().unwrap_or(Path::new("/"));
    parent
        .join(format!("{name}.door"))
        .to_string_lossy()
        .into_owned()
}

/// What the door knows of its stage: its role and agent as the engine gets them (`ONT_ROLE`,
/// `ONT_AGENT`), its record, its `cwd` and the `store` its request names.
pub(crate) struct Stage {
    pub role: String,
    pub agent: String,
    pub run: String,
    pub cwd: String,
    pub store: Option<String>,
}

/// One of the two output streams of an answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Stream {
    Out,
    Err,
}
/// The closing message of an answer (R2): the exit status of what ran, `here` with a reason
/// (`check`, R4), `ended` with a reason (`bound`, `closed`), or a refusal's code and field.
#[derive(Debug, PartialEq)]
pub(crate) enum Closing {
    Exit(i32),
    Here(String),
    Ended(String),
    Refused(String, String),
}
impl Closing {
    fn to_json(&self) -> Value {
        match self {
            Closing::Exit(n) => json!({ "exit": n }),
            Closing::Here(r) => json!({ "here": r }),
            Closing::Ended(r) => json!({ "ended": r }),
            Closing::Refused(code, field) => json!({"refused": {"code": code, "field": field}}),
        }
    }
    fn of(v: &Value) -> Option<Self> {
        let o = v.as_object().filter(|o| o.len() == 1)?;
        let text = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
        if let Some(n) = o.get("exit") {
            return n
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .map(Closing::Exit);
        }
        if let Some(r) = text("here") {
            return Some(Closing::Here(r));
        }
        if let Some(r) = text("ended") {
            return Some(Closing::Ended(r));
        }
        let r = o.get("refused")?;
        let (code, field) = (r["code"].as_str()?, r["field"].as_str()?);
        Some(Closing::Refused(code.to_owned(), field.to_owned()))
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
/// Kills a process group the door started (R7), by the system's own program and no variable.
fn kill_group(pid: u32) {
    let _ = Command::new("/bin/kill")
        .env_clear()
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// What the record line of a request holds before its end (R6).
struct Line {
    at: SystemTime,
    arrival: Instant,
    verb: Option<&'static str>,
    name: Option<String>,
}
/// One request in service: how it was ended, if it was, the process groups of the programs
/// started for it, its connection and its record line.
struct Req {
    state: Mutex<(Option<&'static str>, Vec<u32>)>,
    stream: Option<UnixStream>,
    line: Mutex<Line>,
    logged: AtomicBool,
}
impl Req {
    /// Ends the request for `reason` (the first reason stands) and kills every program of it.
    fn end(&self, reason: &'static str) {
        let mut st = lock(&self.state);
        st.0.get_or_insert(reason);
        for pid in &st.1 {
            kill_group(*pid);
        }
    }
    fn ended(&self) -> Option<&'static str> {
        lock(&self.state).0
    }
    fn started(&self, pid: u32) {
        let mut st = lock(&self.state);
        st.1.push(pid);
        // A program started after the end is killed at once.
        if st.0.is_some() {
            kill_group(pid);
        }
    }
    fn finished(&self, pid: u32) {
        lock(&self.state).1.retain(|p| *p != pid);
    }
}
/// The client of one request: the frames of its answer, and the programs started for it.
struct Client {
    stream: Mutex<Option<UnixStream>>,
    req: Arc<Req>,
}
impl Client {
    fn frame(&self, tag: u8, bytes: &[u8]) {
        let mut stream = lock(&self.stream);
        let Some(s) = stream.as_mut() else {
            return;
        };
        let mut head = [tag, 0, 0, 0, 0];
        head[1..].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
        if s.write_all(&head)
            .and_then(|()| s.write_all(bytes))
            .is_err()
        {
            *stream = None;
            drop(stream);
            self.req.end("client_gone");
        }
    }
    fn send(&self, tag: u8, bytes: &[u8]) {
        for chunk in bytes.chunks(1 << 20) {
            self.frame(tag, chunk);
        }
    }
}
impl Io for Client {
    fn out(&self, bytes: &[u8]) {
        self.send(TAG_OUT, bytes);
    }
    fn err(&self, bytes: &[u8]) {
        self.send(TAG_ERR, bytes);
    }
    fn started(&self, pid: u32) {
        self.req.started(pid);
    }
    fn finished(&self, pid: u32) {
        self.req.finished(pid);
    }
}

/// What every request of the door shares.
struct Shared {
    stage: Stage,
    /// The stage's `cwd` resolved through every link, outside the stage (R3).
    cwd: Option<String>,
    /// The port's environment and policy file, read once at the opening (R1).
    vars: Arc<Vec<(String, String)>>,
    policy: Option<PathBuf>,
    stop: AtomicBool,
    served: Mutex<Vec<Arc<Req>>>,
    busy: Mutex<usize>,
    freed: Condvar,
    handlers: Mutex<Vec<JoinHandle<()>>>,
}
impl Shared {
    fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// An open door; `close` ends it (R7), as its drop does.
pub(crate) struct Door {
    path: PathBuf,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
}

static TEMPORARY: AtomicU32 = AtomicU32::new(0);
/// The address of `name` in the directory `dir` holds: `/proc/self/fd/<n>/<name>`.
fn through(dir: &File, name: &OsStr) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())).join(name)
}
/// Binds the socket at `path`, whatever its length (R1): a path a socket address cannot hold is
/// reached through a descriptor of its directory, and a name too long even so is bound at a short
/// name there and then linked to its own, which fails when that name is taken.
fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if path.as_os_str().len() <= ADDRESS_MAX {
        return UnixListener::bind(path);
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    let dir = File::open(parent)?;
    let short = through(&dir, name);
    if short.as_os_str().len() <= ADDRESS_MAX {
        return UnixListener::bind(short);
    }
    let n = TEMPORARY.fetch_add(1, Ordering::SeqCst);
    let temporary = format!(".ap-door.{}.{n}", std::process::id());
    let listener = UnixListener::bind(through(&dir, OsStr::new(&temporary)))?;
    let linked = fs::hard_link(parent.join(&temporary), path);
    let _ = fs::remove_file(parent.join(&temporary));
    linked.map(|()| listener)
}
/// Connects to the socket at `path`, whatever its length (R8).
fn connect(path: &Path) -> std::io::Result<UnixStream> {
    if path.as_os_str().len() <= ADDRESS_MAX {
        return UnixStream::connect(path);
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    let dir = File::open(parent)?;
    let short = through(&dir, name);
    if short.as_os_str().len() <= ADDRESS_MAX {
        return UnixStream::connect(short);
    }
    let socket = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::PATH.bits() as i32)
        .open(path)?;
    UnixStream::connect(format!("/proc/self/fd/{}", socket.as_raw_fd()))
}
fn remove_socket(path: &Path) {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_socket()) {
        let _ = fs::remove_file(path);
    }
}

/// Opens the door of a stage at `path` (R1): binds the socket with mode 0600 and serves it on a
/// thread of this process. The port is read here once: its environment and its policy file.
pub(crate) fn open(s: &impl System, stage: Stage, path: &str) -> std::io::Result<Door> {
    let vars = Arc::new(s.environment());
    let policy = s.policy_file();
    let listener = bind(Path::new(path))?;
    let ready = fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .and_then(|()| listener.set_nonblocking(true));
    if let Err(e) = ready {
        drop(listener);
        remove_socket(Path::new(path));
        return Err(e);
    }
    let cwd = fs::canonicalize(&stage.cwd)
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned));
    let shared = Arc::new(Shared {
        stage,
        cwd,
        vars,
        policy,
        stop: AtomicBool::new(false),
        served: Mutex::new(vec![]),
        busy: Mutex::new(0),
        freed: Condvar::new(),
        handlers: Mutex::new(vec![]),
    });
    let accept = {
        let shared = shared.clone();
        std::thread::spawn(move || accept(&listener, &shared))
    };
    Ok(Door {
        path: PathBuf::from(path),
        shared,
        accept: Some(accept),
    })
}
fn accept(listener: &UnixListener, shared: &Arc<Shared>) {
    while !shared.stop.load(Ordering::SeqCst) {
        let Ok((stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        // Its turn: at most four requests are served at once (R2).
        {
            let mut busy = lock(&shared.busy);
            while *busy >= AT_ONCE && !shared.stop.load(Ordering::SeqCst) {
                busy = match shared.freed.wait_timeout(busy, Duration::from_millis(50)) {
                    Ok((b, _)) => b,
                    Err(e) => e.into_inner().0,
                };
            }
            if shared.stop.load(Ordering::SeqCst) {
                return;
            }
            *busy += 1;
        }
        let req = Arc::new(Req {
            state: Mutex::new((None, vec![])),
            stream: stream.try_clone().ok(),
            line: Mutex::new(Line {
                at: SystemTime::now(),
                arrival: Instant::now(),
                verb: None,
                name: None,
            }),
            logged: AtomicBool::new(false),
        });
        lock(&shared.served).push(req.clone());
        let handler = {
            let shared = shared.clone();
            std::thread::spawn(move || {
                serve(&shared, stream, &req);
                *lock(&shared.busy) -= 1;
                shared.freed.notify_all();
            })
        };
        lock(&shared.handlers).push(handler);
    }
}

/// The request line: one line of at most 65,536 bytes, read within 10 seconds (R2).
fn read_line(stream: &mut UnixStream) -> Option<Vec<u8>> {
    let deadline = Instant::now() + READ_WITHIN;
    let mut line = vec![];
    let mut buf = [0u8; 8192];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        stream.set_read_timeout(Some(left)).ok()?;
        match stream.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                line.extend_from_slice(&buf[..n]);
                if let Some(i) = line.iter().position(|b| *b == b'\n') {
                    line.truncate(i);
                    return (i <= REQUEST_MAX).then_some(line);
                }
                if line.len() > REQUEST_MAX {
                    return None;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
}

/// What a request asks, once read (R2, R3, R5).
enum What {
    Check(Call),
    Fn(String, Value),
}
type Refusal = (&'static str, &'static str);
fn invalid<T>(field: &'static str) -> Result<T, Refusal> {
    Err(("invalid_request", field))
}
fn not_admitted<T>(field: &'static str) -> Result<T, Refusal> {
    Err(("not_admitted", field))
}
/// At most 200 bytes of a name, cut at a character (R6).
fn cut(name: &str) -> String {
    let mut end = name.len().min(NAME_MAX);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}
/// The verb, the name of the record line and what is asked, or the refusal.
fn asked(
    shared: &Shared,
    line: Option<&[u8]>,
) -> (Option<&'static str>, Option<String>, Result<What, Refusal>) {
    let v: Option<Value> = line.and_then(|l| serde_json::from_slice(l).ok());
    let Some(o) = v.as_ref().and_then(Value::as_object) else {
        return (None, None, invalid("request"));
    };
    let verb = match o.get("verb").and_then(Value::as_str) {
        Some("check") => Some("check"),
        Some("fn") => Some("fn"),
        _ => None,
    };
    let name = match verb {
        Some("check") => o.get("command").and_then(|c| c.get(0)),
        Some("fn") => o.get("name"),
        _ => None,
    };
    let name = name.and_then(Value::as_str).map(cut);
    let what = (|| {
        if o.get("door").and_then(Value::as_u64) != Some(1) {
            return invalid("door");
        }
        let members: &[&'static str] = match verb {
            Some("check") => &["door", "verb", "dir", "timeout", "command"],
            Some("fn") => &["door", "verb", "name", "input"],
            _ => return invalid("verb"),
        };
        if o.keys().any(|k| !members.contains(&k.as_str())) {
            return invalid("request");
        }
        if let Some(missing) = members.iter().find(|k| !o.contains_key(**k)) {
            return invalid(missing);
        }
        match verb {
            Some("check") => check_of(shared, o),
            _ => fn_of(o),
        }
    })();
    (verb, name, what)
}
/// A `check` request (R3): its form, then the stage's role, then its `dir`.
fn check_of(shared: &Shared, o: &serde_json::Map<String, Value>) -> Result<What, Refusal> {
    let Some(dir) = o["dir"].as_str() else {
        return invalid("dir");
    };
    let Some(timeout) = o["timeout"]
        .as_u64()
        .filter(|t| (1..=MAX_TIMEOUT).contains(t))
    else {
        return invalid("timeout");
    };
    let words = o["command"].as_array().filter(|a| !a.is_empty());
    let command: Option<Vec<String>> = words.and_then(|a| {
        a.iter()
            .map(|w| w.as_str().filter(|w| !w.contains('\0')).map(str::to_owned))
            .collect()
    });
    let Some(command) = command.filter(|c| !c[0].is_empty()) else {
        return invalid("command");
    };
    if !CHECKERS.contains(&shared.stage.role.as_str()) {
        return not_admitted("role");
    }
    // The worktree is the door's own resolved `cwd`: no path of a request is opened.
    let Some(cwd) = shared.cwd.as_ref().filter(|c| c.as_str() == dir) else {
        return not_admitted("dir");
    };
    Ok(What::Check(Call {
        dir: cwd.clone(),
        timeout,
        command,
    }))
}
/// A `fn` request (R5).
fn fn_of(o: &serde_json::Map<String, Value>) -> Result<What, Refusal> {
    let Some(name) = o["name"].as_str() else {
        return invalid("name");
    };
    if !o["input"].is_object() {
        return invalid("input");
    }
    if !FUNCTIONS.contains(&name) {
        return not_admitted("name");
    }
    Ok(What::Fn(name.to_owned(), o["input"].clone()))
}

/// One connection: its request read, refused or served, its closing message and its line.
fn serve(shared: &Arc<Shared>, mut stream: UnixStream, req: &Arc<Req>) {
    let _ = stream.set_nonblocking(false);
    let line = read_line(&mut stream);
    let _ = stream.set_read_timeout(None);
    let (verb, name, what) = asked(shared, line.as_deref());
    {
        let mut l = lock(&req.line);
        l.verb = verb;
        l.name = name;
    }
    let client = Arc::new(Client {
        stream: Mutex::new(stream.try_clone().ok()),
        req: req.clone(),
    });
    let done = Arc::new(AtomicBool::new(false));
    let closing = match what {
        Err((code, field)) => Closing::Refused(code.into(), field.into()),
        Ok(_) if req.ended().is_some() => Closing::Ended(String::new()),
        Ok(what) => {
            watch(&stream, req, &done);
            match what {
                What::Check(call) => {
                    let (arrival, at) = {
                        let l = lock(&req.line);
                        (l.arrival, l.at)
                    };
                    let io: Arc<dyn Io> = client.clone();
                    let ctx = Ctx::of(shared.vars.clone(), io);
                    let r = req.clone();
                    let served = move || r.ended().is_none();
                    match check::for_door(
                        &ctx,
                        &call,
                        shared.policy.clone(),
                        (arrival, at),
                        &served,
                    ) {
                        Ended::Exit(n) => Closing::Exit(n),
                        Ended::Here(reason) => Closing::Here(reason.into()),
                    }
                }
                What::Fn(name, input) => call_fn(shared, &client, &name, &input),
            }
        }
    };
    let closing = match req.ended() {
        Some(reason) => Closing::Ended(reason.into()),
        None => closing,
    };
    client.frame(TAG_END, closing.to_json().to_string().as_bytes());
    log(shared, req, &closing);
    done.store(true, Ordering::SeqCst);
    let _ = stream.shutdown(Shutdown::Both);
}
/// A client that closes its connection ends its request (R4): the connection is read to its end.
fn watch(stream: &UnixStream, req: &Arc<Req>, done: &Arc<AtomicBool>) {
    let Ok(mut stream) = stream.try_clone() else {
        return;
    };
    let (req, done) = (req.clone(), done.clone());
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        if !done.load(Ordering::SeqCst) {
            req.end("client_gone");
        }
    });
}

/// `ont fn <name>` once, as the stage (R5).
fn call_fn(shared: &Shared, client: &Arc<Client>, name: &str, input: &Value) -> Closing {
    let mut c = Command::new("ont");
    c.args(["fn", name]);
    if let Some(store) = &shared.stage.store {
        c.args(["--store", store]);
    }
    c.env_clear();
    for k in ["PATH", "HOME", "LANG"] {
        if let Some(v) = shared.var(k) {
            c.env(k, v);
        }
    }
    c.env("ONT_ROLE", &shared.stage.role)
        .env("ONT_AGENT", &shared.stage.agent)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let Ok(mut child) = c.spawn() else {
        return Closing::Exit(127);
    };
    let pid = child.id();
    let req = &client.req;
    req.started(pid);
    let mut body = input.to_string().into_bytes();
    body.push(b'\n');
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&body);
        });
    }
    let (tx, rx) = mpsc::channel();
    let pipes: [(Option<Box<dyn Read + Send>>, u8); 2] = [
        (child.stdout.take().map(|p| Box::new(p) as _), TAG_OUT),
        (child.stderr.take().map(|p| Box::new(p) as _), TAG_ERR),
    ];
    let mut relays = 0;
    for (pipe, tag) in pipes {
        let Some(mut pipe) = pipe else {
            continue;
        };
        relays += 1;
        let (client, tx) = (client.clone(), tx.clone());
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut total = 0;
            loop {
                let n = match pipe.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                };
                let keep = n.min(FN_BYTES - total);
                client.send(tag, &buf[..keep]);
                total += keep;
                if keep < n {
                    client.req.end("bound");
                    break;
                }
            }
            let _ = tx.send(());
        });
    }
    let deadline = Instant::now() + FN_BOUND;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                req.end("bound");
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };
    for _ in 0..relays {
        let _ = rx.recv_timeout(Duration::from_secs(5));
    }
    req.finished(pid);
    Closing::Exit(status.map_or(1, check::code))
}

/// The request's line of the record (R6), once.
fn log(shared: &Shared, req: &Req, closing: &Closing) {
    if req.logged.swap(true, Ordering::SeqCst) {
        return;
    }
    let l = lock(&req.line);
    let (exit, reason) = match closing {
        Closing::Exit(n) => (json!(n), Value::Null),
        Closing::Here(r) | Closing::Ended(r) => (Value::Null, json!(r)),
        Closing::Refused(code, _) => (Value::Null, json!(code)),
    };
    let ms = u64::try_from(l.arrival.elapsed().as_millis()).unwrap_or(u64::MAX);
    let line = json!({"at": stamp(l.at), "run": shared.stage.run, "role": shared.stage.role,
        "verb": l.verb, "name": l.name, "ms": ms, "exit": exit, "reason": reason});
    let path = shared
        .var("AP_DOOR_LOG")
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_LOG);
    // A file made here is the caller's alone; one that exists keeps its mode and owner.
    let _ = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()));
}

impl Door {
    /// The closing (R7): no further connection, every request still served ended (its programs'
    /// groups killed, its line `closed`), the socket removed.
    pub(crate) fn close(mut self) {
        self.shut();
    }
    fn shut(&mut self) {
        let Some(accept) = self.accept.take() else {
            return;
        };
        let shared = &self.shared;
        shared.stop.store(true, Ordering::SeqCst);
        shared.freed.notify_all();
        let _ = accept.join();
        let begun = Instant::now();
        let served: Vec<Arc<Req>> = lock(&shared.served).clone();
        for r in &served {
            if !r.logged.load(Ordering::SeqCst) {
                r.end("closed");
            }
        }
        let open = || served.iter().any(|r| !r.logged.load(Ordering::SeqCst));
        let mut cut = false;
        while open() && begun.elapsed() < CLOSE_WITHIN {
            // A request that does not end on its own loses its connection.
            if !cut && begun.elapsed() >= CLOSE_GRACE {
                cut = true;
                for r in &served {
                    if let Some(s) = &r.stream {
                        let _ = s.shutdown(Shutdown::Both);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        for r in &served {
            r.end("closed");
            log(shared, r, &Closing::Ended("closed".into()));
        }
        let handlers = std::mem::take(&mut *lock(&shared.handlers));
        for h in handlers {
            if h.is_finished() {
                let _ = h.join();
            }
        }
        remove_socket(&self.path);
    }
}
impl Drop for Door {
    fn drop(&mut self) {
        self.shut();
    }
}

/// Asks the door at `path` once: sends `request`, passes each frame of the two streams to `each`
/// and returns the closing message, or None when the door cannot be reached or the answer ends
/// without one.
pub(crate) fn ask(
    path: &Path,
    request: &Value,
    mut each: impl FnMut(Stream, &[u8]),
) -> Option<Closing> {
    let mut stream = connect(path).ok()?;
    let mut line = request.to_string().into_bytes();
    line.push(b'\n');
    stream.write_all(&line).ok()?;
    loop {
        let mut head = [0u8; 5];
        stream.read_exact(&mut head).ok()?;
        let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
        if len > FRAME_MAX {
            return None;
        }
        let mut bytes = vec![0u8; len];
        stream.read_exact(&mut bytes).ok()?;
        match head[0] {
            TAG_OUT => each(Stream::Out, &bytes),
            TAG_ERR => each(Stream::Err, &bytes),
            TAG_END => return Closing::of(&serde_json::from_slice(&bytes).ok()?),
            _ => return None,
        }
    }
}

/// `ap fn --help`.
pub const FN_HELP: &str = "Usage: ap fn <name> < request.json\nInside a stage with limits: asks the stage's door (AP_DOOR) for the function <name> of the store, as the stage's role; the door answers route alone. The standard input is the function's input, one JSON object (empty: {}); the engine's standard output, standard error and exit status are ap fn's. A refusal exits 2, a door that cannot be reached 3 (no_door).\n";

fn refuse(code: &str, field: &str, status: u8) -> ExitCode {
    let v = json!({"error": {"code": code, "field": field, "detail": null}});
    match writeln!(std::io::stderr(), "{v}") {
        Ok(()) => ExitCode::from(status),
        Err(_) => ExitCode::from(4),
    }
}
fn name_ok(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}
/// `ap fn <name> < request.json` (R8).
pub fn cli(argv: &[OsString]) -> ExitCode {
    let name = match argv {
        [name] => name.to_str().filter(|n| name_ok(n)),
        _ => None,
    };
    let Some(name) = name else {
        return refuse("usage", "args", 2);
    };
    let mut bytes = vec![];
    let read = std::io::stdin()
        .take(INPUT_MAX as u64 + 1)
        .read_to_end(&mut bytes);
    let input = match read {
        Ok(_) if bytes.len() > INPUT_MAX => None,
        Ok(_) if bytes.iter().all(u8::is_ascii_whitespace) => Some(json!({})),
        Ok(_) => serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object),
        Err(_) => None,
    };
    let Some(input) = input else {
        return refuse("invalid_input", "stdin", 2);
    };
    let Some(at) = std::env::var_os("AP_DOOR").filter(|v| !v.is_empty()) else {
        return refuse("no_door", "AP_DOOR", 2);
    };
    let request = json!({"door": 1, "verb": "fn", "name": name, "input": input});
    let ended = ask(Path::new(&at), &request, |stream, bytes| {
        let _ = match stream {
            Stream::Out => std::io::stdout().write_all(bytes),
            Stream::Err => std::io::stderr().write_all(bytes),
        };
    });
    match ended {
        Some(Closing::Exit(n)) => ExitCode::from(u8::try_from(n).unwrap_or(1)),
        Some(Closing::Refused(code, field)) => refuse(&code, &field, 2),
        _ => refuse("no_door", "AP_DOOR", 3),
    }
}
