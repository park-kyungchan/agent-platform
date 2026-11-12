mod support;
use replay::{
    Fact, MentalModel, RecordedReply, Replay, ReplayStop, Shape, TraceStep, WireRequest,
    assert_recorded_keys, project,
};
use serde_json::{Value, json};
use support::replay;

const ROOT: &str = "/v1/default/banks/fixture-bank";
fn request(method: &str, route: &str, body: Value) -> WireRequest {
    WireRequest::new(method, &format!("{ROOT}{route}"), body)
}
fn checked(r: &mut Replay, q: WireRequest, recording: &'static str) -> RecordedReply {
    let reply = r.apply(q).expect("recorded request");
    assert_recorded_keys(&reply, &Shape { recording }).unwrap();
    reply
}
fn read(r: &mut Replay, route: &str, recording: &'static str) -> Value {
    checked(r, request("GET", route, Value::Null), recording).body
}
fn retain(r: &mut Replay, op: &str, text: &str, hash: bool, recording: &'static str) -> Value {
    let mut item = json!({"document_id":"fixture-document","content":text,"tags":["fixture-tag","fixture-tag"],"observation_scopes":[],"update_mode":"replace"});
    if hash {
        item["metadata"] = json!({"body_hash":"fixture-body-digest"});
    }
    checked(
        r,
        request(
            "POST",
            "/memories",
            json!({"operation_id":op,"async":true,"items":[item]}),
        ),
        recording,
    )
    .body
}
fn recall_args(tag: &str) -> Value {
    json!({"query":"invented","tags":[tag],"tags_match":"all_strict"})
}
fn complete(r: &mut Replay, op: &str) {
    if let Some(tag) = r.accepted_documents(op)[0].tags.first().cloned() {
        r.advance(TraceStep::Recall {
            query: "invented".into(),
            tags: vec![tag],
        });
    }
    r.advance(TraceStep::Retained {
        operation_id: op.into(),
        child_id: format!("{op}-child"),
        facts: vec![Fact {
            id: format!("{op}-fact"),
            document_id: "fixture-document".into(),
            text: format!("invented {op} fact"),
        }],
    });
}
fn rpc(r: &mut Replay, method: &str, params: Value, recording: &'static str) -> RecordedReply {
    let mut q = WireRequest::new(
        "POST",
        "/mcp/fixture-bank/",
        json!({"jsonrpc":"2.0","id":9,"method":method,"params":params}),
    );
    if method != "initialize" {
        q.session = Some("fixture-session".into());
    }
    checked(r, q, recording)
}

#[test]
fn recorded_keys_only() {
    let mut r = Replay::default();
    // These are independently selected expected variants, not a shape chosen by project.
    let reply = checked(&mut r, request("GET", "/config", Value::Null), "006");
    let mut extra = reply.clone();
    extra.body["config"]["unknown_internal_key"] = json!(true);
    assert!(assert_recorded_keys(&extra, &Shape { recording: "006" }).is_err());
    let mut missing = reply.clone();
    missing.body["config"]
        .as_object_mut()
        .unwrap()
        .remove("enable_observations");
    assert!(assert_recorded_keys(&missing, &Shape { recording: "006" }).is_err());
    let mut wrong = reply.clone();
    wrong.body["config"]["retain_custom_instructions"] = json!({});
    assert!(assert_recorded_keys(&wrong, &Shape { recording: "006" }).is_err());
    let mut status = reply;
    status.status = 404;
    assert!(assert_recorded_keys(&status, &Shape { recording: "006" }).is_err());
    let mut defaults = Replay::upstream_defaults();
    let config = read(&mut defaults, "/config", "161");
    assert_eq!(config["config"]["enable_observations"], true);
    assert_eq!(config["config"]["enable_auto_consolidation"], true);
    let mut absent = Replay::absent();
    read(&mut absent, "/mental-models", "465");
    read(&mut r, "/mental-models", "470");
    let mut models = Replay::with_models(vec![MentalModel {
        id: "fixture-model".into(),
        name: "invented model".into(),
        tags: vec!["fixture-tag".into()],
    }]);
    let one = read(&mut models, "/mental-models", "472");
    assert_eq!(one["total"], 1);
    assert_eq!(one["items"].as_array().unwrap().len(), 1);
    let page = read(
        &mut models,
        "/mental-models?tags=fixture-tag&tags_match=exact&detail=full&limit=1&offset=1&exclude_knowledge_pages=false",
        "473",
    );
    assert_eq!(page, json!({"items":[],"total":1,"limit":1,"offset":1}));
    let mut injected = RecordedReply {
        body: one,
        ..checked(
            &mut models,
            request("GET", "/mental-models", Value::Null),
            "472",
        )
    };
    injected.body["items"][0]["test_target"] = json!("forbidden");
    assert!(assert_recorded_keys(&injected, &Shape { recording: "472" }).is_err());
    // No cross-variant union: a plain document cannot acquire body_hash metadata.
    retain(&mut r, "fixture-op-plain", "plain", false, "010");
    complete(&mut r, "fixture-op-plain");
    let mut doc = checked(
        &mut r,
        request("GET", "/documents/fixture-document", Value::Null),
        "016",
    );
    doc.body["document_metadata"] = json!({"body_hash":"invented"});
    assert!(assert_recorded_keys(&doc, &Shape { recording: "016" }).is_err());
    assert!(assert_recorded_keys(&doc, &Shape { recording: "060" }).is_err()); // retain_params still plain
    let mut auth = request("GET", "/documents/fixture-document", Value::Null);
    auth.authorized = false;
    assert_eq!(checked(&mut r, auth, "110").status, 401);
}

#[test]
fn measured_replace_delete() {
    let mut r = Replay::default();
    let old = "invented old bytes\r\nwith duplicate tags";
    retain(&mut r, "fixture-op-old", old, false, "010");
    retain(
        &mut r,
        "fixture-op-old",
        "ignored changed bytes",
        false,
        "012",
    );
    assert_eq!(r.accepted_documents("fixture-op-old")[0].original_text, old);
    complete(&mut r, "fixture-op-old");
    assert_eq!(
        read(&mut r, "/documents/fixture-document", "016")["original_text"],
        old
    );
    read(&mut r, "/documents/fixture-document", "024");
    read(&mut r, "/memories/list?limit=1&offset=0", "025");
    read(&mut r, "/memories/list?limit=1&offset=1", "033");
    read(&mut r, "/documents?limit=1&offset=0", "030");
    read(&mut r, "/documents?limit=1&offset=1", "031");
    retain(
        &mut r,
        "fixture-op-old",
        "ignored completed bytes",
        false,
        "054",
    );
    assert_eq!(r.active_operations(), 0);
    checked(
        &mut r,
        request(
            "POST",
            "/memories",
            json!({"operation_id":"fixture-op-new","async":true,"items":[{"document_id":"fixture-document","content":"invented replacement","tags":["fixture-new-tag","fixture-new-tag"],"metadata":{"body_hash":"fixture-body-digest"},"observation_scopes":[],"update_mode":"replace"}]}),
        ),
        "075",
    );
    complete(&mut r, "fixture-op-new");
    let doc = read(&mut r, "/documents/fixture-document", "060");
    assert_eq!(doc["original_text"], "invented replacement");
    assert_eq!(doc["tags"], json!(["fixture-new-tag", "fixture-new-tag"]));
    let facts = read(&mut r, "/memories/list?document_id=fixture-document", "061");
    assert_eq!(facts["total"], 1);
    assert_eq!(facts["items"][0]["text"], "invented fixture-op-new fact");
    assert!(!facts.to_string().contains("fixture-op-old"));
    assert_eq!(
        facts["items"][0]["tags"],
        json!(["fixture-new-tag", "fixture-new-tag"])
    );
    assert_eq!(
        facts["items"][0]["metadata"]["body_hash"],
        "fixture-body-digest"
    );
    checked(
        &mut r,
        request("POST", "/memories/recall", recall_args("fixture-new-tag")),
        "062",
    );
    retain(&mut r, "fixture-op-return", old, false, "079");
    complete(&mut r, "fixture-op-return");
    assert_eq!(
        read(&mut r, "/documents/fixture-document", "016")["original_text"],
        old
    );
    let deleted = checked(
        &mut r,
        request("DELETE", "/documents/fixture-document", Value::Null),
        "124",
    );
    assert_eq!(deleted.body["memory_units_deleted"], 1);
    read(&mut r, "/documents/fixture-document", "125");
    assert_eq!(
        read(&mut r, "/memories/list?document_id=fixture-document", "126")["items"],
        json!([])
    );
    read(&mut r, "/documents/fixture-document/chunks", "127");
    assert_eq!(
        checked(
            &mut r,
            request("DELETE", "/documents/fixture-document", Value::Null),
            "128"
        )
        .status,
        404
    );
    read(&mut r, "/mental-models", "475");
}

#[test]
fn no_internal_operation_fields() {
    fn walk(v: &Value) {
        match v {
            Value::Object(o) => {
                for (key, v) in o {
                    assert!(!["request", "test_order", "test_target"].contains(&key.as_str()));
                    walk(v);
                }
            }
            Value::Array(a) => {
                for v in a {
                    walk(v);
                }
            }
            _ => {}
        }
    }
    let mut r = Replay::default();
    let mut q = request(
        "POST",
        "/memories",
        json!({"operation_id":"fixture-op-private","async":true,"items":[{"document_id":"fixture-document","content":"invented FUTURE private payload","tags":[],"test_order":8123,"test_target":"hidden"}],"test_order":8123,"request":"hidden"}),
    );
    let ack = checked(&mut r, q.clone(), "010");
    walk(&ack.body);
    assert!(!ack.body.to_string().contains("FUTURE"));
    r.advance(TraceStep::ParentPendingChildError {
        operation_id: "fixture-op-private".into(),
        child_id: "fixture-failing-child".into(),
    });
    let pending = read(
        &mut r,
        "/operations/fixture-op-private?include_payload=false",
        "434",
    );
    walk(&pending);
    assert_eq!(pending["status"], "pending");
    assert!(pending["task_payload"].is_null());
    assert!(!pending.to_string().contains("FUTURE"));
    assert_eq!(
        read(
            &mut r,
            "/operations/fixture-op-private?include_payload=false",
            "446"
        ),
        pending
    );
    q.body["items"][0]["content"] = json!("different");
    checked(&mut r, q, "012");
    complete(&mut r, "fixture-op-private");
    let list = read(&mut r, "/operations?document_id=fixture-document", "018");
    walk(&list);
    assert_eq!(list["total"], 2);
    assert!(!list.to_string().contains("FUTURE"));
    let child = read(
        &mut r,
        "/operations/fixture-op-private-child?include_payload=true",
        "116",
    );
    walk(&child);
    assert_eq!(
        child["task_payload"]["contents"][0]["content"],
        "invented FUTURE private payload"
    );
    assert_eq!(
        child["result_metadata"]["parent_operation_id"],
        "fixture-op-private"
    );
    let mut injected = RecordedReply {
        status: 200,
        headers: Default::default(),
        body: child,
    };
    injected.body["task_payload"]["contents"][0]["test_order"] = json!(1);
    assert!(assert_recorded_keys(&injected, &Shape { recording: "116" }).is_err());
    let mut background = Replay::default();
    background.advance(TraceStep::RefreshCompleted);
    read(&mut background, "/operations", "072");
    let mut maintenance = Replay::default();
    maintenance.advance(TraceStep::MaintenanceCompleted);
    read(&mut maintenance, "/operations", "073");
}

#[test]
fn rpc_and_observation_shapes() {
    let mut r = Replay::default();
    retain(
        &mut r,
        "fixture-rpc-op",
        "invented exact UTF-8: Ω\n",
        false,
        "010",
    );
    complete(&mut r, "fixture-rpc-op");
    let init = rpc(&mut r, "initialize", json!({}), "007");
    assert_eq!(init.headers["mcp-session-id"], "fixture-session");
    rpc(&mut r, "tools/list", json!({}), "021");
    let doc = rpc(
        &mut r,
        "tools/call",
        json!({"name":"get_document","arguments":{"document_id":"fixture-document"}}),
        "109",
    );
    assert!(doc.body.as_str().unwrap().contains("invented exact UTF-8"));
    let recall = rpc(
        &mut r,
        "tools/call",
        json!({"name":"recall","arguments":recall_args("fixture-tag")}),
        "036",
    );
    let s = recall.body.as_str().unwrap();
    let data = s.lines().find_map(|l| l.strip_prefix("data: ")).unwrap();
    let mut body: Value = serde_json::from_str(data).unwrap();
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let mut nested: Value = serde_json::from_str(text).unwrap();
    nested["results"][0]["unrecorded"] = json!(true);
    body["result"]["content"][0]["text"] = json!(nested.to_string());
    let bad = RecordedReply {
        body: json!(format!("event: message\ndata: {body}\n\n")),
        ..recall.clone()
    };
    assert!(assert_recorded_keys(&bad, &Shape { recording: "036" }).is_err());
    let mut bad = recall;
    bad.headers.remove("mcp-session-id");
    assert!(assert_recorded_keys(&bad, &Shape { recording: "036" }).is_err());
    r.advance(TraceStep::Observations(vec![Fact {
        id: "fixture-observation".into(),
        document_id: String::new(),
        text: "invented observation".into(),
    }]));
    let stats = read(&mut r, "/stats", "181");
    assert_eq!(stats["total_observations"], 1);
    read(&mut r, "/memories/list?type=observation", "190");
    // project alone cannot mutate state or turn a read into a trace advance.
    let q = request("GET", "/documents/fixture-document", Value::Null);
    assert_recorded_keys(&project(&r, &q).unwrap(), &Shape { recording: "016" }).unwrap();
}

#[test]
fn hashed_mcp_recall_uses_recorded_empty_metadata() {
    let mut r = Replay::default();
    retain(
        &mut r,
        "fixture-hashed-recall",
        "invented hashed body",
        true,
        "010",
    );
    complete(&mut r, "fixture-hashed-recall");
    rpc(&mut r, "initialize", json!({}), "007");
    let reply = rpc(
        &mut r,
        "tools/call",
        json!({"name":"recall","arguments":recall_args("fixture-tag")}),
        "036",
    );
    assert!(!reply.body.as_str().unwrap().contains("body_hash"));
}

#[test]
fn recorded_crlf_frames_are_accepted() {
    let mut r = Replay::default();
    retain(&mut r, "fixture-crlf", "invented frame body", false, "010");
    complete(&mut r, "fixture-crlf");
    let frames = [
        (rpc(&mut r, "initialize", json!({}), "007"), "007"),
        (rpc(&mut r, "tools/list", json!({}), "021"), "021"),
        (
            rpc(
                &mut r,
                "tools/call",
                json!({"name":"recall","arguments":recall_args("fixture-tag")}),
                "036",
            ),
            "036",
        ),
        (
            rpc(
                &mut r,
                "tools/call",
                json!({"name":"get_document","arguments":{"document_id":"fixture-document"}}),
                "109",
            ),
            "109",
        ),
    ];
    for (mut reply, recording) in frames {
        assert!(
            reply
                .body
                .as_str()
                .unwrap()
                .starts_with("event: message\r\n")
        );
        assert!(reply.body.as_str().unwrap().ends_with("\r\n\r\n"));
        // Only framing comes from the recordings; all JSON values remain invented.
        let s = reply
            .body
            .as_str()
            .unwrap()
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        reply.body = json!(s);
        assert_recorded_keys(&reply, &Shape { recording }).unwrap();
    }
}

#[test]
fn unknown_operation_matches_recording_019() {
    let mut r = Replay::default();
    let q = request(
        "GET",
        "/operations/fixture-unknown?include_payload=true",
        Value::Null,
    );
    let reply = r.apply(q).expect("019 is a recorded lookup");
    assert_recorded_keys(&reply, &Shape { recording: "019" }).unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body["status"], "not_found");
    assert_eq!(reply.body["operation_id"], "fixture-unknown");
    assert!(matches!(
        Replay::absent().apply(request("GET", "/operations/fixture-unknown", Value::Null)),
        Err(ReplayStop::NotRecorded("operation_lookup_http404"))
    ));
    for (key, value) in reply.body.as_object().unwrap() {
        if !["id", "operation_id", "status"].contains(&key.as_str()) {
            assert!(value.is_null());
        }
    }
}
#[test]
fn completed_parent_is_typed_stop() {
    let mut r = Replay::default();
    retain(
        &mut r,
        "fixture-completed-parent",
        "invented parent body",
        false,
        "010",
    );
    complete(&mut r, "fixture-completed-parent");
    let q = request(
        "GET",
        "/operations/fixture-completed-parent?include_payload=true",
        Value::Null,
    );
    let stop =
        std::panic::catch_unwind(|| project(&r, &q)).expect("completed parent must not panic");
    assert!(matches!(stop, Err(ReplayStop::NotRecorded(_))));
}

#[test]
fn apply_does_not_emit_an_unrecorded_variant() {
    let mut r = Replay::default();
    retain(
        &mut r,
        "fixture-unrecorded",
        "invented hashed document",
        true,
        "010",
    );
    complete(&mut r, "fixture-unrecorded");
    rpc(&mut r, "initialize", json!({}), "007");
    let mut q = WireRequest::new(
        "POST",
        "/mcp/fixture-bank/",
        json!({"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"get_document","arguments":{"document_id":"fixture-document"}}}),
    );
    q.session = Some("fixture-session".into());
    // This cannot pass through shape109 (which records null document metadata).
    assert!(matches!(r.apply(q), Err(ReplayStop::ShapeMismatch(_))));
}

fn wire_checked(
    server: &support::upstream::Server,
    comparison: &mut Replay,
    q: WireRequest,
    recording: &'static str,
) -> RecordedReply {
    // Transport comparison; the separate recorded shape is still the key oracle.
    let expected = comparison.apply(q.clone()).unwrap();
    let reply = server.request(q).unwrap();
    assert_eq!(reply.status, expected.status);
    assert_eq!(reply.body, expected.body);
    assert_recorded_keys(&reply, &Shape { recording }).unwrap();
    reply
}
fn wire_rpc(
    server: &support::upstream::Server,
    comparison: &mut Replay,
    method: &str,
    params: Value,
    recording: &'static str,
) -> RecordedReply {
    let mut q = WireRequest::new(
        "POST",
        "/mcp/fixture-bank/",
        json!({"jsonrpc":"2.0","id":51,"method":method,"params":params}),
    );
    if method != "initialize" {
        q.session = Some("fixture-session".into());
    }
    wire_checked(server, comparison, q, recording)
}
fn wire_complete(server: &support::upstream::Server, comparison: &mut Replay) {
    comparison.advance(TraceStep::Recall {
        query: "invented".into(),
        tags: vec!["fixture-wire-tag".into()],
    });
    server.advance(TraceStep::Recall {
        query: "invented".into(),
        tags: vec!["fixture-wire-tag".into()],
    });
    comparison.advance(TraceStep::Retained {
        operation_id: "fixture-wire-operation".into(),
        child_id: "fixture-wire-child".into(),
        facts: vec![Fact {
            id: "fixture-wire-fact".into(),
            document_id: "fixture-wire-document".into(),
            text: "invented wire fact".into(),
        }],
    });
    server.advance(TraceStep::Retained {
        operation_id: "fixture-wire-operation".into(),
        child_id: "fixture-wire-child".into(),
        facts: vec![Fact {
            id: "fixture-wire-fact".into(),
            document_id: "fixture-wire-document".into(),
            text: "invented wire fact".into(),
        }],
    });
}
fn wire_retain() -> WireRequest {
    request(
        "POST",
        "/memories",
        json!({"operation_id":"fixture-wire-operation","async":true,"items":[{"document_id":"fixture-wire-document","content":"invented wire bytes Ω\r\n","tags":["fixture-wire-tag"]}]}),
    )
}
#[test]
fn every_wire_reply_projected() {
    let mut comparison = Replay::default();
    let mut server = support::upstream::Server::start(comparison.clone());
    wire_checked(
        &server,
        &mut comparison,
        request("GET", "/config", Value::Null),
        "006",
    );
    wire_checked(
        &server,
        &mut comparison,
        request("GET", "/mental-models", Value::Null),
        "470",
    );
    wire_checked(&server, &mut comparison, wire_retain(), "010");
    wire_complete(&server, &mut comparison);
    for (route, shape) in [
        ("/documents/fixture-wire-document", "016"),
        ("/memories/list", "025"),
        ("/documents?limit=1&offset=0", "030"),
        ("/operations?document_id=fixture-wire-document", "018"),
        ("/operations/fixture-absent?include_payload=true", "019"),
    ] {
        wire_checked(
            &server,
            &mut comparison,
            request("GET", route, Value::Null),
            shape,
        );
    }
    wire_checked(
        &server,
        &mut comparison,
        request("POST", "/memories/recall", recall_args("fixture-wire-tag")),
        "062",
    );
    wire_rpc(&server, &mut comparison, "initialize", json!({}), "007");
    wire_rpc(&server, &mut comparison, "tools/list", json!({}), "021");
    wire_rpc(
        &server,
        &mut comparison,
        "tools/call",
        json!({"name":"recall","arguments":recall_args("fixture-wire-tag")}),
        "036",
    );
    wire_rpc(
        &server,
        &mut comparison,
        "tools/call",
        json!({"name":"get_document","arguments":{"document_id":"fixture-wire-document"}}),
        "109",
    );
    let mut denied = request("GET", "/documents/fixture-wire-document", Value::Null);
    denied.authorized = false;
    wire_checked(&server, &mut comparison, denied, "110");
    wire_checked(
        &server,
        &mut comparison,
        request("DELETE", "/documents/fixture-wire-document", Value::Null),
        "124",
    );
    wire_checked(
        &server,
        &mut comparison,
        request("GET", "/documents/fixture-wire-document", Value::Null),
        "125",
    );
    wire_checked(
        &server,
        &mut comparison,
        request("GET", "/memories/list", Value::Null),
        "126",
    );
    server.stop_and_join().unwrap();
}
fn raw_wire(address: std::net::SocketAddr, bytes: &[u8]) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(1)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    let mut result = Vec::new();
    let read = stream.take(131072).read_to_end(&mut result);
    assert!(read.is_ok() || read.unwrap_err().kind() == std::io::ErrorKind::ConnectionReset);
    result
}
#[test]
fn bodyless_get_delete() {
    let mut comparison = Replay::default();
    let mut server = support::upstream::Server::start(comparison.clone());
    let bytes=raw_wire(server.owned_address(),format!("GET {ROOT}/config HTTP/1.1\r\nHost: fixture\r\nAuthorization: Bearer fixture-test-token\r\nConnection: close\r\n\r\n").as_bytes());
    let split = bytes.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let reply = RecordedReply {
        status: 200,
        headers: Default::default(),
        body: serde_json::from_slice(&bytes[split + 4..]).unwrap(),
    };
    assert_recorded_keys(&reply, &Shape { recording: "006" }).unwrap();
    wire_checked(&server, &mut comparison, wire_retain(), "010");
    wire_complete(&server, &mut comparison);
    let bytes=raw_wire(server.owned_address(),format!("DELETE {ROOT}/documents/fixture-wire-document HTTP/1.1\r\nHost: fixture\r\nAuthorization: Bearer fixture-test-token\r\nConnection: close\r\n\r\n").as_bytes());
    assert!(bytes.starts_with(b"HTTP/1.1 200 "));
    let split = bytes.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let reply = RecordedReply {
        status: 200,
        headers: Default::default(),
        body: serde_json::from_slice(&bytes[split + 4..]).unwrap(),
    };
    assert_recorded_keys(&reply, &Shape { recording: "124" }).unwrap();
    assert_eq!(reply.body["memory_units_deleted"], 1);
    let absent = server
        .request(request(
            "GET",
            "/documents/fixture-wire-document",
            Value::Null,
        ))
        .unwrap();
    assert_recorded_keys(&absent, &Shape { recording: "125" }).unwrap();
    server.stop_and_join().unwrap();
}
#[test]
fn transfer_encoding_refused() {
    let mut server = support::upstream::Server::start(Replay::default());
    for headers in [
        "Transfer-Encoding: chunked\r\n".to_owned(),
        "Content-Length: 0\r\nContent-Length: 0\r\n".into(),
        "Content-Length: 65537\r\n".into(),
        format!("X-Large: {}\r\n", "x".repeat(8192)),
    ] {
        let bytes = raw_wire(
            server.owned_address(),
            format!("POST {ROOT}/memories HTTP/1.1\r\nHost: fixture\r\n{headers}\r\n").as_bytes(),
        );
        assert!(
            bytes.is_empty(),
            "transport refusal must not manufacture a native answer"
        );
    }
    let reply = server
        .request(request("GET", "/operations", Value::Null))
        .unwrap();
    assert_recorded_keys(&reply, &Shape { recording: "018" }).unwrap();
    assert_eq!(reply.body["total"], 0);
    server.stop_and_join().unwrap();
}
#[test]
fn worker_joined() {
    let mut server = support::upstream::Server::start(Replay::default());
    assert!(server.owned_address().ip().is_loopback());
    assert_ne!(server.owned_address().port(), 0);
    server
        .request(request("GET", "/config", Value::Null))
        .unwrap();
    server.stop_and_join().unwrap();
    server.stop_and_join().unwrap();
    assert_eq!(
        server
            .request(request("GET", "/config", Value::Null))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotConnected
    );
    drop(support::upstream::Server::start(Replay::default()));
}
#[test]
fn proxy_uses_replay_bridge() {
    use support::proxy::{FaultProxy, Frame, Step};
    let server = support::upstream::Server::start(Replay::default());
    let mut proxy = FaultProxy::start(
        server.into_owned_backend(),
        vec![Step::ForwardThenDropAnswer],
    );
    let exchange = |q: WireRequest| {
        let mut stream = std::net::TcpStream::connect_timeout(
            &proxy.address(),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        Frame {
            head: format!(
                "{} {} HTTP/1.1\r\nHost: fixture\r\nAuthorization: Bearer fixture-test-token",
                q.method, q.path
            ),
            body: if q.body.is_null() {
                Vec::new()
            } else {
                serde_json::to_vec(&q.body).unwrap()
            },
        }
        .write(&mut stream)
        .unwrap();
        Frame::read(&mut stream)
    };
    assert!(exchange(wire_retain()).is_err());
    let lookup = exchange(request("GET", "/operations", Value::Null)).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&lookup.body).unwrap()["total"],
        1
    );
    proxy.stop_and_join().unwrap();
    assert!(proxy.transcript()[0].response_received);
}

#[test]
fn no_behaviour_or_model_serializer() {
    let wire = include_str!("support/upstream.rs");
    for forbidden in [
        "struct Behaviour",
        "enum MutationFault",
        "struct Model",
        "Model::",
        "serde::Serialize",
        "json!",
    ] {
        assert!(
            !wire.contains(forbidden),
            "retired model/projection bypass remains"
        );
    }
    assert!(!include_str!("support/mod.rs").contains("fixture_server"));
    assert_eq!(wire.matches(".apply(request)").count(), 1);
    assert_eq!(wire.matches("write_reply(&mut stream, &reply)").count(), 1);
    assert!(!include_str!("support/replay.rs").contains("serde::Serialize"));
}
#[test]
fn key_sets_all_routes() {
    let one_model = || {
        Replay::with_models(vec![MentalModel {
            id: "fixture-route-model".into(),
            name: "invented route model".into(),
            tags: vec!["fixture-route-tag".into()],
        }])
    };
    let mut observations = Replay::default();
    observations.advance(TraceStep::Observations(vec![Fact {
        id: "fixture-route-observation".into(),
        document_id: String::new(),
        text: "invented route observation".into(),
    }]));
    let mut refresh = Replay::default();
    refresh.advance(TraceStep::RefreshCompleted);
    let mut maintenance = Replay::default();
    maintenance.advance(TraceStep::MaintenanceCompleted);
    for (mut comparison, route, shape) in [
        (Replay::upstream_defaults(), "/config", "161"),
        (Replay::absent(), "/mental-models", "465"),
        (one_model(), "/mental-models", "472"),
        (
            one_model(),
            "/mental-models?tags=fixture-route-tag&tags_match=exact&detail=full&limit=1&offset=1&exclude_knowledge_pages=false",
            "473",
        ),
        (observations.clone(), "/stats", "181"),
        (observations, "/memories/list?type=observation", "190"),
        (refresh, "/operations", "072"),
        (maintenance, "/operations", "073"),
    ] {
        let mut server = support::upstream::Server::start(comparison.clone());
        wire_checked(
            &server,
            &mut comparison,
            request("GET", route, Value::Null),
            shape,
        );
        server.stop_and_join().unwrap();
    }
    let mut comparison = Replay::default();
    retain(
        &mut comparison,
        "fixture-routes-hash",
        "invented hashed route body",
        true,
        "010",
    );
    complete(&mut comparison, "fixture-routes-hash");
    let mut server = support::upstream::Server::start(comparison.clone());
    wire_checked(
        &server,
        &mut comparison,
        request("GET", "/documents/fixture-document", Value::Null),
        "060",
    );
    wire_checked(
        &server,
        &mut comparison,
        request("GET", "/memories/list", Value::Null),
        "061",
    );
    wire_checked(
        &server,
        &mut comparison,
        request("POST", "/memories/recall", recall_args("fixture-tag")),
        "062",
    );
    wire_rpc(&server, &mut comparison, "initialize", json!({}), "007");
    wire_rpc(
        &server,
        &mut comparison,
        "tools/call",
        json!({"name":"recall","arguments":recall_args("fixture-tag")}),
        "036",
    );
    server.stop_and_join().unwrap();
    let mut comparison = Replay::default();
    retain(
        &mut comparison,
        "fixture-routes-pending",
        "invented pending route body",
        false,
        "010",
    );
    comparison.advance(TraceStep::ParentPendingChildError {
        operation_id: "fixture-routes-pending".into(),
        child_id: "fixture-routes-pending-child".into(),
    });
    let mut server = support::upstream::Server::start(comparison.clone());
    wire_checked(
        &server,
        &mut comparison,
        request(
            "GET",
            "/operations/fixture-routes-pending?include_payload=false",
            Value::Null,
        ),
        "434",
    );
    server.stop_and_join().unwrap();
    complete(&mut comparison, "fixture-routes-pending");
    let mut server = support::upstream::Server::start(comparison.clone());
    wire_checked(
        &server,
        &mut comparison,
        request(
            "GET",
            "/operations/fixture-routes-pending-child?include_payload=true",
            Value::Null,
        ),
        "116",
    );
    server.stop_and_join().unwrap();
}

#[test]
fn recall_rejects_unmeasured_scope() {
    let mut r = Replay::default();
    let q = request(
        "POST",
        "/memories",
        json!({"operation_id":"fixture-scope-operation","async":true,"items":[
        {"document_id":"fixture-scope-good","content":"invented requested source","tags":["subject:task:fixture-good"]},
        {"document_id":"fixture-scope-foreign","content":"invented foreign source","tags":["subject:task:fixture-foreign"]}]}),
    );
    checked(&mut r, q, "010");
    r.advance(TraceStep::Retained {
        operation_id: "fixture-scope-operation".into(),
        child_id: "fixture-scope-child".into(),
        facts: vec![
            Fact {
                id: "fixture-scope-fact".into(),
                document_id: "fixture-scope-good".into(),
                text: "invented good fact".into(),
            },
            Fact {
                id: "fixture-foreign-fact".into(),
                document_id: "fixture-scope-foreign".into(),
                text: "invented foreign fact".into(),
            },
        ],
    });
    r.advance(TraceStep::Recall {
        query: "invented scoped query".into(),
        tags: vec!["subject:task:fixture-good".into()],
    });
    let args = json!({"query":"invented scoped query","tags":["subject:task:fixture-good"],"tags_match":"all_strict"});
    let good = checked(
        &mut r,
        request("POST", "/memories/recall", args.clone()),
        "062",
    );
    assert_eq!(good.body["results"].as_array().unwrap().len(), 1);
    assert_eq!(good.body["results"][0]["document_id"], "fixture-scope-good");
    // The same reader oracle rejects omitted/wrong selectors; no final-reader stub.
    for key in ["tags", "tags_match", "query"] {
        let mut bad = args.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(r.apply(request("POST", "/memories/recall", bad)).is_err());
    }
    let mut wrong = args.clone();
    wrong["tags"] = json!(["subject:task:fixture-foreign"]);
    assert!(matches!(
        r.apply(request("POST", "/memories/recall", wrong)),
        Err(ReplayStop::NotRecorded(_))
    ));
    let mut missing = args.clone();
    missing.as_object_mut().unwrap().remove("tags");
    assert!(
        r.apply(request("POST", "/memories/recall", missing))
            .is_err()
    );
    let mut wrong_query = args.clone();
    wrong_query["query"] = json!("unmeasured invented query");
    assert!(
        r.apply(request("POST", "/memories/recall", wrong_query))
            .is_err()
    );
    let mut bank = request("POST", "/memories/recall", args.clone());
    bank.path = bank.path.replace("fixture-bank", "unmeasured-bank");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| r.apply(bank)))
        .expect("bank mismatch is typed");
    assert!(matches!(result, Err(ReplayStop::NotRecorded(_))));
    rpc(&mut r, "initialize", json!({}), "007");
    let reply = rpc(
        &mut r,
        "tools/call",
        json!({"name":"recall","arguments":args}),
        "036",
    );
    assert!(
        !reply
            .body
            .as_str()
            .unwrap()
            .contains("invented foreign fact")
    );
}
#[test]
fn replay_gap_is_not_a_publisher_failure() {
    let mut server = support::upstream::Server::start(Replay::default());
    let client_failed = server
        .request(request("GET", "/not-recorded", Value::Null))
        .is_err();
    let joined = server.stop_and_join();
    assert_eq!(
        server.replay_stop(),
        Some(ReplayStop::NotRecorded("wire_route"))
    );
    assert!(client_failed);
    let error = joined.expect_err("replay gap must survive joining");
    assert!(matches!(
        error.get_ref().and_then(|e| e.downcast_ref::<ReplayStop>()),
        Some(ReplayStop::NotRecorded("wire_route"))
    ));
}

#[test]
fn proxy_preserves_replay_gap_before_transport_error() {
    use std::net::TcpStream;
    use support::proxy::{FaultProxy, Frame, Step};
    for gap in [false, true] {
        let server = support::upstream::Server::start(Replay::default());
        let steps = if gap {
            vec![]
        } else {
            vec![Step::RefuseBeforeForward]
        };
        let mut proxy = FaultProxy::start(server.into_owned_backend(), steps);
        let mut stream = TcpStream::connect(proxy.address()).unwrap();
        Frame {
            head: format!(
                "GET {ROOT}/not-recorded HTTP/1.1\r\nAuthorization: Bearer fixture-test-token"
            ),
            body: vec![],
        }
        .write(&mut stream)
        .unwrap();
        let publisher_failed = Frame::read(&mut stream).is_err();
        let joined = proxy.stop_and_join();
        assert!(publisher_failed);
        let case_passed = publisher_failed && joined.is_ok();
        assert_eq!(
            case_passed, !gap,
            "the same oracle must reject a replay gap"
        );
        if gap {
            let error = joined.expect_err("gap must outrank EOF");
            assert!(matches!(
                error.get_ref().and_then(|e| e.downcast_ref::<ReplayStop>()),
                Some(ReplayStop::NotRecorded("wire_route"))
            ));
            let events = proxy.transcript();
            assert_eq!(events.len(), 1);
            assert_eq!(
                events[0].replay_stop,
                Some(ReplayStop::NotRecorded("wire_route"))
            );
            assert!(events[0].forwarded && !events[0].response_received);
        }
    }
}
