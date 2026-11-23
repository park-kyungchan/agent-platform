//! `ap`'s side of the proxy of a look on the PC (`specs/ap/pc-egress.md` R6): the program's text,
//! the words of its `outside` form for a role, and the taking of its lines out of the look's error
//! file into the egress log of `specs/ap/run-limits.md` R19. Unit pc-stage places the look and calls
//! these; this module launches nothing.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "unit pc-stage is the caller of these items; until it lands only the tests call them"
    )
)]
use super::{LOG_DEFAULT, RANGES, names};
use crate::host::System;
use serde_json::{Map, Value};
use std::{io::Write, path::Path};

/// The program of the look's proxy, run on the PC as `python3 -I -c <text> <form> <argument>...`
/// (R1); the file is `crates/ap/src/check/egress.py`.
pub(crate) const PROGRAM: &str = include_str!("../../check/egress.py");

/// The words of the program's `outside` form (R1, R6): the socket it binds, the nonce of its lines,
/// the role, and the role's names and the refused ranges as `names` and `RANGES` of this host's
/// proxy hold them, each joined by commas; no name or range is written a second time.
pub(crate) fn outside(role: &str, socket: &Path, nonce: &str) -> Vec<String> {
    vec![
        "outside".to_owned(),
        socket.to_string_lossy().into_owned(),
        nonce.to_owned(),
        role.to_owned(),
        names(role).join(","),
        RANGES.join(","),
    ]
}

/// The egress log's path: `AP_EGRESS_LOG` of the port's environment when set and not empty (the last
/// of several wins), else the default of R19.
fn log_path(s: &impl System) -> String {
    s.environment()
        .into_iter()
        .rev()
        .find(|(k, _)| k == "AP_EGRESS_LOG")
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| LOG_DEFAULT.to_owned())
}

/// The members of a line of the program (R4), in the order of R19 once `run` is put in.
const MEMBERS: [&str; 6] = ["at", "role", "name", "port", "status", "reason"];

/// The line of R19 for the JSON of a line of R4, with `run` the path of the call's record; None
/// when that JSON does not have exactly the members of R4 with their types.
fn entry(json: &[u8], run: &str) -> Option<Vec<u8>> {
    let Value::Object(mut given) = serde_json::from_slice::<Value>(json).ok()? else {
        return None;
    };
    let get = |m: &str| given.get(m);
    let text_or_null = |m: &str| get(m).is_some_and(|v| v.is_null() || v.is_string());
    let ok = given.len() == MEMBERS.len()
        && get("at").is_some_and(Value::is_string)
        && get("role").is_some_and(Value::is_string)
        && text_or_null("name")
        && get("port").is_some_and(|v| v.is_null() || v.as_u64().is_some_and(|p| p <= 65_535))
        && get("status").is_some_and(Value::is_u64)
        && text_or_null("reason");
    if !ok {
        return None;
    }
    let mut line = Map::new();
    for member in ["at", "run", "role", "name", "port", "status", "reason"] {
        let value = if member == "run" {
            Value::String(run.to_owned())
        } else {
            given.remove(member)?
        };
        line.insert(member.to_owned(), value);
    }
    let mut bytes = Value::Object(line).to_string().into_bytes();
    bytes.push(b'\n');
    Some(bytes)
}

/// One line of the log by one write in append mode; a file made here has mode 0600 and the mode of an
/// existing one is not changed (R19).
fn append(log: &str, line: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(log)?
        .write_all(line)
}

/// Takes out of the look's error file `errors` each complete line of R4 whose nonce is `nonce` (a line
/// that begins with a NUL byte) and appends its JSON, with `run` the path of the call's record, to the
/// egress log (R19), each line's own `at` kept. Every other byte of the file stays in its order: a
/// line of another nonce, one whose JSON is not R4's, a line cut short and any other output. A line
/// that cannot be appended stays in the file. The log is appended before the file is rewritten, so a
/// failure leaves a line twice, never none. Returns the number of lines taken; a file without one is
/// not written.
pub(crate) fn take(
    s: &impl System,
    errors: &Path,
    nonce: &str,
    run: &str,
) -> std::io::Result<usize> {
    let log = log_path(s);
    let bytes = std::fs::read(errors)?;
    let prefix = format!("\0ap-egress {nonce} ").into_bytes();
    let mut kept = Vec::with_capacity(bytes.len());
    let mut taken = 0;
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        let line_of_r4 = line
            .strip_prefix(prefix.as_slice())
            .and_then(|rest| rest.strip_suffix(b"\n"))
            .and_then(|json| entry(json, run));
        match line_of_r4 {
            Some(out) if append(&log, &out).is_ok() => taken += 1,
            _ => kept.extend_from_slice(line),
        }
    }
    if taken > 0 {
        std::fs::write(errors, kept)?;
    }
    Ok(taken)
}

#[cfg(test)]
mod tests {
    use super::{LOG_DEFAULT, PROGRAM, RANGES, log_path, names, outside, take};
    use crate::host::{Answer, System, error};
    use serde_json::{Value, json};
    use std::{
        fs,
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        os::unix::{fs::PermissionsExt, net::UnixStream},
        path::{Path, PathBuf},
        process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Output, Stdio},
        sync::{
            Mutex,
            atomic::{AtomicU32, Ordering},
        },
        time::{Duration, Instant},
    };

    static COUNT: AtomicU32 = AtomicU32::new(0);
    /// The tests that run `check` one after the other: each compares `/tmp` before and after.
    static CHECKS: Mutex<()> = Mutex::new(());
    const OK: &[u8] = b"HTTP/1.1 200 Connection established\r\n\r\n";

    /// A directory under `/tmp` (a socket address holds 107 bytes of path), removed at the drop.
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Self {
            let n = COUNT.fetch_add(1, Ordering::SeqCst);
            let path = PathBuf::from(format!("/tmp/ap-pce-{}-{n}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn python(args: &[&str]) -> Command {
        let mut c = Command::new("python3");
        c.arg("-I").arg("-c").arg(PROGRAM).args(args);
        c
    }
    fn wait_within(child: &mut Child, seconds: u64) -> Option<ExitStatus> {
        let end = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < end {
            if let Some(status) = child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
    /// Runs a command to its end within 30 seconds, its output captured.
    fn finish(mut command: Command) -> Output {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if wait_within(&mut child, 30).is_none() {
            let _ = child.kill();
            panic!("the program did not end");
        }
        child.wait_with_output().unwrap()
    }
    fn signal(child: &Child, name: &str) {
        let status = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(child.id().to_string())
            .status()
            .unwrap();
        assert!(status.success());
    }
    fn listener() -> (TcpListener, u16) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        (l, port)
    }
    fn closed_port() -> u16 {
        listener().1
    }
    fn at_format(at: &str) -> bool {
        let b = at.as_bytes();
        b.len() == 20
            && b.iter().enumerate().all(|(i, c)| match i {
                4 | 7 => *c == b'-',
                10 => *c == b'T',
                13 | 16 => *c == b':',
                19 => *c == b'Z',
                _ => c.is_ascii_digit(),
            })
    }
    fn refusal(status: u16, phrase: &str) -> Vec<u8> {
        format!("HTTP/1.1 {status} {phrase}\r\n\r\n").into_bytes()
    }

    /// `outside` running in a directory, its standard error and output piped.
    struct Proxy {
        child: Child,
        stderr: BufReader<ChildStderr>,
        stdout: ChildStdout,
        socket: PathBuf,
        _dir: Dir,
    }
    impl Proxy {
        /// A proxy of `role` with this host's names and the given ranges, resolving from `seam`.
        fn start(role: &str, ranges: &str, seam: &Value) -> Self {
            let dir = Dir::new();
            let socket = dir.join("s");
            Self::at(dir, socket, role, ranges, seam)
        }
        fn at(dir: Dir, socket: PathBuf, role: &str, ranges: &str, seam: &Value) -> Self {
            let file = dir.join("resolve.json");
            fs::write(&file, seam.to_string()).unwrap();
            let mut words = outside(role, &socket, "ab");
            words[5] = ranges.to_owned();
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let mut child = python(&words)
                .env("AP_EGRESS_RESOLVE", &file)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let stderr = BufReader::new(child.stderr.take().unwrap());
            let stdout = child.stdout.take().unwrap();
            let end = Instant::now() + Duration::from_secs(10);
            while !socket.exists() {
                assert!(Instant::now() < end, "the socket did not appear");
                std::thread::sleep(Duration::from_millis(20));
            }
            Self {
                child,
                stderr,
                stdout,
                socket,
                _dir: dir,
            }
        }
        fn connect(&self) -> UnixStream {
            let c = UnixStream::connect(&self.socket).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
            c
        }
        /// Sends the bytes and returns what the proxy sent back until it closed.
        fn ask(&self, request: &[u8]) -> Vec<u8> {
            let mut c = self.connect();
            let _ = c.write_all(request);
            let mut out = vec![];
            let _ = c.read_to_end(&mut out);
            out
        }
        /// The next line of R4: its text, which begins with the NUL byte and the nonce.
        fn line(&mut self) -> String {
            let mut raw = vec![];
            self.stderr.read_until(b'\n', &mut raw).unwrap();
            assert!(raw.ends_with(b"\n"), "{raw:?}");
            String::from_utf8(raw).unwrap()
        }
        fn record(&mut self) -> Value {
            let line = self.line();
            let json = line.strip_prefix("\0ap-egress ab ").unwrap();
            serde_json::from_str(json).unwrap()
        }
        /// Ends it with a signal; its exit status and what it wrote after the lines read.
        fn end(&mut self, name: &str) -> (ExitStatus, Vec<u8>, Vec<u8>) {
            signal(&self.child, name);
            let status = wait_within(&mut self.child, 15).expect("ended within 15 seconds");
            let (mut err, mut out) = (vec![], vec![]);
            self.stderr.read_to_end(&mut err).unwrap();
            self.stdout.read_to_end(&mut out).unwrap();
            (status, err, out)
        }
    }
    impl Drop for Proxy {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// `inside` on a port, its first line read.
    struct Inside {
        child: Child,
        stdout: BufReader<ChildStdout>,
        port: u16,
    }
    impl Inside {
        fn start(socket: &Path) -> Self {
            let port = closed_port();
            let mut child = python(&["inside", &socket.to_string_lossy(), &port.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdout = BufReader::new(child.stdout.take().unwrap());
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            assert_eq!(line, "ready\n");
            Self {
                child,
                stdout,
                port,
            }
        }
        fn connect(&self) -> TcpStream {
            let c = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
            c
        }
    }
    impl Drop for Inside {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn read_n(c: &mut impl Read, n: usize) -> Vec<u8> {
        let mut buf = vec![0; n];
        c.read_exact(&mut buf).unwrap();
        buf
    }

    // spec: specs/ap/pc-egress.md#R1
    #[test]
    fn the_text_is_python_3_7_syntax() {
        let mut child = Command::new("python3")
            .args([
                "-I",
                "-c",
                "import ast, sys; ast.parse(sys.stdin.read(), feature_version=(3, 7))",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(PROGRAM.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // spec: specs/ap/pc-egress.md#R1
    #[test]
    fn an_old_python_writes_one_line_and_exits_3() {
        let old = "MINIMUM = (3, 10)";
        assert_eq!(PROGRAM.matches(old).count(), 1);
        let text = PROGRAM.replace(old, "MINIMUM = (99, 0)");
        let dir = Dir::new();
        let long = dir.join("s").to_string_lossy().into_owned();
        for args in [
            vec!["check"],
            vec!["inside", &long, "8080"],
            vec!["outside", &long, "ab", "reviewer", "", ""],
            vec!["outside", "relative", "", "nobody", "", ""],
            vec!["frob"],
            vec![],
        ] {
            let mut c = Command::new("python3");
            c.args(["-I", "-c", &text]).args(&args);
            let out = finish(c);
            assert_eq!(out.status.code(), Some(3), "{args:?}");
            assert_eq!(
                out.stderr.iter().filter(|b| **b == b'\n').count(),
                1,
                "{args:?}"
            );
            assert!(out.stdout.is_empty() && !dir.join("s").exists());
        }
    }

    // spec: specs/ap/pc-egress.md#R1
    #[test]
    fn each_form_and_argument_is_refused() {
        let dir = Dir::new();
        let s = dir.join("s").to_string_lossy().into_owned();
        let slash = format!("{s}/");
        let long_name = format!("{}/{}", dir.0.display(), "a".repeat(108));
        let host256 = "a".repeat(256);
        let nonce65 = "a".repeat(65);
        for args in [
            vec![],
            vec!["frob"],
            vec!["check", "extra"],
            vec!["outside"],
            vec!["outside", &s, "ab", "reviewer", ""],
            vec!["outside", &s, "ab", "reviewer", "", "", "more"],
            vec!["outside", "rel/s", "ab", "reviewer", "", ""],
            vec!["outside", &slash, "ab", "reviewer", "", ""],
            vec!["outside", &long_name, "ab", "reviewer", "", ""],
            vec!["outside", &s, "", "reviewer", "", ""],
            vec!["outside", &s, &nonce65, "reviewer", "", ""],
            vec!["outside", &s, "AB", "reviewer", "", ""],
            vec!["outside", &s, "xyz", "reviewer", "", ""],
            vec!["outside", &s, "ab", "lead", "", ""],
            vec!["outside", &s, "ab", "Reviewer", "", ""],
            vec!["outside", &s, "ab", "reviewer", "a,,b", ""],
            vec!["outside", &s, "ab", "reviewer", "a b", ""],
            vec!["outside", &s, "ab", "reviewer", &host256, ""],
            vec!["outside", &s, "ab", "reviewer", "", "10.0.0.0"],
            vec!["outside", &s, "ab", "reviewer", "", "10.0.0.0/33"],
            vec!["outside", &s, "ab", "reviewer", "", "10.0.0.1/8"],
            vec!["outside", &s, "ab", "reviewer", "", "zz/8"],
            vec!["outside", &s, "ab", "reviewer", "", "::/129"],
            vec!["outside", &s, "ab", "reviewer", "", "10.0.0.0/8,"],
            vec!["inside", &s],
            vec!["inside", "rel/s", "80"],
            vec!["inside", &s, "0"],
            vec!["inside", &s, "65536"],
            vec!["inside", &s, "x"],
            vec!["inside", &s, ""],
            vec!["inside", &s, "-1"],
            vec!["inside", &s, "80", "more"],
        ] {
            let out = finish(python(&args));
            assert_eq!(out.status.code(), Some(2), "{args:?}");
            assert_eq!(
                out.stderr.iter().filter(|b| **b == b'\n').count(),
                1,
                "{args:?}"
            );
            assert!(out.stdout.is_empty(), "{args:?}");
        }
        assert!(!dir.join("s").exists());
    }

    // spec: specs/ap/pc-egress.md#R1
    #[test]
    fn a_long_socket_is_bound_and_reached_by_its_last_component() {
        let dir = Dir::new();
        let mut path = dir.0.clone();
        for _ in 0..3 {
            path.push("d".repeat(40));
            fs::create_dir(&path).unwrap();
        }
        let socket = path.join("s");
        assert!(socket.as_os_str().len() > 107);
        let proxy = Proxy::at(Dir::new(), socket.clone(), "reviewer", "", &json!({}));
        let inside = Inside::start(&socket);
        let mut c = inside.connect();
        c.write_all(b"CONNECT 1.2.3.4:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut out = vec![];
        let _ = c.read_to_end(&mut out);
        assert_eq!(out, refusal(403, "Forbidden"));
        drop(proxy);
        // The program as a file runs the same.
        let file = concat!(env!("CARGO_MANIFEST_DIR"), "/src/check/egress.py");
        let out = finish({
            let mut c = Command::new("python3");
            c.args(["-I", file, "frob"]);
            c
        });
        assert_eq!(out.status.code(), Some(2));
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn every_status_and_reason_of_the_proxy_is_answered_and_recorded() {
        let seam = json!({
            "api.anthropic.com": ["100.64.0.5"],
            "github.com": ["::ffff:10.0.0.1"],
            "api.github.com": ["1.1.1.1", "[fd7a:115c:a1e0::1]:99"],
            "index.crates.io": [],
        });
        let mut p = Proxy::start("verifier", &RANGES.join(","), &seam);
        let big = format!(
            "CONNECT api.anthropic.com:443 HTTP/1.1\r\nX: {}\r\n\r\n",
            "a".repeat(9_000)
        );
        let cases: Vec<(String, u16, &str, &str, Value, Value)> = vec![
            (
                "GARBAGE\r\n\r\n".into(),
                400,
                "Bad Request",
                "form",
                Value::Null,
                Value::Null,
            ),
            (
                "CONNECT api.anthropic.com:443 HTTP/2\r\n\r\n".into(),
                400,
                "Bad Request",
                "form",
                Value::Null,
                Value::Null,
            ),
            (
                "CONNECT api.anthropic.com:0443 HTTP/1.1\r\n\r\n".into(),
                400,
                "Bad Request",
                "form",
                Value::Null,
                Value::Null,
            ),
            (big, 400, "Bad Request", "form", Value::Null, Value::Null),
            (
                "GET http://x/ HTTP/1.1\r\n\r\n".into(),
                405,
                "Method Not Allowed",
                "method",
                Value::Null,
                Value::Null,
            ),
            (
                "CONNECT api.anthropic.com:80 HTTP/1.1\r\n\r\n".into(),
                403,
                "Forbidden",
                "port",
                json!("api.anthropic.com"),
                json!(80),
            ),
            (
                "CONNECT 1.2.3.4:443 HTTP/1.1\r\n\r\n".into(),
                403,
                "Forbidden",
                "literal",
                json!("1.2.3.4"),
                json!(443),
            ),
            (
                "CONNECT [::1]:443 HTTP/1.0\r\n\r\n".into(),
                403,
                "Forbidden",
                "literal",
                json!("[::1]"),
                json!(443),
            ),
            (
                "CONNECT example.com:443 HTTP/1.1\r\n\r\n".into(),
                403,
                "Forbidden",
                "not_listed",
                json!("example.com"),
                json!(443),
            ),
            (
                "CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n".into(),
                403,
                "Forbidden",
                "address",
                json!("api.anthropic.com"),
                json!(443),
            ),
            (
                "CONNECT github.com:443 HTTP/1.1\r\n\r\n".into(),
                403,
                "Forbidden",
                "address",
                json!("github.com"),
                json!(443),
            ),
            (
                "CONNECT api.github.com:443 HTTP/1.1\r\n\r\n".into(),
                403,
                "Forbidden",
                "address",
                json!("api.github.com"),
                json!(443),
            ),
            (
                "CONNECT index.crates.io:443 HTTP/1.1\r\n\r\n".into(),
                502,
                "Bad Gateway",
                "unresolved",
                json!("index.crates.io"),
                json!(443),
            ),
            (
                "CONNECT static.crates.io:443 HTTP/1.1\r\n\r\n".into(),
                502,
                "Bad Gateway",
                "unresolved",
                json!("static.crates.io"),
                json!(443),
            ),
        ];
        for (request, status, phrase, reason, name, port) in cases {
            let answer = p.ask(request.as_bytes());
            assert_eq!(answer, refusal(status, phrase), "{reason}");
            let r = p.record();
            assert_eq!(r["status"], status, "{reason}");
            assert_eq!(r["reason"], reason);
            assert_eq!((&r["name"], &r["port"]), (&name, &port), "{reason}");
            assert_eq!(r["role"], "verifier");
            assert!(at_format(r["at"].as_str().unwrap()));
        }
        // An incomplete request is a form fault too.
        assert_eq!(p.ask(b"CONNECT api.anth"), refusal(400, "Bad Request"));
        assert_eq!(p.record()["reason"], "form");
        // The model provider alone for a reviewer.
        let mut r = Proxy::start("reviewer", "", &json!({}));
        assert_eq!(
            r.ask(b"CONNECT github.com:443 HTTP/1.1\r\n\r\n"),
            refusal(403, "Forbidden")
        );
        assert_eq!(r.record()["reason"], "not_listed");
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn a_request_not_complete_in_ten_seconds_is_refused() {
        let mut p = Proxy::start("writer", "", &json!({}));
        let started = Instant::now();
        let mut c = p.connect();
        c.write_all(b"CONNECT api.anthro").unwrap();
        let mut out = vec![];
        let _ = c.read_to_end(&mut out);
        assert_eq!(out, refusal(400, "Bad Request"));
        let took = started.elapsed();
        assert!(
            took >= Duration::from_secs(9) && took < Duration::from_secs(15),
            "{took:?}"
        );
        assert_eq!(p.record()["reason"], "form");
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn an_admitted_request_relays_the_bytes_after_its_empty_line_first_and_then_both_ways() {
        let (l, port) = listener();
        let seam = json!({"api.anthropic.com": [format!("127.0.0.1:{port}")]});
        let mut p = Proxy::start("reviewer", "10.0.0.0/8", &seam);
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let first = read_n(&mut s, 5);
            s.write_all(b"world").unwrap();
            let second = read_n(&mut s, 5);
            (first, second)
        });
        let mut c = p.connect();
        c.write_all(b"CONNECT API.Anthropic.COM.:443 HTTP/1.1\r\nHost: x\r\n\r\nhello")
            .unwrap();
        assert_eq!(read_n(&mut c, OK.len()), OK);
        assert_eq!(read_n(&mut c, 5), b"world");
        c.write_all(b"again").unwrap();
        let (first, second) = server.join().unwrap();
        assert_eq!(
            (first.as_slice(), second.as_slice()),
            (&b"hello"[..], &b"again"[..])
        );
        // The server's side ended: so does the client's.
        let mut rest = vec![];
        let _ = c.read_to_end(&mut rest);
        assert!(rest.is_empty());
        let r = p.record();
        assert_eq!(
            (r["status"].as_u64(), &r["reason"]),
            (Some(200), &Value::Null)
        );
        assert_eq!(
            (&r["name"], &r["port"]),
            (&json!("API.Anthropic.COM."), &json!(443))
        );
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn an_unreachable_address_is_502_and_a_refusal_opens_no_connection() {
        let closed = closed_port();
        let seam = json!({"api.anthropic.com": [format!("127.0.0.1:{closed}")]});
        let mut p = Proxy::start("reviewer", "", &seam);
        let answer = p.ask(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n");
        assert_eq!(answer, refusal(502, "Bad Gateway"));
        assert_eq!(p.record()["reason"], "unreachable");

        let (l, port) = listener();
        l.set_nonblocking(true).unwrap();
        let seam = json!({"api.anthropic.com": [format!("127.0.0.1:{port}")]});
        let mut p = Proxy::start("reviewer", "127.0.0.0/8", &seam);
        for request in [
            "CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n",
            "CONNECT api.anthropic.com:80 HTTP/1.1\r\n\r\n",
            "CONNECT example.com:443 HTTP/1.1\r\n\r\n",
            "GET / HTTP/1.1\r\n\r\n",
        ] {
            assert!(p.ask(request.as_bytes()).starts_with(b"HTTP/1.1 4"));
            let _ = p.record();
        }
        let again = l.accept();
        assert_eq!(again.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn the_socket_has_mode_0600_and_a_path_that_exists_refuses() {
        let p = Proxy::start("reviewer", "", &json!({}));
        assert_eq!(
            fs::symlink_metadata(&p.socket)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let words = outside("reviewer", &p.socket, "ab");
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let out = finish(python(&words));
        assert_eq!(out.status.code(), Some(2));
        assert!(p.socket.exists());
        assert_eq!(
            p.ask(b"GET / HTTP/1.1\r\n\r\n"),
            refusal(405, "Method Not Allowed")
        );
        // A plain file in the way refuses too.
        let dir = Dir::new();
        fs::write(dir.join("s"), b"x").unwrap();
        let words = outside("reviewer", &dir.join("s"), "ab");
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(finish(python(&words)).status.code(), Some(2));
        assert_eq!(fs::read(dir.join("s")).unwrap(), b"x");
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn at_most_64_connections_are_served_at_once() {
        let p = Proxy::start("reviewer", "", &json!({}));
        let mut held: Vec<UnixStream> = (0..64).map(|_| p.connect()).collect();
        std::thread::sleep(Duration::from_millis(500));
        let mut extra = p.connect();
        extra.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        extra
            .set_read_timeout(Some(Duration::from_millis(1_500)))
            .unwrap();
        let mut one = [0u8; 1];
        assert!(
            extra.read(&mut one).is_err(),
            "a 65th connection waits its turn"
        );
        drop(held.pop());
        extra
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        let mut out = vec![];
        let _ = extra.read_to_end(&mut out);
        assert_eq!(out, refusal(405, "Method Not Allowed"));
    }

    // spec: specs/ap/pc-egress.md#R2
    #[test]
    fn sigterm_and_sighup_close_every_connection_and_remove_the_socket() {
        for name in ["TERM", "HUP"] {
            let mut p = Proxy::start("reviewer", "", &json!({}));
            let mut held = p.connect();
            std::thread::sleep(Duration::from_millis(300));
            let started = Instant::now();
            let (status, _, _) = p.end(name);
            assert!(started.elapsed() < Duration::from_secs(15));
            assert!(status.success(), "{name}: {status:?}");
            assert!(!p.socket.exists(), "{name}");
            held.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut out = vec![];
            let _ = held.read_to_end(&mut out);
            assert!(UnixStream::connect(&p.socket).is_err());
        }
    }

    // spec: specs/ap/pc-egress.md#R3
    #[test]
    fn inside_passes_each_connection_to_outside_both_ways() {
        let (l, port) = listener();
        let seam = json!({"api.anthropic.com": [format!("127.0.0.1:{port}")]});
        let p = Proxy::start("reviewer", "10.0.0.0/8", &seam);
        let mut inside = Inside::start(&p.socket);
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let first = read_n(&mut s, 5);
            s.write_all(b"world").unwrap();
            let second = read_n(&mut s, 5);
            (first, second)
        });
        let mut c = inside.connect();
        c.write_all(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\nhello")
            .unwrap();
        assert_eq!(read_n(&mut c, OK.len()), OK);
        assert_eq!(read_n(&mut c, 5), b"world");
        c.write_all(b"again").unwrap();
        let (first, second) = server.join().unwrap();
        assert_eq!(
            (first.as_slice(), second.as_slice()),
            (&b"hello"[..], &b"again"[..])
        );
        // A refusal passes through the same way.
        let mut c = inside.connect();
        c.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        let mut out = vec![];
        let _ = c.read_to_end(&mut out);
        assert_eq!(out, refusal(405, "Method Not Allowed"));
        // It ends at SIGTERM and has written `ready` and nothing after.
        signal(&inside.child, "TERM");
        assert!(wait_within(&mut inside.child, 15).is_some());
        let mut rest = vec![];
        inside.stdout.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty());
    }

    // spec: specs/ap/pc-egress.md#R3
    #[test]
    fn inside_closes_a_connection_whose_socket_cannot_be_reached() {
        let dir = Dir::new();
        let inside = Inside::start(&dir.join("absent"));
        let mut c = inside.connect();
        let _ = c.write_all(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n");
        let mut out = vec![];
        let _ = c.read_to_end(&mut out);
        assert!(out.is_empty());
    }

    // spec: specs/ap/pc-egress.md#R4
    #[test]
    fn each_request_leaves_one_line_of_a_nul_byte_the_nonce_and_six_members() {
        let mut p = Proxy::start("verifier", &RANGES.join(","), &json!({}));
        for request in [
            "GET / HTTP/1.1\r\n\r\n",
            "CONNECT api.github.com:80 HTTP/1.1\r\n\r\n",
            "CONNECT 10.1.2.3:443 HTTP/1.1\r\n\r\n",
            "nonsense\r\n\r\n",
        ] {
            let _ = p.ask(request.as_bytes());
        }
        let (status, err, out) = p.end("TERM");
        assert!(status.success() && out.is_empty());
        let mut count = 0;
        for raw in err.split_inclusive(|b| *b == b'\n') {
            let text = std::str::from_utf8(raw).unwrap();
            let json = text
                .strip_prefix("\0ap-egress ab ")
                .and_then(|t| t.strip_suffix('\n'))
                .unwrap_or_else(|| panic!("not a line of R4: {text:?}"));
            // The members, in the order of R19 without `run`.
            let places: Vec<usize> = ["at", "role", "name", "port", "status", "reason"]
                .iter()
                .map(|m| json.find(&format!("\"{m}\":")).unwrap())
                .collect();
            assert!(
                places.windows(2).all(|w| w[0] < w[1]) && places[0] == 1,
                "{json}"
            );
            let v: Value = serde_json::from_str(json).unwrap();
            assert_eq!(v.as_object().unwrap().len(), 6);
            assert!(at_format(v["at"].as_str().unwrap()));
            count += 1;
        }
        assert_eq!(count, 4);
    }

    // spec: specs/ap/pc-egress.md#R5
    #[test]
    fn check_answers_ready_and_leaves_nothing_in_tmp() {
        let _lock = CHECKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = check_leftovers();
        let out = finish(python(&["check"]));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.starts_with("ap-egress ready ") && text.ends_with('\n'));
        assert_eq!(text.matches('\n').count(), 1);
        assert_eq!(check_leftovers(), before);
        // The same program run as a file.
        let file = concat!(env!("CARGO_MANIFEST_DIR"), "/src/check/egress.py");
        let out = finish({
            let mut c = Command::new("python3");
            c.args(["-I", file, "check"]);
            c
        });
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("ap-egress ready "));
        assert_eq!(check_leftovers(), before);
    }
    fn check_leftovers() -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir("/tmp")
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("ap-egress-check-"))
            .collect();
        found.sort();
        found
    }

    // spec: specs/ap/pc-egress.md#R5
    #[test]
    fn check_does_not_answer_ready_when_the_judgment_admits_every_request() {
        let _lock = CHECKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The substitution: the program's one `listed` function admits every name.
        let judgment = "    return name in names";
        assert_eq!(PROGRAM.matches(judgment).count(), 1);
        let patched = PROGRAM.replace(judgment, "    return True");
        let (_l, port) = listener();
        let dir = Dir::new();
        let seam = dir.join("resolve.json");
        fs::write(
            &seam,
            json!({"ap-egress-check.invalid": [format!("127.0.0.1:{port}")]}).to_string(),
        )
        .unwrap();
        let mut c = Command::new("python3");
        c.args(["-I", "-c", &patched, "check"])
            .env("AP_EGRESS_RESOLVE", &seam);
        let out = finish(c);
        assert_eq!(out.status.code(), Some(1));
        assert!(!String::from_utf8_lossy(&out.stdout).contains("ap-egress ready"));
        assert!(!dir.join("s").exists());
    }

    // spec: specs/ap/pc-egress.md#R6
    #[test]
    fn the_words_of_outside_are_built_from_the_constants_of_this_hosts_proxy() {
        for role in ["implementer", "fixer", "reviewer", "verifier", "writer"] {
            let words = outside(role, Path::new("/tmp/x/s"), "ab-12");
            let (listed, ranges) = (names(role).join(","), RANGES.join(","));
            let expected: Vec<&str> = vec!["outside", "/tmp/x/s", "ab-12", role, &listed, &ranges];
            assert_eq!(words, expected, "{role}");
            assert_eq!(words[5].split(',').count(), 13);
        }
        assert_eq!(
            outside("verifier", Path::new("/s"), "a")[4]
                .split(',')
                .count(),
            5
        );
        assert_eq!(
            outside("writer", Path::new("/s"), "a")[4],
            "api.anthropic.com"
        );
        // No range is written in the program, and no name but the one its self-check sends (R5).
        for range in RANGES {
            assert!(!PROGRAM.contains(range), "{range}");
        }
        for name in names("verifier") {
            let held = usize::from(*name == "api.anthropic.com");
            assert_eq!(PROGRAM.matches(name).count(), held, "{name}");
        }
    }

    struct Env(Vec<(String, String)>);
    impl System for Env {
        fn call(&mut self, program: &str, _: &[String], _: &Value, _: u64) -> Answer {
            Err(error("process_failed", program))
        }
        fn environment(&self) -> Vec<(String, String)> {
            self.0.clone()
        }
    }
    fn env(log: &Path) -> Env {
        Env(vec![(
            "AP_EGRESS_LOG".to_owned(),
            log.to_string_lossy().into_owned(),
        )])
    }

    // spec: specs/ap/pc-egress.md#R6
    #[test]
    fn take_moves_only_its_nonces_lines_and_keeps_every_other_byte() {
        let dir = Dir::new();
        let log = dir.join("egress.jsonl");
        let errors = dir.join("err");
        let a = r#"{"at":"2026-11-23T07:00:00Z","role":"reviewer","name":"api.anthropic.com","port":443,"status":200,"reason":null}"#;
        let b = r#"{"at":"2026-11-23T07:00:05Z","role":"reviewer","name":"example.com","port":443,"status":403,"reason":"not_listed"}"#;
        let forged = r#"{"at":"2026-11-23T07:00:06Z","role":"reviewer","name":null,"port":null,"status":400,"reason":"form","run":"x"}"#;
        // Each piece of the file, and whether `take` takes it for the nonce `ab`.
        let pieces: Vec<(Vec<u8>, bool)> = vec![
            (b"stderr noise\n".to_vec(), false),
            (format!("\0ap-egress ab {a}\n").into_bytes(), true),
            (format!("\0ap-egress cd {b}\n").into_bytes(), false),
            (b"binary \xff\xfe\x00 tail\n".to_vec(), false),
            (format!("\0ap-egress ab {b}\n").into_bytes(), true),
            (format!("\0ap-egress ab {forged}\n").into_bytes(), false),
            (b"\0ap-egress ab {not json}\n".to_vec(), false),
            (format!("x \0ap-egress ab {a}\n").into_bytes(), false),
            (format!("\0ap-egress abc {a}\n").into_bytes(), false),
            (format!("\0ap-egress ab {a}").into_bytes(), false),
        ];
        let join = |keep: &dyn Fn(bool) -> bool| -> Vec<u8> {
            pieces
                .iter()
                .filter(|(_, taken)| keep(*taken))
                .flat_map(|(p, _)| p.clone())
                .collect()
        };
        fs::write(&errors, join(&|_| true)).unwrap();
        // An existing log keeps its content and its mode.
        fs::write(&log, "old\n").unwrap();
        fs::set_permissions(&log, fs::Permissions::from_mode(0o644)).unwrap();

        let run = "/work/x/7.run.json";
        assert_eq!(take(&env(&log), &errors, "ab", run).unwrap(), 2);
        let text = fs::read_to_string(&log).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("old"));
        let got: Vec<Value> = lines.map(|l| serde_json::from_str(l).unwrap()).collect();
        let mut want_a: Value = serde_json::from_str(a).unwrap();
        let mut want_b: Value = serde_json::from_str(b).unwrap();
        want_a["run"] = json!(run);
        want_b["run"] = json!(run);
        assert_eq!(got, [want_a, want_b]);
        assert!(
            got.iter()
                .all(|v| v.as_object().is_some_and(|o| o.len() == 7))
        );
        assert_eq!(
            fs::metadata(&log).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(fs::read(&errors).unwrap(), join(&|taken| !taken));

        // Nothing of the nonce: the file is not written and no log is made; a made log has mode 0600.
        let before = fs::read(&errors).unwrap();
        let fresh = dir.join("fresh.jsonl");
        assert_eq!(take(&env(&fresh), &errors, "ee", run).unwrap(), 0);
        assert_eq!(fs::read(&errors).unwrap(), before);
        assert!(!fresh.exists());
        fs::write(&errors, format!("\0ap-egress ee {a}\n")).unwrap();
        assert_eq!(take(&env(&fresh), &errors, "ee", run).unwrap(), 1);
        assert_eq!(
            fs::metadata(&fresh).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(fs::read(&errors).unwrap().is_empty());
        // A file that cannot be read is an error.
        assert!(take(&env(&fresh), &dir.join("absent"), "ee", run).is_err());
    }

    // spec: specs/ap/pc-egress.md#R6
    #[test]
    fn the_log_is_the_variable_when_set_and_not_empty_else_the_default() {
        assert_eq!(log_path(&Env(vec![])), LOG_DEFAULT);
        assert_eq!(
            log_path(&Env(vec![("AP_EGRESS_LOG".into(), String::new())])),
            LOG_DEFAULT
        );
        assert_eq!(log_path(&env(Path::new("/tmp/l"))), "/tmp/l");
    }

    // spec: specs/ap/pc-egress.md#R7
    #[test]
    fn python3_of_this_account_is_3_10_or_later() {
        let _lock = CHECKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let out = finish(python(&["check"]));
        let text = String::from_utf8(out.stdout).unwrap();
        let version = text.strip_prefix("ap-egress ready ").unwrap().trim_end();
        let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap()).collect();
        assert!(
            parts.len() == 3 && (parts[0], parts[1]) >= (3, 10),
            "{version}"
        );
    }

    // spec: specs/ap/pc-egress.md#R8
    #[test]
    fn the_seam_resolves_in_place_of_the_system_and_names_the_port() {
        let (l, port) = listener();
        let seam = json!({
            "api.anthropic.com": [format!("127.0.0.1:{port}")],
            "github.com": ["[fe80::1]:80"],
            "api.github.com": "not a list",
        });
        let mut p = Proxy::start("verifier", "10.0.0.0/8,fe80::/10", &seam);
        // The entry's port is the one connected to; a request for any but 443 is refused first.
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            read_n(&mut s, 2)
        });
        let mut c = p.connect();
        c.write_all(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\nhi")
            .unwrap();
        assert_eq!(read_n(&mut c, OK.len()), OK);
        assert_eq!(server.join().unwrap(), b"hi");
        assert_eq!(p.record()["status"], 200);
        assert_eq!(
            p.ask(b"CONNECT api.anthropic.com:8080 HTTP/1.1\r\n\r\n"),
            refusal(403, "Forbidden")
        );
        assert_eq!(p.record()["reason"], "port");
        // A bracketed IPv6 entry with a port is read and judged by the ranges.
        assert_eq!(
            p.ask(b"CONNECT github.com:443 HTTP/1.1\r\n\r\n"),
            refusal(403, "Forbidden")
        );
        assert_eq!(p.record()["reason"], "address");
        // A name the seam holds badly, or not at all, has no address.
        assert_eq!(
            p.ask(b"CONNECT api.github.com:443 HTTP/1.1\r\n\r\n"),
            refusal(502, "Bad Gateway")
        );
        assert_eq!(p.record()["reason"], "unresolved");
        // A seam file that cannot be read resolves nothing.
        let dir = Dir::new();
        let mut words = outside("reviewer", &dir.join("s"), "ab");
        words[5] = String::new();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut child = python(&words)
            .env("AP_EGRESS_RESOLVE", dir.join("absent.json"))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let socket = dir.join("s");
        let end = Instant::now() + Duration::from_secs(10);
        while !socket.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut c = UnixStream::connect(&socket).unwrap();
        c.write_all(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut out = vec![];
        let _ = c.read_to_end(&mut out);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(out, refusal(502, "Bad Gateway"));
    }
}
