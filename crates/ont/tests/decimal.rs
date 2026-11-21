//! A criterion on a declared decimal compares numbers; a string of every
//! other declared type keeps its text order (`specs/ont/action.md` R111).
//! Temporary stores and synthetic schemas only; the command runs in process.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::collections::BTreeMap;
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
            "ont-decimal-{}-{nonce}-{}",
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
    fn install(&self, s: &Value) {
        let p = self.0.join("schema.json");
        std::fs::write(&p, s.to_string()).unwrap();
        let (code, a) = self.run(&["schema", "install", p.to_str().unwrap()], &json!({}));
        assert_eq!(code, 0, "{a}");
    }
    fn applied(&self, action: &str, parameters: Value) -> Value {
        let (code, out) = self.run(
            &["action", "apply"],
            &json!({"action_type":action,"parameters":parameters}),
        );
        assert_eq!(code, 0, "{out}");
        out["result"]["edits"][0]["id"].clone()
    }
    fn validate(&self, action: &str, parameters: Value) -> (i32, Value) {
        self.run(
            &["action", "validate"],
            &json!({"action_type":action,"parameters":parameters}),
        )
    }
    /// Each criterion's code and whether it passed.
    fn passed(&self, action: &str, parameters: Value) -> BTreeMap<String, bool> {
        let (code, out) = self.validate(action, parameters);
        assert_eq!(code, 0, "{out}");
        out["result"]["criteria"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["code"].as_str().unwrap().to_string(),
                    c["passed"].as_bool().unwrap(),
                )
            })
            .collect()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn criterion(code: &str, condition: Value) -> Value {
    json!({"code":code,"message":format!("{code} failed."),"all":[condition]})
}
fn decimal() -> Value {
    json!({"type":"decimal","required":true})
}
fn text() -> Value {
    json!({"type":"string","required":true})
}

/// The domain `dec`: an `account` with a decimal `balance`, a string `code`
/// and entries of a decimal `value` (link reverse name `entries`); `compare`
/// compares decimals, `words` strings that look like numbers.
fn schema() -> Value {
    json!({
        "format":"ontology-schema/v1","domain":"dec","version":1,
        "namespace":{"instance":"test","account":"root"},
        "value_types":{"money":{"base":"decimal"}},
        "object_types":{
            "account":{"owner":"ontology","primary_key":{"from":"$action_id"},
                "properties":{"balance":decimal(),"code":text()}},
            "entry":{"owner":"ontology","primary_key":{"from":"$action_id"},
                "properties":{"account":{"type":{"ref":"account"},"required":true},"value":decimal()}}
        },
        "link_types":{"entry_account":{"from":"entry","via":"account","to":"account",
            "cardinality":"many_to_one","names":{"forward":"account_of","reverse":"entries"}}},
        "action_types":{
            "open":{"parameters":{"balance":decimal(),"code":text()},
                "rules":[{"create_object":"account","set":{"balance":"$balance","code":"$code"}}]},
            "post":{"parameters":{"account":{"type":{"ref":"account"},"required":true},"value":decimal()},
                "rules":[{"create_object":"entry","set":{"account":"$account","value":"$value"}}]},
            "compare":{"parameters":{
                    "account":{"type":{"ref":"account"},"required":true},
                    "big":decimal(),"small":decimal(),"one":decimal(),
                    "zero":{"type":{"value_type":"money"},"required":true}},
                "submission_criteria":[
                    criterion("gt_literal",json!({"gt":["$big","100"]})),
                    criterion("lt_literal",json!({"lt":["$small","10"]})),
                    criterion("eq_scale",json!({"eq":["$one","1"]})),
                    criterion("ne_scale",json!({"ne":["$one","1.000"]})),
                    criterion("eq_negative_zero",json!({"eq":["$zero","0"]})),
                    criterion("ge_value_type",json!({"ge":["$zero","-0.0"]})),
                    criterion("in_list",json!({"in":["$one",["2","01.000"]]})),
                    criterion("lt_parameters",json!({"lt":["$small","$big"]})),
                    criterion("ge_bare_property",json!({"ge":["balance","100"]})),
                    criterion("gt_reference_property",json!({"gt":["$account.balance","99.5"]})),
                    criterion("linked_where",json!({"linked":{"from":"$account","link":"entries",
                        "where":{"all":[{"gt":["value","20"]}]}}}))
                ],
                "rules":[]},
            "odd":{"parameters":{"big":decimal()},
                "submission_criteria":[criterion("not_a_decimal",json!({"gt":["$big","abc"]}))],
                "rules":[]},
            "words":{"parameters":{
                    "account":{"type":{"ref":"account"},"required":true},
                    "big":text(),"small":text(),"one":text()},
                "submission_criteria":[
                    criterion("gt_literal",json!({"gt":["$big","100"]})),
                    criterion("lt_literal",json!({"lt":["$small","10"]})),
                    criterion("eq_scale",json!({"eq":["$one","1"]})),
                    criterion("in_list",json!({"in":["$one",["1"]]})),
                    criterion("text_order",json!({"lt":["$big","100"]})),
                    criterion("gt_bare_property",json!({"gt":["code","100"]})),
                    criterion("not_a_decimal",json!({"gt":["$big","abc"]}))
                ],
                "rules":[]}
        },
        "grants":[{"principal":"agent","scope":{},"read":["account","entry"],"derive":[],
            "actions":["open","post","compare","odd","words"]}]
    })
}

#[test]
fn decimal_criterion_compares_numbers() {
    // spec: specs/ont/action.md#R111
    let t = Temp::new();
    t.install(&schema());
    let account = t.applied("open", json!({"balance":"0999","code":"0999"}));
    t.applied("post", json!({"account":account,"value":"100"}));
    let passed = t.passed(
        "compare",
        json!({"account":account,"big":"0999","small":"9","one":"1.0","zero":"-0"}),
    );
    let expected: BTreeMap<String, bool> = [
        ("gt_literal", true),
        ("lt_literal", true),
        ("eq_scale", true),
        ("ne_scale", false),
        ("eq_negative_zero", true),
        ("ge_value_type", true),
        ("in_list", true),
        ("lt_parameters", true),
        ("ge_bare_property", true),
        ("gt_reference_property", true),
        ("linked_where", true),
    ]
    .into_iter()
    .map(|(c, p)| (c.to_string(), p))
    .collect();
    assert_eq!(passed, expected);
    // An operand that is not a decimal makes the comparison unevaluable.
    let (code, out) = t.validate("odd", json!({"big":"1"}));
    assert_eq!(code, 2, "{out}");
    assert_eq!(out["code"], "criterion_not_evaluable", "{out}");
}

#[test]
fn string_criterion_keeps_text_order() {
    // spec: specs/ont/action.md#R111
    let t = Temp::new();
    t.install(&schema());
    let account = t.applied("open", json!({"balance":"1","code":"0999"}));
    let passed = t.passed(
        "words",
        json!({"account":account,"big":"0999","small":"9","one":"1.0"}),
    );
    let expected: BTreeMap<String, bool> = [
        ("gt_literal", false),
        ("lt_literal", false),
        ("eq_scale", false),
        ("in_list", false),
        ("text_order", true),
        ("gt_bare_property", false),
        ("not_a_decimal", false),
    ]
    .into_iter()
    .map(|(c, p)| (c.to_string(), p))
    .collect();
    assert_eq!(passed, expected);
}
