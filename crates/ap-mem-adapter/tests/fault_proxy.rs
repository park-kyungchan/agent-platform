#[path = "support/proxy.rs"]
mod proxy;
#[path = "support/replay.rs"]
mod replay;
use proxy::{Assumption, CrashPoint, FaultProxy, Frame, OwnedBackend, Proxy, Step};
use replay::{MentalModel, Replay, TraceStep, WireRequest};
use serde_json::{Value, json};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

const ROOT: &str = "/v1/default/banks/fixture-bank";
type State = Arc<Mutex<Replay>>;
fn backend(replay: Replay, publisher: Option<Child>) -> (OwnedBackend, State, Arc<AtomicBool>) {
    let state = Arc::new(Mutex::new(replay));
    let shared = state.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let incoming = listener.try_clone().unwrap();
    incoming.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let joined = Arc::new(AtomicBool::new(false));
    let finished = joined.clone();
    let worker = thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            let (mut stream, peer) = match incoming.accept() {
                Ok(pair) => pair,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => return Err(e),
            };
            assert!(peer.ip().is_loopback());
            let frame = Frame::read(&mut stream)?;
            let (method, path) = frame.target()?;
            let value = if frame.body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&frame.body).unwrap()
            };
            let reply = shared
                .lock()
                .unwrap()
                .apply(WireRequest::new(method, path, value))
                .map_err(|_| io::Error::other("recorded_backend_stop"))?;
            let body = if let Some(s) = reply.body.as_str() {
                s.as_bytes().to_vec()
            } else {
                serde_json::to_vec(&reply.body).unwrap()
            };
            let mut head = format!("HTTP/1.1 {} Fixture", reply.status);
            for (key, value) in reply.headers {
                head.push_str(&format!("\r\n{key}: {value}"));
            }
            Frame { head, body }.write(&mut stream)?;
        }
        finished.store(true, Ordering::SeqCst);
        Ok(())
    });
    (
        OwnedBackend::new(listener, stop, worker, publisher),
        state,
        joined,
    )
}
fn send(proxy: &Proxy, method: &str, target: &str, body: Value) -> io::Result<Frame> {
    let mut stream = TcpStream::connect_timeout(&proxy.address(), Duration::from_secs(2))?;
    Frame {
        head: format!("{method} {target} HTTP/1.1\r\nHost: ignored.example"),
        body: if body.is_null() {
            Vec::new()
        } else {
            serde_json::to_vec(&body).unwrap()
        },
    }
    .write(&mut stream)?;
    Frame::read(&mut stream)
}
fn retain() -> Value {
    json!({"operation_id":"fixture-proxy-operation","async":true,"items":[{"document_id":"fixture-proxy-document","content":"invented PROXY_PRIVATE_BODY","tags":["fixture-proxy-tag"],"update_mode":"replace"}]})
}
fn models() -> Replay {
    Replay::with_models(vec![MentalModel {
        id: "fixture-proxy-model".into(),
        name: "invented proxy model".into(),
        tags: vec!["fixture-proxy-tag".into()],
    }])
}
fn assumption(case: &str) -> Assumption {
    Assumption::new(
        case,
        "Unmeasured target behaviour; this fault is not native operation/finality evidence.",
    )
    .unwrap()
}

#[test]
fn before_forward_no_effect_in_target() {
    let (owned, state, joined) = backend(Replay::default(), None);
    let mut proxy = FaultProxy::start(owned, vec![Step::RefuseBeforeForward]);
    let reply = send(&proxy, "POST", &format!("{ROOT}/memories"), retain()).unwrap();
    assert!(reply.head.starts_with("HTTP/1.1 503 "));
    assert!(reply.body.is_empty());
    proxy.stop_and_join().unwrap();
    assert_eq!(state.lock().unwrap().active_operations(), 0);
    assert!(joined.load(Ordering::SeqCst));
    let events = proxy.transcript();
    assert_eq!(events.len(), 1);
    assert!(!events[0].forwarded);
    assert!(!events[0].response_received);
}

#[test]
fn forwarded_lost_answer() {
    let (owned, state, _) = backend(Replay::default(), None);
    let mut proxy = FaultProxy::start(owned, vec![Step::ForwardThenDropAnswer]);
    assert!(send(&proxy, "POST", &format!("{ROOT}/memories"), retain()).is_err());
    // A lost answer is not a target refusal: exactly one accepted intent exists.
    assert_eq!(state.lock().unwrap().active_operations(), 1);
    let lookup = send(
        &proxy,
        "GET",
        &format!("{ROOT}/operations?document_id=fixture-proxy-document"),
        Value::Null,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&lookup.body).unwrap()["total"],
        1
    );
    proxy.stop_and_join().unwrap();
    let events = proxy.transcript();
    assert_eq!(events.len(), 2);
    assert!(events[0].forwarded && events[0].response_received && events[0].dropped_answer);
    assert!(events[1].forwarded && !events[1].dropped_answer);
    let trace = format!("{events:?}");
    assert!(!trace.contains("PROXY_PRIVATE_BODY"));
    assert!(!trace.contains("ignored.example"));
}

#[test]
fn assumptions_explicit() {
    assert!(Assumption::new("", "missing case").is_err());
    assert!(Assumption::new("case", "").is_err());
    for (step, kind) in [
        (Step::GappedList(assumption("gapped_list")), "gapped_list"),
        (
            Step::ChangingList(assumption("changing_list")),
            "changing_list",
        ),
    ] {
        assert_eq!(step.label(), format!("assumption:{kind}"));
        let (owned, _, _) = backend(models(), None);
        let mut proxy = FaultProxy::start(owned, vec![step]);
        let first: Value = serde_json::from_slice(
            &send(&proxy, "GET", &format!("{ROOT}/mental-models"), Value::Null)
                .unwrap()
                .body,
        )
        .unwrap();
        let second: Value = serde_json::from_slice(
            &send(&proxy, "GET", &format!("{ROOT}/mental-models"), Value::Null)
                .unwrap()
                .body,
        )
        .unwrap();
        assert_eq!(first["items"], json!([]));
        assert_eq!(first["total"], if kind == "gapped_list" { 1 } else { 0 });
        assert_eq!(second["total"], 1);
        assert_eq!(second["items"].as_array().unwrap().len(), 1);
        proxy.stop_and_join().unwrap();
        assert_eq!(proxy.transcript()[0].script, format!("assumption:{kind}"));
    }
    let a = assumption("terminal_failure");
    assert!(!a.limitation().is_empty());
    let (owned, state, _) = backend(Replay::default(), None);
    let mut proxy = FaultProxy::start(owned, vec![Step::Pass, Step::TerminalFailure(a)]);
    send(&proxy, "POST", &format!("{ROOT}/memories"), retain()).unwrap();
    state
        .lock()
        .unwrap()
        .advance(TraceStep::ParentPendingChildError {
            operation_id: "fixture-proxy-operation".into(),
            child_id: "fixture-proxy-child".into(),
        });
    let route = format!("{ROOT}/operations/fixture-proxy-operation?include_payload=false");
    let fault: Value =
        serde_json::from_slice(&send(&proxy, "GET", &route, Value::Null).unwrap().body).unwrap();
    assert_eq!(fault["status"], "failed");
    assert_eq!(fault["error_message"], "assumption:terminal_failure");
    let native: Value =
        serde_json::from_slice(&send(&proxy, "GET", &route, Value::Null).unwrap().body).unwrap();
    assert_eq!(native["status"], "pending");
    assert_eq!(state.lock().unwrap().active_operations(), 1);
    proxy.stop_and_join().unwrap();
    assert_eq!(proxy.transcript()[1].script, "assumption:terminal_failure");
}

#[test]
fn proxy_loopback_join() {
    let (owned, _, joined) = backend(models(), None);
    {
        let mut proxy = FaultProxy::start(owned, vec![]);
        assert!(proxy.address().ip().is_loopback());
        assert_ne!(proxy.address().port(), 0);
        let reply = send(&proxy, "GET", &format!("{ROOT}/mental-models"), Value::Null).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&reply.body).unwrap()["total"],
            1
        );
        proxy.stop_and_join().unwrap();
        proxy.stop_and_join().unwrap();
    }
    assert!(joined.load(Ordering::SeqCst));
    let (owned, _, joined) = backend(models(), None);
    drop(FaultProxy::start(owned, vec![]));
    assert!(joined.load(Ordering::SeqCst));
}

#[test]
fn kill_only_owned_publisher_at_declared_point() {
    for point in [
        CrashPoint::BeforeForward,
        CrashPoint::AfterForward,
        CrashPoint::AfterReply,
    ] {
        let child = Command::new("/bin/sleep")
            .arg("30")
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (owned, state, joined) = backend(Replay::default(), Some(child));
        let mut proxy = FaultProxy::start(owned, vec![Step::KillPublisherAt(point)]);
        let reply = send(&proxy, "POST", &format!("{ROOT}/memories"), retain());
        assert_eq!(reply.is_ok(), point == CrashPoint::AfterReply);
        proxy.stop_and_join().unwrap();
        assert!(joined.load(Ordering::SeqCst));
        assert_eq!(
            state.lock().unwrap().active_operations(),
            usize::from(point != CrashPoint::BeforeForward)
        );
        let events = proxy.transcript();
        assert_eq!(events[0].crash, Some(point));
        assert_eq!(events[0].forwarded, point != CrashPoint::BeforeForward);
    }
}

#[test]
fn request_cannot_retarget_backend() {
    let (owned, state, joined) = backend(Replay::default(), None);
    let mut proxy = FaultProxy::start(owned, vec![]);
    assert!(send(&proxy, "GET", "http://unowned.invalid/", Value::Null).is_err());
    assert!(proxy.stop_and_join().is_err());
    assert!(joined.load(Ordering::SeqCst));
    assert_eq!(state.lock().unwrap().active_operations(), 0);
    assert!(proxy.transcript().is_empty());
}
