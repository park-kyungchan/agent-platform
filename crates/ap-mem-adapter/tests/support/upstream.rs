//! Rules for extensions: EVERY unverified upstream choice belongs to Behaviour,
//! has a hostile Default and a handler contrast test. ALL assigned identifiers
//! come from opaque_id(); never derive them from caller bytes or a counter.
//! Fixed HTTP framing and test_* aliases are test transport, not native bindings.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
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
    pub delete_immediate: bool, // U5: default false; acknowledgement precedes completion.
    pub absent_delete_ok: bool, // U5: default false; an absent DELETE returns 404.
    pub echo_delete: bool,      // U5: default false; acknowledgement does not echo caller id.
    pub clean_deleted_facts: bool, // U5: default false; fact residue remains.
    pub clean_deleted_pages: bool, // U15: default false; page-source membership remains.
    pub exact_retained_body: bool, // U4: default false; retain normalizes/changes body bytes.
    pub replace_tags: bool,     // U4/U13: default false; old tags survive replacement.
    pub replace_facts: bool,    // U4/U10: default false; extraction/fact projection stays old.
    pub last_finish_wins: bool, // U14: default true; an old operation/job may overwrite newer work.
    pub page_deduplicate: bool, // U6: default false; create retry duplicates.
    pub reject_orphan: bool,    // U6: default false; orphan parent accepted silently.
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
    pub pending_deletes: BTreeSet<String>,
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
            pending_deletes: BTreeSet::new(),
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
        assert!(self.pending_deletes.remove(id));
        self.delete_projection(id);
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
        if success
            && !self.behaviour.refresh_keeps_old
            && self.may_apply(
                format!("page:{page_id}"),
                job["test_order"].as_u64().unwrap(),
            )
        {
            let page = self.pages.get_mut(page_id).unwrap();
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

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

pub struct Server {
    address: SocketAddr,
    model: Arc<Mutex<Model>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(fixture: Value) -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let model = Arc::new(Mutex::new(Model::new(fixture)));
        let stop = Arc::new(AtomicBool::new(false));
        let (data, stopping) = (Arc::clone(&model), Arc::clone(&stop));
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, peer)) => {
                        assert!(peer.ip().is_loopback());
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let (reply, disconnect, malformed) = match read_request(&mut stream) {
                            Ok(request) => {
                                let mut model = data.lock().unwrap();
                                let malformed =
                                    model.behaviour.malformed_reads && is_read(&request);
                                let reply = route(&mut model, request);
                                (reply, std::mem::take(&mut model.disconnect), malformed)
                            }
                            Err(_) => (
                                Reply {
                                    status: 400,
                                    body: json!({"error":"test_bad_request"}),
                                },
                                false,
                                false,
                            ),
                        };
                        if !disconnect {
                            let _ = write_reply(&mut stream, reply, malformed);
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("synthetic accept: {e}"),
                }
            }
        });
        Self {
            address,
            model,
            stop,
            worker: Some(worker),
        }
    }

    pub fn stop_and_join(&mut self) -> bool {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .is_none_or(|worker| worker.join().is_ok())
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn edit<T>(&self, f: impl FnOnce(&mut Model) -> T) -> T {
        f(&mut self.model.lock().unwrap())
    }

    // There is no URL/host argument: every client call targets this owned listener.
    pub fn request(&self, method: &str, path: &str, body: Value) -> io::Result<Reply> {
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(1))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;
        let payload = serde_json::to_vec(&body)?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.address,
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
        Ok(Reply {
            status,
            body: serde_json::from_slice(&raw[split + 4..])?,
        })
    }

    pub fn mcp(&self, name: &str, args: Value) -> Value {
        if self.edit(|m| m.behaviour.require_initialize && !m.initialized) {
            let reply = self
                .request(
                    "POST",
                    MCP_PATH,
                    json!({"jsonrpc":"2.0","id":0,"method":"initialize"}),
                )
                .unwrap();
            assert_eq!(reply.status, 200);
        }
        let reply = self.request("POST", MCP_PATH, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
        assert_eq!(reply.status, 200);
        assert!(reply.body.get("error").is_none(), "{}", reply.body);
        reply.body["result"].clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let ok = self.stop_and_join();
        assert!(ok || thread::panicking(), "synthetic worker panicked");
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
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
    if parts.len() != 3 || parts[2] != "HTTP/1.1" {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let headers: Vec<_> = lines.filter_map(|line| line.split_once(':')).collect();
    if headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("transfer-encoding"))
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let lengths: Vec<_> = headers
        .into_iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>())
        .collect();
    let length = match lengths.as_slice() {
        [] => 0,
        [Ok(n)] if *n <= LIMIT => *n,
        _ => return Err(io::ErrorKind::InvalidData.into()),
    };
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    Ok(Request {
        method: parts[0].into(),
        path: parts[1].into(),
        body: if body.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&body)?
        },
    })
}

fn write_reply(stream: &mut TcpStream, reply: Reply, malformed: bool) -> io::Result<()> {
    let body = if malformed {
        b"{".to_vec()
    } else {
        serde_json::to_vec(&reply.body)?
    };
    write!(
        stream,
        "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        body.len()
    )?;
    stream.write_all(&body)
}

fn unknown(model: &Model) -> Reply {
    Reply {
        status: if model.behaviour.reject_unknown {
            404
        } else {
            200
        },
        body: Value::Null,
    }
}
fn read_reply(model: &Model, value: Value) -> Reply {
    Reply {
        status: if value.is_null() && model.behaviour.absent_read_404 {
            404
        } else {
            200
        },
        body: model.view(value),
    }
}
fn is_read(request: &Request) -> bool {
    request.method == "GET"
        || (request.method == "POST"
            && request.path == MCP_PATH
            && matches!(
                request.body["method"].as_str(),
                Some("tools/call" | "tools/list")
            ))
}
fn route(model: &mut Model, request: Request) -> Reply {
    model.requests.push(request.clone());
    if is_read(&request) {
        match model.behaviour.fault.take() {
            Some(MutationFault::BeforeAccept) => {
                return Reply {
                    status: 503,
                    body: Value::Null,
                };
            }
            Some(MutationFault::LostAck) => model.disconnect = true,
            None => {}
        }
    }
    if request.method == "GET" {
        if request.path == "/config" {
            return if model.behaviour.root_routes {
                Reply {
                    status: 200,
                    body: if model.behaviour.config_resolved {
                        model.fixture["config"].clone()
                    } else {
                        json!({})
                    },
                }
            } else {
                Reply {
                    status: 404,
                    body: Value::Null,
                }
            };
        }
        for (prefix, jobs) in [
            ("/__test__/operation/", &model.operations),
            ("/__test__/page-job/", &model.page_jobs),
        ] {
            if let Some(id) = request.path.strip_prefix(prefix) {
                return if model.behaviour.observable_operations {
                    read_reply(model, jobs.get(id).cloned().unwrap_or(Value::Null))
                } else {
                    Reply {
                        status: 404,
                        body: Value::Null,
                    }
                };
            }
        }
        if let Some(id) = request.path.strip_prefix("/__test__/document/") {
            return read_reply(model, model.fixture["documents"][id].clone());
        }
        if request.path == "/__test__/facts" {
            return read_reply(model, model.fixture["facts"].clone());
        }
    }
    if request.method == "POST" && request.path == "/memories" {
        return if model.behaviour.root_routes {
            mutate(model, &request, None)
        } else {
            Reply {
                status: 404,
                body: Value::Null,
            }
        };
    }
    if request.method == "DELETE"
        && let Some(id) = request.path.strip_prefix("/__test__/document/")
    {
        return mutate(model, &request, Some(id));
    }
    if let Some(reply) = page_route(model, &request) {
        return reply;
    }
    if request.method != "POST" || request.path != MCP_PATH {
        return unknown(model);
    }
    let id = request.body["id"].clone();
    let method = request.body["method"].as_str();
    if method == Some("initialize") {
        model.initialized = true;
    } else if model.behaviour.require_initialize && !model.initialized {
        return Reply {
            status: 409,
            body: json!({"error":"test_initialize_required"}),
        };
    }
    let result = match method {
        Some("initialize") => json!({"protocolVersion":"test-only"}),
        Some("tools/list") => json!({"tools":model.fixture["tool_names"]}),
        Some("tools/call") => {
            let name = request.body["params"]["name"].as_str().unwrap_or("");
            let args = &request.body["params"]["arguments"];
            let result = match name {
                "get_document" => model.fixture["documents"]
                    [args["test_document_id"].as_str().unwrap_or("")]
                .clone(),
                "get_knowledge_page" if args["test_page_id"].is_string() => model
                    .pages
                    .get(args["test_page_id"].as_str().unwrap())
                    .cloned()
                    .unwrap_or(Value::Null),
                "get_knowledge_base_tree" if args["test_live_tree"] == true => {
                    let mut pages = model.pages.clone();
                    if !model.behaviour.tree_bodies {
                        for page in pages.values_mut() {
                            if let Some(map) = page.as_object_mut() {
                                map.remove("body");
                            }
                        }
                    }
                    json!({"folders":model.folders,"pages":pages})
                }
                _ => {
                    let selected = match name {
                        "recall" if model.behaviour.recall_current => {
                            Some(json!({"facts":model.fixture["facts"]}))
                        }
                        "search_knowledge_base" if model.behaviour.search_current => Some(
                            json!({"pages":model.pages.iter().map(|(id,p)|json!({"page_id":id,"snippet":p["body"],"document_ids":p["document_ids"]})).collect::<Vec<_>>()}),
                        ),
                        _ => model.fixture["responses"].get(name).cloned(),
                    };
                    let Some(mut response) = selected else {
                        return if model.behaviour.reject_unknown {
                            Reply {
                                status: 200,
                                body: json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"test_unknown_tool"}}),
                            }
                        } else {
                            Reply {
                                status: 200,
                                body: json!({"jsonrpc":"2.0","id":id,"result":null}),
                            }
                        };
                    };
                    if model.behaviour.honor_retrieval_args
                        && let Some(doc) = args["test_document_id"].as_str()
                    {
                        for key in ["facts", "pages"] {
                            if let Some(items) = response.get_mut(key).and_then(Value::as_array_mut)
                            {
                                items.retain(|v| {
                                    v["document_ids"]
                                        .as_array()
                                        .is_some_and(|ids| ids.contains(&json!(doc)))
                                });
                            }
                        }
                    }
                    response
                }
            };
            if result.is_null() && model.behaviour.absent_read_404 {
                return Reply {
                    status: 200,
                    body: json!({"jsonrpc":"2.0","id":id,"error":{"code":-32004,"message":"test_absent"}}),
                };
            }
            model.view(result)
        }
        _ => return unknown(model),
    };
    Reply {
        status: 200,
        body: json!({"jsonrpc":"2.0","id":id,"result":result}),
    }
}

fn mutate(model: &mut Model, request: &Request, delete: Option<&str>) -> Reply {
    let fault = model.behaviour.fault.take();
    if fault == Some(MutationFault::BeforeAccept) {
        return Reply {
            status: 503,
            body: Value::Null,
        };
    }
    let reply = if let Some(id) = delete {
        if model.fixture["documents"][id].is_null() && !model.behaviour.absent_delete_ok {
            return Reply {
                status: 404,
                body: Value::Null,
            };
        }
        if model.behaviour.delete_immediate {
            model.delete_projection(id);
        } else {
            model.pending_deletes.insert(id.to_owned());
        }
        Reply {
            status: 200,
            body: if model.behaviour.echo_delete {
                json!({"test_deleted":id})
            } else {
                json!({"test_accepted":true})
            },
        }
    } else {
        let body = &request.body;
        if model.behaviour.validate_shape
            && (!body["test_operation_id"].is_string()
                || !body["document"]["document_id"].is_string()
                || !body["document"]["body"].is_string()
                || !body["document"]["tags"].is_array()
                || body["async"] != true
                || body["update_mode"] != "replace")
        {
            return Reply {
                status: 400,
                body: Value::Null,
            };
        }
        let correlation = body["test_operation_id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(opaque_id);
        let old_id = model.retains.get(&correlation).cloned();
        let mismatch = old_id
            .as_ref()
            .is_some_and(|id| model.operations[id]["request"] != *body);
        if mismatch && model.behaviour.reject_mismatch {
            return Reply {
                status: 409,
                body: Value::Null,
            };
        }
        let key = if model.behaviour.deduplicate {
            old_id.clone().unwrap_or_else(opaque_id)
        } else {
            opaque_id()
        };
        if !model.operations.contains_key(&key) || (mismatch && model.behaviour.restart_mismatch) {
            let order = model.order();
            model.operations.insert(key.clone(), json!({"test_operation_id":key,"test_order":order,"test_state":"running","request":body}));
        }
        model.retains.entry(correlation).or_insert(key.clone());
        Reply {
            status: 202,
            body: json!({"test_operation_id":key}),
        }
    };
    model.disconnect = fault == Some(MutationFault::LostAck);
    reply
}

fn page_route(model: &mut Model, request: &Request) -> Option<Reply> {
    if request.method != "POST" {
        return None;
    }
    let folder = request.path == "/knowledge-base/folders";
    let page = request.path == "/knowledge-base/pages";
    let refresh = request
        .path
        .strip_prefix("/mental-models/")
        .and_then(|p| p.strip_suffix("/refresh"));
    if !folder && !page && refresh.is_none() {
        return None;
    }
    let fault = model.behaviour.fault.take();
    if fault == Some(MutationFault::BeforeAccept) {
        return Some(Reply {
            status: 503,
            body: Value::Null,
        });
    }
    let reply = if let Some(id) = refresh {
        if !model.pages.contains_key(id) && model.behaviour.absent_read_404 {
            return Some(Reply {
                status: 404,
                body: Value::Null,
            });
        }
        if request.body != json!({"mode":"full"}) && model.behaviour.validate_shape {
            return Some(Reply {
                status: 400,
                body: Value::Null,
            });
        }
        let key = opaque_id();
        let order = model.order();
        model.page_jobs.insert(key.clone(), json!({"test_job_id":key,"test_page_id":id,"test_order":order,"test_state":"running","test_target":model.fixture["page_refresh_result"]}));
        Reply {
            status: 202,
            body: json!({"test_job_id":key}),
        }
    } else {
        if !request.body["test_id"].is_string() && model.behaviour.validate_shape {
            return Some(Reply {
                status: 400,
                body: Value::Null,
            });
        }
        if page
            && model.behaviour.reject_orphan
            && !model
                .folders
                .contains_key(request.body["test_parent_id"].as_str().unwrap_or(""))
        {
            return Some(Reply {
                status: 409,
                body: Value::Null,
            });
        }
        let intent = (
            folder,
            request.body["test_id"].as_str().unwrap_or("").to_owned(),
        );
        let old = model.creates.get(&intent).cloned();
        let key = if model.behaviour.page_deduplicate {
            old.unwrap_or_else(opaque_id)
        } else {
            opaque_id()
        };
        let mut value = if model.behaviour.echo_created {
            request.body.clone()
        } else {
            model.fixture[if folder {
                "folder_response"
            } else {
                "page_response"
            }]
            .clone()
        };
        if !value.is_object() {
            value = json!({"test_unverified":true});
        }
        value["test_id"] = json!(key);
        let entries = if folder {
            &mut model.folders
        } else {
            &mut model.pages
        };
        let value = entries.entry(key.clone()).or_insert(value).clone();
        model.creates.entry(intent).or_insert(key.clone());
        Reply {
            status: 201,
            body: json!({"test_id":key,"test_value":value}),
        }
    };
    model.disconnect = fault == Some(MutationFault::LostAck);
    Some(reply)
}
