//! The proxy of a stage with `limits` (`specs/ap/run-limits.md` R17 to R20): the one listener of
//! `ap run` in the stage's network namespace. It admits a `CONNECT` to port 443 of a name on the
//! list of the stage's role, resolved and connected outside the stage through the port's way out,
//! relays the bytes both ways, writes one line per request to `AP_EGRESS_LOG` and is closed after
//! the call's last launch.
use super::stamp;
pub(super) mod pc;
use crate::host::{Network, System, WayOut};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

/// The names every role reaches: the model provider (R18).
const MODEL: [&str; 1] = ["api.anthropic.com"];
/// The names an implementer, a fixer and a verifier reach: the model provider, GitHub and the
/// crate registry, in the order of R18.
const BUILD: [&str; 5] = [
    "api.anthropic.com",
    "github.com",
    "api.github.com",
    "index.crates.io",
    "static.crates.io",
];
/// The ranges no stage reaches, in CIDR form, in the order of R18: what `forbidden` judges by, and
/// what the proxy of a look on the PC is given (`specs/ap/pc-egress.md` R6).
pub(crate) const RANGES: [&str; 13] = [
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "224.0.0.0/3",
    "::/128",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
];
/// The longest request: its request line and header lines with the empty line (R18).
const REQUEST_MAX: usize = 8_192;
/// The bound of reading a request and of connecting to a name's addresses (R18).
const WITHIN: Duration = Duration::from_secs(10);
/// The connections served at once (R18).
const AT_ONCE: usize = 64;
/// The only port a request may name (R18).
const PORT: u16 = 443;
/// The record without `AP_EGRESS_LOG` (R19).
const LOG_DEFAULT: &str = "/var/lib/agent-platform/egress.jsonl";

/// The list of names of a role, its `ONT_ROLE` (R18).
pub(crate) fn names(role: &str) -> &'static [&'static str] {
    match role {
        "implementer" | "fixer" | "verifier" => &BUILD,
        _ => &MODEL,
    }
}

/// An open proxy; `close` ends it (R20), as its drop does.
pub(crate) struct Proxy {
    port: u16,
    names: &'static [&'static str],
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
    namespace: Option<OwnedFd>,
}
struct Shared {
    names: &'static [&'static str],
    run: String,
    role: String,
    log: PathBuf,
    way: Arc<dyn WayOut>,
    stop: AtomicBool,
    busy: Mutex<usize>,
    freed: Condvar,
    open: Mutex<Open>,
}
/// The connections being served, each with its client and, once admitted, its upstream: what the
/// close shuts (R20).
#[derive(Default)]
struct Open {
    closed: bool,
    next: u64,
    streams: BTreeMap<u64, Vec<TcpStream>>,
}
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Opens the proxy of a call (R17): the port makes the stage's network and answers the way out;
/// the record's path is read from the port's environment once, here (R19). `run` is the path of
/// the call's record and `role` the stage's `ONT_ROLE`.
pub(crate) fn open(s: &mut impl System, run: String, role: &str) -> std::io::Result<Proxy> {
    let log = s
        .environment()
        .into_iter()
        .rev()
        .find(|(k, _)| k == "AP_EGRESS_LOG")
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| LOG_DEFAULT.to_owned());
    let Network {
        listener,
        namespace,
    } = s.network()?;
    let way = s.way_out();
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let names = names(role);
    let shared = Arc::new(Shared {
        names,
        run,
        role: role.to_owned(),
        log: PathBuf::from(log),
        way,
        stop: AtomicBool::new(false),
        busy: Mutex::new(0),
        freed: Condvar::new(),
        open: Mutex::new(Open::default()),
    });
    let accept = {
        let shared = shared.clone();
        std::thread::spawn(move || accept(listener, &shared))
    };
    Ok(Proxy {
        port,
        names,
        shared,
        accept: Some(accept),
        namespace,
    })
}

impl Proxy {
    /// What the stage's variables name (R17) and the record's `egress.proxy` (R7).
    pub(crate) fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    /// The record's `egress` (R7).
    pub(crate) fn member(&self) -> serde_json::Value {
        json!({"proxy": self.url(), "names": self.names})
    }
    /// The handle of the stage's namespace, when the port made one.
    pub(crate) fn namespace(&self) -> Option<BorrowedFd<'_>> {
        self.namespace.as_ref().map(AsFd::as_fd)
    }
    /// The close (R20): no further connection, every connection shut both ways, the listener and
    /// the namespace's handle let go.
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
        // The accepting thread owns the listener: it is closed when the thread ends.
        let _ = accept.join();
        let mut open = lock(&shared.open);
        open.closed = true;
        for stream in open.streams.values().flatten() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        open.streams.clear();
        drop(open);
        self.namespace = None;
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.shut();
    }
}

fn accept(listener: TcpListener, shared: &Arc<Shared>) {
    while !shared.stop.load(Ordering::SeqCst) {
        // Its turn: at most 64 connections are served at once; a further one waits (R18).
        {
            let mut busy = lock(&shared.busy);
            while *busy >= AT_ONCE && !shared.stop.load(Ordering::SeqCst) {
                busy = match shared.freed.wait_timeout(busy, Duration::from_millis(50)) {
                    Ok((b, _)) => b,
                    Err(e) => e.into_inner().0,
                };
            }
        }
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok((stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };
        let at = SystemTime::now();
        *lock(&shared.busy) += 1;
        let shared = shared.clone();
        std::thread::spawn(move || {
            serve(&shared, stream, at);
            *lock(&shared.busy) -= 1;
            shared.freed.notify_all();
        });
    }
    drop(listener);
}

/// Notes a stream of connection `id` for the close; false when the proxy is closed.
fn hold(shared: &Shared, id: Option<u64>, stream: &TcpStream) -> Option<u64> {
    let mut open = lock(&shared.open);
    if open.closed {
        return None;
    }
    let id = id.unwrap_or_else(|| {
        open.next += 1;
        open.next
    });
    if let Ok(copy) = stream.try_clone() {
        open.streams.entry(id).or_default().push(copy);
    }
    Some(id)
}
fn release(shared: &Shared, id: u64) {
    lock(&shared.open).streams.remove(&id);
}

/// How a request ends: admitted, or refused with a status and a word (R18).
enum Verdict {
    Admitted(TcpStream),
    Refused(u16, &'static str),
}

fn serve(shared: &Shared, mut client: TcpStream, at: SystemTime) {
    let _ = client.set_nonblocking(false);
    let Some(id) = hold(shared, None, &client) else {
        let _ = client.shutdown(Shutdown::Both);
        return;
    };
    let read = read_request(&mut client);
    let (target, rest, verdict) = match read.as_ref().and_then(|(head, _)| form(head)) {
        None => (None, &[][..], Verdict::Refused(400, "form")),
        Some(None) => (None, &[][..], Verdict::Refused(405, "method")),
        Some(Some((host, port))) => {
            let rest = read.as_ref().map_or(&[][..], |(_, rest)| rest.as_slice());
            let verdict = judge(shared, &host, port);
            (Some((host, port)), rest, verdict)
        }
    };
    let (status, reason) = match &verdict {
        Verdict::Admitted(_) => (200, None),
        Verdict::Refused(status, reason) => (*status, Some(*reason)),
    };
    record(shared, at, target.as_ref(), status, reason);
    match verdict {
        Verdict::Refused(status, _) => {
            let line = format!("HTTP/1.1 {status} {}\r\n\r\n", phrase(status));
            let _ = client.write_all(line.as_bytes());
            let _ = client.shutdown(Shutdown::Both);
        }
        Verdict::Admitted(upstream) => {
            if hold(shared, Some(id), &upstream).is_some() {
                relay(client, upstream, rest);
            } else {
                let _ = upstream.shutdown(Shutdown::Both);
                let _ = client.shutdown(Shutdown::Both);
            }
        }
    }
    release(shared, id);
}

fn phrase(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        _ => "Bad Gateway",
    }
}

/// The end of the first empty line: the length of the request through it.
fn head_end(buf: &[u8]) -> Option<usize> {
    let mut start = 0;
    while let Some(i) = buf[start..].iter().position(|b| *b == b'\n') {
        let line = &buf[start..start + i];
        if line.strip_suffix(b"\r").unwrap_or(line).is_empty() {
            return Some(start + i + 1);
        }
        start += i + 1;
    }
    None
}

/// One request: its request line, and the bytes the client sent after its empty line; None when
/// it is not complete within 10 seconds and 8,192 bytes (R18).
fn read_request(client: &mut TcpStream) -> Option<(Vec<u8>, Vec<u8>)> {
    let deadline = Instant::now() + WITHIN;
    let mut buf: Vec<u8> = vec![];
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = head_end(&buf) {
            if end > REQUEST_MAX {
                return None;
            }
            let rest = buf.split_off(end);
            let line = buf.split(|b| *b == b'\n').next().unwrap_or_default();
            let line = line.strip_suffix(b"\r").unwrap_or(line).to_vec();
            return Some((line, rest));
        }
        if buf.len() >= REQUEST_MAX {
            return None;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || client.set_read_timeout(Some(left)).is_err() {
            return None;
        }
        match client.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => (),
            Err(_) => return None,
        }
    }
}

/// The form of a request line (R18): None when it is not of the form; else, for `CONNECT`, its
/// host and port, for another first word nothing.
fn form(line: &[u8]) -> Option<Option<(String, u16)>> {
    let words: Vec<&[u8]> = line.split(|b| *b == b' ').collect();
    let [method, target, version] = words[..] else {
        return None;
    };
    let version = version == b"HTTP/1.1" || version == b"HTTP/1.0";
    if method.is_empty() || target.is_empty() || !version {
        return None;
    }
    if method != b"CONNECT" {
        return Some(None);
    }
    let colon = target.iter().rposition(|b| *b == b':')?;
    let (host, port) = (&target[..colon], &target[colon + 1..]);
    let host_ok = (1..=255).contains(&host.len()) && host.iter().all(|b| (0x21..=0x7e).contains(b));
    let port_ok =
        (1..=5).contains(&port.len()) && port.iter().all(u8::is_ascii_digit) && port[0] != b'0';
    if !host_ok || !port_ok {
        return None;
    }
    let port: u32 = std::str::from_utf8(port).ok()?.parse().ok()?;
    let port = u16::try_from(port).ok()?;
    Some(Some((String::from_utf8(host.to_vec()).ok()?, port)))
}

/// An IPv4 address in four decimal parts, or a bracketed IPv6 address (R18).
fn literal(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner.parse::<Ipv6Addr>().is_ok();
    }
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// An address no stage reaches: of this host, of the tailnet, of a private, link-local or
/// multicast network, or unspecified (R18): one in a range of `RANGES`.
fn forbidden(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v) => forbidden_v4(v),
        IpAddr::V6(v) => {
            if let Some(mapped) = v.to_ipv4_mapped() {
                return forbidden_v4(mapped);
            }
            RANGES.iter().any(|range| {
                let parsed = range.split_once('/').and_then(|(net, len)| {
                    Some((net.parse::<Ipv6Addr>().ok()?, len.parse::<u32>().ok()?))
                });
                let Some((net, len)) = parsed else {
                    return false;
                };
                let mask = u128::MAX
                    .checked_shl(128u32.saturating_sub(len))
                    .unwrap_or(0);
                u128::from(v) & mask == u128::from(net) & mask
            })
        }
    }
}
fn forbidden_v4(v: Ipv4Addr) -> bool {
    RANGES.iter().any(|range| {
        let parsed = range
            .split_once('/')
            .and_then(|(net, len)| Some((net.parse::<Ipv4Addr>().ok()?, len.parse::<u32>().ok()?)));
        let Some((net, len)) = parsed else {
            return false;
        };
        let mask = u32::MAX.checked_shl(32u32.saturating_sub(len)).unwrap_or(0);
        u32::from(v) & mask == u32::from(net) & mask
    })
}

/// The judgement of a `CONNECT` of the form, in the order of R18; a connection is opened only to
/// port 443 of an address of a listed name whose every address is allowed.
fn judge(shared: &Shared, host: &str, port: u16) -> Verdict {
    if port != PORT {
        return Verdict::Refused(403, "port");
    }
    if literal(host) {
        return Verdict::Refused(403, "literal");
    }
    let lower = host.to_ascii_lowercase();
    let name = lower.strip_suffix('.').unwrap_or(&lower);
    if !shared.names.contains(&name) {
        return Verdict::Refused(403, "not_listed");
    }
    let addresses = match shared.way.resolve(name) {
        Ok(found) if !found.is_empty() => found,
        _ => return Verdict::Refused(502, "unresolved"),
    };
    if addresses.iter().any(|a| forbidden(*a)) {
        return Verdict::Refused(403, "address");
    }
    let deadline = Instant::now() + WITHIN;
    for address in addresses {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        if let Ok(upstream) = shared.way.connect(SocketAddr::new(address, PORT), left) {
            return Verdict::Admitted(upstream);
        }
    }
    Verdict::Refused(502, "unreachable")
}

/// The answer 200, the bytes the client sent after its request, then both ways until either side
/// closes; then both are shut (R18).
fn relay(mut client: TcpStream, mut upstream: TcpStream, rest: &[u8]) {
    let started = client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .and_then(|()| upstream.write_all(rest))
        .and_then(|()| client.set_read_timeout(None));
    let shut = |a: &TcpStream, b: &TcpStream| {
        let _ = a.shutdown(Shutdown::Both);
        let _ = b.shutdown(Shutdown::Both);
    };
    let (Ok(()), Ok(mut back_from), Ok(mut back_to)) =
        (started, upstream.try_clone(), client.try_clone())
    else {
        shut(&client, &upstream);
        return;
    };
    let back = std::thread::spawn(move || {
        let _ = std::io::copy(&mut back_from, &mut back_to);
        shut(&back_from, &back_to);
    });
    let _ = std::io::copy(&mut client, &mut upstream);
    shut(&client, &upstream);
    let _ = back.join();
}

/// One line of the record (R19), by one `write` in append mode; a file made here has mode 0600.
fn record(
    shared: &Shared,
    at: SystemTime,
    target: Option<&(String, u16)>,
    status: u16,
    reason: Option<&str>,
) {
    use std::os::unix::fs::OpenOptionsExt;
    let line = json!({"at": stamp(at), "run": shared.run, "role": shared.role,
        "name": target.map(|(host, _)| host), "port": target.map(|(_, port)| port),
        "status": status, "reason": reason});
    let mut bytes = line.to_string().into_bytes();
    bytes.push(b'\n');
    let _ = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(&shared.log)
        .and_then(|mut f| f.write(&bytes));
}

#[cfg(test)]
mod tests {
    use super::{forbidden, form, head_end, literal, names};
    // spec: specs/ap/run-limits.md#R18
    #[test]
    fn the_form_of_a_request_line() {
        let ok = |l: &str| form(l.as_bytes());
        assert_eq!(
            ok("CONNECT api.anthropic.com:443 HTTP/1.1"),
            Some(Some(("api.anthropic.com".into(), 443)))
        );
        assert_eq!(
            ok("CONNECT [::1]:443 HTTP/1.0"),
            Some(Some(("[::1]".into(), 443)))
        );
        assert_eq!(ok("GET http://x/ HTTP/1.1"), Some(None));
        for bad in [
            "CONNECT api.anthropic.com:0443 HTTP/1.1",
            "CONNECT  api.anthropic.com:443 HTTP/1.1",
            "CONNECT api.anthropic.com HTTP/1.1",
            "CONNECT api.anthropic.com:65536 HTTP/1.1",
            "CONNECT api.anthropic.com:0 HTTP/1.1",
            "CONNECT :443 HTTP/1.1",
            "CONNECT a:443 HTTP/2",
            "CONNECT a:443",
            "",
        ] {
            assert_eq!(ok(bad), None, "{bad}");
        }
        assert_eq!(
            ok("CONNECT a:65535 HTTP/1.1"),
            Some(Some(("a".into(), 65535)))
        );
        assert_eq!(head_end(b"A\r\nB: c\r\n\r\nrest"), Some(11));
        assert_eq!(head_end(b"A\r\nB: c\r\n"), None);
    }
    // spec: specs/ap/run-limits.md#R18
    #[test]
    fn literals_ranges_and_lists() {
        assert!(literal("127.0.0.1") && literal("[::1]") && literal("[fd7a:115c:a1e0::1]"));
        assert!(!literal("api.anthropic.com") && !literal("1.2.3") && !literal("[x]"));
        for a in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fd7a:115c:a1e0::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:100.64.0.1",
        ] {
            assert!(forbidden(a.parse().unwrap()), "{a}");
        }
        for a in [
            "1.1.1.1",
            "100.128.0.1",
            "172.32.0.1",
            "160.79.104.10",
            "2607:6bc0::10",
            "::ffff:1.1.1.1",
        ] {
            assert!(!forbidden(a.parse().unwrap()), "{a}");
        }
        assert_eq!(names("reviewer"), ["api.anthropic.com"]);
        assert_eq!(names("writer"), ["api.anthropic.com"]);
        for role in ["implementer", "fixer", "verifier"] {
            assert_eq!(names(role).len(), 5);
        }
    }
}
