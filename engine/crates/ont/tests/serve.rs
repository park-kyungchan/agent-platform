//! Integration tests for `ont serve` (`specs/ont/serve.md`), remote peer
//! attribution (`specs/ont/action.md` R12, R101, R112, R113), and remote MCP
//! over HTTP (`specs/ont/mcp.md` R20). All stores, sockets, and scripts are
//! synthetic temporary fixtures with zero network access.
#![allow(clippy::unwrap_used)]

use ont::principal::EngineIdentity;
use ont::serve::{
    Config, MAX_CONCURRENT_CONNECTIONS, MAX_HEADER_BYTES, PeerIdentityResolver, TailscaleWhois,
    is_tailnet_ip, serve_listener,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

const NOW: u64 = 1_700_000_000;
const ONT_BIN: &str = env!("CARGO_BIN_EXE_ont");

struct Temp {
    dir: PathBuf,
}

impl Temp {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Keep path short so Unix domain socket paths stay well under 108 bytes.
        let dir = std::env::temp_dir().join(format!(
            "ont-srv-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn store(&self) -> PathBuf {
        self.path("store.sqlite")
    }

    fn write_script(&self, name: &str, body: &str) -> String {
        let path = self.path(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .unwrap();
        f.sync_all().unwrap();
        drop(f);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    fn install_schema(&self, schema: &Value) {
        let schema_path = self.path("schema.json");
        std::fs::write(&schema_path, schema.to_string()).unwrap();
        let args = vec![
            "--store".to_string(),
            self.store().display().to_string(),
            "--now".to_string(),
            NOW.to_string(),
            "schema".to_string(),
            "install".to_string(),
            schema_path.display().to_string(),
        ];
        let out = ont::run(&args, &b""[..], 0, 0);
        assert_eq!(
            out.code, 0,
            "schema install failed: {} {}",
            out.stdout, out.stderr
        );
    }

    fn write_roles(&self, roles: &Value) {
        let path = self.path("roles.json");
        std::fs::write(&path, roles.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn last_edit_actor(&self) -> Value {
        let text = std::fs::read_to_string(self.path("edits.jsonl")).unwrap();
        let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        last["actor"].clone()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn synthetic_schema(fn_program: &str) -> Value {
    json!({
        "format": "ontology-schema/v1",
        "domain": "demo",
        "version": 1,
        "namespace": {"instance": "test://demo", "account": "root"},
        "scope_keys": [],
        "value_types": {},
        "sources": {},
        "object_types": {
            "item": {
                "owner": "ontology",
                "primary_key": {"from": "$action_id"},
                "title": "title",
                "properties": {
                    "title": {"type": "string", "required": true},
                    "status": {"type": "string", "required": true, "max_bytes": 64}
                }
            },
            "note": {
                "owner": "ontology",
                "primary_key": {"from": "$action_id"},
                "title": "body",
                "properties": {
                    "body": {"type": "string", "required": true},
                    "item": {"type": {"ref": "item"}, "required": true}
                }
            }
        },
        "link_types": {
            "note_item": {
                "from": "note",
                "via": "item",
                "to": "item",
                "cardinality": "many_to_one",
                "names": {
                    "forward": "item",
                    "reverse": "notes"
                }
            }
        },
        "views": {
            "items": {
                "object_type": "item",
                "order": [{"property": "title", "direction": "asc"}],
                "columns": [
                    {"header": "Title", "property": "title"},
                    {"header": "Status", "property": "status"}
                ]
            }
        },
        "action_types": {
            "create_item": {
                "parameters": {
                    "title": {"type": "string", "required": true},
                    "status": {"type": "string", "required": true}
                },
                "rules": [{
                    "create_object": "item",
                    "set": {"title": "$title", "status": "$status"}
                }]
            },
            "create_note": {
                "parameters": {
                    "body": {"type": "string", "required": true},
                    "item": {"type": {"ref": "item"}, "required": true}
                },
                "rules": [{
                    "create_object": "note",
                    "set": {"body": "$body", "item": "$item"}
                }]
            }
        },
        "functions": {
            "echo_fn": {
                "program": fn_program,
                "parameters": {
                    "word": {"type": "string", "required": true}
                },
                "answer_bytes": 4096,
                "timeout_ms": 5000
            }
        },
        "grants": [{
            "principal": "agent",
            "scope": {},
            "read": ["item", "note"],
            "derive": ["item"],
            "actions": ["create_item", "create_note"]
        }]
    })
}

fn role_checked_schema(fn_program: &str) -> Value {
    json!({
        "format": "ontology-schema/v1",
        "domain": "guarded",
        "version": 1,
        "namespace": {"instance": "test://guarded", "account": "root"},
        "scope_keys": [],
        "value_types": {},
        "sources": {},
        "object_types": {
            "item": {
                "owner": "ontology",
                "primary_key": {"from": "$action_id"},
                "title": "title",
                "properties": {
                    "title": {"type": "string", "required": true}
                }
            }
        },
        "link_types": {},
        "action_types": {
            "open_create": {
                "parameters": {"title": {"type": "string", "required": true}},
                "rules": [{"create_object": "item", "set": {"title": "$title"}}]
            },
            "lead_create": {
                "parameters": {"title": {"type": "string", "required": true}},
                "submission_criteria": [{
                    "code": "must_be_laptop",
                    "message": "caller host must be laptop-1",
                    "all": [{"actor": {"host": "laptop-1"}}]
                }],
                "rules": [{"create_object": "item", "set": {"title": "$title"}}]
            }
        },
        "functions": {
            "lead_fn": {
                "program": fn_program,
                "parameters": {"word": {"type": "string", "required": true}},
                "answer_bytes": 4096,
                "timeout_ms": 5000
            }
        },
        "grants": [
            {
                "principal": "agent",
                "scope": {},
                "read": ["item"],
                "derive": [],
                "actions": ["open_create"],
                "functions": []
            },
            {
                "principal": "agent",
                "role": "lead",
                "scope": {},
                "read": ["item"],
                "derive": [],
                "actions": ["lead_create"],
                "functions": ["lead_fn"]
            }
        ]
    })
}

struct MockResolver {
    result: Mutex<Result<EngineIdentity, String>>,
    calls: AtomicUsize,
}

impl MockResolver {
    fn ok(principal: &str, host: &str) -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::new(Ok(EngineIdentity {
                principal: principal.to_string(),
                host: host.to_string(),
            })),
            calls: AtomicUsize::new(0),
        })
    }

    fn set(&self, next: Result<EngineIdentity, String>) {
        *self.result.lock().unwrap() = next;
    }
}

impl PeerIdentityResolver for MockResolver {
    fn resolve(&self, _peer: SocketAddr) -> Result<EngineIdentity, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.result.lock().unwrap().clone()
    }
}

struct RunningServer {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<std::io::Result<()>>>,
}

impl RunningServer {
    fn start(config: Config, resolver: Arc<dyn PeerIdentityResolver>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&shutdown);
        let handle = thread::spawn(move || serve_listener(listener, config, resolver, flag));
        Self {
            addr,
            shutdown,
            handle: Some(handle),
        }
    }

    fn stop(mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap().unwrap();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct RawResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl RawResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("invalid JSON body ({e}): {:?}", self.body))
    }
}

fn send_raw(addr: SocketAddr, raw: &[u8]) -> RawResponse {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(raw).unwrap();
    stream.flush().unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| {
            panic!(
                "no HTTP response header delimiter: {:?}",
                String::from_utf8_lossy(&bytes)
            )
        });
    let head = std::str::from_utf8(&bytes[..split]).unwrap();
    let body = String::from_utf8(bytes[split + 4..].to_vec()).unwrap();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    RawResponse {
        status,
        headers,
        body,
    }
}

fn http_req(
    addr: SocketAddr,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) -> RawResponse {
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    let mut raw = req.into_bytes();
    raw.extend_from_slice(body);
    send_raw(addr, &raw)
}

#[test]
fn cli_argument_validation_bind_tailnet_only_and_store_precheck() {
    // spec: specs/ont/serve.md#R1
    // spec: specs/ont/serve.md#R2
    // spec: specs/ont/serve.md#R3
    let t = Temp::new("cli");
    let store = t.store().display().to_string();

    // Missing --bind
    let out = ont::run(
        &["--store".into(), store.clone(), "serve".into()],
        &b""[..],
        0,
        0,
    );
    assert_eq!(out.code, 2);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["code"], "invalid_argument");
    assert_eq!(body["field"], "--bind");

    // Unparsable --bind
    let out = ont::run(
        &[
            "--store".into(),
            store.clone(),
            "serve".into(),
            "--bind".into(),
            "not-a-socket-addr".into(),
        ],
        &b""[..],
        0,
        0,
    );
    assert_eq!(out.code, 2);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["code"], "invalid_argument");
    assert_eq!(body["field"], "--bind");

    // Tailnet IP predicate boundaries
    for valid in [
        "100.64.0.0",
        "100.64.0.1",
        "100.127.255.255",
        "fd7a:115c:a1e0::1",
        "fd7a:115c:a1e0:ffff::ffff",
    ] {
        assert!(is_tailnet_ip(valid.parse::<IpAddr>().unwrap()), "{valid}");
    }
    for invalid in [
        "0.0.0.0",
        "::",
        "127.0.0.1",
        "::1",
        "10.0.0.1",
        "192.168.1.10",
        "8.8.8.8",
        "100.63.255.255",
        "100.128.0.0",
        "::ffff:100.64.0.1",
        "fd7a:115c:a1e1::1",
    ] {
        assert!(
            !is_tailnet_ip(invalid.parse::<IpAddr>().unwrap()),
            "{invalid}"
        );
        let addr_str = if invalid.contains(':') {
            format!("[{invalid}]:8080")
        } else {
            format!("{invalid}:8080")
        };
        let out = ont::run(
            &[
                "--store".into(),
                store.clone(),
                "serve".into(),
                "--bind".into(),
                addr_str.clone(),
            ],
            &b""[..],
            0,
            0,
        );
        assert_eq!(out.code, 2, "{addr_str}: {}", out.stdout);
        let body: Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(body["status"], "refused");
        assert_eq!(body["code"], "bind_not_tailnet", "{addr_str}: {body}");
        assert_eq!(body["field"], "--bind");
    }

    // Invalid --tailscale-socket (relative path and NUL byte)
    for bad_sock in ["relative/tailscaled.sock", "/run/tailscale/bad\0sock"] {
        let out = ont::run(
            &[
                "--store".into(),
                store.clone(),
                "serve".into(),
                "--bind".into(),
                "100.64.0.1:8080".into(),
                "--tailscale-socket".into(),
                bad_sock.into(),
            ],
            &b""[..],
            0,
            0,
        );
        assert_eq!(out.code, 2);
        let body: Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(body["code"], "invalid_argument");
        assert_eq!(body["field"], "--tailscale-socket");
    }

    // Invalid --client
    let out = ont::run(
        &[
            "--store".into(),
            store.clone(),
            "serve".into(),
            "--bind".into(),
            "100.64.0.1:8080".into(),
            "--client".into(),
            "INVALID_CLIENT".into(),
        ],
        &b""[..],
        0,
        0,
    );
    assert_eq!(out.code, 2);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["code"], "invalid_request");
    assert_eq!(body["field"], "--client");

    // Non-root UID check
    let out = ont::run(
        &[
            "--store".into(),
            store.clone(),
            "serve".into(),
            "--bind".into(),
            "100.64.0.1:8080".into(),
        ],
        &b""[..],
        1000,
        1000,
    );
    assert_eq!(out.code, 2);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["code"], "principal_not_trusted");

    // --now on default store refuses now_at_default_store (R104)
    let out = ont::run(
        &[
            "--store".into(),
            ont::DEFAULT_STORE.into(),
            "--now".into(),
            "1000".into(),
            "serve".into(),
            "--bind".into(),
            "100.64.0.1:8080".into(),
        ],
        &b""[..],
        0,
        0,
    );
    assert_eq!(out.code, 2);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["code"], "now_at_default_store");
    assert_eq!(body["field"], "--now");

    // ONT_FN_READ_ONLY=1 refuses ont serve with read_only_call
    let ro = Command::new(ONT_BIN)
        .args(["--store", &store, "serve", "--bind", "100.64.0.1:8080"])
        .env("ONT_FN_READ_ONLY", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(ro.status.code(), Some(2));
    let body: Value = serde_json::from_slice(&ro.stdout).unwrap();
    assert_eq!(body["code"], "read_only_call");

    // Missing store refuses store_not_found and never creates a store file
    assert!(!t.store().exists());
    let out = ont::run(
        &[
            "--store".into(),
            store.clone(),
            "serve".into(),
            "--bind".into(),
            "100.64.0.1:8080".into(),
        ],
        &b""[..],
        0,
        0,
    );
    assert_eq!(out.code, 2);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["code"], "store_not_found");
    assert!(!t.store().exists(), "serve must never create a store file");

    // Corrupt/foreign store refuses internal exit 1
    let bad_store = t.path("corrupt.sqlite");
    std::fs::write(&bad_store, b"not a sqlite database").unwrap();
    let out = ont::run(
        &[
            "--store".into(),
            bad_store.display().to_string(),
            "serve".into(),
            "--bind".into(),
            "100.64.0.1:8080".into(),
        ],
        &b""[..],
        0,
        0,
    );
    assert_eq!(out.code, 1);
    assert!(out.stderr.starts_with("store_failure@"), "{}", out.stderr);
}

#[test]
fn tailscale_whois_real_unix_socket_chunked_tags_and_rejection_cases() {
    // spec: specs/ont/serve.md#R4
    // spec: specs/ont/serve.md#R5
    // spec: specs/ont/serve.md#R6
    // spec: specs/ont/serve.md#R12
    // spec: specs/ont/serve.md#R15
    let t = Temp::new("whois");
    let fn_prog = t.write_script("fn.sh", "echo '{\"ok\":true}'");
    t.install_schema(&synthetic_schema(&fn_prog));

    let sock_path = t.path("ts.sock");
    let unix_listener = UnixListener::bind(&sock_path).unwrap();
    unix_listener.set_nonblocking(true).unwrap();

    let next_response: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let unix_stop = Arc::new(AtomicBool::new(false));

    let resp_ref = Arc::clone(&next_response);
    let req_ref = Arc::clone(&captured_requests);
    let stop_ref = Arc::clone(&unix_stop);
    let unix_thread = thread::spawn(move || {
        while !stop_ref.load(Ordering::Acquire) {
            match unix_listener.accept() {
                Ok((mut conn, _)) => {
                    let _ = conn.set_nonblocking(false);
                    let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buf = [0u8; 2048];
                    let n = conn.read(&mut buf).unwrap_or(0);
                    req_ref
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&buf[..n]).to_string());
                    let payload = resp_ref.lock().unwrap().clone();
                    let _ = conn.write_all(&payload);
                    let _ = conn.flush();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    let resolver: Arc<dyn PeerIdentityResolver> = Arc::new(TailscaleWhois::new(sock_path.clone()));
    let srv = RunningServer::start(
        Config {
            store: t.store(),
            now: Some(NOW),
            client: String::new(),
        },
        resolver,
    );

    // 1. Standard HTTP/1.0 200 with LoginName and ComputedName + X-Forwarded-For ignored
    let user_body = json!({
        "Node": {"ComputedName": "dev-laptop", "Name": "dev-laptop.tailnet.ts.net."},
        "UserProfile": {"LoginName": "alice@example.com"}
    })
    .to_string();
    *next_response.lock().unwrap() = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{user_body}",
        user_body.len()
    )
    .into_bytes();

    let health = http_req(
        srv.addr,
        "GET",
        "/healthz",
        &[("X-Forwarded-For", "100.64.0.99")],
        b"",
    );
    assert_eq!(health.status, 200, "{}", health.body);
    assert_eq!(health.header("connection"), Some("close"));
    assert_eq!(health.header("content-type"), Some("application/json"));
    let hb = health.json();
    assert_eq!(hb["status"], "ok");
    assert_eq!(hb["service"], "ont-serve");
    assert!(hb["version"].as_str().is_some_and(|v| !v.is_empty()));

    // Verify whois query used kernel peer_addr (127.0.0.1), never X-Forwarded-For
    let last_whois = captured_requests.lock().unwrap().last().cloned().unwrap();
    assert!(
        last_whois.starts_with("GET /localapi/v0/whois?addr=127.0.0.1%3A"),
        "{last_whois}"
    );
    assert!(last_whois.contains("Host: local-tailscaled.sock\r\n"));
    assert!(!last_whois.contains("100.64.0.99"));

    // Verify IPv6 peer addresses percent-encode brackets and colons in whois query
    let direct_whois = TailscaleWhois::new(sock_path.clone());
    let ipv6_id = direct_whois
        .resolve("[fd7a:115c:a1e0::2]:54321".parse().unwrap())
        .unwrap();
    assert_eq!(ipv6_id.principal, "ts:alice@example.com");
    assert_eq!(ipv6_id.host, "dev-laptop");
    let ipv6_whois_req = captured_requests.lock().unwrap().last().cloned().unwrap();
    assert!(
        ipv6_whois_req.starts_with(
            "GET /localapi/v0/whois?addr=%5Bfd7a%3A115c%3Aa1e0%3A%3A2%5D%3A54321 HTTP/1.0\r\n"
        ),
        "{ipv6_whois_req}"
    );

    // 2. Chunked HTTP/1.1 200 with multiple tags (lexicographically smallest wins) and Name fallback
    let tagged_body = json!({
        "Node": {
            "ComputedName": "",
            "Name": "worker-node-7.example.ts.net.",
            "Tags": ["tag:zeta", "tag:alpha", "tag:middle"]
        },
        "UserProfile": {"LoginName": "tagged-devices"}
    })
    .to_string();
    let split = tagged_body.len() / 2;
    let part1 = &tagged_body[..split];
    let part2 = &tagged_body[split..];
    *next_response.lock().unwrap() = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n{:x}\r\n{part1}\r\n{:x}\r\n{part2}\r\n0\r\n\r\n",
        part1.len(),
        part2.len()
    )
    .into_bytes();

    let apply_resp = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[],
        json!({
            "action_type": "create_item",
            "parameters": {"title": "from-tagged", "status": "open"},
            "request_id": "tag-req-1"
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(apply_resp.status, 200, "{}", apply_resp.body);
    let applied = apply_resp.json();
    assert_eq!(applied["result"]["outcome"], "applied");
    let actor = t.last_edit_actor();
    assert_eq!(actor["principal"], "ts:tag:alpha");
    assert_eq!(actor["host"], "worker-node-7");
    let claimed = actor["claimed"].as_array().unwrap();
    assert!(!claimed.contains(&json!("principal")));
    assert!(!claimed.contains(&json!("host")));

    // 3. Rejection cases (using a nonexistent store path to prove store is never opened)
    srv.stop();
    let missing_store = t.path("never-created.sqlite");
    let srv_missing = RunningServer::start(
        Config {
            store: missing_store.clone(),
            now: Some(NOW),
            client: String::new(),
        },
        Arc::new(TailscaleWhois::new(sock_path.clone())),
    );

    let oversize_body = "x".repeat(70_000);
    let rejection_payloads: Vec<(&str, Vec<u8>)> = vec![
        ("non-200", b"HTTP/1.0 404 Not Found\r\n\r\nnot found".to_vec()),
        ("malformed-json", b"HTTP/1.0 200 OK\r\n\r\n{not json".to_vec()),
        (
            "missing-user-profile",
            b"HTTP/1.0 200 OK\r\n\r\n{\"Node\":{\"ComputedName\":\"n1\"}}".to_vec(),
        ),
        (
            "tagged-devices-without-tags",
            b"HTTP/1.0 200 OK\r\n\r\n{\"Node\":{\"ComputedName\":\"n1\",\"Tags\":[]},\"UserProfile\":{\"LoginName\":\"tagged-devices\"}}".to_vec(),
        ),
        (
            "invalid-host-chars",
            b"HTTP/1.0 200 OK\r\n\r\n{\"Node\":{\"ComputedName\":\"bad host!\"},\"UserProfile\":{\"LoginName\":\"alice@example.com\"}}".to_vec(),
        ),
        (
            "invalid-principal-slash",
            b"HTTP/1.0 200 OK\r\n\r\n{\"Node\":{\"ComputedName\":\"n1\"},\"UserProfile\":{\"LoginName\":\"alice/evil\"}}".to_vec(),
        ),
        (
            "invalid-principal-dotdot",
            b"HTTP/1.0 200 OK\r\n\r\n{\"Node\":{\"ComputedName\":\"n1\"},\"UserProfile\":{\"LoginName\":\"alice..evil\"}}".to_vec(),
        ),
        (
            "oversize",
            format!("HTTP/1.0 200 OK\r\n\r\n{oversize_body}").into_bytes(),
        ),
    ];

    for (label, payload) in rejection_payloads {
        *next_response.lock().unwrap() = payload;
        let resp = http_req(srv_missing.addr, "POST", "/v1/describe", &[], b"{}");
        assert_eq!(resp.status, 403, "{label}: {}", resp.body);
        let j = resp.json();
        assert_eq!(j["status"], "refused", "{label}: {j}");
        assert_eq!(j["code"], "peer_not_identified", "{label}: {j}");
        assert!(!missing_store.exists());
    }
    srv_missing.stop();

    // Missing Unix socket also returns 403 peer_not_identified
    let srv_nosock = RunningServer::start(
        Config {
            store: missing_store,
            now: Some(NOW),
            client: String::new(),
        },
        Arc::new(TailscaleWhois::new(t.path("no-such-socket.sock"))),
    );
    let resp = http_req(srv_nosock.addr, "GET", "/healthz", &[], b"");
    assert_eq!(resp.status, 403);
    assert_eq!(resp.json()["code"], "peer_not_identified");
    srv_nosock.stop();

    unix_stop.store(true, Ordering::Release);
    unix_thread.join().unwrap();
}

#[test]
fn all_v1_endpoints_status_codes_and_excluded_admin_endpoints() {
    // spec: specs/ont/serve.md#R10
    // spec: specs/ont/serve.md#R11
    // spec: specs/ont/serve.md#R12
    // spec: specs/ont/serve.md#R15
    let t = Temp::new("v1");
    let fn_prog = t.write_script("echo_fn.sh", "cat >/dev/null; echo '{\"echoed\":\"yes\"}'");
    t.install_schema(&synthetic_schema(&fn_prog));

    let resolver = MockResolver::ok("ts:alice@example.com", "laptop-1");
    let srv = RunningServer::start(
        Config {
            store: t.store(),
            now: Some(NOW),
            client: String::new(),
        },
        resolver,
    );

    // 1. POST /v1/describe (both empty body and {})
    let r = http_req(srv.addr, "POST", "/v1/describe", &[], b"");
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["status"], "ok");

    // 2. POST /v1/action/list
    let r = http_req(srv.addr, "POST", "/v1/action/list", &[], b"{}");
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["status"], "ok");

    // 3. POST /v1/action/validate
    let create_req = json!({
        "action_type": "create_item",
        "parameters": {"title": "alpha", "status": "open"},
        "request_id": "v1-item-1"
    });
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/validate",
        &[],
        create_req.to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["valid"], true);

    // 4. POST /v1/action/apply (item + linked note)
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[],
        create_req.to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let item_apply = r.json();
    let item_id = item_apply["result"]["edits"][0]["id"].clone();
    let actor = t.last_edit_actor();
    assert_eq!(actor["principal"], "ts:alice@example.com");
    assert_eq!(actor["host"], "laptop-1");
    assert_eq!(actor["client"], "remote");

    let note_req = json!({
        "action_type": "create_note",
        "parameters": {"body": "note-text", "item": item_id},
        "request_id": "v1-note-1"
    });
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[],
        note_req.to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);

    // 5. POST /v1/get
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/get",
        &[],
        json!({"id": item_id}).to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(
        r.json()["result"]["objects"][0]["observations"][0]["fields"]["title"]["value"],
        "alpha"
    );

    // 6. POST /v1/search
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/search",
        &[],
        json!({"object_type": "item"}).to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["objects"].as_array().unwrap().len(), 1);

    // 7. POST /v1/around
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/around",
        &[],
        json!({"from": {"object_type": "note"}, "path": ["item"]})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["objects"].as_array().unwrap().len(), 1);

    // 8. POST /v1/aggregate
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/aggregate",
        &[],
        json!({"object_type": "item", "group_by": ["status"], "metrics": ["count"]})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["groups"][0]["count"], 1);

    // 9. POST /v1/log
    let r = http_req(srv.addr, "POST", "/v1/log", &[], b"{}");
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["rows"].as_array().unwrap().len(), 2);

    // 10. POST /v1/render (read-only view render)
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/render",
        &[],
        json!({"view": "items"}).to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.json()["markdown"].as_str().unwrap().contains("alpha"));

    // 11. POST /v1/fn
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/fn",
        &[],
        json!({"name": "echo_fn", "parameters": {"word": "hi"}})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["answer"]["echoed"], "yes");

    // 400 Bad Request on refusal
    let r = http_req(srv.addr, "POST", "/v1/get", &[], b"{}");
    assert_eq!(r.status, 400, "{}", r.body);
    assert_eq!(r.json()["status"], "refused");
    assert_eq!(r.json()["code"], "invalid_request");

    // 413 Payload Too Large over MAX_REQUEST
    let too_big = vec![b'a'; ont::MAX_REQUEST + 1];
    let r = http_req(srv.addr, "POST", "/v1/describe", &[], &too_big);
    assert_eq!(r.status, 413, "{}", r.body);
    assert_eq!(r.json()["code"], "request_too_large");

    // 405 Method Not Allowed
    let r = http_req(srv.addr, "GET", "/v1/describe", &[], b"");
    assert_eq!(r.status, 405, "{}", r.body);
    assert_eq!(r.json()["code"], "method_not_allowed");
    let r = http_req(srv.addr, "POST", "/healthz", &[], b"");
    assert_eq!(r.status, 405, "{}", r.body);
    assert_eq!(r.json()["code"], "method_not_allowed");

    // 404 Not Found for unknown & host-local admin endpoints
    for path in [
        "/v1/index",
        "/v1/backup",
        "/v1/schema",
        "/v1/schema/install",
        "/v1/render/write",
        "/unknown",
    ] {
        let r = http_req(srv.addr, "POST", path, &[], b"{}");
        assert_eq!(r.status, 404, "{path}: {}", r.body);
        assert_eq!(r.json()["code"], "unknown_endpoint", "{path}");
    }

    srv.stop();

    // 500 Internal Server Error on damaged store
    let bad_store = t.path("damaged.sqlite");
    std::fs::write(&bad_store, b"corrupt-bytes").unwrap();
    let srv_bad = RunningServer::start(
        Config {
            store: bad_store,
            now: Some(NOW),
            client: String::new(),
        },
        MockResolver::ok("ts:alice@example.com", "laptop-1"),
    );
    let r = http_req(srv_bad.addr, "POST", "/v1/describe", &[], b"{}");
    assert_eq!(r.status, 500, "{}", r.body);
    let j = r.json();
    assert_eq!(j["status"], "internal");
    assert!(j["code"].as_str().unwrap().starts_with("store_failure@"));
    srv_bad.stop();
}

#[test]
fn remote_attribution_roles_json_and_spoofing_resistance() {
    // spec: specs/ont/serve.md#R7
    // spec: specs/ont/serve.md#R9
    // spec: specs/ont/action.md#R12
    // spec: specs/ont/action.md#R101
    // spec: specs/ont/action.md#R112
    // spec: specs/ont/action.md#R113
    let t = Temp::new("roles");
    let fn_prog = t.write_script("lead_fn.sh", "cat >/dev/null; echo '{\"ok\":1}'");
    t.install_schema(&role_checked_schema(&fn_prog));
    t.write_roles(&json!({
        "principals": {
            "ts:alice@example.com": ["lead"],
            "ts:tag:worker": ["worker"]
        }
    }));

    let resolver = MockResolver::ok("ts:alice@example.com", "laptop-1");
    let srv = RunningServer::start(
        Config {
            store: t.store(),
            now: Some(NOW),
            client: "srv-default".to_string(),
        },
        Arc::clone(&resolver) as Arc<dyn PeerIdentityResolver>,
    );

    // 1. Header attribution (case-insensitive) + role admission for ts: principal + host criterion
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[
            ("x-ont-client", "pc-cli"),
            ("X-Ont-Agent", "pc-agent"),
            ("x-ont-role", "lead"),
        ],
        json!({
            "action_type": "lead_create",
            "parameters": {"title": "from-lead"},
            "request_id": "role-req-1"
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let actor = t.last_edit_actor();
    assert_eq!(actor["principal"], "ts:alice@example.com");
    assert_eq!(actor["host"], "laptop-1");
    assert_eq!(actor["client"], "pc-cli");
    assert_eq!(actor["agent"], "pc-agent");
    assert_eq!(actor["role"], "lead");
    assert_eq!(actor["claimed"], json!(["agent", "client", "role"]));

    // 2. X-Ont-Client: cli maps to "remote", and body agent/role win over headers
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[
            ("X-Ont-Client", "cli"),
            ("X-Ont-Agent", "hdr-agent"),
            ("X-Ont-Role", "worker"),
        ],
        json!({
            "action_type": "lead_create",
            "parameters": {"title": "body-wins"},
            "request_id": "role-req-2",
            "agent": "body-agent",
            "role": "lead"
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let actor = t.last_edit_actor();
    assert_eq!(actor["client"], "remote");
    assert_eq!(actor["agent"], "body-agent");
    assert_eq!(actor["role"], "lead");

    // 3. /v1/fn uses X-Ont-Role against roles.json for the peer ts: principal
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/fn",
        &[("X-Ont-Role", "lead")],
        json!({"name": "lead_fn", "parameters": {"word": "ok"}})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["answer"]["ok"], 1);

    // 4. Body principal/host fields are refused as unknown fields
    for bad_body in [
        json!({"action_type":"open_create","parameters":{"title":"x"},"principal":"uid:0"}),
        json!({"action_type":"open_create","parameters":{"title":"x"},"host":"fake-host"}),
    ] {
        let r = http_req(
            srv.addr,
            "POST",
            "/v1/action/apply",
            &[],
            bad_body.to_string().as_bytes(),
        );
        assert_eq!(r.status, 400, "{}", r.body);
        assert_eq!(r.json()["code"], "invalid_request");
    }

    // 5. Invalid attribution headers refuse 400 invalid_request
    for (hdr, val) in [
        ("X-Ont-Client", "BAD CLIENT"),
        ("X-Ont-Agent", "bad agent!"),
        ("X-Ont-Role", "bad role!"),
    ] {
        let r = http_req(srv.addr, "POST", "/v1/describe", &[(hdr, val)], b"{}");
        assert_eq!(r.status, 400, "{hdr}={val}: {}", r.body);
        assert_eq!(r.json()["code"], "invalid_request");
    }

    // 6. Peer without "lead" role is refused role_not_admitted and logged in refusals.jsonl with peer ts: principal
    resolver.set(Ok(EngineIdentity {
        principal: "ts:tag:worker".to_string(),
        host: "worker-2".to_string(),
    }));
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[("X-Ont-Role", "lead")],
        json!({
            "action_type": "lead_create",
            "parameters": {"title": "denied"},
            "request_id": "role-req-denied"
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 400, "{}", r.body);
    let j = r.json();
    assert_eq!(j["code"], "role_not_admitted");
    assert_eq!(j["detail"], "role_not_listed");
    let refusals_text = std::fs::read_to_string(t.path("refusals.jsonl")).unwrap();
    let last_refusal: Value = serde_json::from_str(refusals_text.lines().last().unwrap()).unwrap();
    assert_eq!(last_refusal["actor"]["principal"], "ts:tag:worker");
    assert_eq!(last_refusal["actor"]["host"], "worker-2");
    assert_eq!(last_refusal["code"], "role_not_admitted");

    // 7. Peer with lead role on wrong host fails host criterion
    resolver.set(Ok(EngineIdentity {
        principal: "ts:alice@example.com".to_string(),
        host: "other-host".to_string(),
    }));
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/validate",
        &[("X-Ont-Role", "lead")],
        json!({
            "action_type": "lead_create",
            "parameters": {"title": "wrong-host"}
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["valid"], false);

    // 8. roles.json rejects malformed ts: principals (/, .., whitespace) as roles_file_unusable
    for bad_principal in ["ts:bad/slash", "ts:bad..dots", "ts:has space", "ts:"] {
        t.write_roles(&json!({ "principals": { bad_principal: ["lead"] } }));
        let r = http_req(
            srv.addr,
            "POST",
            "/v1/action/validate",
            &[("X-Ont-Role", "lead")],
            json!({
                "action_type": "lead_create",
                "parameters": {"title": "x"}
            })
            .to_string()
            .as_bytes(),
        );
        assert_eq!(r.status, 400, "{bad_principal}: {}", r.body);
        assert_eq!(r.json()["code"], "role_not_admitted");
        assert_eq!(r.json()["detail"], "roles_file_unusable", "{bad_principal}");
    }

    srv.stop();
}

#[test]
fn server_env_ont_role_and_ont_agent_are_never_inherited_by_remote_requests() {
    // spec: specs/ont/serve.md#R9
    // spec: specs/ont/serve.md#R13
    // spec: specs/ont/action.md#R12
    // spec: specs/ont/action.md#R112
    // spec: specs/ont/mcp.md#R20
    if std::env::var_os("ONT_SERVE_ENV_ISOLATION_CHILD").is_none() {
        let exe = std::env::current_exe().unwrap();
        let status = Command::new(exe)
            .args([
                "--exact",
                "server_env_ont_role_and_ont_agent_are_never_inherited_by_remote_requests",
                "--nocapture",
            ])
            .env("ONT_SERVE_ENV_ISOLATION_CHILD", "1")
            .env("ONT_ROLE", "lead")
            .env("ONT_AGENT", "leaked-server-agent")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "child failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        return;
    }

    // Running inside child process where ONT_ROLE=lead and ONT_AGENT=leaked-server-agent are set.
    assert_eq!(std::env::var("ONT_ROLE").unwrap(), "lead");
    assert_eq!(std::env::var("ONT_AGENT").unwrap(), "leaked-server-agent");

    let t = Temp::new("env-iso");
    let fn_prog = t.write_script("lead_fn.sh", "cat >/dev/null; echo '{\"ok\":1}'");
    t.install_schema(&role_checked_schema(&fn_prog));
    t.write_roles(&json!({
        "principals": { "ts:alice@example.com": ["lead"] }
    }));

    let srv = RunningServer::start(
        Config {
            store: t.store(),
            now: Some(NOW),
            client: String::new(),
        },
        MockResolver::ok("ts:alice@example.com", "laptop-1"),
    );

    // 1. /v1/action/apply without role header must NOT inherit ONT_ROLE=lead -> not_granted
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[],
        json!({
            "action_type": "lead_create",
            "parameters": {"title": "no-role"},
            "request_id": "env-1"
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 400, "{}", r.body);
    assert_eq!(r.json()["code"], "not_granted");

    // 2. /v1/fn without role header must NOT inherit ONT_ROLE=lead -> not_granted
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/fn",
        &[],
        json!({"name": "lead_fn", "parameters": {"word": "x"}})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 400, "{}", r.body);
    assert_eq!(r.json()["code"], "not_granted");

    // 3. /v1/action/apply on open_create without agent header must NOT inherit ONT_AGENT or ONT_ROLE
    let r = http_req(
        srv.addr,
        "POST",
        "/v1/action/apply",
        &[],
        json!({
            "action_type": "open_create",
            "parameters": {"title": "clean"},
            "request_id": "env-2"
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let actor = t.last_edit_actor();
    assert_eq!(actor["principal"], "ts:alice@example.com");
    assert_eq!(actor["host"], "laptop-1");
    assert_eq!(actor["client"], "remote");
    assert!(actor.get("agent").is_none(), "{actor}");
    assert!(actor.get("role").is_none(), "{actor}");

    // 4. /mcp tools/call must NOT inherit ONT_ROLE or ONT_AGENT either
    let mcp_lead = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "ont_action_apply",
            "arguments": {
                "action_type": "lead_create",
                "parameters": {"title": "mcp-no-role"},
                "request_id": "env-mcp-1"
            }
        }
    });
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        mcp_lead.to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let mcp_res = r.json();
    assert_eq!(mcp_res["result"]["isError"], true);
    let inner: Value =
        serde_json::from_str(mcp_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["code"], "not_granted");

    let mcp_open = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ont_action_apply",
            "arguments": {
                "action_type": "open_create",
                "parameters": {"title": "mcp-clean"},
                "request_id": "env-mcp-2"
            }
        }
    });
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        mcp_open.to_string().as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let inner: Value =
        serde_json::from_str(r.json()["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["result"]["outcome"], "applied");
    let actor = t.last_edit_actor();
    assert_eq!(actor["principal"], "ts:alice@example.com");
    assert!(actor.get("agent").is_none(), "{actor}");
    assert!(actor.get("role").is_none(), "{actor}");

    srv.stop();
}

#[test]
fn mcp_over_http_methods_notifications_ont_index_refusal_and_bounds() {
    // spec: specs/ont/serve.md#R13
    // spec: specs/ont/serve.md#R14
    // spec: specs/ont/mcp.md#R20
    let t = Temp::new("mcp");
    let fn_prog = t.write_script("lead_fn.sh", "cat >/dev/null; echo '{\"via\":\"mcp\"}'");
    t.install_schema(&role_checked_schema(&fn_prog));
    t.write_roles(&json!({
        "principals": { "ts:alice@example.com": ["lead"] }
    }));

    let srv = RunningServer::start(
        Config {
            store: t.store(),
            now: Some(NOW),
            client: String::new(),
        },
        MockResolver::ok("ts:alice@example.com", "laptop-1"),
    );

    // initialize
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-11-25", "clientInfo": {"name": "pc"}}
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.json()["result"]["protocolVersion"], "2025-11-25");

    // notification -> 202 Accepted with empty body
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 202, "{}", r.body);
    assert!(r.body.is_empty());

    // ping
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        json!({"jsonrpc": "2.0", "id": 2, "method": "ping"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["result"], json!({}));

    // tools/list
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["result"]["tools"].as_array().unwrap().len(), 9);

    // tools/call ont_index -> refused operation_not_served
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[],
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "ont_index", "arguments": {}}
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let idx_res = r.json();
    assert_eq!(idx_res["result"]["isError"], true);
    let inner: Value =
        serde_json::from_str(idx_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["code"], "operation_not_served");

    // tools/call ont_action_apply works with peer identity and remote headers (proving read_only wasn't abused)
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[
            ("X-Ont-Client", "cli"),
            ("X-Ont-Agent", "mcp-remote-agent"),
            ("X-Ont-Role", "lead"),
        ],
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "ont_action_apply",
                "arguments": {
                    "action_type": "lead_create",
                    "parameters": {"title": "mcp-applied"},
                    "request_id": "mcp-apply-1"
                }
            }
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let apply_res = r.json();
    assert_eq!(apply_res["result"]["isError"], false, "{apply_res}");
    let inner: Value =
        serde_json::from_str(apply_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["result"]["outcome"], "applied");
    let actor = t.last_edit_actor();
    assert_eq!(actor["principal"], "ts:alice@example.com");
    assert_eq!(actor["host"], "laptop-1");
    assert_eq!(actor["client"], "remote");
    assert_eq!(actor["agent"], "mcp-remote-agent");
    assert_eq!(actor["role"], "lead");

    // tools/call ont_fn uses X-Ont-Role
    let r = http_req(
        srv.addr,
        "POST",
        "/mcp",
        &[("X-Ont-Role", "lead")],
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "ont_fn",
                "arguments": {"name": "lead_fn", "parameters": {"word": "hi"}}
            }
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let fn_res = r.json();
    assert_eq!(fn_res["result"]["isError"], false, "{fn_res}");
    let inner: Value =
        serde_json::from_str(fn_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inner["answer"]["via"], "mcp");

    // Non-POST /mcp -> 405
    let r = http_req(srv.addr, "GET", "/mcp", &[], b"");
    assert_eq!(r.status, 405);
    assert_eq!(r.json()["code"], "method_not_allowed");

    // Body > mcp::MAX_LINE -> 413 JSON-RPC -32600 request_too_large (detected from Content-Length)
    let oversize_head = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n",
        ont::mcp::MAX_LINE + 1
    );
    let r = send_raw(srv.addr, oversize_head.as_bytes());
    assert_eq!(r.status, 413, "{}", r.body);
    let j = r.json();
    assert_eq!(j["jsonrpc"], "2.0");
    assert_eq!(j["id"], Value::Null);
    assert_eq!(j["error"]["code"], -32600);
    assert_eq!(j["error"]["message"], "request_too_large");

    srv.stop();
}

struct GateResolver {
    entered: Mutex<usize>,
    entered_cv: Condvar,
    release: Mutex<bool>,
    release_cv: Condvar,
    panic_next: AtomicBool,
}

impl GateResolver {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Mutex::new(0),
            entered_cv: Condvar::new(),
            release: Mutex::new(true),
            release_cv: Condvar::new(),
            panic_next: AtomicBool::new(false),
        })
    }

    fn hold(&self) {
        *self.release.lock().unwrap() = false;
        *self.entered.lock().unwrap() = 0;
    }

    fn wait_entered(&self, target: usize) {
        let mut guard = self.entered.lock().unwrap();
        while *guard < target {
            guard = self.entered_cv.wait(guard).unwrap();
        }
    }

    fn release_all(&self) {
        *self.release.lock().unwrap() = true;
        self.release_cv.notify_all();
    }
}

impl PeerIdentityResolver for GateResolver {
    fn resolve(&self, _peer: SocketAddr) -> Result<EngineIdentity, String> {
        if self.panic_next.swap(false, Ordering::AcqRel) {
            panic!("synthetic resolver panic");
        }
        {
            let mut count = self.entered.lock().unwrap();
            *count += 1;
            self.entered_cv.notify_all();
        }
        let mut rel = self.release.lock().unwrap();
        while !*rel {
            rel = self.release_cv.wait(rel).unwrap();
        }
        Ok(EngineIdentity {
            principal: "ts:alice@example.com".to_string(),
            host: "laptop-1".to_string(),
        })
    }
}

#[test]
fn http_framing_header_limit_concurrency_cap_and_panic_containment() {
    // spec: specs/ont/serve.md#R8
    // spec: specs/ont/serve.md#R3
    ont::mcp::install_panic_recorder();
    let t = Temp::new("wire");
    let fn_prog = t.write_script("fn.sh", "echo '{}'");
    t.install_schema(&synthetic_schema(&fn_prog));

    let gate = GateResolver::new();
    let srv = RunningServer::start(
        Config {
            store: t.store(),
            now: Some(NOW),
            client: String::new(),
        },
        Arc::clone(&gate) as Arc<dyn PeerIdentityResolver>,
    );

    // 1. Incoming Transfer-Encoding -> 400 unsupported_transfer_encoding
    let r = send_raw(
        srv.addr,
        b"POST /v1/describe HTTP/1.1\r\nHost: 127.0.0.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
    );
    assert_eq!(r.status, 400, "{}", r.body);
    assert_eq!(r.header("connection"), Some("close"));
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(r.json()["code"], "unsupported_transfer_encoding");

    // 2. Malformed headers / request line / duplicate Content-Length -> 400 malformed_request
    for raw in [
        &b"BADREQUEST\r\n\r\n"[..],
        b"GET /healthz HTTP/2.0\r\nHost: x\r\n\r\n",
        b"GET /healthz HTTP/1.1\r\nBadHeaderWithoutColon\r\n\r\n",
        b"POST /v1/describe HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
        b"GET /healthz HTTP/1.1\r\n obs-fold: x\r\n\r\n",
    ] {
        let r = send_raw(srv.addr, raw);
        assert_eq!(r.status, 400, "{}", r.body);
        assert_eq!(r.json()["code"], "malformed_request");
    }

    // 3. Header block > 16,384 bytes -> 431 headers_too_large
    let big_val = "a".repeat(MAX_HEADER_BYTES);
    let big_req = format!("GET /healthz HTTP/1.1\r\nHost: x\r\nX-Pad: {big_val}\r\n\r\n");
    let r = send_raw(srv.addr, big_req.as_bytes());
    assert_eq!(r.status, 431, "{}", r.body);
    assert_eq!(r.json()["code"], "headers_too_large");

    // 4. Panic containment -> 500 internal panic@<file>:<line>, and server stays healthy
    gate.panic_next.store(true, Ordering::Release);
    let r = http_req(srv.addr, "GET", "/healthz", &[], b"");
    assert_eq!(r.status, 500, "{}", r.body);
    let j = r.json();
    assert_eq!(j["status"], "internal");
    let code = j["code"].as_str().unwrap();
    assert!(code.starts_with("panic@"), "{code}");
    let after = http_req(srv.addr, "GET", "/healthz", &[], b"");
    assert_eq!(after.status, 200, "{}", after.body);

    // 5. Concurrency cap at 16 -> 17th connection gets 503 server_busy
    gate.hold();
    let mut handles = Vec::new();
    for _ in 0..MAX_CONCURRENT_CONNECTIONS {
        let addr = srv.addr;
        handles.push(thread::spawn(move || {
            http_req(addr, "GET", "/healthz", &[], b"")
        }));
    }
    gate.wait_entered(MAX_CONCURRENT_CONNECTIONS);

    let overflow = http_req(srv.addr, "GET", "/healthz", &[], b"");
    assert_eq!(overflow.status, 503, "{}", overflow.body);
    assert_eq!(overflow.json()["code"], "server_busy");

    gate.release_all();
    for h in handles {
        let r = h.join().unwrap();
        assert_eq!(r.status, 200, "{}", r.body);
    }

    // Clean shutdown drains and returns Ok(())
    srv.stop();
}
