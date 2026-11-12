mod support;
use serde_json::{Value, json};
use std::io::{Read, Write};
use support::upstream::{MutationFault, Server};

fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/mutation-cases.json")).unwrap()
}
fn server() -> Server {
    let mut f: Value = serde_json::from_str(include_str!("fixtures/read-cases.json")).unwrap();
    f["config"] = cases()["config"].clone();
    support::fixture_server(f)
}
fn document(s: &Server) -> Value {
    s.request("GET", "/__test__/document/eng-a", json!({}))
        .unwrap()
        .body
}
fn operation(s: &Server, id: &str) -> Value {
    let reply = s
        .request("GET", &format!("/__test__/operation/{id}"), json!({}))
        .unwrap();
    assert_eq!(reply.status, 200);
    reply.body
}
fn retain(s: &Server, body: Value) -> String {
    let reply = s.request("POST", "/memories", body).unwrap();
    assert_eq!(reply.status, 202);
    reply.body["test_operation_id"].as_str().unwrap().to_owned()
}

fn bodyless(s: &Server, method: &str, path: &str) -> Value {
    let timeout = std::time::Duration::from_secs(1);
    let mut stream = std::net::TcpStream::connect_timeout(&s.address(), timeout).unwrap();
    stream.set_read_timeout(Some(timeout)).unwrap();
    stream.set_write_timeout(Some(timeout)).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.take(131072).read_to_string(&mut response).unwrap();
    let (header, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(header.starts_with("HTTP/1.1 200 "));
    serde_json::from_str(body).unwrap()
}

#[test]
fn running_async_order() {
    let s = server();
    let data = cases();
    s.edit(|m| {
        m.behaviour.deduplicate = true;
        m.behaviour.reject_mismatch = true;
    });
    s.edit(|m| m.behaviour.fault = Some(MutationFault::BeforeAccept));
    assert_eq!(
        s.request("POST", "/memories", data["retain"].clone())
            .unwrap()
            .status,
        503
    );
    assert!(s.edit(|m| m.operations.is_empty()));
    s.edit(|m| m.behaviour.fault = Some(MutationFault::LostAck));
    assert!(
        s.request("POST", "/memories", data["retain"].clone())
            .is_err()
    );
    let first = s.edit(|m| m.operations.keys().next().unwrap().clone()); // lost ack: inspect model, never guess.
    assert_eq!(operation(&s, &first)["test_state"], "running");
    assert_eq!(document(&s)["body"], data["expected_old"]);
    assert_eq!(retain(&s, data["retain"].clone()), first);
    assert_eq!(s.edit(|m| m.operations.len()), 1);
    let mut changed = data["retain"].clone();
    changed["document"]["body"] = json!("CHANGED");
    assert_eq!(
        s.request("POST", "/memories", changed.clone())
            .unwrap()
            .status,
        409
    );
    let newer = retain(&s, data["newer"].clone());
    assert_ne!(first, newer);
    s.edit(|m| m.finish(&newer, true));
    assert_eq!(document(&s)["body"], data["expected_newer"]);
    s.edit(|m| m.finish(&first, true));
    assert_eq!(document(&s)["body"], data["expected_new"]);
    let mut failed = data["retain"].clone();
    failed["test_operation_id"] = json!("failed-intent");
    let failed = retain(&s, failed);
    s.edit(|m| m.finish(&failed, false));
    assert_eq!(operation(&s, &failed)["test_state"], "failed");
    let hostile = server();
    let a = retain(&hostile, data["retain"].clone());
    let b = retain(&hostile, data["retain"].clone());
    assert_ne!(a, b);
    let c = retain(&hostile, changed.clone());
    assert_ne!(a, c);
    assert_eq!(operation(&hostile, &c)["request"], changed);
    hostile.edit(|m| {
        m.behaviour.deduplicate = true;
        m.finish(&a, true);
    });
    assert_eq!(retain(&hostile, changed), a);
    assert_eq!(operation(&hostile, &a)["test_state"], "succeeded");
    assert_eq!(operation(&hostile, &a)["request"], data["retain"]); // silent mismatch keeps stale bytes.
}

#[test]
fn config_false_required() {
    let s = server();
    let data = cases();
    assert_eq!(bodyless(&s, "GET", "/config"), data["config"]);
    for case in data["config_cases"].as_array().unwrap() {
        s.edit(|m| m.fixture["config"] = case["value"].clone());
        let reply = s.request("GET", "/config", json!({})).unwrap();
        assert_eq!(reply.body, case["value"]);
        assert_eq!(
            json!(
                reply.body["enable_observations"] == false
                    && reply.body["enable_auto_consolidation"] == false
            ),
            case["allowed"]
        );
    }
    // The fake never hides a missing publisher-side preflight by enforcing it.
    let id = retain(&s, data["retain"].clone());
    assert_eq!(operation(&s, &id)["test_state"], "running");
}

#[test]
fn delete_source_residue() {
    let s = server();
    s.edit(|m| {
        m.behaviour.delete_immediate = true;
        m.behaviour.absent_delete_ok = true;
    });
    let recall = s.mcp("recall", json!({}));
    let search = s.mcp("search_knowledge_base", json!({}));
    let page = s.mcp("get_knowledge_page", json!({}));
    s.edit(|m| m.behaviour.fault = Some(MutationFault::BeforeAccept));
    assert_eq!(
        s.request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        503
    );
    assert!(!document(&s).is_null());
    s.edit(|m| m.behaviour.fault = Some(MutationFault::LostAck));
    assert!(
        s.request("DELETE", "/__test__/document/eng-a", json!({}))
            .is_err()
    );
    assert!(document(&s).is_null());
    assert_eq!(
        s.request("GET", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        404
    );
    assert_eq!(s.mcp("recall", json!({})), recall);
    assert_eq!(s.mcp("search_knowledge_base", json!({})), search);
    assert_eq!(s.mcp("get_knowledge_page", json!({})), page);
    assert_eq!(
        s.request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        200
    );
    assert_eq!(
        bodyless(&s, "DELETE", "/__test__/document/eng-a"),
        json!({"test_accepted":true})
    );
    let delayed = server();
    delayed
        .request("DELETE", "/__test__/document/eng-a", json!({}))
        .unwrap();
    assert!(!document(&delayed).is_null());
    let delete = delayed.edit(|m| m.pending_deletes.keys().next().unwrap().clone());
    delayed.edit(|m| m.finish_delete(&delete));
    assert!(document(&delayed).is_null());
    assert_eq!(
        delayed
            .request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        404
    );
}
