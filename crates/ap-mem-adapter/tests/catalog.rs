use ap_mem_adapter::catalog::{initialize, parse_call, tools, tools_list};
use serde_json::{Value, json};

#[test]
fn tools_list_exact_bytes() {
    let expected = include_bytes!("fixtures/tools-list.json");
    let actual = serde_json::to_string(&tools_list(json!(1))).unwrap() + "\n";
    assert_eq!(actual.as_bytes(), expected);
    assert_eq!(actual.len(), 1697);
    assert_eq!(serde_json::to_vec(&tools()).unwrap().len(), 1662);
    for id in [json!("request-한글"), json!(12345)] {
        let wire = serde_json::to_vec(&tools_list(id.clone())).unwrap();
        assert_eq!(
            wire.len() + 1,
            1697 + serde_json::to_vec(&id).unwrap().len() - 1
        );
    }
}

#[test]
fn mcp_read_only() {
    let catalog = tools();
    let names: Vec<_> = catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "mem_recall",
            "mem_page_search",
            "mem_page_read",
            "mem_tree_read",
            "mem_document_read"
        ]
    );
    for name in ["publish", "mem_publish", "retain", "delete", "reflect"] {
        assert_eq!(parse_call(name, json!({})), Err("unknown_tool"));
    }
}

#[test]
fn argument_max_lengths() {
    for tool in ["mem_recall", "mem_page_search"] {
        let args = |id: &str, q: &str| json!({"subject":{"type":"task","id":id},"query":q});
        assert!(parse_call(tool, args(&"한".repeat(200), &"글".repeat(2048))).is_ok());
        for (id, q) in [
            ("한".repeat(201), "q".into()),
            ("x".into(), "글".repeat(2049)),
            (" ".into(), "q".into()),
            ("x".into(), "\n\t".into()),
        ] {
            assert!(parse_call(tool, args(&id, &q)).is_err());
        }
        assert!(
            parse_call(
                tool,
                json!({"subject":{"type":"task","id":"x","bank":"lesson"},"query":"q"})
            )
            .is_err()
        );
    }
    for (tool, key) in [
        ("mem_page_read", "page_id"),
        ("mem_document_read", "record_id"),
    ] {
        assert!(parse_call(tool, json!({key:"a".repeat(200)})).is_ok());
        for value in [
            json!("a".repeat(201)),
            json!(" "),
            json!(""),
            json!(1),
            Value::Null,
        ] {
            assert!(parse_call(tool, json!({key:value})).is_err());
        }
        assert!(parse_call(tool, json!({})).is_err());
    }
    assert!(parse_call("mem_page_read", json!({"page_id":"한".repeat(200)})).is_ok());
    assert!(parse_call("mem_page_read", json!({"page_id":"한".repeat(201)})).is_err());
    assert!(parse_call("mem_document_read", json!({"record_id":"a--b"})).is_err());
    for tool in tools()["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        assert!(parse_call(name, json!({"bank":"lesson"})).is_err());
        assert!(parse_call(name, Value::Null).is_err());
    }
    assert!(parse_call("mem_tree_read", json!({})).is_ok());
}

#[test]
fn initialize_exact() {
    assert_eq!(
        initialize(),
        json!({
            "protocolVersion":"2025-11-25","capabilities":{"tools":{}},
            "serverInfo":{"name":"ap-mem-adapter","version":"0.1.0"},
            "instructions":"Engineering memory reads only. Resolve the subject and form an English query; preserve Korean proper nouns. Start with mem_recall, then use a focused page or exact document when needed. mem_tree_read supplies filtered navigation. Only ready records and pages without pending subjects are returned. unresolved_ready_count counts current ready records with unavailable Git metadata; a positive count means partial memory. Busy or unavailable means no current-memory answer; do not fall back to another endpoint. Preserve attribution and verify operational claims at their native owner. Publication is outside MCP."
        })
    );
}
