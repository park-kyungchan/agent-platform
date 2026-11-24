//! `ap intent` (README "One intent", `specs/ap/intent.md`) against a fake of `ont` that answers as the
//! engine does: the schema's actions on `intent`, `criterion` and `end_delegation` with their refusals,
//! request ids replayed as `already_applied`, the actor's role from the request member `role` or else
//! from `ONT_ROLE`, `expected` revisions, and searches qualified as the engine qualifies them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::{
    host::{Answer, Failure, Reply, System},
    intent::{execute, text_of},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
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
fn refused(v: Value) -> Answer {
    let mut v = v;
    v["status"] = json!("refused");
    reply(false, v)
}
const WRITERS: [&str; 13] = [
    "draft_intent",
    "amend_intent",
    "note_intent",
    "name_lead",
    "assume_intent",
    "confirm_intent",
    "confirm_intent_delegated",
    "add_criterion",
    "check_criterion",
    "waive_criterion",
    "close_intent",
    "retire_intent",
    "end_delegation",
];
/// The fake store: objects by native key, each with its type, its properties and its revision.
struct F {
    calls: Vec<(Vec<String>, Value)>,
    stores: Vec<Option<String>>,
    objects: BTreeMap<String, Value>,
    revisions: BTreeMap<String, u64>,
    applied: BTreeMap<String, (Value, Value)>,
    /// Every apply's actor as the engine records it: action, session, role.
    log: Vec<(String, String, Option<String>)>,
    env: Vec<(String, String)>,
    now: String,
    made: usize,
    busy: usize,
    pauses: usize,
    /// The next apply of this action fails as a process, after this many of its applies passed.
    cut: Option<(&'static str, usize)>,
    /// After the next `get` of an intent, the store names this session its Lead.
    rename: Option<&'static str>,
    unqualified: bool,
    /// The keys the next criteria get, in the order they are added (else `cr<n>`).
    keys: Vec<&'static str>,
    /// The engine's log of the applied `add_criterion` actions, oldest first.
    rows: Vec<Value>,
    /// The rows of one `log` answer.
    page: usize,
}
impl F {
    fn new() -> Self {
        let mut f = Self {
            calls: vec![],
            stores: vec![],
            objects: BTreeMap::new(),
            revisions: BTreeMap::new(),
            applied: BTreeMap::new(),
            log: vec![],
            env: vec![],
            now: "2026-11-17T12:00:00Z".into(),
            made: 0,
            busy: 0,
            pauses: 0,
            cut: None,
            rename: None,
            unqualified: false,
            keys: vec![],
            rows: vec![],
            page: 200,
        };
        // Requests of the user, invented: q-* are current and unrouted unless routed below.
        for (key, kind, said) in [
            ("q-start", "order", "2026-11-17T11:00:00Z"),
            ("q-two", "order", "2026-11-17T11:00:01Z"),
            ("q-yes", "order", "2026-11-17T12:00:05Z"),
            ("q-old", "order", "2026-11-17T11:30:00Z"),
            ("q-name", "order", "2026-11-17T11:40:00Z"),
            ("q-free", "order", "2026-11-17T12:00:06Z"),
            ("q-deleg", "delegation", "2026-11-17T11:50:00Z"),
            ("q-stop", "stop", "2026-11-17T12:00:07Z"),
            ("q-waive", "decision", "2026-11-17T12:00:08Z"),
        ] {
            f.put(
                key,
                json!({"object_type": "request", "kind": kind, "said_at": said, "status": "current",
                    "route": "unrouted", "topic": "walk/one", "words_verified": true, "words": "invented"}),
            );
        }
        f
    }
    fn put(&mut self, key: &str, o: Value) {
        self.objects.insert(key.into(), o);
        *self.revisions.entry(key.into()).or_insert(0) += 1;
    }
    fn modify(&mut self, key: &str, set: Value) {
        let o = self.objects.get_mut(key).unwrap();
        for (k, v) in set.as_object().unwrap() {
            if v.is_null() {
                o.as_object_mut().unwrap().remove(k);
            } else {
                o[k] = v.clone();
            }
        }
        *self.revisions.get_mut(key).unwrap() += 1;
    }
    fn route(&mut self, request: &str, intent: &str) {
        self.modify(
            request,
            json!({"route": "intent", "intent": oid("intent", intent)}),
        );
    }
    /// Schema 24: a request routed to the subject `to` (`route_request`).
    fn route_subject(&mut self, request: &str, to: Value) {
        self.modify(request, json!({"route": "subject", "subject": to}));
    }
    fn o(&self, key: &str) -> &Value {
        &self.objects[key]
    }
    fn names(&self) -> Vec<String> {
        let name = |(a, b): &(Vec<String>, Value)| match a[0].as_str() {
            "action" => format!("apply {}", b["action_type"].as_str().unwrap()),
            "get" => format!("get {}", b["id"]["object_type"].as_str().unwrap()),
            "log" => format!("log {}", b["action_type"].as_str().unwrap()),
            other => format!("{other} {}", b["object_type"].as_str().unwrap()),
        };
        self.calls.iter().map(name).collect()
    }
    fn applies(&self) -> Vec<&Value> {
        let a = self.calls.iter().filter(|c| c.0[0] == "action");
        a.map(|c| &c.1).collect()
    }
    fn criteria_of(&self, intent: &str) -> Vec<&String> {
        let mine = self.objects.iter();
        mine.filter(|(_, o)| {
            o["object_type"] == "criterion" && o["intent"] == oid("intent", intent)
        })
        .map(|(k, _)| k)
        .collect()
    }
    /// The object a reference parameter names, when it exists with that type.
    fn the(&self, p: &Value, kind: &str) -> Option<&Value> {
        let o = self.objects.get(p["native_key"].as_str()?)?;
        (o["object_type"] == kind).then_some(o)
    }
    fn failed(&self, action: &str, p: &Value, role: Option<&str>) -> Vec<&'static str> {
        let mut failed = vec![];
        let intent = self.the(&p["intent"], "intent");
        let status = intent.map(|i| i["status"].as_str().unwrap()).unwrap_or("");
        let open = intent.is_some() && !["met", "retired"].contains(&status);
        let word = |k: &str, kinds: &[&str]| {
            self.the(&p[k], "request").is_some_and(|r| {
                r["status"] == "current" && kinds.contains(&r["kind"].as_str().unwrap())
            })
        };
        let routed = |k: &str, to: &Value| {
            self.the(&p[k], "request")
                .is_some_and(|r| {
                    (r["route"] == "intent" && &r["intent"] == to)
                        || (r["route"] == "subject" && &r["subject"] == to)
                })
        };
        let lead = role == Some("lead");
        let living = |state: &str| {
            let mine = self.objects.values();
            let mut c =
                mine.filter(|o| o["object_type"] == "criterion" && o["intent"] == p["intent"]);
            match state {
                "unwaived" => c.any(|o| o["state"] != "waived"),
                _ => c.any(|o| o["state"] == "open" || o["state"] == "fail"),
            }
        };
        let mut need = |ok: bool, code: &'static str| {
            if !ok {
                failed.push(code);
            }
        };
        match action {
            "draft_intent" => {
                need(
                    self.the(&p["origin"], "request")
                        .is_some_and(|r| r["status"] == "current"),
                    "origin_not_current",
                );
                need(p["outcome"] != "", "outcome_empty");
                need(p["first_user"] != "", "first_user_empty");
            }
            "amend_intent" => {
                need(open, "intent_closed");
                need(p["outcome"] != "", "outcome_empty");
            }
            "note_intent" => need(open, "intent_closed"),
            "name_lead" => {
                need(open, "intent_closed");
                need(
                    word("by", &["order", "decision", "delegation"]),
                    "by_not_the_users_word",
                );
                need(routed("by", &p["intent"]), "by_not_routed_here");
            }
            "assume_intent" => {
                need(status == "draft", "intent_not_draft");
                need(lead, "not_the_lead");
                need(p["reason"] != "", "reason_empty");
            }
            "confirm_intent" => {
                need(["draft", "assumed"].contains(&status), "intent_not_open");
                need(
                    word("by", &["order", "decision", "answer"]),
                    "by_not_the_users_word",
                );
                need(routed("by", &p["intent"]), "by_not_routed_here");
                let said = self.the(&p["by"], "request").map(|r| r["said_at"].clone());
                let later = intent.is_some_and(|i| {
                    said.as_ref().and_then(|s| s.as_str()) > i["amended_at"].as_str()
                });
                need(later, "by_older_than_amendment");
                need(living("unwaived"), "no_criterion");
            }
            "confirm_intent_delegated" => {
                need(["draft", "assumed"].contains(&status), "intent_not_open");
                need(lead, "not_the_lead");
                need(word("delegation", &["delegation"]), "not_a_delegation");
                need(p["reason"] != "", "reason_empty");
                need(living("unwaived"), "no_criterion");
            }
            "add_criterion" => {
                need(open, "intent_closed");
                need(p["statement"] != "", "statement_empty");
                need(p["check"] != "", "check_empty");
            }
            "check_criterion" => {
                need(
                    p["state"] == "pass" || p["state"] == "fail",
                    "state_not_a_result",
                );
                need(p["evidence"] != "", "evidence_empty");
                let c = self.the(&p["criterion"], "criterion");
                need(
                    c.is_some_and(|c| c["state"] != "waived"),
                    "criterion_waived",
                );
            }
            "waive_criterion" => {
                need(
                    word("by", &["decision", "order", "answer"]),
                    "by_not_the_users_word",
                );
                let c = self.the(&p["criterion"], "criterion");
                need(
                    c.is_some_and(|c| routed("by", &c["intent"])),
                    "by_not_routed_here",
                );
                need(p["reason"] != "", "reason_empty");
            }
            "close_intent" => {
                need(status == "confirmed", "intent_not_confirmed");
                need(!living("open"), "criterion_not_passed");
            }
            "retire_intent" => {
                need(open, "intent_closed");
                need(p["reason"] != "", "reason_empty");
            }
            "end_delegation" => {
                need(word("request", &["delegation"]), "not_a_delegation");
                need(
                    word("stopped_by", &["order", "decision", "correction", "stop"]),
                    "by_not_the_users_word",
                );
            }
            other => panic!("action {other}"),
        }
        failed
    }
    fn apply(&mut self, body: &Value) -> Answer {
        let (action, p) = (body["action_type"].as_str().unwrap(), &body["parameters"]);
        assert!(WRITERS.contains(&action), "{action}");
        let role = body
            .get("role")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                let env = self
                    .env
                    .iter()
                    .find(|(k, v)| k == "ONT_ROLE" && !v.is_empty());
                env.map(|(_, v)| v.clone())
            });
        let session = body["session"].as_str().unwrap_or_default().to_owned();
        let derived = format!("{}/{}", body["agent"], body["request_id"]);
        let content = json!([action, p]);
        if let Some((first, edits)) = self.applied.get(&derived) {
            if *first == content {
                return ok(json!({"outcome": "already_applied", "edits": edits}));
            }
            return refused(json!({"code": "request_id_reused"}));
        }
        if let Some((cut, after)) = self.cut {
            if cut == action && after == 0 {
                self.cut = None;
                return Err(Failure {
                    code: "process_failed",
                    field: "ont".into(),
                    detail: Value::Null,
                });
            }
            if cut == action {
                self.cut = Some((cut, after - 1));
            }
        }
        for e in body["expected"].as_array().into_iter().flatten() {
            let key = e["id"]["native_key"].as_str().unwrap();
            if Some(format!("r#{}", self.revisions[key]))
                != e["revision"].as_str().map(str::to_owned)
            {
                return refused(json!({"code": "revision_conflict", "id": e["id"]}));
            }
        }
        if action == "draft_intent"
            && self
                .objects
                .contains_key(p["intent"]["native_key"].as_str().unwrap())
        {
            return refused(json!({"code": "revision_conflict", "id": p["intent"]}));
        }
        let failed = self.failed(action, p, role.as_deref());
        if !failed.is_empty() {
            return refused(json!({"code": "submission_criteria", "failed": failed}));
        }
        self.log.push((action.into(), session, role));
        let key = |p: &Value, k: &str| p[k]["native_key"].as_str().unwrap().to_owned();
        let change =
            |kind: &str, key: &str, change: &str| json!({"change": change, "id": oid(kind, key)});
        let now = self.now.clone();
        let edits = match action {
            "draft_intent" => {
                let k = key(p, "intent");
                let mut o = p.clone();
                o.as_object_mut().unwrap().remove("intent");
                o["object_type"] = json!("intent");
                o["status"] = json!("draft");
                o["amended_at"] = json!(now);
                self.put(&k, o);
                let origin = key(p, "origin");
                self.route(&origin, &k);
                vec![
                    change("intent", &k, "initial"),
                    change("request", &origin, "modification"),
                ]
            }
            "add_criterion" => {
                self.made += 1;
                let k = match self.keys.get(self.made - 1) {
                    Some(k) => (*k).to_owned(),
                    None => format!("cr{}", self.made),
                };
                let mut o = p.clone();
                o["object_type"] = json!("criterion");
                o["state"] = json!("open");
                o["added_at"] = json!(now);
                self.put(&k, o);
                vec![change("criterion", &k, "initial")]
            }
            "check_criterion" | "waive_criterion" => {
                let k = key(p, "criterion");
                let set = match action {
                    "check_criterion" => {
                        json!({"state": p["state"], "evidence": p["evidence"], "checked_at": now})
                    }
                    _ => json!({"state": "waived", "evidence": p["reason"], "waived_by": p["by"]}),
                };
                self.modify(&k, set);
                vec![change("criterion", &k, "modification")]
            }
            "end_delegation" => {
                let k = key(p, "request");
                self.modify(&k, json!({"status": "superseded"}));
                vec![change("request", &k, "modification")]
            }
            _ => {
                let k = key(p, "intent");
                let set = match action {
                    "amend_intent" => {
                        let mut s = p.clone();
                        s.as_object_mut().unwrap().remove("intent");
                        s["status"] = json!("draft");
                        s["amended_at"] = json!(now);
                        s["confirmed_delegated"] = json!(false);
                        s["confirmed_by"] = Value::Null;
                        s
                    }
                    "note_intent" => json!({"notes": p["notes"]}),
                    "name_lead" => json!({"lead_session": p["session"], "lead_named_by": p["by"]}),
                    "assume_intent" => json!({"status": "assumed", "assumed_reason": p["reason"]}),
                    "confirm_intent" => json!({"status": "confirmed", "confirmed_by": p["by"],
                        "confirmed_at": now, "confirmed_delegated": false}),
                    "confirm_intent_delegated" => {
                        json!({"status": "confirmed", "confirmed_by": p["delegation"],
                        "confirmed_at": now, "confirmed_delegated": true})
                    }
                    "close_intent" => {
                        json!({"status": "met", "closed_at": now, "closed_reason": p["reason"]})
                    }
                    _ => {
                        json!({"status": "retired", "closed_at": now, "closed_reason": p["reason"]})
                    }
                };
                self.modify(&k, set);
                vec![change("intent", &k, "modification")]
            }
        };
        if action == "add_criterion" {
            self.rows.push(
                json!({"action_type": action, "outcome": "applied", "parameters": p,
                "edits": edits}),
            );
        }
        self.applied
            .insert(derived, (content, json!(edits.clone())));
        ok(json!({"outcome": "applied", "edits": edits}))
    }
    /// `ont log`: the rows newest first, `limit` of them to a page.
    fn log(&self, body: &Value) -> Answer {
        assert_eq!(body["action_type"], "add_criterion");
        assert_eq!(body["outcome"], "applied");
        assert_eq!(body["parameters"], true, "the parameters name the intent");
        let limit = body["limit"].as_u64().unwrap() as usize;
        assert!((1..=1000).contains(&limit));
        let newest: Vec<&Value> = self.rows.iter().rev().collect();
        let from: usize = body["continuation"]
            .as_str()
            .map_or(0, |c| c.parse().unwrap());
        let to = (from + limit.min(self.page)).min(newest.len());
        let next = (to < newest.len()).then(|| to.to_string());
        ok(json!({"rows": newest[from..to], "continuation": next, "scan_limited": false}))
    }
    fn fields(o: &Value) -> Value {
        let mine = o.as_object().unwrap().iter();
        let fields = mine.filter(|(k, _)| *k != "object_type");
        fields
            .map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
    }
    fn get(&mut self, body: &Value) -> Answer {
        let id = &body["id"];
        let kind = id["object_type"].as_str().unwrap();
        let key = id["native_key"].as_str().unwrap().to_owned();
        if self.the(id, kind).is_none() {
            return refused(json!({"code": "unavailable"}));
        }
        let head = format!("r#{}", self.revisions[&key]);
        let observation = json!({"revision": head, "fields": Self::fields(&self.objects[&key])});
        let answer = ok(
            json!({"objects": [{"id": id, "head": head, "observations": [{"revision": "r#0", "fields": {}}, observation]}]}),
        );
        if kind == "intent"
            && let Some(session) = self.rename.take()
        {
            self.modify(&key, json!({"lead_session": session}));
        }
        answer
    }
    fn search(&self, body: &Value) -> Answer {
        let kind = body["object_type"].as_str().unwrap();
        let all = body["where"]["all"].as_array().unwrap();
        let on = |c: &Value| c["eq"][0].as_str().unwrap().to_owned();
        // As on the engine: a filter on the optional reference `intent` of a request is not qualified.
        let by_intent = kind == "request" && all.iter().any(|c| on(c) == "intent");
        let qualified = !(self.unqualified || by_intent);
        let matches =
            |o: &Value| o["object_type"] == kind && all.iter().all(|c| o[on(c)] == c["eq"][1]);
        let found: Vec<_> = (self.objects.iter())
            .filter(|(_, o)| matches(o))
            .map(|(k, o)| json!({"id": oid(kind, k), "head": {"fields": Self::fields(o)}}))
            .collect();
        let from: usize = body["continuation"]
            .as_str()
            .map_or(0, |c| c.parse().unwrap());
        let page = body["limit"].as_u64().unwrap() as usize;
        let to = (from + page).min(found.len());
        let next = (to < found.len()).then(|| to.to_string());
        ok(
            json!({"basis": {"qualified": qualified}, "objects": found[from..to], "continuation": next}),
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
            return refused(json!({"code": "store_busy"}));
        }
        match args[0].as_str() {
            "action" => {
                assert_eq!(args, ["action", "apply", "--client", "ap"]);
                self.apply(input)
            }
            "get" => self.get(input),
            "search" => self.search(input),
            "log" => self.log(input),
            other => panic!("ont {other}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        panic!("no file is read: {path}")
    }
    fn pause(&mut self, _: std::time::Duration) {
        self.pauses += 1;
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}
fn writer(op: &str, rid: &str) -> Value {
    json!({"op": op, "request_id": rid, "agent": "lead", "session": "s-lead"})
}
fn with(mut v: Value, members: Value) -> Value {
    for (k, x) in members.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}
fn without(mut v: Value, k: &str) -> Value {
    v.as_object_mut().unwrap().remove(k);
    v
}
fn draft() -> Value {
    with(
        writer("draft", "d1"),
        json!({"intent": "widget-report", "outcome": "Every widget is listed.", "first_user": "An invented reader.",
            "origin": "q-start", "criteria": [{"statement": "All listed.", "check": "count rows"},
            {"statement": "It builds.", "check": "make report", "baseline": "none"}]}),
    )
}
fn run(f: &mut F, q: &Value) -> Value {
    match execute(f, None, q) {
        Ok(v) => v,
        Err(e) => panic!("{} {} {} {:?}", e.code, e.field, e.detail, e.after),
    }
}
fn refusal(f: &mut F, q: &Value) -> Box<ap::intent::Refusal> {
    execute(f, None, q).expect_err("a refusal")
}
/// A drafted intent `widget-report` with two criteria, its Lead `s-lead` named by a routed order.
fn led() -> F {
    let mut f = F::new();
    run(&mut f, &draft());
    f.route("q-name", "widget-report");
    let q = with(
        writer("lead", "l1"),
        json!({"intent": "widget-report", "lead_session": "s-lead", "by": "q-name"}),
    );
    run(&mut f, &q);
    f.calls.clear();
    f.log.clear();
    f
}
fn assume(session: &str) -> Value {
    let q = with(
        writer("assume", "a1"),
        json!({"intent": "widget-report", "reason": "The user is away."}),
    );
    with(q, json!({"session": session}))
}
fn delegated(session: &str) -> Value {
    let q = with(
        writer("confirm", "c1"),
        json!({"intent": "widget-report", "delegation": "q-deleg", "reason": "It names this intent."}),
    );
    with(q, json!({"session": session}))
}
fn ap(args: &[&str], stdin: &[u8]) -> (Option<i32>, Vec<u8>, Value) {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("intent")
        .args(args)
        .env("PATH", "/nonexistent")
        .env_remove("ONT_ROLE")
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

// spec: specs/ap/intent.md#R1
#[test]
fn r1_arguments_are_store_and_text_for_show_else_usage() {
    // `PATH` names no program: none of these reaches a store.
    let show = json!({"op": "show", "intent": "widget-report"});
    for (args, stdin) in [
        (vec!["--bogus"], show.clone()),
        (vec!["--store"], show.clone()),
        (vec!["--store", "relative"], show.clone()),
        (vec!["--store", "/a", "--store", "/b"], show.clone()),
        (vec!["--text", "--text"], show.clone()),
        (vec!["--text"], draft()),
        (vec!["--store", "/s", "--text"], assume("s-lead")),
    ] {
        let (status, out, e) = ap(&args, stdin.to_string().as_bytes());
        assert_eq!(status, Some(2), "{args:?}");
        assert!(out.is_empty());
        assert_eq!(
            e,
            json!({"error": {"code": "usage", "field": "args", "detail": null, "after": null, "intent": null, "criteria": []}}),
            "{args:?}"
        );
    }
    // The store is forwarded to every call.
    let mut f = F::new();
    execute(&mut f, Some("/tmp/scratch/store.sqlite"), &draft()).unwrap();
    assert!(f.stores.len() > 3);
    assert!(
        f.stores
            .iter()
            .all(|s| s.as_deref() == Some("/tmp/scratch/store.sqlite"))
    );
}

// spec: specs/ap/intent.md#R2
#[test]
fn r2_a_malformed_request_is_invalid_input_with_its_field_before_any_call() {
    for (q, field) in [
        (json!([1]), "stdin"),
        (json!({"intent": "widget-report"}), "op"),
        (json!({"op": "drop"}), "op"),
        (
            json!({"op": "show", "intent": "widget-report", "extra": 1}),
            "extra",
        ),
        (json!({"op": "show"}), "intent"),
        (with(draft(), json!({"title": "x"})), "title"),
        (without(draft(), "outcome"), "outcome"),
        (without(draft(), "origin"), "origin"),
        (with(draft(), json!({"outcome": 7})), "outcome"),
        (without(assume("s-lead"), "reason"), "reason"),
        (with(writer("criterion", "k1"), json!({"do": "drop"})), "do"),
        (
            with(
                writer("criterion", "k1"),
                json!({"do": "check", "criterion": "cr1", "state": "pass"}),
            ),
            "evidence",
        ),
        (
            with(
                writer("end_delegation", "e1"),
                json!({"request": "q-deleg"}),
            ),
            "stopped_by",
        ),
        (
            with(
                writer("note", "n1"),
                json!({"intent": "widget-report", "notes": "n", "reason": "r"}),
            ),
            "reason",
        ),
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", field),
            "{q}"
        );
        assert!(f.calls.is_empty(), "{q}");
    }
}

// spec: specs/ap/intent.md#R3
#[test]
fn r3_ids_sessions_keys_and_texts_have_their_form() {
    let lead = |s: &str| {
        with(
            writer("lead", "l1"),
            json!({"intent": "widget-report", "lead_session": s, "by": "q-name"}),
        )
    };
    for (q, field) in [
        (
            with(draft(), json!({"request_id": "x".repeat(45)})),
            "request_id",
        ),
        (with(draft(), json!({"request_id": "a b"})), "request_id"),
        (with(draft(), json!({"agent": ""})), "agent"),
        (with(draft(), json!({"session": "s/1"})), "session"),
        (with(draft(), json!({"session": "s".repeat(65)})), "session"),
        (lead("has space"), "lead_session"),
        (lead(&"s".repeat(65)), "lead_session"),
        (with(draft(), json!({"origin": ""})), "origin"),
        (with(draft(), json!({"origin": "o".repeat(513)})), "origin"),
        (
            with(draft(), json!({"outcome": "o".repeat(1201)})),
            "outcome",
        ),
        (
            with(draft(), json!({"first_user": "f".repeat(601)})),
            "first_user",
        ),
        (with(draft(), json!({"why": "w".repeat(801)})), "why"),
        (
            with(draft(), json!({"directory": "d".repeat(301)})),
            "directory",
        ),
        (
            with(
                writer("note", "n1"),
                json!({"intent": "widget-report", "notes": "n".repeat(4001)}),
            ),
            "notes",
        ),
        (
            with(assume("s-lead"), json!({"reason": "r".repeat(601)})),
            "reason",
        ),
        (
            with(
                writer("criterion", "k"),
                json!({"do": "check", "criterion": "cr1", "state": "open", "evidence": "e"}),
            ),
            "state",
        ),
        (
            with(
                writer("criterion", "k"),
                json!({"do": "check", "criterion": "cr1", "state": "pass", "evidence": "e".repeat(1001)}),
            ),
            "evidence",
        ),
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", field),
            "{field}"
        );
        assert!(f.calls.is_empty(), "{field}");
    }
    // The bounds themselves are admitted; an empty text is the store's to refuse.
    let mut f = F::new();
    let q = with(
        draft(),
        json!({"request_id": "x".repeat(44), "session": "s".repeat(64), "outcome": "o".repeat(1200)}),
    );
    assert_eq!(run(&mut f, &q)["outcome"], "applied");
    let e = refusal(&mut F::new(), &with(draft(), json!({"outcome": ""})));
    assert_eq!(e.code, "outcome_empty");
    let mut f = led();
    let q = with(
        writer("criterion", "w1"),
        json!({"do": "waive", "criterion": "cr1", "by": "q-name", "reason": "r".repeat(1000)}),
    );
    assert_eq!(run(&mut f, &q)["state"], "waived");
}

// spec: specs/ap/intent.md#R4
#[test]
fn r4_the_key_of_a_new_intent_is_lower_case_letters_digits_and_dashes() {
    for key in [
        "Widget",
        "widget report",
        "widget/report",
        "1widget",
        "-widget",
        "",
        &"w".repeat(61),
        "wídget",
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &with(draft(), json!({"intent": key})));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", "intent"),
            "{key}"
        );
        // The refused member is carried as null, not as the value that was refused.
        assert_eq!(e.intent, None, "{key}");
        assert!(f.calls.is_empty(), "{key}");
    }
    let mut f = F::new();
    let key = format!("w{}-9", "a".repeat(57));
    assert_eq!(
        run(&mut f, &with(draft(), json!({"intent": key})))["intent"],
        key
    );
}

// spec: specs/ap/intent.md#R5
#[test]
fn r5_at_most_five_questions_each_with_a_default() {
    let q = |n: usize| {
        (0..n)
            .map(|i| json!({"question": format!("Q{i}?"), "default": "yes"}))
            .collect::<Vec<_>>()
    };
    let mut f = F::new();
    let e = refusal(&mut f, &with(draft(), json!({"questions": q(6)})));
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), &e.detail),
        ("too_many_questions", "questions", &json!(6))
    );
    assert!(e.after.is_none() && f.calls.is_empty());
    let amend = with(
        writer("amend", "m1"),
        json!({"intent": "widget-report", "questions": q(6)}),
    );
    let e = refusal(&mut f, &amend);
    assert_eq!(e.code, "too_many_questions");
    assert!(f.calls.is_empty());
    for (bad, field) in [
        (
            json!([{"question": "", "default": "yes"}]),
            "questions.0.question",
        ),
        (json!([{"question": "Q?"}]), "questions.0.default"),
        (
            json!([{"question": "Q?", "default": "d", "answer": 1}]),
            "questions.0.answer",
        ),
        (
            json!([{"question": "Q?", "default": "d", "by": "me"}]),
            "questions.0.by",
        ),
        (json!({"question": "Q?"}), "questions"),
    ] {
        let e = refusal(&mut f, &with(draft(), json!({"questions": bad})));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", field)
        );
    }
    assert!(f.calls.is_empty());
    let mut five = q(5);
    five[1]["answer"] = json!("no");
    run(&mut f, &with(draft(), json!({"questions": five})));
    let stored = &f.o("widget-report")["questions"];
    assert_eq!(stored.as_array().unwrap().len(), 5);
    assert_eq!(
        stored[0],
        json!({"question": "Q0?", "default": "yes", "answer": ""})
    );
    assert_eq!(stored[1]["answer"], "no");
}

// spec: specs/ap/intent.md#R6
#[test]
fn r6_a_refusal_says_how_far_the_call_went() {
    // Nothing applied: `after` null, the intent named.
    let mut f = F::new();
    let e = refusal(&mut f, &with(draft(), json!({"origin": "q-gone"})));
    assert_eq!(
        (e.code.as_str(), e.after.as_deref(), e.intent.as_deref()),
        ("origin_not_current", None, Some("widget-report"))
    );
    assert!(e.criteria.is_empty());
    // Refused at the second criterion: after the first, with its key.
    let mut f = F::new();
    f.cut = Some(("add_criterion", 1));
    let e = refusal(&mut f, &draft());
    assert_eq!(e.code, "process_failed");
    assert_eq!(e.after.as_deref(), Some("criterion-1"));
    assert_eq!(e.intent.as_deref(), Some("widget-report"));
    assert_eq!(e.criteria, ["cr1"]);
    // Refused after the draft, before any criterion.
    let mut f = F::new();
    f.cut = Some(("add_criterion", 0));
    let e = refusal(&mut f, &draft());
    assert_eq!((e.after.as_deref(), e.criteria.len()), (Some("draft"), 0));
    // The line on standard error, from the command (refused before any call: `PATH` names no program).
    let six: Vec<_> = (0..6)
        .map(|_| json!({"question": "Q?", "default": "d"}))
        .collect();
    let (status, out, e) = ap(
        &[],
        with(draft(), json!({"questions": six}))
            .to_string()
            .as_bytes(),
    );
    assert_eq!((status, out.len()), (Some(2), 0));
    assert_eq!(
        e,
        json!({"error": {"code": "too_many_questions", "field": "questions", "detail": 6, "after": null,
            "intent": "widget-report", "criteria": []}})
    );
}

// spec: specs/ap/intent.md#R7
#[test]
fn r7_a_store_refusal_keeps_its_code_and_object_and_a_used_key_is_intent_exists() {
    let mut f = led();
    let e = refusal(
        &mut f,
        &with(
            writer("confirm", "c9"),
            json!({"intent": "widget-report", "by": "q-free"}),
        ),
    );
    assert_eq!(e.code, "by_not_routed_here");
    assert_eq!(
        e.detail,
        json!({"status": "refused", "code": "submission_criteria", "failed": ["by_not_routed_here"]})
    );
    assert_eq!(e.field, "confirm");
    // The same key under another request id.
    let e = refusal(
        &mut f,
        &with(draft(), json!({"request_id": "d2", "origin": "q-two"})),
    );
    assert_eq!(
        (e.code.as_str(), e.field.as_str(), e.after.as_deref()),
        ("intent_exists", "intent", None)
    );
    assert_eq!(e.detail["code"], "revision_conflict");
    assert_eq!(f.o("q-two")["route"], "unrouted");
    // A request id used with other content.
    let e = refusal(
        &mut f,
        &with(draft(), json!({"outcome": "Something else."})),
    );
    assert_eq!(e.code, "request_id_reused");
}

// spec: specs/ap/intent.md#R8
#[test]
fn r8_actions_carry_their_request_ids_the_client_and_the_actor_without_validate() {
    let mut f = led();
    f.route("q-yes", "widget-report");
    for q in [
        with(
            writer("amend", "m1"),
            json!({"intent": "widget-report", "why": "An invented need."}),
        ),
        with(
            writer("note", "n1"),
            json!({"intent": "widget-report", "notes": "Next: the report."}),
        ),
        assume("s-lead"),
        with(
            writer("confirm", "c1"),
            json!({"intent": "widget-report", "by": "q-yes"}),
        ),
        with(
            writer("criterion", "k1"),
            json!({"do": "add", "intent": "widget-report", "statement": "S", "check": "C"}),
        ),
        with(
            writer("criterion", "k2"),
            json!({"do": "check", "criterion": "cr1", "state": "pass", "evidence": "ok"}),
        ),
        with(
            writer("criterion", "k3"),
            json!({"do": "waive", "criterion": "cr2", "by": "q-yes", "reason": "Not needed."}),
        ),
        with(
            writer("retire", "r1"),
            json!({"intent": "widget-report", "reason": "Replaced."}),
        ),
        with(
            writer("end_delegation", "e1"),
            json!({"request": "q-deleg", "stopped_by": "q-stop"}),
        ),
    ] {
        run(&mut f, &q);
    }
    let ids: Vec<_> = f
        .applies()
        .iter()
        .map(|b| {
            (
                b["action_type"].as_str().unwrap(),
                b["request_id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        ids,
        [
            ("amend_intent", "m1-amend"),
            ("note_intent", "n1-note"),
            ("assume_intent", "a1-assume"),
            ("confirm_intent", "c1-confirm"),
            ("add_criterion", "k1-criterion"),
            ("check_criterion", "k2-check"),
            ("waive_criterion", "k3-waive"),
            ("retire_intent", "r1-retire"),
            ("end_delegation", "e1-end"),
        ]
    );
    assert!(
        f.applies()
            .iter()
            .all(|b| b["agent"] == "lead" && b["session"] == "s-lead")
    );
    assert!(
        !f.calls
            .iter()
            .any(|c| c.0[1..].contains(&"validate".to_owned()))
    );
    let mut f = F::new();
    run(&mut f, &draft());
    let ids: Vec<_> = f
        .applies()
        .iter()
        .map(|b| b["request_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["d1-draft", "d1-criterion-1", "d1-criterion-2"]);
}

// spec: specs/ap/intent.md#R9
#[test]
fn r9_the_role_goes_only_with_assume_and_a_delegated_confirm_from_the_lead() {
    // Another session: refused before any apply, with the Lead it names.
    for q in [assume("s-other"), delegated("s-other")] {
        let mut f = led();
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), &e.detail, e.after.as_deref()),
            ("not_the_lead", &json!("s-lead"), None)
        );
        assert_eq!(f.names(), ["get intent"]);
        assert!(f.log.is_empty());
    }
    // An intent without a Lead, and one that does not exist.
    let mut f = F::new();
    run(&mut f, &draft());
    f.calls.clear();
    let e = refusal(&mut f, &assume("s-lead"));
    assert_eq!((e.code.as_str(), &e.detail), ("not_the_lead", &Value::Null));
    assert_eq!(f.names(), ["get intent"]);
    let e = refusal(&mut F::new(), &assume("s-lead"));
    assert_eq!(e.code, "unknown_intent");
    // The Lead's session: the role goes with the apply, and the engine logs it.
    let mut f = led();
    assert_eq!(run(&mut f, &assume("s-lead"))["status"], "assumed");
    assert_eq!(
        f.names(),
        ["get intent", "apply assume_intent", "get intent"]
    );
    assert_eq!(f.applies()[0]["role"], "lead");
    run(&mut f, &delegated("s-lead"));
    assert_eq!(f.applies()[1]["role"], "lead");
    assert_eq!(f.o("widget-report")["confirmed_delegated"], true);
    // Every other apply carries the role `lead` too (roles.md R3), with no `expected`.
    let mut f = led();
    f.route("q-yes", "widget-report");
    run(
        &mut f,
        &with(
            writer("note", "n1"),
            json!({"intent": "widget-report", "notes": "n"}),
        ),
    );
    run(
        &mut f,
        &with(
            writer("confirm", "c2"),
            json!({"intent": "widget-report", "by": "q-yes"}),
        ),
    );
    run(
        &mut f,
        &with(
            writer("criterion", "k1"),
            json!({"do": "check", "criterion": "cr1", "state": "pass", "evidence": "ok"}),
        ),
    );
    run(
        &mut f,
        &with(
            writer("criterion", "k2"),
            json!({"do": "check", "criterion": "cr2", "state": "pass", "evidence": "ok"}),
        ),
    );
    run(
        &mut f,
        &with(
            writer("close", "x1"),
            json!({"intent": "widget-report", "reason": "Passed."}),
        ),
    );
    run(
        &mut f,
        &with(
            writer("end_delegation", "e1"),
            json!({"request": "q-deleg", "stopped_by": "q-stop"}),
        ),
    );
    assert!(f.applies().iter().all(|b| b["role"] == "lead"));
    assert!(
        f.log
            .iter()
            .all(|(_, _, role)| role.as_deref() == Some("lead")),
        "{:?}",
        f.log
    );
}

// spec: specs/ap/intent.md#R10
#[test]
fn r10_the_role_apply_names_the_revision_it_read_so_a_lead_renamed_in_between_ends_it() {
    let mut f = led();
    let revision = format!("r#{}", f.revisions["widget-report"]);
    run(&mut f, &assume("s-lead"));
    assert_eq!(
        f.applies()[0]["expected"],
        json!([{"id": oid("intent", "widget-report"), "revision": revision}])
    );
    // Renamed between the read and the apply: the engine refuses, and nothing is applied for s-lead.
    for q in [
        with(assume("s-lead"), json!({"request_id": "a2"})),
        delegated("s-lead"),
    ] {
        let mut f = led();
        f.rename = Some("s-new");
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), e.after.as_deref()),
            ("revision_conflict", None)
        );
        assert_eq!(f.o("widget-report")["status"], "draft");
        assert!(f.log.is_empty());
    }
}

// spec: specs/ap/intent.md#R11
#[test]
fn r11_ont_role_lead_in_the_environment_refuses_every_writing_call_without_the_role() {
    let ops = [
        with(
            draft(),
            json!({"request_id": "d9", "intent": "other-one", "origin": "q-two"}),
        ),
        with(
            writer("amend", "m1"),
            json!({"intent": "widget-report", "why": "w"}),
        ),
        with(
            writer("note", "n1"),
            json!({"intent": "widget-report", "notes": "n"}),
        ),
        with(
            writer("lead", "l9"),
            json!({"intent": "widget-report", "lead_session": "s-lead", "by": "q-name"}),
        ),
        with(
            writer("confirm", "c1"),
            json!({"intent": "widget-report", "by": "q-yes"}),
        ),
        with(
            writer("criterion", "k1"),
            json!({"do": "add", "intent": "widget-report", "statement": "S", "check": "C"}),
        ),
        with(
            writer("criterion", "k2"),
            json!({"do": "check", "criterion": "cr1", "state": "pass", "evidence": "ok"}),
        ),
        with(
            writer("criterion", "k3"),
            json!({"do": "waive", "criterion": "cr1", "by": "q-name", "reason": "r"}),
        ),
        with(
            writer("close", "x1"),
            json!({"intent": "widget-report", "reason": "r"}),
        ),
        with(
            writer("retire", "r1"),
            json!({"intent": "widget-report", "reason": "r"}),
        ),
        with(
            writer("end_delegation", "e1"),
            json!({"request": "q-deleg", "stopped_by": "q-stop"}),
        ),
    ];
    for q in &ops {
        let mut f = led();
        f.env = vec![("ONT_ROLE".into(), "lead".into())];
        let e = refusal(&mut f, q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str(), e.after.as_deref()),
            ("role_in_environment", "ONT_ROLE", None),
            "{q}"
        );
        assert!(f.calls.is_empty(), "{q}");
    }
    // The role ops: from another session refused as not the Lead, never applied with the variable's role.
    for q in [assume("s-other"), delegated("s-other")] {
        let mut f = led();
        f.env = vec![("ONT_ROLE".into(), "lead".into())];
        assert!(refusal(&mut f, &q).code == "not_the_lead" && f.log.is_empty());
    }
    // From the Lead's session they carry the role themselves.
    let mut f = led();
    f.env = vec![("ONT_ROLE".into(), "lead".into())];
    run(&mut f, &assume("s-lead"));
    // `show` writes nothing and is not refused.
    assert_eq!(
        run(&mut f, &json!({"op": "show", "intent": "widget-report"}))["status"],
        "assumed"
    );
    // Another value of the environment does not change the claim of the body: `lead` (roles.md R3).
    let mut f = led();
    f.env = vec![("ONT_ROLE".into(), "verifier".into())];
    run(&mut f, &ops[2]);
    assert_eq!(
        f.log,
        [("note_intent".into(), "s-lead".into(), Some("lead".into()))]
    );
}

// spec: specs/ap/intent.md#R12
#[test]
fn r12_draft_applies_the_draft_then_each_criterion_in_order_checked_first() {
    let mut f = F::new();
    let v = run(&mut f, &draft());
    assert_eq!(
        f.names(),
        [
            "apply draft_intent",
            "get intent",
            "apply add_criterion",
            "get criterion",
            "apply add_criterion",
            "get criterion"
        ]
    );
    let p = f.applies()[0]["parameters"].clone();
    assert_eq!(p["intent"], oid("intent", "widget-report"));
    assert_eq!(p["origin"], oid("request", "q-start"));
    assert!(p.get("questions").is_none() && p.get("why").is_none());
    assert_eq!(v["criteria"], json!(["cr1", "cr2"]));
    assert_eq!(f.o("cr1")["statement"], "All listed.");
    assert_eq!(f.o("cr2")["baseline"], "none");
    assert!(f.o("cr1").get("baseline").is_none());
    assert_eq!(f.o("q-start")["route"], "intent");
    let c = |n: usize| {
        (0..n)
            .map(|_| json!({"statement": "S", "check": "C"}))
            .collect::<Vec<_>>()
    };
    let mut bad = c(3);
    bad[2]["statement"] = json!("s".repeat(601));
    let mut empty = c(2);
    empty[1]["check"] = json!("");
    let mut long = c(1);
    long[0]["check"] = json!("c".repeat(801));
    let mut base = c(1);
    base[0]["baseline"] = json!("b".repeat(301));
    let mut other = c(1);
    other[0]["state"] = json!("pass");
    for (criteria, field) in [
        (json!([]), "criteria"),
        (json!(c(13)), "criteria"),
        (json!({"statement": "S"}), "criteria"),
        (json!(bad), "criteria.2.statement"),
        (json!(empty), "criteria.1.check"),
        (json!(long), "criteria.0.check"),
        (json!(base), "criteria.0.baseline"),
        (json!(other), "criteria.0.state"),
    ] {
        let mut f = F::new();
        let e = refusal(&mut f, &with(draft(), json!({"criteria": criteria})));
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", field)
        );
        assert!(f.calls.is_empty());
    }
    let mut f = F::new();
    assert_eq!(
        run(&mut f, &with(draft(), json!({"criteria": c(12)})))["criteria"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    let mut f = F::new();
    let v = run(&mut f, &without(draft(), "criteria"));
    assert_eq!(
        v,
        json!({"intent": "widget-report", "outcome": "applied", "status": "draft", "criteria": []})
    );
}

// spec: specs/ap/intent.md#R13
#[test]
fn r13_a_repeated_or_cut_draft_adds_what_is_missing_and_nothing_twice() {
    let mut f = F::new();
    let first = run(&mut f, &draft());
    let before = f.objects.clone();
    let again = run(&mut f, &draft());
    assert_eq!(again["outcome"], "already_applied");
    assert_eq!(again["criteria"], first["criteria"]);
    assert_eq!(f.objects, before);
    // Cut between the two criteria, then the same call.
    let mut f = F::new();
    f.cut = Some(("add_criterion", 1));
    let e = refusal(&mut f, &draft());
    assert_eq!(
        (e.after.as_deref(), e.criteria.clone()),
        (Some("criterion-1"), vec!["cr1".to_owned()])
    );
    assert_eq!(f.criteria_of("widget-report").len(), 1);
    let v = run(&mut f, &draft());
    assert_eq!(v["outcome"], "already_applied");
    assert_eq!(v["criteria"], json!(["cr1", "cr2"]));
    assert_eq!(f.criteria_of("widget-report").len(), 2);
    // Cut after the draft.
    let mut f = F::new();
    f.cut = Some(("add_criterion", 0));
    refusal(&mut f, &draft());
    assert_eq!(f.criteria_of("widget-report").len(), 0);
    run(&mut f, &draft());
    assert_eq!(f.criteria_of("widget-report").len(), 2);
}

// spec: specs/ap/intent.md#R14
#[test]
fn r14_every_apply_is_read_back_and_answered_with_the_status_read() {
    let mut f = led();
    f.route("q-yes", "widget-report");
    let confirm = with(
        writer("confirm", "c1"),
        json!({"intent": "widget-report", "by": "q-yes"}),
    );
    assert_eq!(
        run(&mut f, &confirm),
        json!({"intent": "widget-report", "outcome": "applied", "status": "confirmed"})
    );
    assert_eq!(f.names(), ["apply confirm_intent", "get intent"]);
    let add = with(
        writer("criterion", "k1"),
        json!({"do": "add", "intent": "widget-report", "statement": "S", "check": "C"}),
    );
    assert_eq!(
        run(&mut f, &add),
        json!({"criterion": "cr3", "intent": "widget-report", "outcome": "applied", "state": "open"})
    );
    let end = with(
        writer("end_delegation", "e1"),
        json!({"request": "q-deleg", "stopped_by": "q-stop"}),
    );
    assert_eq!(
        run(&mut f, &end),
        json!({"request": "q-deleg", "outcome": "applied", "status": "superseded"})
    );
    // A confirmation repeated after an amendment: the same request answers what the store holds.
    run(
        &mut f,
        &with(
            writer("amend", "m1"),
            json!({"intent": "widget-report", "why": "w"}),
        ),
    );
    assert_eq!(
        run(&mut f, &confirm),
        json!({"intent": "widget-report", "outcome": "already_applied", "status": "draft"})
    );
    // A read back that finds nothing is unreadable, after the step.
    struct Lost(F);
    impl System for Lost {
        fn call(&mut self, p: &str, a: &[String], i: &Value, ms: u64) -> Answer {
            match a[0].as_str() {
                "get" => refused(json!({"code": "unavailable"})),
                _ => self.0.call(p, a, i, ms),
            }
        }
        fn environment(&self) -> Vec<(String, String)> {
            vec![]
        }
    }
    let mut lost = Lost(F::new());
    let e = execute(&mut lost, None, &draft()).expect_err("unreadable");
    assert_eq!(
        (e.code.as_str(), e.after.as_deref()),
        ("store_answer_unreadable", Some("draft"))
    );
}

// spec: specs/ap/intent.md#R15
#[test]
fn r15_amend_note_lead_close_retire_and_end_apply_their_actions() {
    let mut f = led();
    let amend = with(
        writer("amend", "m1"),
        json!({"intent": "widget-report", "outcome": "New.", "questions": [{"question": "Q?", "default": "d"}]}),
    );
    run(&mut f, &amend);
    assert_eq!(
        f.applies()[0]["parameters"],
        json!({"intent": oid("intent", "widget-report"), "outcome": "New.", "questions": [{"question": "Q?", "default": "d", "answer": ""}]})
    );
    for (q, field) in [
        (with(amend.clone(), json!({"origin": "q-two"})), "origin"),
        (with(amend.clone(), json!({"criteria": []})), "criteria"),
    ] {
        assert_eq!(refusal(&mut f, &q).field, field);
    }
    run(
        &mut f,
        &with(
            writer("note", "n1"),
            json!({"intent": "widget-report", "notes": "Next."}),
        ),
    );
    assert_eq!(f.o("widget-report")["notes"], "Next.");
    // The first Lead is named without any role.
    let mut g = F::new();
    run(&mut g, &draft());
    g.route("q-name", "widget-report");
    let v = run(
        &mut g,
        &with(
            writer("lead", "l1"),
            json!({"intent": "widget-report", "lead_session": "s-x", "by": "q-name"}),
        ),
    );
    assert_eq!(v["status"], "draft");
    let p = &g.applies()[3]["parameters"];
    assert_eq!(
        *p,
        json!({"intent": oid("intent", "widget-report"), "session": "s-x", "by": oid("request", "q-name")})
    );
    assert_eq!(g.o("widget-report")["lead_session"], "s-x");
    let e = refusal(
        &mut g,
        &with(
            writer("lead", "l2"),
            json!({"intent": "widget-report", "lead_session": "s-y", "by": "q-free"}),
        ),
    );
    assert_eq!(e.code, "by_not_routed_here");
    let e = refusal(
        &mut f,
        &with(
            writer("close", "x1"),
            json!({"intent": "widget-report", "reason": "r"}),
        ),
    );
    assert_eq!(e.code, "intent_not_confirmed");
    run(
        &mut f,
        &with(
            writer("retire", "r1"),
            json!({"intent": "widget-report", "reason": "Replaced."}),
        ),
    );
    assert_eq!(f.o("widget-report")["status"], "retired");
    let p = &f.applies().last().unwrap()["parameters"];
    assert_eq!(
        *p,
        json!({"intent": oid("intent", "widget-report"), "reason": "Replaced."})
    );
    run(
        &mut f,
        &with(
            writer("end_delegation", "e1"),
            json!({"request": "q-deleg", "stopped_by": "q-stop"}),
        ),
    );
    let p = &f.applies().last().unwrap()["parameters"];
    assert_eq!(
        *p,
        json!({"request": oid("request", "q-deleg"), "stopped_by": oid("request", "q-stop")})
    );
}

// spec: specs/ap/intent.md#R16
#[test]
fn r16_confirm_has_exactly_one_of_by_and_delegation() {
    let base = with(writer("confirm", "c1"), json!({"intent": "widget-report"}));
    for (q, field) in [
        (base.clone(), "by"),
        (
            with(
                base.clone(),
                json!({"by": "q-yes", "delegation": "q-deleg", "reason": "r"}),
            ),
            "delegation",
        ),
        (
            with(base.clone(), json!({"by": "q-yes", "reason": "r"})),
            "reason",
        ),
        (
            with(base.clone(), json!({"delegation": "q-deleg"})),
            "reason",
        ),
    ] {
        let mut f = led();
        let e = refusal(&mut f, &q);
        assert_eq!(
            (e.code.as_str(), e.field.as_str()),
            ("invalid_input", field)
        );
        assert!(f.calls.is_empty());
    }
    // `by` applies confirm_intent without `expected` and without reading the Lead.
    let mut f = led();
    f.route("q-yes", "widget-report");
    run(&mut f, &with(base.clone(), json!({"by": "q-yes"})));
    assert_eq!(f.names(), ["apply confirm_intent", "get intent"]);
    assert!(f.applies()[0].get("expected").is_none());
    assert_eq!(f.o("widget-report")["confirmed_delegated"], false);
    // An order said before the last amendment does not confirm.
    let mut f = led();
    f.route("q-old", "widget-report");
    assert_eq!(
        refusal(&mut f, &with(base, json!({"by": "q-old"}))).code,
        "by_older_than_amendment"
    );
}

// spec: specs/ap/intent.md#R17
#[test]
fn r17_criterion_adds_checks_and_waives() {
    let mut f = led();
    let add = with(
        writer("criterion", "k1"),
        json!({"do": "add", "intent": "widget-report", "statement": "S", "check": "C", "baseline": "0"}),
    );
    run(&mut f, &add);
    assert_eq!(
        f.applies()[0]["parameters"],
        json!({"intent": oid("intent", "widget-report"), "statement": "S", "check": "C", "baseline": "0"})
    );
    for (q, field) in [
        (with(add.clone(), json!({"statement": ""})), "statement"),
        (
            with(add.clone(), json!({"check": "c".repeat(801)})),
            "check",
        ),
        (with(add.clone(), json!({"criterion": "cr1"})), "criterion"),
    ] {
        assert_eq!(refusal(&mut f, &q).field, field);
    }
    let check = with(
        writer("criterion", "k2"),
        json!({"do": "check", "criterion": "cr1", "state": "fail", "evidence": "1 of 2"}),
    );
    assert_eq!(run(&mut f, &check)["state"], "fail");
    assert_eq!(f.o("cr1")["evidence"], "1 of 2");
    let waive = with(
        writer("criterion", "k3"),
        json!({"do": "waive", "criterion": "cr1", "by": "q-free", "reason": "r"}),
    );
    assert_eq!(refusal(&mut f, &waive).code, "by_not_routed_here");
    let waive = with(waive, json!({"request_id": "k4", "by": "q-name"}));
    let v = run(&mut f, &waive);
    assert_eq!(
        v,
        json!({"criterion": "cr1", "intent": "widget-report", "outcome": "applied", "state": "waived"})
    );
    let names: Vec<_> = f
        .applies()
        .iter()
        .map(|b| b["action_type"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "add_criterion",
            "check_criterion",
            "waive_criterion",
            "waive_criterion"
        ]
    );
}

fn show(f: &mut F) -> Value {
    run(f, &json!({"op": "show", "intent": "widget-report"}))
}
// spec: specs/ap/intent.md#R18
#[test]
fn r18_show_reads_the_intent_its_criteria_and_its_requests_and_writes_nothing() {
    let mut f = led();
    f.route("q-yes", "widget-report");
    f.route("q-two", "other-one");
    // Schema 24: a request routed to the intent as its subject is shown too; one routed to a unit of the same key is not.
    f.route_subject("q-free", oid("intent", "widget-report"));
    f.route_subject("q-stop", oid("unit", "widget-report"));
    f.route_subject("q-waive", oid("intent", "other-one"));
    f.calls.clear();
    let v = show(&mut f);
    assert_eq!(
        f.names(),
        [
            "get intent",
            "search criterion",
            "search request",
            "search request",
            "log add_criterion"
        ]
    );
    assert_eq!(
        f.calls[1].1["where"],
        json!({"all": [{"eq": ["intent", oid("intent", "widget-report")]}]})
    );
    assert_eq!(
        f.calls[2].1["where"],
        json!({"all": [{"eq": ["status", "current"]}, {"eq": ["route", "intent"]}]})
    );
    assert_eq!(
        f.calls[3].1["where"],
        json!({"all": [{"eq": ["status", "current"]}, {"eq": ["route", "subject"]}]})
    );
    assert!(f.applies().is_empty());
    let keys = |v: &Value, k: &str| {
        v[k].as_array()
            .unwrap()
            .iter()
            .map(|c| {
                c[if k == "criteria" {
                    "criterion"
                } else {
                    "request"
                }]
                .clone()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&v, "criteria"), [json!("cr1"), json!("cr2")]);
    // Only the requests routed to this intent, by the time they were said.
    assert_eq!(
        keys(&v, "requests"),
        [json!("q-start"), json!("q-name"), json!("q-yes"), json!("q-free")]
    );
    // Every page, sorted by time then key.
    let mut f = led();
    for i in 0..20 {
        let at = if i % 2 == 0 {
            "2026-11-17T09:00:00Z"
        } else {
            "2026-11-17T08:00:00Z"
        };
        f.put(
            &format!("zc{i:02}"),
            json!({"object_type": "criterion", "intent": oid("intent", "widget-report"),
            "statement": "S", "check": "C", "state": "open", "added_at": at}),
        );
    }
    f.calls.clear();
    let v = show(&mut f);
    assert_eq!(
        f.names()
            .iter()
            .filter(|n| *n == "search criterion")
            .count(),
        2
    );
    let got = keys(&v, "criteria");
    assert_eq!(got.len(), 22);
    assert_eq!(got[0], "zc01");
    assert_eq!(got[9], "zc19");
    assert_eq!(got[10], "zc00");
    assert_eq!(&got[20..], [json!("cr1"), json!("cr2")]);
    // Unqualified is refused; an intent that does not exist is unknown.
    let mut f = led();
    f.unqualified = true;
    let e = refusal(&mut f, &json!({"op": "show", "intent": "widget-report"}));
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("store_answer_unqualified", "criteria")
    );
    let e = refusal(
        &mut F::new(),
        &json!({"op": "show", "intent": "widget-report"}),
    );
    assert_eq!(e.code, "unknown_intent");
}

// spec: specs/ap/intent.md#R18
#[test]
fn r18_show_prints_the_criteria_of_a_draft_in_the_order_it_listed_them() {
    // Three criteria of one call: the same second, and keys (hashes) in another order than the draft's.
    let mut f = F::new();
    f.keys = vec!["k-zz", "k-aa", "k-mm"];
    let q = with(
        draft(),
        json!({"criteria": [{"statement": "First.", "check": "c1"}, {"statement": "Second.", "check": "c2"},
            {"statement": "Third.", "check": "c3"}]}),
    );
    run(&mut f, &q);
    let keys = |v: &Value| {
        let found = v["criteria"].as_array().unwrap().iter();
        found.map(|c| c["criterion"].clone()).collect::<Vec<_>>()
    };
    // The log is read to its end, a page at a time.
    for page in [200, 1] {
        f.page = page;
        f.calls.clear();
        let v = show(&mut f);
        assert_eq!(keys(&v), [json!("k-zz"), json!("k-aa"), json!("k-mm")]);
        let logs = f.calls.iter().filter(|c| c.0[0] == "log").count();
        assert_eq!(logs, if page == 1 { 3 } else { 1 });
        assert!(f.applies().is_empty());
    }
    let text = text_of(&show(&mut f));
    let at = |w: &str| text.find(w).unwrap();
    assert!(
        at("[open] First.") < at("[open] Second.") && at("[open] Second.") < at("[open] Third.")
    );
    // A criterion added later comes last.
    run(
        &mut f,
        &with(
            writer("criterion", "k9"),
            json!({"do": "add", "intent": "widget-report", "statement": "Later.", "check": "c4"}),
        ),
    );
    assert_eq!(
        keys(&show(&mut f)),
        [json!("k-zz"), json!("k-aa"), json!("k-mm"), json!("cr4")]
    );
}

// spec: specs/ap/intent.md#R19
#[test]
fn r19_show_answers_the_intent_as_stored_and_how_it_was_confirmed() {
    let mut f = led();
    let v = show(&mut f);
    for (k, x) in f.o("widget-report").as_object().unwrap() {
        if k != "object_type" {
            assert_eq!(&v[k], x, "{k}");
        }
    }
    assert_eq!(v["intent"], "widget-report");
    assert_eq!(v["confirmed"], "no");
    assert_eq!(
        v["criteria"][1],
        json!({"criterion": "cr2", "statement": "It builds.", "check": "make report", "baseline": "none",
            "state": "open", "evidence": null, "checked_at": null})
    );
    assert_eq!(
        v["requests"][0],
        json!({"request": "q-start", "topic": "walk/one", "kind": "order", "said_at": "2026-11-17T11:00:00Z", "words_verified": true})
    );
    run(&mut f, &delegated("s-lead"));
    assert_eq!(show(&mut f)["confirmed"], "by delegation");
    let mut f = led();
    f.route("q-yes", "widget-report");
    run(
        &mut f,
        &with(
            writer("confirm", "c1"),
            json!({"intent": "widget-report", "by": "q-yes"}),
        ),
    );
    assert_eq!(show(&mut f)["confirmed"], "by the user");
    for c in ["cr1", "cr2"] {
        f.modify(c, json!({"state": "pass"}));
    }
    run(
        &mut f,
        &with(
            writer("close", "x1"),
            json!({"intent": "widget-report", "reason": "Passed."}),
        ),
    );
    assert_eq!(show(&mut f)["confirmed"], "by the user");
}

// spec: specs/ap/intent.md#R20
#[test]
fn r20_show_text_prints_plain_lines() {
    let mut f = led();
    let amend = with(
        writer("amend", "m1"),
        json!({"intent": "widget-report", "not_included": "No charts.",
            "questions": [{"question": "Which format?", "default": "Markdown"}]}),
    );
    run(&mut f, &amend);
    run(
        &mut f,
        &with(
            writer("note", "n1"),
            json!({"intent": "widget-report", "notes": "Next: the report."}),
        ),
    );
    f.modify("cr1", json!({"state": "pass"}));
    run(&mut f, &assume("s-lead"));
    assert_eq!(
        text_of(&show(&mut f)),
        "widget-report assumed (assumed: The user is away.)\noutcome: Every widget is listed.\nfirst user: An invented reader.\n\
         not included: No charts.\nquestion: Which format? (default: Markdown; answer: )\n\
         [pass] All listed. — count rows\n[open] It builds. — make report\nnotes: Next: the report.\n"
    );
    let mut f = led();
    run(&mut f, &delegated("s-lead"));
    let text = text_of(&show(&mut f));
    assert!(
        text.starts_with("widget-report confirmed (confirmed by delegation)\noutcome: "),
        "{text}"
    );
    assert!(
        !text.contains("not included") && !text.contains("notes:") && !text.contains("question:")
    );
}

// spec: specs/ap/intent.md#R21
#[test]
fn r21_store_busy_is_asked_again_five_times_and_nothing_else_is() {
    let mut f = F::new();
    f.busy = 5;
    run(&mut f, &draft());
    assert_eq!(f.pauses, 5);
    let mut f = F::new();
    f.busy = 6;
    let e = refusal(&mut f, &draft());
    assert_eq!((e.code.as_str(), f.calls.len()), ("store_busy", 6));
    let mut f = led();
    let e = refusal(
        &mut f,
        &with(
            writer("close", "x1"),
            json!({"intent": "widget-report", "reason": "r"}),
        ),
    );
    assert_eq!(
        (e.code.as_str(), f.calls.len(), f.pauses),
        ("intent_not_confirmed", 1, 0)
    );
}

// spec: specs/ap/intent.md#R22
#[test]
fn r22_only_the_schema_actions_of_the_verb_write() {
    let mut f = led();
    f.route("q-yes", "widget-report");
    for q in [
        with(
            writer("amend", "m1"),
            json!({"intent": "widget-report", "why": "w"}),
        ),
        with(
            writer("note", "n1"),
            json!({"intent": "widget-report", "notes": "n"}),
        ),
        assume("s-lead"),
        delegated("s-lead"),
        with(
            writer("criterion", "k1"),
            json!({"do": "check", "criterion": "cr1", "state": "pass", "evidence": "ok"}),
        ),
        with(
            writer("criterion", "k2"),
            json!({"do": "waive", "criterion": "cr2", "by": "q-yes", "reason": "r"}),
        ),
        with(
            writer("close", "x1"),
            json!({"intent": "widget-report", "reason": "r"}),
        ),
        with(
            writer("end_delegation", "e1"),
            json!({"request": "q-deleg", "stopped_by": "q-stop"}),
        ),
        json!({"op": "show", "intent": "widget-report"}),
    ] {
        run(&mut f, &q);
    }
    // The fake asserts every apply is one of the verb's actions and panics on any other program or command.
    assert!(
        f.applies()
            .iter()
            .all(|b| WRITERS.contains(&b["action_type"].as_str().unwrap()))
    );
    assert!(
        f.calls
            .iter()
            .all(|c| ["action", "get", "search", "log"].contains(&c.0[0].as_str()))
    );
    assert_eq!(f.o("widget-report")["status"], "met");
}
