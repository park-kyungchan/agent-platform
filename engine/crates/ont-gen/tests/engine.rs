//! The committed module of the synthetic fixture against the engine: each
//! call function's request, and a run on a temporary store through the
//! engine in process (`ont::run` with uid 0 passed, as the engine's own tests
//! do, so that it holds where the test user is not root).
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/generated/example.rs"
));

use example::{Call, Command, Engine, Id, actions, functions, links, objects, properties, values};

/// An engine that keeps each request and answers it with `{}`.
#[derive(Default)]
struct Recorder(Vec<(Vec<&'static str>, Value)>);
impl Engine for Recorder {
    type Error = std::convert::Infallible;
    fn call(&mut self, command: Command, request: Value) -> Result<Value, Self::Error> {
        self.0.push((command.argv(), request));
        Ok(json!({}))
    }
}

/// The engine in process on one temporary store.
struct InProcess {
    store: String,
}
impl Engine for InProcess {
    type Error = String;
    fn call(&mut self, command: Command, request: Value) -> Result<Value, String> {
        let mut args: Vec<String> = ["--store", &self.store, "--now", "1000"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.extend(command.argv().into_iter().map(String::from));
        let out = ont::run(&args, request.to_string().as_bytes(), 0, 0);
        let answer: Value = serde_json::from_str(&out.stdout)
            .map_err(|_| format!("stdout {:?} stderr {:?}", out.stdout, out.stderr))?;
        if out.code == 0 {
            Ok(answer)
        } else {
            Err(format!("{command:?} {request} refused: {answer}"))
        }
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ont-gen-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn shelf(key: &str) -> Id<objects::Shelf> {
    Id::new(key)
}

fn add_item(r#type: Option<&str>) -> actions::AddItem {
    actions::AddItem {
        item: Id::new("lamp"),
        name: "Lamp".into(),
        price: "12.50".into(),
        made_on: "2026-01-02".into(),
        count: 3,
        seen_at: 1_700_000_000,
        fragile: true,
        shelf: shelf("north"),
        backups: vec![shelf("south")],
        shade: values::Shade::Dark,
        code: values::Code("ABC".into()),
        detail: actions::AddItemDetail {
            note: "top".into(),
            weight: 7,
        },
        r#type: r#type.map(String::from),
    }
}

fn id_value(object_type: &str, key: &str) -> Value {
    json!({"object_type": object_type, "namespace": {"instance": "test", "account": "example"}, "native_key": key})
}

// spec: specs/ont/gen.md#R5
#[test]
fn ids_and_properties() {
    let id = shelf("north");
    assert_eq!(id.to_value(), id_value("shelf", "north"));
    assert_eq!(
        Id::<objects::Shelf>::from_value(&id.to_value()),
        Some(id.clone())
    );
    assert_eq!(id.native_key(), "north");
    assert_eq!(Id::<objects::Item>::from_value(&id.to_value()), None);
    let mut extra = id.to_value();
    extra["more"] = json!(1);
    assert_eq!(Id::<objects::Shelf>::from_value(&extra), None);
    assert_eq!(Id::<objects::Shelf>::from_value(&json!("north")), None);
    let mut empty = id.to_value();
    empty["native_key"] = json!("");
    assert_eq!(Id::<objects::Shelf>::from_value(&empty), None);
    let other = Id::<objects::Shelf>::in_namespace("elsewhere", "someone", "north");
    assert_eq!(
        other.to_value()["namespace"],
        json!({"instance": "elsewhere", "account": "someone"})
    );
    assert_ne!(other, id);
    assert_eq!(<objects::Item as example::ObjectType>::NAME, "item");
    assert_eq!(properties::Item::MadeOn.name(), "made_on");
    assert_eq!(properties::Shelf::Label.name(), "label");
}

// spec: specs/ont/gen.md#R6
#[test]
fn value_types() {
    assert_eq!(values::Shade::Dark.as_str(), "dark");
    assert_eq!(values::Shade::parse("light"), Some(values::Shade::Light));
    assert_eq!(values::Shade::parse("Light"), None);
    let code = values::Code("not checked here".into());
    assert_eq!(code.0, "not checked here");
}

// spec: specs/ont/gen.md#R7
// spec: specs/ont/gen.md#R8
// spec: specs/ont/gen.md#R11
#[test]
fn action_requests() {
    let mut engine = Recorder::default();
    example::apply(&mut engine, &add_item(None), &Call::default()).unwrap();
    let call = Call {
        request_id: Some("rq-1".into()),
        agent: Some("tester".into()),
        session: Some("s-1".into()),
        role: Some("lead".into()),
        expected: vec![(shelf("north").to_value(), "r1".into())],
    };
    example::validate(&mut engine, &add_item(Some("lamp")), &call).unwrap();
    let parameters = json!({
        "item": id_value("item", "lamp"),
        "name": "Lamp", "price": "12.50", "made_on": "2026-01-02", "count": 3,
        "seen_at": 1_700_000_000, "fragile": true,
        "shelf": id_value("shelf", "north"), "backups": [id_value("shelf", "south")],
        "shade": "dark", "code": "ABC", "detail": {"note": "top", "weight": 7}
    });
    assert_eq!(engine.0[0].0, ["action", "apply"]);
    assert_eq!(
        engine.0[0].1,
        json!({"action_type": "add_item", "parameters": parameters})
    );
    let mut with_type = parameters.clone();
    with_type["type"] = json!("lamp");
    assert_eq!(engine.0[1].0, ["action", "validate"]);
    assert_eq!(
        engine.0[1].1,
        json!({
            "action_type": "add_item", "parameters": with_type,
            "request_id": "rq-1", "agent": "tester", "session": "s-1", "role": "lead",
            "expected": [{"id": id_value("shelf", "north"), "revision": "r1"}]
        })
    );
    assert_eq!(<actions::AddItem as example::Action>::NAME, "add_item");
}

// spec: specs/ont/gen.md#R9
// spec: specs/ont/gen.md#R10
// spec: specs/ont/gen.md#R11
#[test]
fn read_and_function_requests() {
    let mut engine = Recorder::default();
    example::get(&mut engine, &shelf("north")).unwrap();
    let filter = json!({"eq": [properties::Item::Name.name(), "Lamp"]});
    example::search::<objects::Item, _>(&mut engine, Some(filter.clone()), Some(5), Some("c1"))
        .unwrap();
    example::search::<objects::Shelf, _>(&mut engine, None, None, None).unwrap();
    example::around(
        &mut engine,
        Some(filter.clone()),
        (links::ItemShelf, links::ItemShelfReverse, links::ItemBackup),
        Some(9),
    )
    .unwrap();
    example::around(&mut engine, None, (links::ItemBackupReverse,), None).unwrap();
    let function = functions::Echo {
        word: "hello".into(),
        shelf: None,
        count: Some(2),
    };
    example::call(&mut engine, &function).unwrap();
    let expected: Vec<(Vec<&str>, Value)> = vec![
        (vec!["get"], json!({"id": id_value("shelf", "north")})),
        (
            vec!["search"],
            json!({"object_type": "item", "where": filter, "limit": 5, "continuation": "c1"}),
        ),
        (vec!["search"], json!({"object_type": "shelf"})),
        (
            vec!["around"],
            json!({"from": {"object_type": "item", "where": filter}, "path": ["shelf", "items", "backups"], "limit": 9}),
        ),
        (
            vec!["around"],
            json!({"from": {"object_type": "shelf"}, "path": ["backed_items"]}),
        ),
        (
            vec!["fn", "echo"],
            json!({"parameters": {"word": "hello", "count": 2}, "domain": "example"}),
        ),
    ];
    assert_eq!(engine.0.len(), expected.len());
    for ((argv, request), (want_argv, want)) in engine.0.iter().zip(&expected) {
        assert_eq!(argv, want_argv);
        assert_eq!(request, want);
        assert!(request.get("op").is_none());
    }
    assert_eq!(Command::ActionValidate.argv(), ["action", "validate"]);
    assert_eq!(<functions::Echo as example::Function>::NAME, "echo");
    assert_eq!(<links::ItemShelfReverse as example::Hop>::NAME, "items");
}

/// The program of the fixture's function.
const PROGRAM: &str = "/bin/cat";

/// The engine's owner rule for a program: uid 0 or the real uid.
fn program_owner_trusted(program: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(program).unwrap().uid();
    let real = std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|f| f.split_whitespace().next().map(str::to_string))
        .unwrap();
    owner == 0 || owner.to_string() == real
}

/// The pin of an answer's envelope for this module's domain.
fn pin(answer: &Value) -> &Value {
    &answer["schema"][example::DOMAIN]
}

// spec: specs/ont/gen.md#R11
// spec: specs/ont/gen.md#R12
#[test]
fn a_temporary_store_answers_every_call() {
    let t = Temp::new();
    let store = t.0.join("store.sqlite").display().to_string();
    let schema =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/example.schema.json");
    let args: Vec<String> = ["--store", &store, "--now", "1000", "schema", "install"]
        .iter()
        .map(|s| s.to_string())
        .chain([schema.display().to_string()])
        .collect();
    let out = ont::run(&args, &b""[..], 0, 0);
    assert_eq!(out.code, 0, "{} {}", out.stdout, out.stderr);
    let mut engine = InProcess { store };
    let call = Call::default();
    let pinned = json!({"version": example::VERSION, "sha256": example::SHA256});

    for key in ["north", "south"] {
        let action = actions::AddShelf {
            shelf: shelf(key),
            label: key.to_uppercase(),
        };
        let answer = example::apply(&mut engine, &action, &call).unwrap();
        assert_eq!(answer["status"], "ok", "{answer}");
        assert_eq!(pin(&answer), &pinned, "{answer}");
    }
    let validated = example::validate(&mut engine, &add_item(Some("lamp")), &call).unwrap();
    assert_eq!(validated["status"], "ok", "{validated}");
    assert_eq!(validated["result"]["valid"], true, "{validated}");
    assert_eq!(pin(&validated), &pinned);
    let applied = example::apply(&mut engine, &add_item(Some("lamp")), &call).unwrap();
    assert_eq!(applied["status"], "ok", "{applied}");
    assert_eq!(applied["result"]["outcome"], "applied", "{applied}");

    let item = Id::<objects::Item>::new("lamp");
    let got = example::get(&mut engine, &item).unwrap();
    assert_eq!(got["status"], "ok", "{got}");
    assert_eq!(pin(&got), &pinned);
    let object = &got["result"]["objects"][0];
    assert_eq!(
        Id::<objects::Item>::from_value(&object["id"]),
        Some(item.clone()),
        "{got}"
    );
    let fields: serde_json::Map<String, Value> = object["observations"][0]["fields"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, field)| (name.clone(), field["value"].clone()))
        .collect();
    assert_eq!(
        Value::Object(fields),
        json!({
            "name": "Lamp", "price": "12.50", "made_on": "2026-01-02", "count": 3,
            "seen_at": 1_700_000_000, "fragile": true,
            "shelf": shelf("north").to_value(), "backups": [shelf("south").to_value()],
            "shade": "dark", "code": "ABC", "detail": {"note": "top", "weight": 7},
            "kind": "lamp"
        }),
        "{got}"
    );

    let filter = json!({"eq": [properties::Item::Name.name(), "Lamp"]});
    let found =
        example::search::<objects::Item, _>(&mut engine, Some(filter), Some(10), None).unwrap();
    assert_eq!(found["status"], "ok", "{found}");
    assert_eq!(pin(&found), &pinned);
    let ids = |answer: &Value| -> Vec<Value> {
        answer["result"]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].clone())
            .collect()
    };
    assert_eq!(ids(&found), [item.to_value()], "{found}");

    let near = example::around(&mut engine, None, (links::ItemShelf,), None).unwrap();
    assert_eq!(near["status"], "ok", "{near}");
    assert_eq!(pin(&near), &pinned);
    assert_eq!(ids(&near), [shelf("north").to_value()], "{near}");
    let back = example::around(
        &mut engine,
        None,
        (links::ItemBackup, links::ItemBackupReverse),
        Some(10),
    )
    .unwrap();
    assert_eq!(back["status"], "ok", "{back}");
    assert_eq!(pin(&back), &pinned);
    assert_eq!(ids(&back), [item.to_value()], "{back}");

    let function = functions::Echo {
        word: "hello".into(),
        shelf: Some(shelf("north")),
        count: None,
    };
    if !program_owner_trusted(PROGRAM) {
        // A build host whose files belong to an unmapped uid has no program
        // the engine trusts (R7 of specs/ont/fn.md); the call still reaches
        // the function and is refused for that reason alone.
        let refused = example::call(&mut engine, &function).unwrap_err();
        assert!(
            refused.contains("\"detail\":\"owner_not_trusted\""),
            "{refused}"
        );
        assert!(refused.contains("\"function\":\"echo\""), "{refused}");
        eprintln!(
            "{PROGRAM} is owned by neither uid 0 nor this uid: the call was refused for that alone"
        );
        return;
    }
    let answered = example::call(&mut engine, &function).unwrap();
    assert_eq!(answered["function"], "echo", "{answered}");
    assert_eq!(answered["domain"], example::DOMAIN, "{answered}");
    assert_eq!(
        answered["answer"],
        json!({"word": "hello", "shelf": shelf("north").to_value()}),
        "{answered}"
    );
}
