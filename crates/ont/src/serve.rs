//! `ont serve`: HTTP/1.1 tailnet server offering reads, actions, functions, and
//! MCP over HTTP (`specs/ont/serve.md`).
//!
//! Each accepted TCP peer is authenticated by its kernel `peer_addr()` via
//! Tailscale LocalAPI whois over Unix domain socket (`TailscaleWhois`), giving
//! a `ts:<tag>` or `ts:<login>` principal and tailnet node host. No header,
//! body field, or server environment variable (`ONT_ROLE`, `ONT_AGENT`) can
//! forge or inject identity or role.
use crate::action::{
    ApplyActionRequest, ListActionsRequest, ValidateActionRequest, apply_action,
    invalid_actor_field, list_actions, valid_actor_field, validate_action,
};
use crate::cli::{Op, execute, now_seconds, render, take_role};
use crate::mcp::{self, MAX_LINE as MCP_MAX_LINE, RemoteCall};
use crate::principal::{EngineIdentity, principal_form};
use crate::{Context, Failure, MAX_REQUEST, Refusal};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Default Tailscale LocalAPI Unix domain socket path.
pub const DEFAULT_TAILSCALE_SOCKET: &str = "/run/tailscale/tailscaled.sock";
/// Maximum incoming HTTP header block size in bytes (including `\r\n\r\n`).
pub const MAX_HEADER_BYTES: usize = 16_384;
/// Maximum concurrent in-flight HTTP connections.
pub const MAX_CONCURRENT_CONNECTIONS: usize = 16;
/// Maximum Tailscale LocalAPI whois HTTP response size in bytes.
pub const WHOIS_MAX_BYTES: usize = 65_536;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
const WHOIS_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_POLL_NS: i64 = 50_000_000;

/// Configuration for [`serve_listener`].
#[derive(Debug, Clone)]
pub struct Config {
    pub store: PathBuf,
    /// Fixed clock override (`--now`); `None` reads the wall clock per request.
    pub now: Option<u64>,
    /// Default client name (`--client`); empty or `"cli"` maps to `"remote"`.
    pub client: String,
}

/// Resolves an accepted TCP peer's kernel socket address into an authenticated
/// [`EngineIdentity`].
pub trait PeerIdentityResolver: Send + Sync {
    fn resolve(&self, peer: SocketAddr) -> Result<EngineIdentity, String>;
}

/// Production [`PeerIdentityResolver`] backed by Tailscale LocalAPI
/// `/localapi/v0/whois` over a Unix domain socket.
#[derive(Debug, Clone)]
pub struct TailscaleWhois {
    socket_path: PathBuf,
}

impl TailscaleWhois {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

/// Whether `ip` lies in Tailscale's CGNAT IPv4 range (`100.64.0.0/10`) or ULA
/// IPv6 range (`fd7a:115c:a1e0::/48`).
pub fn is_tailnet_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            octets[0] == 100 && (octets[1] & 0xc0) == 0x40
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            segments[0] == 0xfd7a && segments[1] == 0x115c && segments[2] == 0xa1e0
        }
    }
}

fn valid_host(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn percent_encode_query_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 12);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn find_crlf_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|w| w == b"\r\n")
}

fn parse_whois_response_headers(header_bytes: &[u8]) -> Result<(bool, Option<usize>), String> {
    let text = std::str::from_utf8(header_bytes)
        .map_err(|_| "whois headers are not valid UTF-8".to_string())?;
    let mut lines = text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| "missing whois status line".to_string())?;
    let mut status_parts = status_line.split_whitespace();
    let version = status_parts
        .next()
        .ok_or_else(|| "missing whois HTTP version".to_string())?;
    let status_code = status_parts
        .next()
        .ok_or_else(|| "missing whois status code".to_string())?;
    if !(version == "HTTP/1.0" || version == "HTTP/1.1") || status_code != "200" {
        return Err("whois status not 200".to_string());
    }
    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err("malformed whois header".to_string());
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            if value
                .split(',')
                .any(|tok| tok.trim().eq_ignore_ascii_case("chunked"))
            {
                is_chunked = true;
            }
        } else if name.eq_ignore_ascii_case("content-length") {
            if !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err("invalid whois content-length".to_string());
            }
            let cl = value
                .parse::<usize>()
                .map_err(|_| "invalid whois content-length".to_string())?;
            if content_length.replace(cl).is_some() {
                return Err("duplicate whois content-length".to_string());
            }
        }
    }
    Ok((is_chunked, content_length))
}

/// Decode an HTTP/1.1 chunked body within `max_bytes`.
/// Returns `Ok(Some(bytes))` once the terminating `0` chunk and trailing CRLF
/// have been parsed, `Ok(None)` when more input bytes are needed, or `Err` on
/// malformed framing or size overflow.
fn decode_chunked_complete(input: &[u8], max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    loop {
        let Some(line_len) = find_crlf(&input[pos..]) else {
            return Ok(None);
        };
        let line = std::str::from_utf8(&input[pos..pos + line_len])
            .map_err(|_| "invalid chunk size line".to_string())?;
        let size_part = line
            .split(';')
            .next()
            .unwrap_or("")
            .trim_end_matches([' ', '\t']);
        if size_part.is_empty() || !size_part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid chunk size".to_string());
        }
        let size =
            usize::from_str_radix(size_part, 16).map_err(|_| "chunk size overflow".to_string())?;
        pos += line_len + 2;
        if size == 0 {
            let rest = &input[pos..];
            if rest.len() < 2 {
                return Ok(None);
            }
            if rest.starts_with(b"\r\n") {
                return Ok(Some(out));
            }
            if find_crlf_crlf(rest).is_some() {
                return Ok(Some(out));
            }
            return Ok(None);
        }
        if out.len().checked_add(size).is_none_or(|t| t > max_bytes) {
            return Err("chunked body exceeds limit".to_string());
        }
        let Some(chunk_end) = pos.checked_add(size).and_then(|v| v.checked_add(2)) else {
            return Err("chunk framing overflow".to_string());
        };
        if input.len() < chunk_end {
            return Ok(None);
        }
        if &input[pos + size..chunk_end] != b"\r\n" {
            return Err("missing chunk terminator".to_string());
        }
        out.extend_from_slice(&input[pos..pos + size]);
        pos = chunk_end;
    }
}

fn parse_whois_identity(body: &[u8]) -> Result<EngineIdentity, String> {
    let val: Value =
        serde_json::from_slice(body).map_err(|_| "malformed whois JSON".to_string())?;
    let obj = val
        .as_object()
        .ok_or_else(|| "whois body must be a JSON object".to_string())?;
    let node = obj
        .get("Node")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing whois Node".to_string())?;
    let user_profile = obj
        .get("UserProfile")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing whois UserProfile".to_string())?;

    let computed = node
        .get("ComputedName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let host = match computed {
        Some(name) => name,
        None => node
            .get("Name")
            .and_then(Value::as_str)
            .and_then(|s| s.split('.').next())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "missing whois host".to_string())?,
    };
    if !valid_host(host) {
        return Err("invalid whois host".to_string());
    }

    let principal = match node.get("Tags") {
        Some(Value::Array(tags)) if !tags.is_empty() => {
            let mut valid_tags = Vec::with_capacity(tags.len());
            for item in tags {
                let Some(tag) = item.as_str() else {
                    return Err("non-string tag in whois Node.Tags".to_string());
                };
                if !tag.is_empty() {
                    valid_tags.push(tag);
                }
            }
            let smallest = valid_tags
                .into_iter()
                .min()
                .ok_or_else(|| "no non-empty tag in whois Node.Tags".to_string())?;
            format!("ts:{smallest}")
        }
        None | Some(Value::Null) | Some(Value::Array(_)) => {
            let login = user_profile
                .get("LoginName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && *s != "tagged-devices")
                .ok_or_else(|| "missing whois UserProfile.LoginName".to_string())?;
            format!("ts:{login}")
        }
        Some(_) => return Err("malformed whois Node.Tags".to_string()),
    };
    if !principal_form(&principal) {
        return Err("invalid whois principal".to_string());
    }

    Ok(EngineIdentity {
        principal,
        host: host.to_string(),
    })
}

impl PeerIdentityResolver for TailscaleWhois {
    fn resolve(&self, peer: SocketAddr) -> Result<EngineIdentity, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|_| "failed to connect to tailscale socket".to_string())?;
        stream
            .set_read_timeout(Some(WHOIS_TIMEOUT))
            .map_err(|_| "failed to set whois read timeout".to_string())?;
        stream
            .set_write_timeout(Some(WHOIS_TIMEOUT))
            .map_err(|_| "failed to set whois write timeout".to_string())?;

        let encoded = percent_encode_query_value(&peer.to_string());
        let request = format!(
            "GET /localapi/v0/whois?addr={encoded} HTTP/1.0\r\nHost: local-tailscaled.sock\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .and_then(|()| stream.flush())
            .map_err(|_| "failed to write whois request".to_string())?;

        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        let mut header_end: Option<usize> = None;
        let mut is_chunked = false;
        let mut content_length: Option<usize> = None;

        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if raw.len().saturating_add(n) > WHOIS_MAX_BYTES {
                        return Err("whois response exceeded byte limit".to_string());
                    }
                    raw.extend_from_slice(&buf[..n]);
                    if header_end.is_none()
                        && let Some(pos) = find_crlf_crlf(&raw)
                    {
                        let (chunked, cl) = parse_whois_response_headers(&raw[..pos])?;
                        is_chunked = chunked;
                        content_length = cl;
                        header_end = Some(pos + 4);
                    }
                    if let Some(end) = header_end {
                        let body_slice = &raw[end..];
                        if is_chunked {
                            if decode_chunked_complete(body_slice, WHOIS_MAX_BYTES)?.is_some() {
                                break;
                            }
                        } else if let Some(cl) = content_length
                            && body_slice.len() >= cl
                        {
                            break;
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Err("failed to read whois response".to_string()),
            }
        }

        let end = header_end.ok_or_else(|| "incomplete whois response headers".to_string())?;
        let body = if is_chunked {
            decode_chunked_complete(&raw[end..], WHOIS_MAX_BYTES)?
                .ok_or_else(|| "truncated chunked whois response".to_string())?
        } else {
            let slice = &raw[end..];
            if let Some(cl) = content_length {
                if slice.len() < cl {
                    return Err("truncated whois response body".to_string());
                }
                slice[..cl].to_vec()
            } else {
                slice.to_vec()
            }
        };
        parse_whois_identity(&body)
    }
}

struct InFlight {
    count: Mutex<usize>,
    zero: Condvar,
}

impl InFlight {
    fn new() -> Self {
        Self {
            count: Mutex::new(0),
            zero: Condvar::new(),
        }
    }

    fn try_acquire(self: &Arc<Self>) -> Option<InFlightGuard> {
        let mut guard = match self.count.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *guard >= MAX_CONCURRENT_CONNECTIONS {
            return None;
        }
        *guard += 1;
        Some(InFlightGuard {
            state: Arc::clone(self),
        })
    }

    fn wait_zero(&self) {
        let mut guard = match self.count.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        while *guard > 0 {
            guard = match self.zero.wait(guard) {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }
}

struct InFlightGuard {
    state: Arc<InFlight>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        let mut guard = match self.state.count.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = guard.saturating_sub(1);
        if *guard == 0 {
            self.state.zero.notify_all();
        }
    }
}

/// Serve HTTP/1.1 requests on an already-bound `TcpListener` until `shutdown`
/// becomes `true`, then drain active in-flight connections and return `Ok(())`.
pub fn serve_listener(
    listener: TcpListener,
    config: Config,
    resolver: Arc<dyn PeerIdentityResolver>,
    shutdown: Arc<AtomicBool>,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    let config = Arc::new(config);
    let in_flight = Arc::new(InFlight::new());
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: ACCEPT_POLL_NS,
    };

    while !shutdown.load(Ordering::Acquire) {
        let mut fds = [PollFd::new(&listener, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => continue,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(e) => return Err(std::io::Error::from_raw_os_error(e.raw_os_error())),
        }
        loop {
            if shutdown.load(Ordering::Acquire) {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let Some(slot) = in_flight.try_acquire() else {
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        let resp = HttpResponse::refusal(
                            503,
                            "Service Unavailable",
                            Refusal::new("server_busy"),
                            config.now.unwrap_or_else(now_seconds),
                        );
                        let _ = write_response(&mut stream, &resp);
                        continue;
                    };
                    let fallback = stream.try_clone().ok();
                    let cfg = Arc::clone(&config);
                    let res = Arc::clone(&resolver);
                    let spawn = std::thread::Builder::new().spawn(move || {
                        let _slot = slot;
                        handle_connection(stream, &cfg, &*res);
                    });
                    if spawn.is_err()
                        && let Some(mut s) = fallback
                    {
                        // `_slot` was moved into the closure and dropped on error,
                        // decrementing `in_flight` cleanly.
                        let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
                        let resp = HttpResponse::refusal(
                            503,
                            "Service Unavailable",
                            Refusal::new("server_busy"),
                            config.now.unwrap_or_else(now_seconds),
                        );
                        let _ = write_response(&mut s, &resp);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }

    in_flight.wait_zero();
    Ok(())
}

struct HttpResponse {
    status: u16,
    reason: &'static str,
    body: String,
}

impl HttpResponse {
    fn json(status: u16, reason: &'static str, body: String) -> Self {
        Self {
            status,
            reason,
            body,
        }
    }

    fn empty(status: u16, reason: &'static str) -> Self {
        Self {
            status,
            reason,
            body: String::new(),
        }
    }

    fn refusal(status: u16, reason: &'static str, refusal: Refusal, now: u64) -> Self {
        let outcome = render(Err(Failure::Refused(refusal)), now);
        Self::json(
            status,
            reason,
            outcome.stdout.trim_end_matches('\n').to_string(),
        )
    }

    fn internal(code: &str) -> Self {
        Self::json(
            500,
            "Internal Server Error",
            json!({"status":"internal","code":code}).to_string(),
        )
    }
}

fn write_response(stream: &mut TcpStream, resp: &HttpResponse) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        resp.status,
        resp.reason,
        resp.body.len()
    );
    stream.write_all(head.as_bytes())?;
    if !resp.body.is_empty() {
        stream.write_all(resp.body.as_bytes())?;
    }
    stream.flush()?;
    let _ = stream.shutdown(Shutdown::Write);
    Ok(())
}

fn handle_connection(mut stream: TcpStream, config: &Config, resolver: &dyn PeerIdentityResolver) {
    let _ = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));

    let response = match mcp::catch_call_panic(|| process_connection(&mut stream, config, resolver))
    {
        Ok(Some(resp)) => resp,
        Ok(None) => return,
        Err(loc) => HttpResponse::internal(&format!("panic@{loc}")),
    };
    let _ = write_response(&mut stream, &response);
}

enum HeadError {
    HeadersTooLarge,
    UnsupportedTransferEncoding,
    Malformed,
}

struct RequestHead {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    content_length: usize,
    expect_continue: bool,
    body_prefix: Vec<u8>,
}

fn is_http_token_byte(b: u8) -> bool {
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

fn is_valid_header_value(s: &str) -> bool {
    s.bytes().all(|b| b == b'\t' || (0x20..=0x7E).contains(&b))
}

fn read_request_head(stream: &mut TcpStream) -> Result<Option<RequestHead>, HeadError> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find_crlf_crlf(&buf) {
            let end = pos + 4;
            if end > MAX_HEADER_BYTES {
                return Err(HeadError::HeadersTooLarge);
            }
            break end;
        }
        if buf.len() >= MAX_HEADER_BYTES {
            return Err(HeadError::HeadersTooLarge);
        }
        let max_read = (MAX_HEADER_BYTES + 1 - buf.len()).min(tmp.len());
        match stream.read(&mut tmp[..max_read]) {
            Ok(0) => {
                if buf.is_empty() {
                    return Ok(None);
                }
                return Err(HeadError::Malformed);
            }
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Ok(None),
        }
    };

    let head_bytes = &buf[..header_end - 4];
    let head_str = std::str::from_utf8(head_bytes).map_err(|_| HeadError::Malformed)?;
    // Reject bare LF or bare CR anywhere in the header block.
    let mut lines = head_str.split("\r\n");
    let request_line = lines.next().ok_or(HeadError::Malformed)?;
    if request_line.contains('\r') || request_line.contains('\n') {
        return Err(HeadError::Malformed);
    }
    let mut parts = request_line.split(' ');
    let (Some(method), Some(path), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HeadError::Malformed);
    };
    if method.is_empty()
        || !method.bytes().all(is_http_token_byte)
        || !path.starts_with('/')
        || path.bytes().any(|b| b <= 0x20 || b >= 0x7F)
        || !(version == "HTTP/1.1" || version == "HTTP/1.0")
    {
        return Err(HeadError::Malformed);
    }

    let mut headers = BTreeMap::new();
    let mut content_length: Option<usize> = None;
    let mut expect_continue = false;
    for line in lines {
        if line.contains('\r') || line.contains('\n') {
            return Err(HeadError::Malformed);
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Reject obs-fold continuation lines.
            return Err(HeadError::Malformed);
        }
        let Some((raw_name, raw_val)) = line.split_once(':') else {
            return Err(HeadError::Malformed);
        };
        if raw_name.is_empty() || !raw_name.bytes().all(is_http_token_byte) {
            return Err(HeadError::Malformed);
        }
        let value = raw_val.trim_matches([' ', '\t']);
        if !is_valid_header_value(value) {
            return Err(HeadError::Malformed);
        }
        let name = raw_name.to_ascii_lowercase();
        if name == "transfer-encoding" {
            return Err(HeadError::UnsupportedTransferEncoding);
        }
        if name == "content-length" {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(HeadError::Malformed);
            }
            let parsed = value.parse::<usize>().map_err(|_| HeadError::Malformed)?;
            if content_length.replace(parsed).is_some() {
                return Err(HeadError::Malformed);
            }
            continue;
        }
        if name == "expect" {
            if value.eq_ignore_ascii_case("100-continue") {
                expect_continue = true;
            }
            continue;
        }
        if matches!(
            name.as_str(),
            "host" | "x-ont-client" | "x-ont-agent" | "x-ont-role"
        ) && headers.contains_key(&name)
        {
            return Err(HeadError::Malformed);
        }
        headers.insert(name, value.to_string());
    }

    Ok(Some(RequestHead {
        method: method.to_string(),
        path: path.to_string(),
        headers,
        content_length: content_length.unwrap_or(0),
        expect_continue,
        body_prefix: buf[header_end..].to_vec(),
    }))
}

#[derive(Clone, Copy)]
enum Route {
    Healthz,
    Mcp,
    V1(V1Op),
}

#[derive(Clone, Copy)]
enum V1Op {
    Describe,
    Get,
    Search,
    Around,
    Aggregate,
    Log,
    Render,
    Fn,
    ActionList,
    ActionValidate,
    ActionApply,
}

fn match_route(path: &str) -> Option<Route> {
    match path {
        "/healthz" => Some(Route::Healthz),
        "/mcp" => Some(Route::Mcp),
        "/v1/describe" => Some(Route::V1(V1Op::Describe)),
        "/v1/get" => Some(Route::V1(V1Op::Get)),
        "/v1/search" => Some(Route::V1(V1Op::Search)),
        "/v1/around" => Some(Route::V1(V1Op::Around)),
        "/v1/aggregate" => Some(Route::V1(V1Op::Aggregate)),
        "/v1/log" => Some(Route::V1(V1Op::Log)),
        "/v1/render" => Some(Route::V1(V1Op::Render)),
        "/v1/fn" => Some(Route::V1(V1Op::Fn)),
        "/v1/action/list" => Some(Route::V1(V1Op::ActionList)),
        "/v1/action/validate" => Some(Route::V1(V1Op::ActionValidate)),
        "/v1/action/apply" => Some(Route::V1(V1Op::ActionApply)),
        _ => None,
    }
}

struct RemoteAttribution {
    client: String,
    agent: Option<String>,
    role: Option<String>,
}

fn resolve_attribution(
    headers: &BTreeMap<String, String>,
    default_client: &str,
) -> Result<RemoteAttribution, Refusal> {
    let client = match headers.get("x-ont-client") {
        Some(raw) => {
            if !mcp::valid_client(raw) {
                return Err(Refusal::input(
                    "invalid_request",
                    "/client",
                    "1 to 32 ASCII characters: lowercase letter or digit first; then lowercase letters, digits, or hyphen",
                ));
            }
            if raw == "cli" {
                "remote".to_string()
            } else {
                raw.clone()
            }
        }
        None => {
            if default_client.is_empty()
                || default_client == "cli"
                || !mcp::valid_client(default_client)
            {
                "remote".to_string()
            } else {
                default_client.to_string()
            }
        }
    };
    let agent = match headers.get("x-ont-agent") {
        Some(raw) => {
            if !valid_actor_field(raw) {
                return Err(invalid_actor_field("/agent"));
            }
            Some(raw.clone())
        }
        None => None,
    };
    let role = match headers.get("x-ont-role") {
        Some(raw) => {
            if !valid_actor_field(raw) {
                return Err(invalid_actor_field("/role"));
            }
            Some(raw.clone())
        }
        None => None,
    };
    Ok(RemoteAttribution {
        client,
        agent,
        role,
    })
}

fn read_body(
    stream: &mut TcpStream,
    prefix: &[u8],
    content_length: usize,
    expect_continue: bool,
) -> Result<Vec<u8>, ()> {
    if content_length == 0 {
        return Ok(Vec::new());
    }
    if prefix.len() >= content_length {
        return Ok(prefix[..content_length].to_vec());
    }
    if expect_continue {
        let _ = stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .and_then(|()| stream.flush());
    }
    let mut body = vec![0u8; content_length];
    body[..prefix.len()].copy_from_slice(prefix);
    let mut filled = prefix.len();
    while filled < content_length {
        match stream.read(&mut body[filled..]) {
            Ok(0) => return Err(()),
            Ok(n) => filled += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err(()),
        }
    }
    Ok(body)
}

fn process_connection(
    stream: &mut TcpStream,
    config: &Config,
    resolver: &dyn PeerIdentityResolver,
) -> Option<HttpResponse> {
    let now = config.now.unwrap_or_else(now_seconds);

    // Peer authentication runs before parsing HTTP input or opening the store.
    let Ok(peer_addr) = stream.peer_addr() else {
        return Some(HttpResponse::refusal(
            403,
            "Forbidden",
            Refusal::new("peer_not_identified"),
            now,
        ));
    };
    let Ok(identity) = resolver.resolve(peer_addr) else {
        return Some(HttpResponse::refusal(
            403,
            "Forbidden",
            Refusal::new("peer_not_identified"),
            now,
        ));
    };

    let head = match read_request_head(stream) {
        Ok(Some(head)) => head,
        Ok(None) => return None,
        Err(HeadError::HeadersTooLarge) => {
            return Some(HttpResponse::refusal(
                431,
                "Request Header Fields Too Large",
                Refusal::new("headers_too_large"),
                now,
            ));
        }
        Err(HeadError::UnsupportedTransferEncoding) => {
            return Some(HttpResponse::refusal(
                400,
                "Bad Request",
                Refusal::new("unsupported_transfer_encoding"),
                now,
            ));
        }
        Err(HeadError::Malformed) => {
            return Some(HttpResponse::refusal(
                400,
                "Bad Request",
                Refusal::new("malformed_request"),
                now,
            ));
        }
    };

    let Some(route) = match_route(&head.path) else {
        return Some(HttpResponse::refusal(
            404,
            "Not Found",
            Refusal::new("unknown_endpoint"),
            now,
        ));
    };

    match route {
        Route::Healthz => {
            if head.method != "GET" {
                return Some(HttpResponse::refusal(
                    405,
                    "Method Not Allowed",
                    Refusal::new("method_not_allowed"),
                    now,
                ));
            }
            Some(HttpResponse::json(
                200,
                "OK",
                json!({
                    "status": "ok",
                    "service": "ont-serve",
                    "version": env!("ONT_BUILD_REVISION")
                })
                .to_string(),
            ))
        }
        Route::Mcp => {
            if head.method != "POST" {
                return Some(HttpResponse::refusal(
                    405,
                    "Method Not Allowed",
                    Refusal::new("method_not_allowed"),
                    now,
                ));
            }
            if head.content_length > MCP_MAX_LINE {
                return Some(HttpResponse::json(
                    413,
                    "Payload Too Large",
                    json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {"code": -32600, "message": "request_too_large"}
                    })
                    .to_string(),
                ));
            }
            let attr = match resolve_attribution(&head.headers, &config.client) {
                Ok(a) => a,
                Err(r) => return Some(HttpResponse::refusal(400, "Bad Request", r, now)),
            };
            let Ok(body) = read_body(
                stream,
                &head.body_prefix,
                head.content_length,
                head.expect_continue,
            ) else {
                return Some(HttpResponse::refusal(
                    400,
                    "Bad Request",
                    Refusal::new("malformed_request"),
                    now,
                ));
            };
            let mcp_cfg = mcp::Config {
                store: config.store.clone(),
                now: config.now,
                client: attr.client,
                read_only: false,
            };
            let remote = RemoteCall {
                identity,
                agent: attr.agent,
                role: attr.role,
            };
            let mut server = mcp::Server::new_remote(mcp_cfg, &execute, remote);
            match server.handle_line(&body) {
                Some(resp) => Some(HttpResponse::json(200, "OK", resp.to_string())),
                None => Some(HttpResponse::empty(202, "Accepted")),
            }
        }
        Route::V1(v1_op) => {
            if head.method != "POST" {
                return Some(HttpResponse::refusal(
                    405,
                    "Method Not Allowed",
                    Refusal::new("method_not_allowed"),
                    now,
                ));
            }
            if head.content_length > MAX_REQUEST {
                return Some(HttpResponse::refusal(
                    413,
                    "Payload Too Large",
                    Refusal::new("request_too_large"),
                    now,
                ));
            }
            let attr = match resolve_attribution(&head.headers, &config.client) {
                Ok(a) => a,
                Err(r) => return Some(HttpResponse::refusal(400, "Bad Request", r, now)),
            };
            let Ok(mut body) = read_body(
                stream,
                &head.body_prefix,
                head.content_length,
                head.expect_continue,
            ) else {
                return Some(HttpResponse::refusal(
                    400,
                    "Bad Request",
                    Refusal::new("malformed_request"),
                    now,
                ));
            };
            if body.iter().all(u8::is_ascii_whitespace) {
                body = b"{}".to_vec();
            }

            let mut ctx = Context::new(config.store.clone(), now);
            ctx.client = attr.client;
            ctx.agent = attr.agent;
            ctx.role = attr.role;
            ctx.peer_identity = Some(identity);

            let result = match v1_op {
                V1Op::Describe => execute(Op::Describe, &body, &ctx),
                V1Op::Get => execute(Op::Get, &body, &ctx),
                V1Op::Search => execute(Op::Search, &body, &ctx),
                V1Op::Around => execute(Op::Around, &body, &ctx),
                V1Op::Aggregate => execute(Op::Aggregate, &body, &ctx),
                V1Op::Log => execute(Op::Log, &body, &ctx),
                V1Op::Render => crate::render::RenderRequest::parse(&body)
                    .map_err(Failure::from)
                    .and_then(|req| crate::render::render(&ctx, &req)),
                V1Op::Fn => execute(Op::Fn, &body, &ctx),
                V1Op::ActionList => ListActionsRequest::parse(&body)
                    .map_err(Failure::from)
                    .and_then(|req| list_actions(&ctx, &req)),
                V1Op::ActionValidate => {
                    take_role(body)
                        .map_err(Failure::from)
                        .and_then(|(bytes, body_role)| {
                            let mut call_ctx = ctx.clone();
                            call_ctx.role = body_role.or(call_ctx.role);
                            let req = ValidateActionRequest::parse(&bytes)?;
                            validate_action(&call_ctx, &req)
                        })
                }
                V1Op::ActionApply => {
                    take_role(body)
                        .map_err(Failure::from)
                        .and_then(|(bytes, body_role)| {
                            let mut call_ctx = ctx.clone();
                            call_ctx.role = body_role.or(call_ctx.role);
                            let req = ApplyActionRequest::parse(&bytes)?;
                            apply_action(&call_ctx, &req)
                        })
                }
            };
            let outcome = render(result, now);
            match outcome.code {
                0 => Some(HttpResponse::json(
                    200,
                    "OK",
                    outcome.stdout.trim_end_matches('\n').to_string(),
                )),
                2 => Some(HttpResponse::json(
                    400,
                    "Bad Request",
                    outcome.stdout.trim_end_matches('\n').to_string(),
                )),
                _ => Some(HttpResponse::internal(outcome.stderr.trim_end())),
            }
        }
    }
}
