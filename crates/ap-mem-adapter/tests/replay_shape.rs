#[path = "support/replay.rs"]
mod replay;
use replay::{
    Fact, MentalModel, RecordedReply, Replay, Shape, TraceStep, WireRequest, assert_recorded_keys,
    project,
};
use serde_json::{Value, json};

const ROOT: &str = "/v1/default/banks/fixture-bank";
fn request(method: &str, route: &str, body: Value) -> WireRequest {
    WireRequest::new(method, &format!("{ROOT}{route}"), body)
}
fn checked(r: &mut Replay, q: WireRequest, recording: &'static str) -> RecordedReply {
    let reply = r.apply(q);
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
fn complete(r: &mut Replay, op: &str) {
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
        request("POST", "/memories/recall", json!({})),
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
        json!({"name":"recall","arguments":{"query":"invented"}}),
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
    assert_recorded_keys(&project(&r, &q), &Shape { recording: "016" }).unwrap();
}
