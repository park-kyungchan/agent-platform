//! The object a rule edits through one parameter: the one its `target`
//! names, or the only declared reference parameter to its type
//! (`specs/ont/action.md` R108 to R110). Temporary stores and synthetic
//! schemas only; the command runs in process.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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
            "ont-rule-target-{}-{nonce}-{}",
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
    fn file(&self, s: &Value) -> String {
        let p = self.0.join("schema.json");
        std::fs::write(&p, s.to_string()).unwrap();
        p.display().to_string()
    }
    fn check(&self, s: &Value) -> Value {
        self.run(&["schema", "check", &self.file(s)], &json!({})).1
    }
    fn install(&self, s: &Value) -> Value {
        self.run(&["schema", "install", &self.file(s)], &json!({}))
            .1
    }
    /// Install `schema()`, then replace its stored bytes by `s`, as bytes
    /// installed before the rule that `s` breaks.
    fn planted(&self, s: &Value) {
        let a = self.install(&schema());
        assert_eq!(a["status"], "ok", "{a}");
        let bytes = s.to_string().into_bytes();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let db = rusqlite::Connection::open(self.0.join("store.sqlite")).unwrap();
        db.execute(
            "UPDATE schema_installs SET bytes=?1,sha256=?2",
            rusqlite::params![bytes, sha],
        )
        .unwrap();
        // The planted bytes change action declarations only.
        db.execute(
            "UPDATE candidate_generation SET projected=mutation,semantics_projected=mutation",
            [],
        )
        .unwrap();
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
    /// The ids an applied action edited.
    fn edited(&self, action: &str, parameters: Value) -> Vec<Value> {
        let (code, out) = self.apply(action, parameters);
        assert_eq!(code, 0, "{out}");
        out["result"]["edits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].clone())
            .collect()
    }
    /// The object's head: `{"fields":{<property>:<value>}}`, a property
    /// without a known value left out.
    fn object(&self, id: &Value) -> Value {
        let (code, out) = self.run(&["get"], &json!({"id":id}));
        assert_eq!(code, 0, "{out}");
        let object = &out["result"]["objects"][0];
        let head = object["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["revision"] == object["head"])
            .unwrap_or_else(|| panic!("{out}"));
        let fields: serde_json::Map<String, Value> = head["fields"]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(k, v)| v.get("value").map(|v| (k.clone(), v.clone())))
            .collect();
        json!({"fields":fields})
    }
    fn log_len(&self) -> usize {
        let (code, out) = self.run(&["log"], &json!({}));
        assert_eq!(code, 0, "{out}");
        out["result"]["rows"].as_array().unwrap().len()
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

fn item() -> Value {
    json!({"type":{"ref":"item"}})
}
fn required_item() -> Value {
    json!({"type":{"ref":"item"},"required":true})
}

/// One domain `rt`: the type `item` (keyed by the action id, with a label and
/// a reference `peer` to another item), the type `keyed` (keyed by its
/// reference's `native_key`), and one action per rule kind of R108 whose
/// rule names `z_main` while the optional `a_other` comes first in name order.
fn schema() -> Value {
    json!({
        "format":"ontology-schema/v1","domain":"rt","version":1,
        "namespace":{"instance":"test","account":"root"},
        "object_types":{
            "item":{"owner":"ontology","primary_key":{"from":"$action_id"},
                "properties":{"label":{"type":"string","required":true},"peer":{"type":{"ref":"item"}}}},
            "keyed":{"owner":"ontology","primary_key":{"from":"/native_key"},
                "properties":{"label":{"type":"string"}}}
        },
        "action_types":{
            "seed":{"parameters":{"label":{"type":"string","required":true}},
                "rules":[{"create_object":"item","set":{"label":"$label"}}]},
            "modify":{"parameters":{"z_main":required_item(),"a_other":item(),
                    "label":{"type":"string","required":true}},
                "rules":[{"modify_object":"item","target":"$z_main","set":{"label":"$label"}}]},
            "delete":{"parameters":{"z_main":required_item(),"a_other":item()},
                "rules":[{"delete_object":"item","target":"$z_main"}]},
            "set_ref":{"parameters":{"z_main":required_item(),"a_other":item()},
                "rules":[{"set_reference":{"object_type":"item","property":"peer","value":"$a_other"},
                    "target":"$z_main"}]},
            "clear_ref":{"parameters":{"z_main":required_item(),"a_other":item()},
                "rules":[{"clear_reference":{"object_type":"item","property":"peer"},"target":"$z_main"}]},
            "upsert":{"parameters":{"z_main":item(),"a_other":item(),
                    "label":{"type":"string","required":true}},
                "rules":[{"create_or_modify_object":"item","target":"$z_main","set":{"label":"$label"}}]},
            "modify_opt":{"parameters":{"z_opt":item(),"a_other":item(),
                    "label":{"type":"string","required":true}},
                "rules":[{"modify_object":"item","target":"$z_opt","set":{"label":"$label"}}]},
            "touch":{"parameters":{"item":item(),"label":{"type":"string","required":true}},
                "rules":[{"modify_object":"item","set":{"label":"$label"}}]}
        },
        "grants":[{"principal":"agent","scope":{},"read":["item","keyed"],"derive":[],
            "actions":["seed","modify","delete","set_ref","clear_ref","upsert","modify_opt","touch"]}]
    })
}

#[test]
fn rule_edits_the_parameter_target_names() {
    // spec: specs/ont/action.md#R108
    let t = Temp::new();
    let a = t.install(&schema());
    assert_eq!(a["status"], "ok", "{a}");
    let x = t.edited("seed", json!({"label":"x"})).remove(0);
    let y = t.edited("seed", json!({"label":"y"})).remove(0);
    let both = |extra: Value| {
        let mut p = json!({"z_main":x,"a_other":y});
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        p
    };
    // validate and apply name the same object.
    let (code, out) = t.validate("modify", both(json!({"label":"m"})));
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["edits"][0]["id"], x, "{out}");
    assert_eq!(
        t.edited("modify", both(json!({"label":"m"}))),
        vec![x.clone()]
    );
    assert_eq!(t.object(&x)["fields"]["label"], "m");
    assert_eq!(t.object(&y)["fields"]["label"], "y");
    assert_eq!(t.edited("set_ref", both(json!({}))), vec![x.clone()]);
    assert_eq!(t.object(&x)["fields"]["peer"], y);
    assert!(
        t.object(&y)["fields"]
            .get("peer")
            .is_none_or(Value::is_null)
    );
    assert_eq!(t.edited("clear_ref", both(json!({}))), vec![x.clone()]);
    assert_eq!(
        t.edited("upsert", both(json!({"label":"u"}))),
        vec![x.clone()]
    );
    assert_eq!(t.object(&x)["fields"]["label"], "u");
    assert_eq!(t.object(&y)["fields"]["label"], "y");
    // An absent target of an action-id upsert creates; `a_other` is not read.
    let created = t.edited("upsert", json!({"a_other":y,"label":"n"}));
    assert_eq!(created.len(), 1);
    assert_ne!(created[0], x);
    assert_ne!(created[0], y);
    assert_eq!(t.object(&created[0])["fields"]["label"], "n");
    // A named target the request does not supply is refused at it.
    let body = refused(
        &t.apply("modify_opt", json!({"a_other":y,"label":"q"})),
        "invalid_parameter",
    );
    assert_eq!(body["field"], "/parameters/z_opt", "{body}");
    assert_eq!(t.object(&y)["fields"]["label"], "y");
    // Without `target`, the only reference parameter; absent, `/parameters`.
    assert_eq!(
        t.edited("touch", json!({"item":y,"label":"t"})),
        vec![y.clone()]
    );
    let body = refused(&t.apply("touch", json!({"label":"t"})), "invalid_parameter");
    assert_eq!(body["field"], "/parameters", "{body}");
    assert_eq!(t.edited("delete", both(json!({}))), vec![x.clone()]);
    assert_eq!(t.object(&y)["fields"]["label"], "t");
    let (_, gone) = t.run(&["get"], &json!({"id":x}));
    let head = gone["result"]["objects"][0]["observations"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(head["change"], "retraction", "{gone}");
}

fn violations(answer: &Value) -> BTreeSet<(String, String)> {
    answer["violations"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"))
        .iter()
        .map(|v| {
            (
                v["code"].as_str().unwrap().to_string(),
                v["pointer"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn undetermined_rule_target_refuses_schema_check() {
    // spec: specs/ont/action.md#R109
    let t = Temp::new();
    assert_eq!(t.check(&schema())["status"], "ok");
    let mut s = schema();
    let at = s["action_types"].as_object_mut().unwrap();
    // Every rule kind of R108 without `target`, two references to its type.
    at.insert(
        "amb".into(),
        json!({"parameters":{"first":item(),"second":item(),"label":{"type":"string","required":true}},
            "rules":[
                {"modify_object":"item","set":{"label":"$label"}},
                {"delete_object":"item"},
                {"set_reference":{"object_type":"item","property":"peer","value":"$first"}},
                {"clear_reference":{"object_type":"item","property":"peer"}},
                {"create_or_modify_object":"item","set":{"label":"$label"}}
            ]}),
    );
    let one = |target: &str, extra: Value| {
        let mut parameters =
            json!({"z_main":required_item(),"label":{"type":"string","required":true}});
        for (k, v) in extra.as_object().unwrap() {
            parameters[k] = v.clone();
        }
        json!({"parameters":parameters,
            "rules":[{"modify_object":"item","target":target,"set":{"label":"$label"}}]})
    };
    at.insert("no_dollar".into(), one("z_main", json!({})));
    at.insert("undeclared".into(), one("$nobody", json!({})));
    at.insert("scalar".into(), one("$label", json!({})));
    at.insert(
        "other_type".into(),
        one("$thing", json!({"thing":{"type":{"ref":"keyed"}}})),
    );
    at.insert(
        "array".into(),
        one("$many", json!({"many":{"type":{"array":{"ref":"item"}}}})),
    );
    at.insert(
        "on_create".into(),
        json!({"parameters":{"z_main":required_item(),"label":{"type":"string","required":true}},
            "rules":[{"create_object":"item","target":"$z_main","set":{"label":"$label"}}]}),
    );
    at.insert(
        "on_keyed_upsert".into(),
        json!({"parameters":{"k":{"type":{"ref":"keyed"},"required":true},"label":{"type":"string","required":true}},
            "rules":[{"create_or_modify_object":"keyed","target":"$k","set":{"label":"$label"}}]}),
    );
    let mut expected: BTreeSet<(String, String)> = (0..5)
        .map(|i| {
            (
                "ambiguous_rule_target".to_string(),
                format!("/action_types/amb/rules/{i}"),
            )
        })
        .collect();
    for action in [
        "no_dollar",
        "undeclared",
        "scalar",
        "other_type",
        "array",
        "on_create",
        "on_keyed_upsert",
    ] {
        expected.insert((
            "invalid_rule_target".to_string(),
            format!("/action_types/{action}/rules/0/target"),
        ));
    }
    let answer = t.check(&s);
    assert_eq!(answer["status"], "refused", "{answer}");
    assert_eq!(violations(&answer), expected, "{answer}");
    let answer = t.install(&s);
    assert_eq!(answer["status"], "refused", "{answer}");
    assert_eq!(violations(&answer), expected, "{answer}");
}

#[test]
fn installed_undetermined_target_refuses_only_that_action() {
    // spec: specs/ont/action.md#R110
    let t = Temp::new();
    let mut s = schema();
    s["action_types"]["amb"] = json!({"parameters":{"first":item(),"second":item(),
            "label":{"type":"string","required":true}},
        "rules":[{"modify_object":"item","set":{"label":"$label"}}]});
    s["grants"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!("amb"));
    t.planted(&s);
    // The store opens; reads, backup and the other actions answer.
    let (code, d) = t.run(&["describe"], &json!({}));
    assert_eq!(code, 0, "{d}");
    let x = t.edited("seed", json!({"label":"x"})).remove(0);
    assert_eq!(t.object(&x)["fields"]["label"], "x");
    let (code, b) = t.run(&["backup"], &json!({"op":"backup"}));
    assert_eq!(code, 0, "{b}");
    let rows = t.log_len();
    let parameters = json!({"first":x,"label":"a"});
    for answer in [
        t.validate("amb", parameters.clone()),
        t.apply("amb", parameters.clone()),
    ] {
        let body = refused(&answer, "ambiguous_rule_target");
        assert_eq!(body["pointer"], "/action_types/amb/rules/0", "{body}");
    }
    assert_eq!(t.log_len(), rows);
    assert_eq!(t.object(&x)["fields"]["label"], "x");
    // A new install, the identical bytes included, takes the strict check.
    let answer = t.install(&s);
    assert_eq!(answer["status"], "refused", "{answer}");
    assert!(
        violations(&answer).contains(&(
            "ambiguous_rule_target".to_string(),
            "/action_types/amb/rules/0".to_string()
        )),
        "{answer}"
    );
    // The repair: a higher version that names `target`.
    s["version"] = json!(2);
    s["action_types"]["amb"]["rules"][0]["target"] = json!("$second");
    let answer = t.install(&s);
    assert_eq!(answer["status"], "ok", "{answer}");
    let (code, out) = t.apply("amb", json!({"first":x,"label":"a"}));
    refused(&(code, out), "invalid_parameter");
    assert_eq!(
        t.edited("amb", json!({"second":x,"label":"a"})),
        vec![x.clone()]
    );
    assert_eq!(t.object(&x)["fields"]["label"], "a");
}
