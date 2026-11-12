mod support;
use serde_json::{Value, json};
use support::upstream::{MCP_PATH, MutationFault, Server};
fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/lag-cases.json")).unwrap()
}
fn server() -> (Server, String) {
    let data = cases();
    let mut f: Value = serde_json::from_str(include_str!("fixtures/read-cases.json")).unwrap();
    f["documents"]["eng-a"]["body"] = data["old"].clone();
    f["facts"] = data["facts"].clone();
    f["responses"]["recall"] = json!({"facts":data["facts"]});
    f["page_refresh_result"] = data["target"].clone();
    let s = support::fixture_server(f);
    let reply = s
        .request("POST", "/knowledge-base/pages", data["page"].clone())
        .unwrap();
    let page = reply.body["test_id"].as_str().unwrap().to_owned();
    s.edit(|m| {
        m.fixture["responses"]["search_knowledge_base"] =
            json!({"pages":[{"page_id":page,"snippet":data["old"],"document_ids":["eng-a"]}]})
    });
    (s, page)
}
fn document(s: &Server) -> Value {
    s.request("GET", "/__test__/document/eng-a", json!({}))
        .unwrap()
        .body
}
fn retain(s: &Server) {
    let reply = s
        .request("POST", "/memories", cases()["retain"].clone())
        .unwrap();
    assert_eq!(reply.status, 202);
    let id = reply.body["test_operation_id"].as_str().unwrap();
    s.edit(|m| m.finish(id, true));
}
fn refresh(s: &Server, page: &str) {
    let reply = s
        .request(
            "POST",
            &format!("/mental-models/{page}/refresh"),
            json!({"mode":"full"}),
        )
        .unwrap();
    assert_eq!(reply.status, 202);
    let id = reply.body["test_job_id"].as_str().unwrap();
    s.edit(|m| m.finish_refresh(id, true));
}
fn recalled(s: &Server) -> Value {
    s.mcp("recall", json!({}))
}
fn searched(s: &Server) -> Value {
    s.mcp("search_knowledge_base", json!({}))
}
fn text_for_a(value: &Value) -> &Value {
    &value["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["document_ids"] == json!(["eng-a"]))
        .unwrap()["text"]
}
#[test]
fn replace_lag() {
    let (s, page) = server();
    let data = cases();
    let old_recall = recalled(&s);
    let old_search = searched(&s);
    retain(&s);
    assert_eq!(document(&s)["body"], data["new"]);
    s.edit(|m| m.behaviour.refresh_keeps_old = false);
    refresh(&s, &page);
    assert_eq!(
        s.mcp("get_knowledge_page", json!({"test_page_id":page}))["body"],
        data["new"]
    );
    assert_eq!(recalled(&s), old_recall);
    assert_eq!(searched(&s), old_search);
    s.edit(|m| m.behaviour.recall_current = true);
    assert_eq!(text_for_a(&recalled(&s)), &data["new"]);
    assert_eq!(searched(&s), old_search);
    s.edit(|m| m.behaviour.search_current = true);
    assert_eq!(searched(&s)["pages"][0]["snippet"], data["new"]);
    // U8: a partial read is explicit and independently selected.
    s.edit(|m| m.behaviour.complete_reads = false);
    let partial = recalled(&s);
    assert_eq!(partial["test_partial"], true);
    assert_eq!(partial["facts"].as_array().unwrap().len(), 1);
    s.edit(|m| {
        m.behaviour.complete_reads = true;
        m.behaviour.foreign_provenance = true;
    });
    assert_eq!(
        recalled(&s)["facts"][0]["document_ids"],
        json!(["TEST_FOREIGN_SOURCE"])
    );
    assert_eq!(
        searched(&s)["pages"][0]["document_ids"],
        json!(["TEST_FOREIGN_SOURCE"])
    );
}
#[test]
fn delete_lag() {
    let (s, page) = server();
    let data = cases();
    let old = recalled(&s);
    let old_search = searched(&s);
    s.edit(|m| {
        m.behaviour.delete_immediate = true;
        m.behaviour.clean_deleted_facts = true;
        m.behaviour.clean_deleted_pages = true;
    });
    assert_eq!(
        s.request("DELETE", "/__test__/document/eng-a", json!({}))
            .unwrap()
            .status,
        200
    );
    assert!(document(&s).is_null());
    assert_eq!(recalled(&s), old);
    assert_eq!(searched(&s), old_search);
    s.edit(|m| {
        m.behaviour.recall_current = true;
        m.behaviour.refresh_keeps_old = false;
        m.fixture["page_refresh_result"] = data["withdrawn"].clone();
    });
    assert!(
        !recalled(&s)
            .to_string()
            .contains(data["old"].as_str().unwrap())
    );
    assert_eq!(searched(&s), old_search);
    refresh(&s, &page);
    s.edit(|m| m.behaviour.search_current = true);
    assert_eq!(searched(&s)["pages"][0]["snippet"], data["empty"]);
    assert!(
        !searched(&s)
            .to_string()
            .contains(data["old"].as_str().unwrap())
    );
}
#[test]
fn refresh_search_lag() {
    let (s, page) = server();
    let data = cases();
    let old_search = searched(&s);
    let old_recall = recalled(&s);
    s.edit(|m| m.behaviour.refresh_keeps_old = false);
    refresh(&s, &page);
    assert_eq!(
        s.mcp("get_knowledge_page", json!({"test_page_id":page}))["body"],
        data["new"]
    );
    assert_eq!(searched(&s), old_search);
    s.edit(|m| m.behaviour.search_current = true);
    assert_eq!(searched(&s)["pages"][0]["snippet"], data["new"]);
    assert_eq!(recalled(&s), old_recall);
    // U1/U2/U8/U9/U14: fault injection is selected solely through Behaviour.
    let reads = [
        ("GET", "/config", json!({})),
        ("GET", "/__test__/document/eng-a", json!({})),
        ("GET", "/__test__/facts", json!({})),
        ("POST", MCP_PATH, json!({"id":1,"method":"tools/list"})),
        (
            "POST",
            MCP_PATH,
            json!({"id":1,"method":"tools/call","params":{"name":"recall","arguments":{}}}),
        ),
        (
            "POST",
            MCP_PATH,
            json!({"id":1,"method":"tools/call","params":{"name":"search_knowledge_base","arguments":{}}}),
        ),
        (
            "POST",
            MCP_PATH,
            json!({"id":1,"method":"tools/call","params":{"name":"get_document","arguments":{"test_document_id":"eng-a"}}}),
        ),
        (
            "POST",
            MCP_PATH,
            json!({"id":1,"method":"tools/call","params":{"name":"get_knowledge_page","arguments":{"test_page_id":page}}}),
        ),
        (
            "POST",
            MCP_PATH,
            json!({"id":1,"method":"tools/call","params":{"name":"get_knowledge_base_tree","arguments":{"test_live_tree":true}}}),
        ),
    ];
    for (method, path, body) in reads {
        s.edit(|m| m.behaviour.malformed_reads = true);
        assert!(s.request(method, path, body.clone()).is_err());
        s.edit(|m| {
            m.behaviour.malformed_reads = false;
            m.behaviour.fault = Some(MutationFault::BeforeAccept);
        });
        assert_eq!(s.request(method, path, body.clone()).unwrap().status, 503);
        s.edit(|m| m.behaviour.fault = Some(MutationFault::LostAck));
        assert!(s.request(method, path, body.clone()).is_err());
        assert_eq!(s.request(method, path, body).unwrap().status, 200);
    }
}
