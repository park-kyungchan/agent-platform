//! `ap request` (README "One request", `specs/ap/request.md`) against a fake of `ont` that answers as
//! the engine does, and a session record of invented lines (`fixtures/request/session.jsonl`) under the
//! fake's own home.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::{
    host::{Answer, Failure, Reply, System},
    request::{Refusal, execute, hook, is_hook, text},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const RECORD: &[u8] = include_bytes!("fixtures/request/session.jsonl");
/// The fake's home and its directory of session records (R25).
const HOME: &str = "/home/hearer";
const DIR: &str = "/home/hearer/.claude/projects/-w-work-walk/";
const PATH: &str = "/home/hearer/.claude/projects/-w-work-walk/session.jsonl";
/// The fake's work root.
const WORK: &str = "/w/work/";
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
    /// How many times each object was written (its observations).
    written: BTreeMap<String, usize>,
    /// The port's home and work root: never the real ones, so no result depends on who runs the tests.
    home: Option<std::path::PathBuf>,
    work: std::path::PathBuf,
    /// How many files were read.
    reads: std::cell::Cell<usize>,
    /// The port's wall clock: fixed, and no later than the store's `NOW`.
    wall: std::time::SystemTime,
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
            written: BTreeMap::new(),
            home: Some(HOME.into()),
            work: WORK.into(),
            reads: std::cell::Cell::new(0),
            wall: std::time::UNIX_EPOCH
                + std::time::Duration::from_secs(ap::watch::epoch(NOW).unwrap()),
        };
        f.files.insert(PATH.into(), RECORD.to_vec());
        for (key, status) in [("open", "draft"), ("met", "met"), ("gone", "retired")] {
            f.objects.insert(
                key.into(),
                json!({"object_type": "intent", "status": status}),
            );
        }
        // Schema 11: a unit and a finding, keyed by their native key.
        f.objects.insert(
            "u1".into(),
            json!({"object_type": "unit", "state": "ready"}),
        );
        f.objects.insert(
            "f1".into(),
            json!({"object_type": "finding", "state": "open"}),
        );
        // Schema 24: a host is a subject too.
        f.objects
            .insert("h1".into(), json!({"object_type": "host"}));
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
        // Schema 24: the subject is any object of an implementing type; a closed intent is not refused.
        if action == "route_request" {
            let kind = p["subject"]["object_type"].as_str().unwrap_or_default();
            if !self.is(&p["subject"], kind, |_| true) {
                failed.push("unknown_subject");
            }
        }
        if action == "set_request_aside" && p["reason"] == "" {
            failed.push("reason_empty");
        }
        failed
    }
    fn create(&mut self, p: &Value) -> String {
        let key = format!("rq{}", self.requests().len() + 3);
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
            // The rules of schema 24: `route_request` sets the subject, `set_request_aside` the reason; both clear
            // `intent`, `unit` and `finding`, and `set_request_aside` clears the subject.
            _ => {
                let o = self.objects.get_mut(&target).unwrap();
                let o = o.as_object_mut().unwrap();
                for k in ["intent", "unit", "finding", "subject"] {
                    o.remove(k);
                }
                if action == "route_request" {
                    o.insert("route".into(), json!("subject"));
                    o.insert("subject".into(), p["subject"].clone());
                } else {
                    o.insert("route".into(), json!("aside"));
                    o.insert("route_reason".into(), p["reason"].clone());
                }
                vec![change(&target, "modification")]
            }
        };
        for e in &edits {
            let key = e["id"]["native_key"].as_str().unwrap().to_owned();
            *self.written.entry(key).or_insert(0) += 1;
        }
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
        let key = id["native_key"].as_str().unwrap();
        let o = &self.objects[key];
        // As the engine answers: every observation of the object, the last 100 at most, the head last.
        let n = self.written.get(key).copied().unwrap_or(1).max(1);
        let observations: Vec<Value> = (n.saturating_sub(99).max(1)..=n)
            .map(|i| json!({"revision": format!("r#{i}"), "change": "modification", "fields": Self::fields(o)}))
            .collect();
        ok(json!({"objects": [{"id": id, "head": format!("r#{n}"), "observations": observations}]}))
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
        // As the engine answers: a filter on a property that a scanned object of the type lacks is unqualified.
        let lacks = self.objects.values().any(|o| {
            o["object_type"] == body["object_type"]
                && all
                    .iter()
                    .any(|c| o.get(c["eq"][0].as_str().unwrap()).is_none())
        });
        ok(
            json!({"basis": {"qualified": !(self.unqualified || lacks)}, "objects": found[from..to],
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
        self.reads.set(self.reads.get() + 1);
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
    fn pause(&mut self, _: std::time::Duration) {
        self.pauses += 1;
    }
    fn home(&self) -> Option<std::path::PathBuf> {
        self.home.clone()
    }
    fn work_root(&self) -> std::path::PathBuf {
        self.work.clone()
    }
    fn wall(&self) -> std::time::SystemTime {
        self.wall
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
    let (status, out, err) = ap_raw(args, stdin);
    let err = serde_json::from_slice(&err).unwrap_or(Value::Null);
    (status, out, err)
}
/// The built binary's status, standard output and standard error as bytes.
fn ap_raw(args: &[&str], stdin: &[u8]) -> (Option<i32>, Vec<u8>, Vec<u8>) {
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
    (out.status.code(), out.stdout, out.stderr)
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
        (
            with(rec(), "route", json!({"unit": "u1", "finding": "f1"})),
            "route",
        ),
        (with(rec(), "route", json!({"unit": ""})), "route.unit"),
        (with(rec(), "route", json!({"finding": 1})), "route.finding"),
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
        (route.clone(), "route"),
        (
            with(
                with(route.clone(), "intent", json!("open")),
                "aside",
                json!("r"),
            ),
            "route",
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
        &format!("{DIR}missing.jsonl"),
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
// spec: specs/ap/request.md#R9
#[test]
fn r9_a_words_file_over_one_mebibyte_is_input_too_large_on_the_bytes_read() {
    let q = with(without(rec(), "words"), "words_file", json!("/w/big.txt"));
    let mut f = F::new();
    f.files.insert("/w/big.txt".into(), vec![b'a'; LIMIT + 1]);
    let e = refusal(&mut f, &q);
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after, &e.request),
        ("input_too_large", "words_file", None, &None)
    );
    assert!(f.calls.is_empty());
    // The bound is on the bytes: exactly 1 MiB is read, then cut as every long words are (R11).
    f.files.insert("/w/big.txt".into(), vec![b'a'; LIMIT]);
    let v = run(&mut f, &q);
    assert_eq!(
        (v["truncated"].clone(), v["words_bytes"].clone()),
        (json!(true), json!(4000))
    );
    // Words read from a session record are read whole.
    let mut f = F::new();
    let line = |t: &str| json!({"type": "user", "timestamp": t, "message": {"content": "hello"}});
    let mut record = vec![];
    for i in 0..30_000 {
        record.extend_from_slice(
            format!("{}\n", line(&format!("2026-11-16T00:00:{:02}.0Z", i % 3))).as_bytes(),
        );
    }
    assert!(record.len() > LIMIT);
    record.extend_from_slice(format!("{}\n", line("2026-11-16T03:04:05.1Z")).as_bytes());
    f.files.insert(PATH.into(), record);
    assert_eq!(
        run(&mut f, &from("2026-11-16T03:04:05"))["words_verified"],
        true
    );
}
// spec: specs/ap/request.md#R13
#[test]
fn r13_a_request_routed_often_reads_back_and_can_be_superseded() {
    // Words of 24,000 quotes: 48,000 bytes as JSON, so 25 observations of the head pass 1 MiB.
    let words = "\"".repeat(24_000);
    let mut f = F::new();
    let q = with(rec(), "words", json!(words));
    let first = run(&mut f, &q);
    let key = first["request"].as_str().unwrap().to_owned();
    for i in 0..25 {
        let route = json!({"op": "route", "request_id": format!("r{i}"), "agent": "lead", "session": "s1",
            "request": key, "aside": format!("reason {i}")});
        run(&mut f, &route);
    }
    // `ont get` of it now answers more than the verb's bound: the read is of the head only.
    let get = f.call(
        "ont",
        &["get".to_owned()],
        &json!({"id": oid("request", &key)}),
        1000,
    );
    assert!(get.is_err(), "an answer over 1 MiB is no answer");
    f.calls.clear();
    // The call repeated whole: the record is `already_applied` and reads back.
    let again = run(&mut f, &q);
    assert_eq!(
        (again["record"].as_str(), again["request"].as_str()),
        (Some("already_applied"), Some(key.as_str()))
    );
    // A search of the topic and time, to the end, and no `get` of the request.
    let last = f.calls.last().unwrap();
    assert_eq!(
        last.1["where"],
        json!({"all": [{"eq": ["topic", "walk/one"]}, {"eq": ["said_at", "2026-11-16T03:04:05Z"]}]})
    );
    assert!(f.names().iter().all(|n| n != "get request"));
    // Superseded: the route it had (aside, with its reason) is read from the superseded rows.
    let sup = with(with(rec(), "op", json!("supersede")), "request", json!(key));
    let sup = with(
        with(sup, "request_id", json!("m2")),
        "words",
        json!("corrected"),
    );
    let v = run(&mut f, &sup);
    assert_eq!(
        (v["record"].as_str(), v["route"].as_str()),
        (Some("applied"), Some("applied"))
    );
    let new = v["request"].as_str().unwrap();
    assert_eq!(f.request(new)["route_reason"], "reason 24");
    assert!(f.names().iter().all(|n| n != "get request"));
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
                let long = format!("{DIR}long.jsonl");
                f.files
                    .insert(long.clone(), format!("{line}\n").into_bytes());
                let mut q = from("2026-11-16T04:00:00");
                q["words_from"]["record"] = json!(long);
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
fn r13_read_back_is_qualified_when_a_stored_request_has_no_session() {
    let mut f = F::new();
    // A request recorded without the optional `session`: a filter on `session` would answer unqualified.
    f.objects.insert(
        "nosession".into(),
        json!({"object_type": "request", "status": "current", "route": "unrouted",
            "topic": "walk/other", "said_at": "2026-11-15T01:02:03Z", "words": "older"}),
    );
    let v = run(&mut f, &from("2026-11-16T03:04:05"));
    assert_eq!(v["record"], "applied");
    assert_eq!(v["words_verified"], true);
    let last = f.calls.last().unwrap();
    assert_eq!(last.1["object_type"], "request");
    assert_eq!(
        last.1["where"],
        json!({"all": [{"eq": ["topic", "walk/one"]}, {"eq": ["said_at", "2026-11-16T03:04:05Z"]}]})
    );
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
            "search request",
            "apply route_request"
        ]
    );
    // The read-back is a search of the call's topic and time; the row of the key is the one read.
    assert_eq!(f.calls[2].1["object_type"], "request");
    assert_eq!(
        f.calls[2].1["where"],
        json!({"all": [{"eq": ["topic", "walk/one"]}, {"eq": ["said_at", "2026-11-16T03:04:05Z"]}]})
    );
    assert_eq!(
        applied_params(&f, "route_request"),
        json!({"request": oid("request", "rq3"), "subject": oid("intent", "open")})
    );
    // Other words read back: no route, and the refusal names the recorded request.
    let mut f = F::new();
    f.alter_words = true;
    let e = refusal(&mut f, &with(rec(), "route", json!({"aside": "a remark"})));
    assert_eq!(
        (e.code.as_str(), e.after, e.request.as_deref()),
        ("readback_differs", Some("record"), Some("rq3"))
    );
    assert_eq!(f.names(), ["apply record_request", "search request"]);
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
            Some(("route_request", json!({"subject": oid("intent", "open")}))),
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
                // R17: an inherited intent is read once more (the check of R12) before `route_request`.
                let middle = if action == "route_request" {
                    "get intent"
                } else {
                    "search request"
                };
                assert_eq!(
                    tail,
                    ["search request", middle, format!("apply {action}").as_str()]
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
            "unit": null, "finding": null, "host": null, "route_reason": "a remark", "reading": reading, "words": "words d\n"})
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
    let row = |route: &str, key: Value, reason: Value, verified: bool, truncated: bool| {
        let mut r = json!({"request": "k", "topic": "walk/one", "kind": "order", "said_at": "2026-11-16T03:04:05Z",
            "words_verified": verified, "truncated": truncated, "route": route, "intent": null,
            "unit": null, "finding": null, "host": null, "route_reason": reason, "reading": "the reading", "words": "two\nlines "});
        if ["intent", "unit", "finding", "host"].contains(&route) {
            r[route] = key;
        }
        r
    };
    let v = json!({"requests": [
        row("unrouted", Value::Null, Value::Null, true, false),
        row("intent", json!("open"), Value::Null, false, true),
        row("aside", Value::Null, json!("a remark"), true, false),
        row("unit", json!("u1"), Value::Null, true, false),
        row("finding", json!("f1"), Value::Null, true, false),
    ]});
    assert_eq!(
        text(&v),
        "2026-11-16T03:04:05Z order walk/one -> unrouted\nthe reading\ntwo\nlines \n\n\
         2026-11-16T03:04:05Z order walk/one [not verified] [truncated] -> intent open\nthe reading\ntwo\nlines \n\n\
         2026-11-16T03:04:05Z order walk/one -> aside (a remark)\nthe reading\ntwo\nlines \n\n\
         2026-11-16T03:04:05Z order walk/one -> unit u1\nthe reading\ntwo\nlines \n\n\
         2026-11-16T03:04:05Z order walk/one -> finding f1\nthe reading\ntwo\nlines \n\n"
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
/// A request of `rec()` (`rq3`) routed to `route`/`member` = `value` as another writer may have left it.
fn routed_by_hand(f: &mut F, route: &str, value: Value) -> Value {
    let key = run(f, &rec())["request"].clone();
    let o = f.objects.get_mut(key.as_str().unwrap()).unwrap();
    o["route"] = json!(route);
    if !value.is_null() {
        o[route] = value;
    }
    key
}
// spec: specs/ap/request.md#R16
#[test]
fn r16_an_inherited_route_to_a_unit_or_a_finding_is_applied_again() {
    // A route written before schema 24 (`unit` or `finding`) is passed on as the `subject` of `route_request`.
    for route in ["unit", "finding"] {
        let action = "route_request";
        let mut f = F::new();
        let target = oid(route, &format!("{}1", &route[..1]));
        let old = routed_by_hand(&mut f, route, target.clone());
        let v = run(&mut f, &supersede(&old, from("2026-11-16T03:04:05")));
        let new = v["request"].as_str().unwrap();
        assert_eq!(v["route"], "applied", "{route}");
        assert_eq!(f.names().last().unwrap(), &format!("apply {action}"));
        assert_eq!(
            applied_params(&f, action),
            json!({"request": oid("request", new), "subject": target})
        );
        assert_eq!(f.calls.last().unwrap().1["request_id"], "m2-route");
        assert_eq!(
            (&f.request(new)["route"], &f.request(new)["subject"]),
            (&json!("subject"), &target)
        );
        // Repeated whole: nothing written.
        let n = f.objects.clone();
        let again = run(&mut f, &supersede(&old, from("2026-11-16T03:04:05")));
        assert_eq!(again["route"], "already_applied");
        assert!(f.objects == n);
    }
}
// spec: specs/ap/request.md#R16
#[test]
fn r16_an_inherited_subject_is_passed_on_whatever_its_type() {
    for (kind, key) in [
        ("intent", "open"),
        ("unit", "u1"),
        ("finding", "f1"),
        ("host", "h1"),
    ] {
        let mut f = F::new();
        let target = oid(kind, key);
        let old = routed_by_hand(&mut f, "subject", Value::Null);
        f.objects.get_mut(old.as_str().unwrap()).unwrap()["subject"] = target.clone();
        let v = run(&mut f, &supersede(&old, from("2026-11-16T03:04:05")));
        let new = v["request"].as_str().unwrap();
        assert_eq!(v["route"], "applied", "{kind}");
        assert_eq!(
            applied_params(&f, "route_request"),
            json!({"request": oid("request", new), "subject": target})
        );
        assert_eq!(f.request(new)["subject"], target);
    }
}
// spec: specs/ap/request.md#R18
#[test]
fn r18_each_route_applies_route_request() {
    for (member, key) in [("intent", "open"), ("unit", "u1"), ("finding", "f1")] {
        let mut f = F::new();
        let request = run(&mut f, &rec())["request"].clone();
        let q = json!({"op": "route", "request_id": "m9", "agent": "lead", "session": "s1", "request": request, member: key});
        let v = run(&mut f, &q);
        assert_eq!(
            v,
            json!({"request": request, "route": "applied"}),
            "{member}"
        );
        // The action is `route_request`, and its `subject` is the object of that type and key.
        assert_eq!(
            applied_params(&f, "route_request"),
            json!({"request": oid("request", request.as_str().unwrap()), "subject": oid(member, key)})
        );
        let stored = f.request(request.as_str().unwrap());
        assert_eq!(
            (&stored["route"], &stored["subject"]),
            (&json!("subject"), &oid(member, key))
        );
        assert!(stored.get("intent").is_none() && stored.get("unit").is_none());
    }
}
// spec: specs/ap/request.md#R18
#[test]
fn r18_route_and_the_member_route_take_a_unit_or_a_finding() {
    for (route, key, action) in [
        ("unit", "u1", "route_request"),
        ("finding", "f1", "route_request"),
    ] {
        let mut f = F::new();
        let request = run(&mut f, &rec())["request"].clone();
        let q = json!({"op": "route", "request_id": "m9", "agent": "lead", "session": "s1", "request": request, route: key});
        let v = run(&mut f, &q);
        assert_eq!(v, json!({"request": request, "route": "applied"}));
        assert_eq!(
            applied_params(&f, action),
            json!({"request": oid("request", request.as_str().unwrap()), "subject": oid(route, key)})
        );
        assert_eq!(f.calls.last().unwrap().1["request_id"], "m9-route");
        // An unknown key: the store's refusal, passed on, with nothing checked before.
        let e = refusal(
            &mut f,
            &with(
                with(q, "request_id", json!("m10")),
                route,
                json!("none-such"),
            ),
        );
        let code = "unknown_subject".to_owned();
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            (code.as_str(), "route", None)
        );
        // The member `route` of `record`: no read before the record; a refusal after it says so.
        let mut f = F::new();
        let v = run(&mut f, &with(rec(), "route", json!({route: key})));
        assert_eq!(v["route"], "applied");
        assert_eq!(
            f.names(),
            [
                "apply record_request",
                "search request",
                &format!("apply {action}")
            ]
        );
        assert_eq!(f.request("rq3")["subject"], oid(route, key));
        let mut f = F::new();
        let e = refusal(&mut f, &with(rec(), "route", json!({route: "none-such"})));
        assert_eq!(
            (e.code.as_str(), e.after, e.request.as_deref()),
            (code.as_str(), Some("record"), Some("rq3"))
        );
    }
}
// spec: specs/ap/request.md#R24
#[test]
fn r24_a_route_the_verb_does_not_know_is_refused_never_read_as_unrouted() {
    let mut f = F::new();
    let old = routed_by_hand(&mut f, "elsewhere", Value::Null);
    let e = refusal(&mut f, &supersede(&old, from("2026-11-16T03:04:05")));
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
            &e.detail,
            e.after,
            e.request.as_deref()
        ),
        (
            "route_unknown",
            "supersedes",
            &json!("elsewhere"),
            Some("supersede"),
            Some(new.as_str())
        )
    );
    assert_eq!(f.names().last().unwrap(), "search request");
    // The same call with a route finishes it.
    let q = with(
        from("2026-11-16T03:04:05"),
        "route",
        json!({"aside": "routed by hand"}),
    );
    let v = run(&mut f, &supersede(&old, q));
    assert_eq!(
        (v["record"].as_str(), v["route"].as_str()),
        (Some("already_applied"), Some("applied"))
    );
    assert_eq!(f.request(&new)["route"], "aside");
}
// spec: specs/ap/request.md#R20
#[test]
fn r20_list_names_the_unit_or_finding_a_request_is_routed_to() {
    let mut f = F::new();
    let q = |id: &str, at: &str, route: Value| {
        let q = json!({"op": "record", "request_id": id, "agent": "lead", "session": "s1",
            "topic": "walk/one", "kind": "order", "reading": "r", "words": id, "said_at": at});
        with(q, "route", route)
    };
    run(
        &mut f,
        &q("a", "2026-11-16T03:00:00Z", json!({"unit": "u1"})),
    );
    run(
        &mut f,
        &q("b", "2026-11-16T02:00:00Z", json!({"finding": "f1"})),
    );
    let v = run(&mut f, &json!({"op": "list", "topic": "walk/one"}));
    let pick = |r: &Value| {
        (
            r["route"].clone(),
            r["intent"].clone(),
            r["unit"].clone(),
            r["finding"].clone(),
        )
    };
    assert_eq!(
        pick(&v["requests"][0]),
        (json!("unit"), Value::Null, json!("u1"), Value::Null)
    );
    assert_eq!(
        pick(&v["requests"][1]),
        (json!("finding"), Value::Null, Value::Null, json!("f1"))
    );
    assert!(text(&v).contains(" -> unit u1\n") && text(&v).contains(" -> finding f1\n"));
}
// spec: specs/ap/request.md#R20
#[test]
fn r20_list_names_a_stored_subject_by_its_type_and_a_stored_route_as_it_is() {
    let mut f = F::new();
    // A route written before schema 24 is listed as stored; a route `subject` by the subject's type.
    let mut routed = |id: &str, route: &str, member: &str, to: Value| {
        let key = run(&mut f, &with(rec(), "request_id", json!(id)))["request"].clone();
        let o = f.objects.get_mut(key.as_str().unwrap()).unwrap();
        o["route"] = json!(route);
        o[member] = to;
        key
    };
    let old = routed("m1", "unit", "unit", oid("unit", "u1"));
    let host = routed("m2", "subject", "subject", oid("host", "h1"));
    let intent = routed("m3", "subject", "subject", oid("intent", "open"));
    let v = run(&mut f, &json!({"op": "list", "topic": "walk/one"}));
    let row = |k: &Value| {
        v["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["request"] == *k)
            .unwrap()
            .clone()
    };
    let pick = |r: Value| {
        [
            &r["route"],
            &r["intent"],
            &r["unit"],
            &r["finding"],
            &r["host"],
        ]
        .map(Value::clone)
    };
    let n = Value::Null;
    assert_eq!(
        pick(row(&old)),
        [json!("unit"), n.clone(), json!("u1"), n.clone(), n.clone()]
    );
    assert_eq!(
        pick(row(&host)),
        [json!("host"), n.clone(), n.clone(), n.clone(), json!("h1")]
    );
    assert_eq!(
        pick(row(&intent)),
        [json!("intent"), json!("open"), n.clone(), n.clone(), n]
    );
    let shown = text(&v);
    assert!(
        shown.contains(" -> host h1\n")
            && shown.contains(" -> intent open\n")
            && shown.contains(" -> unit u1\n")
    );
}

/// A line of a session record: a prompt the user typed at `t` (`origin.kind` `human`).
fn typed(t: &str, words: &str) -> Value {
    json!({"type": "user", "timestamp": t, "sessionId": "sess-1", "origin": {"kind": "human"},
        "message": {"role": "user", "content": words}})
}
/// A line of an answer at `t`.
fn answer(t: &str) -> Value {
    json!({"type": "assistant", "timestamp": t, "message": {"role": "assistant", "content": "an answer"}})
}
fn lines(v: &[Value]) -> Vec<u8> {
    v.iter()
        .map(|l| format!("{l}\n"))
        .collect::<String>()
        .into_bytes()
}
/// The record the hook's input names, under the fake's home.
fn hooked() -> String {
    format!("{DIR}hooked.jsonl")
}
/// A fake whose record `hooked()` holds `record`.
fn holding(record: &[Value]) -> F {
    let mut f = F::new();
    f.files.insert(hooked(), lines(record));
    f
}
/// The input of `UserPromptSubmit` for `prompt`, with members of the CLI the verb does not read.
fn submitted(prompt: &str) -> Value {
    json!({"hook_event_name": "UserPromptSubmit", "session_id": "sess-1", "transcript_path": hooked(),
        "cwd": "/w/work/walk-topic/wt/lifecycle-platform", "permission_mode": "default", "prompt": prompt})
}
fn stopped(session: &str) -> Value {
    json!({"hook_event_name": "Stop", "session_id": session, "transcript_path": hooked(),
        "cwd": "/w/work/walk-topic", "stop_hook_active": false})
}
/// The line of 3f for `recorded`.
fn recorded(key: &str, at: &str) -> String {
    format!(
        "ap request: recorded {key} (verified, unrouted) by the hook. Do not record this message again: route it \
         (op route, request {key}), or supersede it with your kind and reading and \
         \"words_from\":{{\"record\":\"{}\",\"at\":\"{at}\"}}.",
        hooked()
    )
}
/// The line of R27 (f) for a request recorded unverified.
fn unverified(key: &str) -> String {
    format!(
        "ap request: recorded {key} (unverified, unrouted) by the hook. Do not record this message again: route it \
         (op route, request {key}), or supersede it with your kind and reading."
    )
}
/// The line of 3f for `not recorded`; the record named when it is admitted.
fn not_recorded(code: &str, record: Option<&str>) -> String {
    let tail = record.map_or(String::new(), |r| format!("; its session record is {r}"));
    format!(
        "ap request: not recorded by the hook ({code}). If this message is the user's, record it with ap request{tail}."
    )
}
const T: &str = "2026-11-16T05:00:00.250Z";
const AT: &str = "2026-11-16T05:00:00";
// spec: specs/ap/request.md#R25
#[test]
fn r25_only_a_record_under_the_users_claude_projects_gives_verified_words() {
    // Refused whether or not the file exists and whatever it holds, before any call to the store.
    let refused = |f: &mut F, record: &str| {
        let mut q = from("2026-11-16T03:04:05");
        q["words_from"]["record"] = json!(record);
        let e = refusal(f, &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            ("invalid_input", "words_from.record", None),
            "{record}"
        );
        assert!(f.calls.is_empty(), "{record}");
    };
    for record in [
        "/tmp/session.jsonl".to_owned(),
        "/home/other/.claude/projects/-w-work-walk/session.jsonl".to_owned(),
        format!("{HOME}/.claude/projects/../projects/-w-work-walk/session.jsonl"),
        format!("{DIR}../-w-work-walk/session.jsonl"),
        format!("{DIR}./session.jsonl"),
        format!("{HOME}/.claude/projects//session.jsonl"),
        format!("{HOME}/.claude/projects/"),
        format!("{HOME}/.claude/projects"),
        format!("{DIR}session.jsonl/"),
        format!("{DIR}sess\u{7f}ion.jsonl"),
        format!("{DIR}sess\nion.jsonl"),
        format!("{HOME}/.claude/session.jsonl"),
        format!("{HOME}x/.claude/projects/-w-work-walk/session.jsonl"),
    ] {
        let mut f = F::new();
        f.files.insert(record.clone(), RECORD.to_vec());
        refused(&mut f, &record);
    }
    // At most 400 bytes.
    let long = format!("{DIR}{}", "s".repeat(400 - DIR.len()));
    let longer = format!("{long}s");
    let mut f = F::new();
    f.files.insert(long.clone(), RECORD.to_vec());
    f.files.insert(longer.clone(), RECORD.to_vec());
    refused(&mut f, &longer);
    let mut q = from("2026-11-16T03:04:05");
    q["words_from"]["record"] = json!(long);
    assert_eq!(run(&mut f, &q)["words_verified"], true);
    // No home: nothing is admitted.
    let mut f = F::new();
    f.home = None;
    refused(&mut f, PATH);
    // The home of the port, with or without a final `/`, not another.
    let mut f = F::new();
    f.home = Some("/home/other".into());
    refused(&mut f, PATH);
    let mut f = F::new();
    f.home = Some(format!("{HOME}/").into());
    assert_eq!(
        run(&mut f, &from("2026-11-16T03:04:05"))["words_verified"],
        true
    );
    // The admitted path, taken by `record` and by `supersede`.
    let mut f = F::new();
    let first = run(&mut f, &from("2026-11-16T03:04:05"));
    assert_eq!(applied_params(&f, "record_request")["record"], PATH);
    let mut sup = supersede(&first["request"], from("2026-11-16T03:04:07"));
    let v = run(&mut f, &sup);
    assert_eq!(
        (
            &v["words_verified"],
            &applied_params(&f, "supersede_request")["record"]
        ),
        (&json!(true), &json!(PATH))
    );
    f.files.insert("/tmp/session.jsonl".into(), RECORD.to_vec());
    sup["words_from"]["record"] = json!("/tmp/session.jsonl");
    sup["request_id"] = json!("m3");
    let writes = f.writes();
    let e = refusal(&mut f, &sup);
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("invalid_input", "words_from.record", None)
    );
    assert_eq!(f.writes(), writes);
    // The same file as typed words is stored, not verified.
    let mut f = F::new();
    f.files.insert("/tmp/words.txt".into(), b"typed".to_vec());
    run(
        &mut f,
        &with(
            without(rec(), "words"),
            "words_file",
            json!("/tmp/words.txt"),
        ),
    );
    assert_eq!(
        applied_params(&f, "record_request")["words_verified"],
        false
    );
    let listed = run(&mut f, &json!({"op": "list", "topic": "walk/one"}));
    assert!(
        text(&listed)
            .starts_with("2026-11-16T03:04:05Z order walk/one [not verified] -> unrouted\n")
    );
}
// spec: specs/ap/request.md#R26
#[test]
fn r26_a_hooks_input_is_its_own_form_and_unknown_events_do_nothing() {
    // An unknown event: nothing printed, no file read, no call.
    for event in ["SessionStart", "PreToolUse", "", "Stop "] {
        let mut f = holding(&[typed(T, "hello")]);
        let q = with(submitted("hello"), "hook_event_name", json!(event));
        assert!(is_hook(&q));
        assert_eq!(hook(&mut f, None, &q), None, "{event}");
        assert_eq!((f.calls.len(), f.reads.get()), (0, 0), "{event}");
    }
    // Members the verb does not read are ignored, whatever they hold.
    let mut f = holding(&[typed(T, "hello")]);
    let mut q = submitted("hello");
    q["permission_mode"] = json!({"nested": [1, 2]});
    q["a_member_of_a_later_version"] = json!(null);
    q["op_like"] = json!("record");
    assert_eq!(hook(&mut f, None, &q), Some(recorded("rq3", AT)));
    // `--store` is forwarded.
    let mut f = holding(&[typed(T, "hello")]);
    hook(&mut f, Some("/s/store"), &submitted("hello")).unwrap();
    assert!(!f.stores.is_empty());
    assert!(f.stores.iter().all(|s| s.as_deref() == Some("/s/store")));
    // An object with `op` and `hook_event_name` is no hook's input: its `op` refuses the member.
    for q in [
        json!({"op": "list", "hook_event_name": "Stop"}),
        with(rec(), "hook_event_name", json!("UserPromptSubmit")),
    ] {
        assert!(!is_hook(&q));
        let mut f = F::new();
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            ("invalid_input", "hook_event_name", None)
        );
        assert!(f.calls.is_empty());
    }
    // A `hook_event_name` that is no string is no hook's input either.
    let q = json!({"hook_event_name": 1});
    assert!(!is_hook(&q));
    assert_eq!(refusal(&mut F::new(), &q).field, "op");
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_a_typed_prompt_is_recorded_verified_and_unrouted() {
    let mut f = holding(&[
        typed("2026-11-16T04:00:00.0Z", "an earlier prompt"),
        answer("2026-11-16T04:00:01.0Z"),
        typed(T, "do \"this\"\n가나다"),
        // An answer of a sub-agent is no answer of the turn.
        json!({"type": "assistant", "timestamp": "2026-11-16T05:00:01.0Z", "isSidechain": true, "message": {"content": "x"}}),
    ]);
    let line = hook(&mut f, None, &submitted("do \"this\"\n가나다"));
    assert_eq!(line, Some(recorded("rq3", AT)));
    assert_eq!(f.names(), ["apply record_request", "search request"]);
    let action = &f.calls[0].1;
    assert_eq!(
        (&action["request_id"], &action["agent"], &action["session"]),
        (
            &json!("hook-7602eec8756b29f8660e7dc1097437c9ea9c33f5-record"),
            &json!("hook"),
            &json!("sess-1")
        )
    );
    assert_eq!(
        action["parameters"],
        json!({"words": "do \"this\"\n가나다", "words_verified": true, "truncated": false,
            "reading": "Recorded by the hook of the CLI; kind and reading are not yet an agent's.",
            "kind": "order", "topic": "walk-topic", "said_at": "2026-11-16T05:00:00Z", "session": "sess-1",
            "record": hooked(), "runtime": "claude"})
    );
    let stored = f.request("rq3");
    assert_eq!(
        (&stored["route"], &stored["status"]),
        (&json!("unrouted"), &json!("current"))
    );
    // The agent's supersede with the given `words_from` reads the same words.
    let q = json!({"op": "supersede", "request": "rq3", "request_id": "m9", "agent": "lead", "session": "sess-1",
        "topic": "walk-topic", "kind": "decision", "reading": "an agent's reading",
        "words_from": {"record": hooked(), "at": AT}, "route": {"aside": "an example"}});
    let v = run(&mut f, &q);
    assert_eq!(
        (v["words_verified"].clone(), v["route"].clone()),
        (json!(true), json!("applied"))
    );
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_the_topic_is_the_task_directory_else_the_last_component() {
    for (cwd, topic) in [
        (
            json!("/w/work/walk-topic/wt/lifecycle-platform"),
            "walk-topic",
        ),
        (json!("/w/work/walk-topic"), "walk-topic"),
        (json!("/w/work"), "work"),
        (json!("/w/workshop/a/b"), "b"),
        (json!("/srv/some dir/가나"), "------"),
        (json!("/w/work/a:b.c_d-e/x"), "a:b.c_d-e"),
        (json!(format!("/w/work/{}", "t".repeat(130))), "t"),
        (json!("/"), "hook"),
        (json!(5), "hook"),
        (Value::Null, "hook"),
    ] {
        let mut f = holding(&[typed(T, "hello")]);
        let q = match cwd.clone() {
            Value::Null => without(submitted("hello"), "cwd"),
            c => with(submitted("hello"), "cwd", c),
        };
        assert_eq!(hook(&mut f, None, &q), Some(recorded("rq3", AT)), "{cwd}");
        let got = applied_params(&f, "record_request")["topic"].clone();
        let want = if topic == "t" {
            "t".repeat(120)
        } else {
            topic.to_owned()
        };
        assert_eq!(got, json!(want), "{cwd}");
    }
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_an_input_out_of_form_is_not_recorded_as_invalid_input() {
    let path = hooked();
    for (q, record) in [
        (
            with(submitted("hello"), "session_id", json!("a b")),
            Some(path.as_str()),
        ),
        (
            with(submitted("hello"), "session_id", json!("s".repeat(65))),
            Some(path.as_str()),
        ),
        (
            without(submitted("hello"), "session_id"),
            Some(path.as_str()),
        ),
        (
            with(submitted("hello"), "prompt", json!("")),
            Some(path.as_str()),
        ),
        (
            with(submitted("hello"), "prompt", json!(["hello"])),
            Some(path.as_str()),
        ),
        (without(submitted("hello"), "prompt"), Some(path.as_str())),
        (
            with(
                submitted("hello"),
                "transcript_path",
                json!("/tmp/hooked.jsonl"),
            ),
            None,
        ),
        (
            with(
                submitted("hello"),
                "transcript_path",
                json!(format!("{DIR}../hooked.jsonl")),
            ),
            None,
        ),
        (without(submitted("hello"), "transcript_path"), None),
    ] {
        let mut f = holding(&[typed(T, "hello")]);
        f.files
            .insert("/tmp/hooked.jsonl".into(), lines(&[typed(T, "hello")]));
        assert_eq!(
            hook(&mut f, None, &q),
            Some(not_recorded("invalid_input", record)),
            "{q}"
        );
        assert!(f.calls.is_empty(), "{q}");
    }
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_a_prompt_the_record_does_not_hold_as_a_human_line_now_is_recorded_unverified() {
    let human_meta = with(typed(T, "hello"), "isMeta", json!(true));
    let sidechain = with(typed(T, "hello"), "isSidechain", json!(true));
    let no_origin = without(typed(T, "hello"), "origin");
    let other_origin = with(
        typed(T, "hello"),
        "origin",
        json!({"kind": "task-notification"}),
    );
    let queued = json!({"type": "queue-operation", "timestamp": T, "operation": "enqueue", "content": "hello"});
    let blocks = with(
        typed(T, "x"),
        "message",
        json!({"role": "user", "content": [{"type": "text", "text": "hello"}]}),
    );
    let bad_time = with(
        typed(T, "hello"),
        "timestamp",
        json!("2026-11-16 05:00:00Z"),
    );
    let cases: Vec<(&str, Option<Vec<Value>>)> = vec![
        // The first prompt of a session: the record does not exist yet.
        ("absent", None),
        ("empty", Some(vec![])),
        ("other text", Some(vec![typed(T, "hello!")])),
        ("no origin", Some(vec![no_origin])),
        ("another origin", Some(vec![other_origin])),
        ("isMeta", Some(vec![human_meta])),
        ("sidechain", Some(vec![sidechain])),
        ("queued", Some(vec![queued])),
        ("blocks", Some(vec![blocks])),
        ("bad time", Some(vec![bad_time])),
        // An earlier line with the same text, no request of the hook names it, and an answer follows.
        (
            "answered",
            Some(vec![typed(T, "hello"), answer("2026-11-16T05:00:02.0Z")]),
        ),
        (
            "answered then other",
            Some(vec![
                typed(T, "hello"),
                answer("2026-11-16T05:00:02.0Z"),
                typed("2026-11-16T05:01:00.0Z", "next"),
            ]),
        ),
    ];
    for (name, record) in cases {
        let mut f = F::new();
        if let Some(record) = record {
            f.files.insert(hooked(), lines(&record));
        }
        assert_eq!(
            hook(&mut f, None, &submitted("hello")),
            Some(unverified("rq3")),
            "{name}"
        );
        let p = applied_params(&f, "record_request");
        assert_eq!(
            (&p["words"], &p["words_verified"], &p["said_at"]),
            (&json!("hello"), &json!(false), &json!(NOW)),
            "{name}"
        );
        assert!(p.get("record").is_none(), "{name}");
        assert_eq!(f.requests().len(), 1, "{name}");
    }
    // The last such line counts: an answered earlier one, then the same text again unanswered.
    let mut f = holding(&[
        typed("2026-11-16T04:00:00.0Z", "hello"),
        answer("2026-11-16T04:00:01.0Z"),
        typed(T, "hello"),
    ]);
    assert_eq!(
        hook(&mut f, None, &submitted("hello")),
        Some(recorded("rq3", AT))
    );
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_pre_flush_human_prompt_records_at_wall_and_non_human_inputs_skip() {
    // No line of the prompt in the record: recorded unverified at the port's wall second.
    let cases: Vec<(&str, Option<Vec<Value>>, &str)> = vec![
        ("absent", None, "hello"),
        ("empty", Some(vec![]), "hello"),
        (
            "answered earlier turn",
            Some(vec![typed(T, "hello"), answer("2026-11-16T05:00:02.0Z")]),
            "hello",
        ),
        ("path prompt", None, "/home/x/file.rs is broken"),
        ("pasted content", None, "<pasted_content>x</pasted_content>"),
    ];
    for (name, record, prompt) in cases {
        let mut f = F::new();
        if let Some(record) = record {
            f.files.insert(hooked(), lines(&record));
        }
        assert_eq!(
            hook(&mut f, None, &submitted(prompt)),
            Some(unverified("rq3")),
            "{name}"
        );
        let p = applied_params(&f, "record_request");
        assert_eq!(
            (
                &p["words"],
                &p["words_verified"],
                &p["said_at"],
                &p["runtime"]
            ),
            (&json!(prompt), &json!(false), &json!(NOW), &json!("claude")),
            "{name}"
        );
        assert!(p.get("record").is_none(), "{name}");
        let id = f.calls[0].1["request_id"].as_str().unwrap().to_owned();
        assert_eq!(
            id, "hook-75ec691436b53573dcd4d8fd55b7c803b3137722-record",
            "{name}"
        );
        assert_eq!(
            f.names(),
            ["apply record_request", "search request"],
            "{name}"
        );
        // A second identical prompt in that second is not recorded again.
        f.calls.clear();
        assert_eq!(
            hook(&mut f, None, &submitted(prompt)),
            Some(not_recorded("words_not_found", Some(&hooked()))),
            "{name}"
        );
        assert_eq!(f.names(), ["apply record_request"], "{name}");
        assert_eq!(f.requests().len(), 1, "{name}");
        // One with other words in that second ends with the store's code.
        assert_eq!(
            hook(&mut f, None, &submitted("other words")),
            Some(not_recorded("request_id_reused", Some(&hooked()))),
            "{name}"
        );
        assert_eq!(f.requests().len(), 1, "{name}");
    }
    // A subagent's prompt, a slash command and the harness and channel envelopes: no read, no call.
    let skipped = [
        ("agent_id", "hello"),
        ("/compact", "/compact"),
        ("/plugin:cmd arg", "/plugin:cmd arg"),
        ("/etc", "/etc"),
        (
            "task-notification",
            "<task-notification>done</task-notification>",
        ),
        ("event", "<event kind=\"x\">y</event>"),
        ("agent-message", "<agent-message>hi</agent-message>"),
        ("command-message", "<command-message>x</command-message>"),
        (
            "local-command",
            "<local-command-stdout>x</local-command-stdout>",
        ),
    ];
    for (name, prompt) in skipped {
        let mut f = holding(&[typed(T, prompt)]);
        let mut q = submitted(prompt);
        if name == "agent_id" {
            q["agent_id"] = json!("agent-1");
        }
        assert_eq!(
            hook(&mut f, None, &q),
            Some(not_recorded("words_not_found", Some(&hooked()))),
            "{name}"
        );
        assert_eq!((f.reads.get(), f.calls.len()), (0, 0), "{name}");
    }
    // A null `agent_id` is no subagent's.
    let mut q = submitted("hello");
    q["agent_id"] = Value::Null;
    let mut f = F::new();
    assert_eq!(hook(&mut f, None, &q), Some(unverified("rq3")));
}
// spec: specs/ap/request.md#R7
#[test]
fn r7_envelopes_and_non_human_lines_do_not_count_and_a_human_queued_command_does() {
    let enqueue = |t: &str, c: &str| json!({"type": "queue-operation", "timestamp": t, "operation": "enqueue", "content": c});
    let at = "2026-11-16T05:00:00";
    // An `<event ...>` enqueue beside a human enqueue in one second is no ambiguity.
    let rec = lines(&[
        enqueue("2026-11-16T05:00:00.100Z", "<event kind=\"x\">y</event>"),
        enqueue("2026-11-16T05:00:00.200Z", "a human words"),
    ]);
    assert_eq!(from_record_words(&rec, at), "a human words");
    // User lines of a task notification or of the harness do not count.
    let notified = with(
        typed("2026-11-16T05:00:00.300Z", "note"),
        "origin",
        json!({"kind": "task-notification"}),
    );
    let harness = with(
        typed("2026-11-16T05:00:00.400Z", "harness"),
        "rowSource",
        json!("harness"),
    );
    let rec = lines(&[notified, harness, typed(T, "the human")]);
    assert_eq!(from_record_words(&rec, at), "the human");
    // A human queued_command attachment counts; one of another origin or with an envelope does not.
    let queued = |origin: Value, prompt: &str| {
        json!({"type": "attachment", "timestamp": "2026-11-16T05:00:00.500Z",
            "attachment": {"type": "queued_command", "origin": origin, "prompt": prompt}})
    };
    let rec = lines(&[
        queued(json!({"kind": "human"}), "queued words"),
        queued(json!({"kind": "task-notification"}), "other"),
        queued(
            json!({"kind": "human"}),
            "<task-notification>x</task-notification>",
        ),
    ]);
    assert_eq!(from_record_words(&rec, at), "queued words");
}
/// The words `ap request` reads from a record at `at`, through a `record` call.
fn from_record_words(record: &[u8], at: &str) -> String {
    let mut f = F::new();
    f.files.insert(PATH.into(), record.to_vec());
    run(&mut f, &from(at));
    applied_params(&f, "record_request")["words"]
        .as_str()
        .unwrap()
        .to_owned()
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_two_strings_in_the_lines_second_are_ambiguous() {
    for other in [
        json!({"type": "user", "timestamp": "2026-11-16T05:00:00.900Z", "message": {"content": "another"}}),
        json!({"type": "queue-operation", "timestamp": "2026-11-16T05:00:00.100Z", "operation": "enqueue", "content": "another"}),
    ] {
        let mut f = holding(&[other.clone(), typed(T, "hello")]);
        assert_eq!(
            hook(&mut f, None, &submitted("hello")),
            Some(not_recorded("words_ambiguous", Some(&hooked()))),
            "{other}"
        );
        assert!(f.calls.is_empty());
    }
    // The same string twice in the second (a line and its queued copy) is one.
    let queued = json!({"type": "queue-operation", "timestamp": T, "operation": "enqueue", "content": "hello"});
    let mut f = holding(&[queued, typed(T, "hello")]);
    assert_eq!(
        hook(&mut f, None, &submitted("hello")),
        Some(recorded("rq3", AT))
    );
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_a_line_that_has_its_request_is_not_recorded_again() {
    // The same input twice: the second call sends the same action, which creates nothing.
    let mut f = holding(&[typed(T, "hello")]);
    let q = submitted("hello");
    assert_eq!(hook(&mut f, None, &q), Some(recorded("rq3", AT)));
    let first = f.calls[0].1.clone();
    f.calls.clear();
    assert_eq!(
        hook(&mut f, None, &q),
        Some(not_recorded("words_not_found", Some(&hooked())))
    );
    assert_eq!(f.names(), ["apply record_request"]);
    assert_eq!(f.calls[0].1, first);
    assert_eq!(f.requests().len(), 1);
    // An earlier line with the same text whose request is current and unrouted, then routed: a new
    // message with that text the record does not hold yet is not recorded, with one call each.
    for route in [
        None,
        Some(json!({"aside": "done"})),
        Some(json!({"intent": "open"})),
    ] {
        let mut f = holding(&[typed(T, "hello")]);
        hook(&mut f, None, &submitted("hello")).unwrap();
        if let Some(route) = route.clone() {
            let mut q = json!({"op": "route", "request_id": "r1", "agent": "lead", "session": "sess-1", "request": "rq3"});
            q.as_object_mut()
                .unwrap()
                .extend(route.as_object().unwrap().clone());
            run(&mut f, &q);
        }
        f.calls.clear();
        assert_eq!(
            hook(&mut f, None, &submitted("hello")),
            Some(not_recorded("words_not_found", Some(&hooked()))),
            "{route:?}"
        );
        assert_eq!(f.calls.len(), 1, "{route:?}");
        assert_eq!(f.requests().len(), 1, "{route:?}");
    }
}
// spec: specs/ap/request.md#R27
#[test]
fn r27_a_prompt_over_the_bound_is_stored_cut_and_truncated() {
    let said = format!("{}가", "x".repeat(3_999)) + &"y".repeat(30_000);
    let mut f = holding(&[typed(T, &said)]);
    assert_eq!(
        hook(&mut f, None, &submitted(&said)),
        Some(recorded("rq3", AT))
    );
    let p = applied_params(&f, "record_request");
    assert_eq!(
        (&p["words"], &p["truncated"], &p["words_verified"]),
        (&json!("x".repeat(3_999)), &json!(true), &json!(true))
    );
}
/// A change of the fake before a call.
type Setup = Box<dyn Fn(&mut F)>;
// spec: specs/ap/request.md#R27
#[test]
fn r27_a_refusal_or_a_failure_of_the_store_is_not_recorded_with_its_code() {
    let engine = json!({"status": "refused", "code": "invalid_parameter", "parameter": "kind"});
    let criteria = json!({"status": "refused", "code": "submission_criteria", "failed": ["said_in_the_future"]});
    let cases: Vec<(&str, Setup)> = vec![
        (
            "invalid_parameter",
            Box::new(move |f: &mut F| f.refuse = Some(("record_request", engine.clone()))),
        ),
        (
            "said_in_the_future",
            Box::new(move |f: &mut F| f.refuse = Some(("record_request", criteria.clone()))),
        ),
        ("store_busy", Box::new(|f: &mut F| f.busy = 6)),
        (
            "process_failed",
            Box::new(|f: &mut F| f.cut = Some("record_request")),
        ),
        (
            "readback_differs",
            Box::new(|f: &mut F| f.alter_words = true),
        ),
        (
            "store_answer_unqualified",
            Box::new(|f: &mut F| f.unqualified = true),
        ),
    ];
    for (code, set) in cases {
        let mut f = holding(&[typed(T, "hello")]);
        set(&mut f);
        assert_eq!(
            hook(&mut f, None, &submitted("hello")),
            Some(not_recorded(code, Some(&hooked()))),
            "{code}"
        );
        assert!(
            f.calls
                .iter()
                .all(|c| c.1["action_type"] != "route_request")
        );
    }
}
/// A request of session `session` said at `at`, recorded by an agent.
fn said(f: &mut F, id: &str, session: &str, at: &str) -> String {
    let q = json!({"op": "record", "request_id": id, "agent": "lead", "session": session, "topic": "walk-topic",
        "kind": "order", "reading": "r", "words": format!("w {id}"), "said_at": at});
    run(f, &q)["request"].as_str().unwrap().to_owned()
}
fn reason(n: usize, keys: &[&str]) -> String {
    json!({"decision": "block", "reason": format!(
        "ap request: {n} unrouted request(s) of this session: {}. Route each (op route) or set it aside, then end the turn.",
        keys.join(", "))}).to_string()
}
// spec: specs/ap/request.md#R28
#[test]
fn r28_stop_says_once_that_requests_of_the_session_are_unrouted() {
    // Already continued by this hook: nothing, and no call.
    let mut f = F::new();
    said(&mut f, "a", "sess-1", "2026-11-16T01:00:00Z");
    f.calls.clear();
    assert_eq!(
        hook(
            &mut f,
            None,
            &with(stopped("sess-1"), "stop_hook_active", json!(true))
        ),
        None
    );
    assert!(f.calls.is_empty());
    // No row.
    let mut f = F::new();
    assert_eq!(hook(&mut f, None, &stopped("sess-1")), None);
    assert_eq!(f.names(), ["search request"]);
    assert_eq!(
        f.calls[0].1["where"]["all"],
        json!([{"eq": ["route", "unrouted"]}, {"eq": ["status", "current"]}])
    );
    // Rows of this and of another session, one routed: this session's unrouted ones, oldest first.
    let mut f = F::new();
    let late = said(&mut f, "a", "sess-1", "2026-11-16T03:00:00Z");
    said(&mut f, "b", "sess-2", "2026-11-16T01:00:00Z");
    let early = said(&mut f, "c", "sess-1", "2026-11-16T02:00:00Z");
    let routed = said(&mut f, "d", "sess-1", "2026-11-16T00:00:00Z");
    run(
        &mut f,
        &json!({"op": "route", "request_id": "r", "agent": "lead", "session": "sess-1", "request": routed, "aside": "done"}),
    );
    f.calls.clear();
    let line = hook(&mut f, None, &stopped("sess-1")).unwrap();
    assert_eq!(line, reason(2, &[&early, &late]));
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 2);
    assert_eq!(f.writes(), 0);
    assert_eq!(f.reads.get(), 0);
    assert_eq!(hook(&mut f, None, &stopped("sess-3")), None);
    // More than five: the count, and the first five keys.
    let mut f = F::new();
    let keys: Vec<String> = (0..7)
        .map(|i| {
            said(
                &mut f,
                &format!("k{i}"),
                "sess-1",
                &format!("2026-11-16T0{}:00:00Z", 7 - i),
            )
        })
        .collect();
    let mut oldest: Vec<&str> = keys.iter().rev().map(String::as_str).collect();
    oldest.truncate(5);
    assert_eq!(
        hook(&mut f, None, &stopped("sess-1")),
        Some(reason(7, &oldest))
    );
    // Every page is read.
    let mut f = F::new();
    many(&mut f, 20, "walk-topic");
    let line = hook(&mut f, None, &stopped("s1")).unwrap();
    assert!(line.contains("ap request: 20 unrouted request(s)"));
    // An unqualified answer, a refusal and a failed call: nothing.
    for set in [
        Box::new(|f: &mut F| f.unqualified = true) as Setup,
        Box::new(|f: &mut F| f.busy = 6),
    ] {
        let mut f = F::new();
        said(&mut f, "a", "sess-1", "2026-11-16T01:00:00Z");
        set(&mut f);
        assert_eq!(hook(&mut f, None, &stopped("sess-1")), None);
    }
    let mut f = F::new();
    said(&mut f, "a", "sess-1", "2026-11-16T01:00:00Z");
    struct Cut;
    impl System for Cut {
        fn call(&mut self, _: &str, _: &[String], _: &Value, _: u64) -> Answer {
            Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            })
        }
    }
    assert_eq!(hook(&mut Cut, None, &stopped("sess-1")), None);
}
// spec: specs/ap/request.md#R29
#[test]
fn r29_the_hook_form_never_refuses_and_prints_at_most_one_line() {
    let mut inputs = vec![
        submitted("hello"),
        submitted("other"),
        with(submitted("hello"), "transcript_path", json!("/tmp/x.jsonl")),
        with(submitted("hello"), "session_id", json!(1)),
        stopped("sess-1"),
        with(stopped("sess-1"), "stop_hook_active", json!(true)),
        json!({"hook_event_name": "Notification"}),
        json!({"hook_event_name": "UserPromptSubmit"}),
        json!({"hook_event_name": "Stop"}),
    ];
    inputs.push(with(
        submitted("hello"),
        "prompt",
        json!("x".repeat(30_000)),
    ));
    for q in &inputs {
        for busy in [0, 6] {
            let mut f = holding(&[typed(T, "hello")]);
            said(&mut f, "a", "sess-1", "2026-11-16T01:00:00Z");
            f.busy = busy;
            assert!(is_hook(q));
            if let Some(line) = hook(&mut f, None, q) {
                assert!(
                    !line.contains('\n') && !line.starts_with("{\"error\""),
                    "{line}"
                );
                assert!(
                    line.starts_with("ap request: ") || line.starts_with("{\"decision\":\"block\""),
                    "{line}"
                );
            }
        }
    }
    // The built binary: status 0, nothing on standard error, for the cases that reach no store.
    let stdin = |v: Value| v.to_string().into_bytes();
    for (args, q, out) in [
        (
            vec![],
            json!({"hook_event_name": "SessionStart", "session_id": "s"}),
            String::new(),
        ),
        (
            vec!["--store", "/s/store"],
            json!({"hook_event_name": "Notification"}),
            String::new(),
        ),
        (
            vec![],
            with(stopped("sess-1"), "stop_hook_active", json!(true)),
            String::new(),
        ),
        (
            vec![],
            with(
                submitted("hello"),
                "transcript_path",
                json!("/tmp/hooked.jsonl"),
            ),
            format!("{}\n", not_recorded("invalid_input", None)),
        ),
        (
            vec!["--store", "/s/store"],
            with(
                with(submitted("hello"), "session_id", json!("a b")),
                "transcript_path",
                json!("/tmp/hooked.jsonl"),
            ),
            format!("{}\n", not_recorded("invalid_input", None)),
        ),
    ] {
        let (status, stdout, stderr) = ap_raw(&args, &stdin(q.clone()));
        assert_eq!(
            (status, String::from_utf8(stdout).unwrap(), stderr),
            (Some(0), out, vec![]),
            "{q}"
        );
    }
    // An object with `op` and `hook_event_name` is refused; `--text` with a hook's input is `usage`.
    let (status, out, e) = ap(&[], br#"{"op":"list","hook_event_name":"Stop"}"#);
    assert_eq!(
        (
            status,
            out.is_empty(),
            &e["error"]["code"],
            &e["error"]["field"]
        ),
        (
            Some(2),
            true,
            &json!("invalid_input"),
            &json!("hook_event_name")
        )
    );
    for args in [
        vec!["--text"],
        vec!["--store", "/s/store", "--text"],
        vec!["--bogus"],
    ] {
        let (status, out, e) = ap(&args, &stdin(json!({"hook_event_name": "Notification"})));
        assert_eq!(
            (
                status,
                out.is_empty(),
                &e["error"]["code"],
                &e["error"]["field"]
            ),
            (Some(2), true, &json!("usage"), &json!("args")),
            "{args:?}"
        );
    }
}
