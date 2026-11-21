//! Render contracts, using synthetic objects and temporary stores/files only.
#![allow(clippy::unwrap_used)]
use ont_core::{Change, Coverage, Id, Knowledge, Lineage, Namespace, Observation, Scope};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
const FIXTURE: &[u8] = include_bytes!("fixtures/render-board.json");
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ont-render-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p.to_str().unwrap().into()
    }
    fn run(&self, cmd: &[&str], input: Value) -> (i32, Value) {
        let mut args = vec![
            "--store".into(),
            self.0.join("store.sqlite").display().to_string(),
            "--now".into(),
            "1000".into(),
        ];
        args.extend(cmd.iter().map(|s| s.to_string()));
        let out = ont::run(&args, input.to_string().as_bytes(), 0, 0);
        assert!(out.stderr.is_empty(), "{}", out.stderr);
        (out.code, serde_json::from_str(out.stdout.trim()).unwrap())
    }
    fn install(&self, schema: Value) {
        let file = self.file("schema.json", &serde_json::to_vec(&schema).unwrap());
        let (code, ans) = self.run(&["schema", "install", &file], json!({}));
        assert_eq!(code, 0, "{ans}");
    }
    fn setup() -> Self {
        let t = Self::new();
        t.install(serde_json::from_slice(FIXTURE).unwrap());
        t
    }
    fn render(&self, view: &str, args: Value) -> (i32, Value) {
        self.run(&["render"], json!({"op":"render","view":view,"args":args}))
    }
    fn create(&self, title: &str, rank: i64) -> Value {
        let (code, ans) = self.run(
            &["action", "apply"],
            json!({"action_type":"create_item","parameters":{"title":title,"rank":rank}}),
        );
        assert_eq!(code, 0, "{ans}");
        ans
    }
    fn write(&self, file: &str, view: &str) -> (i32, Value) {
        self.run(
            &[
                "render",
                "--write",
                file,
                "--section",
                view,
                "--preimage-dir",
                self.0.to_str().unwrap(),
            ],
            json!({}),
        )
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn schema() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}
fn ns() -> Namespace {
    Namespace {
        instance: "test".into(),
        account: "root".into(),
    }
}
fn id(key: &str) -> Id {
    Id {
        object_type: "item".into(),
        namespace: ns(),
        native_key: key.into(),
    }
}
fn append(
    t: &Temp,
    key: &str,
    fields: BTreeMap<String, Knowledge>,
    change: Change,
    fresh_until: u64,
) {
    let mut store = ont_store::Store::open_schema(&t.0.join("store.sqlite")).unwrap();
    store
        .append_schema(
            &[ont_store::Append {
                id: id(key),
                scope: Scope::domain(ns(), "board", BTreeMap::new()).unwrap(),
                observation: Observation {
                    revision: "r1".into(),
                    source_revision: "s1".into(),
                    observed_at: 10,
                    fresh_until,
                    change,
                    supersedes: None,
                    coverage: Coverage::Complete,
                    fields,
                    lineage: Lineage {
                        artifact_sha256: "a".repeat(64),
                        pointer: "/0".into(),
                    },
                },
            }],
            15,
        )
        .unwrap();
}
fn fields(title: &str, rank: i64) -> BTreeMap<String, Knowledge> {
    BTreeMap::from([
        ("title".into(), Knowledge::Value(json!(title))),
        ("rank".into(), Knowledge::Value(json!(rank))),
    ])
}

fn constrained_schema() -> Value {
    let mut s = schema();
    s["value_types"] = json!({
        "day_text":{"base":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
        "choice_text":{"base":"string","enum":["open","done"]},
        "floor_text":{"base":"string","min":"m"},
        "ceiling_text":{"base":"string","max":"m"}
    });
    for (property, definition) in [
        ("day", json!({"type":{"value_type":"day_text"}})),
        ("short", json!({"type":"string","max_bytes":4})),
        ("choice", json!({"type":{"value_type":"choice_text"}})),
        ("floor", json!({"type":{"value_type":"floor_text"}})),
        ("ceiling", json!({"type":{"value_type":"ceiling_text"}})),
    ] {
        s["object_types"]["item"]["properties"][property] = definition;
    }
    for (name, args, filter) in [
        ("day_eq", json!(["when"]), json!({"eq":["day","$when"]})),
        ("day_ge", json!(["when"]), json!({"ge":["day","$when"]})),
        (
            "day_prefix",
            json!(["when"]),
            json!({"prefix":["day","$when"]}),
        ),
        ("ref_eq", json!(["key"]), json!({"eq":["ref","$key"]})),
        (
            "day_in",
            json!(["a", "z"]),
            json!({"in":["day",["$a","$z"]]}),
        ),
        (
            "two_properties",
            json!(["value"]),
            json!({"all":[
                {"eq":["title","$value"]},{"any":[{"eq":["day","$value"]}]}
            ]}),
        ),
        (
            "two_refusals",
            json!(["value"]),
            json!({"none":[
                {"eq":["short","$value"]},{"eq":["day","$value"]}
            ]}),
        ),
        (
            "argument_order",
            json!(["z", "a"]),
            json!({"any":[
                {"eq":["day","$z"]},{"eq":["short","$a"]}
            ]}),
        ),
    ] {
        let mut view = s["views"]["items"].clone();
        view["args"] = args;
        view["filter"] = filter;
        s["views"][name] = view;
    }
    for property in ["short", "choice", "floor", "ceiling"] {
        let mut view = s["views"]["items"].clone();
        view["args"] = json!(["value"]);
        view["filter"] = json!({"eq":[property,"$value"]});
        s["views"][format!("{property}_eq")] = view;
    }
    s
}

fn argument_refusal(code: i32, answer: &Value, argument: &str, property: &str) {
    assert_eq!(code, 2, "{answer}");
    assert_eq!(answer["code"], "invalid_argument", "{answer}");
    assert_eq!(answer["field"], format!("/args/{argument}"));
    assert_eq!(answer["argument"], argument);
    assert_eq!(answer["property"], property);
    assert_eq!(
        answer["rule"],
        "argument must satisfy the compared property's value constraints"
    );
}

#[test]
fn a1_argument_against_a_pattern_is_refused_for_eq_and_ordering() {
    // spec: specs/ont/render.md#R14
    // spec: specs/ont/render.md#R15
    let t = Temp::new();
    t.install(constrained_schema());
    for (key, day, rank) in [
        ("before", "2040-06-01", 1),
        ("equal", "2040-06-15", 2),
        ("after", "2040-07-01", 3),
    ] {
        let mut f = fields(key, rank);
        f.insert("day".into(), Knowledge::Value(json!(day)));
        append(&t, key, f, Change::Initial, 2000);
    }
    for (view, rows) in [("day_eq", 1), ("day_ge", 2)] {
        for value in ["2040-6-1", "20400102", "2040"] {
            let (code, answer) = t.render(view, json!({"when":value}));
            argument_refusal(code, &answer, "when", "day");
        }
        let (code, answer) = t.render(view, json!({"when":"2040-06-15"}));
        assert_eq!(code, 0, "{answer}");
        assert_eq!(answer["rows"], rows);
        let mut expected = format!(
            "<!-- ont:render {view} when=2040-06-15 schema=board@1 -->\n\
             | Key | Title | Rank |\n| --- | --- | --- |\n| equal | equal | 2 |\n"
        );
        if view == "day_ge" {
            expected.push_str("| after | after | 3 |\n");
        }
        expected.push_str("<!-- /ont:render -->");
        assert_eq!(answer["markdown"], expected);
    }
    // A literal keeps its existing search semantics, including constraint violations.
    let (code, answer) = t.run(
        &["search"],
        json!({"object_type":"item","where":{"ge":["day","2040"]}}),
    );
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["result"]["objects"].as_array().unwrap().len(), 3);
}

#[test]
fn a1_argument_over_max_bytes_outside_allowed_values_or_bounds_is_refused() {
    // spec: specs/ont/render.md#R15
    let t = Temp::new();
    t.install(constrained_schema());
    let mut f = fields("bounds", 1);
    for (property, value) in [
        ("short", "four"),
        ("choice", "open"),
        ("floor", "m"),
        ("ceiling", "m"),
    ] {
        f.insert(property.into(), Knowledge::Value(json!(value)));
    }
    append(&t, "bounds", f, Change::Initial, 2000);
    for (property, invalid, bound) in [
        ("short", "five5", "four"),
        ("choice", "absent", "open"),
        ("floor", "l", "m"),
        ("ceiling", "n", "m"),
    ] {
        let view = format!("{property}_eq");
        let (code, answer) = t.render(&view, json!({"value":invalid}));
        argument_refusal(code, &answer, "value", property);
        let (code, answer) = t.render(&view, json!({"value":bound}));
        assert_eq!(code, 0, "{answer}");
        assert_eq!(answer["rows"], 1);
        assert!(
            answer["markdown"]
                .as_str()
                .unwrap()
                .contains("| bounds | bounds | 1 |")
        );
    }
}

#[test]
fn a1_prefix_and_reference_arguments_are_read_as_before() {
    // spec: specs/ont/render.md#R17
    let t = Temp::new();
    t.install(constrained_schema());
    let key = "/synthetic/item-1";
    let mut f = fields("matched", 1);
    f.insert("day".into(), Knowledge::Value(json!("2040-06-15")));
    f.insert("ref".into(), Knowledge::Value(json!(id(key))));
    append(&t, "match", f, Change::Initial, 2000);
    for (view, args, opening) in [
        ("day_prefix", json!({"when":"2040-06"}), "when=2040-06"),
        ("ref_eq", json!({"key":key}), "key=/synthetic/item-1"),
    ] {
        let (code, answer) = t.render(view, args);
        assert_eq!(code, 0, "{answer}");
        assert_eq!(answer["rows"], 1);
        assert_eq!(
            answer["markdown"],
            format!(
                "<!-- ont:render {view} {opening} schema=board@1 -->\n\
             | Key | Title | Rank |\n| --- | --- | --- |\n\
             | match | matched | 1 |\n<!-- /ont:render -->"
            )
        );
    }
    let (code, answer) = t.render("ref_eq", json!({"key":"bad key"}));
    assert_eq!(code, 2);
    assert_eq!(answer["code"], "reference_argument_unrepresentable");
    assert_eq!(answer["field"], "/args/key");
    // The same partial value is refused for equality, so the exemption is operator-specific.
    let (code, answer) = t.render("day_eq", json!({"when":"2040-06"}));
    argument_refusal(code, &answer, "when", "day");
}

#[test]
fn a1_argument_in_a_list_and_in_two_leaves_is_checked_in_each() {
    // spec: specs/ont/render.md#R14
    // spec: specs/ont/render.md#R20
    let t = Temp::new();
    t.install(constrained_schema());
    let (code, answer) = t.render("day_in", json!({"a":"2040-06-15","z":"2040"}));
    argument_refusal(code, &answer, "z", "day");
    let (code, answer) = t.render("two_properties", json!({"value":"2040"}));
    argument_refusal(code, &answer, "value", "day");
    // Written leaf order wins over the alphabetical property order (day, short).
    let (code, answer) = t.render("two_refusals", json!({"value":"2040-6-1"}));
    argument_refusal(code, &answer, "value", "short");
    // Argument names are alphabetical even when declaration and filter order differ.
    let (code, answer) = t.render("argument_order", json!({"z":"2040","a":"too-long"}));
    argument_refusal(code, &answer, "a", "short");
    let mut f = fields("2040-06-15", 1);
    f.insert("day".into(), Knowledge::Value(json!("2040-06-15")));
    append(&t, "match", f, Change::Initial, 2000);
    for (view, args) in [
        ("day_in", json!({"a":"2040-06-15","z":"2040-06-16"})),
        ("two_properties", json!({"value":"2040-06-15"})),
    ] {
        let (code, answer) = t.render(view, args);
        assert_eq!(code, 0, "{answer}");
        assert_eq!(answer["rows"], 1);
    }
}

#[test]
fn a1_refusal_names_argument_and_property_and_holds_no_value() {
    // spec: specs/ont/render.md#R18
    // spec: specs/ont/render.md#R19
    let t = Temp::new();
    t.install(constrained_schema());
    let marker = "REJECTED_VALUE_MARKER";
    let args = vec![
        "--store".into(),
        t.0.join("store.sqlite").display().to_string(),
        "render".into(),
    ];
    let out = ont::run(
        &args,
        json!({"view":"day_eq","args":{"when":marker}})
            .to_string()
            .as_bytes(),
        0,
        0,
    );
    assert!(!out.stdout.contains(marker));
    assert!(!out.stderr.contains(marker));
    let answer: Value = serde_json::from_str(&out.stdout).unwrap();
    argument_refusal(out.code, &answer, "when", "day");
    let ctx = ont::Context::new(t.0.join("store.sqlite"), 1000);
    let req = ont::render::RenderRequest::parse(
        json!({"view":"day_eq","args":{"when":marker}})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    let Err(ont::Failure::Refused(refusal)) = ont::render::render(&ctx, &req) else {
        panic!("library render must refuse the argument");
    };
    assert_eq!(refusal.code, "invalid_argument");
    assert_eq!(refusal.extra["property"], "day");
    assert!(!format!("{refusal:?}").contains(marker));
    let block = format!(
        "<!-- ont:render day_eq when={marker} schema=board@1 -->\nold\n<!-- /ont:render -->"
    );
    let file = t.file("refused.md", block.as_bytes());
    let (code, answer) = t.run(
        &[
            "render",
            "--write",
            &file,
            "--section",
            "day_eq",
            "--arg",
            &format!("when={marker}"),
            "--preimage-dir",
            t.0.to_str().unwrap(),
        ],
        json!({}),
    );
    argument_refusal(code, &answer, "when", "day");
    assert!(!answer.to_string().contains(marker));
    assert_eq!(std::fs::read_to_string(file).unwrap(), block);
}

#[test]
fn a1_check_reports_a_refused_block_as_drift() {
    // spec: specs/ont/render.md#R23
    let t = Temp::new();
    t.install(constrained_schema());
    let block = "<!-- ont:render day_eq when=2040 schema=board@1 -->\nold\n<!-- /ont:render -->";
    let file = t.file("drift.md", block.as_bytes());
    let (code, answer) = t.run(&["render", "--check", &file], json!({}));
    assert_eq!(code, 2);
    assert_eq!(answer["code"], "render_drift");
    assert_eq!(
        answer["drift"],
        json!([
            {"view":"day_eq","args":{"when":"2040"},"line":1,"refusal_code":"invalid_argument"}
        ])
    );
    assert_eq!(std::fs::read_to_string(file).unwrap(), block);
}

#[test]
fn deterministic_order_ties_arguments_and_actions() {
    // spec: specs/ont/render.md#R2
    // spec: specs/ont/render.md#R5
    // spec: specs/ont/render.md#R24
    let t = Temp::setup();
    append(&t, "z", fields("last", 10), Change::Initial, 2000);
    append(&t, "b", fields("equal", 2), Change::Initial, 2000);
    append(&t, "a", fields("equal", 2), Change::Initial, 2000);
    let (code, a) = t.render("items", json!({}));
    assert_eq!(code, 0, "{a}");
    assert_eq!(a, t.render("items", json!({})).1);
    assert_eq!(
        a["markdown"],
        "<!-- ont:render items schema=board@1 -->\n| Key | Title | Rank |\n| --- | --- | --- |\n| a | equal | 2 |\n| b | equal | 2 |\n| z | last | 10 |\n<!-- /ont:render -->"
    );
    let (_, filtered) = t.render("named_items", json!({"title":"equal"}));
    assert_eq!(filtered["rows"], 2);
    assert!(
        filtered["markdown"]
            .as_str()
            .unwrap()
            .starts_with("<!-- ont:render named_items title=equal schema=board@1 -->")
    );
    for args in [
        json!({}),
        json!({"title":"equal","extra":"x"}),
        json!({"title":"bad value"}),
        json!({"title":""}),
        json!({"title":"<!--"}),
    ] {
        let (code, ans) = t.render("named_items", args);
        assert_eq!((code, ans["code"].as_str()), (2, Some("invalid_argument")));
        assert!(ans["argument"].is_string());
    }
    t.create("created", 1);
    let (_, created) = t.render("items", json!({}));
    assert_eq!(created["rows"], 4);
    let (code, ans) = t.run(
        &["action", "apply"],
        json!({"action_type":"modify_item","parameters":{"item":id("a"),"title":"modified"}}),
    );
    assert_eq!(code, 0, "{ans}");
    assert!(
        t.render("items", json!({})).1["markdown"]
            .as_str()
            .unwrap()
            .contains("| a | modified | 2 |")
    );
    let (code, ans) = t.run(
        &["action", "apply"],
        json!({"action_type":"delete_item","parameters":{"item":id("a")}}),
    );
    assert_eq!(code, 0, "{ans}");
    let (_, after) = t.render("items", json!({}));
    assert_eq!(after["rows"], 3);
    assert!(!after["markdown"].as_str().unwrap().contains("| a |"));
}
#[test]
fn every_cell_form_and_escaping_and_stale() {
    // spec: specs/ont/render.md#R6
    // spec: specs/ont/render.md#R7
    // spec: specs/ont/render.md#R8
    // spec: specs/ont/render.md#R30
    let t = Temp::new();
    let mut s = schema();
    let names = [
        "$key",
        "title",
        "rank",
        "flag",
        "ref",
        "tags",
        "meta",
        "missing",
        "null",
        "blank",
        "unknown",
        "not_performed",
    ];
    s["views"]["items"]["columns"] = Value::Array(
        names
            .iter()
            .map(|p| json!({"header":p,"property":p}))
            .collect(),
    );
    t.install(s);
    let mut f = fields("pipe|\n<!--x-->", 42);
    f.extend([
        ("flag".into(), Knowledge::Value(json!(true))),
        ("ref".into(), Knowledge::Value(json!(id("self")))),
        ("tags".into(), Knowledge::Value(json!(["one", "two"]))),
        ("meta".into(), Knowledge::Value(json!({"x":"v"}))),
        ("missing".into(), Knowledge::Missing),
        ("null".into(), Knowledge::Null),
        ("blank".into(), Knowledge::Blank),
        ("unknown".into(), Knowledge::Unknown),
        ("not_performed".into(), Knowledge::NotPerformed),
    ]);
    append(&t, "self", f, Change::Initial, 20);
    let (code, ans) = t.render("items", json!({}));
    assert_eq!(code, 0, "{ans}");
    assert!(ans["markdown"].as_str().unwrap().contains("| self | pipe\\| <\\!--x--\\> (stale) | 42 (stale) | true (stale) | self (stale) | one, two (stale) | {\"x\":\"v\"} (stale) | (missing) | (null) | (blank) | (unknown) | (not_performed) |"), "{ans}");
}
#[test]
fn exact_truncation_and_pagination() {
    // spec: specs/ont/render.md#R3
    // spec: specs/ont/render.md#R25
    let t = Temp::new();
    let mut s = schema();
    s["views"]["items"]["limit"] = json!(1);
    t.install(s);
    append(&t, "a", fields("first", 1), Change::Initial, 2000);
    append(&t, "b", fields("second", 2), Change::Initial, 2000);
    append(&t, "c", fields("third", 3), Change::Initial, 2000);
    let (_, a) = t.render("items", json!({}));
    assert_eq!(a["rows"], 1);
    assert_eq!(a["truncated"], true);
    assert!(
        a["markdown"]
            .as_str()
            .unwrap()
            .contains("| (+2 more; limit 1) |  |  |\n<!-- /ont:render -->")
    );
}
#[test]
fn write_check_preserves_outside_mode_and_preimage_and_is_idempotent() {
    // spec: specs/ont/render.md#R10
    // spec: specs/ont/render.md#R11
    // spec: specs/ont/render.md#R12
    // spec: specs/ont/render.md#R22
    let t = Temp::setup();
    append(&t, "a", fields("equal", 2), Change::Initial, 2000);
    let original = b"before\r\n\xff\n<!-- ont:render items schema=board@0 -->\nold\n<!-- /ont:render -->\nafter\r\n";
    let file = t.file("board.md", original);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    let (code, ans) = t.write(&file, "items");
    assert_eq!(code, 0, "{ans}");
    assert_eq!(ans["changed"], true);
    assert_eq!(
        std::fs::read(ans["preimage"].as_str().unwrap()).unwrap(),
        original
    );
    assert_eq!(
        std::fs::metadata(ans["preimage"].as_str().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let updated = std::fs::read(&file).unwrap();
    assert!(updated.starts_with(b"before\r\n\xff\n"));
    assert!(updated.ends_with(b"\nafter\r\n"));
    let (code, ans) = t.run(&["render", "--check", &file], json!({}));
    assert_eq!(code, 0, "{ans}");
    assert_eq!(ans["blocks"], 1);
    let entries = std::fs::read_dir(&t.0).unwrap().count();
    let (code, ans) = t.write(&file, "items");
    assert_eq!(code, 0);
    assert_eq!(ans["changed"], false);
    assert_eq!(ans["preimage"], Value::Null);
    assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), entries);
    let edit = String::from_utf8_lossy(&updated).replace("| a | equal", "| a | edited");
    std::fs::write(&file, edit).unwrap();
    let (code, ans) = t.run(&["render", "--check", &file], json!({}));
    assert_eq!((code, ans["code"].as_str()), (2, Some("render_drift")));
    assert_eq!(ans["drift"][0], json!({"view":"items","args":{},"line":3}));
}
#[test]
fn missing_duplicate_unclosed_unknown_symlink_and_preimage_failure() {
    // spec: specs/ont/render.md#R10
    // spec: specs/ont/render.md#R22
    let t = Temp::setup();
    let block = "<!-- ont:render items schema=board@0 -->\nstale\n<!-- /ont:render -->";
    for (text, expected) in [
        ("no blocks".to_string(), "render_block_missing"),
        (format!("{block}\n{block}"), "render_block_ambiguous"),
        (
            "<!-- ont:render items schema=board@0 -->\nunclosed".into(),
            "render_block_missing",
        ),
    ] {
        let file = t.file("board.md", text.as_bytes());
        let (code, ans) = t.write(&file, "items");
        assert_eq!((code, ans["code"].as_str()), (2, Some(expected)), "{ans}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }
    for text in [
        "<!-- ont:render items schema=board@0 -->",
        "<!-- ont:render unknown schema=board@1 -->\n<!-- /ont:render -->",
        "<!-- ont:render items BAD -->\n<!-- /ont:render -->",
    ] {
        let file = t.file("check.md", text.as_bytes());
        let (code, ans) = t.run(&["render", "--check", &file], json!({}));
        assert_eq!((code, ans["code"].as_str()), (2, Some("render_drift")));
        assert_eq!(ans["drift"][0]["line"], 1);
    }
    let file = t.file("board.md", block.as_bytes());
    let link = t.0.join("link.md");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    assert_eq!(
        t.write(link.to_str().unwrap(), "items").1["code"],
        "render_target_invalid"
    );
    let (code, ans) = t.run(
        &[
            "render",
            "--write",
            &file,
            "--section",
            "items",
            "--preimage-dir",
            t.0.join("missing").to_str().unwrap(),
        ],
        json!({}),
    );
    assert_eq!(
        (code, ans["code"].as_str()),
        (2, Some("render_preimage_failed"))
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), block);
}
#[test]
fn forbidden_type_and_old_schema() {
    let t = Temp::new();
    let mut s = schema();
    s["grants"][0]["read"] = json!([]);
    t.install(s);
    assert_eq!(t.render("items", json!({})).1["code"], "unavailable");
    let old = Temp::new();
    let mut s = schema();
    s.as_object_mut().unwrap().remove("views");
    old.install(s);
    assert_eq!(old.render("items", json!({})).1["code"], "unavailable");
    let file = old.file("plain.md", b"no generated blocks\n");
    assert_eq!(
        old.run(&["render", "--check", &file], json!({})),
        (0, json!({"status":"ok","blocks":0,"drift":[]}))
    );
}
#[test]
fn schema_refuses_each_invalid_view_and_duplicate_domain_names() {
    // spec: specs/ont/render.md#R1
    // spec: specs/ont/render.md#R4
    let changes = [
        ("/views/items/extra", json!(true)),
        ("/views/items/object_type", json!("absent")),
        ("/views/items/columns/0/property", json!("absent")),
        ("/views/items/order/0/property", json!("absent")),
        ("/views/items/columns", json!([])),
        ("/views/items/order", json!([])),
        ("/views/items/columns/0/header", json!("bad|header")),
        ("/views/items/columns/0/header", json!("bad\nheader")),
        ("/views/items/columns/0/header", json!("bad\rheader")),
        ("/views/items/limit", json!(0)),
        ("/views/items/limit", json!(1001)),
        (
            "/views/named_items/filter",
            json!({"eq":["title","$undeclared"]}),
        ),
        ("/views/items/filter", json!({"eq":["absent","x"]})),
        ("/views/items/filter", json!({"wrong":["title","x"]})),
        ("/views/items/order/0/direction", json!("up")),
    ];
    for (pointer, value) in changes {
        let t = Temp::new();
        let mut s = schema();
        if pointer.ends_with("/extra") {
            s["views"]["items"]["extra"] = value;
        } else if pointer.ends_with("/filter") {
            let view = if pointer.contains("named_items") {
                "named_items"
            } else {
                "items"
            };
            s["views"][view]["filter"] = value;
        } else {
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            s.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(key.into(), value);
        }
        let file = t.file("invalid.json", &serde_json::to_vec(&s).unwrap());
        let (code, ans) = t.run(&["schema", "check", &file], json!({}));
        assert_eq!(
            (code, ans["code"].as_str()),
            (2, Some("schema_violation")),
            "{pointer}: {ans}"
        );
    }
    let t = Temp::setup();
    let mut s = schema();
    s["domain"] = json!("other");
    // Give the other domain a different type name while retaining the view name.
    let item = s["object_types"]
        .as_object_mut()
        .unwrap()
        .remove("item")
        .unwrap();
    s["object_types"]["other_item"] = item;
    s["object_types"]["other_item"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("ref");
    s["action_types"] = json!({});
    s["grants"] = json!([]);
    for view in ["items", "named_items"] {
        s["views"][view]["object_type"] = json!("other_item");
    }
    let file = t.file("other.json", &serde_json::to_vec(&s).unwrap());
    assert_eq!(
        t.run(&["schema", "check", &file], json!({})).1["code"],
        "schema_violation"
    );
    assert_eq!(t.run(&["schema", "install", &file], json!({})).0, 2);
}

#[test]
fn sorted_arguments_write_only_matching_section_and_check_all_blocks() {
    // spec: specs/ont/render.md#R21
    // spec: specs/ont/render.md#R22
    let t = Temp::new();
    let mut s = schema();
    s["views"]["named_items"]["args"] = json!(["z", "a"]);
    s["views"]["named_items"]["filter"] =
        json!({"any":[{"eq":["title","$z"]},{"eq":["title","$a"]}]});
    t.install(s);
    append(&t, "a", fields("equal", 2), Change::Initial, 2000);
    let (_, answer) = t.render("named_items", json!({"z":"equal","a":"last"}));
    assert!(
        answer["markdown"]
            .as_str()
            .unwrap()
            .starts_with("<!-- ont:render named_items a=last z=equal schema=board@1 -->")
    );
    let old = "before\n<!-- ont:render named_items z=equal a=last schema=board@0 -->\nold\n<!-- /ont:render -->\n";
    let (_, other) = t.render("named_items", json!({"z":"other","a":"other"}));
    let outside = format!("{}\nafter\n", other["markdown"].as_str().unwrap());
    let file = t.file("args.md", format!("{old}{outside}").as_bytes());
    let (code, ans) = t.run(
        &[
            "render",
            "--write",
            &file,
            "--section",
            "named_items",
            "--arg",
            "z=equal",
            "--arg",
            "a=last",
            "--preimage-dir",
            t.0.to_str().unwrap(),
        ],
        json!({}),
    );
    assert_eq!(code, 0, "{ans}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.ends_with(&outside));
    assert_eq!(
        t.run(&["render", "--check", &file], json!({})),
        (0, json!({"status":"ok","blocks":2,"drift":[]}))
    );
    // A named section with different arguments does not match.
    let (code, ans) = t.run(
        &[
            "render",
            "--write",
            &file,
            "--section",
            "named_items",
            "--arg",
            "z=missing",
            "--arg",
            "a=last",
            "--preimage-dir",
            t.0.to_str().unwrap(),
        ],
        json!({}),
    );
    assert_eq!(
        (code, ans["code"].as_str()),
        (2, Some("render_block_missing"))
    );
}

#[test]
fn pagination_uses_one_snapshot_and_concurrent_action_commits_after_read() {
    // spec: specs/ont/render.md#R27
    // spec: specs/ont/render.md#R28
    let t = Temp::new();
    let mut s = schema();
    s["views"]["items"]["limit"] = json!(2);
    t.install(s);
    let mut store = ont_store::Store::open_schema(&t.0.join("store.sqlite")).unwrap();
    let entries: Vec<_> = (0..1002)
        .map(|i| ont_store::Append {
            id: id(&format!("k{i:04}")),
            scope: Scope::domain(ns(), "board", BTreeMap::new()).unwrap(),
            observation: Observation {
                revision: "r1".into(),
                source_revision: "s1".into(),
                observed_at: 10,
                fresh_until: 2000,
                change: Change::Initial,
                supersedes: None,
                coverage: Coverage::Complete,
                fields: fields("page", 1002 - i),
                lineage: Lineage {
                    artifact_sha256: "a".repeat(64),
                    pointer: "/0".into(),
                },
            },
        })
        .collect();
    store.append_schema(&entries, 15).unwrap();
    drop(store);
    let store_path = t.0.join("store.sqlite");
    let writer = std::sync::Arc::new(std::sync::Mutex::new(None));
    let pending = writer.clone();
    ont::render::__set_between_pages_hook(move || {
        let (ready, receiver) = std::sync::mpsc::channel();
        *pending.lock().unwrap() = Some(std::thread::spawn(move || {
            let mut store = ont_store::Store::open_schema_existing_writable(&store_path).unwrap();
            store
                .write(1000, |tx| {
                    let access = tx.head_access(&id("k1001"))?.unwrap();
                    let head = tx.head_after_access(&access)?.unwrap();
                    let mut observation = head.head.observation;
                    observation.supersedes = Some(observation.revision.clone());
                    observation.revision = "r2".into();
                    observation.change = Change::Update;
                    observation.fields = fields("concurrent", 1);
                    tx.append(&ont_store::Append {
                        id: head.id,
                        scope: head.scope,
                        observation,
                    })?;
                    ready.send(()).unwrap();
                    Ok(())
                })
                .unwrap();
        }));
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
    });
    let (code, ans) = t.render("items", json!({}));
    assert_eq!(code, 0, "{ans}");
    writer.lock().unwrap().take().unwrap().join().unwrap();
    assert!(
        t.render("items", json!({})).1["markdown"]
            .as_str()
            .unwrap()
            .contains("concurrent")
    );
    assert!(
        ans["markdown"].as_str().unwrap().contains(
            "| k1001 | page | 1 |\n| k1000 | page | 2 |\n| (+1000 more; limit 2) |  |  |"
        )
    );
}

#[test]
fn schema_view_names_and_nested_unknown_keys_and_filter_types() {
    for (name, value) in [
        ("Bad", json!({})),
        (
            "items",
            json!({"columns":[{"header":"Key","property":"$key","extra":true}]}),
        ),
        (
            "items",
            json!({"order":[{"property":"rank","direction":"asc","extra":true}]}),
        ),
        ("items", json!({"filter":{"eq":["rank","text"]}})),
        ("items", json!({"filter":{"prefix":["rank",1]}})),
        ("items", json!({"args":["x","x"]})),
    ] {
        let t = Temp::new();
        let mut s = schema();
        let mut view = s["views"]["items"].clone();
        for (k, v) in value.as_object().unwrap() {
            view[k] = v.clone();
        }
        s["views"][name] = view;
        let file = t.file("invalid.json", &serde_json::to_vec(&s).unwrap());
        assert_eq!(
            t.run(&["schema", "check", &file], json!({})).1["code"],
            "schema_violation",
            "{s}"
        );
    }
}

#[test]
fn descending_order_reverses_key_scan_and_breaks_ties_by_key() {
    // spec: specs/ont/render.md#R5
    // spec: specs/ont/render.md#R50
    let t = Temp::new();
    let mut s = schema();
    s["views"]["items"]["order"][0]["direction"] = json!("desc");
    t.install(s);
    append(&t, "a", fields("low", 1), Change::Initial, 2000);
    append(&t, "z", fields("high", 9), Change::Initial, 2000);
    append(&t, "y", fields("high", 9), Change::Initial, 2000);
    let (_, answer) = t.render("items", json!({}));
    assert_eq!(
        answer["markdown"],
        "<!-- ont:render items schema=board@1 -->\n| Key | Title | Rank |\n| --- | --- | --- |\n| y | high | 9 |\n| z | high | 9 |\n| a | low | 1 |\n<!-- /ont:render -->"
    );
}

#[test]
fn check_distinguishes_refusal_from_byte_drift() {
    // spec: specs/ont/render.md#R41
    let t = Temp::setup();
    let (_, rendered) = t.render("items", json!({}));
    let good = rendered["markdown"].as_str().unwrap();
    let text = format!(
        "{}\n<!-- ont:render absent schema=board@1 -->\n<!-- /ont:render -->",
        good.replace("schema=board@1", "schema=board@0")
    );
    let file = t.file("check.md", text.as_bytes());
    let (code, answer) = t.run(&["render", "--check", &file], json!({}));
    assert_eq!(code, 2);
    assert_eq!(answer["code"], "render_drift");
    assert_eq!(answer["drift"][0]["view"], "items");
    assert!(answer["drift"][0].get("refusal_code").is_none());
    assert_eq!(answer["drift"][1]["refusal_code"], "unavailable");
    // A missing store is an operational refusal, with its own cause retained.
    let empty = Temp::new();
    let (_, answer) = empty.run(&["render", "--check", &file], json!({}));
    assert_eq!(answer["drift"][0]["refusal_code"], "store_not_found");
}

#[test]
fn fenced_markers_and_readme_examples_are_ignored() {
    // spec: specs/ont/render.md#R38
    let t = Temp::setup();
    for fence in ["```markdown", "~~~markdown"] {
        let text = format!(
            "{fence}\n<!-- ont:render items schema=board@0 -->\nexample\n<!-- /ont:render -->\n{}\n",
            &fence[..3]
        );
        let file = t.file("fenced.md", text.as_bytes());
        assert_eq!(
            t.run(&["render", "--check", &file], json!({})),
            (0, json!({"status":"ok","blocks":0,"drift":[]}))
        );
        assert_eq!(t.write(&file, "items").1["code"], "render_block_missing");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }
    for (name, bytes) in [
        (
            "action.md",
            include_bytes!("../../../specs/ont/action.md").as_slice(),
        ),
        (
            "schema-language.md",
            include_bytes!("../../../specs/ont/schema-language.md").as_slice(),
        ),
        (
            "log.md",
            include_bytes!("../../../specs/ont/log.md").as_slice(),
        ),
        (
            "store.md",
            include_bytes!("../../../specs/ont/store.md").as_slice(),
        ),
        (
            "read.md",
            include_bytes!("../../../specs/ont/read.md").as_slice(),
        ),
        (
            "index.md",
            include_bytes!("../../../specs/ont/index.md").as_slice(),
        ),
        (
            "mcp.md",
            include_bytes!("../../../specs/ont/mcp.md").as_slice(),
        ),
        (
            "render.md",
            include_bytes!("../../../specs/ont/render.md").as_slice(),
        ),
        (
            "cli.md",
            include_bytes!("../../../specs/ont/cli.md").as_slice(),
        ),
    ] {
        let file = t.file(name, bytes);
        assert_eq!(
            t.run(&["render", "--check", &file], json!({})),
            (0, json!({"status":"ok","blocks":0,"drift":[]}))
        );
    }
}

#[test]
fn writer_lock_wait_is_bounded_and_hardlinks_are_refused() {
    // spec: specs/ont/render.md#R31
    // spec: specs/ont/render.md#R32
    // spec: specs/ont/render.md#R36
    let t = Temp::setup();
    let original = b"<!-- ont:render items schema=board@0 -->\nold\n<!-- /ont:render -->";
    let file = t.file("write.md", original);
    let lock_path = format!("{file}.ont-render.lock");
    let lock = std::fs::File::create(&lock_path).unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let start = std::time::Instant::now();
    let (code, answer) = t.write(&file, "items");
    assert_eq!((code, answer["code"].as_str()), (2, Some("render_busy")));
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(std::fs::read(&file).unwrap(), original);
    drop(lock);
    assert_eq!(t.write(&file, "items").0, 0);
    let linked = t.0.join("alias.md");
    std::fs::hard_link(&file, &linked).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(t.write(&file, "items").1["code"], "render_target_invalid");
    assert_eq!(
        t.run(&["render", "--check", &file], json!({})).1["code"],
        "render_target_invalid"
    );
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert_eq!(std::fs::read(linked).unwrap(), bytes);
}

#[test]
fn cells_escape_backslashes_pipes_comments_and_crlf_and_headers() {
    // spec: specs/ont/render.md#R8
    // spec: specs/ont/render.md#R40
    let t = Temp::new();
    let mut s = schema();
    s["views"]["items"]["columns"][1]["header"] = json!("<!--Title-->");
    t.install(s);
    append(
        &t,
        "a",
        fields("a\\|b\r\n<!--x-->", 1),
        Change::Initial,
        2000,
    );
    let (code, answer) = t.render("items", json!({}));
    assert_eq!(code, 0);
    let markdown = answer["markdown"].as_str().unwrap();
    assert!(markdown.contains(r"| Key | <\!--Title--\> | Rank |"));
    assert!(
        markdown.contains(r"| a | a\\\|b <\!--x--\> | 1 |"),
        "{markdown}"
    );
    assert_eq!(markdown.lines().count(), 5);
}

#[test]
fn conflicting_preimage_crlf_and_eof_without_newline() {
    use sha2::{Digest, Sha256};
    let t = Temp::setup();
    let block = "<!-- ont:render items schema=board@0 -->\nold\n<!-- /ont:render -->";
    let file = t.file("collision.md", block.as_bytes());
    let hash = format!("{:x}", Sha256::digest(block.as_bytes()));
    let preimage = t.file(&format!("collision.md.{}", &hash[..12]), b"other bytes");
    std::fs::set_permissions(&preimage, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(t.write(&file, "items").1["code"], "render_preimage_failed");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), block);
    assert_eq!(std::fs::read(&preimage).unwrap(), b"other bytes");
    let eof = t.file("eof.md", block.as_bytes());
    assert_eq!(t.write(&eof, "items").0, 0);
    let (_, rendered) = t.render("items", json!({}));
    assert_eq!(
        std::fs::read_to_string(&eof).unwrap(),
        rendered["markdown"].as_str().unwrap()
    );
    assert!(!std::fs::read(&eof).unwrap().ends_with(b"\n"));
    assert_eq!(t.run(&["render", "--check", &eof], json!({})).0, 0);
    let text = format!("before\r\n{}\r\nafter\r\n", block.replace('\n', "\r\n"));
    let crlf = t.file("crlf.md", text.as_bytes());
    assert_eq!(t.write(&crlf, "items").0, 0);
    let after = std::fs::read(&crlf).unwrap();
    assert!(after.starts_with(b"before\r\n"));
    assert!(after.ends_with(b"\r\nafter\r\n"));
    assert_eq!(t.run(&["render", "--check", &crlf], json!({})).0, 0);
}

#[test]
fn schema_argument_type_refusals_name_the_filter_pointer() {
    // spec: specs/ont/render.md#R42
    for property in ["rank", "flag", "tags", "meta"] {
        for filter in [
            json!({"eq":[property,"$title"]}),
            json!({"in":[property,["$title"]]}),
        ] {
            let t = Temp::new();
            let mut s = schema();
            s["views"]["named_items"]["filter"] = filter;
            let file = t.file("invalid.json", &serde_json::to_vec(&s).unwrap());
            let (code, answer) = t.run(&["schema", "check", &file], json!({}));
            assert_eq!(
                (code, answer["code"].as_str()),
                (2, Some("schema_violation"))
            );
            assert!(
                answer["violations"].as_array().unwrap().contains(
                    &json!({"code":"invalid_schema","pointer":"/views/named_items/filter"})
                ),
                "{answer}"
            );
        }
    }
    let t = Temp::new();
    let mut s = schema();
    s["value_types"] = json!({"label":{"base":"string"}});
    s["object_types"]["item"]["properties"]["title"]["type"] = json!({"value_type":"label"});
    s["views"]["named_items"]["filter"] = json!({"in":["title",["$title"]]});
    t.install(s);
    assert_eq!(t.render("named_items", json!({"title":"value"})).0, 0);
}

const FENCE_EXAMPLE_BLOCK: &str =
    "<!-- ont:render items schema=board@0 -->\nexample\n<!-- /ont:render -->";

#[test]
fn unclosed_fence_above_real_block_refuses_check_and_write() {
    // spec: specs/ont/render.md#R39
    let t = Temp::setup();
    for opener in ["```", "   ~~~~text"] {
        let text = format!("intro\n{opener}\n{FENCE_EXAMPLE_BLOCK}\n");
        let file = t.file("unclosed.md", text.as_bytes());
        let (code, answer) = t.run(&["render", "--check", &file], json!({}));
        assert_eq!((code, answer["code"].as_str()), (2, Some("render_drift")));
        assert_eq!(answer["drift"][0]["refusal_code"], "render_fence_unclosed");
        assert_eq!(answer["drift"][0]["line"], 2);
        let (code, answer) = t.write(&file, "items");
        assert_eq!(
            (code, answer["code"].as_str()),
            (2, Some("render_fence_unclosed"))
        );
        assert_eq!(answer["line"], 2);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }
}

#[test]
fn four_backtick_fence_keeps_three_backtick_line_as_content() {
    // spec: specs/ont/render.md#R37
    let t = Temp::setup();
    let text = format!("````markdown\n```\n{FENCE_EXAMPLE_BLOCK}\n````\n");
    let file = t.file("four.md", text.as_bytes());
    assert_eq!(
        t.run(&["render", "--check", &file], json!({})),
        (0, json!({"status":"ok","blocks":0,"drift":[]}))
    );
    assert_eq!(t.write(&file, "items").1["code"], "render_block_missing");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn tilde_line_inside_backtick_fence_does_not_close_it() {
    // spec: specs/ont/render.md#R37
    let t = Temp::setup();
    let text = format!("```markdown\n~~~\n{FENCE_EXAMPLE_BLOCK}\n```\n");
    let file = t.file("mixed.md", text.as_bytes());
    assert_eq!(
        t.run(&["render", "--check", &file], json!({})),
        (0, json!({"status":"ok","blocks":0,"drift":[]}))
    );
    assert_eq!(t.write(&file, "items").1["code"], "render_block_missing");
}

#[test]
fn indented_closing_fence_accepts_up_to_three_spaces() {
    // spec: specs/ont/render.md#R37
    let t = Temp::setup();
    for indent in ["", " ", "  ", "   "] {
        let text = format!("   ~~~markdown\n{FENCE_EXAMPLE_BLOCK}\n{indent}~~~~  \n");
        let file = t.file("indented.md", text.as_bytes());
        assert_eq!(
            t.run(&["render", "--check", &file], json!({})),
            (0, json!({"status":"ok","blocks":0,"drift":[]}))
        );
        assert_eq!(t.write(&file, "items").1["code"], "render_block_missing");
    }
    // Four-space indentation and non-space suffixes cannot close the fence.
    for closer in ["    ~~~", "~~~text", "~~~\t"] {
        let file = t.file("not-closed.md", format!("~~~\n{closer}\n").as_bytes());
        let (_, answer) = t.run(&["render", "--check", &file], json!({}));
        assert_eq!(answer["drift"][0]["refusal_code"], "render_fence_unclosed");
    }
}

#[test]
fn real_block_after_correctly_closed_fence_is_checked_and_written() {
    let t = Temp::setup();
    let prefix = format!("````markdown\n```\n~~~\n{FENCE_EXAMPLE_BLOCK}\n   ````` \n");
    let file = t.file(
        "after-fence.md",
        format!("{prefix}{FENCE_EXAMPLE_BLOCK}").as_bytes(),
    );
    let (code, answer) = t.run(&["render", "--check", &file], json!({}));
    assert_eq!(code, 2);
    assert_eq!(answer["drift"].as_array().unwrap().len(), 1);
    assert_eq!(answer["drift"][0]["view"], "items");
    assert_eq!(answer["drift"][0]["line"], 8);
    assert!(answer["drift"][0].get("refusal_code").is_none());
    assert_eq!(t.write(&file, "items").0, 0);
    assert!(std::fs::read_to_string(&file).unwrap().starts_with(&prefix));
    assert_eq!(
        t.run(&["render", "--check", &file], json!({})),
        (0, json!({"status":"ok","blocks":1,"drift":[]}))
    );
}

#[test]
fn reference_view_argument_native_key_without_mirror_and_unsafe_key_refusal() {
    // spec: specs/ont/render.md#R46
    // spec: specs/ont/render.md#R13
    // spec: specs/ont/render.md#R43
    // spec: specs/ont/render.md#R44
    // spec: specs/ont/render.md#R45
    // spec: specs/ont/render.md#R47
    let t = Temp::new();
    let mut s = schema();
    s["views"]["named_items"]["filter"] = json!({"all":[{"eq":["ref","$title"]}]});
    t.install(s);
    let key = "/synthetic/task-1";
    let mut matching = fields("matched", 1);
    matching.insert("ref".into(), Knowledge::Value(json!(id(key))));
    append(&t, "match", matching, Change::Initial, 2000);
    let mut other = fields("other", 2);
    other.insert("ref".into(), Knowledge::Value(json!(id("other-task"))));
    append(&t, "other", other, Change::Initial, 2000);
    let (code, answer) = t.render("named_items", json!({"title":key}));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["rows"], 1);
    let markdown = answer["markdown"].as_str().unwrap();
    assert!(
        markdown
            .starts_with("<!-- ont:render named_items title=/synthetic/task-1 schema=board@1 -->")
    );
    assert!(markdown.contains("| match | matched | 1 |"));
    assert!(!markdown.contains("| other |"));
    for key in ["bad key", "bad=key", "é", "", "bad\nkey"] {
        let (code, answer) = t.render("named_items", json!({"title":key}));
        assert_eq!(code, 2, "{answer}");
        assert_eq!(answer["code"], "reference_argument_unrepresentable");
        assert_eq!(answer["field"], "/args/title");
        assert_eq!(
            answer["rule"],
            "nonempty ASCII letters, digits, or . _ : / -"
        );
    }
    // The bounded addition is eq on a single reference, not in/ne/prefix,
    // reference arrays, nested placeholders or ordered reference comparisons.
    for filter in [
        json!({"in":["ref",["$title"]]}),
        json!({"ne":["ref","$title"]}),
        json!({"prefix":["ref","$title"]}),
        json!({"lt":["ref","$title"]}),
        json!({"eq":["ref",{"native_key":"$title"}]}),
    ] {
        let mut bad = schema();
        bad["views"]["named_items"]["filter"] = filter;
        let file = t.file("bad-ref.json", &serde_json::to_vec(&bad).unwrap());
        let (code, answer) = t.run(&["schema", "check", &file], json!({}));
        assert_eq!(code, 2, "{answer}");
        assert_eq!(answer["code"], "schema_violation");
        assert!(
            answer["violations"]
                .as_array()
                .unwrap()
                .contains(&json!({"code":"invalid_schema","pointer":"/views/named_items/filter"})),
            "{answer}"
        );
    }
}

#[test]
fn expect_blocks_detects_balanced_fence_hiding_a_real_block() {
    // spec: specs/ont/cli.md#R13
    let t = Temp::setup();
    let (_, rendered) = t.render("items", json!({}));
    let block = rendered["markdown"].as_str().unwrap();
    // The inserted fence consumes the real block and closes at the bash
    // example's closing fence; this is syntactically balanced Markdown.
    let hidden = format!("```\n{block}\n```bash\nprintf example\n```\n");
    let file = t.file("balanced.md", hidden.as_bytes());
    assert_eq!(
        t.run(&["render", "--check", &file], json!({})),
        (0, json!({"status":"ok","blocks":0,"drift":[]}))
    );
    let (code, answer) = t.run(
        &["render", "--check", &file, "--expect-blocks", "1"],
        json!({}),
    );
    assert_eq!(
        (code, answer["code"].as_str()),
        (2, Some("render_block_count"))
    );
    assert_eq!(answer["expected"], 1);
    assert_eq!(answer["found"], 0);
    assert_eq!(
        t.run(
            &["render", "--check", &file, "--expect-blocks", "0"],
            json!({})
        )
        .0,
        0
    );
    let real = t.file("real-count.md", block.as_bytes());
    assert_eq!(
        t.run(
            &["render", "--check", &real, "--expect-blocks", "1"],
            json!({})
        )
        .0,
        0
    );
    let (code, answer) = t.run(
        &["render", "--check", &real, "--expect-blocks", "2"],
        json!({}),
    );
    assert_eq!(
        (code, answer["expected"].as_u64(), answer["found"].as_u64()),
        (2, Some(2), Some(1))
    );
    for n in ["-1", "1.5", "18446744073709551616"] {
        let (code, answer) = t.run(
            &["render", "--check", &real, "--expect-blocks", n],
            json!({}),
        );
        assert_eq!(code, 2);
        assert_eq!(answer["field"], "--expect-blocks");
        assert!(answer["rule"].is_string());
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), hidden);
}

#[test]
fn expect_blocks_matching_count_still_detects_content_drift() {
    let t = Temp::setup();
    let (_, rendered) = t.render("items", json!({}));
    let drifted = rendered["markdown"].as_str().unwrap().replace(
        "<!-- /ont:render -->",
        "changed content\n<!-- /ont:render -->",
    );
    let file = t.file("drift-count.md", drifted.as_bytes());
    let (code, answer) = t.run(
        &["render", "--check", &file, "--expect-blocks", "1"],
        json!({}),
    );
    assert_eq!((code, answer["code"].as_str()), (2, Some("render_drift")));
    assert_eq!(answer["drift"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(file).unwrap(), drifted);
}

#[test]
fn expect_blocks_refuses_write_and_section_combinations_without_writing() {
    let t = Temp::setup();
    let original = b"<!-- ont:render items schema=board@0 -->\nold\n<!-- /ont:render -->";
    let file = t.file("write-count.md", original);
    for command in [
        vec![
            "render",
            "--write",
            &file,
            "--section",
            "items",
            "--preimage-dir",
            t.0.to_str().unwrap(),
            "--expect-blocks",
            "1",
        ],
        vec!["render", "--section", "items", "--expect-blocks", "1"],
        vec![
            "render",
            "--check",
            &file,
            "--section",
            "items",
            "--expect-blocks",
            "1",
        ],
    ] {
        let (code, answer) = t.run(&command, json!({}));
        assert_eq!(
            (code, answer["code"].as_str()),
            (2, Some("invalid_argument"))
        );
        assert_eq!(std::fs::read(&file).unwrap(), original);
    }
}

#[test]
fn describe_discovers_readable_views_and_arguments_on_cli_and_mcp() {
    // spec: specs/ont/store.md#R2
    let t = Temp::new();
    let mut schema = schema();
    schema["object_types"]["hidden"] = json!({"owner":"ontology","primary_key":{"from":"$action_id"},"properties":{"title":{"type":"string"}}});
    schema["views"]["hidden_view"] = json!({"object_type":"hidden","order":[{"property":"title","direction":"asc"}],"columns":[{"header":"Title","property":"title"}]});
    t.install(schema);
    let expected = json!([
        {"domain":"board","name":"items","args":[],"object_type":"item"},
        {"domain":"board","name":"named_items","args":["title"],"object_type":"item"}
    ]);
    for input in [
        json!({}),
        json!({"domain":"board"}),
        json!({"object_type":"item"}),
    ] {
        let (code, answer) = t.run(&["describe"], input.clone());
        assert_eq!(code, 0, "{answer}");
        assert_eq!(answer["result"]["views"], expected);
        let config = ont::mcp::Config {
            store: t.0.join("store.sqlite"),
            now: Some(1000),
            client: "test".into(),
            read_only: false,
        };
        let mut server = ont::mcp::Server::new(config, &ont::cli::execute);
        let response = server.handle_line(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ont_describe","arguments":input}}).to_string().as_bytes()).unwrap();
        assert_eq!(response["result"]["isError"], false, "{response}");
        let body: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(body["result"]["views"], expected);
    }
    assert_eq!(
        t.run(&["describe"], json!({"object_type":"hidden"})).1["result"]["views"],
        json!([])
    );
    let (_, help) = t.run(&["help"], json!({}));
    let describe = help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "describe")
        .unwrap();
    assert_eq!(
        describe["fields"],
        "op?: describe|null; domain?: string|null; object_type?: string|null"
    );
    assert!(
        describe["notes"]
            .as_str()
            .unwrap()
            .contains("lists views and args")
    );
}

// Documents: one generated block over several document blocks.
fn note_id(key: &str) -> Id {
    Id {
        object_type: "note".into(),
        namespace: ns(),
        native_key: key.into(),
    }
}
fn observation(fields: BTreeMap<String, Knowledge>, fresh_until: u64) -> Observation {
    Observation {
        revision: "r1".into(),
        source_revision: "s1".into(),
        observed_at: 10,
        fresh_until,
        change: Change::Initial,
        supersedes: None,
        coverage: Coverage::Complete,
        fields,
        lineage: Lineage {
            artifact_sha256: "a".repeat(64),
            pointer: "/0".into(),
        },
    }
}
fn append_all(t: &Temp, entries: Vec<(Id, Observation)>) {
    let mut store = ont_store::Store::open_schema(&t.0.join("store.sqlite")).unwrap();
    let entries: Vec<_> = entries
        .into_iter()
        .map(|(id, observation)| ont_store::Append {
            id,
            scope: Scope::domain(ns(), "board", BTreeMap::new()).unwrap(),
            observation,
        })
        .collect();
    store.append_schema(&entries, 15).unwrap();
}
fn note(t: &Temp, key: &str, text: Knowledge, fresh_until: u64) {
    append_all(
        t,
        vec![(
            note_id(key),
            observation(BTreeMap::from([("text".into(), text)]), fresh_until),
        )],
    );
}
fn text(s: &str) -> Knowledge {
    Knowledge::Value(json!(s))
}
/// The fixture plus a second object type `note`, readable by the principal,
/// and the document `overview` of specs/ont/render.md R53.
fn document_schema() -> Value {
    let mut s = schema();
    s["object_types"]["note"] = json!({"owner":"ontology","primary_key":{"from":"$action_id"},
        "properties":{"text":{"type":"string"},"short":{"type":"string","max_bytes":4},
            "item":{"type":{"ref":"item"}}}});
    s["grants"][0]["read"] = json!(["item", "note"]);
    s["views"]["overview"] = json!({"args":["title"],"blocks":[
        {"heading":"Items","table":{"object_type":"item","filter":{"eq":["title","$title"]},
            "order":[{"property":"rank","direction":"asc"}],
            "columns":[{"header":"Key","property":"$key"},{"header":"Rank","property":"rank"}]}},
        {"heading":"Notes","list":{"object_type":"note",
            "order":[{"property":"text","direction":"asc"}],"properties":["$key","text"]}}]});
    s
}
const OVERVIEW: &str = "<!-- ont:render overview title=equal schema=board@1 -->\n## Items\n| Key | Rank |\n| --- | --- |\n| a | 2 |\n| b | 2 |\n\n## Notes\n- n1: first\n- n2: second\n<!-- /ont:render -->";
fn overview_store() -> Temp {
    let t = Temp::new();
    t.install(document_schema());
    append(&t, "z", fields("last", 10), Change::Initial, 2000);
    append(&t, "b", fields("equal", 2), Change::Initial, 2000);
    append(&t, "a", fields("equal", 2), Change::Initial, 2000);
    note(&t, "n2", text("second"), 2000);
    note(&t, "n1", text("first"), 2000);
    t
}
/// Every line between the markers is empty or opens with `## `, `| ` or `- `.
fn lines_are_inert(markdown: &str) {
    let lines: Vec<_> = markdown.split('\n').collect();
    assert!(lines[0].starts_with("<!-- ont:render "));
    assert_eq!(lines[lines.len() - 1], "<!-- /ont:render -->");
    for line in &lines[1..lines.len() - 1] {
        assert!(
            line.is_empty() || ["## ", "| ", "- "].iter().any(|p| line.starts_with(p)),
            "{line}"
        );
    }
}

#[test]
fn document_over_two_types_renders_the_same_bytes_twice() {
    // spec: specs/ont/render.md#R1
    // spec: specs/ont/render.md#R51
    // spec: specs/ont/render.md#R53
    let t = overview_store();
    let (code, answer) = t.render("overview", json!({"title":"equal"}));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer,
        json!({"status":"ok","view":"overview","schema":"board@1","rows":4,
            "truncated":false,"markdown":OVERVIEW})
    );
    lines_are_inert(OVERVIEW);
    assert_eq!(t.render("overview", json!({"title":"equal"})), (0, answer));
    // The table views of the same schema keep the bytes of a schema without documents.
    assert_eq!(
        t.render("items", json!({})).1["markdown"],
        "<!-- ont:render items schema=board@1 -->\n| Key | Title | Rank |\n| --- | --- | --- |\n| a | equal | 2 |\n| b | equal | 2 |\n| z | last | 10 |\n<!-- /ont:render -->"
    );
    assert_eq!(
        t.render("named_items", json!({"title":"last"})).1,
        json!({"status":"ok","view":"named_items","schema":"board@1","rows":1,"truncated":false,
            "markdown":"<!-- ont:render named_items title=last schema=board@1 -->\n| Key | Title | Rank |\n| --- | --- | --- |\n| z | last | 10 |\n<!-- /ont:render -->"})
    );
    // describe lists the table views and not the document.
    let (code, answer) = t.run(&["describe"], json!({}));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["result"]["views"],
        json!([
            {"domain":"board","name":"items","args":[],"object_type":"item"},
            {"domain":"board","name":"named_items","args":["title"],"object_type":"item"}
        ])
    );
    // A document name is unique among the views of every installed domain.
    let mut other = document_schema();
    other["domain"] = json!("other");
    other["object_types"] = json!({"other_note":other["object_types"]["note"].clone()});
    other["object_types"]["other_note"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("item");
    other["action_types"] = json!({});
    other["grants"] = json!([]);
    other["views"] = json!({"overview":{"blocks":[{"heading":"Notes","list":{"object_type":"other_note",
        "order":[{"property":"text","direction":"asc"}],"properties":["text"]}}]}});
    let file = t.file("other.json", &serde_json::to_vec(&other).unwrap());
    let (code, answer) = t.run(&["schema", "install", &file], json!({}));
    assert_eq!(
        (code, answer["code"].as_str()),
        (2, Some("schema_violation"))
    );
    let (code, answer) = t.run(&["schema", "check", &file], json!({}));
    assert_eq!(code, 2, "{answer}");
    assert!(
        answer["violations"]
            .as_array()
            .unwrap()
            .contains(&json!({"code":"duplicate_type_name","pointer":"/views/overview"})),
        "{answer}"
    );
}

fn document_refusal(s: &Value, code: &str, pointer: &str) {
    let t = Temp::new();
    let file = t.file("invalid.json", &serde_json::to_vec(s).unwrap());
    let (exit, answer) = t.run(&["schema", "check", &file], json!({}));
    assert_eq!(
        (exit, answer["code"].as_str()),
        (2, Some("schema_violation")),
        "{pointer}: {answer}"
    );
    assert!(
        answer["violations"]
            .as_array()
            .unwrap()
            .contains(&json!({"code":code,"pointer":pointer})),
        "{code}@{pointer}: {answer}"
    );
}

#[test]
fn document_schema_refusals_name_code_and_pointer() {
    // spec: specs/ont/render.md#R52
    let base = document_schema();
    let t = Temp::new();
    let file = t.file("valid.json", &serde_json::to_vec(&base).unwrap());
    assert_eq!(t.run(&["schema", "check", &file], json!({})).0, 0);
    let doc = "/views/overview";
    let table = "/views/overview/blocks/0/table";
    let list = "/views/overview/blocks/1/list";
    let block = base["views"]["overview"]["blocks"][1].clone();
    let cases: Vec<(&str, Value, &str, String)> = vec![
        // (a) 1 to 20 document blocks.
        (
            "/blocks",
            json!([]),
            "invalid_document",
            format!("{doc}/blocks"),
        ),
        (
            "/blocks",
            Value::Array(vec![block; 21]),
            "invalid_document",
            format!("{doc}/blocks"),
        ),
        // (b) headings.
        (
            "/blocks/0/heading",
            json!(""),
            "invalid_document",
            format!("{doc}/blocks/0/heading"),
        ),
        (
            "/blocks/0/heading",
            json!("h".repeat(201)),
            "invalid_document",
            format!("{doc}/blocks/0/heading"),
        ),
        (
            "/blocks/0/heading",
            json!("a\nb"),
            "invalid_document",
            format!("{doc}/blocks/0/heading"),
        ),
        (
            "/blocks/0/heading",
            json!("a\rb"),
            "invalid_document",
            format!("{doc}/blocks/0/heading"),
        ),
        (
            "/blocks/0/heading",
            json!("a\tb"),
            "invalid_document",
            format!("{doc}/blocks/0/heading"),
        ),
        // (c) list properties.
        (
            "/blocks/1/list/properties",
            json!([]),
            "invalid_document",
            format!("{list}/properties"),
        ),
        (
            "/blocks/1/list/properties",
            json!(["text", "text", "text", "text", "text"]),
            "invalid_document",
            format!("{list}/properties"),
        ),
        (
            "/blocks/1/list/properties",
            json!(["absent"]),
            "unknown_property",
            format!("{list}/properties"),
        ),
        // (d) the document's arguments.
        (
            "/args",
            json!(["Bad"]),
            "invalid_schema",
            format!("{doc}/args"),
        ),
        (
            "/args",
            json!(["title", "title"]),
            "invalid_schema",
            format!("{doc}/args"),
        ),
        // (e) the checks of a table view, under the document block's pointer.
        (
            "/blocks/0/table/object_type",
            json!("absent"),
            "unknown_type",
            format!("{table}/object_type"),
        ),
        (
            "/blocks/1/list/object_type",
            json!("absent"),
            "unknown_type",
            format!("{list}/object_type"),
        ),
        (
            "/blocks/0/table/limit",
            json!(0),
            "invalid_schema",
            format!("{table}/limit"),
        ),
        (
            "/blocks/1/list/limit",
            json!(1001),
            "invalid_schema",
            format!("{list}/limit"),
        ),
        (
            "/blocks/0/table/order",
            json!([]),
            "invalid_schema",
            format!("{table}/order"),
        ),
        (
            "/blocks/1/list/order",
            json!([]),
            "invalid_schema",
            format!("{list}/order"),
        ),
        (
            "/blocks/0/table/columns",
            json!([]),
            "invalid_schema",
            format!("{table}/columns"),
        ),
        (
            "/blocks/0/table/columns",
            json!([{"header":"a|b","property":"rank"}]),
            "invalid_schema",
            format!("{table}/columns"),
        ),
        (
            "/blocks/0/table/columns",
            json!([{"header":"A","property":"absent"}]),
            "unknown_property",
            format!("{table}/columns"),
        ),
        (
            "/blocks/0/table/order",
            json!([{"property":"absent","direction":"asc"}]),
            "unknown_property",
            format!("{table}/order"),
        ),
        (
            "/blocks/1/list/order",
            json!([{"property":"absent","direction":"asc"}]),
            "unknown_property",
            format!("{list}/order"),
        ),
        (
            "/blocks/0/table/filter",
            json!({"eq":["title","$undeclared"]}),
            "invalid_schema",
            format!("{table}/filter"),
        ),
        (
            "/blocks/1/list/filter",
            json!({"eq":["text","$undeclared"]}),
            "invalid_schema",
            format!("{list}/filter"),
        ),
        (
            "/blocks/1/list/filter",
            json!({"eq":["absent","x"]}),
            "invalid_schema",
            format!("{list}/filter"),
        ),
        (
            "/blocks/1/list/filter",
            json!({"eq":["text",1]}),
            "invalid_schema",
            format!("{list}/filter"),
        ),
    ];
    for (at, value, code, pointer) in cases {
        let mut s = base.clone();
        let (parent, key) = format!("{doc}{at}")
            .rsplit_once('/')
            .map(|(p, k)| (p.to_string(), k.to_string()))
            .unwrap();
        s.pointer_mut(&parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key, value);
        document_refusal(&s, code, &pointer);
    }
    // (f) shapes no form names: exit 2 and schema_violation from check and install.
    let mut shapes = Vec::new();
    for (member, value) in [
        ("object_type", json!("item")),
        ("filter", json!({"eq":["title","x"]})),
        ("order", json!([{"property":"rank","direction":"asc"}])),
        ("columns", json!([{"header":"Key","property":"$key"}])),
        ("limit", json!(5)),
        ("extra", json!(true)),
    ] {
        let mut s = base.clone();
        s["views"]["overview"][member] = value;
        shapes.push(s);
    }
    let mut both = base.clone();
    both["views"]["overview"]["blocks"][1]["table"] =
        base["views"]["overview"]["blocks"][0]["table"].clone();
    shapes.push(both);
    let mut neither = base.clone();
    neither["views"]["overview"]["blocks"][1]
        .as_object_mut()
        .unwrap()
        .remove("list");
    shapes.push(neither);
    for (path, member, value) in [
        ("/views/overview/blocks/0", "args", json!(["title"])),
        ("/views/overview/blocks/0", "description", json!("block")),
        (
            "/views/overview/blocks/0/table",
            "properties",
            json!(["title"]),
        ),
        ("/views/overview/blocks/0/table", "args", json!(["title"])),
        (
            "/views/overview/blocks/1/list",
            "columns",
            json!([{"header":"K","property":"$key"}]),
        ),
        (
            "/views/overview/blocks/1/list",
            "description",
            json!("list"),
        ),
    ] {
        let mut s = base.clone();
        s.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(member.into(), value);
        shapes.push(s);
    }
    let mut no_heading = base.clone();
    no_heading["views"]["overview"]["blocks"][0]
        .as_object_mut()
        .unwrap()
        .remove("heading");
    shapes.push(no_heading);
    for s in shapes {
        let t = Temp::new();
        let file = t.file("invalid.json", &serde_json::to_vec(&s).unwrap());
        for command in ["check", "install"] {
            let (code, answer) = t.run(&["schema", command, &file], json!({}));
            assert_eq!(
                (code, answer["code"].as_str()),
                (2, Some("schema_violation")),
                "{command} {}: {answer}",
                s["views"]["overview"]
            );
        }
    }
}

#[test]
fn document_list_prints_none_truncation_and_escapes() {
    // spec: specs/ont/render.md#R54
    let t = Temp::new();
    let mut s = document_schema();
    s["views"]["lists"] = json!({"blocks":[
        {"heading":"Empty","list":{"object_type":"note","filter":{"eq":["text","nothing"]},
            "order":[{"property":"text","direction":"asc"}],"properties":["text"]}},
        {"heading":"Cut | <!-- x -->","list":{"object_type":"item",
            "order":[{"property":"rank","direction":"asc"}],"properties":["$key","title","rank"],"limit":1}},
        {"heading":"Escaped","list":{"object_type":"note",
            "order":[{"property":"$key","direction":"asc"}],"properties":["$key","text"]}}]});
    t.install(s);
    append(&t, "a", fields("first", 1), Change::Initial, 2000);
    append(&t, "b", fields("second", 2), Change::Initial, 2000);
    note(&t, "e1", text("line\nbreak|pipe\\back<!--c-->"), 2000);
    note(&t, "e2", text("old"), 20);
    note(&t, "e3", Knowledge::Null, 2000);
    let (code, answer) = t.render("lists", json!({}));
    assert_eq!(code, 0, "{answer}");
    let expected = "<!-- ont:render lists schema=board@1 -->\n\
        ## Empty\n- (none)\n\n\
        ## Cut \\| <\\!-- x --\\>\n- a: first: 1\n- (+1 more; limit 1)\n\n\
        ## Escaped\n- e1: line break\\|pipe\\\\back<\\!--c--\\>\n- e2: old (stale)\n- e3: (null)\n\
        <!-- /ont:render -->";
    assert_eq!(
        answer,
        json!({"status":"ok","view":"lists","schema":"board@1","rows":4,"truncated":true,
            "markdown":expected})
    );
    lines_are_inert(expected);
}

#[test]
fn document_arguments_and_admission_cover_every_block() {
    // spec: specs/ont/render.md#R55
    let t = Temp::new();
    let mut s = document_schema();
    s["views"]["checked"] = json!({"args":["title"],"blocks":[
        {"heading":"Items","table":{"object_type":"item","filter":{"eq":["title","$title"]},
            "order":[{"property":"rank","direction":"asc"}],"columns":[{"header":"Key","property":"$key"}]}},
        {"heading":"Short","list":{"object_type":"note","filter":{"eq":["short","$title"]},
            "order":[{"property":"$key","direction":"asc"}],"properties":["$key"]}}]});
    s["views"]["mixed"] = json!({"args":["key"],"blocks":[
        {"heading":"Notes of","table":{"object_type":"note","filter":{"eq":["item","$key"]},
            "order":[{"property":"$key","direction":"asc"}],"columns":[{"header":"Note","property":"$key"}]}},
        {"heading":"Titled","list":{"object_type":"item","filter":{"eq":["title","$key"]},
            "order":[{"property":"$key","direction":"asc"}],"properties":["$key","rank"]}}]});
    t.install(s.clone());
    append(&t, "a", fields("a", 1), Change::Initial, 2000);
    append(&t, "b", fields("b", 2), Change::Initial, 2000);
    append_all(
        &t,
        vec![
            (
                note_id("on-a"),
                observation(
                    BTreeMap::from([("item".into(), Knowledge::Value(json!(id("a"))))]),
                    2000,
                ),
            ),
            (
                note_id("on-b"),
                observation(
                    BTreeMap::from([("item".into(), Knowledge::Value(json!(id("b"))))]),
                    2000,
                ),
            ),
        ],
    );
    // Missing and undeclared arguments.
    let (code, answer) = t.render("checked", json!({}));
    assert_eq!(
        (code, answer["code"].as_str()),
        (2, Some("invalid_argument"))
    );
    assert_eq!(answer["field"], "/args/title");
    let (code, answer) = t.render("checked", json!({"title":"a","other":"x"}));
    assert_eq!(
        (code, answer["code"].as_str()),
        (2, Some("invalid_argument"))
    );
    assert_eq!(answer["field"], "/args/other");
    // Only the second document block's property refuses the value, without naming it.
    let (code, answer) = t.render("checked", json!({"title":"toolong"}));
    argument_refusal(code, &answer, "title", "short");
    assert!(!answer.to_string().contains("toolong"), "{answer}");
    let (code, answer) = t.render("checked", json!({"title":"a"}));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["markdown"],
        "<!-- ont:render checked title=a schema=board@1 -->\n## Items\n| Key |\n| --- |\n| a |\n\n## Short\n- (none)\n<!-- /ont:render -->"
    );
    // A reference in one document block and a string in another.
    let (code, answer) = t.render("mixed", json!({"key":"a"}));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["markdown"],
        "<!-- ont:render mixed key=a schema=board@1 -->\n## Notes of\n| Note |\n| --- |\n| on-a |\n\n## Titled\n- a: 1\n<!-- /ont:render -->"
    );
    assert_eq!(answer["rows"], 2);
    for key in ["bad key", "bad=key", ""] {
        let (code, answer) = t.render("mixed", json!({"key":key}));
        assert_eq!(
            (code, answer["code"].as_str()),
            (2, Some("reference_argument_unrepresentable")),
            "{answer}"
        );
        assert_eq!(answer["field"], "/args/key");
    }
    // A principal who may not read the second document block's type learns nothing.
    let hidden = Temp::new();
    s["grants"][0]["read"] = json!(["item"]);
    hidden.install(s);
    append(&hidden, "a", fields("a", 1), Change::Initial, 2000);
    let absent = hidden.render("absent", json!({}));
    assert_eq!(absent.1["code"], "unavailable");
    for args in [json!({"title":"a"}), json!({}), json!({"title":"toolong"})] {
        assert_eq!(hidden.render("checked", args), absent);
    }
}

#[test]
fn document_blocks_share_one_snapshot() {
    // spec: specs/ont/render.md#R56
    let t = Temp::new();
    let mut s = document_schema();
    s["views"]["snap"] = json!({"blocks":[
        {"heading":"Items","table":{"object_type":"item","limit":2,
            "order":[{"property":"rank","direction":"asc"}],
            "columns":[{"header":"Key","property":"$key"},{"header":"Rank","property":"rank"}]}},
        {"heading":"Notes","list":{"object_type":"note",
            "order":[{"property":"$key","direction":"asc"}],"properties":["$key","text"]}}]});
    t.install(s);
    append_all(
        &t,
        (0..1002)
            .map(|i| {
                (
                    id(&format!("k{i:04}")),
                    observation(fields("page", 1002 - i), 2000),
                )
            })
            .collect(),
    );
    let store_path = t.0.join("store.sqlite");
    let writer = std::sync::Arc::new(std::sync::Mutex::new(None));
    let pending = writer.clone();
    ont::render::__set_between_pages_hook(move || {
        let (ready, receiver) = std::sync::mpsc::channel();
        *pending.lock().unwrap() = Some(std::thread::spawn(move || {
            let mut store = ont_store::Store::open_schema_existing_writable(&store_path).unwrap();
            store
                .write(1000, |tx| {
                    tx.append(&ont_store::Append {
                        id: note_id("late"),
                        scope: Scope::domain(ns(), "board", BTreeMap::new()).unwrap(),
                        observation: observation(
                            BTreeMap::from([("text".into(), text("concurrent"))]),
                            2000,
                        ),
                    })?;
                    ready.send(()).unwrap();
                    Ok(())
                })
                .unwrap();
        }));
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
    });
    let (code, answer) = t.render("snap", json!({}));
    assert_eq!(code, 0, "{answer}");
    writer.lock().unwrap().take().unwrap().join().unwrap();
    assert_eq!(
        answer["markdown"],
        "<!-- ont:render snap schema=board@1 -->\n## Items\n| Key | Rank |\n| --- | --- |\n| k1001 | 1 |\n| k1000 | 2 |\n| (+1000 more; limit 2) |  |\n\n## Notes\n- (none)\n<!-- /ont:render -->"
    );
    assert_eq!(
        (answer["rows"].clone(), answer["truncated"].clone()),
        (json!(2), json!(true))
    );
    let (code, next) = t.render("snap", json!({}));
    assert_eq!(code, 0, "{next}");
    assert!(
        next["markdown"]
            .as_str()
            .unwrap()
            .ends_with("## Notes\n- late: concurrent\n<!-- /ont:render -->"),
        "{next}"
    );
}

#[test]
fn document_blocks_share_the_candidate_bound() {
    // spec: specs/ont/render.md#R57
    let t = Temp::new();
    let mut s = document_schema();
    s["views"]["notes"] = json!({"object_type":"note","order":[{"property":"$key","direction":"asc"}],
        "columns":[{"header":"Key","property":"$key"}]});
    s["views"]["both"] = json!({"blocks":[
        {"heading":"Items","table":{"object_type":"item","order":[{"property":"$key","direction":"asc"}],
            "columns":[{"header":"Key","property":"$key"}]}},
        {"heading":"Notes","list":{"object_type":"note","order":[{"property":"$key","direction":"asc"}],
            "properties":["$key"]}}]});
    s["views"]["items_only"] = json!({"blocks":[
        {"heading":"Items","table":{"object_type":"item","order":[{"property":"$key","direction":"asc"}],
            "columns":[{"header":"Key","property":"$key"}]}}]});
    t.install(s);
    let mut entries: Vec<_> = (0..5001)
        .map(|i| {
            (
                id(&format!("i{i:05}")),
                observation(fields("bulk", i), 2000),
            )
        })
        .collect();
    entries.extend((0..5001).map(|i| {
        (
            note_id(&format!("n{i:05}")),
            observation(BTreeMap::from([("text".into(), text("bulk"))]), 2000),
        )
    }));
    append_all(&t, entries);
    for view in ["items", "notes", "items_only"] {
        let (code, answer) = t.render(view, json!({}));
        assert_eq!(code, 0, "{view}: {answer}");
        assert_eq!(answer["rows"], 200);
        assert_eq!(answer["truncated"], true);
    }
    let (code, answer) = t.render("both", json!({}));
    assert_eq!(
        (code, answer["code"].as_str()),
        (2, Some("result_too_large"))
    );
    assert!(answer.get("markdown").is_none(), "{answer}");
}

#[test]
fn document_is_one_generated_block_for_write_and_check() {
    // spec: specs/ont/render.md#R58
    let t = overview_store();
    let (code, answer) = t.render("overview", json!({"title":"equal"}));
    assert_eq!(code, 0, "{answer}");
    let mut members: Vec<_> = answer.as_object().unwrap().keys().cloned().collect();
    members.sort();
    assert_eq!(
        members,
        ["markdown", "rows", "schema", "status", "truncated", "view"]
    );
    let original = "before\n<!-- ont:render overview title=equal schema=board@0 -->\nold\n<!-- /ont:render -->\nafter\n";
    let file = t.file("doc.md", original.as_bytes());
    let write = || {
        t.run(
            &[
                "render",
                "--write",
                &file,
                "--section",
                "overview",
                "--arg",
                "title=equal",
                "--preimage-dir",
                t.0.to_str().unwrap(),
            ],
            json!({}),
        )
    };
    let (code, written) = write();
    assert_eq!(code, 0, "{written}");
    assert_eq!(
        (
            &written["view"],
            &written["changed"],
            &written["rows"],
            &written["status"]
        ),
        (&json!("overview"), &json!(true), &json!(4), &json!("ok"))
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("before\n{OVERVIEW}\nafter\n")
    );
    assert_eq!(
        t.run(
            &["render", "--check", &file, "--expect-blocks", "1"],
            json!({})
        ),
        (0, json!({"status":"ok","blocks":1,"drift":[]}))
    );
    let (code, again) = write();
    assert_eq!(code, 0, "{again}");
    assert_eq!(
        (&again["changed"], &again["preimage"]),
        (&json!(false), &Value::Null)
    );
    // A change in the second document block is drift of the one generated block.
    let mut store =
        ont_store::Store::open_schema_existing_writable(&t.0.join("store.sqlite")).unwrap();
    store
        .write(1000, |tx| {
            let access = tx.head_access(&note_id("n2"))?.unwrap();
            let head = tx.head_after_access(&access)?.unwrap();
            let mut observation = head.head.observation;
            observation.supersedes = Some(observation.revision.clone());
            observation.revision = "r2".into();
            observation.change = Change::Update;
            observation.fields = BTreeMap::from([("text".into(), text("changed"))]);
            tx.append(&ont_store::Append {
                id: head.id,
                scope: head.scope,
                observation,
            })
        })
        .unwrap();
    drop(store);
    let (code, answer) = t.run(
        &["render", "--check", &file, "--expect-blocks", "1"],
        json!({}),
    );
    assert_eq!(
        (code, answer["code"].as_str()),
        (2, Some("render_drift")),
        "{answer}"
    );
    assert_eq!(
        answer["drift"],
        json!([{"view":"overview","args":{"title":"equal"},"line":2}])
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("before\n") && text.ends_with("\nafter\n"));
}
