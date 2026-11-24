//! Remote HTTP/1.1 client for `ont serve` (`specs/ap/client.md`, `specs/ap/policy.md` R19):
//! tailnet `store_url` validation, remote-mode precedence resolution, zero-dependency HTTP/1.1
//! transport over `host::WayOut::connect`, and VPS-only driver verb refusal.
use crate::host::{
    ANSWER_LIMIT, Answer, Failure, Host, LIMIT, Network, Reply, System, WayOut, error, fail,
};
use serde_json::{Map, Value, json};
use std::{
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

/// Maximum HTTP/1.1 response header block size including the terminating `\r\n\r\n` (R8).
const MAX_HEADER_BYTES: usize = 16_384;

/// A validated tailnet `ont serve` HTTP endpoint (`http://<tailnet-ip>:<port>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub addr: SocketAddr,
    pub host: String,
}

/// Validates a strict tailnet `store_url` (`specs/ap/policy.md` R19, `specs/ap/client.md` R4):
/// `http://<tailnet-ip>:<port>` with IPv4 in `100.64.0.0/10` or bracketed IPv6 in
/// `[fd7a:115c:a1e0::/48]` and port `1..=65535`, with no DNS hostname, userinfo, path, query,
/// fragment, whitespace, or control character.
pub fn parse_store_url(raw: &str) -> Option<Endpoint> {
    if raw.bytes().any(|b| !(0x21..0x7f).contains(&b)) {
        return None;
    }
    let authority = raw.strip_prefix("http://")?;
    if authority.is_empty()
        || authority
            .bytes()
            .any(|b| matches!(b, b'@' | b'/' | b'?' | b'#'))
    {
        return None;
    }
    let (ip, port_str) = if let Some(rest) = authority.strip_prefix('[') {
        let (addr_str, after) = rest.split_once(']')?;
        let port_str = after.strip_prefix(':')?;
        let v6: Ipv6Addr = addr_str.parse().ok()?;
        let seg = v6.segments();
        if seg[0] != 0xfd7a || seg[1] != 0x115c || seg[2] != 0xa1e0 {
            return None;
        }
        (IpAddr::V6(v6), port_str)
    } else {
        let (addr_str, port_str) = authority.split_once(':')?;
        let v4: Ipv4Addr = addr_str.parse().ok()?;
        let o = v4.octets();
        if o[0] != 100 || (o[1] & 0xc0) != 0x40 {
            return None;
        }
        (IpAddr::V4(v4), port_str)
    };
    if port_str.is_empty()
        || port_str.starts_with('0')
        || !port_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let port: u16 = port_str.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(Endpoint {
        addr: SocketAddr::new(ip, port),
        host: authority.to_owned(),
    })
}

fn policy_invalid(detail: &str) -> Failure {
    fail("policy_invalid", "policy", json!(detail))
}

/// Validates the optional top-level `store_url` member of a policy JSON object
/// (`specs/ap/policy.md` R19, reader `client` of `ap policy check`).
pub fn config(policy: &Value) -> Result<Option<Endpoint>, Failure> {
    let Some(val) = policy.get("store_url") else {
        return Ok(None);
    };
    let Some(raw) = val.as_str() else {
        return Err(policy_invalid("store_url is not a string"));
    };
    parse_store_url(raw)
        .map(Some)
        .ok_or_else(|| policy_invalid("store_url is not http://<tailnet-ip>:<port>"))
}

/// Resolves remote mode for a remote-enabled client verb or VPS-only driver guard
/// (`specs/ap/client.md` R1 to R3):
/// 1. Explicit `--store <path>` always selects local mode without reading env or policy.
/// 2. Non-empty `AP_STORE_URL` (via `System::store_url_env`) validates strictly or fails
///    `store_url_invalid`.
/// 3. Policy file (via `System::client_policy_file`) selects remote mode if `store_url` is
///    present and valid, local mode if the file or member is absent, or fails `policy_invalid`.
pub fn resolve(s: &impl System, store: Option<&str>) -> Result<Option<Endpoint>, Failure> {
    if store.is_some() {
        return Ok(None);
    }
    if let Some(raw) = s.store_url_env()
        && !raw.is_empty()
    {
        let Some(text) = raw.to_str() else {
            return Err(fail(
                "store_url_invalid",
                "store_url",
                json!("AP_STORE_URL is not valid UTF-8"),
            ));
        };
        return parse_store_url(text).map(Some).ok_or_else(|| {
            fail(
                "store_url_invalid",
                "store_url",
                json!("AP_STORE_URL is not http://<tailnet-ip>:<port>"),
            )
        });
    }
    let Some(file) = s.client_policy_file() else {
        return Ok(None);
    };
    let Some(path) = file.to_str() else {
        if std::fs::metadata(&file).is_err_and(|e| e.kind() == ErrorKind::NotFound) {
            return Ok(None);
        }
        return Err(policy_invalid("file path is not valid UTF-8"));
    };
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(policy_invalid("file is unreadable")),
    };
    if bytes.len() as u64 > LIMIT {
        return Err(policy_invalid("file is too large"));
    }
    let v: Value =
        serde_json::from_slice(&bytes).map_err(|_| policy_invalid("file is not JSON"))?;
    if !v.is_object() {
        return Err(policy_invalid("file is not one JSON object"));
    }
    config(&v)
}

fn header_value_safe(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

fn env_header(env: &[(String, String)], name: &'static str) -> Result<Option<String>, Failure> {
    let Some((_, val)) = env.iter().rev().find(|(k, _)| k == name) else {
        return Ok(None);
    };
    if val.is_empty() {
        return Ok(None);
    }
    if !header_value_safe(val) {
        return Err(error("invalid_input", name));
    }
    Ok(Some(val.clone()))
}

struct Mapped {
    path: &'static str,
    body: Value,
    client: String,
    cmd_name: &'static str,
}

fn parse_client_flag(rest: &[&str], subcmd: &str) -> Result<String, Failure> {
    match rest {
        [] => Ok("ap".to_owned()),
        ["--client", val] => {
            if val.is_empty() || !header_value_safe(val) {
                return Err(error("invalid_input", "client"));
            }
            Ok((*val).to_owned())
        }
        ["--client"] => Err(error("invalid_input", "client")),
        _ => Err(fail("operation_not_served", "command", json!(subcmd))),
    }
}

fn map_command(args: &[String], body: &Value) -> Result<Mapped, Failure> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let not_served = |sub: &str| fail("operation_not_served", "command", json!(sub));
    match words.as_slice() {
        ["describe"] => Ok(Mapped {
            path: "/v1/describe",
            body: if body.is_null() {
                json!({})
            } else {
                body.clone()
            },
            client: "ap".to_owned(),
            cmd_name: "describe",
        }),
        ["get"] => Ok(Mapped {
            path: "/v1/get",
            body: body.clone(),
            client: "ap".to_owned(),
            cmd_name: "get",
        }),
        ["search"] => Ok(Mapped {
            path: "/v1/search",
            body: body.clone(),
            client: "ap".to_owned(),
            cmd_name: "search",
        }),
        ["around"] => Ok(Mapped {
            path: "/v1/around",
            body: body.clone(),
            client: "ap".to_owned(),
            cmd_name: "around",
        }),
        ["aggregate"] => Ok(Mapped {
            path: "/v1/aggregate",
            body: body.clone(),
            client: "ap".to_owned(),
            cmd_name: "aggregate",
        }),
        ["log"] => Ok(Mapped {
            path: "/v1/log",
            body: body.clone(),
            client: "ap".to_owned(),
            cmd_name: "log",
        }),
        ["render"] => Ok(Mapped {
            path: "/v1/render",
            body: body.clone(),
            client: "ap".to_owned(),
            cmd_name: "render",
        }),
        ["action", "list"] => Ok(Mapped {
            path: "/v1/action/list",
            body: if body.is_null() {
                json!({})
            } else {
                body.clone()
            },
            client: "ap".to_owned(),
            cmd_name: "action",
        }),
        ["action", "validate", rest @ ..] => Ok(Mapped {
            path: "/v1/action/validate",
            body: body.clone(),
            client: parse_client_flag(rest, "action")?,
            cmd_name: "action",
        }),
        ["action", "apply", rest @ ..] => Ok(Mapped {
            path: "/v1/action/apply",
            body: body.clone(),
            client: parse_client_flag(rest, "action")?,
            cmd_name: "action",
        }),
        ["fn", name] if !name.is_empty() => {
            let mut obj = match body {
                Value::Null => Map::new(),
                Value::Object(m) => m.clone(),
                _ => return Err(error("invalid_input", "body")),
            };
            obj.insert("name".to_owned(), json!(name));
            Ok(Mapped {
                path: "/v1/fn",
                body: Value::Object(obj),
                client: "ap".to_owned(),
                cmd_name: "fn",
            })
        }
        [first, ..] => Err(not_served(first)),
        [] => Err(not_served("")),
    }
}

fn unreachable(detail: &'static str) -> Failure {
    fail("store_unreachable", "store_url", json!(detail))
}

fn write_all_deadline(
    stream: &mut TcpStream,
    mut buf: &[u8],
    deadline: Instant,
) -> Result<(), Failure> {
    while !buf.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(unreachable("timeout"));
        }
        let _ = stream.set_write_timeout(Some(remaining));
        match stream.write(buf) {
            Ok(0) => return Err(unreachable("io")),
            Ok(n) => buf = &buf[n..],
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                return Err(unreachable("timeout"));
            }
            Err(_) => return Err(unreachable("io")),
        }
    }
    Ok(())
}

fn read_chunk(stream: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> Result<usize, Failure> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(unreachable("timeout"));
        }
        let _ = stream.set_read_timeout(Some(remaining));
        match stream.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                return Err(unreachable("timeout"));
            }
            Err(_) => return Err(unreachable("io")),
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn has_bare_cr_or_lf(bytes: &[u8]) -> bool {
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return true;
        }
        if b == b'\n' && (i == 0 || bytes[i - 1] != b'\r') {
            return true;
        }
    }
    false
}

fn token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn parse_status_line(line: &str) -> Option<u16> {
    let rest = line
        .strip_prefix("HTTP/1.1 ")
        .or_else(|| line.strip_prefix("HTTP/1.0 "))?;
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || !bytes[..3].iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 3 && bytes[3] != b' ' {
        return None;
    }
    if bytes.get(4..).is_some_and(|reason| {
        reason
            .iter()
            .any(|&b| b != b'\t' && !(0x20..=0x7e).contains(&b))
    }) {
        return None;
    }
    rest[..3].parse().ok()
}

fn parse_headers(header_bytes: &[u8]) -> Option<(u16, Option<u64>)> {
    if has_bare_cr_or_lf(header_bytes) {
        return None;
    }
    let text = std::str::from_utf8(header_bytes).ok()?;
    let mut lines = text.split("\r\n");
    let status = parse_status_line(lines.next()?)?;
    let mut content_length: Option<u64> = None;
    for line in lines {
        let (name, raw_val) = line.split_once(':')?;
        if name.is_empty() || !name.bytes().all(token_byte) {
            return None;
        }
        if raw_val
            .bytes()
            .any(|b| b != b'\t' && !(0x20..=0x7e).contains(&b))
        {
            return None;
        }
        let val = raw_val.trim_matches(|c| c == ' ' || c == '\t');
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return None;
        }
        if name.eq_ignore_ascii_case("content-length") {
            if val.is_empty() || !val.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let len: u64 = val.parse().ok()?;
            match content_length {
                None => content_length = Some(len),
                Some(prev) if prev == len => {}
                Some(_) => return None,
            }
        }
    }
    Some((status, content_length))
}

/// Sends one `ont` command over HTTP/1.1 to `endpoint` via `s.way_out()` (`specs/ap/client.md`
/// R5 to R9).
pub fn call_http(
    s: &impl System,
    endpoint: &Endpoint,
    args: &[String],
    body: &Value,
    ms: u64,
) -> Answer {
    let mapped = map_command(args, body)?;
    let env = s.environment();
    let role_hdr = env_header(&env, "ONT_ROLE")?;
    let agent_hdr = env_header(&env, "ONT_AGENT")?;
    let unreadable = || error("store_answer_unreadable", mapped.cmd_name);
    let body_bytes = serde_json::to_vec(&mapped.body).map_err(|_| unreadable())?;

    let mut req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Ont-Client: {}\r\n",
        mapped.path,
        endpoint.host,
        body_bytes.len(),
        mapped.client
    );
    if let Some(role) = &role_hdr {
        req.push_str("X-Ont-Role: ");
        req.push_str(role);
        req.push_str("\r\n");
    }
    if let Some(agent) = &agent_hdr {
        req.push_str("X-Ont-Agent: ");
        req.push_str(agent);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");
    let mut req_bytes = req.into_bytes();
    req_bytes.extend_from_slice(&body_bytes);

    let deadline = Instant::now() + Duration::from_millis(ms);
    let connect_timeout = deadline.saturating_duration_since(Instant::now());
    if connect_timeout.is_zero() {
        return Err(unreachable("timeout"));
    }
    let way = s.way_out();
    let mut stream = way
        .connect(endpoint.addr, connect_timeout)
        .map_err(|e| match e.kind() {
            ErrorKind::TimedOut | ErrorKind::WouldBlock => unreachable("timeout"),
            _ => unreachable("connect"),
        })?;

    write_all_deadline(&mut stream, &req_bytes, deadline)?;

    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            if pos + 4 > MAX_HEADER_BYTES {
                return Err(unreadable());
            }
            break pos;
        }
        if buf.len() >= MAX_HEADER_BYTES {
            return Err(unreadable());
        }
        let n = read_chunk(&mut stream, &mut chunk, deadline)?;
        if n == 0 {
            return Err(if buf.is_empty() {
                unreachable("io")
            } else {
                unreadable()
            });
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let (status, content_length) = parse_headers(&buf[..header_end]).ok_or_else(unreadable)?;
    let mut resp_body = buf[header_end + 4..].to_vec();
    match content_length {
        Some(expected_u64) => {
            if expected_u64 > ANSWER_LIMIT {
                return Err(unreadable());
            }
            let expected = expected_u64 as usize;
            if resp_body.len() > expected {
                return Err(unreadable());
            }
            while resp_body.len() < expected {
                let want = (expected - resp_body.len()).min(chunk.len());
                let n = read_chunk(&mut stream, &mut chunk[..want], deadline)?;
                if n == 0 {
                    return Err(unreadable());
                }
                resp_body.extend_from_slice(&chunk[..n]);
            }
        }
        None => {
            if resp_body.len() as u64 > ANSWER_LIMIT {
                return Err(unreadable());
            }
            loop {
                let n = read_chunk(&mut stream, &mut chunk, deadline)?;
                if n == 0 {
                    break;
                }
                if (resp_body.len() + n) as u64 > ANSWER_LIMIT {
                    return Err(unreadable());
                }
                resp_body.extend_from_slice(&chunk[..n]);
            }
        }
    }

    match status {
        200 => Ok(Reply {
            success: true,
            stdout: resp_body.clone(),
            bytes: resp_body,
        }),
        400 | 403 | 404 | 405 | 413 | 431 | 503 => Ok(Reply {
            success: false,
            stdout: resp_body.clone(),
            bytes: resp_body,
        }),
        _ => Err(unreadable()),
    }
}

/// A `System` adapter that routes `"ont"` calls over HTTP/1.1 to `endpoint` and delegates every
/// other port method to the wrapped `System`.
pub struct Transport<'a, S: System> {
    inner: &'a mut S,
    endpoint: Endpoint,
}

impl<'a, S: System> Transport<'a, S> {
    pub fn new(inner: &'a mut S, endpoint: Endpoint) -> Self {
        Self { inner, endpoint }
    }
}

impl<S: System> System for Transport<'_, S> {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        if program == "ont" {
            call_http(self.inner, &self.endpoint, args, input, ms)
        } else {
            self.inner.call(program, args, input, ms)
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.inner.read(path)
    }
    fn work_root(&self) -> PathBuf {
        self.inner.work_root()
    }
    fn home(&self) -> Option<PathBuf> {
        self.inner.home()
    }
    fn cwd(&self) -> std::io::Result<PathBuf> {
        self.inner.cwd()
    }
    fn directory(&self, path: &str) -> std::io::Result<PathBuf> {
        self.inner.directory(path)
    }
    fn now(&self) -> Instant {
        self.inner.now()
    }
    fn wall(&self) -> SystemTime {
        self.inner.wall()
    }
    fn pause(&mut self, duration: Duration) {
        self.inner.pause(duration);
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<std::process::Child> {
        self.inner.launch(argv, env, cwd, files)
    }
    fn units(&self) -> bool {
        self.inner.units()
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.inner.policy_file()
    }
    fn git_hooks(&self) -> Option<PathBuf> {
        self.inner.git_hooks()
    }
    fn examine_key(&self, path: &str) -> std::io::Result<crate::api::Examined> {
        self.inner.examine_key(path)
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.inner.environment()
    }
    fn network(&mut self) -> std::io::Result<Network> {
        self.inner.network()
    }
    fn way_out(&self) -> Arc<dyn WayOut> {
        self.inner.way_out()
    }
    fn store_url_env(&self) -> Option<OsString> {
        self.inner.store_url_env()
    }
    fn client_policy_file(&self) -> Option<PathBuf> {
        self.inner.client_policy_file()
    }
}

/// Identifies a VPS-only driver invocation that must be refused in remote mode
/// (`specs/ap/client.md` R12, R13). Returns `None` for `--help`, `-h`, `version`, `--version`,
/// `policy check`, non-driver verbs, and any invocation containing `--store`.
fn driver_verb(args: &[OsString]) -> Option<&'static str> {
    if args
        .iter()
        .any(|a| a == "--store" || a == "--help" || a == "-h")
    {
        return None;
    }
    match args.first().and_then(|a| a.to_str()) {
        Some("deliver") => Some("deliver"),
        Some("work") => Some("work"),
        Some("unit") => match args.get(1).and_then(|a| a.to_str()) {
            Some("ready") => Some("unit ready"),
            Some("run") => Some("unit run"),
            Some("review") => Some("unit review"),
            Some("write") => Some("unit write"),
            _ => None,
        },
        _ => None,
    }
}

/// Checks whether `args` invokes a VPS-only driver verb under remote mode (`specs/ap/client.md`
/// R12, R13).
pub fn guard_driver(s: &impl System, args: &[OsString]) -> Result<(), Failure> {
    let Some(verb) = driver_verb(args) else {
        return Ok(());
    };
    match resolve(s, None)? {
        Some(_) => Err(fail("remote_mode_unsupported", "verb", json!(verb))),
        None => Ok(()),
    }
}

/// CLI entrypoint of the VPS-only driver guard called from `main.rs` (`specs/ap/client.md` R12,
/// R13): returns `None` to continue normal dispatch, or `Some(ExitCode)` when refused.
pub fn guard_cli(args: &[OsString]) -> Option<ExitCode> {
    let Err(e) = guard_driver(&Host, args) else {
        return None;
    };
    let line = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}});
    let code = if writeln!(std::io::stderr(), "{line}").is_ok() {
        2
    } else {
        4
    };
    Some(ExitCode::from(code))
}
