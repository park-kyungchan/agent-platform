//! `ap row` (README "One row") against a fake of `ont` and a session record made in the test.
use ap::{
    host::{Answer, Failure, Reply, System},
    row::{Refusal, execute},
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn field_map(o: &Value) -> Map<String, Value> {
    let mine = o.as_object().unwrap().iter().filter(|(k, _)| *k != "id");
    mine.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
        .collect()
}
/// A store of decisions, items and objectives with the two actions as the schema states them.
struct R {
    calls: Vec<(String, Vec<String>, Value)>,
    files: BTreeMap<String, Vec<u8>>,
    objects: Vec<Value>,
    applied: BTreeSet<String>,
    /// The next apply of `triage_decision` is cut: the call fails and nothing is written.
    cut_triage: bool,
    /// The store keeps other user words than the ones it was given.
    alter_words: bool,
    /// A row written by `record_decision` is already triaged (by another call).
    pre_triaged: bool,
}
impl R {
    fn new() -> Self {
        let task = oid("task", "tk");
        let objects = vec![
            json!({"id": oid("item", "it-a"), "task": task, "key": "item-a"}),
            json!({"id": oid("objective", "ob-a"), "task": task, "name": "ship-it"}),
        ];
        Self {
            calls: vec![],
            files: BTreeMap::new(),
            objects,
            applied: BTreeSet::new(),
            cut_triage: false,
            alter_words: false,
            pre_triaged: false,
        }
    }
    fn writes(&self) -> usize {
        self.calls
            .iter()
            .filter(|c| c.1.get(1).is_some_and(|w| w == "apply"))
            .count()
    }
    fn names(&self) -> Vec<String> {
        let name = |c: &(String, Vec<String>, Value)| match c.1[0].as_str() {
            "action" => format!("{} {}", c.1[1], c.2["action_type"].as_str().unwrap()),
            other => other.to_owned(),
        };
        self.calls.iter().map(name).collect()
    }
    fn decisions(&self) -> Vec<&Value> {
        self.objects
            .iter()
            .filter(|o| o["id"]["object_type"] == "decision")
            .collect()
    }
    fn failed(&self, body: &Value) -> Vec<&'static str> {
        let p = &body["parameters"];
        let row = self.objects.iter().find(|o| o["id"] == p["decision"]);
        match body["action_type"].as_str().unwrap() {
            "record_decision" => {
                let mut failed = vec![];
                let current = |d: &&Value| d["topic"] == p["topic"] && d["status"] == "current";
                if self.decisions().iter().any(current) {
                    failed.push("decision_already_current");
                }
                if p["decided_by"] == "user" && p["user_form"] == "question" {
                    failed.push("user_question_is_not_a_decision");
                }
                failed
            }
            _ => match row {
                Some(r) if r["decided_by"] != "user" => vec!["not_a_user_row"],
                Some(r) if !r["triage"].is_null() => vec!["already_triaged"],
                Some(_) => vec![],
                None => vec!["not_a_user_row"],
            },
        }
    }
    fn ont(&mut self, a: &[String], body: &Value) -> Answer {
        let reply = |ok: bool, v: Value| {
            let bytes = serde_json::to_vec(&v).unwrap();
            Ok(Reply {
                success: ok,
                stdout: bytes.clone(),
                bytes,
            })
        };
        if a[0] == "search" {
            let kind = body["object_type"].as_str().unwrap();
            let task = &body["where"]["all"][0]["eq"][1];
            let found: Vec<_> = (self.objects.iter())
                .filter(|o| o["id"]["object_type"] == kind && o["task"] == *task)
                .map(|o| json!({"id": o["id"], "head": {"fields": field_map(o), "revision": "r"}}))
                .collect();
            return reply(
                true,
                json!({"status":"ok","result":{"basis":{"qualified":true},"objects":found,"continuation":null}}),
            );
        }
        assert_eq!(a[0], "action", "{a:?}");
        assert_eq!(a[2..4], ["--client", "lead"], "{a:?}");
        let (p, id) = (
            &body["parameters"],
            body["request_id"].as_str().unwrap().to_owned(),
        );
        assert!(
            body["agent"].is_string() && body["session"].is_string(),
            "{body}"
        );
        let failed = self.failed(body);
        if a[1] == "validate" {
            let criteria: Vec<_> = failed
                .iter()
                .map(|c| json!({"code": c, "passed": false}))
                .collect();
            return reply(
                true,
                json!({"status":"ok","result":{"valid":failed.is_empty(),"criteria":criteria}}),
            );
        }
        let done = |outcome: &str| json!({"status":"ok","result":{"outcome":outcome,"edits":[]}});
        if self.applied.contains(&id) {
            return reply(true, done("already_applied"));
        }
        if !failed.is_empty() {
            return reply(
                false,
                json!({"status":"refused","code":"submission_criteria","failed":failed}),
            );
        }
        if body["action_type"] == "triage_decision" {
            if std::mem::take(&mut self.cut_triage) {
                return Err(Failure {
                    code: "process_failed",
                    field: "ont".into(),
                    detail: Value::Null,
                });
            }
            let row = self
                .objects
                .iter_mut()
                .find(|o| o["id"] == p["decision"])
                .unwrap();
            row["triage"] = p["triage"].clone();
        } else {
            let n = self.decisions().len();
            let mut row = json!({"id": oid("decision", &format!("d{n}")), "task": p["task"],
                "topic": p["topic"], "status": "current", "decided_by": p["decided_by"],
                "text": p["text"], "triage": if self.pre_triaged { json!("none") } else { Value::Null }});
            let words = p["user_words"].as_str().map(|w| {
                if self.alter_words {
                    format!("{w}!")
                } else {
                    w.into()
                }
            });
            row["user_words"] = json!(words);
            self.objects.push(row);
        }
        self.applied.insert(id);
        reply(true, done("applied"))
    }
}
impl System for R {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _ms: u64) -> Answer {
        assert_eq!(program, "ont", "only the store is called");
        let mut args = args.to_vec();
        self.calls
            .push((program.into(), args.clone(), input.clone()));
        if let Some(i) = args.iter().position(|a| a == "--store") {
            args.truncate(i);
        }
        self.ont(&args, input)
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
}
fn user() -> Value {
    json!({"task":"tk","request_id":"m1","agent":"lead","session":"s1","topic":"topic-one",
        "text":"the reading","by":"user","words":"do the thing","triage":{"word":"none","reason":"a question"}})
}
fn lead() -> Value {
    json!({"task":"tk","request_id":"m2","agent":"lead","session":"s1","topic":"topic-two",
        "text":"the choice","by":"lead","reason":"because","undo":"revert it"})
}
fn run(r: &mut R, request: &Value) -> Result<Value, Refusal> {
    execute(r, None, request)
}
fn refusal(r: &mut R, request: &Value) -> Refusal {
    run(r, request).expect_err("a refusal")
}
fn with(mut v: Value, k: &str, x: Value) -> Value {
    v[k] = x;
    v
}
fn without(mut v: Value, k: &str) -> Value {
    v.as_object_mut().unwrap().remove(k);
    v
}
const AT: &str = "2026-11-16T03:04:05";
fn line(kind: &str, at: &str, extra: Value) -> String {
    let mut v = json!({"type": kind, "timestamp": format!("{at}.123Z")});
    v.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    format!("{v}\n")
}
fn record(r: &mut R, lines: &[String]) -> Value {
    r.files
        .insert("/rec/s.jsonl".into(), lines.concat().into_bytes());
    with(
        without(user(), "words"),
        "words_from",
        json!({"record": "/rec/s.jsonl", "at": AT}),
    )
}
fn user_line(at: &str, words: &str) -> String {
    line(
        "user",
        at,
        json!({"message": {"role": "user", "content": words}}),
    )
}

#[test]
fn a_user_row_is_recorded_read_back_and_triaged_in_order() {
    let mut r = R::new();
    let v = run(&mut r, &user()).ok().unwrap();
    assert_eq!(
        r.names(),
        [
            "validate record_decision",
            "apply record_decision",
            "search",
            "validate triage_decision",
            "apply triage_decision"
        ]
    );
    let record = &r.calls[1].2;
    assert_eq!(record["request_id"], "m1-record");
    assert_eq!(
        (record["agent"].as_str(), record["session"].as_str()),
        (Some("lead"), Some("s1"))
    );
    assert_eq!(
        record["parameters"],
        json!({"task": oid("task", "tk"), "topic": "topic-one", "text": "the reading",
            "decided_by": "user", "user_words": "do the thing", "user_form": "decision"})
    );
    let triage = &r.calls[4].2;
    assert_eq!(triage["request_id"], "m1-triage");
    assert_eq!(
        triage["parameters"],
        json!({"decision": oid("decision", "d0"), "triage": "none", "reason": "a question"})
    );
    assert_eq!(
        v,
        json!({"topic": "topic-one", "decision": oid("decision", "d0"), "record": "applied",
            "triage": "none", "words_bytes": 12})
    );
}
#[test]
fn store_is_forwarded_to_every_call() {
    let mut r = R::new();
    execute(&mut r, Some("/s/store"), &user()).ok().unwrap();
    assert_eq!(r.calls.len(), 5);
    for c in &r.calls {
        assert_eq!(c.1[c.1.len() - 2..], ["--store", "/s/store"], "{c:?}");
    }
}
#[test]
fn item_and_intent_triage_name_the_task_s_own_object() {
    let mut r = R::new();
    let item = json!({"word": "item", "item": "item-a"});
    run(&mut r, &with(user(), "triage", item)).ok().unwrap();
    assert_eq!(
        r.calls.last().unwrap().2["parameters"]["item"],
        oid("item", "it-a")
    );
    let mut r = R::new();
    let intent = json!({"word": "intent", "objective": "ship-it", "reason": "r"});
    run(&mut r, &with(user(), "triage", intent)).ok().unwrap();
    let p = &r.calls.last().unwrap().2["parameters"];
    assert_eq!(
        (&p["objective"], &p["reason"]),
        (&oid("objective", "ob-a"), &json!("r"))
    );
}
#[test]
fn a_lead_row_has_reason_and_undo_and_no_triage() {
    let mut r = R::new();
    let v = run(&mut r, &lead()).ok().unwrap();
    assert_eq!(
        r.names(),
        [
            "validate record_decision",
            "apply record_decision",
            "search"
        ]
    );
    assert_eq!(
        r.calls[1].2["parameters"],
        json!({"task": oid("task", "tk"), "topic": "topic-two", "text": "the choice",
            "decided_by": "lead", "reason": "because", "undo": "revert it"})
    );
    assert_eq!(v["triage"], Value::Null);
    assert_eq!(v["record"], "applied");
}
#[test]
fn words_file_is_the_whole_text_byte_for_byte() {
    let mut r = R::new();
    let words = " line one\n\tline two\r\n ";
    r.files
        .insert("/w/words.txt".into(), words.as_bytes().to_vec());
    let q = with(
        without(user(), "words"),
        "words_file",
        json!("/w/words.txt"),
    );
    let v = run(&mut r, &q).ok().unwrap();
    assert_eq!(r.calls[1].2["parameters"]["user_words"], words);
    assert_eq!(v["words_bytes"], words.len());
    for bad in ["relative.txt", "/w/missing.txt"] {
        let q = with(without(user(), "words"), "words_file", json!(bad));
        let e = refusal(&mut R::new(), &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            ("invalid_input", "words_file", None),
            "{bad}"
        );
    }
    r.files.insert("/w/binary".into(), vec![0xff, 0xfe]);
    let q = with(without(user(), "words"), "words_file", json!("/w/binary"));
    assert_eq!(refusal(&mut R::new(), &q).code, "invalid_input");
}
#[test]
fn words_from_a_record_take_the_one_line_of_the_second_user_or_queued() {
    let mut r = R::new();
    let q = record(
        &mut r,
        &[
            user_line("2026-11-16T03:04:04", "earlier"),
            user_line(AT, "  these words\n"),
            user_line("2026-11-16T03:04:06", "later"),
        ],
    );
    run(&mut r, &q).ok().unwrap();
    assert_eq!(r.calls[1].2["parameters"]["user_words"], "  these words\n");
    // A message sent while a turn ran is a `queue-operation` line.
    let mut r = R::new();
    let queued = line(
        "queue-operation",
        AT,
        json!({"operation": "enqueue", "content": "queued words"}),
    );
    let q = record(&mut r, &[user_line("2026-11-16T03:04:04", "x"), queued]);
    run(&mut r, &q).ok().unwrap();
    assert_eq!(r.calls[1].2["parameters"]["user_words"], "queued words");
}
#[test]
fn only_the_two_shapes_of_line_count_and_the_count_is_reported() {
    let others = [
        line(
            "user",
            AT,
            json!({"message": {"content": [{"type": "tool_result"}]}}),
        ),
        line("assistant", AT, json!({"message": {"content": "answer"}})),
        line(
            "queue-operation",
            AT,
            json!({"operation": "dequeue", "content": "x"}),
        ),
        line(
            "queue-operation",
            AT,
            json!({"operation": "enqueue", "content": ["x"]}),
        ),
        "not json\n".to_owned(),
    ];
    let mut r = R::new();
    let q = record(&mut r, &others);
    let e = refusal(&mut r, &q);
    assert_eq!(
        (e.code.as_str(), e.detail, e.after),
        ("words_not_found", json!(0), None)
    );
    let mut lines = others.to_vec();
    lines.extend([
        user_line(AT, "one"),
        user_line(AT, "two"),
        line(
            "queue-operation",
            AT,
            json!({"operation": "enqueue", "content": "three"}),
        ),
    ]);
    let q = record(&mut r, &lines);
    let e = refusal(&mut r, &q);
    assert_eq!(
        (e.code.as_str(), e.detail, e.after),
        ("words_ambiguous", json!(3), None)
    );
    assert_eq!(r.calls.len(), 0, "no call before the words are found");
    // The time is a UTC time to the second; the record an absolute path.
    for at in ["2026-11-16", "2026-11-16 03:04:05", "2026-11-16T03:04:05Z"] {
        let mut q = record(&mut r, &lines);
        q["words_from"]["at"] = json!(at);
        assert_eq!(refusal(&mut r, &q).code, "invalid_input", "{at}");
    }
    let mut q = record(&mut r, &lines);
    q["words_from"]["record"] = json!("rec/s.jsonl");
    assert_eq!(refusal(&mut r, &q).code, "invalid_input");
}
#[test]
fn words_above_the_limit_are_refused_with_their_byte_count_before_any_call() {
    let mut r = R::new();
    let long = "x".repeat(1001);
    let e = refusal(&mut r, &with(user(), "words", json!(long)));
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("words_too_long", "words", None)
    );
    assert_eq!(e.detail["bytes"], 1001);
    // Bytes, not characters: 334 three-byte characters are 1002 bytes.
    let e = refusal(&mut r, &with(user(), "words", json!("가".repeat(334))));
    assert_eq!(
        (e.code.as_str(), e.detail["bytes"].clone()),
        ("words_too_long", json!(1002))
    );
    // The same through a file and through a record.
    r.files.insert("/w/long".into(), long.clone().into_bytes());
    let q = with(without(user(), "words"), "words_file", json!("/w/long"));
    assert_eq!(refusal(&mut r, &q).code, "words_too_long");
    let q = record(&mut r, &[user_line(AT, &long)]);
    assert_eq!(refusal(&mut r, &q).code, "words_too_long");
    assert!(r.calls.is_empty());
    // The limit itself is kept whole.
    let exact = "y".repeat(1000);
    run(&mut r, &with(user(), "words", json!(exact)))
        .ok()
        .unwrap();
    assert_eq!(r.calls[1].2["parameters"]["user_words"], "y".repeat(1000));
}
#[test]
fn text_reason_and_undo_have_their_limits() {
    let cases = [
        (user(), "text", 1001, "text_too_long"),
        (lead(), "text", 1001, "text_too_long"),
        (lead(), "reason", 601, "reason_too_long"),
        (lead(), "undo", 301, "undo_too_long"),
    ];
    for (base, member, size, code) in cases {
        let mut r = R::new();
        let e = refusal(&mut r, &with(base.clone(), member, json!("z".repeat(size))));
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            (code, member, None)
        );
        assert_eq!(e.detail["bytes"], size);
        assert!(r.calls.is_empty(), "{member}");
        let ok = with(base, member, json!("z".repeat(size - 1)));
        run(&mut r, &ok).ok().unwrap();
    }
    let mut r = R::new();
    let long = json!({"word": "none", "reason": "z".repeat(601)});
    let e = refusal(&mut r, &with(user(), "triage", long));
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("reason_too_long", "triage.reason")
    );
    assert!(r.calls.is_empty());
}
#[test]
fn a_malformed_request_is_refused_with_its_field_before_any_call() {
    let mut cases: Vec<(Value, &str)> = vec![];
    for k in [
        "task",
        "request_id",
        "agent",
        "session",
        "topic",
        "text",
        "by",
        "triage",
    ] {
        cases.push((without(user(), k), k));
    }
    for k in ["reason", "undo"] {
        cases.push((without(lead(), k), k));
    }
    cases.extend([
        (without(user(), "words"), "words"),
        (with(user(), "words_file", json!("/w/f")), "words_file"),
        (
            with(
                with(user(), "words_from", json!({"record":"/r","at":AT})),
                "words_file",
                json!("/f"),
            ),
            "words_from",
        ),
        (with(user(), "extra", json!(1)), "extra"),
        (with(lead(), "words", json!("w")), "words"),
        (
            with(lead(), "triage", json!({"word": "none", "reason": "r"})),
            "triage",
        ),
        (with(user(), "reason", json!("r")), "reason"),
        (with(user(), "by", json!("admin")), "by"),
        (with(user(), "by", json!(1)), "by"),
        (with(user(), "words", json!("")), "words"),
        (with(user(), "task", json!(5)), "task"),
        (with(user(), "session", json!("")), "session"),
        (
            with(user(), "triage", json!({"word": "maybe"})),
            "triage.word",
        ),
        (
            with(user(), "triage", json!({"word": "none"})),
            "triage.reason",
        ),
        (
            with(user(), "triage", json!({"word": "not_adopted"})),
            "triage.reason",
        ),
        (with(user(), "triage", json!({"word": "item"})), "triage"),
        (with(user(), "triage", json!({"word": "intent"})), "triage"),
        (
            with(
                user(),
                "triage",
                json!({"word": "none", "reason": "r", "item": "item-a"}),
            ),
            "triage",
        ),
        (
            with(
                user(),
                "triage",
                json!({"word": "item", "item": "item-a", "objective": "ship-it"}),
            ),
            "triage",
        ),
        (
            with(
                user(),
                "triage",
                json!({"word": "none", "reason": "r", "more": 1}),
            ),
            "triage.more",
        ),
        (with(user(), "triage", json!("none")), "triage"),
    ]);
    for (request, field) in cases {
        let mut r = R::new();
        let e = refusal(&mut r, &request);
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after),
            ("invalid_input", field, None),
            "{request}"
        );
        assert!(r.calls.is_empty(), "{request}");
    }
    let e = refusal(&mut R::new(), &json!([1]));
    assert_eq!(e.code, "invalid_input");
}
#[test]
fn an_unknown_item_or_objective_is_refused_before_any_change() {
    for (triage, code) in [
        (json!({"word": "item", "item": "nope"}), "item_unknown"),
        (
            json!({"word": "intent", "objective": "nope"}),
            "objective_unknown",
        ),
    ] {
        let mut r = R::new();
        let e = refusal(&mut r, &with(user(), "triage", triage));
        assert_eq!((e.code.as_str(), e.after), (code, None));
        assert_eq!(r.writes(), 0);
    }
}
#[test]
fn a_store_refusal_keeps_its_code_and_says_how_far_the_call_went() {
    // A question is refused at the record: nothing changed.
    let mut r = R::new();
    let e = refusal(&mut r, &with(user(), "form", json!("question")));
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after),
        ("user_question_is_not_a_decision", "record", None)
    );
    assert_eq!(r.names(), ["validate record_decision"]);
    // A second current decision of a topic is the store's `decision_already_current`.
    let mut r = R::new();
    run(&mut r, &user()).ok().unwrap();
    let e = refusal(&mut r, &with(user(), "request_id", json!("m9")));
    assert_eq!(
        (e.code.as_str(), e.after, r.decisions().len()),
        ("decision_already_current", None, 1)
    );
    // A triage the store refuses comes after the record.
    let mut r = R::new();
    r.pre_triaged = true;
    let e = refusal(&mut r, &user());
    assert_eq!(e.code, "already_triaged");
    assert_eq!((e.field.as_str(), e.after), ("triage", Some("record")));
}
#[test]
fn a_row_read_back_with_other_words_is_readback_differs_and_not_triaged() {
    let mut r = R::new();
    r.alter_words = true;
    let e = refusal(&mut r, &user());
    assert_eq!(
        (e.code.as_str(), e.after),
        ("readback_differs", Some("record"))
    );
    assert_eq!(
        r.names(),
        [
            "validate record_decision",
            "apply record_decision",
            "search"
        ]
    );
    // A current row of the topic the request did not write is the same refusal.
    let mut r = R::new();
    r.objects.push(json!({"id": oid("decision", "d0"), "task": oid("task", "tk"), "topic": "topic-one", "status": "current", "decided_by": "user", "user_words": "other words", "text": "t", "triage": null}));
    r.applied.insert("m1-record".into());
    let e = refusal(&mut r, &user());
    assert_eq!(
        (e.code.as_str(), e.after),
        ("readback_differs", Some("record"))
    );
    assert_eq!(r.writes(), 1);
}
#[test]
fn a_call_cut_after_the_record_runs_again_and_triages_without_a_second_record() {
    let mut r = R::new();
    r.cut_triage = true;
    let e = refusal(&mut r, &user());
    assert_eq!(
        (e.code.as_str(), e.after),
        ("process_failed", Some("record"))
    );
    assert_eq!(r.decisions().len(), 1);
    let v = run(&mut r, &user()).ok().unwrap();
    assert_eq!(
        (v["record"].as_str(), v["triage"].as_str()),
        (Some("already_applied"), Some("none"))
    );
    assert_eq!(r.decisions().len(), 1);
    assert_eq!(r.decisions()[0]["triage"], "none");
    // A call repeated whole writes nothing and answers the same.
    let before = r.applied.len();
    let again = run(&mut r, &user()).ok().unwrap();
    assert_eq!(
        (again["record"].as_str(), again["decision"].clone()),
        (Some("already_applied"), v["decision"].clone())
    );
    assert_eq!((r.applied.len(), r.decisions().len()), (before, 1));
}
#[test]
fn a_lead_row_repeated_answers_already_applied() {
    let mut r = R::new();
    let first = run(&mut r, &lead()).ok().unwrap();
    let second = run(&mut r, &lead()).ok().unwrap();
    assert_eq!(
        (first["record"].as_str(), second["record"].as_str()),
        (Some("applied"), Some("already_applied"))
    );
    assert_eq!(r.decisions().len(), 1);
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
    let e = execute(&mut Down, None, &user()).err().unwrap();
    assert_eq!(
        (e.code.as_str(), e.after),
        ("store_answer_unreadable", None)
    );
}
fn ap(args: &[&str], stdin: &str) -> (Option<i32>, Value) {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // A refusal of the arguments is made before stdin is read.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    let out = child.wait_with_output().unwrap();
    let said = if out.stderr.is_empty() {
        &out.stdout
    } else {
        &out.stderr
    };
    (
        out.status.code(),
        serde_json::from_slice(said).unwrap_or(Value::Null),
    )
}
#[test]
fn the_command_refuses_in_the_one_form_with_exit_2_before_any_call() {
    for (args, stdin, code, field) in [
        (vec!["--bogus"], "{}", "usage", "args"),
        (vec!["--store"], "{}", "usage", "args"),
        (vec!["--store", "relative"], "{}", "usage", "args"),
        (vec![], "not json", "invalid_input", "stdin"),
        (vec![], "{}", "invalid_input", "by"),
        (
            vec!["--store", "/nonexistent/store"],
            r#"{"by":"lead","extra":1}"#,
            "invalid_input",
            "extra",
        ),
    ] {
        let (status, v) = ap(&[&["row"], args.as_slice()].concat(), stdin);
        assert_eq!(status, Some(2), "{args:?}");
        let e = &v["error"];
        assert_eq!(
            (e["code"].as_str(), e["field"].as_str(), &e["after"]),
            (Some(code), Some(field), &Value::Null),
            "{args:?}"
        );
    }
}
