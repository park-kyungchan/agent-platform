#![allow(dead_code)] // Legacy state is removed in R1g; never served by this listener.
//! Rules for extensions: EVERY unverified upstream choice belongs to Behaviour,
//! has a hostile Default and a handler contrast test. ALL assigned identifiers
//! come from opaque_id(); never derive them from caller bytes or a counter.
//! Fixed HTTP framing and test_* aliases are test transport, not native bindings.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const MCP_PATH: &str = "/mcp/coding-agent::engineering/";
const LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Value,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MutationFault {
    BeforeAccept,
    LostAck,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Behaviour {
    pub recall_current: bool, // U10: default false; recall index stays at old snapshot.
    pub search_current: bool, // U10: default false; page-search index stays at old snapshot.
    pub foreign_provenance: bool, // U2/U4: default true; attributed ids are foreign.
    pub malformed_reads: bool, // U1/U2/U8: default true; read response bytes are invalid JSON.
    pub require_initialize: bool, // U1: default true; bare MCP is refused.
    pub root_routes: bool,    // U1: default false; unverified root HTTP routes unavailable.
    pub reject_unknown: bool, // U1/U2: default true; unknown routes/tools refused.
    pub validate_shape: bool, // U4/U6/U12/U13: default false; malformed input accepted.
    pub provenance: bool,     // U2: default false; no complete document_ids attribution.
    pub complete_reads: bool, // U8: default false; partial body/list responses.
    pub honor_retrieval_args: bool, // U2/U8: default false; recall/search ignore selectors.
    pub config_resolved: bool, // U11: default false; config omits resolved values.
    pub observable_operations: bool, // U3/U14: default false; jobs cannot be inspected.
    pub absent_read_404: bool, // U4/U7: default true; absent reads fail.
    pub deduplicate: bool,    // U3: default false; resend creates another opaque operation.
    pub reject_mismatch: bool, // U3: default false. With dedup on, changed bytes are
    // silently IGNORED unless restart_mismatch is selected: stale intent is acknowledged,
    // the least favourable outcome for a publisher expecting its new bytes to apply.
    pub restart_mismatch: bool, // U3/U14: default false; finished work never restarts implicitly.
    pub deduplicate_deletes: bool, // U5: default false; every pending retry is a separate opaque job.
    pub missing_refresh_succeeds: bool, // U7: default true; a missing result may still report success.
    pub delete_immediate: bool,         // U5: default false; acknowledgement precedes completion.
    pub absent_delete_ok: bool,         // U5: default false; an absent DELETE returns 404.
    pub echo_delete: bool, // U5: default false; acknowledgement does not echo caller id.
    pub clean_deleted_facts: bool, // U5: default false; fact residue remains.
    pub clean_deleted_pages: bool, // U15: default false; page-source membership remains.
    pub exact_retained_body: bool, // U4: default false; retain normalizes/changes body bytes.
    pub replace_tags: bool, // U4/U13: default false; old tags survive replacement.
    pub replace_facts: bool, // U4/U10: default false; extraction/fact projection stays old.
    pub last_finish_wins: bool, // U14: default true; an old operation/job may overwrite newer work.
    pub page_deduplicate: bool, // U6: default false; create retry duplicates.
    pub reject_orphan: bool, // U6: default false; orphan parent accepted silently.
    pub echo_created: bool, // U6: default false; page/folder metadata comes from a separate fixture.
    pub refresh_keeps_old: bool, // U7: default true; successful refresh can leave stale text.
    pub tree_bodies: bool,  // U6/U8: default false; navigation need not include page bodies.
    pub fault: Option<MutationFault>, // U9/U14: default LostAck; uncertain acceptance before any retry.
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            recall_current: false,
            search_current: false,
            foreign_provenance: true,
            malformed_reads: true,
            require_initialize: true,
            root_routes: false,
            reject_unknown: true,
            validate_shape: false,
            provenance: false,
            complete_reads: false,
            honor_retrieval_args: false,
            config_resolved: false,
            observable_operations: false,
            absent_read_404: true,
            deduplicate: false,
            reject_mismatch: false,
            restart_mismatch: false,
            deduplicate_deletes: false,
            missing_refresh_succeeds: true,
            delete_immediate: false,
            absent_delete_ok: false,
            echo_delete: false,
            clean_deleted_facts: false,
            clean_deleted_pages: false,
            exact_retained_body: false,
            replace_tags: false,
            replace_facts: false,
            last_finish_wins: true,
            page_deduplicate: false,
            reject_orphan: false,
            echo_created: false,
            refresh_keeps_old: true,
            tree_bodies: false,
            fault: Some(MutationFault::LostAck),
        }
    }
}

/// Only source of assigned operation, job, folder, page and duplicate identities.
pub fn opaque_id() -> String {
    let mut bytes = [0_u8; 16];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut bytes)
        .unwrap();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Model {
    pub fixture: Value,
    pub behaviour: Behaviour,
    pub requests: Vec<Request>,
    pub operations: BTreeMap<String, Value>,
    pub pending_deletes: BTreeMap<String, Value>,
    pub folders: BTreeMap<String, Value>,
    pub pages: BTreeMap<String, Value>,
    pub page_jobs: BTreeMap<String, Value>,
    initialized: bool,
    disconnect: bool,
    retains: BTreeMap<String, String>,
    creates: BTreeMap<(bool, String), String>,
    applied_order: BTreeMap<String, u64>,
    next_order: u64,
}

impl Model {
    fn new(fixture: Value) -> Self {
        Self {
            fixture,
            behaviour: Behaviour::default(),
            requests: Vec::new(),
            operations: BTreeMap::new(),
            pending_deletes: BTreeMap::new(),
            folders: BTreeMap::new(),
            pages: BTreeMap::new(),
            page_jobs: BTreeMap::new(),
            initialized: false,
            disconnect: false,
            retains: BTreeMap::new(),
            creates: BTreeMap::new(),
            applied_order: BTreeMap::new(),
            next_order: 0,
        }
    }
    fn order(&mut self) -> u64 {
        self.next_order += 1;
        self.next_order
    }
    fn may_apply(&mut self, target: String, order: u64) -> bool {
        if !self.behaviour.last_finish_wins
            && self.applied_order.get(&target).is_some_and(|n| *n > order)
        {
            return false;
        }
        self.applied_order.insert(target, order);
        true
    }
    #[allow(dead_code)]
    pub fn finish(&mut self, id: &str, success: bool) {
        let op = self.operations.get_mut(id).unwrap();
        assert_eq!(op["test_state"], "running");
        op["test_state"] = json!(if success { "succeeded" } else { "failed" });
        let mut doc = op["request"]["document"].clone();
        let order = op["test_order"].as_u64().unwrap();
        let Some(doc_id) = doc["document_id"].as_str().map(str::to_owned) else {
            return;
        };
        if !success || !self.may_apply(format!("document:{doc_id}"), order) {
            return;
        }
        if !self.behaviour.exact_retained_body {
            doc["body"] = json!("TEST_NORMALIZED_BODY");
        }
        if !self.behaviour.replace_tags {
            let mut tags = self.fixture["documents"][&doc_id]["tags"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            tags.extend(doc["tags"].as_array().cloned().unwrap_or_default());
            doc["tags"] = json!(tags);
        }
        self.fixture["documents"][&doc_id] = doc.clone();
        if self.behaviour.replace_facts {
            let facts = self.fixture["facts"].as_array_mut().unwrap();
            facts.retain(|f| {
                !f["document_ids"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(doc_id)))
            });
            facts.push(json!({"text":doc["body"],"document_ids":[doc_id]}));
        }
    }
    #[allow(dead_code)]
    pub fn finish_delete(&mut self, id: &str) {
        let delete = self.pending_deletes.remove(id).unwrap();
        let target = delete["document_id"].as_str().unwrap();
        if self.may_apply(
            format!("document:{target}"),
            delete["test_order"].as_u64().unwrap(),
        ) {
            self.delete_projection(target);
        }
    }
    fn delete_projection(&mut self, id: &str) {
        self.fixture["documents"]
            .as_object_mut()
            .unwrap()
            .remove(id);
        if self.behaviour.clean_deleted_facts {
            self.fixture["facts"].as_array_mut().unwrap().retain(|f| {
                !f["document_ids"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(id)))
            });
        }
        if self.behaviour.clean_deleted_pages {
            for page in self.pages.values_mut().chain(
                self.fixture["responses"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut(),
            ) {
                if let Some(ids) = page.get_mut("document_ids").and_then(Value::as_array_mut) {
                    ids.retain(|v| v != id);
                }
            }
        }
    }
    #[allow(dead_code)]
    pub fn finish_refresh(&mut self, id: &str, success: bool) {
        let job = self.page_jobs.get_mut(id).unwrap();
        assert_eq!(job["test_state"], "running");
        job["test_state"] = json!(if success { "succeeded" } else { "failed" });
        let job = job.clone();
        let page_id = job["test_page_id"].as_str().unwrap();
        if success && !self.pages.contains_key(page_id) {
            if !self.behaviour.missing_refresh_succeeds {
                self.page_jobs.get_mut(id).unwrap()["test_state"] = json!("failed");
            }
            return;
        }
        if success
            && !self.behaviour.refresh_keeps_old
            && self.may_apply(
                format!("page:{page_id}"),
                job["test_order"].as_u64().unwrap(),
            )
            && let Some(page) = self.pages.get_mut(page_id)
        {
            for key in ["body", "document_ids"] {
                page[key] = job["test_target"][key].clone();
            }
        }
    }
    fn view(&self, mut value: Value) -> Value {
        fn strip(value: &mut Value, provenance: bool, complete: bool, foreign: bool) {
            match value {
                Value::Object(map) => {
                    if !provenance {
                        map.remove("document_ids");
                    } else if foreign && map.contains_key("document_ids") {
                        map.insert("document_ids".into(), json!(["TEST_FOREIGN_SOURCE"]));
                    }
                    if !complete {
                        map.remove("body");
                        map.insert("test_partial".into(), json!(true));
                    }
                    for v in map.values_mut() {
                        strip(v, provenance, complete, foreign);
                    }
                }
                Value::Array(items) => {
                    if !complete {
                        items.truncate(1);
                    }
                    for v in items {
                        strip(v, provenance, complete, foreign);
                    }
                }
                _ => {}
            }
        }
        strip(
            &mut value,
            self.behaviour.provenance,
            self.behaviour.complete_reads,
            self.behaviour.foreign_provenance,
        );
        value
    }
}

use super::proxy::OwnedBackend;
use super::replay::{RecordedReply, Replay, TraceStep, WireRequest};

/// The listener owns only transport and a typed replay; no route builds JSON here.
pub struct Server {
    address: SocketAddr,
    listener: Option<TcpListener>,
    replay: Arc<Mutex<Replay>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
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
                let reply = data.lock().unwrap().apply(request).map_err(|_| {
                    io::Error::new(io::ErrorKind::Unsupported, "replay_stopped_without_reply")
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
        }
    }
    pub fn owned_address(&self) -> SocketAddr {
        self.address
    }
    pub fn advance(&self, step: TraceStep) {
        self.replay.lock().unwrap().advance(step);
    }
    pub fn stop_and_join(&mut self) -> io::Result<()> {
        if let Some(worker) = self.worker.take() {
            self.stop.store(true, Ordering::Release);
            self.listener.take();
            worker
                .join()
                .map_err(|_| io::Error::other("replay_worker_panicked"))??;
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
        || !(parts[1].starts_with("/v1/default/banks/fixture-bank/")
            || parts[1] == "/mcp/fixture-bank/")
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
