//! Interfaces (`specs/ont/schema-language.md` R31 to R35, `specs/ont/action.md`
//! R116, `specs/ont/read.md` R105 and R106): on a temporary store, one action
//! whose parameter is an interface applied to objects of two implementing
//! types, a call with an object of a type that does not implement it refused
//! without a write, and the reads over the interface. Synthetic schemas only;
//! the command runs in process.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ont-interface-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn run(&self, cmd: &[&str], body: &Value) -> (i32, Value) {
        let mut args = vec![
            "--store".to_string(),
            self.0.join("store.sqlite").display().to_string(),
            "--now".into(),
            "1000".into(),
        ];
        args.extend(cmd.iter().map(|s| s.to_string()));
        let out = ont::run(&args, body.to_string().as_bytes(), 0, 0);
        let value = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|_| panic!("stdout {:?} stderr {:?}", out.stdout, out.stderr));
        (out.code, value)
    }
    fn file(&self, name: &str, s: &Value) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, s.to_string()).unwrap();
        p.display().to_string()
    }
    fn install(&self, name: &str, s: &Value) -> (i32, Value) {
        self.run(&["schema", "install", &self.file(name, s)], &json!({}))
    }
    fn apply(&self, action: &str, parameters: Value) -> (i32, Value) {
        self.run(
            &["action", "apply"],
            &json!({"action_type":action,"parameters":parameters}),
        )
    }
    fn validate(&self, action: &str, parameters: Value) -> (i32, Value) {
        self.run(
            &["action", "validate"],
            &json!({"action_type":action,"parameters":parameters}),
        )
    }
    fn count(&self, table: &str) -> i64 {
        let db = rusqlite::Connection::open(self.0.join("store.sqlite")).unwrap();
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
    fn lines(&self, file: &str) -> Vec<Value> {
        std::fs::read_to_string(self.0.join(file))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn search(&self, request: Value) -> (i32, Value) {
        self.run(&["search"], &request)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn id(object_type: &str, key: &str) -> Value {
    json!({"object_type":object_type,"namespace":{"instance":"test","account":"work"},"native_key":key})
}

fn add(object_type: &str) -> Value {
    json!({"parameters":{"object":{"type":{"ref":object_type},"required":true},
                         "title":{"type":"string","required":true}},
           "rules":[{"create_object":object_type,"set":{"title":"$title"}}]})
}

/// `subject` implemented by `intent` and `unit`; `host` does not implement
/// it; a `hold` references a subject, through the link `hold_subject`.
fn schema() -> Value {
    let subject_type = json!({"owner":"ontology","primary_key":{"from":"/native_key"},"title":"title",
        "implements":["subject"],"properties":{"title":{"type":"string","required":true}}});
    json!({
        "format":"ontology-schema/v1","domain":"work","version":1,
        "namespace":{"instance":"test","account":"work"},
        "interfaces":{"subject":{"description":"What work is about.",
            "properties":{"title":{"type":"string","required":true,"description":"Its title."}}}},
        "object_types":{
            "intent":subject_type,
            "unit":subject_type,
            "host":{"owner":"ontology","primary_key":{"from":"/native_key"},"title":"title",
                "properties":{"title":{"type":"string","required":true}}},
            "hold":{"owner":"ontology","primary_key":{"from":"$action_id"},
                "properties":{"subject":{"type":{"ref":"subject"},"required":true},
                              "reason":{"type":"string","required":true}}}},
        "link_types":{"hold_subject":{"from":"hold","via":"subject","to":"subject",
            "cardinality":"many_to_one","names":{"forward":"subject","reverse":"holds"}}},
        "action_types":{
            "add_intent":add("intent"),"add_unit":add("unit"),"add_host":add("host"),
            "set_hold":{"parameters":{"subject":{"type":{"ref":"subject"},"required":true},
                                      "reason":{"type":"string","required":true}},
                "submission_criteria":[{"code":"subject_exists","message":"The subject must exist.",
                    "all":[{"exists":"$subject"}]}],
                "rules":[{"create_object":"hold","set":{"subject":"$subject","reason":"$reason"}}]},
            "mark":{"parameters":{"subject":{"type":{"ref":"subject"},"required":true}},
                "rules":[{"create_object":"hold","set":{"subject":"$subject","reason":"mark"}}]},
            "close_unit":{"parameters":{"unit":{"type":{"ref":"unit"},"required":true}},
                "submission_criteria":[{"code":"blocked","message":"A hold must block the unit.",
                    "all":[{"linked":{"from":"$unit","link":"holds",
                        "where":{"all":[{"eq":["reason","blocked"]}]}}}]}],
                "rules":[{"modify_object":"unit","set":{"title":"closed"}}]}
        },
        "grants":[{"principal":"agent","read":["intent","unit","host","hold"],
            "actions":["add_intent","add_unit","add_host","set_hold","mark","close_unit"]}]
    })
}

fn installed() -> Temp {
    let t = Temp::new();
    let (code, out) = t.install("work.json", &schema());
    assert_eq!(code, 0, "{out}");
    for (action, ty, key) in [
        ("add_intent", "intent", "i1"),
        ("add_unit", "unit", "u1"),
        ("add_unit", "unit", "u2"),
        ("add_host", "host", "h1"),
    ] {
        let (code, out) = t.apply(action, json!({"object":id(ty, key),"title":key}));
        assert_eq!(code, 0, "{out}");
    }
    t
}

fn keys(answer: &Value) -> Vec<String> {
    answer["result"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}/{}",
                o["id"]["object_type"].as_str().unwrap(),
                o["id"]["native_key"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn an_interface_parameter_admits_the_implementing_types_only() {
    // spec: specs/ont/action.md#R116
    // spec: specs/ont/schema-language.md#R33
    let t = installed();
    for (ty, key) in [("intent", "i1"), ("unit", "u1")] {
        let (code, out) = t.validate(
            "set_hold",
            json!({"subject":id(ty, key),"reason":"blocked"}),
        );
        assert_eq!((code, &out["result"]["valid"]), (0, &json!(true)), "{out}");
        let (code, out) = t.apply(
            "set_hold",
            json!({"subject":id(ty, key),"reason":"blocked"}),
        );
        assert_eq!(code, 0, "{out}");
        assert_eq!(out["result"]["outcome"], "applied", "{out}");
    }
    let rows = t.count("action_log");
    let observations = t.count("observations");
    let edits = t.lines("edits.jsonl").len();
    assert_eq!(rows, 6);
    assert_eq!(edits, 6);
    // An object of a type that does not implement the interface.
    let host = json!({"subject":id("host", "h1"),"reason":"blocked"});
    for cmd in ["validate", "apply"] {
        let (code, out) = t.run(
            &["action", cmd],
            &json!({"action_type":"set_hold","parameters":host}),
        );
        assert_eq!(code, 2, "{out}");
        assert_eq!(out["code"], "invalid_parameter", "{out}");
        assert_eq!(out["field"], "/parameters/subject", "{out}");
    }
    assert_eq!(t.count("action_log"), rows);
    assert_eq!(t.count("observations"), observations);
    assert_eq!(t.lines("edits.jsonl").len(), edits);
    let refusals = t.lines("refusals.jsonl");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0]["code"], "invalid_parameter");
    assert_eq!(refusals[0]["action_type"], "set_hold");
    assert_eq!(refusals[0]["outcome"], "refused");
    assert_eq!(refusals[0]["parameters"], host);
    // A stored hold names its subject as the implementing type's id.
    let (code, out) =
        t.search(json!({"object_type":"hold","where":{"eq":["subject",id("unit","u1")]}}));
    assert_eq!(code, 0, "{out}");
    assert_eq!(keys(&out).len(), 1, "{out}");
    // A filter with an id of another type is wrong_value_type.
    let (code, out) =
        t.search(json!({"object_type":"hold","where":{"eq":["subject",id("host","h1")]}}));
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("wrong_value_type")),
        "{out}"
    );
    // The reverse name of the link into the interface, from an implementing
    // type, in a criterion: u1 is held, u2 is not.
    let (code, out) = t.validate("close_unit", json!({"unit":id("unit","u1")}));
    assert_eq!((code, &out["result"]["valid"]), (0, &json!(true)), "{out}");
    let (code, out) = t.validate("close_unit", json!({"unit":id("unit","u2")}));
    assert_eq!((code, &out["result"]["valid"]), (0, &json!(false)), "{out}");
}

#[test]
fn search_over_an_interface_reads_the_implementing_types() {
    // spec: specs/ont/read.md#R105
    let t = installed();
    let (code, out) = t.search(json!({"object_type":"subject"}));
    assert_eq!(code, 0, "{out}");
    assert_eq!(keys(&out), ["intent/i1", "unit/u1", "unit/u2"]);
    assert_eq!(out["result"]["basis"]["scanned"], 3, "{out}");
    assert_eq!(out["result"]["basis"]["matched"], 3, "{out}");
    assert_eq!(out["result"]["continuation"], Value::Null);
    assert_eq!(out["result"]["objects"][0]["title"], "i1");
    // Pages across the types: limit 1, then each continuation.
    let mut seen = Vec::new();
    let mut continuation = Value::Null;
    loop {
        let mut request = json!({"object_type":"subject","limit":1});
        if !continuation.is_null() {
            request["continuation"] = continuation.clone();
        }
        let (code, out) = t.search(request);
        assert_eq!(code, 0, "{out}");
        seen.extend(keys(&out));
        continuation = out["result"]["continuation"].clone();
        if continuation.is_null() {
            break;
        }
        assert!(continuation.as_str().unwrap().starts_with("i1:"), "{out}");
    }
    assert_eq!(seen, ["intent/i1", "unit/u1", "unit/u2"]);
    // A filter names the interface's properties only.
    let (code, out) = t.search(json!({"object_type":"subject","where":{"eq":["title","u2"]}}));
    assert_eq!(code, 0, "{out}");
    assert_eq!(keys(&out), ["unit/u2"]);
    let (code, out) = t.search(json!({"object_type":"subject","where":{"eq":["reason","x"]}}));
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("undeclared_property")),
        "{out}"
    );
    // A token of another search is invalid here; a search of one type keeps
    // its own token.
    let (_, one) = t.search(json!({"object_type":"unit","limit":1}));
    let c1 = one["result"]["continuation"].clone();
    assert!(c1.as_str().unwrap().starts_with("c1:"), "{one}");
    let (code, out) = t.search(json!({"object_type":"unit","continuation":c1}));
    assert_eq!(code, 0, "{out}");
    assert_eq!(keys(&out), ["unit/u2"]);
    let (_, page) = t.search(json!({"object_type":"subject","limit":1}));
    let i1 = page["result"]["continuation"].clone();
    for (object_type, token) in [
        ("subject", c1.clone()),
        ("unit", i1.clone()),
        ("subject", json!("i1:zz")),
    ] {
        let (code, out) = t.search(json!({"object_type":object_type,"continuation":token}));
        assert_eq!(
            (code, out["code"].as_str()),
            (2, Some("invalid_continuation")),
            "{out}"
        );
    }
}

#[test]
fn around_follows_links_into_the_interface() {
    // spec: specs/ont/read.md#R106
    let t = installed();
    for (ty, key, reason) in [("intent", "i1", "blocked"), ("unit", "u1", "waiting")] {
        let (code, out) = t.apply("set_hold", json!({"subject":id(ty, key),"reason":reason}));
        assert_eq!(code, 0, "{out}");
    }
    // From an object of an implementing type by the reverse name of the link
    // into the interface.
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"unit","where":{"eq":["title","u1"]}},"path":["holds"]}),
    );
    assert_eq!(code, 0, "{out}");
    let holds = &out["result"]["objects"];
    assert_eq!(holds.as_array().unwrap().len(), 1, "{out}");
    assert_eq!(holds[0]["id"]["object_type"], "hold");
    // From the interface, and back by the forward name into it.
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"subject"},"path":["holds","subject"]}),
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(keys(&out), ["intent/i1", "unit/u1"]);
    assert_eq!(out["result"]["basis"]["dangling"], 0, "{out}");
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"subject","where":{"eq":["title","i1"]}},"path":["holds"]}),
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["basis"]["matched"], 1, "{out}");
    // The type reached is the interface: no link leaves it forward.
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"hold"},"path":["subject","subject"]}),
    );
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("invalid_link")),
        "{out}"
    );
    assert_eq!(out["available"], json!(["holds"]), "{out}");
    // A type that does not implement it has no such link.
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"host"},"path":["holds"]}),
    );
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("invalid_link")),
        "{out}"
    );
}

#[test]
fn describe_lists_the_interface_with_the_types_the_principal_reads() {
    // spec: specs/ont/schema-language.md#R35
    let t = installed();
    let (code, out) = t.run(&["describe"], &json!({}));
    assert_eq!(code, 0, "{out}");
    let result = &out["result"];
    assert_eq!(
        result["interfaces"],
        json!([{"domain":"work","name":"subject","description":"What work is about.",
            "properties":[{"name":"title","description":"Its title.","type":"string","required":true}],
            "implemented_by":["intent","unit"]}])
    );
    let types = result["object_types"].as_array().unwrap();
    let unit = types.iter().find(|t| t["name"] == "unit").unwrap();
    assert_eq!(unit["implements"], json!(["subject"]));
    let host = types.iter().find(|t| t["name"] == "host").unwrap();
    assert!(host.get("implements").is_none(), "{host}");
    assert!(
        result["link_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["name"] == "hold_subject" && l["to"] == "subject"),
        "{out}"
    );
    let (_, one) = t.run(&["describe"], &json!({"object_type":"subject"}));
    assert_eq!(one["result"]["interfaces"][0]["name"], "subject");
    assert_eq!(one["result"]["object_types"], json!([]));
    assert_eq!(one["result"]["link_types"].as_array().unwrap().len(), 1);
    let (_, unit) = t.run(&["describe"], &json!({"object_type":"unit"}));
    assert!(unit["result"].get("interfaces").is_none(), "{unit}");

    // A principal that reads no implementing type sees no interface, no link
    // into it, and cannot search or start from it.
    let t = Temp::new();
    let mut s = schema();
    s["grants"][0]["read"] = json!(["host", "hold"]);
    let (code, out) = t.install("work.json", &s);
    assert_eq!(code, 0, "{out}");
    let (_, out) = t.run(&["describe"], &json!({}));
    assert!(out["result"].get("interfaces").is_none(), "{out}");
    assert_eq!(out["result"]["link_types"], json!([]), "{out}");
    let (code, out) = t.search(json!({"object_type":"subject"}));
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("unavailable")),
        "{out}"
    );
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"subject"},"path":["holds"]}),
    );
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("unavailable")),
        "{out}"
    );
}

#[test]
fn a_name_is_one_type_or_one_interface_among_installed_domains() {
    // spec: specs/ont/schema-language.md#R34
    let t = installed();
    let other = |object_types: Value, interfaces: Value| {
        json!({"format":"ontology-schema/v1","domain":"other","version":1,
            "namespace":{"instance":"test","account":"other"},
            "object_types":object_types,"interfaces":interfaces})
    };
    let thing = json!({"owner":"ontology","primary_key":{"from":"$action_id"},"properties":{}});
    for (s, field) in [
        // An object type named like an interface of another domain.
        (
            other(json!({"subject":thing}), json!({})),
            "/interfaces/subject",
        ),
        // An interface named like an interface of another domain.
        (
            other(json!({}), json!({"subject":{"properties":{}}})),
            "/interfaces/subject",
        ),
        // An interface named like an object type of another domain.
        (
            other(json!({}), json!({"unit":{"properties":{}}})),
            "/interfaces/unit",
        ),
    ] {
        let (code, out) = t.install("other.json", &s);
        assert_eq!(code, 2, "{out}");
        assert_eq!(out["code"], "schema_violation", "{out}");
        // A fault of the schema alone names its violations; one of the
        // composition, the one violation that refused it.
        let violation = json!({"code":"duplicate_type_name","pointer":field});
        assert!(
            out["violation"] == violation
                || out["violations"]
                    .as_array()
                    .is_some_and(|v| v.contains(&violation)),
            "{out}"
        );
    }
    // `schema check` with the store sees an interface named like an object
    // type of another domain.
    let file = t.file(
        "check.json",
        &other(json!({}), json!({"unit":{"properties":{}}})),
    );
    let (code, out) = t.run(&["schema", "check", &file], &json!({}));
    assert_eq!(code, 2, "{out}");
    assert_eq!(
        out["violations"],
        json!([{"code":"duplicate_type_name","pointer":"/interfaces/unit"}])
    );
}

#[test]
fn a_stored_interface_reference_keeps_the_namespace_rule() {
    // spec: specs/ont/schema-language.md#R33
    let t = installed();
    let foreign = json!({"object_type":"unit","namespace":{"instance":"test","account":"other"},"native_key":"u1"});
    let rows = t.count("action_log");
    let observations = t.count("observations");
    // The parameter admits the id of an implementing type in any namespace,
    // as a reference to that type does; the stored reference is held to the
    // namespace of the type's domain.
    let (code, out) = t.apply("mark", json!({"subject":foreign}));
    assert_eq!(code, 2, "{out}");
    assert!(out.to_string().contains("ref_namespace_mismatch"), "{out}");
    assert_eq!(t.count("action_log"), rows);
    assert_eq!(t.count("observations"), observations);
    let (code, out) = t.apply("mark", json!({"subject":id("unit", "u1")}));
    assert_eq!(code, 0, "{out}");
}
