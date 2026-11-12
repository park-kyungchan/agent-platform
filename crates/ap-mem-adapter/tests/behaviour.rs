mod support;
use serde_json::{Value, json};
use support::upstream::{Behaviour, MCP_PATH, MutationFault, Server, opaque_id};

fn fixture() -> Value {
    let mut f: Value = serde_json::from_str(include_str!("fixtures/read-cases.json")).unwrap();
    f["config"] = json!({"enable_observations":false,"enable_auto_consolidation":false});
    f["page_refresh_result"] = json!({"body":"NEW_PAGE","document_ids":["eng-new"]});
    f
}
fn retain(s: &Server, body: &str) -> String {
    let reply = s.request("POST", "/memories", json!({"test_operation_id":"intent","document":{"document_id":"eng-a","body":body,"tags":["new-tag"]},"async":true,"update_mode":"replace"})).unwrap();
    assert_eq!(reply.status, 202);
    reply.body["test_operation_id"].as_str().unwrap().to_owned()
}
fn page(s: &Server) -> String {
    s.request("POST", "/knowledge-base/pages", json!({"test_id":"caller-page","test_owner":"manual-api","body":"OLD_PAGE","document_ids":["eng-a"]})).unwrap().body["test_id"].as_str().unwrap().to_owned()
}
fn refresh(s: &Server, page: &str) -> String {
    s.request(
        "POST",
        &format!("/mental-models/{page}/refresh"),
        json!({"mode":"full"}),
    )
    .unwrap()
    .body["test_job_id"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn doc(s: &Server) -> Value {
    s.request("GET", "/__test__/document/eng-a", json!({}))
        .unwrap()
        .body
}

#[test]
fn whole_hostile_default() {
    // No `..Default`: adding ANY field makes this test fail to compile until
    // its hostile value is asserted explicitly. The contrast tests below cover
    // handlers, and compare semantic outcomes, never differing random ids.
    assert_eq!(
        Behaviour::default(),
        Behaviour {
            recall_current: false,
            search_current: false,
            foreign_provenance: true,
            malformed_reads: true,
            require_initialize: true,
            root_routes: false,
            reject_unknown: true,
            validate_shape: false,
            provenance: false,
            complete_reads: false,
            honor_retrieval_args: false,
            config_resolved: false,
            observable_operations: false,
            absent_read_404: true,
            deduplicate: false,
            reject_mismatch: false,
            restart_mismatch: false,
            deduplicate_deletes: false,
            missing_refresh_succeeds: true,
            delete_immediate: false,
            absent_delete_ok: false,
            echo_delete: false,
            clean_deleted_facts: false,
            clean_deleted_pages: false,
            exact_retained_body: false,
            replace_tags: false,
            replace_facts: false,
            last_finish_wins: true,
            page_deduplicate: false,
            reject_orphan: false,
            echo_created: false,
            refresh_keeps_old: true,
            tree_bodies: false,
            fault: Some(MutationFault::LostAck),
        }
    );
}

macro_rules! contrasts {
    ($($field:ident, $off:tt, $on:tt, $probe:expr);+ $(;)?) => {
        const CONTRAST_FIELDS: &[&str] = &[$(stringify!($field)),+, "fault"];
        $(#[test]
        fn $field() {
            for (selected, expected) in [(false, json!($off)), (true, json!($on))] {
                let s = support::fixture_server(fixture());
                s.edit(|m| m.behaviour.$field = selected);
                let actual: Value = ($probe)(&s);
                assert_eq!(actual, expected, "{}={selected}", stringify!($field));
            }
        })+
    }
}
contrasts! {
    deduplicate_deletes, 2, 1, |s: &Server| {s.request("DELETE","/__test__/document/eng-a",json!({})).unwrap();s.request("DELETE","/__test__/document/eng-a",json!({})).unwrap();json!(s.edit(|m|m.pending_deletes.len()))};
    missing_refresh_succeeds, ["failed",true], ["succeeded",true], |s: &Server| {
        s.edit(|m|{m.behaviour.absent_read_404=false;m.behaviour.refresh_keeps_old=false;});
        let job=refresh(s,"missing-target");s.edit(|m|m.finish_refresh(&job,true));
        json!([s.request("GET",&format!("/__test__/page-job/{job}"),json!({})).unwrap().body["test_state"],s.edit(|m|m.pages.is_empty())])
    };

    recall_current, "READY_ONLY", "NEW", |s: &Server| {let id=retain(s,"NEW");s.edit(|m|m.finish(&id,true));s.mcp("recall",json!({}))["facts"][0]["text"].clone()};
    search_current, "ENGINEERING_OLD_SYNTHETIC", "NEW_PAGE", |s: &Server| {let p=page(s);s.edit(|m|m.behaviour.refresh_keeps_old=false);let job=refresh(s,&p);s.edit(|m|m.finish_refresh(&job,true));s.mcp("search_knowledge_base",json!({}))["pages"][0]["snippet"].clone()};
    foreign_provenance, "eng-a", "TEST_FOREIGN_SOURCE", |s: &Server| {s.request("GET","/__test__/facts",json!({})).unwrap().body[0]["document_ids"][0].clone()};
    malformed_reads, false, true, |s: &Server| {json!(s.request("GET","/__test__/facts",json!({})).is_err())};

require_initialize, 200, 409, |s: &Server| json!(
    s.request("POST", MCP_PATH, json!({"id":1,"method":"tools/list"}))
        .unwrap()
        .status
);
root_routes, 404, 200, |s: &Server| json!(
    s.request("GET", "/config", json!({})).unwrap().status
);
reject_unknown, [200, false], [404, true], |s: &Server| {
    let http = s.request("GET", "/unknown", json!({})).unwrap();
    let mcp = s
        .request(
            "POST",
            MCP_PATH,
            json!({"id":1,"method":"tools/call","params":{"name":"unknown"}}),
        )
        .unwrap();
    json!([http.status, mcp.body.get("error").is_some()])
};

    validate_shape,
    [202, 201, 202],
    [400, 400, 400],
    |s: &Server| {
        let p = page(s);
        let retain = s.request("POST", "/memories", json!({})).unwrap().status;
        let create = s
            .request("POST", "/knowledge-base/pages", json!({}))
            .unwrap()
            .status;
        let refresh = s
            .request(
                "POST",
                &format!("/mental-models/{p}/refresh"),
                json!({"mode":"invalid"}),
            )
            .unwrap()
            .status;
        json!([retain, create, refresh])
    }
;
provenance, false, true, |s: &Server| json!(
    s.request("GET", "/__test__/facts", json!({})).unwrap().body[0]
        .get("document_ids")
        .is_some()
);
complete_reads, false, true, |s: &Server| json!(
    doc(s).get("body").is_some()
);
honor_retrieval_args, 8, 0, |s: &Server| json!(
    s.mcp("recall", json!({"test_document_id":"missing"}))["facts"]
        .as_array()
        .unwrap()
        .len()
);
config_resolved, 0, 2, |s: &Server| json!(
    s.request("GET", "/config", json!({}))
        .unwrap()
        .body
        .as_object()
        .unwrap()
        .len()
);
observable_operations, 404, 200, |s: &Server| {
    let id = retain(s, "NEW");
    json!(
        s.request("GET", &format!("/__test__/operation/{id}"), json!({}))
            .unwrap()
            .status
    )
};
absent_read_404, 200, 404, |s: &Server| json!(
    s.request("GET", "/__test__/document/missing", json!({}))
        .unwrap()
        .status
);
deduplicate, 2, 1, |s: &Server| {
    retain(s, "NEW");
    retain(s, "NEW");
    json!(s.edit(|m| m.operations.len()))
};
reject_mismatch, 202, 409, |s: &Server| {
    s.edit(|m| m.behaviour.deduplicate = true);
    retain(s, "OLD");
    json!(s.request("POST", "/memories", json!({"test_operation_id":"intent","document":{"document_id":"eng-a","body":"CHANGED","tags":[]},"async":true,"update_mode":"replace"})).unwrap().status)
};

    restart_mismatch,
    ["succeeded", "OLD"],
    ["running", "CHANGED"],
    |s: &Server| {
        s.edit(|m| m.behaviour.deduplicate = true);
        let id = retain(s, "OLD");
        s.edit(|m| m.finish(&id, true));
        retain(s, "CHANGED");
        s.edit(|m| {
            json!([
                m.operations[&id]["test_state"],
                m.operations[&id]["request"]["document"]["body"]
            ])
        })
    }
;
delete_immediate, true, false, |s: &Server| {
    s.request("DELETE", "/__test__/document/eng-a", json!({}))
        .unwrap();
    json!(!doc(s).is_null())
};
absent_delete_ok, 404, 200, |s: &Server| json!(
    s.request("DELETE", "/__test__/document/missing", json!({}))
        .unwrap()
        .status
);
echo_delete, false, true, |s: &Server| json!(
    s.request("DELETE", "/__test__/document/eng-a", json!({}))
        .unwrap()
        .body
        .get("test_deleted")
        .is_some()
);
clean_deleted_facts, 1, 0, |s: &Server| {
    s.edit(|m| m.behaviour.delete_immediate = true);
    s.request("DELETE", "/__test__/document/eng-a", json!({}))
        .unwrap();
    json!(
        s.request("GET", "/__test__/facts", json!({}))
            .unwrap()
            .body
            .as_array()
            .unwrap()
            .len()
    )
};
clean_deleted_pages, 1, 0, |s: &Server| {
    let id = page(s);
    s.edit(|m| m.behaviour.delete_immediate = true);
    s.request("DELETE", "/__test__/document/eng-a", json!({}))
        .unwrap();
    json!(
        s.mcp("get_knowledge_page", json!({"test_page_id":id}))["document_ids"]
            .as_array()
            .unwrap()
            .len()
    )
};

    exact_retained_body,
    "TEST_NORMALIZED_BODY",
    "NEW",
    |s: &Server| {
        let id = retain(s, "NEW");
        s.edit(|m| m.finish(&id, true));
        doc(s)["body"].clone()
    }
;

    replace_tags,
    ["subject:task:fixture", "new-tag"],
    ["new-tag"],
    |s: &Server| {
        let id = retain(s, "NEW");
        s.edit(|m| m.finish(&id, true));
        doc(s)["tags"].clone()
    }
;

    replace_facts,
    "ENGINEERING_OLD_SYNTHETIC",
    "NEW",
    |s: &Server| {
        let id = retain(s, "NEW");
        s.edit(|m| m.finish(&id, true));
        s.request("GET", "/__test__/facts", json!({})).unwrap().body[0]["text"].clone()
    }
;

    last_finish_wins,
    ["NEWER", "NEW_PAGE", "AFTER_DELETE", true],
    ["OLDER", "OLDER_PAGE", null, false],
    |s: &Server| {
        let old = retain(s, "OLDER");
        let new = retain(s, "NEWER");
        s.edit(|m| {
            m.finish(&new, true);
            m.finish(&old, true);
            m.behaviour.refresh_keeps_old = false;
        });
        let document = doc(s)["body"].clone();
        let p = page(s);
        s.edit(|m| m.fixture["page_refresh_result"]["body"] = json!("OLDER_PAGE"));
        let old = refresh(s, &p);
        s.edit(|m| m.fixture["page_refresh_result"]["body"] = json!("NEW_PAGE"));
        let new = refresh(s, &p);
        s.edit(|m| {
            m.finish_refresh(&new, true);
            m.finish_refresh(&old, true);
        });
        let page_body=s.mcp("get_knowledge_page",json!({"test_page_id":p}))["body"].clone();
        s.request("DELETE","/__test__/document/eng-a",json!({})).unwrap();
        let delete=s.edit(|m|m.pending_deletes.keys().next().unwrap().clone());
        let newer=retain(s,"AFTER_DELETE");s.edit(|m|m.finish(&newer,true));
        s.edit(|m|m.finish_delete(&delete));let delayed=doc(s)["body"].clone();
        let seed=retain(s,"SEED");s.edit(|m|m.finish(&seed,true));
        let older=retain(s,"RESURRECTED");s.edit(|m|m.behaviour.delete_immediate=true);
        s.request("DELETE","/__test__/document/eng-a",json!({})).unwrap();
        s.edit(|m|m.finish(&older,true));
        json!([document,page_body,delayed,doc(s).is_null()])
    }
;
page_deduplicate, 2, 1, |s: &Server| {
    page(s);
    page(s);
    json!(s.edit(|m| m.pages.len()))
};
reject_orphan, 201, 409, |s: &Server| json!(
    s.request(
        "POST",
        "/knowledge-base/pages",
        json!({"test_id":"intent","test_parent_id":"missing"})
    )
    .unwrap()
    .status
);
echo_created, false, true, |s: &Server| {
    let id = page(s);
    json!(
        s.mcp("get_knowledge_page", json!({"test_page_id":id}))
            .get("test_owner")
            .is_some()
    )
};
refresh_keeps_old, "NEW_PAGE", "OLD_PAGE", |s: &Server| {
    let p = page(s);
    let id = refresh(s, &p);
    s.edit(|m| m.finish_refresh(&id, true));
    s.mcp("get_knowledge_page", json!({"test_page_id":p}))["body"].clone()
};
tree_bodies, false, true, |s: &Server| {
    let id = page(s);
    json!(
        s.mcp("get_knowledge_base_tree", json!({"test_live_tree":true}))["pages"][&id]
            .get("body")
            .is_some()
    )
};
}

#[test]
fn fault() {
    for (fault, expected) in [
        (None, (false, 1)),
        (Some(MutationFault::LostAck), (true, 1)),
        (Some(MutationFault::BeforeAccept), (false, 0)),
    ] {
        let s = support::fixture_server(fixture());
        s.edit(|m| m.behaviour.fault = fault);
        let reply = s.request("POST","/memories",json!({"test_operation_id":"intent","document":{"document_id":"x","body":"x","tags":[]},"async":true,"update_mode":"replace"}));
        assert_eq!((reply.is_err(), s.edit(|m| m.operations.len())), expected);
    }
}
#[test]
fn assigned_identifiers_are_opaque() {
    let ids: std::collections::BTreeSet<_> = (0..64).map(|_| opaque_id()).collect();
    assert_eq!(ids.len(), 64);
    assert!(
        ids.iter()
            .all(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    );
    let s = support::fixture_server(fixture());
    let op = retain(&s, "NEW");
    let p = page(&s);
    let job = refresh(&s, &p);
    s.request("DELETE", "/__test__/document/eng-a", json!({}))
        .unwrap();
    let delete = s.edit(|m| m.pending_deletes.keys().next().unwrap().clone());
    for id in [op, p, job, delete] {
        assert_eq!(id.len(), 32);
        assert!(!id.contains("intent"));
    }
}

#[test]
fn every_field_has_a_handler_contrast() {
    let serialized = serde_json::to_value(Behaviour::default()).unwrap();
    let fields: std::collections::BTreeSet<_> = serialized
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(fields, CONTRAST_FIELDS.iter().copied().collect());
    assert_eq!(CONTRAST_FIELDS.len(), fields.len());
}
