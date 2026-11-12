mod support;
use serde_json::{Value, json};
use support::upstream::{MutationFault, Server};

fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/page-cases.json")).unwrap()
}
fn server() -> Server {
    let mut fixture: Value =
        serde_json::from_str(include_str!("fixtures/read-cases.json")).unwrap();
    fixture["page_refresh_result"] = cases()["target"].clone();
    let server = Server::start(fixture);
    assert!(server.address().ip().is_loopback());
    server
}
fn create(server: &Server) {
    assert_eq!(
        server
            .request("POST", "/knowledge-base/folders", cases()["folder"].clone())
            .unwrap()
            .status,
        201
    );
    assert_eq!(
        server
            .request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .unwrap()
            .status,
        201
    );
}
fn read_page(server: &Server) -> Value {
    server.mcp(
        "get_knowledge_page",
        json!({"test_page_id":"page-engineering"}),
    )
}
fn refresh(server: &Server) -> String {
    let reply = server
        .request(
            "POST",
            "/mental-models/page-engineering/refresh",
            json!({"mode":"full"}),
        )
        .unwrap();
    assert_eq!(reply.status, 202);
    reply.body["test_job_id"].as_str().unwrap().to_owned()
}

#[test]
fn page_policy() {
    let server = server();
    create(&server);
    let page = read_page(&server);
    assert_eq!(page["test_owner"], "manual-api");
    assert_eq!(
        page["test_subjects"],
        json!([{"type":"platform","id":"fixture"}])
    );
    assert_eq!(page["tags"], json!(["subject:platform:fixture"]));
    assert_eq!(page["test_tag_match"], "strict");
    assert_eq!(page["fact_types"], json!(["world", "experience"]));
    assert_eq!(page["exclude_mental_models"], true);
    assert_eq!(page["refresh_after_consolidation"], false);
    assert_eq!(
        page["test_cron_fields"],
        json!({"synthetic-cron-a":null,"synthetic-cron-b":null})
    );
    assert!(
        page["test_cron_fields"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_null)
    );
    let mut bad_parent = cases()["page"].clone();
    bad_parent["test_parent_id"] = json!("page-engineering");
    assert_eq!(
        server
            .request("POST", "/knowledge-base/pages", bad_parent)
            .unwrap()
            .status,
        409
    );
    let tree = server.mcp("get_knowledge_base_tree", json!({"test_live_tree":true}));
    assert!(tree["folders"]["folder-platform"].is_object());
    assert_eq!(tree["pages"]["page-engineering"], page);
    // U6: exact wire cron names are unknown; test_cron_fields are explicit aliases.
    for (n, fault) in cases()["faults"].as_array().unwrap().iter().enumerate() {
        let mut wrong = cases()["page"].clone();
        for (key, value) in fault["patch"].as_object().unwrap() {
            wrong[key] = value.clone();
        }
        server.edit(|m| m.page_reply_override = Some(wrong.clone()));
        let reply = server
            .request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .unwrap();
        assert_eq!(reply.status, 201);
        let assigned = format!("page-engineering#{}", n + 2);
        assert_eq!(reply.body["test_id"], assigned);
        wrong["test_id"] = json!(assigned);
        assert_eq!(reply.body["test_value"], wrong, "{}", fault["label"]);
        assert_ne!(reply.body["test_value"], page);
    }
}

#[test]
fn refresh_old_result() {
    let server = server();
    create(&server);
    assert!(server.edit(|m| m.refresh_keeps_old)); // U7 hostile default.
    let first = refresh(&server);
    assert_eq!(read_page(&server)["body"], "PAGE_OLD_SYNTHETIC");
    server.edit(|m| m.finish_refresh(&first, true));
    let job = server
        .request("GET", &format!("/__test__/page-job/{first}"), json!({}))
        .unwrap();
    assert_eq!(job.body["test_state"], "succeeded");
    assert_eq!(read_page(&server)["body"], "PAGE_OLD_SYNTHETIC");
    server.edit(|m| m.refresh_keeps_old = false);
    let failed = refresh(&server);
    server.edit(|m| m.finish_refresh(&failed, false));
    assert_eq!(read_page(&server)["body"], "PAGE_OLD_SYNTHETIC");
    let fresh = refresh(&server);
    server.edit(|m| m.finish_refresh(&fresh, true));
    assert_eq!(read_page(&server)["body"], "PAGE_NEW_SYNTHETIC");
    assert_eq!(read_page(&server)["document_ids"], json!(["eng-new"]));
    // U7/U14: an older in-flight job can overwrite a newer completed refresh.
    server.edit(|m| {
        m.fixture["page_refresh_result"] = json!({"body":"OLDER_JOB","document_ids":["old"]})
    });
    let old = refresh(&server);
    server.edit(|m| m.fixture["page_refresh_result"] = cases()["target"].clone());
    let newer = refresh(&server);
    server.edit(|m| m.finish_refresh(&newer, true));
    assert_eq!(read_page(&server)["body"], "PAGE_NEW_SYNTHETIC");
    server.edit(|m| m.finish_refresh(&old, true));
    assert_eq!(read_page(&server)["body"], "OLDER_JOB");
}

#[test]
fn create_lost_ack() {
    let server = server();
    server
        .request("POST", "/knowledge-base/folders", cases()["folder"].clone())
        .unwrap();
    server.edit(|m| m.next_fault = Some(MutationFault::BeforeAccept));
    assert_eq!(
        server
            .request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .unwrap()
            .status,
        503
    );
    assert!(server.edit(|m| m.pages.is_empty()));
    assert!(!server.edit(|m| m.page_deduplicate)); // U6 hostile default.
    server.edit(|m| m.next_fault = Some(MutationFault::LostAck));
    assert!(
        server
            .request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .is_err()
    );
    let discovered = server.mcp("get_knowledge_base_tree", json!({"test_live_tree":true}));
    assert_eq!(discovered["pages"].as_object().unwrap().len(), 1);
    let retry = server
        .request("POST", "/knowledge-base/pages", cases()["page"].clone())
        .unwrap();
    assert_eq!(retry.body["test_id"], "page-engineering#2");
    assert_eq!(server.edit(|m| m.pages.len()), 2);
    let tree = server.mcp("get_knowledge_base_tree", json!({"test_live_tree":true}));
    for id in ["page-engineering", "page-engineering#2"] {
        assert_eq!(tree["pages"][id]["test_id"], id);
        let page = server.mcp("get_knowledge_page", json!({"test_page_id":id}));
        assert_eq!(page["test_id"], id);
    }
    let folder = server
        .request("POST", "/knowledge-base/folders", cases()["folder"].clone())
        .unwrap();
    assert_eq!(folder.body["test_id"], "folder-platform#2");
    assert_eq!(folder.body["test_value"]["test_id"], "folder-platform#2");
    let tree = server.mcp("get_knowledge_base_tree", json!({"test_live_tree":true}));
    assert_eq!(
        tree["folders"]["folder-platform#2"]["test_id"],
        "folder-platform#2"
    );
    server.edit(|m| m.page_deduplicate = true);
    let retry = server
        .request("POST", "/knowledge-base/pages", cases()["page"].clone())
        .unwrap();
    assert_eq!(retry.body["test_id"], "page-engineering");
    assert_eq!(server.edit(|m| m.pages.len()), 2);
    server.edit(|m| m.next_fault = Some(MutationFault::LostAck));
    assert!(
        server
            .request(
                "POST",
                "/mental-models/page-engineering/refresh",
                json!({"mode":"full"})
            )
            .is_err()
    );
    assert_eq!(server.edit(|m| m.page_jobs.len()), 1);
    assert_eq!(read_page(&server)["body"], "PAGE_OLD_SYNTHETIC");
}
