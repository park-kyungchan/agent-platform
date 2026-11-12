mod support;
use serde_json::{Value, json};
use support::upstream::{MutationFault, Server};
fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/page-cases.json")).unwrap()
}
fn server() -> Server {
    let mut f: Value = serde_json::from_str(include_str!("fixtures/read-cases.json")).unwrap();
    f["page_refresh_result"] = cases()["target"].clone();
    support::fixture_server(f)
}
fn create(s: &Server) -> (String, String) {
    let folder = s
        .request("POST", "/knowledge-base/folders", cases()["folder"].clone())
        .unwrap()
        .body["test_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut body = cases()["page"].clone();
    body["test_parent_id"] = json!(folder);
    let page = s
        .request("POST", "/knowledge-base/pages", body)
        .unwrap()
        .body["test_id"]
        .as_str()
        .unwrap()
        .to_owned();
    (folder, page)
}
fn read(s: &Server, id: &str) -> Value {
    s.mcp("get_knowledge_page", json!({"test_page_id":id}))
}
fn refresh(s: &Server, id: &str) -> String {
    let reply = s
        .request(
            "POST",
            &format!("/mental-models/{id}/refresh"),
            json!({"mode":"full"}),
        )
        .unwrap();
    assert_eq!(reply.status, 202);
    reply.body["test_job_id"].as_str().unwrap().to_owned()
}
#[test]
fn page_policy() {
    let s = server();
    let (folder, page) = create(&s);
    let value = read(&s, &page);
    assert_eq!(value["test_owner"], "manual-api");
    assert_eq!(
        value["test_subjects"],
        json!([{"type":"platform","id":"fixture"}])
    );
    assert_eq!(value["tags"], json!(["subject:platform:fixture"]));
    assert_eq!(value["test_tag_match"], "strict");
    assert_eq!(value["fact_types"], json!(["world", "experience"]));
    assert_eq!(value["exclude_mental_models"], true);
    assert_eq!(value["refresh_after_consolidation"], false);
    assert_eq!(
        value["test_cron_fields"],
        json!({"synthetic-cron-a":null,"synthetic-cron-b":null})
    );
    assert_eq!(value["test_parent_id"], folder);
    s.edit(|m| m.behaviour.reject_orphan = true);
    assert_eq!(
        s.request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .unwrap()
            .status,
        409
    );
    s.edit(|m| {
        m.behaviour.reject_orphan = false;
        m.behaviour.echo_created = false;
    });
    for fault in cases()["faults"].as_array().unwrap() {
        let mut wrong = cases()["page"].clone();
        for (key, value) in fault["patch"].as_object().unwrap() {
            wrong[key] = value.clone();
        }
        s.edit(|m| m.fixture["page_response"] = wrong.clone());
        let reply = s
            .request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .unwrap();
        let assigned = reply.body["test_id"].as_str().unwrap();
        wrong["test_id"] = json!(assigned);
        assert_eq!(reply.body["test_value"], wrong);
        assert_ne!(assigned, "page-engineering");
    }
}
#[test]
fn refresh_old_result() {
    let s = server();
    let (_, page) = create(&s);
    let first = refresh(&s, &page);
    s.edit(|m| m.finish_refresh(&first, true));
    assert_eq!(read(&s, &page)["body"], "PAGE_OLD_SYNTHETIC");
    s.edit(|m| m.behaviour.refresh_keeps_old = false);
    let failed = refresh(&s, &page);
    s.edit(|m| m.finish_refresh(&failed, false));
    assert_eq!(read(&s, &page)["body"], "PAGE_OLD_SYNTHETIC");
    let fresh = refresh(&s, &page);
    s.edit(|m| m.finish_refresh(&fresh, true));
    assert_eq!(read(&s, &page)["body"], "PAGE_NEW_SYNTHETIC");
    assert_eq!(read(&s, &page)["document_ids"], json!(["eng-new"]));
    s.edit(|m| {
        m.fixture["page_refresh_result"] = json!({"body":"OLDER_JOB","document_ids":["old"]})
    });
    let old = refresh(&s, &page);
    s.edit(|m| m.fixture["page_refresh_result"] = cases()["target"].clone());
    let newer = refresh(&s, &page);
    s.edit(|m| m.finish_refresh(&newer, true));
    s.edit(|m| m.finish_refresh(&old, true));
    assert_eq!(read(&s, &page)["body"], "OLDER_JOB");
}
#[test]
fn create_lost_ack() {
    let s = server();
    s.edit(|m| m.behaviour.fault = Some(MutationFault::BeforeAccept));
    assert_eq!(
        s.request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .unwrap()
            .status,
        503
    );
    s.edit(|m| m.behaviour.fault = Some(MutationFault::LostAck));
    assert!(
        s.request("POST", "/knowledge-base/pages", cases()["page"].clone())
            .is_err()
    );
    let first = s.edit(|m| m.pages.keys().next().unwrap().clone());
    let reply = s
        .request("POST", "/knowledge-base/pages", cases()["page"].clone())
        .unwrap();
    let second = reply.body["test_id"].as_str().unwrap();
    assert_ne!(first, second);
    let tree = s.mcp("get_knowledge_base_tree", json!({"test_live_tree":true}));
    for id in [first.as_str(), second] {
        assert_eq!(tree["pages"][id]["test_id"], id);
        assert_eq!(read(&s, id)["test_id"], id);
    }
    s.edit(|m| m.behaviour.page_deduplicate = true);
    let retry = s
        .request("POST", "/knowledge-base/pages", cases()["page"].clone())
        .unwrap();
    assert_eq!(retry.body["test_id"], first);
    s.edit(|m| m.behaviour.fault = Some(MutationFault::LostAck));
    assert!(
        s.request(
            "POST",
            &format!("/mental-models/{first}/refresh"),
            json!({"mode":"full"})
        )
        .is_err()
    );
    assert_eq!(s.edit(|m| m.page_jobs.len()), 1);
}
