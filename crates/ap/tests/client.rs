//! Remote HTTP/1.1 client for `ont serve` (`specs/ap/client.md` R1 to R13): real HTTP/1.1
//! roundtrips over a local `TcpListener` injected via `host::WayOut`, strict tailnet URL
//! validation, remote-enabled verbs (`ap status`, `ap request`, `ap intent`), and CLI subprocess
//! refusal of VPS-only driver verbs.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::client::{Endpoint, Transport, call_http, parse_store_url, resolve};
use ap::host::{Answer, Failure, System, WayOut};
use ap::{intent, request, status, store};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const TAILNET_V4: &str = "http://100.95.133.99:7432";
const TAILNET_V6: &str = "http://[fd7a:115c:a1e0::1]:7432";

/// A captured HTTP/1.1 request received by the mock `ont serve` TCP server.
#[derive(Debug, Clone)]
struct RecordedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl RecordedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// What the mock HTTP/1.1 TCP server sends back for one connection.
enum ServerAction {
    /// Standard JSON response with status line and accurate `Content-Length`.
    Json(u16, Value),
    /// Raw bytes written directly to the TCP stream.
    Raw(Vec<u8>),
    /// Sleep longer than the client deadline before closing.
    Delay(Duration),
    /// Close immediately without sending any bytes.
    CloseEmpty,
}

/// A real local `TcpListener` speaking HTTP/1.1 for scripted responses.
struct MockServer {
    addr: SocketAddr,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    accepted: Arc<AtomicUsize>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn start(actions: Vec<ServerAction>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let rec_clone = Arc::clone(&recorded);
        let acc_clone = Arc::clone(&accepted);
        let queue = VecDeque::from(actions);
        let handle = thread::spawn(move || {
            for action in queue {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                acc_clone.fetch_add(1, Ordering::SeqCst);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let req = read_http_request(&mut stream);
                if let Some(r) = req {
                    rec_clone.lock().unwrap().push(r);
                }
                match action {
                    ServerAction::Json(status, body) => {
                        let payload = serde_json::to_vec(&body).unwrap();
                        let reason = match status {
                            200 => "OK",
                            400 => "Bad Request",
                            403 => "Forbidden",
                            404 => "Not Found",
                            500 => "Internal Server Error",
                            503 => "Service Unavailable",
                            _ => "Status",
                        };
                        let resp = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            payload.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.write_all(&payload);
                    }
                    ServerAction::Raw(bytes) => {
                        let _ = stream.write_all(&bytes);
                    }
                    ServerAction::Delay(dur) => {
                        thread::sleep(dur);
                    }
                    ServerAction::CloseEmpty => {}
                }
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        Self {
            addr,
            recorded,
            accepted,
            handle: Some(handle),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.recorded.lock().unwrap().clone()
    }

    fn accepts(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    fn finish(mut self) -> Vec<RecordedRequest> {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.requests()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            // Wake the listener if it is still waiting on unused scripted actions.
            let _ = TcpStream::connect(self.addr);
            let _ = h.join();
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = std::str::from_utf8(&buf[..header_end]).ok()?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?.to_owned();
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in lines {
        let (k, v) = line.split_once(':')?;
        let val = v.trim().to_owned();
        if k.eq_ignore_ascii_case("content-length") {
            content_length = val.parse().ok()?;
        }
        headers.push((k.to_owned(), val));
    }
    let mut body_bytes = buf[header_end + 4..].to_vec();
    while body_bytes.len() < content_length {
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 {
            break;
        }
        body_bytes.extend_from_slice(&tmp[..n]);
    }
    let body = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    Some(RecordedRequest {
        request_line,
        headers,
        body,
    })
}

/// Injects TCP connections to a local `MockServer` while asserting the caller passes the exact
/// validated tailnet `SocketAddr`.
struct RedirectWay {
    expected: SocketAddr,
    target: Option<SocketAddr>,
    connect_err: Option<ErrorKind>,
}

impl WayOut for RedirectWay {
    fn resolve(&self, _name: &str) -> std::io::Result<Vec<IpAddr>> {
        Err(ErrorKind::Unsupported.into())
    }
    fn connect(&self, address: SocketAddr, within: Duration) -> std::io::Result<TcpStream> {
        assert_eq!(
            address, self.expected,
            "client connected to unexpected address"
        );
        if let Some(kind) = self.connect_err {
            return Err(kind.into());
        }
        TcpStream::connect_timeout(&self.target.unwrap(), within)
    }
}

/// Fake `System` for remote-client tests; records any subprocess call or file read so tests can
/// assert local `ont` or VPS delivery files are never touched in remote mode.
struct FakeHost {
    store_url_env: Option<OsString>,
    policy_file: Option<PathBuf>,
    client_policy_file: Option<PathBuf>,
    files: Vec<(String, Result<Vec<u8>, ErrorKind>)>,
    env: Vec<(String, String)>,
    way: Arc<dyn WayOut>,
    calls: Vec<(String, Vec<String>, Value)>,
    reads: RefCell<Vec<String>>,
    pauses: usize,
    clock: Instant,
    units_called: RefCell<usize>,
}

impl FakeHost {
    fn remote(url: &str, server: &MockServer) -> Self {
        let ep = parse_store_url(url).unwrap();
        Self {
            store_url_env: Some(OsString::from(url)),
            policy_file: None,
            client_policy_file: None,
            files: Vec::new(),
            env: Vec::new(),
            way: Arc::new(RedirectWay {
                expected: ep.addr,
                target: Some(server.addr),
                connect_err: None,
            }),
            calls: Vec::new(),
            reads: RefCell::new(Vec::new()),
            pauses: 0,
            clock: Instant::now(),
            units_called: RefCell::new(0),
        }
    }

    fn with_error_way(url: &str, kind: ErrorKind) -> Self {
        let ep = parse_store_url(url).unwrap();
        Self {
            store_url_env: Some(OsString::from(url)),
            policy_file: None,
            client_policy_file: None,
            files: Vec::new(),
            env: Vec::new(),
            way: Arc::new(RedirectWay {
                expected: ep.addr,
                target: None,
                connect_err: Some(kind),
            }),
            calls: Vec::new(),
            reads: RefCell::new(Vec::new()),
            pauses: 0,
            clock: Instant::now(),
            units_called: RefCell::new(0),
        }
    }
}

impl System for FakeHost {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _ms: u64) -> Answer {
        self.calls
            .push((program.to_owned(), args.to_vec(), input.clone()));
        Err(Failure {
            code: "process_failed",
            field: program.to_owned(),
            detail: Value::Null,
        })
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_owned());
        for (p, r) in &self.files {
            if p == path {
                return match r {
                    Ok(b) => Ok(b.clone()),
                    Err(k) => Err((*k).into()),
                };
            }
        }
        Err(ErrorKind::NotFound.into())
    }
    fn now(&self) -> Instant {
        self.clock
    }
    fn pause(&mut self, d: Duration) {
        self.pauses += 1;
        self.clock += d;
    }
    fn units(&self) -> bool {
        *self.units_called.borrow_mut() += 1;
        true
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy_file.clone()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
    fn way_out(&self) -> Arc<dyn WayOut> {
        Arc::clone(&self.way)
    }
    fn store_url_env(&self) -> Option<OsString> {
        self.store_url_env.clone()
    }
    fn client_policy_file(&self) -> Option<PathBuf> {
        self.client_policy_file.clone()
    }
}

fn err_of(a: Answer) -> Failure {
    match a {
        Ok(_) => panic!("expected error, got Ok reply"),
        Err(e) => e,
    }
}

fn empty_search() -> Value {
    json!({
        "status": "ok",
        "now": 1_795_000_000,
        "schema": {"work": {"version": 24}},
        "result": {"objects": [], "basis": {"qualified": true}, "continuation": null}
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ap-client-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// spec: specs/ap/client.md#R1
// spec: specs/ap/client.md#R2
// spec: specs/ap/client.md#R3
// spec: specs/ap/client.md#R4
#[test]
fn r1_to_r4_precedence_isolation_and_strict_tailnet_validation() {
    // Valid tailnet IPv4 and IPv6 endpoints.
    for (raw, want_host) in [
        ("http://100.64.0.0:1", "100.64.0.0:1"),
        (TAILNET_V4, "100.95.133.99:7432"),
        ("http://100.127.255.255:65535", "100.127.255.255:65535"),
        (TAILNET_V6, "[fd7a:115c:a1e0::1]:7432"),
    ] {
        let ep = parse_store_url(raw).unwrap();
        assert_eq!(ep.host, want_host, "{raw}");
    }
    // Rejected URL vectors: loopback, unspecified, non-tailnet, https, DNS, path/query/fragment,
    // userinfo, bad port, non-tailnet IPv6, whitespace/control.
    for bad in [
        "http://127.0.0.1:7432",
        "http://0.0.0.0:7432",
        "http://100.63.255.255:7432",
        "http://100.128.0.0:7432",
        "https://100.95.133.99:7432",
        "http://vps.ts.net:7432",
        "http://localhost:7432",
        "http://100.95.133.99:7432/",
        "http://100.95.133.99:7432/v1/get",
        "http://100.95.133.99:7432?a=1",
        "http://100.95.133.99:7432#x",
        "http://user@100.95.133.99:7432",
        "http://100.95.133.99:0",
        "http://100.95.133.99:65536",
        "http://100.95.133.99:07432",
        "http://100.95.133.99",
        "http://[::1]:7432",
        "http://[fe80::1]:7432",
        "http://[fd7a:115c:a1e1::1]:7432",
        "http://fd7a:115c:a1e0::1:7432",
        "http://100.95.133.99:7432 ",
        " http://100.95.133.99:7432",
        "http://100.95.133.99:7432\r\n",
    ] {
        assert!(parse_store_url(bad).is_none(), "must reject {bad:?}");
    }

    // Precedence (a): explicit `--store` always chooses local without reading env or policy.
    let mut host = FakeHost::with_error_way(TAILNET_V4, ErrorKind::ConnectionRefused);
    host.store_url_env = Some(OsString::from("http://127.0.0.1:7432"));
    host.client_policy_file = Some(PathBuf::from("/p.json"));
    host.files
        .push(("/p.json".into(), Ok(b"not-json".to_vec())));
    assert_eq!(resolve(&host, Some("/tmp/local.sqlite")).unwrap(), None);
    assert!(host.reads.borrow().is_empty());

    // Precedence (b): `AP_STORE_URL` beats policy `store_url`, and invalid/non-UTF-8 refuses.
    host.store_url_env = Some(OsString::from(TAILNET_V4));
    assert_eq!(
        resolve(&host, None).unwrap(),
        Some(parse_store_url(TAILNET_V4).unwrap())
    );
    assert!(host.reads.borrow().is_empty());

    host.store_url_env = Some(OsString::from("http://127.0.0.1:7432"));
    let err = resolve(&host, None).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_str()),
        ("store_url_invalid", "store_url")
    );

    host.store_url_env = Some(OsString::from_vec(vec![0xff, 0xfe]));
    let err = resolve(&host, None).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_str()),
        ("store_url_invalid", "store_url")
    );

    // Precedence (c): empty `AP_STORE_URL` falls through to `client_policy_file`.
    // Minimal PC policy without `models` or `accounts` works.
    host.store_url_env = Some(OsString::from(""));
    host.files = vec![(
        "/p.json".into(),
        Ok(json!({"store_url": TAILNET_V6}).to_string().into_bytes()),
    )];
    assert_eq!(
        resolve(&host, None).unwrap(),
        Some(parse_store_url(TAILNET_V6).unwrap())
    );

    // Policy without `store_url` or absent policy file selects local mode.
    host.store_url_env = None;
    host.files = vec![("/p.json".into(), Ok(b"{}".to_vec()))];
    assert_eq!(resolve(&host, None).unwrap(), None);
    host.files.clear();
    assert_eq!(resolve(&host, None).unwrap(), None);

    // Malformed / unreadable policy file refuses `policy_invalid` with detail starting with
    // `"file"` or `"store_url"`.
    for (content, prefix) in [
        (Err(ErrorKind::PermissionDenied), "file"),
        (Ok(b"{bad".to_vec()), "file"),
        (Ok(b"[]".to_vec()), "file"),
        (
            Ok(json!({"store_url": "https://100.95.133.99:7432"})
                .to_string()
                .into_bytes()),
            "store_url",
        ),
        (
            Ok(json!({"store_url": 123}).to_string().into_bytes()),
            "store_url",
        ),
    ] {
        host.files = vec![("/p.json".into(), content)];
        let err = resolve(&host, None).unwrap_err();
        assert_eq!((err.code, err.field.as_str()), ("policy_invalid", "policy"));
        assert!(err.detail.as_str().unwrap().starts_with(prefix), "{err:?}");
    }

    // Test-fake isolation: setting `policy_file` alone (leaving `client_policy_file` default None)
    // never triggers a client policy read.
    host.policy_file = Some(PathBuf::from("/p.json"));
    host.client_policy_file = None;
    host.reads.borrow_mut().clear();
    host.files = vec![("/p.json".into(), Ok(b"{bad".to_vec()))];
    assert_eq!(resolve(&host, None).unwrap(), None);
    assert!(host.reads.borrow().is_empty());
}

// spec: specs/ap/client.md#R5
// spec: specs/ap/client.md#R6
// spec: specs/ap/client.md#R7
#[test]
fn r5_r6_r7_http_format_headers_endpoints_and_local_refusals() {
    let ok = |extra: Value| {
        ServerAction::Json(
            200,
            json!({"status": "ok", "now": 1_795_000_000, "result": extra}),
        )
    };
    let server = MockServer::start(vec![
        ok(json!({"object_types": []})),                // describe
        ok(json!({"objects": []})),                     // around
        ok(json!({"groups": []})),                      // aggregate
        ok(json!({"actions": []})),                     // action list
        ok(json!({"valid": true})),                     // action validate
        ok(json!({"outcome": "applied", "edits": []})), // action apply
        ServerAction::Json(
            200,
            json!({"function": "capacity", "domain": "work", "answer": {"ok": true}, "elapsed_ms": 1}),
        ),
        ServerAction::Json(
            200,
            json!({"status": "ok", "truncated": false, "rows": 0, "markdown": "| Key |\n| --- |\n"}),
        ),
    ]);
    let mut host = FakeHost::remote(TAILNET_V4, &server);
    host.env.push(("ONT_ROLE".into(), "lead".into()));
    host.env.push(("ONT_AGENT".into(), "pc-agent".into()));
    let ep: Endpoint = parse_store_url(TAILNET_V4).unwrap();

    // 1. describe (null body -> {})
    let r = call_http(&host, &ep, &["describe".into()], &Value::Null, 5_000).unwrap();
    assert!(r.success);
    // 2. around
    call_http(&host, &ep, &["around".into()], &json!({"from": {}}), 5_000).unwrap();
    // 3. aggregate
    call_http(
        &host,
        &ep,
        &["aggregate".into()],
        &json!({"object_type": "unit"}),
        5_000,
    )
    .unwrap();
    // 4. action list
    call_http(
        &host,
        &ep,
        &["action".into(), "list".into()],
        &Value::Null,
        5_000,
    )
    .unwrap();
    // 5. action validate with custom --client
    call_http(
        &host,
        &ep,
        &[
            "action".into(),
            "validate".into(),
            "--client".into(),
            "ap-val".into(),
        ],
        &json!({"action_type": "record_request"}),
        5_000,
    )
    .unwrap();
    // 6. action apply strips --client and keeps body role
    call_http(
        &host,
        &ep,
        &[
            "action".into(),
            "apply".into(),
            "--client".into(),
            "ap".into(),
        ],
        &json!({"action_type": "record_request", "role": "lead"}),
        5_000,
    )
    .unwrap();
    // 7. fn <name> injects "name" into JSON body object
    let mut transport = Transport::new(&mut host, ep.clone());
    let ans = store::function(&mut transport, None, "capacity", &json!({"host": "pc"})).unwrap();
    assert_eq!(ans, json!({"ok": true}));
    // 8. render
    let keys =
        store::view_keys(&mut transport, None, "runs_of_unit", json!({"unit": "u1"})).unwrap();
    assert!(keys.is_empty());

    let reqs = server.finish();
    assert_eq!(reqs.len(), 8);
    assert_eq!(reqs[0].request_line, "POST /v1/describe HTTP/1.1");
    assert_eq!(reqs[0].header("host"), Some("100.95.133.99:7432"));
    assert_eq!(reqs[0].header("content-type"), Some("application/json"));
    assert_eq!(reqs[0].header("connection"), Some("close"));
    assert_eq!(reqs[0].header("x-ont-client"), Some("ap"));
    assert_eq!(reqs[0].header("x-ont-role"), Some("lead"));
    assert_eq!(reqs[0].header("x-ont-agent"), Some("pc-agent"));
    assert!(reqs[0].header("transfer-encoding").is_none());
    assert_eq!(reqs[0].body, json!({}));

    assert_eq!(reqs[1].request_line, "POST /v1/around HTTP/1.1");
    assert_eq!(reqs[2].request_line, "POST /v1/aggregate HTTP/1.1");
    assert_eq!(reqs[3].request_line, "POST /v1/action/list HTTP/1.1");
    assert_eq!(reqs[3].body, json!({}));
    assert_eq!(reqs[4].request_line, "POST /v1/action/validate HTTP/1.1");
    assert_eq!(reqs[4].header("x-ont-client"), Some("ap-val"));
    assert_eq!(reqs[5].request_line, "POST /v1/action/apply HTTP/1.1");
    assert_eq!(reqs[5].header("x-ont-client"), Some("ap"));
    assert_eq!(reqs[5].body["role"], "lead");
    assert_eq!(reqs[6].request_line, "POST /v1/fn HTTP/1.1");
    assert_eq!(
        reqs[6].body,
        json!({"name": "capacity", "parameters": {"host": "pc"}})
    );
    assert_eq!(reqs[7].request_line, "POST /v1/render HTTP/1.1");
    assert_eq!(
        reqs[7].body,
        json!({"view": "runs_of_unit", "args": {"unit": "u1"}})
    );

    // Optional env headers omitted when absent or empty.
    let s2 = MockServer::start(vec![ok(json!({}))]);
    let mut h2 = FakeHost::remote(TAILNET_V4, &s2);
    h2.env.push(("ONT_ROLE".into(), "".into()));
    call_http(&h2, &ep, &["describe".into()], &Value::Null, 5_000).unwrap();
    let r2 = s2.finish();
    assert!(r2[0].header("x-ont-role").is_none());
    assert!(r2[0].header("x-ont-agent").is_none());

    // Header injection and unserved commands refuse BEFORE connecting.
    let s3 = MockServer::start(vec![]);
    let mut h3 = FakeHost::remote(TAILNET_V4, &s3);
    for (bad_env, field) in [
        (("ONT_ROLE", "lead\r\nX-Evil: 1"), "ONT_ROLE"),
        (("ONT_ROLE", "lead\n"), "ONT_ROLE"),
        (("ONT_AGENT", "a\x00b"), "ONT_AGENT"),
        (("ONT_AGENT", "a\x7fb"), "ONT_AGENT"),
    ] {
        h3.env = vec![(bad_env.0.into(), bad_env.1.into())];
        let err = err_of(call_http(&h3, &ep, &["get".into()], &json!({}), 5_000));
        assert_eq!((err.code, err.field.as_str()), ("invalid_input", field));
    }
    h3.env.clear();
    for bad_client in ["ap\r\nEvil: 1", "ap\x1f", "ap\x7f", ""] {
        let err = err_of(call_http(
            &h3,
            &ep,
            &[
                "action".into(),
                "apply".into(),
                "--client".into(),
                bad_client.into(),
            ],
            &json!({}),
            5_000,
        ));
        assert_eq!((err.code, err.field.as_str()), ("invalid_input", "client"));
    }
    for (cmd, detail) in [
        (vec!["index", "--source", "s"], "index"),
        (vec!["backup"], "backup"),
        (vec!["schema", "check"], "schema"),
        (vec!["serve"], "serve"),
        (vec!["mcp"], "mcp"),
        (vec!["render", "--view", "v"], "render"),
    ] {
        let argv: Vec<String> = cmd.into_iter().map(str::to_owned).collect();
        let err = err_of(call_http(&h3, &ep, &argv, &json!({}), 5_000));
        assert_eq!(
            (err.code, err.field.as_str(), err.detail),
            ("operation_not_served", "command", json!(detail))
        );
    }
    assert_eq!(s3.accepts(), 0, "local refusals must not open TCP");
}

// spec: specs/ap/client.md#R5
// spec: specs/ap/client.md#R8
// spec: specs/ap/client.md#R9
#[test]
fn r8_r9_response_parsing_status_mapping_retries_and_unreachable() {
    let ep = parse_store_url(TAILNET_V4).unwrap();

    // 1. store_busy on 503 retries and succeeds; unavailable on 404 returns None;
    //    peer_not_identified (403), server_busy (503), not_granted (403) pass through verbatim;
    //    500 -> store_answer_unreadable.
    let server = MockServer::start(vec![
        ServerAction::Json(503, json!({"status": "refused", "code": "store_busy"})),
        ServerAction::Json(503, json!({"status": "refused", "code": "store_busy"})),
        ServerAction::Json(200, empty_search()),
        ServerAction::Json(404, json!({"status": "refused", "code": "unavailable"})),
        ServerAction::Json(
            403,
            json!({"status": "refused", "code": "peer_not_identified"}),
        ),
        ServerAction::Json(503, json!({"status": "refused", "code": "server_busy"})),
        ServerAction::Json(403, json!({"status": "refused", "code": "not_granted"})),
        ServerAction::Json(500, json!({"status": "error", "code": "internal"})),
    ]);
    let mut host = FakeHost::remote(TAILNET_V4, &server);
    let mut tr = Transport::new(&mut host, ep.clone());

    let found = store::search(&mut tr, None, "unit", vec![]).unwrap();
    assert!(found.is_empty());
    let missing = store::get(&mut tr, None, "intent", "nope").unwrap();
    assert!(missing.is_none());
    for code in ["peer_not_identified", "server_busy", "not_granted"] {
        let err = store::get(&mut tr, None, "intent", "k").unwrap_err();
        assert_eq!(store::code(&err), code);
    }
    let err500 = store::get(&mut tr, None, "intent", "k").unwrap_err();
    assert_eq!(err500.code, "store_answer_unreadable");
    assert_eq!(host.pauses, 2, "two store_busy retries paused twice");
    let reqs = server.finish();
    assert_eq!(reqs.len(), 8);

    // 2. Malformed HTTP responses -> store_answer_unreadable.
    let huge_headers = format!(
        "HTTP/1.1 200 OK\r\nX-Pad: {}\r\n\r\n{{}}",
        "a".repeat(17_000)
    );
    let bad_responses = vec![
        ServerAction::Raw(b"HTTP/1.1 200 OK\nContent-Length: 2\n\n{}".to_vec()),
        ServerAction::Raw(
            b"HTTP/1.1 200 OK\r\nX-Bad: a\rb\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
        ),
        ServerAction::Raw(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n".to_vec(),
        ),
        ServerAction::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: abc\r\n\r\n{}".to_vec()),
        ServerAction::Raw(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\n{}".to_vec(),
        ),
        ServerAction::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{}".to_vec()),
        ServerAction::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n{}".to_vec()),
        ServerAction::Raw(huge_headers.into_bytes()),
    ];
    let n_bad = bad_responses.len();
    let s_bad = MockServer::start(bad_responses);
    let h_bad = FakeHost::remote(TAILNET_V4, &s_bad);
    for i in 0..n_bad {
        let err = err_of(call_http(&h_bad, &ep, &["get".into()], &json!({}), 5_000));
        assert_eq!(err.code, "store_answer_unreadable", "case {i}");
    }
    s_bad.finish();

    // 3. Transport errors -> store_unreachable with fixed non-secret detail.
    let h_conn = FakeHost::with_error_way(TAILNET_V4, ErrorKind::ConnectionRefused);
    let err = err_of(call_http(&h_conn, &ep, &["get".into()], &json!({}), 5_000));
    assert_eq!(
        (err.code, err.field.as_str(), err.detail),
        ("store_unreachable", "store_url", json!("connect"))
    );

    let s_slow = MockServer::start(vec![
        ServerAction::Delay(Duration::from_millis(300)),
        ServerAction::CloseEmpty,
    ]);
    let h_slow = FakeHost::remote(TAILNET_V4, &s_slow);
    let err = err_of(call_http(&h_slow, &ep, &["get".into()], &json!({}), 80));
    assert_eq!(
        (err.code, err.field.as_str(), err.detail),
        ("store_unreachable", "store_url", json!("timeout"))
    );
    let err = err_of(call_http(&h_slow, &ep, &["get".into()], &json!({}), 5_000));
    assert_eq!(
        (err.code, err.field.as_str(), err.detail),
        ("store_unreachable", "store_url", json!("io"))
    );
    s_slow.finish();
}

// spec: specs/ap/client.md#R10
// spec: specs/ap/client.md#R11
#[test]
fn r10_r11_status_request_and_intent_run_over_http_and_skip_local_vps_files() {
    // 1. `ap status` overview over HTTP skips `/etc/agent-platform/deliver.json` and `last.json`.
    let server = MockServer::start(vec![
        ServerAction::Json(200, empty_search()), // requests
        ServerAction::Json(200, empty_search()), // intents
        ServerAction::Json(200, empty_search()), // units
        ServerAction::Json(200, empty_search()), // runs
        ServerAction::Json(200, empty_search()), // findings
        ServerAction::Json(200, empty_search()), // holds
    ]);
    let mut host = FakeHost::remote(TAILNET_V4, &server);
    host.files.push((
        "/etc/agent-platform/deliver.json".into(),
        Ok(json!({"state": "/var/lib/agent-platform/deliver"})
            .to_string()
            .into_bytes()),
    ));
    host.files.push((
        "/var/lib/agent-platform/deliver/last.json".into(),
        Ok(
            json!({"at": "2026-11-24T00:00:00Z", "end": "installed", "tip": "abc", "target": "ap"})
                .to_string()
                .into_bytes(),
        ),
    ));
    let out = status::execute(&mut host, None, &status::Call::Overview).unwrap();
    assert_eq!(out["delivery"], Value::Null);
    assert_eq!(out["delivery_unreadable"], json!(true));
    assert!(host.calls.is_empty(), "no local ont subprocess");
    assert!(
        host.reads.borrow().is_empty(),
        "local delivery files never read"
    );
    let reqs = server.finish();
    assert_eq!(reqs.len(), 6);
    assert!(
        reqs.iter()
            .all(|r| r.request_line == "POST /v1/search HTTP/1.1")
    );

    // 2. `ap status --wait` in remote mode skips local `watched()` / `units()` and returns `all_ended`.
    let intent_get = json!({
        "status": "ok",
        "now": 1_795_000_000,
        "result": {
            "objects": [{
                "id": {"object_type": "intent", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": "i1"},
                "head": "rev-1",
                "observations": [{"revision": "rev-1", "fields": {"status": {"kind": "value", "value": "confirmed"}}}]
            }]
        }
    });
    let s_wait = MockServer::start(vec![
        ServerAction::Json(200, intent_get),
        ServerAction::Json(200, empty_search()), // units
        ServerAction::Json(200, empty_search()), // findings
    ]);
    let mut h_wait = FakeHost::remote(TAILNET_V4, &s_wait);
    h_wait.policy_file = Some(PathBuf::from("/etc/agent-platform/policy.json"));
    let waited = status::execute(&mut h_wait, None, &status::Call::Wait("i1".into(), 60)).unwrap();
    assert_eq!(waited["reason"], "all_ended");
    assert_eq!(
        *h_wait.units_called.borrow(),
        0,
        "local systemd never queried"
    );
    assert!(h_wait.reads.borrow().is_empty());
    assert!(h_wait.calls.is_empty());
    s_wait.finish();

    // 3. `ap status --wait` fails fast on bad `AP_STORE_URL` or bad policy `store_url` with 0 pauses.
    let mut h_bad = FakeHost::with_error_way(TAILNET_V4, ErrorKind::ConnectionRefused);
    h_bad.store_url_env = Some(OsString::from("http://127.0.0.1:7432"));
    let err = status::execute(&mut h_bad, None, &status::Call::Wait("i1".into(), 60)).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_str()),
        ("store_url_invalid", "store_url")
    );
    assert_eq!(h_bad.pauses, 0);
    assert_eq!(status::exit_code(true, &err), 2);

    // 4. `ap request` record + readback over HTTP.
    let s_req = MockServer::start(vec![
        ServerAction::Json(
            200,
            json!({
                "status": "ok",
                "result": {
                    "outcome": "applied",
                    "edits": [{
                        "change": "initial",
                        "id": {"object_type": "request", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": "rq-1"}
                    }]
                }
            }),
        ),
        ServerAction::Json(
            200,
            json!({
                "status": "ok",
                "now": 1_795_000_000,
                "result": {
                    "objects": [{
                        "id": {"object_type": "request", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": "rq-1"},
                        "head": {"fields": {"words": {"kind": "value", "value": "ship from pc"}}}
                    }],
                    "basis": {"qualified": true},
                    "continuation": null
                }
            }),
        ),
    ]);
    let mut h_req = FakeHost::remote(TAILNET_V4, &s_req);
    let rec = request::execute(
        &mut h_req,
        None,
        &json!({
            "op": "record",
            "request_id": "pc-1",
            "agent": "herdr",
            "session": "s1",
            "topic": "pc/test",
            "kind": "order",
            "reading": "record from pc",
            "words": "ship from pc",
            "said_at": "2026-11-24T12:00:00Z"
        }),
    )
    .unwrap_or_else(|e| panic!("request refused: {} {} {}", e.code, e.field, e.detail));
    assert_eq!(rec["request"], "rq-1");
    assert_eq!(rec["record"], "applied");
    assert!(h_req.calls.is_empty());
    let reqs = s_req.finish();
    assert_eq!(reqs[0].request_line, "POST /v1/action/apply HTTP/1.1");
    assert_eq!(reqs[0].header("x-ont-client"), Some("ap"));
    assert_eq!(reqs[0].body["role"], "lead");
    assert_eq!(reqs[1].request_line, "POST /v1/search HTTP/1.1");

    // 5. `ap intent` show over HTTP.
    let s_int = MockServer::start(vec![
        ServerAction::Json(
            200,
            json!({
                "status": "ok",
                "now": 1_795_000_000,
                "result": {
                    "objects": [{
                        "id": {"object_type": "intent", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": "demo"},
                        "head": "r1",
                        "observations": [{"revision": "r1", "fields": {
                            "status": {"kind": "value", "value": "confirmed"},
                            "outcome": {"kind": "value", "value": "works remotely"},
                            "first_user": {"kind": "value", "value": "pc user"}
                        }}]
                    }]
                }
            }),
        ),
        ServerAction::Json(200, empty_search()), // criteria
        ServerAction::Json(200, empty_search()), // requests route=intent
        ServerAction::Json(200, empty_search()), // requests route=subject
        ServerAction::Json(
            200,
            json!({"status": "ok", "result": {"rows": [], "withheld": 0, "continuation": null}}),
        ), // log
    ]);
    let mut h_int = FakeHost::remote(TAILNET_V4, &s_int);
    let shown =
        intent::execute(&mut h_int, None, &json!({"op": "show", "intent": "demo"})).unwrap();
    assert_eq!(shown["intent"], "demo");
    assert_eq!(shown["outcome"], "works remotely");
    assert!(h_int.calls.is_empty());
    let int_reqs = s_int.finish();
    assert_eq!(int_reqs[0].request_line, "POST /v1/get HTTP/1.1");
    assert_eq!(int_reqs[4].request_line, "POST /v1/log HTTP/1.1");
}

// spec: specs/ap/client.md#R11
// spec: specs/ap/client.md#R12
// spec: specs/ap/client.md#R13
#[test]
fn r12_r13_cli_refuses_vps_drivers_in_remote_mode_and_preserves_bypasses() {
    let dir = scratch("cli-guard");
    let valid_policy = dir.join("policy-remote.json");
    std::fs::write(&valid_policy, json!({"store_url": TAILNET_V4}).to_string()).unwrap();
    let bad_policy = dir.join("policy-bad.json");
    std::fs::write(
        &bad_policy,
        json!({"store_url": "http://127.0.0.1:7432"}).to_string(),
    )
    .unwrap();
    let checkable_policy = dir.join("policy-full.json");
    std::fs::write(
        &checkable_policy,
        json!({
            "models": {"allowed": ["opus"], "never": []},
            "accounts": {"claude": ["/root/.claude"]},
            "store_url": TAILNET_V4
        })
        .to_string(),
    )
    .unwrap();
    let empty_policy = dir.join("absent.json");

    let run_ap = |args: &[&str], store_url: Option<&str>, policy_file: &PathBuf| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
        c.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("AP_POLICY_FILE", policy_file)
            .stdin(Stdio::null());
        if let Some(u) = store_url {
            c.env("AP_STORE_URL", u);
        }
        c.output().unwrap()
    };

    // All six VPS-only driver verbs refuse exit 2 with `remote_mode_unsupported` under both
    // `AP_STORE_URL` and policy `store_url`.
    let drivers: [(&[&str], &str); 6] = [
        (&["deliver"], "deliver"),
        (&["work"], "work"),
        (&["unit", "ready"], "unit ready"),
        (&["unit", "run", "k"], "unit run"),
        (&["unit", "review"], "unit review"),
        (&["unit", "write", "k"], "unit write"),
    ];
    for (argv, verb) in drivers {
        for (env_url, pol) in [(Some(TAILNET_V4), &empty_policy), (None, &valid_policy)] {
            let out = run_ap(argv, env_url, pol);
            assert_eq!(out.status.code(), Some(2), "{argv:?}");
            assert!(out.stdout.is_empty(), "{argv:?}");
            let e: Value = serde_json::from_slice(&out.stderr).unwrap();
            assert_eq!(
                e,
                json!({"error": {"code": "remote_mode_unsupported", "field": "verb", "detail": verb}}),
                "{argv:?}"
            );
        }
        // Malformed `AP_STORE_URL` refuses `store_url_invalid` (exit 2), never falling back.
        let out = run_ap(argv, Some("http://127.0.0.1:7432"), &empty_policy);
        assert_eq!(out.status.code(), Some(2), "{argv:?}");
        assert!(out.stdout.is_empty(), "{argv:?}");
        let e: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(e["error"]["code"], "store_url_invalid", "{argv:?}");
    }

    // Malformed policy `store_url` on driver verb refuses `policy_invalid` (exit 2).
    let out = run_ap(&["work"], None, &bad_policy);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(e["error"]["code"], "policy_invalid");

    // `ap status --wait` CLI fast-fails with exit 2 (not exit 3) on bad `AP_STORE_URL` and bad policy.
    for (env_url, pol, want_code) in [
        (
            Some("http://127.0.0.1:7432"),
            &empty_policy,
            "store_url_invalid",
        ),
        (None, &bad_policy, "policy_invalid"),
    ] {
        let out = run_ap(&["status", "--wait", "i1"], env_url, pol);
        assert_eq!(out.status.code(), Some(2), "{want_code}");
        assert!(out.stdout.is_empty(), "{want_code}");
        let e: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(e["error"]["code"], want_code);
    }

    // Bypasses surviving malformed `AP_STORE_URL` and malformed policy file:
    // `--help`, `-h`, `<verb> --help`, `version`, `--version`, `policy check`.
    for argv in [
        &["--help"][..],
        &["-h"],
        &["deliver", "--help"],
        &["work", "--help"],
        &["unit", "--help"],
        &["version"],
        &["--version"],
    ] {
        let out = run_ap(argv, Some("not-a-url"), &bad_policy);
        assert_eq!(out.status.code(), Some(0), "{argv:?}");
        assert!(!out.stdout.is_empty(), "{argv:?}");
    }
    let out = run_ap(
        &["policy", "check", checkable_policy.to_str().unwrap()],
        Some("not-a-url"),
        &bad_policy,
    );
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");

    // Explicit `--store` bypasses the remote guard even with malformed `AP_STORE_URL` / policy.
    let store_path = dir.join("scratch.sqlite");
    let store_str = store_path.to_str().unwrap();
    for argv in [
        vec!["work", "--store", store_str],
        vec!["unit", "ready", "--store", store_str],
        vec!["unit", "run", "k", "--store", store_str],
        vec!["unit", "review", "--store", store_str],
        vec!["unit", "write", "k", "--store", store_str],
    ] {
        let out = run_ap(&argv, Some("http://127.0.0.1:7432"), &bad_policy);
        let stderr_str = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr_str.contains("store_url_invalid")
                && !stderr_str.contains("remote_mode_unsupported"),
            "{argv:?} unexpectedly hit remote guard: {stderr_str}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}
