mod support;
use serde_json::{Value, json};
use support::upstream::{MCP_PATH, Server};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/read-cases.json")).unwrap()
}

#[test]
fn fixed_endpoint() {
    let mut server = Server::start(fixture());
    let address = server.address();
    assert!(address.ip().is_loopback());
    assert_ne!(address.port(), 0);
    let response = server
        .request(
            "POST",
            MCP_PATH,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        )
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body["result"]["tools"], fixture()["tool_names"]);
    for path in ["/mcp/coding-agent::lesson/", "/mcp/", "/config"] {
        assert_eq!(server.request("POST", path, json!({})).unwrap().status, 404);
    }
    server.edit(|m| {
        assert_eq!(m.requests[0].method, "POST");
        assert_eq!(m.requests[0].path, MCP_PATH);
        assert_eq!(m.requests[0].body["method"], "tools/list");
    });
    // TE is unsupported, including a header with no content length.
    use std::io::{Read, Write};
    let mut raw =
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(1)).unwrap();
    raw.set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    raw.write_all(b"POST /mcp/coding-agent::engineering/ HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
    let mut response = String::new();
    raw.take(131072).read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 400 "));
    assert!(server.stop_and_join()); // Joining proves the owned worker exited.
}

#[test]
fn lesson_canary() {
    let server = Server::start(fixture());
    for tool in [
        "recall",
        "search_knowledge_base",
        "get_knowledge_page",
        "get_knowledge_base_tree",
        "get_document",
    ] {
        let raw = server.mcp(tool, json!({"test_document_id":"lesson-canary"}));
        assert!(raw.to_string().contains("LESSON_CANARY_INVENTED"), "{tool}");
    }
    // Fixture server intentionally returns unsafe data: filtering belongs to P15/P16.
    let doc = server
        .request("GET", "/__test__/document/eng-a", json!({}))
        .unwrap();
    assert_eq!(doc.body, fixture()["documents"]["eng-a"]);
    let facts = server.request("GET", "/__test__/facts", json!({})).unwrap();
    assert_eq!(facts.body, fixture()["facts"]);
}

#[test]
fn provenance_cases() {
    let server = Server::start(fixture());
    let raw = server.mcp("recall", json!({"test_query":"fixture"}));
    assert_eq!(raw, fixture()["responses"]["recall"]);
    let labels: Vec<_> = raw["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["case"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "ready",
            "multiple-ready",
            "mixed",
            "pending",
            "final",
            "foreign",
            "empty",
            "missing"
        ]
    );
    assert_eq!(
        raw["facts"][2]["document_ids"],
        json!(["eng-a", "lesson-canary"])
    );
    // U4: explicit wrong-id and wrong-tag document fault cases.
    let wrong_id = server.mcp("get_document", json!({"test_document_id":"wrong-id"}));
    assert_eq!(wrong_id["document_id"], "different-id");
    assert_ne!(wrong_id["document_id"], "wrong-id");
    let wrong_tag = server.mcp("get_document", json!({"test_document_id":"wrong-tag"}));
    assert_eq!(wrong_tag["document_id"], "wrong-tag");
    assert_eq!(wrong_tag["tags"], json!(["subject:task:foreign"]));
    assert_ne!(wrong_tag["tags"], json!(["subject:task:fixture"]));
    let before_search = server.mcp("search_knowledge_base", json!({}));
    server.edit(|m| m.fixture["responses"]["recall"]["summary"] = json!("CHANGED_RECALL_ONLY"));
    assert_eq!(
        server.mcp("search_knowledge_base", json!({})),
        before_search
    );
    assert_eq!(
        server.mcp("recall", json!({}))["summary"],
        "CHANGED_RECALL_ONLY"
    );
}
