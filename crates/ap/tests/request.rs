//! `ap request` (README "One request", `specs/ap/request.md`) against a fake of `ont` that answers as
//! the engine does, and a session record of invented lines (`fixtures/request/session.jsonl`).
use ap::{
    host::{Answer, Failure, Reply, System},
    request::{Refusal, execute, text},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const RECORD: &[u8] = include_bytes!("fixtures/request/session.jsonl");
const PATH: &str = "/rec/session.jsonl";
/// The fake store's clock.
const NOW: &str = "2026-11-17T12:00:00Z";
const ACTIONS: [&str; 4] = [
    "record_request",
    "supersede_request",
    "route_request",
    "set_request_aside",
];
fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
/// A store of requests and intents with the four actions as the schema states them. As the engine does,
/// a request id is the client's, the agent's and the `request_id`: repeated with the same action and
/// parameters it answers `already_applied` with the first edits, with others `request_id_reused`.
struct F {
    calls: Vec<(Vec<String>, Value)>,
    stores: Vec<Option<String>>,
    files: BTreeMap<String, Vec<u8>>,
    objects: BTreeMap<String, Value>,
    applied: BTreeMap<String, (Value, Value)>,
    /// The next this many calls answer `store_busy`.
    busy: usize,
    pauses: usize,
    /// The store keeps other words than it was given.
    alter_words: bool,
    /// The next apply of this action fails as a process and writes nothing.
    cut: Option<&'static str>,
    /// The next apply of this action is refused with this answer.
    refuse: Option<(&'static str, Value)>,
    unqualified: bool,
    /// Each object of a search answer is padded to the size of the largest head the engine answers.
    heavy: bool,
}
/// The bound of `Host::call`: an answer over 1 MiB is `process_failed`.
const LIMIT: usize = 1_048_576;
/// The largest object of an engine search answer: a head of 65,536 canonical bytes, its wrappers and metadata.
const HEAD: usize = 66_700;
fn reply(success: bool, v: Value) -> Answer {
    let bytes = serde_json::to_vec(&v).unwrap();
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
fn ok(result: Value) -> Answer {
    reply(true, json!({"status": "ok", "result": result}))
}
fn refused(code: &str) -> Answer {
    reply(false, json!({"status": "refused", "code": code}))
}
impl F {
    fn new() -> Self {
        let mut f = Self {
            calls: vec![],
            stores: vec![],
            files: BTreeMap::new(),
            objects: BTreeMap::new(),
            applied: BTreeMap::new(),
            busy: 0,
            pauses: 0,
            alter_words: false,
            cut: None,
            refuse: None,
            unqualified: false,
            heavy: false,
        };
        f.files.insert(PATH.into(), RECORD.to_vec());
        for (key, status) in [("open", "draft"), ("met", "met"), ("gone", "retired")] {
            f.objects.insert(
                key.into(),
                json!({"object_type": "intent", "status": status}),
            );
        }
        f
    }
    fn names(&self) -> Vec<String> {
        let name = |(a, b): &(Vec<String>, Value)| match a[0].as_str() {
            "action" => format!("apply {}", b["action_type"].as_str().unwrap()),
            "get" => format!("get {}", b["id"]["object_type"].as_str().unwrap()),
            other => format!("{other} {}", b["object_type"].as_str().unwrap()),
        };
        self.calls.iter().map(name).collect()
    }
    fn writes(&self) -> usize {
        self.calls.iter().filter(|c| c.0[0] == "action").count()
    }
    fn requests(&self) -> Vec<(&String, &Value)> {
        let mine = self.objects.iter();
        mine.filter(|(_, o)| o["object_type"] == "request")
            .collect()
    }
    fn request(&self, key: &str) -> &Value {
        &self.objects[key]
    }
    fn is(&self, key: &Value, kind: &str, check: impl Fn(&Value) -> bool) -> bool {
        let found = key["native_key"].as_str().and_then(|k| self.objects.get(k));
        found.is_some_and(|o| o["object_type"] == kind && check(o))
    }
    fn failed(&self, action: &str, p: &Value) -> Vec<&'static str> {
        let mut failed = vec![];
        let current = |o: &Value| o["status"] == "current";
        if action != "record_request" && !self.is(&p["request"], "request", current) {
            failed.push("request_not_current");
        }
        if action.ends_with("_request") && action != "route_request" {
            if p["words"] == "" {
                failed.push("words_empty");
            }
            if p["said_at"].as_str().unwrap() > NOW {
                failed.push("said_in_the_future");
            }
        }
        if action == "route_request" {
            if !self.is(&p["intent"], "intent", |_| true) {
                failed.push("unknown_intent");
            }
            let closed = |o: &Value| o["status"] == "met" || o["status"] == "retired";
            if self.is(&p["intent"], "intent", closed) {
                failed.push("intent_closed");
            }
        }
        if action == "set_request_aside" && p["reason"] == "" {
            failed.push("reason_empty");
        }
        failed
    }
    fn create(&mut self, p: &Value) -> String {
        let key = format!("rq{}", self.objects.len());
        let mut o = p.clone();
        let o_ = o.as_object_mut().unwrap();
        o_.remove("request");
        o_.insert("object_type".into(), json!("request"));
        o_.insert("status".into(), json!("current"));
        o_.insert("route".into(), json!("unrouted"));
        if self.alter_words {
            o["words"] = json!(format!("{}!", p["words"].as_str().unwrap()));
        }
        self.objects.insert(key.clone(), o);
        key
    }
    fn apply(&mut self, body: &Value) -> Answer {
        let (action, p) = (body["action_type"].as_str().unwrap(), &body["parameters"]);
        let derived = format!("{}/{}", body["agent"], body["request_id"]);
        let content = json!([action, p]);
        if let Some((first, edits)) = self.applied.get(&derived) {
            if *first == content {
                return ok(json!({"outcome": "already_applied", "edits": edits}));
            }
            return refused("request_id_reused");
        }
        if self.cut == Some(action) {
            self.cut = None;
            return Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            });
        }
        if self.refuse.as_ref().is_some_and(|r| r.0 == action) {
            return reply(false, self.refuse.take().unwrap().1);
        }
        let failed = self.failed(action, p);
        if !failed.is_empty() {
            let failures: Vec<_> = failed
                .iter()
                .map(|c| json!({"code": c, "message": format!("the message of {c}")}))
                .collect();
            return reply(
                false,
                json!({"status": "refused", "code": "submission_criteria", "failed": failed, "failures": failures}),
            );
        }
        let target = p["request"]["native_key"].as_str().unwrap_or("").to_owned();
        let change = |key: &str, change: &str| json!({"change": change, "id": oid("request", key)});
        let edits = match action {
            "record_request" => vec![change(&self.create(p), "initial")],
            "supersede_request" => {
                self.objects.get_mut(&target).unwrap()["status"] = json!("superseded");
                let key = self.create(p);
                self.objects.get_mut(&key).unwrap()["supersedes"] = p["request"].clone();
                vec![change(&target, "modification"), change(&key, "initial")]
            }
            "route_request" => {
                let o = self.objects.get_mut(&target).unwrap();
                (o["route"], o["intent"]) = (json!("intent"), p["intent"].clone());
                vec![change(&target, "modification")]
            }
            _ => {
                let o = self.objects.get_mut(&target).unwrap();
                (o["route"], o["route_reason"]) = (json!("aside"), p["reason"].clone());
                o.as_object_mut().unwrap().remove("intent");
                vec![change(&target, "modification")]
            }
        };
        self.applied
            .insert(derived, (content, json!(edits.clone())));
        ok(json!({"outcome": "applied", "edits": edits}))
    }
    fn fields(o: &Value) -> Value {
        let mine = o.as_object().unwrap().iter();
        let fields = mine.filter(|(k, _)| *k != "object_type");
        fields
            .map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
    }
    fn get(&self, body: &Value) -> Answer {
        let id = &body["id"];
        let kind = id["object_type"].as_str().unwrap();
        if !self.is(id, kind, |_| true) {
            return refused("unavailable");
        }
        let o = &self.objects[id["native_key"].as_str().unwrap()];
        let observation =
            json!({"revision": "r#1", "change": "initial", "fields": Self::fields(o)});
        ok(
            json!({"objects": [{"id": id, "head": "r#1", "observations": [{"revision": "r#0", "fields": {}}, observation]}]}),
        )
    }
    fn search(&self, body: &Value) -> Answer {
        let all = body["where"]["all"].as_array().unwrap();
        let matches = |o: &Value| {
            o["object_type"] == body["object_type"]
                && all
                    .iter()
                    .all(|c| o[c["eq"][0].as_str().unwrap()] == c["eq"][1])
        };
        let found: Vec<_> = (self.objects.iter())
            .filter(|(_, o)| matches(o))
            .map(|(k, o)| json!({"id": oid("request", k), "head": {"fields": Self::fields(o)}}))
            .map(|mut o| {
                if self.heavy {
                    let size = o.to_string().len();
                    o["padding"] = json!("p".repeat(HEAD.saturating_sub(size + 14)));
                }
                o
            })
            .collect();
        let from: usize = body["continuation"]
            .as_str()
            .map_or(0, |c| c.parse().unwrap());
        let page = body["limit"].as_u64().unwrap() as usize;
        assert!((1..=1000).contains(&page), "the engine's bound of limit");
        let to = (from + page).min(found.len());
        let next = (to < found.len()).then(|| to.to_string());
        ok(
            json!({"basis": {"qualified": !self.unqualified}, "objects": found[from..to],
            "continuation": next}),
        )
    }
}
impl System for F {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _ms: u64) -> Answer {
        assert_eq!(program, "ont", "only the store is called");
        let mut args = args.to_vec();
        let store = args.iter().position(|a| a == "--store").map(|i| {
            let s = args[i + 1].clone();
            args.truncate(i);
            s
        });
        self.stores.push(store);
        self.calls.push((args.clone(), input.clone()));
        if self.busy > 0 {
            self.busy -= 1;
            return refused("store_busy");
        }
        match args[0].as_str() {
            "action" => {
                assert_eq!(args, ["action", "apply", "--client", "ap"]);
                self.apply(input)
            }
            "get" => self.get(input),
            "search" => self.search(input),
            other => panic!("ont {other}"),
        }
        .and_then(|r| match r.bytes.len() > LIMIT {
            true => Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            }),
            false => Ok(r),
        })
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
    fn pause(&mut self, _: std::time::Duration) {
        self.pauses += 1;
    }
}
fn rec() -> Value {
    json!({"op": "record", "request_id": "m1", "agent": "lead", "session": "s1", "topic": "walk/one",
        "kind": "order", "reading": "the reading", "words": "do the thing", "said_at": "2026-11-16T03:04:05Z"})
}
fn from(at: &str) -> Value {
    let mut q = without(without(rec(), "words"), "said_at");
    q["words_from"] = json!({"record": PATH, "at": at});
    q
}
fn with(mut v: Value, k: &str, x: Value) -> Value {
    v[k] = x;
    v
}
fn without(mut v: Value, k: &str) -> Value {
    v.as_object_mut().unwrap().remove(k);
    v
}
fn run(f: &mut F, q: &Value) -> Value {
    match execute(f, None, q) {
        Ok(v) => v,
        Err(e) => panic!("{} {} {}", e.code, e.field, e.detail),
    }
}
fn refusal(f: &mut F, q: &Value) -> Refusal {
    execute(f, None, q).expect_err("a refusal")
}
fn applied_params(f: &F, action: &str) -> Value {
    let found = f.calls.iter().rev().find(|c| c.1["action_type"] == action);
    found.unwrap().1["parameters"].clone()
}
fn ap(args: &[&str], stdin: &[u8]) -> (Option<i32>, Vec<u8>, Value) {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("request")
        .args(args)
        .env("PATH", "/nonexistent")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin);
    let out = child.wait_with_output().unwrap();
    let err = serde_json::from_slice(&out.stderr).unwrap_or(Value::Null);
    (out.status.code(), out.stdout, err)
}

// spec: specs/ap/request.md#R1
#[test]
fn r1_arguments_are_store_and_text_for_list_else_usage() {
    // `PATH` names no program: none of these reaches a store.
    for (args, stdin) in [
        (vec!["--bogus"], rec()),
        (vec!["--store"], rec()),
        (vec!["--store", "relative"], rec()),
        (vec!["--store", "/a", "--store", "/b"], rec()),
        (vec!["--text", "--text"], json!({"op": "list"})),
        (vec!["--text"], rec()),
        (vec!["--store", "/s", "--text"], json!({"op": "route"})),
    ] {
        let (status, out, e) = ap(&args, stdin.to_string().as_bytes());
        assert_eq!(status, Some(2), "{args:?}");
        assert!(out.is_empty());
        assert_eq!(
            e,
            json!({"error": {"code": "usage", "field": "args", "detail": null, "after": null, "request": null}}),
            "{args:?}"
        );
    }
    let mut f = F::new();
    execute(
        &mut f,
        Some("/s/store"),
        &with(rec(), "route", json!({"intent": "open"})),
    )
    .ok()
    .unwrap();
    execute(&mut f, Some("/s/store"), &json!({"op": "list"}))
        .ok()
        .unwrap();
    assert_eq!(f.stores.len(), 5);
    assert!(f.stores.iter().all(|s| s.as_deref() == Some("/s/store")));
}
// spec: specs/ap/request.md#R2
#[test]
fn r2_a_malformed_request_is_invalid_input_with_its_field_before_any_call() {
    let mut cases = vec![
        (json!([1]), "stdin"),
        (json!({}), "op"),
        (json!({"op": "delete"}), "op"),
        (json!({"op": 1}), "op"),
        (with(rec(), "extra", json!(1)), "extra"),
        (json!({"op": "list", "words": "x"}), "words"),
        (json!({"op": "list", "topic": ""}), "topic"),
        (with(rec(), "request", json!("rq1")), "request"),
        (with(rec(), "op", json!("supersede")), "request"),
        (with(rec(), "route", json!("open")), "route"),
        (with(rec(), "route", json!({})), "route"),
        (
            with(rec(), "route", json!({"intent": "open", "aside": "r"})),
            "route",
        ),
        (with(rec(), "route", json!({"to": "open"})), "route.to"),
        (with(rec(), "route", json!({"intent": 5})), "route.intent"),
        (with(rec(), "words_file", json!("/w/x")), "words_file"),
        (without(rec(), "words"), "words"),
        (
            with(from("2026-11-16T03:04:05"), "words", json!("x")),
            "words_from",
        ),
        (with(rec(), "words", json!(5)), "words"),
        (
            with(
                from("2026-11-16T03:04:05"),
                "said_at",
                json!("2026-11-16T03:04:05Z"),
            ),
            "said_at",
        ),
    ];
    for k in [
        "request_id",
        "agent",
        "session",
        "topic",
        "kind",
        "reading",
        "said_at",
    ] {
        cases.push((without(rec(), k), k));
    }
    let route = json!({"op": "route", "request_id": "m1", "agent": "lead", "session": "s1", "request": "rq1"});
    cases.extend([
        (route.clone(), "intent"),
        (
            with(
                with(route.clone(), "intent", json!("open")),
                "aside",
                json!("r"),
            ),
            "intent",
        ),
        (
            with(without(route.clone(), "request"), "intent", json!("open")),
            "request",
        ),
        (with(route, "topic", json!("t")), "topic"),
    ]);
    for (q, field) in cases {
        let mut f = F::new();
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            ("invalid_input", field, None),
            "{q}"
        );
        assert!(f.calls.is_empty(), "{q}");
    }
    // More than 1 MiB, and what is no JSON, are refused before any call.
    let (status, _, e) = ap(&[], &vec![b' '; 1_048_577]);
    assert_eq!(
        (status, &e["error"]["code"]),
        (Some(2), &json!("input_too_large"))
    );
    let (status, _, e) = ap(&[], b"not json");
    assert_eq!(
        (status, &e["error"]["code"], &e["error"]["field"]),
        (Some(2), &json!("invalid_input"), &json!("stdin"))
    );
}
// spec: specs/ap/request.md#R3
#[test]
fn r3_ids_names_topic_kind_reading_and_runtime_have_their_form() {
    let long = |n: usize| json!("a".repeat(n));
    let cases = [
        ("request_id", long(51)),
        ("request_id", json!("a b")),
        ("request_id", json!("a:b")),
        ("agent", long(65)),
        ("agent", json!("a/b")),
        ("session", json!("")),
        ("session", long(65)),
        ("topic", long(121)),
        ("topic", json!("a topic")),
        ("topic", json!("가")),
        ("kind", json!("wish")),
        ("reading", json!("")),
        ("reading", long(1001)),
        ("runtime", long(41)),
        ("runtime", json!("a/b")),
    ];
    for (k, v) in cases {
        let mut f = F::new();
        let e = refusal(&mut f, &with(rec(), k, v.clone()));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", k),
            "{k} {v}"
        );
        assert!(f.calls.is_empty());
    }
    let e = refusal(&mut F::new(), &with(rec(), "reading", long(1001)));
    assert_eq!(e.detail, json!({"bytes": 1001, "limit": 1000}));
    let edges = [
        ("request_id", long(50)),
        ("request_id", json!("A.b_c-9")),
        ("agent", json!("lead:a.b_c-9")),
        ("session", long(64)),
        ("topic", json!(format!("a.b_c:d/e-{}", "f".repeat(110)))),
        ("kind", json!("delegation")),
        ("reading", long(1000)),
        ("runtime", json!("claude")),
    ];
    for (k, v) in edges {
        let mut f = F::new();
        run(&mut f, &with(rec(), k, v.clone()));
        let p = applied_params(&f, "record_request");
        if ["topic", "kind", "reading", "runtime", "session"].contains(&k) {
            assert_eq!(p[k], v, "{k}");
        }
    }
}
// spec: specs/ap/request.md#R4
#[test]
fn r4_a_refusal_says_whether_the_call_recorded_and_which_request() {
    let (status, _, e) = ap(&[], br#"{"op":"record"}"#);
    assert_eq!(status, Some(2));
    let mut keys: Vec<_> = e["error"].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["after", "code", "detail", "field", "request"]);
    // Refused before the record.
    let mut f = F::new();
    let e = refusal(&mut f, &with(rec(), "route", json!({"intent": "gone"})));
    assert_eq!((e.after, e.request), (None, None));
    // Refused after it: the request's key.
    let mut f = F::new();
    f.cut = Some("set_request_aside");
    let e = refusal(&mut f, &with(rec(), "route", json!({"aside": "a remark"})));
    let key = f.requests()[0].0.clone();
    assert_eq!(
        (e.code.as_str(), e.after, e.request),
        ("process_failed", Some("record"), Some(key))
    );
}
// spec: specs/ap/request.md#R5
#[test]
fn r5_a_store_refusal_keeps_its_code_and_object() {
    let mut f = F::new();
    let e = refusal(
        &mut f,
        &with(rec(), "said_at", json!("2030-01-01T00:00:00Z")),
    );
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("said_in_the_future", "record", None)
    );
    assert_eq!(e.detail["failed"], json!(["said_in_the_future"]));
    assert_eq!(
        e.detail["failures"][0]["message"],
        "the message of said_in_the_future"
    );
    let engine = json!({"status": "refused", "code": "invalid_parameter", "field": "/parameters/kind", "parameter": "kind"});
    let mut f = F::new();
    f.refuse = Some(("record_request", engine.clone()));
    let e = refusal(&mut f, &rec());
    assert_eq!((e.code.as_str(), e.detail), ("invalid_parameter", engine));
    // A supersede of a request that is not current.
    let mut f = F::new();
    let first = run(&mut f, &rec());
    let q = with(
        with(rec(), "op", json!("supersede")),
        "request",
        first["request"].clone(),
    );
    run(&mut f, &with(q.clone(), "request_id", json!("m2")));
    let e = refusal(&mut f, &with(q, "request_id", json!("m3")));
    assert_eq!((e.code.as_str(), e.after), ("request_not_current", None));
}
// spec: specs/ap/request.md#R6
#[test]
fn r6_actions_carry_their_request_ids_the_client_and_the_actor() {
    let mut f = F::new();
    let first = run(&mut f, &with(rec(), "route", json!({"intent": "open"})));
    let q = json!({"op": "supersede", "request": first["request"], "request_id": "m2", "agent": "lead",
        "session": "s1", "topic": "walk/one", "kind": "correction", "reading": "r", "words": "w", "said_at": "2026-11-16T03:04:06Z"});
    run(&mut f, &q);
    let applies: Vec<_> = f.calls.iter().filter(|c| c.0[0] == "action").collect();
    let ids: Vec<_> = applies
        .iter()
        .map(|c| c.1["request_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["m1-record", "m1-route", "m2-supersede", "m2-route"]);
    for c in &applies {
        assert_eq!(c.0, ["action", "apply", "--client", "ap"]);
        assert_eq!(
            (&c.1["agent"], &c.1["session"]),
            (&json!("lead"), &json!("s1"))
        );
    }
    // The same request id with another session is other content; with another agent another request.
    let e = refusal(&mut f, &with(rec(), "session", json!("s2")));
    assert_eq!((e.code.as_str(), e.after), ("request_id_reused", None));
    let other = run(&mut f, &with(rec(), "agent", json!("other")));
    assert_eq!(other["record"], "applied");
    assert_ne!(other["request"], first["request"]);
}
// spec: specs/ap/request.md#R7
#[test]
fn r7_words_from_take_the_one_string_of_the_users_lines_of_the_second() {
    // A harness line (`isMeta`) in the same second is not the user's.
    let mut f = F::new();
    run(&mut f, &from("2026-11-16T03:04:05"));
    assert_eq!(
        applied_params(&f, "record_request")["words"],
        "Make the \"list\" show\nall of it, 가나다 \u{1} "
    );
    // The same string twice in one second (a line and its queued copy) is one.
    let mut f = F::new();
    run(&mut f, &from("2026-11-16T03:04:07"));
    assert_eq!(
        applied_params(&f, "record_request")["words"],
        "same words twice"
    );
    // A compaction summary, a sub-agent's prompt, a tool result and no line at all: none.
    for at in [
        "2026-11-16T03:04:06",
        "2026-11-16T03:04:09",
        "2026-11-16T03:04:10",
        "2026-11-16T03:05:00",
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &from(at));
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), &e.detail, e.after),
            ("words_not_found", "words_from", &json!(0), None),
            "{at}"
        );
        assert!(f.calls.is_empty());
    }
    let mut f = F::new();
    let e = refusal(&mut f, &from("2026-11-16T03:04:08"));
    assert_eq!((e.code.as_str(), e.detail), ("words_ambiguous", json!(2)));
    assert!(f.calls.is_empty());
    // The time is `YYYY-MM-DDTHH:MM:SS`; the record an absolute path of at most 400 bytes that exists.
    for at in [
        "2026-11-16",
        "2026-11-16 03:04:05",
        "2026-11-16T03:04:05Z",
        "2026-11-16T03:04:0x",
    ] {
        assert_eq!(
            refusal(&mut F::new(), &from(at)).field,
            "words_from.at",
            "{at}"
        );
    }
    for path in [
        "rec/session.jsonl",
        "/rec/missing.jsonl",
        &format!("/{}", "r".repeat(400)),
    ] {
        let mut q = from("2026-11-16T03:04:05");
        q["words_from"]["record"] = json!(path);
        let e = refusal(&mut F::new(), &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", "words_from.record")
        );
    }
    let mut q = from("2026-11-16T03:04:05");
    q["words_from"]["line"] = json!(1);
    assert_eq!(refusal(&mut F::new(), &q).field, "words_from.line");
}
// spec: specs/ap/request.md#R8
#[test]
fn r8_words_of_a_record_are_verified_with_the_record_and_the_lines_second() {
    let mut f = F::new();
    let v = run(&mut f, &from("2026-11-16T03:04:05"));
    let p = applied_params(&f, "record_request");
    assert_eq!(
        (&p["words_verified"], &p["record"], &p["said_at"]),
        (&json!(true), &json!(PATH), &json!("2026-11-16T03:04:05Z"))
    );
    assert_eq!(v["words_verified"], true);
    assert_eq!(
        f.request(v["request"].as_str().unwrap())["words_verified"],
        true
    );
}
// spec: specs/ap/request.md#R9
#[test]
fn r9_typed_words_need_said_at_and_are_not_verified() {
    let mut f = F::new();
    let v = run(&mut f, &rec());
    let p = applied_params(&f, "record_request");
    assert_eq!(
        (
            &p["words"],
            &p["words_verified"],
            &p["said_at"],
            p.get("record")
        ),
        (
            &json!("do the thing"),
            &json!(false),
            &json!("2026-11-16T03:04:05Z"),
            None
        )
    );
    assert_eq!(v["words_verified"], false);
    let words = " line one\n\tline two\r\n ";
    let mut f = F::new();
    f.files
        .insert("/w/words.txt".into(), words.as_bytes().to_vec());
    let q = with(without(rec(), "words"), "words_file", json!("/w/words.txt"));
    run(&mut f, &q);
    assert_eq!(applied_params(&f, "record_request")["words"], words);
    f.files.insert("/w/binary".into(), vec![b'a', 0xff, 0xfe]);
    let e = refusal(&mut f, &with(q.clone(), "words_file", json!("/w/binary")));
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("words_not_utf8", "words_file")
    );
    for bad in ["w/words.txt", "/w/missing"] {
        let e = refusal(&mut f, &with(q.clone(), "words_file", json!(bad)));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", "words_file")
        );
    }
    for at in [
        "2026-11-16T03:04:05",
        "2026-11-16 03:04:05Z",
        "2026-11-16T03:04:05.1Z",
    ] {
        let e = refusal(&mut f, &with(rec(), "said_at", json!(at)));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", "said_at")
        );
    }
    assert_eq!(f.writes(), 1, "no refused words reached the store");
}
// spec: specs/ap/request.md#R10
#[test]
fn r10_words_are_never_changed_and_empty_words_are_refused_first() {
    let said = "  \"quoted\"\n\n가나다\t\u{7}\\ end  ";
    let mut f = F::new();
    let v = run(&mut f, &with(rec(), "words", json!(said)));
    assert_eq!(applied_params(&f, "record_request")["words"], said);
    assert_eq!(f.request(v["request"].as_str().unwrap())["words"], said);
    assert_eq!(v["words_bytes"], said.len());
    let mut f = F::new();
    let e = refusal(&mut f, &with(rec(), "words", json!("")));
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("words_empty", "words", None)
    );
    assert!(f.calls.is_empty());
    let mut f = F::new();
    f.files.insert("/w/empty".into(), vec![]);
    let e = refusal(
        &mut f,
        &with(without(rec(), "words"), "words_file", json!("/w/empty")),
    );
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("words_empty", "words_file")
    );
    assert!(f.calls.is_empty());
}
// spec: specs/ap/request.md#R11
#[test]
fn r11_words_over_the_bound_are_their_first_4000_bytes_and_truncated() {
    let cases = [
        ("x".repeat(30_000), 4_000, true),
        ("가".repeat(10_000), 3_999, true),
        ("\u{1}".repeat(12_000), 4_000, true),
        ("x".repeat(24_000), 24_000, false),
        ("\u{1}".repeat(8_000), 8_000, false),
        ("\u{1}".repeat(8_001), 4_000, true),
        (format!("{}가", "x".repeat(23_997)), 24_000, false),
    ];
    for (said, bytes, cut) in cases {
        let (head, len) = (said.chars().next().unwrap(), said.len());
        for typed in [true, false] {
            let mut f = F::new();
            let q = if typed {
                with(rec(), "words", json!(said))
            } else {
                let line = json!({"type": "user", "timestamp": "2026-11-16T04:00:00.5Z", "message": {"content": said}});
                f.files
                    .insert("/rec/long.jsonl".into(), format!("{line}\n").into_bytes());
                let mut q = from("2026-11-16T04:00:00");
                q["words_from"]["record"] = json!("/rec/long.jsonl");
                q
            };
            let v = run(&mut f, &q);
            let stored = applied_params(&f, "record_request")["words"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(
                (stored.len(), v["truncated"].clone()),
                (bytes, json!(cut)),
                "{head:?} {len}"
            );
            assert!(said.starts_with(&stored));
            assert_eq!(applied_params(&f, "record_request")["truncated"], cut);
            assert_eq!(v["words_bytes"], bytes);
        }
    }
}
// spec: specs/ap/request.md#R12
#[test]
fn r12_a_route_is_checked_before_anything_is_applied() {
    for (intent, code) in [
        ("none-such", "unknown_intent"),
        ("met", "intent_closed"),
        ("gone", "intent_closed"),
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &with(rec(), "route", json!({"intent": intent})));
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after, e.request),
            (code, "route.intent", None, None)
        );
        assert_eq!(f.names(), ["get intent"]);
    }
    for (aside, field) in [
        (json!(""), "route.aside"),
        (json!("r".repeat(601)), "route.aside"),
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &with(rec(), "route", json!({"aside": aside})));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", field)
        );
        assert!(f.calls.is_empty());
    }
    let mut f = F::new();
    run(
        &mut f,
        &with(rec(), "route", json!({"aside": "r".repeat(600)})),
    );
    assert_eq!(f.writes(), 2);
}
// spec: specs/ap/request.md#R13
#[test]
fn r13_record_applies_reads_back_then_routes() {
    let mut f = F::new();
    run(&mut f, &with(rec(), "route", json!({"intent": "open"})));
    assert_eq!(
        f.names(),
        [
            "get intent",
            "apply record_request",
            "get request",
            "apply route_request"
        ]
    );
    assert_eq!(f.calls[2].1["id"], oid("request", "rq3"));
    assert_eq!(
        applied_params(&f, "route_request"),
        json!({"request": oid("request", "rq3"), "intent": oid("intent", "open")})
    );
    // Other words read back: no route, and the refusal names the recorded request.
    let mut f = F::new();
    f.alter_words = true;
    let e = refusal(&mut f, &with(rec(), "route", json!({"aside": "a remark"})));
    assert_eq!(
        (e.code.as_str(), e.after, e.request.as_deref()),
        ("readback_differs", Some("record"), Some("rq3"))
    );
    assert_eq!(f.names(), ["apply record_request", "get request"]);
}
// spec: specs/ap/request.md#R14
#[test]
fn r14_a_refused_or_cut_route_is_finished_by_the_same_call_and_a_repeat_writes_nothing() {
    // The store refuses the route: the request stays recorded and unrouted.
    let mut f = F::new();
    let answer = json!({"status": "refused", "code": "submission_criteria", "failed": ["request_confirms_an_intent"]});
    f.refuse = Some(("route_request", answer));
    let q = with(rec(), "route", json!({"intent": "open"}));
    let e = refusal(&mut f, &q);
    assert_eq!(
        (
            e.code.as_str(),
            e.field.as_str(),
            e.after,
            e.request.as_deref()
        ),
        (
            "request_confirms_an_intent",
            "route",
            Some("record"),
            Some("rq3")
        )
    );
    assert_eq!(f.request("rq3")["route"], "unrouted");
    let v = run(&mut f, &with(rec(), "route", json!({"aside": "a remark"})));
    assert_eq!(
        (v["record"].as_str(), v["route"].as_str()),
        (Some("already_applied"), Some("applied"))
    );
    assert_eq!(
        (f.requests().len(), &f.request("rq3")["route"]),
        (1, &json!("aside"))
    );
    // A call cut after the record, run again.
    let mut f = F::new();
    f.cut = Some("route_request");
    assert_eq!(refusal(&mut f, &q).after, Some("record"));
    let v = run(&mut f, &q);
    assert_eq!(
        (v["record"].as_str(), v["route"].as_str()),
        (Some("already_applied"), Some("applied"))
    );
    // Repeated whole: `already_applied` for each, nothing written.
    let before = f.objects.clone();
    let again = run(&mut f, &q);
    assert_eq!(
        (
            again["record"].as_str(),
            again["route"].as_str(),
            &again["request"]
        ),
        (
            Some("already_applied"),
            Some("already_applied"),
            &v["request"]
        )
    );
    assert_eq!((f.objects == before, f.requests().len()), (true, 1));
    // Once the intent is closed the check before the record refuses the repeat.
    f.objects.get_mut("open").unwrap()["status"] = json!("met");
    let e = refusal(&mut f, &q);
    assert_eq!((e.code.as_str(), e.after), ("intent_closed", None));
}
// spec: specs/ap/request.md#R15
#[test]
fn r15_the_answer_of_record() {
    let mut f = F::new();
    let v = run(&mut f, &rec());
    assert_eq!(
        v,
        json!({"request": "rq3", "record": "applied", "route": null, "words_bytes": 12,
            "words_verified": false, "truncated": false})
    );
    let v = run(&mut f, &with(rec(), "route", json!({"aside": "a remark"})));
    assert_eq!(
        (v["record"].as_str(), v["route"].as_str()),
        (Some("already_applied"), Some("applied"))
    );
}
/// The record request `base` made a supersede of `old`, with the request id `m2`.
fn supersede(old: &Value, base: Value) -> Value {
    let q = with(with(base, "op", json!("supersede")), "request", old.clone());
    with(q, "request_id", json!("m2"))
}
// spec: specs/ap/request.md#R16
#[test]
fn r16_supersede_replaces_reads_back_and_keeps_the_route() {
    for (route, inherited) in [
        (
            Some(json!({"intent": "open"})),
            Some(("route_request", json!({"intent": oid("intent", "open")}))),
        ),
        (
            Some(json!({"aside": "a remark"})),
            Some(("set_request_aside", json!({"reason": "a remark"}))),
        ),
        (None, None),
    ] {
        let mut f = F::new();
        let first = run(&mut f, &route.map_or(rec(), |r| with(rec(), "route", r)));
        let old = first["request"].clone();
        let v = run(&mut f, &supersede(&old, from("2026-11-16T03:04:05")));
        assert_eq!(v["supersedes"], old);
        assert_eq!(
            (v["record"].as_str(), v["words_verified"].as_bool()),
            (Some("applied"), Some(true))
        );
        let p = applied_params(&f, "supersede_request");
        assert_eq!(
            (&p["request"], &p["words_verified"]),
            (&oid("request", old.as_str().unwrap()), &json!(true))
        );
        let new = v["request"].as_str().unwrap();
        assert_eq!(
            f.request(new)["supersedes"],
            oid("request", old.as_str().unwrap())
        );
        let tail: Vec<_> = f.names().into_iter().skip(f.names().len() - 3).collect();
        match inherited {
            Some((action, mut p)) => {
                assert_eq!(
                    tail,
                    [
                        "get request",
                        "get request",
                        format!("apply {action}").as_str()
                    ]
                );
                p["request"] = oid("request", new);
                assert_eq!(applied_params(&f, action), p);
                assert_eq!(v["route"], "applied");
            }
            None => {
                assert_eq!(f.request(new)["route"], "unrouted");
                assert_eq!(v["route"], Value::Null);
            }
        }
        // The same supersede again.
        let n = f.objects.len();
        let again = run(&mut f, &supersede(&old, from("2026-11-16T03:04:05")));
        assert_eq!(
            (again["record"].as_str(), &again["request"]),
            (Some("already_applied"), &v["request"])
        );
        assert_eq!(f.objects.len(), n);
    }
    // A given route replaces the old one.
    let mut f = F::new();
    let old = run(&mut f, &with(rec(), "route", json!({"intent": "open"})))["request"].clone();
    run(
        &mut f,
        &supersede(&old, with(rec(), "route", json!({"aside": "now a remark"}))),
    );
    assert_eq!(f.names().last().unwrap(), "apply set_request_aside");
}
// spec: specs/ap/request.md#R17
#[test]
fn r17_an_inherited_route_to_a_closed_intent_is_refused_after_the_record() {
    let mut f = F::new();
    let old = run(&mut f, &with(rec(), "route", json!({"intent": "open"})))["request"].clone();
    f.objects.get_mut("open").unwrap()["status"] = json!("retired");
    let e = refusal(&mut f, &supersede(&old, rec()));
    let new = f
        .requests()
        .iter()
        .find(|r| r.1["status"] == "current")
        .unwrap()
        .0
        .clone();
    assert_eq!(
        (
            e.code.as_str(),
            e.field.as_str(),
            e.after,
            e.request.as_deref()
        ),
        ("intent_closed", "route", Some("record"), Some(new.as_str()))
    );
    let v = run(
        &mut f,
        &supersede(
            &old,
            with(rec(), "route", json!({"aside": "the intent was retired"})),
        ),
    );
    assert_eq!(
        (v["record"].as_str(), v["route"].as_str()),
        (Some("already_applied"), Some("applied"))
    );
    assert_eq!(f.request(&new)["route"], "aside");
}
// spec: specs/ap/request.md#R18
#[test]
fn r18_route_routes_a_request_to_an_intent_or_aside() {
    let mut f = F::new();
    let key = run(&mut f, &rec())["request"].clone();
    let q = json!({"op": "route", "request_id": "m9", "agent": "lead", "session": "s1", "request": key});
    let e = refusal(&mut f, &with(q.clone(), "intent", json!("none-such")));
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("unknown_intent", "intent", None)
    );
    assert_eq!(f.writes(), 1);
    let v = run(&mut f, &with(q.clone(), "intent", json!("open")));
    assert_eq!(v, json!({"request": key, "route": "applied"}));
    assert_eq!(f.calls.last().unwrap().1["request_id"], "m9-route");
    let v = run(
        &mut f,
        &with(
            with(q.clone(), "request_id", json!("m10")),
            "aside",
            json!("a remark"),
        ),
    );
    assert_eq!(v["route"], "applied");
    let o = f.request(key.as_str().unwrap());
    assert_eq!(
        (&o["route"], &o["route_reason"], o.get("intent")),
        (&json!("aside"), &json!("a remark"), None)
    );
    let e = refusal(&mut f, &with(q, "aside", json!("r".repeat(601))));
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("invalid_input", "aside")
    );
}
fn many(f: &mut F, n: usize, topic: &str) {
    for i in 0..n {
        let q = json!({"op": "record", "request_id": format!("{topic}-{i}"), "agent": "lead", "session": "s1",
            "topic": topic, "kind": "order", "reading": "r", "words": format!("w{i}"),
            "said_at": format!("2026-11-16T{:02}:{:02}:00Z", i / 60, i % 60)});
        run(f, &q);
    }
}
// spec: specs/ap/request.md#R19
#[test]
fn r19_list_searches_every_page_qualified_and_writes_nothing() {
    let mut f = F::new();
    many(&mut f, 250, "big");
    many(&mut f, 3, "small");
    f.calls.clear();
    let v = run(&mut f, &json!({"op": "list", "topic": "big"}));
    assert_eq!(v["requests"].as_array().unwrap().len(), 250);
    assert_eq!(f.names(), ["search request"; 17]);
    let filter = &f.calls[0].1["where"]["all"];
    assert_eq!(
        *filter,
        json!([{"eq": ["topic", "big"]}, {"eq": ["status", "current"]}])
    );
    assert_eq!(
        (&f.calls[1].1["continuation"], &f.calls[2].1["continuation"]),
        (&json!("15"), &json!("30"))
    );
    f.calls.clear();
    run(&mut f, &json!({"op": "list"}));
    assert_eq!(
        f.calls[0].1["where"]["all"],
        json!([{"eq": ["route", "unrouted"]}, {"eq": ["status", "current"]}])
    );
    assert_eq!(f.writes(), 0);
    f.unqualified = true;
    let e = refusal(&mut f, &json!({"op": "list", "topic": "small"}));
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("store_answer_unqualified", "list", None)
    );
}
// spec: specs/ap/request.md#R19
#[test]
fn r19_list_pages_fit_the_bound_of_one_answer_with_the_largest_heads() {
    let mut f = F::new();
    f.heavy = true;
    many(&mut f, 40, "long");
    f.calls.clear();
    let v = run(&mut f, &json!({"op": "list", "topic": "long"}));
    assert_eq!(v["requests"].as_array().unwrap().len(), 40);
    let v = run(&mut f, &json!({"op": "list"}));
    assert_eq!(v["requests"].as_array().unwrap().len(), 40);
    assert!(
        f.calls
            .iter()
            .all(|c| c.1["limit"].as_u64().unwrap() as usize * HEAD + 1_000 <= LIMIT)
    );
}
// spec: specs/ap/request.md#R20
#[test]
fn r20_list_answers_the_requests_as_stored_in_their_order() {
    let mut f = F::new();
    assert_eq!(
        run(&mut f, &json!({"op": "list", "topic": "none"})),
        json!({"requests": []})
    );
    let reading = "a | pipe\nand a \\ backslash";
    let q = |id: &str, at: &str| {
        json!({"op": "record", "request_id": id, "agent": "lead", "session": "s1",
        "topic": "walk/one", "kind": "decision", "reading": reading, "words": format!("words {id}\n"), "said_at": at})
    };
    run(&mut f, &q("b", "2026-11-16T03:00:00Z"));
    run(&mut f, &q("a", "2026-11-16T03:00:00Z"));
    run(
        &mut f,
        &with(
            q("c", "2026-11-16T02:00:00Z"),
            "route",
            json!({"intent": "open"}),
        ),
    );
    run(
        &mut f,
        &with(
            q("d", "2026-11-16T04:00:00Z"),
            "route",
            json!({"aside": "a remark"}),
        ),
    );
    run(
        &mut f,
        &with(q("e", "2026-11-16T01:00:00Z"), "topic", json!("other")),
    );
    let v = run(&mut f, &json!({"op": "list", "topic": "walk/one"}));
    let order: Vec<_> = v["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["words"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["words d\n", "words a\n", "words b\n", "words c\n"]);
    let keys: Vec<_> = v["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["request"].as_str().unwrap())
        .collect();
    assert!(keys[1] > keys[2], "the same second: by key, newest first");
    assert_eq!(
        v["requests"][0],
        json!({"request": keys[0], "topic": "walk/one", "kind": "decision", "said_at": "2026-11-16T04:00:00Z",
            "words_verified": false, "truncated": false, "route": "aside", "intent": null,
            "route_reason": "a remark", "reading": reading, "words": "words d\n"})
    );
    assert_eq!(
        (
            &v["requests"][3]["intent"],
            &v["requests"][3]["route_reason"]
        ),
        (&json!("open"), &Value::Null)
    );
    let v = run(&mut f, &json!({"op": "list"}));
    let order: Vec<_> = v["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["words"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["words e\n", "words b\n", "words a\n"]);
}
// spec: specs/ap/request.md#R21
#[test]
fn r21_list_text_prints_one_block_per_request() {
    let row = |route: &str, intent: Value, reason: Value, verified: bool, truncated: bool| {
        json!({"request": "k", "topic": "walk/one", "kind": "order", "said_at": "2026-11-16T03:04:05Z",
            "words_verified": verified, "truncated": truncated, "route": route, "intent": intent,
            "route_reason": reason, "reading": "the reading", "words": "two\nlines "})
    };
    let v = json!({"requests": [
        row("unrouted", Value::Null, Value::Null, true, false),
        row("intent", json!("open"), Value::Null, false, true),
        row("aside", Value::Null, json!("a remark"), true, false),
    ]});
    assert_eq!(
        text(&v),
        "2026-11-16T03:04:05Z order walk/one -> unrouted\nthe reading\ntwo\nlines \n\n\
         2026-11-16T03:04:05Z order walk/one [not verified] [truncated] -> intent open\nthe reading\ntwo\nlines \n\n\
         2026-11-16T03:04:05Z order walk/one -> aside (a remark)\nthe reading\ntwo\nlines \n\n"
    );
    assert_eq!(text(&json!({"requests": []})), "");
}
// spec: specs/ap/request.md#R22
#[test]
fn r22_store_busy_is_asked_again_five_times_and_nothing_else_is() {
    let mut f = F::new();
    f.busy = 5;
    run(&mut f, &rec());
    assert_eq!((f.pauses, f.writes()), (5, 6));
    let mut f = F::new();
    f.busy = 6;
    let e = refusal(&mut f, &rec());
    assert_eq!(
        (e.code.as_str(), e.after, f.pauses, f.requests().len()),
        ("store_busy", None, 5, 0)
    );
    // Another refusal is answered at once.
    let mut f = F::new();
    let e = refusal(
        &mut f,
        &with(rec(), "route", json!({"intent": "none-such"})),
    );
    assert_eq!(
        (e.code.as_str(), f.calls.len(), f.pauses),
        ("unknown_intent", 1, 0)
    );
    let mut f = F::new();
    f.cut = Some("record_request");
    assert_eq!(refusal(&mut f, &rec()).code, "process_failed");
    assert_eq!(f.calls.len(), 1);
}
// spec: specs/ap/request.md#R23
#[test]
fn r23_only_the_four_actions_write() {
    let mut f = F::new();
    let first = run(
        &mut f,
        &with(
            from("2026-11-16T03:04:05"),
            "route",
            json!({"intent": "open"}),
        ),
    );
    run(&mut f, &supersede(&first["request"], rec()));
    let q = json!({"op": "route", "request_id": "m3", "agent": "lead", "session": "s1",
        "request": run(&mut f, &json!({"op": "list", "topic": "walk/one"}))["requests"][0]["request"], "aside": "r"});
    run(&mut f, &q);
    let kinds: Vec<_> = f.calls.iter().map(|c| c.0[0].as_str()).collect();
    assert!(
        kinds
            .iter()
            .all(|k| ["action", "get", "search"].contains(k)),
        "{kinds:?}"
    );
    let actions: Vec<_> = f
        .calls
        .iter()
        .filter(|c| c.0[0] == "action")
        .map(|c| c.1["action_type"].as_str().unwrap())
        .collect();
    assert!(actions.iter().all(|a| ACTIONS.contains(a)), "{actions:?}");
    assert_eq!(actions.len(), 5);
}
#[test]
fn an_unreadable_store_answer_is_a_refusal_in_the_one_form() {
    struct Down;
    impl System for Down {
        fn call(&mut self, _: &str, _: &[String], _: &Value, _: u64) -> Answer {
            Ok(Reply {
                success: false,
                stdout: vec![],
                bytes: b"not json".to_vec(),
            })
        }
    }
    let e = execute(&mut Down, None, &rec()).err().unwrap();
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("store_answer_unreadable", "record", None)
    );
}
