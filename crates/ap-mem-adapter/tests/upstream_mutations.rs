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
    let server = Server::start(f);
    assert!(server.address().ip().is_loopback());
    server
}
fn document(server: &Server) -> Value {
    server.mcp("get_document", json!({"test_document_id":"eng-a"}))
}
fn operation(server: &Server, id: &str) -> Value {
    let reply = server
        .request("GET", &format!("/__test__/operation/{id}"), json!({}))
        .unwrap();
    assert_eq!(reply.status, 200);
    reply.body
}

// A raw ordinary client, independent of Server::request's JSON-body framing.
fn bodyless(server: &Server, method: &str, path: &str) -> Value {
    let timeout = std::time::Duration::from_secs(1);
    let mut stream = std::net::TcpStream::connect_timeout(&server.address(), timeout).unwrap();
    stream.set_read_timeout(Some(timeout)).unwrap();
    stream.set_write_timeout(Some(timeout)).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    stream.take(131072).read_to_string(&mut raw).unwrap();
    let (headers, body) = raw.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("HTTP/1.1 200 "), "{headers}");
    serde_json::from_str(body).unwrap()
}

#[test]
fn running_async_order() {
    let server = server();
    let data = cases();
    // U3: explicitly select favourable deduplication and mismatch refusal.
    server.edit(|m| {
        m.retry.deduplicate = true;
        m.retry.reject_mismatch = true;
    });

    server.edit(|m| {
        m.fixture["facts"]
            .as_array_mut()
            .unwrap()
            .push(json!({"text":"UNRELATED_FACT", "document_ids":["eng-b"]}))
    });
    server.edit(|m| m.next_fault = Some(MutationFault::BeforeAccept));
    assert_eq!(
        server
            .request("POST", "/memories", data["retain"].clone())
            .unwrap()
            .status,
        503
    );
    assert!(server.edit(|m| m.operations.is_empty()));
    assert_eq!(document(&server)["body"], data["expected_old"]);
    server.edit(|m| m.next_fault = Some(MutationFault::LostAck));
    assert!(
        server
            .request("POST", "/memories", data["retain"].clone())
            .is_err()
    );
    assert_eq!(operation(&server, "operation-a")["test_state"], "running");
    assert_eq!(document(&server)["body"], data["expected_old"]);
    assert_eq!(
        server
            .request("POST", "/memories", data["retain"].clone())
            .unwrap()
            .status,
        202
    );
    assert_eq!(server.edit(|m| m.operations.len()), 1);
    let mut conflicting = data["retain"].clone();
    conflicting["document"]["body"] = json!("CONFLICT");
    assert_eq!(
        server
            .request("POST", "/memories", conflicting)
            .unwrap()
            .status,
        409
    );
    assert_eq!(
        server
            .request("POST", "/memories", data["newer"].clone())
            .unwrap()
            .status,
        202
    );
    // Completion is explicitly controlled; submitting or polling never completes it.
    server.edit(|m| m.finish("operation-b", true));
    assert_eq!(document(&server)["body"], data["expected_newer"]);
    assert_eq!(operation(&server, "operation-a")["test_state"], "running");
    server.edit(|m| m.finish("operation-a", true));
    assert_eq!(document(&server)["body"], data["expected_new"]);
    let facts = server
        .request("GET", "/__test__/facts", json!({}))
        .unwrap()
        .body;
    assert_eq!(
        facts,
        json!([{"text":"UNRELATED_FACT","document_ids":["eng-b"]},{"text":data["expected_new"],"document_ids":["eng-a"]}])
    );
    assert_eq!(operation(&server, "operation-a")["test_state"], "succeeded");
    let mut failed = data["retain"].clone();
    failed["test_operation_id"] = json!("operation-failed");
    server.request("POST", "/memories", failed).unwrap();
    server.edit(|m| m.finish("operation-failed", false));
    assert_eq!(
        operation(&server, "operation-failed")["test_state"],
        "failed"
    );
    assert_eq!(document(&server)["body"], data["expected_new"]);
    let hostile = self::server();
    hostile.edit(|m| {
        assert!(!m.retry.deduplicate);
        assert!(!m.retry.reject_mismatch);
    });
    hostile
        .request("POST", "/memories", data["retain"].clone())
        .unwrap();
    hostile
        .request("POST", "/memories", data["retain"].clone())
        .unwrap();
    assert_eq!(hostile.edit(|m| m.operations.len()), 2); // U3: resend duplicates.
    let mut changed = data["retain"].clone();
    changed["document"]["body"] = json!("CHANGED_BYTES_ACCEPTED");
    assert_eq!(
        hostile
            .request("POST", "/memories", changed.clone())
            .unwrap()
            .status,
        202
    );
    assert_eq!(hostile.edit(|m| m.operations.len()), 3);
    assert_eq!(operation(&hostile, "operation-a#3")["request"], changed);
    hostile.edit(|m| m.retry.deduplicate = true);
    assert_eq!(
        hostile
            .request("POST", "/memories", changed.clone())
            .unwrap()
            .status,
        202
    );
    assert_eq!(operation(&hostile, "operation-a")["request"], changed);
}

#[test]
fn config_false_required() {
    let server = server();
    let data = cases();
    assert_eq!(bodyless(&server, "GET", "/config"), data["config"]);
    let cases = data["config_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        server.edit(|m| m.fixture["config"] = case["value"].clone());
        let reply = server.request("GET", "/config", json!({})).unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, case["value"]);
        let both_boolean_false = reply.body["enable_observations"] == json!(false)
            && reply.body["enable_auto_consolidation"] == json!(false);
        assert_eq!(
            json!(both_boolean_false),
            case["allowed"],
            "{}",
            case["name"]
        );
    }
    assert!(server.edit(|m| m.requests.iter().all(|r| r.method == "GET")));
    // The upstream must not mask a missing P19 publisher-side preflight.
    // Even with unsafe config it accepts a retain: future caller tests must
    // assert no POST was sent, rather than relying on an upstream refusal.
    assert_eq!(
        server
            .request("POST", "/memories", data["retain"].clone())
            .unwrap()
            .status,
        202
    );
    assert_eq!(operation(&server, "operation-a")["test_state"], "running");
}

#[test]
fn delete_source_residue() {
    let server = server();
    // U5: explicitly select synchronous deletion and absent-id success.
    server.edit(|m| {
        m.retry.delete_immediate = true;
        m.retry.absent_delete_ok = true;
    });

    let recall = server.mcp("recall", json!({}));
    let search = server.mcp("search_knowledge_base", json!({}));
    let page = server.mcp("get_knowledge_page", json!({}));
    let facts = server
        .request("GET", "/__test__/facts", json!({}))
        .unwrap()
        .body;
    server.edit(|m| m.next_fault = Some(MutationFault::BeforeAccept));
    assert_eq!(
        server
            .request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        503
    );
    assert!(!document(&server).is_null());
    server.edit(|m| m.next_fault = Some(MutationFault::LostAck));
    assert!(
        server
            .request("DELETE", "/__test__/document/eng-a", json!({}))
            .is_err()
    );
    assert!(document(&server).is_null());
    assert_eq!(
        server
            .request("GET", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        404
    );
    assert_eq!(
        server
            .request("GET", "/__test__/facts", json!({}))
            .unwrap()
            .body,
        facts
    );
    assert_eq!(server.mcp("recall", json!({})), recall);
    assert_eq!(server.mcp("search_knowledge_base", json!({})), search);
    assert_eq!(server.mcp("get_knowledge_page", json!({})), page);
    assert!(
        page["document_ids"]
            .as_array()
            .unwrap()
            .contains(&json!("eng-a"))
    );
    assert_eq!(
        bodyless(&server, "DELETE", "/__test__/document/eng-a"),
        json!({"test_deleted":"eng-a"})
    );
    let hostile = self::server();
    hostile.edit(|m| {
        assert!(!m.retry.delete_immediate);
        assert!(!m.retry.absent_delete_ok);
    });
    assert_eq!(
        hostile
            .request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        200
    );
    assert!(!document(&hostile).is_null()); // U5: reply is not completion.
    hostile.edit(|m| m.finish_delete("eng-a"));
    assert!(document(&hostile).is_null());
    assert_eq!(
        hostile
            .request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        404
    );
}
