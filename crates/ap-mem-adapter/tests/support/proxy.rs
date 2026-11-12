//! Owned loopback transport faults. Assumptions never enter the nominal replay.
#![allow(dead_code)]
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Child;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(2);
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "bounded_proxy_frame")
}

/// Bounded HTTP/1.1 framing; headers/body remain in RAM, never in the transcript.
pub struct Frame {
    pub head: String,
    pub body: Vec<u8>,
}
impl Frame {
    pub fn read(stream: &mut TcpStream) -> io::Result<Self> {
        let end = Instant::now() + TIMEOUT;
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            if bytes.len() >= 16384 {
                return Err(invalid());
            }
            stream.set_read_timeout(Some(
                end.checked_duration_since(Instant::now())
                    .ok_or_else(invalid)?,
            ))?;
            let mut byte = [0];
            stream.read_exact(&mut byte)?;
            bytes.push(byte[0]);
        }
        let head = String::from_utf8(bytes[..bytes.len() - 4].to_vec()).map_err(|_| invalid())?;
        let mut length = None;
        for line in head.split("\r\n").skip(1) {
            let (key, value) = line.split_once(':').ok_or_else(invalid)?;
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Err(invalid());
            }
            if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err(invalid());
            }
            if key.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    return Err(invalid());
                }
                length = Some(value.trim().parse::<usize>().map_err(|_| invalid())?);
            }
        }
        let length = length.unwrap_or(0);
        if length > 65536 {
            return Err(invalid());
        }
        let mut body = vec![0; length];
        let mut pos = 0;
        while pos < length {
            stream.set_read_timeout(Some(
                end.checked_duration_since(Instant::now())
                    .ok_or_else(invalid)?,
            ))?;
            let n = stream.read(&mut body[pos..])?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            pos += n;
        }
        Ok(Self { head, body })
    }
    pub fn write(&self, stream: &mut TcpStream) -> io::Result<()> {
        stream.set_write_timeout(Some(TIMEOUT))?;
        for line in self.head.split("\r\n") {
            if line.split_once(':').is_some_and(|(k, _)| {
                k.eq_ignore_ascii_case("content-length") || k.eq_ignore_ascii_case("connection")
            }) {
                continue;
            }
            write!(stream, "{line}\r\n")?;
        }
        write!(
            stream,
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.body.len()
        )?;
        stream.write_all(&self.body)
    }
    pub fn target(&self) -> io::Result<(&str, &str)> {
        let parts: Vec<_> = self
            .head
            .split("\r\n")
            .next()
            .unwrap_or("")
            .split_ascii_whitespace()
            .collect();
        if parts.len() != 3
            || parts[2] != "HTTP/1.1"
            || !parts[1].starts_with('/')
            || parts[1].starts_with("//")
        {
            return Err(invalid());
        }
        Ok((parts[0], parts[1]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashPoint {
    BeforeForward,
    AfterForward,
    AfterReply,
}
#[derive(Clone, Debug)]
pub struct Assumption {
    tag: String,
    limitation: String,
}
impl Assumption {
    pub fn new(case: &str, limitation: &str) -> Result<Self, &'static str> {
        if case.is_empty()
            || !case.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || limitation.trim().is_empty()
        {
            return Err("assumption_case_and_limitation_required");
        }
        Ok(Self {
            tag: format!("assumption:{case}"),
            limitation: limitation.into(),
        })
    }
    pub fn tag(&self) -> &str {
        &self.tag
    }
    pub fn limitation(&self) -> &str {
        &self.limitation
    }
}
pub enum Step {
    Pass,
    RefuseBeforeForward,
    ForwardThenDropAnswer,
    KillPublisherAt(CrashPoint),
    TerminalFailure(Assumption),
    GappedList(Assumption),
    ChangingList(Assumption),
}
impl Step {
    pub fn label(&self) -> &str {
        match self {
            Self::Pass => "transport:pass",
            Self::RefuseBeforeForward => "transport:refuse_before_forward",
            Self::ForwardThenDropAnswer => "transport:forward_then_drop_answer",
            Self::KillPublisherAt(_) => "transport:kill_owned_publisher",
            Self::TerminalFailure(a) | Self::GappedList(a) | Self::ChangingList(a) => a.tag(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ForwardEvent {
    pub script: String,
    pub forwarded: bool,
    pub response_received: bool,
    pub dropped_answer: bool,
    pub crash: Option<CrashPoint>,
}
/// A bound listener token plus its worker/stop owns the destination, not an address
/// supplied in a request. An optional Child permits killing only an owned process.
pub struct OwnedBackend {
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    publisher: Option<Child>,
}
impl OwnedBackend {
    pub fn new(
        listener: TcpListener,
        stop: Arc<AtomicBool>,
        worker: JoinHandle<io::Result<()>>,
        publisher: Option<Child>,
    ) -> Self {
        assert!(listener.local_addr().unwrap().ip().is_loopback());
        Self {
            listener,
            stop,
            worker: Some(worker),
            publisher,
        }
    }
    fn address(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }
    fn kill_publisher(&mut self) -> io::Result<()> {
        let child = self.publisher.as_mut().ok_or_else(invalid)?;
        if child.try_wait()?.is_some() {
            return Err(invalid());
        }
        child.kill()?;
        child.wait()?;
        Ok(())
    }
    fn finish(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(child) = self.publisher.as_mut() {
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
        }
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| invalid())??;
        }
        Ok(())
    }
}
impl Drop for OwnedBackend {
    fn drop(&mut self) {
        let result = self.finish();
        if !thread::panicking() {
            result.expect("owned backend joined");
        }
    }
}
pub struct FaultProxy;
pub struct Proxy {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    events: Arc<Mutex<Vec<ForwardEvent>>>,
}
impl FaultProxy {
    pub fn start(mut owned_backend: OwnedBackend, steps: Vec<Step>) -> Proxy {
        assert!(
            !steps.iter().any(|s| matches!(s, Step::KillPublisherAt(_)))
                || owned_backend.publisher.is_some()
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let trace = events.clone();
        let worker = thread::spawn(move || {
            let mut steps: VecDeque<_> = steps.into();
            while !done.load(Ordering::SeqCst) {
                let (mut client, peer) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                assert!(peer.ip().is_loopback());
                let request = Frame::read(&mut client)?;
                request.target()?;
                let step = steps.pop_front().unwrap_or(Step::Pass);
                let crash = if let Step::KillPublisherAt(point) = &step {
                    Some(*point)
                } else {
                    None
                };
                let mut event = ForwardEvent {
                    script: step.label().into(),
                    forwarded: false,
                    response_received: false,
                    dropped_answer: false,
                    crash,
                };
                if matches!(step, Step::RefuseBeforeForward) {
                    Frame {
                        head: "HTTP/1.1 503 Proxy Refusal".into(),
                        body: Vec::new(),
                    }
                    .write(&mut client)?;
                } else if crash == Some(CrashPoint::BeforeForward) {
                    owned_backend.kill_publisher()?;
                    event.dropped_answer = true;
                } else {
                    // Only this owned listener's address is ever connected; Host and
                    // request-target cannot redirect, retry, or select another backend.
                    let mut upstream =
                        TcpStream::connect_timeout(&owned_backend.address(), TIMEOUT)?;
                    request.write(&mut upstream)?;
                    event.forwarded = true;
                    let mut response = Frame::read(&mut upstream)?;
                    event.response_received = true;
                    if matches!(step, Step::ForwardThenDropAnswer) {
                        event.dropped_answer = true;
                    } else if crash == Some(CrashPoint::AfterForward) {
                        owned_backend.kill_publisher()?;
                        event.dropped_answer = true;
                    } else {
                        match &step {
                            Step::TerminalFailure(a)
                            | Step::GappedList(a)
                            | Step::ChangingList(a) => {
                                // These are explicitly unmeasured fault replies, never
                                // nominal answers or evidence about native finality.
                                let mut value: Value = serde_json::from_slice(&response.body)
                                    .map_err(|_| invalid())?;
                                match &step {
                                    Step::TerminalFailure(_) => {
                                        if value.get("status").is_none()
                                            || value.get("error_message").is_none()
                                        {
                                            return Err(invalid());
                                        }
                                        value["status"] = json!("failed");
                                        value["error_message"] = json!(a.tag());
                                    }
                                    Step::GappedList(_) => {
                                        value["items"].as_array_mut().ok_or_else(invalid)?.pop();
                                    }
                                    Step::ChangingList(_) => {
                                        value["items"].as_array_mut().ok_or_else(invalid)?.clear();
                                        if !value["total"].is_number() {
                                            return Err(invalid());
                                        }
                                        value["total"] = json!(0);
                                    }
                                    _ => unreachable!(),
                                }
                                response.body =
                                    serde_json::to_vec(&value).map_err(|_| invalid())?;
                            }
                            _ => {}
                        }
                        response.write(&mut client)?;
                        if crash == Some(CrashPoint::AfterReply) {
                            owned_backend.kill_publisher()?;
                        }
                    }
                }
                trace.lock().unwrap().push(event);
            }
            owned_backend.finish()
        });
        Proxy {
            address,
            stop,
            worker: Some(worker),
            events,
        }
    }
}
impl Proxy {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn transcript(&self) -> Vec<ForwardEvent> {
        self.events.lock().unwrap().clone()
    }
    pub fn stop_and_join(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| invalid())??;
        }
        Ok(())
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        let result = self.stop_and_join();
        if !thread::panicking() {
            result.expect("proxy joined");
        }
    }
}
