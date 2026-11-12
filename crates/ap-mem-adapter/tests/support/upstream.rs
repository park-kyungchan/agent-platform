//! Synthetic wire only: /__test__ aliases are deliberately not native bindings.
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

// Controls are consumed by distinct integration-test binaries.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum MutationFault {
    BeforeAccept,
    LostAck,
}

pub struct Model {
    pub fixture: Value,
    pub requests: Vec<Request>,
    pub operations: BTreeMap<String, Value>,
    pub next_fault: Option<MutationFault>,
    disconnect: bool,
}

impl Model {
    #[allow(dead_code)]
    pub fn finish(&mut self, operation_id: &str, success: bool) {
        let op = self.operations.get_mut(operation_id).unwrap();
        assert_eq!(op["test_state"], "running");
        op["test_state"] = json!(if success { "succeeded" } else { "failed" });
        if success {
            let document = op["request"]["document"].clone();
            let id = document["document_id"].as_str().unwrap().to_owned();
            self.fixture["documents"][&id] = document.clone();
            // Recall and page-search indexes deliberately remain independent.
            let facts = self.fixture["facts"].as_array_mut().unwrap();
            facts.retain(|fact| {
                !fact["document_ids"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(id)))
            });
            facts.push(json!({"text":document["body"],"document_ids":[id]}));
        }
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
        let model = Arc::new(Mutex::new(Model {
            fixture,
            requests: Vec::new(),
            operations: BTreeMap::new(),
            next_fault: None,
            disconnect: false,
        }));
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
                        let (reply, disconnect) = match read_request(&mut stream) {
                            Ok(request) => {
                                let mut model = data.lock().unwrap();
                                let reply = route(&mut model, request);
                                (reply, std::mem::take(&mut model.disconnect))
                            }
                            Err(_) => (
                                Reply {
                                    status: 400,
                                    body: json!({"error":"test_bad_request"}),
                                },
                                false,
                            ),
                        };
                        if !disconnect {
                            let _ = write_reply(&mut stream, reply);
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
        let reply = self.request("POST", MCP_PATH, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
        assert_eq!(reply.status, 200);
        assert!(reply.body.get("error").is_none(), "{}", reply.body);
        reply.body["result"].clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let result = self.worker.take().unwrap().join();
        if !thread::panicking() {
            result.unwrap();
        }
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
    let lengths: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
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

fn write_reply(stream: &mut TcpStream, reply: Reply) -> io::Result<()> {
    let body = serde_json::to_vec(&reply.body)?;
    write!(
        stream,
        "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        body.len()
    )?;
    stream.write_all(&body)
}

fn route(model: &mut Model, request: Request) -> Reply {
    model.requests.push(request.clone());
    let missing = || Reply {
        status: 404,
        body: json!({"error":"test_not_found"}),
    };
    if request.method == "GET" {
        if request.path == "/config" {
            return Reply {
                status: 200,
                body: model.fixture["config"].clone(),
            };
        }
        if let Some(id) = request.path.strip_prefix("/__test__/operation/") {
            return Reply {
                status: if model.operations.contains_key(id) {
                    200
                } else {
                    404
                },
                body: model.operations.get(id).cloned().unwrap_or(Value::Null),
            };
        }
        if let Some(id) = request.path.strip_prefix("/__test__/document/") {
            return Reply {
                status: if model.fixture["documents"][id].is_null() {
                    404
                } else {
                    200
                },
                body: model.fixture["documents"][id].clone(),
            };
        }
        if request.path == "/__test__/facts" {
            return Reply {
                status: 200,
                body: model.fixture["facts"].clone(),
            };
        }
    }
    if request.method == "POST" && request.path == "/memories" {
        return mutate(model, request, None);
    }
    if request.method == "DELETE"
        && let Some(id) = request.path.strip_prefix("/__test__/document/")
    {
        let id = id.to_owned();
        return mutate(model, request, Some(&id));
    }
    if request.method != "POST" || request.path != MCP_PATH {
        return missing();
    }
    let id = request.body["id"].clone();
    let result = match request.body["method"].as_str() {
        Some("tools/list") => json!({"tools":model.fixture["tool_names"]}),
        Some("tools/call") => {
            let Some(name) = request.body["params"]["name"].as_str() else {
                return missing();
            };
            if name == "get_document" {
                let Some(id) = request.body["params"]["arguments"]["test_document_id"].as_str()
                else {
                    return missing();
                };
                model.fixture["documents"][id].clone()
            } else if let Some(result) = model.fixture["responses"].get(name) {
                result.clone()
            } else {
                return Reply {
                    status: 200,
                    body: json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"test_unknown_tool"}}),
                };
            }
        }
        _ => return missing(),
    };
    Reply {
        status: 200,
        body: json!({"jsonrpc":"2.0","id":id,"result":result}),
    }
}

// Known POST route with visibly synthetic operation/document fields; U3/U4
// remain unverified. The fake does not enforce the publisher's config policy.
fn mutate(model: &mut Model, request: Request, delete: Option<&str>) -> Reply {
    let fault = model.next_fault.take();
    if matches!(fault, Some(MutationFault::BeforeAccept)) {
        return Reply {
            status: 503,
            body: json!({"error":"test_before_accept"}),
        };
    }
    let reply = if let Some(id) = delete {
        model.fixture["documents"]
            .as_object_mut()
            .unwrap()
            .remove(id);
        // A successful DELETE can leave facts, page sources and read indexes.
        Reply {
            status: 200,
            body: json!({"test_deleted":id}),
        }
    } else {
        let body = &request.body;
        let Some(id) = body["test_operation_id"].as_str() else {
            return Reply {
                status: 400,
                body: json!({"error":"test_operation_required"}),
            };
        };
        if !body["document"]["document_id"].is_string()
            || !body["document"]["body"].is_string()
            || !body["document"]["tags"].is_array()
            || body["async"] != true
            || body["update_mode"] != "replace"
        {
            return Reply {
                status: 400,
                body: json!({"error":"test_invalid_retain"}),
            };
        }
        if let Some(old) = model.operations.get(id) {
            if old["request"] != *body {
                return Reply {
                    status: 409,
                    body: json!({"error":"test_payload_mismatch"}),
                };
            }
        } else {
            model.operations.insert(
                id.into(),
                json!({"test_operation_id":id,"test_state":"running","request":body}),
            );
        }
        Reply {
            status: 202,
            body: json!({"test_operation_id":id}),
        }
    };
    model.disconnect = matches!(fault, Some(MutationFault::LostAck));
    reply
}
