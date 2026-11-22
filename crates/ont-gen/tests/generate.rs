//! Tests of `ont_gen::generate`: the committed modules equal what it gives
//! for the committed fixtures, its refusals, and the form of its text. The
//! hostile module is included below and used nowhere, so that the check
//! compiles and lints it as a consumer that uses none of it would; the
//! interface module is included and used by one test.
#![allow(clippy::unwrap_used)]
use ont_core::schema::{Schema, Violation};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/generated/hostile.rs"
));

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/generated/interface.rs"
));

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(dir().join("fixtures").join(format!("{name}.schema.json"))).unwrap()
}

/// The committed text for `name` equals what `generate` gives for its
/// fixture; on a difference the current text is written under the
/// temporary directory and the test names that file.
fn committed(name: &str) -> String {
    let text = ont_gen::generate(&fixture(name)).unwrap();
    let path = dir().join("generated").join(format!("{name}.rs"));
    let stored = std::fs::read_to_string(&path).unwrap_or_default();
    if stored != text {
        let out = std::env::temp_dir().join(format!("ont-gen-{}-{name}.rs", std::process::id()));
        std::fs::write(&out, &text).unwrap();
        panic!(
            "{} differs from generate of its fixture; the current text is in {}",
            path.display(),
            out.display()
        );
    }
    text
}

fn generate_value(schema: &Value) -> Result<String, Vec<Violation>> {
    ont_gen::generate(schema.to_string().as_bytes())
}

/// A small valid schema to which each case adds what it needs.
fn base() -> Value {
    json!({
        "format": "ontology-schema/v1",
        "domain": "sample",
        "version": 1,
        "namespace": {"instance": "test", "account": "sample"},
        "object_types": {
            "thing": {
                "owner": "ontology",
                "primary_key": {"from": "$action_id"},
                "properties": {"label": {"type": "string", "required": true}}
            }
        },
        "grants": []
    })
}

fn collision(schema: &Value, pointer: &str) {
    let faults = generate_value(schema).unwrap_err();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].code, "name_collision");
    assert_eq!(faults[0].pointer, pointer);
}

// spec: specs/ont/gen.md#R2
// spec: specs/ont/gen.md#R3
#[test]
fn committed_example_equals_generate() {
    let text = committed("example");
    assert!(text.lines().any(|l| l == "pub mod example {"));
}

// spec: specs/ont/gen.md#R3
// spec: specs/ont/gen.md#R4
#[test]
fn committed_hostile_equals_generate() {
    let text = committed("hostile");
    assert!(text.lines().any(|l| l == "pub mod r#type {"));
}

// spec: specs/ont/gen.md#R14
#[test]
fn committed_interface_equals_generate() {
    let text = committed("interface");
    let lines: Vec<&str> = text.lines().collect();
    let at = |item: &str| {
        lines
            .iter()
            .position(|l| l.trim_start() == item)
            .unwrap_or_else(|| panic!("{item}"))
    };
    // After `links`, in byte order of the interfaces and their types.
    assert!(at("pub mod links {") < at("pub mod interfaces {"));
    assert!(lines.contains(&"    pub mod interfaces {"));
    assert!(
        at("pub trait Subject: super::ObjectType {}")
            < at("pub trait Tagged: super::ObjectType {}")
    );
    assert!(
        at("impl Subject for super::objects::Intent {}")
            < at("impl Subject for super::objects::Unit {}")
    );
    assert!(!text.contains("impl Subject for super::objects::Host"));
    assert!(text.contains("pub subject: super::interfaces::SubjectRef,"));
    assert!(text.contains(
        "pub others: ::std::option::Option<::std::vec::Vec<super::interfaces::SubjectRef>>,"
    ));
    assert!(text.contains("pub first: super::interfaces::SubjectRef,"));
    // A link into an interface gives no hop; another link keeps its two.
    assert!(!text.contains("pub struct HoldSubject"));
    assert!(text.contains("pub struct HoldHostReverse;"));
    // A schema without interfaces has no submodule `interfaces`.
    for name in ["example", "hostile"] {
        let text = ont_gen::generate(&fixture(name)).unwrap();
        assert!(!text.contains("pub mod interfaces"), "{name}");
    }
}

// spec: specs/ont/gen.md#R14
#[test]
fn an_interface_reference_is_built_from_the_ids_of_its_types() {
    use iface::interfaces::{Subject, SubjectRef, TaggedRef};
    use iface::{Action, Id, actions, objects};
    fn implements<T: Subject>() -> &'static str {
        T::NAME
    }
    assert_eq!(implements::<objects::Intent>(), "intent");
    assert_eq!(implements::<objects::Unit>(), "unit");
    let unit = Id::<objects::Unit>::new("u1");
    let subject = SubjectRef::from(unit.clone());
    assert_eq!(subject.object_type(), "unit");
    assert_eq!(subject.native_key(), "u1");
    assert_eq!(subject.to_value(), unit.to_value());
    assert_eq!(
        SubjectRef::from_value(&unit.to_value()),
        Some(subject.clone())
    );
    let intent = SubjectRef::from(Id::<objects::Intent>::in_namespace("i", "a", "k"));
    assert_eq!(
        intent.to_value(),
        json!({"object_type":"intent","namespace":{"instance":"i","account":"a"},"native_key":"k"})
    );
    assert_ne!(intent, subject);
    let host = Id::<objects::Host>::new("h1").to_value();
    assert_eq!(SubjectRef::from_value(&host), None);
    assert_eq!(TaggedRef::from_value(&unit.to_value()), None);
    let mut extra = unit.to_value();
    extra["more"] = json!(1);
    assert_eq!(SubjectRef::from_value(&extra), None);
    let action = actions::SetHold {
        subject: subject.clone(),
        others: Some(vec![intent.clone()]),
        pair: Some(actions::SetHoldPair { first: intent }),
    };
    let sent = action.parameters();
    assert_eq!(sent["subject"], unit.to_value());
    assert_eq!(sent["others"][0]["object_type"], "intent");
    assert_eq!(sent["pair"]["first"]["object_type"], "intent");
}

// spec: specs/ont/gen.md#R1
// spec: specs/ont/gen.md#R14
#[test]
fn an_interface_and_its_reference_are_items_of_one_submodule() {
    let mut schema = base();
    schema["interfaces"] = json!({"a": {"properties": {}}, "a_ref": {"properties": {}}});
    collision(&schema, "/interfaces/a_ref");
    // A link into an interface defines no item, so it collides with nothing.
    let mut schema = base();
    schema["interfaces"] = json!({"t": {"properties": {}}});
    schema["object_types"]["thing"]["implements"] = json!(["t"]);
    schema["object_types"]["thing"]["properties"]["to"] =
        json!({"type": {"ref": "t"}, "required": false});
    schema["object_types"]["thing"]["properties"]["back"] =
        json!({"type": {"ref": "thing"}, "required": false});
    schema["link_types"] = json!({
        "step": {"from": "thing", "via": "to", "to": "t", "cardinality": "many_to_one",
                 "names": {"forward": "to", "reverse": "to_of"}},
        "step_reverse": {"from": "thing", "via": "back", "to": "thing", "cardinality": "many_to_one",
                 "names": {"forward": "back", "reverse": "back_of"}}
    });
    let text = generate_value(&schema).unwrap();
    assert!(text.contains("pub struct StepReverse;"));
    assert!(!text.contains("pub struct Step;"));
}

// spec: specs/ont/gen.md#R2
#[test]
fn same_bytes_give_same_text() {
    for name in ["example", "hostile", "interface"] {
        let bytes = fixture(name);
        let a = ont_gen::generate(&bytes).unwrap();
        let b = ont_gen::generate(&bytes.clone()).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
    }
}

// spec: specs/ont/gen.md#R1
#[test]
fn a_refused_schema_gives_its_violations_and_no_text() {
    let mut schema = base();
    schema["version"] = json!(0);
    schema["object_types"]["thing"]["properties"]["Bad"] = json!({"type": "string"});
    let bytes = schema.to_string().into_bytes();
    let expected = Schema::check_bytes(&bytes, &BTreeSet::new()).unwrap_err();
    assert!(!expected.is_empty());
    assert_eq!(ont_gen::generate(&bytes).unwrap_err(), expected);
    assert_eq!(
        ont_gen::generate(b"not json").unwrap_err(),
        Schema::check_bytes(b"not json", &BTreeSet::new()).unwrap_err()
    );
}

// spec: specs/ont/gen.md#R1
// spec: specs/ont/gen.md#R4
#[test]
fn two_types_of_one_rust_name_are_refused() {
    let mut schema = base();
    let thing = schema["object_types"]["thing"].clone();
    schema["object_types"]["a_b"] = thing.clone();
    schema["object_types"]["a__b"] = thing;
    collision(&schema, "/object_types/a_b");

    let mut schema = base();
    schema["action_types"] = json!({
        "add": {"parameters": {"unit_acceptance": {"type": {"struct": {"x": "string"}}, "required": true}}},
        "add_unit_acceptance": {"parameters": {}}
    });
    collision(&schema, "/action_types/add_unit_acceptance");

    let mut schema = base();
    schema["value_types"] = json!({"self": {"base": "integer"}, "self_": {"base": "integer"}});
    collision(&schema, "/value_types/self_");
}

// spec: specs/ont/gen.md#R1
// spec: specs/ont/gen.md#R4
#[test]
fn two_fields_of_one_rust_name_are_refused() {
    let mut schema = base();
    schema["action_types"] = json!({
        "touch": {"parameters": {
            "self": {"type": "string", "required": true},
            "self_": {"type": "string", "required": true}
        }}
    });
    collision(&schema, "/action_types/touch/parameters/self_");

    let mut schema = base();
    schema["functions"] = json!({
        "ask": {
            "parameters": {"shape": {"type": {"array": {"struct": {"super": "string", "super_": "integer"}}}, "required": true}},
            "program": "/bin/cat", "answer_bytes": 10, "timeout_ms": 10
        }
    });
    collision(
        &schema,
        "/functions/ask/parameters/shape/type/array/struct/super_",
    );
}

// spec: specs/ont/gen.md#R1
// spec: specs/ont/gen.md#R5
#[test]
fn two_variants_of_properties_are_refused() {
    let mut schema = base();
    schema["object_types"]["thing"]["properties"]["a_1"] = json!({"type": "string"});
    schema["object_types"]["thing"]["properties"]["a1"] = json!({"type": "string"});
    collision(&schema, "/object_types/thing/properties/a_1");
}

// spec: specs/ont/gen.md#R1
// spec: specs/ont/gen.md#R10
#[test]
fn a_reverse_hop_and_a_link_of_one_rust_name_are_refused() {
    let mut schema = base();
    schema["object_types"]["thing"]["properties"]["next"] =
        json!({"type": {"ref": "thing"}, "required": false});
    schema["object_types"]["thing"]["properties"]["prev"] =
        json!({"type": {"ref": "thing"}, "required": false});
    schema["link_types"] = json!({
        "step": {"from": "thing", "via": "next", "to": "thing", "cardinality": "many_to_one",
                 "names": {"forward": "next", "reverse": "next_of"}},
        "step_reverse": {"from": "thing", "via": "prev", "to": "thing", "cardinality": "many_to_one",
                 "names": {"forward": "prev", "reverse": "prev_of"}}
    });
    collision(&schema, "/link_types/step_reverse");
}

// spec: specs/ont/gen.md#R6
#[test]
fn a_value_enum_of_two_equal_variants_takes_the_struct_form() {
    let text = ont_gen::generate(&fixture("hostile")).unwrap();
    assert!(text.contains("pub struct Loose(pub ::std::string::String);"));
    assert!(text.contains("pub enum Mood {"));
    assert!(text.contains("Self::Self_ => \"self\","));
}

// spec: specs/ont/gen.md#R3
#[test]
fn schema_text_never_becomes_code() {
    let text = ont_gen::generate(&fixture("hostile")).unwrap();
    let first = text.lines().next().unwrap();
    assert!(first.starts_with("// ") && first.contains("ont-gen") && first.contains("type"));
    // One item after the first line: the attributes, the module, its end.
    let top: Vec<&str> = text
        .lines()
        .skip(1)
        .filter(|l| !l.starts_with(' ') && !l.is_empty())
        .collect();
    assert_eq!(top.first(), Some(&"#[allow("));
    assert_eq!(top[top.len() - 2..], ["pub mod r#type {", "}"]);
    assert!(!text.contains('\u{202e}'));
    for line in text.lines() {
        if line.contains("injected") {
            assert!(
                line.trim_start()
                    .starts_with("// Ends \" here \\ and   turns }"),
                "{line}"
            );
        }
    }
    assert!(text.contains(r#"const INSTANCE: &str = "a\"b\\c\u{a}d\u{202e}e";"#));
    for word in [
        "unsafe",
        "std::process",
        "std::fs",
        "std::env",
        "extern crate",
    ] {
        assert!(!text.contains(word), "{word}");
    }
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("#[") && !line.starts_with("#[allow(") {
            assert!(
                line.starts_with("#[derive(") && !line.contains("::"),
                "{line}"
            );
        }
    }
    // Every absolute path starts at `::std` or `::serde_json`.
    for (i, _) in text.match_indices("::") {
        let before = text[..i].chars().next_back().unwrap_or(' ');
        if before.is_ascii_alphanumeric() || before == '_' || before == '>' {
            continue;
        }
        let root = &text[i + 2..];
        assert!(
            root.starts_with("std::") || root.starts_with("serde_json::"),
            "{}",
            &root[..root.len().min(40)]
        );
    }
    let unwraps: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("unwrap") || l.contains("expect("))
        .collect();
    assert!(unwraps.is_empty(), "{unwraps:?}");
}

// spec: specs/ont/gen.md#R4
#[test]
fn names_follow_the_rules() {
    let text = ont_gen::generate(&fixture("hostile")).unwrap();
    for item in [
        "pub struct Self_;",
        "pub enum Self_ {",
        "pub struct Self_ {",
        "pub self_: ::std::string::String,",
        "pub r#type: ::std::option::Option<::std::string::String>,",
        "pub r#gen: super::values::Mood,",
        "pub r#try: ::std::option::Option<::std::vec::Vec<SelfTry>>,",
        "pub struct SelfTry {",
        "pub crate_: ::std::string::String,",
        "pub r#loop: super::values::Loose,",
        "pub struct Super {",
        "pub r#async: ::std::string::String,",
        "Self::Type => \"type\",",
        "object.insert(::std::string::String::from(\"self\"), super::Json::json(&self.self_));",
    ] {
        assert!(text.contains(item), "{item}");
    }
    let text = generate_value(&json!({
        "format": "ontology-schema/v1", "domain": "self", "version": 1,
        "namespace": {"instance": "test", "account": "sample"}
    }))
    .unwrap();
    assert!(text.lines().any(|l| l == "pub mod self_ {"));
}
