//! Owned loopback HTTP transport for independently checked recorded replay replies.
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const LIMIT: usize = 64 * 1024;

use super::proxy::OwnedBackend;
use super::replay::{RecordedReply, Replay, ReplayStop, ReplayStops, TraceStep, WireRequest};

/// The listener owns only transport and a typed replay; no route builds JSON here.
pub struct Server {
    address: SocketAddr,
    listener: Option<TcpListener>,
    replay: Arc<Mutex<Replay>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    replay_stops: ReplayStops,
}
impl Server {
    pub fn start(replay: Replay) -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let incoming = listener.try_clone().unwrap();
        incoming.set_nonblocking(true).unwrap();
        let replay = Arc::new(Mutex::new(replay));
        let stop = Arc::new(AtomicBool::new(false));
        let (data, stopping) = (replay.clone(), stop.clone());
        let replay_stops = ReplayStops::default();
        let failures = replay_stops.clone();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                let (mut stream, peer) = match incoming.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                assert!(peer.ip().is_loopback());
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                stream.set_write_timeout(Some(Duration::from_secs(1)))?;
                let Ok(request) = read_request(&mut stream) else {
                    continue;
                };
                let reply = data.lock().unwrap().apply(request).map_err(|error| {
                    failures.lock().unwrap().get_or_insert(error.clone());
                    io::Error::other(error)
                })?;
                write_reply(&mut stream, &reply)?;
            }
            Ok(())
        });
        Self {
            address,
            listener: Some(listener),
            replay,
            stop,
            worker: Some(worker),
            replay_stops,
        }
    }
    pub fn owned_address(&self) -> SocketAddr {
        self.address
    }
    pub fn advance(&self, step: TraceStep) {
        self.replay.lock().unwrap().advance(step);
    }
    pub fn replay_stop(&self) -> Option<ReplayStop> {
        self.replay_stops.lock().unwrap().clone()
    }
    pub fn stop_and_join(&mut self) -> io::Result<()> {
        if let Some(worker) = self.worker.take() {
            self.stop.store(true, Ordering::Release);
            self.listener.take();
            let joined = worker
                .join()
                .map_err(|_| io::Error::other("replay_worker_panicked"))
                .and_then(|r| r);
            if let Some(error) = self.replay_stop() {
                return Err(io::Error::other(error));
            }
            joined?;
        }
        Ok(())
    }
    /// Transfer the listener capability and worker, never a caller-supplied URL.
    pub fn into_owned_backend(mut self) -> OwnedBackend {
        OwnedBackend::new(
            self.listener.take().unwrap(),
            self.stop.clone(),
            self.worker.take().unwrap(),
            None,
            self.replay_stops.clone(),
        )
    }
    pub fn request(&self, request: WireRequest) -> io::Result<RecordedReply> {
        if self.worker.is_none() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        if !request.path.starts_with('/')
            || request.path.contains(['\r', '\n'])
            || !request.method.bytes().all(|c| c.is_ascii_uppercase())
            || request
                .session
                .as_deref()
                .is_some_and(|s| s.contains(['\r', '\n']))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let payload = if request.body.is_null() {
            Vec::new()
        } else {
            serde_json::to_vec(&request.body)?
        };
        if payload.len() > LIMIT {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(1))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;
        write!(
            stream,
            "{} {} HTTP/1.1\r\nHost: {}\r\n",
            request.method, request.path, self.address
        )?;
        if request.authorized {
            write!(stream, "Authorization: Bearer fixture-test-token\r\n")?;
        }
        if let Some(session) = request.session {
            write!(stream, "Mcp-Session-Id: {session}\r\n")?;
        }
        write!(
            stream,
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )?;
        stream.write_all(&payload)?;
        let mut raw = Vec::new();
        stream.take((LIMIT * 2) as u64).read_to_end(&mut raw)?;
        let split = raw
            .windows(4)
            .position(|p| p == b"\r\n\r\n")
            .ok_or(io::ErrorKind::UnexpectedEof)?;
        let header = std::str::from_utf8(&raw[..split]).map_err(|_| io::ErrorKind::InvalidData)?;
        let status = header
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or(io::ErrorKind::InvalidData)?;
        let headers: BTreeMap<String, String> = header
            .split("\r\n")
            .skip(1)
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        let body = if headers.get("content-type").map(String::as_str) == Some("text/event-stream") {
            Value::String(
                String::from_utf8(raw[split + 4..].to_vec())
                    .map_err(|_| io::ErrorKind::InvalidData)?,
            )
        } else {
            serde_json::from_slice(&raw[split + 4..])?
        };
        Ok(RecordedReply {
            status,
            headers,
            body,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let result = self.stop_and_join();
        if !thread::panicking() {
            result.expect("replay worker joined");
        }
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<WireRequest> {
    let started = std::time::Instant::now();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if started.elapsed() >= Duration::from_secs(1) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        header.push(byte[0]);
        if header.len() > 8192 {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
    let header = std::str::from_utf8(&header).map_err(|_| io::ErrorKind::InvalidData)?;
    let mut lines = header.split("\r\n");
    let parts: Vec<_> = lines.next().unwrap_or("").split_whitespace().collect();
    if parts.len() != 3
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with('/')
        || parts[1].starts_with("//")
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let headers: Vec<_> = lines.filter_map(|line| line.split_once(':')).collect();
    if headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("transfer-encoding"))
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let authorized = headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("authorization") && v.trim() == "Bearer fixture-test-token"
    });
    let session = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
        .map(|(_, v)| v.trim().to_owned());
    let lengths: Vec<_> = headers
        .into_iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.trim().parse::<usize>())
        .collect();
    let length = match lengths.as_slice() {
        [] => 0,
        [Ok(n)] if *n <= LIMIT => *n,
        _ => return Err(io::ErrorKind::InvalidData.into()),
    };
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    let mut request = WireRequest::new(
        parts[0],
        parts[1],
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body)?
        },
    );
    request.authorized = authorized;
    request.session = session;
    Ok(request)
}
fn write_reply(stream: &mut TcpStream, reply: &RecordedReply) -> io::Result<()> {
    let body = if reply.headers.get("content-type").map(String::as_str) == Some("text/event-stream")
    {
        reply
            .body
            .as_str()
            .ok_or(io::ErrorKind::InvalidData)?
            .as_bytes()
            .to_vec()
    } else {
        serde_json::to_vec(&reply.body)?
    };
    write!(stream, "HTTP/1.1 {} Recorded\r\n", reply.status)?;
    for (key, value) in &reply.headers {
        write!(stream, "{key}: {value}\r\n")?;
    }
    write!(
        stream,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}
