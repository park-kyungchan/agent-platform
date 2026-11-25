//! Two domains in one store do not reach each other by a name: an action
//! name, a grant, a `linked` link and an `around` path entry each resolve in
//! one domain (`specs/ont/action.md` R105 to R107, `specs/ont/read.md` R104,
//! `specs/ont/schema-language.md` R27). Temporary stores and synthetic
//! domains only; the command runs in process.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const NOW: u64 = 1000;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ont-domains-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn store(&self) -> String {
        self.0.join("store.sqlite").display().to_string()
    }
    /// The command in process (uid 0 passed): (exit code, stdout as JSON).
    fn run(&self, args: &[&str], stdin: &Value) -> (i32, Value) {
        let now = NOW.to_string();
        let store = self.store();
        let mut a: Vec<String> = ["--store", store.as_str(), "--now", now.as_str()]
            .iter()
            .map(|s| s.to_string())
            .collect();
        a.extend(args.iter().map(|s| s.to_string()));
        let out = ont::run(&a, stdin.to_string().as_bytes(), 0, 0);
        let value = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|_| panic!("stdout {:?} stderr {:?}", out.stdout, out.stderr));
        (out.code, value)
    }
    fn install(&self, schemas: &[Value]) {
        for (i, s) in schemas.iter().enumerate() {
            let file = self.0.join(format!("schema-{i}.json"));
            std::fs::write(&file, s.to_string()).unwrap();
            let (code, out) = self.run(&["schema", "install", file.to_str().unwrap()], &json!({}));
            assert_eq!(code, 0, "{out}");
        }
    }
    fn apply(&self, request: Value) -> (i32, Value) {
        self.run(&["action", "apply"], &request)
    }
    fn validate(&self, request: Value) -> (i32, Value) {
        self.run(&["action", "validate"], &request)
    }
    /// An applied action's first edited id.
    fn applied(&self, request: Value) -> Value {
        let (code, out) = self.apply(request);
        assert_eq!(code, 0, "{out}");
        assert_eq!(out["result"]["outcome"], "applied", "{out}");
        out["result"]["edits"][0]["id"].clone()
    }
    /// `(action_type, domain)` of every `action_log` row.
    fn log_rows(&self) -> Vec<(String, String)> {
        let (code, out) = self.run(&["log"], &json!({}));
        assert_eq!(code, 0, "{out}");
        out["result"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["action_type"].as_str().unwrap().to_string(),
                    r["domain"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }
    /// The lines of `refusals.jsonl` (none when it is absent).
    fn refusal_lines(&self) -> Vec<Value> {
        std::fs::read_to_string(self.0.join("refusals.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn search(&self, object_type: &str) -> Vec<Value> {
        let (code, out) = self.run(&["search"], &json!({"object_type":object_type}));
        assert_eq!(code, 0, "{out}");
        out["result"]["objects"].as_array().unwrap().clone()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn refused(answer: &(i32, Value), code: &str) -> Value {
    let (exit, body) = answer;
    assert_eq!(*exit, 2, "{body}");
    assert_eq!(body["status"], "refused", "{body}");
    assert_eq!(body["code"], code, "{body}");
    body.clone()
}

/// A domain `d` with the types `<d>_item` and `<d>_tag` (a tag names an
/// item), the link `tagging` (forward `item_of`, reverse `tags`), the action
/// `mark` (every domain declares it: it creates an item of its own domain),
/// `tag`, `inspect` (refused when the item has a tag) and `only_<d>`; the
/// grant lists those of `granted` the domain declares.
fn domain(d: &str, granted: &[&str]) -> Value {
    let item = format!("{d}_item");
    let tag = format!("{d}_tag");
    let mut actions = json!({
        "mark":{"parameters":{"label":{"type":"string","required":true}},
            "rules":[{"create_object":item,"set":{"label":"$label"}}]},
        "tag":{"parameters":{"item":{"type":{"ref":item},"required":true}},
            "rules":[{"create_object":tag,"set":{"item":"$item"}}]},
        "inspect":{"parameters":{"item":{"type":{"ref":item},"required":true}},
            "submission_criteria":[{"code":"untagged","message":"The item has a tag.",
                "none":[{"linked":{"from":"$item","link":"tags"}}]}],
            "rules":[]}
    });
    actions[format!("only_{d}")] = json!({"parameters":{"label":{"type":"string","required":true}},
        "rules":[{"create_object":item,"set":{"label":"$label"}}]});
    let granted: Vec<&str> = granted
        .iter()
        .copied()
        .filter(|a| actions.get(*a).is_some())
        .collect();
    json!({
        "format":"ontology-schema/v1","domain":d,"version":1,
        "namespace":{"instance":"test","account":"root"},
        "object_types":{
            item.clone():{"owner":"ontology","primary_key":{"from":"$action_id"},
                "properties":{"label":{"type":"string","required":true}}},
            tag.clone():{"owner":"ontology","primary_key":{"from":"$action_id"},
                "properties":{"item":{"type":{"ref":item},"required":true}}}
        },
        "link_types":{"tagging":{"from":tag,"via":"item","to":item,"cardinality":"many_to_one",
            "names":{"forward":"item_of","reverse":"tags"}}},
        "action_types":actions,
        "grants":[{"principal":"agent","scope":{},"read":[item,tag],"derive":[],"actions":granted}]
    })
}

const ALL: &[&str] = &["mark", "tag", "inspect", "only_alpha", "only_beta"];

#[test]
fn shared_action_name_reaches_the_named_domain() {
    // spec: specs/ont/action.md#R105
    // spec: specs/ont/action.md#R106
    // spec: specs/ont/schema-language.md#R27
    let t = Temp::new();
    t.install(&[domain("alpha", ALL), domain("beta", ALL)]);
    for d in ["alpha", "beta"] {
        let request = json!({"action_type":"mark","domain":d,"parameters":{"label":d}});
        let (code, out) = t.validate(request.clone());
        assert_eq!(code, 0, "{out}");
        assert_eq!(out["result"]["valid"], true, "{out}");
        assert_eq!(
            out["result"]["edits"][0]["id"]["object_type"],
            format!("{d}_item"),
            "{out}"
        );
        let id = t.applied(request);
        assert_eq!(id["object_type"], format!("{d}_item"), "{id}");
    }
    let mut rows = t.log_rows();
    rows.sort();
    assert_eq!(
        rows,
        vec![
            ("mark".to_string(), "alpha".to_string()),
            ("mark".to_string(), "beta".to_string())
        ]
    );
    assert_eq!(t.search("alpha_item").len(), 1);
    assert_eq!(t.search("beta_item").len(), 1);
    // A null `domain` names none.
    refused(
        &t.apply(json!({"action_type":"only_beta","domain":null,"parameters":{"label":5}})),
        "invalid_parameter",
    );
}

#[test]
fn shared_action_name_without_domain_is_ambiguous() {
    // spec: specs/ont/action.md#R105
    let t = Temp::new();
    // `gamma` declares `mark` and grants nothing of it: it is still named.
    t.install(&[
        domain("alpha", ALL),
        domain("beta", ALL),
        domain("gamma", &["only_gamma"]),
    ]);
    let request = json!({"action_type":"mark","parameters":{"label":"x"}});
    for answer in [t.validate(request.clone()), t.apply(request)] {
        let body = refused(&answer, "action_ambiguous");
        assert_eq!(body["field"], "/domain", "{body}");
        assert_eq!(body["domains"], json!(["alpha", "beta", "gamma"]), "{body}");
    }
    let body = refused(
        &t.apply(json!({"action_type":"mark","domain":"nowhere","parameters":{"label":"x"}})),
        "unknown_domain",
    );
    assert_eq!(body["field"], "/domain", "{body}");
    let body = refused(
        &t.validate(json!({"action_type":"only_beta","domain":"alpha","parameters":{"label":"x"}})),
        "unknown_action",
    );
    assert_eq!(body["field"], "/action_type", "{body}");
    // Neither refusal is recorded: the action type is not known.
    assert!(t.refusal_lines().is_empty());
    assert!(t.log_rows().is_empty());
    // A name only one domain declares resolves without `domain`.
    let id = t.applied(json!({"action_type":"only_beta","parameters":{"label":"x"}}));
    assert_eq!(id["object_type"], "beta_item");
    assert_eq!(
        t.log_rows(),
        vec![("only_beta".to_string(), "beta".to_string())]
    );
}

#[test]
fn grant_of_one_domain_admits_nothing_in_another() {
    // spec: specs/ont/action.md#R106
    let t = Temp::new();
    t.install(&[domain("alpha", &["mark"]), domain("beta", &["only_beta"])]);
    let request = json!({"action_type":"mark","domain":"beta","parameters":{"label":"x"}});
    refused(&t.validate(request.clone()), "action_forbidden");
    refused(&t.apply(request), "action_forbidden");
    assert!(t.log_rows().is_empty());
    assert!(t.search("beta_item").is_empty());
    let lines = t.refusal_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["domain"], "beta");
    assert_eq!(lines[0]["code"], "action_forbidden");
    assert_eq!(lines[0]["action_type"], "mark");
    // The granted domain answers.
    let id = t.applied(json!({"action_type":"mark","domain":"alpha","parameters":{"label":"x"}}));
    assert_eq!(id["object_type"], "alpha_item");
}

#[test]
fn replay_under_another_domain_is_reused() {
    // spec: specs/ont/action.md#R10
    let t = Temp::new();
    t.install(&[domain("alpha", ALL), domain("beta", ALL)]);
    let first = json!({"action_type":"mark","domain":"alpha","request_id":"r-1",
        "parameters":{"label":"x"}});
    let id = t.applied(first.clone());
    let body = refused(
        &t.apply(
            json!({"action_type":"mark","domain":"beta","request_id":"r-1",
            "parameters":{"label":"x"}}),
        ),
        "request_id_reused",
    );
    assert!(body.get("action_id").is_none(), "{body}");
    assert!(t.search("beta_item").is_empty());
    assert_eq!(
        t.log_rows(),
        vec![("mark".to_string(), "alpha".to_string())]
    );
    let lines = t.refusal_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["domain"], "beta");
    assert_eq!(lines[0]["code"], "request_id_reused");
    // The same request under its own domain is the replay.
    let (code, out) = t.apply(first);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["outcome"], "already_applied", "{out}");
    assert_eq!(out["result"]["edits"][0]["id"], id);
}

#[test]
fn linked_follows_the_link_of_the_actions_domain() {
    // spec: specs/ont/action.md#R107
    let mut verdicts = Vec::new();
    for second in [false, true] {
        let t = Temp::new();
        let mut schemas = vec![domain("alpha", ALL)];
        if second {
            // `beta` sorts after `alpha` and declares the same link names.
            schemas.push(domain("beta", ALL));
        }
        t.install(&schemas);
        let item =
            t.applied(json!({"action_type":"mark","domain":"alpha","parameters":{"label":"x"}}));
        let request = json!({"action_type":"inspect","domain":"alpha","parameters":{"item":item}});
        let (code, before) = t.validate(request.clone());
        assert_eq!(code, 0, "{before}");
        assert_eq!(before["result"]["valid"], true, "{before}");
        t.applied(json!({"action_type":"tag","domain":"alpha","parameters":{"item":item}}));
        if second {
            // A tag of the other domain's link exists too; it is not scanned.
            let other =
                t.applied(json!({"action_type":"mark","domain":"beta","parameters":{"label":"y"}}));
            t.applied(json!({"action_type":"tag","domain":"beta","parameters":{"item":other}}));
        }
        let (code, after) = t.validate(request.clone());
        assert_eq!(code, 0, "{after}");
        assert_eq!(after["result"]["valid"], false, "{after}");
        let basis_types: Vec<String> = after["result"]["basis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["id"]["object_type"].as_str().unwrap().to_string())
            .collect();
        assert!(
            basis_types.iter().all(|t| t.starts_with("alpha_")),
            "{after}"
        );
        refused(&t.apply(request), "submission_criteria");
        verdicts.push((
            before["result"]["criteria"].clone(),
            after["result"]["criteria"].clone(),
        ));
    }
    assert_eq!(verdicts[0], verdicts[1]);
}

#[test]
fn around_follows_the_link_of_the_reached_types_domain() {
    // spec: specs/ont/read.md#R104
    let t = Temp::new();
    t.install(&[domain("alpha", ALL), domain("beta", ALL)]);
    let item = t.applied(json!({"action_type":"mark","domain":"alpha","parameters":{"label":"x"}}));
    let tag = t.applied(json!({"action_type":"tag","domain":"alpha","parameters":{"item":item}}));
    let other = t.applied(json!({"action_type":"mark","domain":"beta","parameters":{"label":"y"}}));
    t.applied(json!({"action_type":"tag","domain":"beta","parameters":{"item":other}}));
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"alpha_item"},"path":["tags"]}),
    );
    assert_eq!(code, 0, "{out}");
    let objects = out["result"]["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 1, "{out}");
    assert_eq!(objects[0]["id"], tag, "{out}");
    let (code, out) = t.run(
        &["around"],
        &json!({"from":{"object_type":"alpha_item"},"path":["tags","item_of"]}),
    );
    assert_eq!(code, 0, "{out}");
    let objects = out["result"]["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 1, "{out}");
    assert_eq!(objects[0]["id"], item, "{out}");
    let body = refused(
        &t.run(
            &["around"],
            &json!({"from":{"object_type":"alpha_item"},"path":["absent"]}),
        ),
        "invalid_link",
    );
    assert_eq!(body["available"], json!(["tags"]), "{body}");
}
