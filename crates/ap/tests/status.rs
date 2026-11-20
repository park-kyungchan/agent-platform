//! `ap status` (README "The status", `specs/ap/status.md`) against a fake of `ont` built from the
//! repository's own schema: which properties are required (a filter on another is answered as not
//! qualified, as the engine does), which types grant a derive, the enumerations, pages of 15 objects,
//! views with escaped cells, and an envelope with the engine's clock. Every object is invented.
use ap::{
    host::{Answer, Failure, Reply, System},
    status::{Call, execute, parse, text_of},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, ffi::OsString};

const SCHEMA: &str = include_str!("../../../ontology/work.schema.json");
/// 2026-11-17T12:00:00Z.
const NOW: u64 = 1_794_916_800;

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
fn refused(v: Value) -> Answer {
    let mut v = v;
    v["status"] = json!("refused");
    reply(false, v)
}
/// The fake store: objects by type and native key (their properties as plain values), the files of
/// the host, and what the verb asked.
struct F {
    objects: BTreeMap<(String, String), Value>,
    calls: Vec<(Vec<String>, Value)>,
    stores: Vec<Option<String>>,
    files: BTreeMap<String, String>,
    reads: RefCell<Vec<String>>,
    schema: Value,
    unqualified: Option<&'static str>,
    truncated: bool,
    refuse: Option<(&'static str, Value)>,
    /// A review the view lists and `get` does not know.
    ghost: Option<String>,
    /// The view states one row too many.
    skew: bool,
    /// The applied rows of the action log, oldest first (the fake answers newest first): `action_type`,
    /// `parameters` and `submitted_at`.
    log: Vec<Value>,
    /// Rows to a page of the log, the rows the engine withheld by action type, and a log that repeats its
    /// continuation.
    log_page: usize,
    withheld: BTreeMap<&'static str, u64>,
    log_loops: bool,
}
impl F {
    fn empty() -> Self {
        Self {
            objects: BTreeMap::new(),
            calls: vec![],
            stores: vec![],
            files: BTreeMap::new(),
            reads: RefCell::new(vec![]),
            schema: serde_json::from_str(SCHEMA).unwrap(),
            unqualified: None,
            truncated: false,
            refuse: None,
            ghost: None,
            skew: false,
            log: vec![],
            log_page: 100,
            withheld: BTreeMap::new(),
            log_loops: false,
        }
    }
    fn put(&mut self, kind: &str, key: &str, fields: Value) {
        self.objects.insert((kind.into(), key.into()), fields);
    }
    fn ok(&self, result: Value) -> Answer {
        // The clock moves with every answer: only the first one is the verb's `at`.
        let now = NOW + self.calls.len() as u64 - 1;
        let version = self.schema["version"].clone();
        reply(
            true,
            json!({"status":"ok","now":now,"schema":{"work":{"version":version}},"result":result}),
        )
    }
    fn names(&self) -> Vec<&str> {
        self.calls.iter().map(|c| c.0[0].as_str()).collect()
    }
    fn fields(o: &Value) -> Value {
        let mine = o.as_object().unwrap().iter();
        mine.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
    }
    fn search(&self, b: &Value) -> Answer {
        let kind = b["object_type"].as_str().unwrap();
        let all = b["where"]["all"].as_array().unwrap();
        let props = &self.schema["object_types"][kind]["properties"];
        let mut qualified = self.unqualified != Some(kind);
        let prop = |c: &Value| c.as_object().unwrap().values().next().unwrap()[0].clone();
        for c in all {
            // As on the engine: a filter on an optional property is not qualified.
            qualified &= props[prop(c).as_str().unwrap()]["required"] == true;
        }
        let holds = |o: &Value| {
            all.iter().all(|c| match (c.get("eq"), c.get("in")) {
                (Some(e), _) => o[e[0].as_str().unwrap()] == e[1],
                (_, Some(i)) => i[1]
                    .as_array()
                    .unwrap()
                    .contains(&o[i[0].as_str().unwrap()]),
                _ => panic!("filter {c}"),
            })
        };
        let found: Vec<Value> = (self.objects.iter())
            .filter(|((k, _), o)| k == kind && holds(o))
            .map(|((k, key), o)| json!({"id": oid(k, key), "head": {"fields": Self::fields(o)}}))
            .collect();
        let from: usize = b["continuation"].as_str().map_or(0, |c| c.parse().unwrap());
        let page = b["limit"].as_u64().unwrap() as usize;
        assert!((1..=15).contains(&page), "a page of {page}");
        let to = (from + page).min(found.len());
        let next = (to < found.len()).then(|| to.to_string());
        self.ok(
            json!({"basis": {"qualified": qualified}, "objects": found[from..to], "continuation": next}),
        )
    }
    fn get(&self, b: &Value) -> Answer {
        let (kind, key) = (
            b["id"]["object_type"].as_str().unwrap(),
            b["id"]["native_key"].as_str().unwrap(),
        );
        let Some(o) = self.objects.get(&(kind.to_owned(), key.to_owned())) else {
            return refused(json!({"code": "unavailable"}));
        };
        let observation = json!({"revision": "r#1", "fields": Self::fields(o)});
        self.ok(
            json!({"objects": [{"id": b["id"], "head": "r#1", "observations": [{"revision": "r#0", "fields": {}}, observation]}]}),
        )
    }
    fn render(&self, b: &Value) -> Answer {
        let view = b["view"].as_str().unwrap();
        assert_eq!(
            b["args"].as_object().unwrap().len(),
            1,
            "one argument: the unit"
        );
        let unit = b["args"]["unit"].as_str().unwrap();
        let (kind, header) = match view {
            "reviews_of_unit" => (
                "review",
                "Key | Kind | Head | Round | Status | Verdict | Closed at",
            ),
            "runs_of_unit" => (
                "run",
                "Key | Role | Model | Status | State | Started at | Ended at",
            ),
            other => panic!("view {other}"),
        };
        let columns = &self.schema["views"][view]["columns"];
        let mut rows: Vec<(&String, &Value)> = (self.objects.iter())
            .filter(|((k, _), o)| k == kind && o["unit"] == oid("unit", unit))
            .map(|((_, key), o)| (key, o))
            .collect();
        rows.sort_by_key(|(k, o)| (o["started_at"].as_str().unwrap().to_owned(), (*k).clone()));
        let cell = |v: &Value| match v {
            Value::Null => "(missing)".to_owned(),
            Value::String(t) => t
                .replace('\\', "\\\\")
                .replace('|', "\\|")
                .replace('\n', " "),
            other => other.to_string(),
        };
        let mut md = format!(
            "<!-- ont:render {view} unit={unit} schema=work@11 -->\n| {header} |\n| --- | --- | --- | --- | --- | --- | --- |\n"
        );
        for (key, o) in &rows {
            let cells: Vec<String> = columns
                .as_array()
                .unwrap()
                .iter()
                .map(|c| match c["property"].as_str().unwrap() {
                    "$key" => cell(&json!(key)),
                    p => cell(&o[p]),
                })
                .collect();
            md += &format!("| {} |\n", cells.join(" | "));
        }
        let mut count = rows.len() + usize::from(self.skew);
        if let Some(ghost) = self.ghost.as_ref().filter(|_| view == "reviews_of_unit") {
            md +=
                &format!("| {ghost} | page | h | (missing) | running | (missing) | (missing) |\n");
            count += 1;
        }
        md += "<!-- /ont:render -->";
        reply(
            true,
            json!({"markdown": md, "rows": count, "schema": "work@11", "status": "ok",
                "truncated": self.truncated, "view": view}),
        )
    }
    /// `ont log`: the rows of one action type newest first, `limit` of them to a page.
    fn log(&self, b: &Value) -> Answer {
        assert_eq!(b["outcome"], "applied");
        assert_eq!(b["parameters"], true, "the parameters name the unit");
        assert_eq!(b["limit"], 100);
        let action = b["action_type"].as_str().unwrap();
        let newest: Vec<&Value> = (self.log.iter().rev())
            .filter(|r| r["action_type"] == action)
            .collect();
        let at = b["continuation"].as_str().filter(|_| !self.log_loops);
        let from: usize = at.map_or(0, |c| c.parse().unwrap());
        let to = (from + self.log_page).min(newest.len());
        let mut next = (to < newest.len()).then(|| to.to_string());
        if self.log_loops {
            next = Some("again".to_owned());
        }
        let withheld = self.withheld.get(action).copied().unwrap_or(0);
        self.ok(
            json!({"rows": newest[from..to], "continuation": next, "scan_limited": false,
            "skipped": 0, "withheld": withheld}),
        )
    }
    fn describe(&self) -> Answer {
        let map = |kind: &str| -> Vec<Value> {
            let all = self.schema[kind].as_object().unwrap().iter();
            all.map(|(name, v)| {
                let mut v = v.clone();
                v["name"] = json!(name);
                v
            })
            .collect()
        };
        let list = |v: &Value| -> Value {
            let all = v.as_object().unwrap().iter();
            all.map(|(k, p)| json!({"name": k, "description": p["description"], "type": p["type"], "required": p["required"]}))
                .collect()
        };
        let types: Vec<Value> = map("object_types")
            .into_iter()
            .map(|t| json!({"name": t["name"], "description": t["description"], "properties": list(&t["properties"])}))
            .collect();
        let actions: Vec<Value> = map("action_types")
            .into_iter()
            .map(|a| {
                let criteria = a["submission_criteria"].as_array().unwrap();
                let criteria: Vec<Value> = criteria.iter().map(|c| json!({"code": c["code"], "message": c["message"]})).collect();
                json!({"name": a["name"], "description": a["description"], "parameters": list(&a["parameters"]), "criteria": criteria})
            })
            .collect();
        let views: Vec<Value> = map("views")
            .into_iter()
            .map(|v| json!({"name": v["name"], "args": v["args"], "object_type": v["object_type"]}))
            .collect();
        self.ok(
            json!({"object_types": types, "action_types": actions, "link_types": map("link_types"),
            "views": views, "functions": [], "sources": [], "domains": []}),
        )
    }
    fn aggregate(&self, b: &Value) -> Answer {
        let (kind, property) = (
            b["object_type"].as_str().unwrap(),
            b["group_by"][0].as_str().unwrap(),
        );
        assert_eq!(b["metrics"], json!(["count"]));
        assert_eq!(
            b["where"],
            json!({"in": [property, []]}),
            "an aggregate that scans no object"
        );
        if !self.schema["grants"][0]["derive"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d == kind)
        {
            return refused(json!({"code": "derive_forbidden"}));
        }
        let t = &self.schema["object_types"][kind]["properties"][property]["type"]["value_type"];
        let enumerated = &self.schema["value_types"][t.as_str().unwrap()]["enum"];
        match enumerated.as_array() {
            Some(members) => {
                let groups: Vec<Value> = members
                    .iter()
                    .map(|m| json!({"count": 0, "key": {property: m}}))
                    .collect();
                self.ok(json!({"kind": "aggregation", "groups": groups}))
            }
            None => self.ok(json!({"kind": "aggregation"})),
        }
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
        if let Some((command, answer)) = &self.refuse
            && *command == args[0]
        {
            return refused(answer.clone());
        }
        match args[0].as_str() {
            "get" => self.get(input),
            "search" => self.search(input),
            "render" => self.render(input),
            "describe" => self.describe(),
            "aggregate" => self.aggregate(input),
            "log" => self.log(input),
            other => panic!("ont {other}: the verb reads only"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_owned());
        match self.files.get(path) {
            Some(t) => Ok(t.clone().into_bytes()),
            None => Err(std::io::ErrorKind::NotFound.into()),
        }
    }
    fn pause(&mut self, _: std::time::Duration) {}
}

fn run(f: &mut F, call: &Call) -> Value {
    f.calls.clear();
    f.stores.clear();
    match execute(f, None, call) {
        Ok(v) => v,
        Err(e) => panic!("{} {} {}", e.code, e.field, e.detail),
    }
}
fn refusal(f: &mut F, call: &Call) -> Failure {
    f.calls.clear();
    f.stores.clear();
    execute(f, None, call).expect_err("a refusal")
}
fn key(k: &str) -> Call {
    Call::Key(k.into())
}
fn term(w: &str) -> Call {
    Call::Term(w.into())
}
fn column(v: &Value, member: &str) -> Vec<Value> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x[member].clone())
        .collect()
}
fn strs(v: &[Value]) -> Vec<&str> {
    v.iter().map(|x| x.as_str().unwrap_or("<null>")).collect()
}

/// Requests: `n` of them, `said` at the given times; routed and superseded ones are not unrouted.
fn request(f: &mut F, key: &str, said: &str, route: &str, status: &str) {
    f.put("request", key, json!({"topic": "walk/one", "kind": "order", "said_at": said, "status": status,
        "route": route, "words_verified": key.len().is_multiple_of(2), "words": "invented", "reading": "invented"}));
}
fn intent(f: &mut F, key: &str, status: &str, more: Value) {
    let mut o = json!({"outcome": format!("Outcome of {key}."), "first_user": "An invented reader.", "status": status,
        "origin": oid("request", "q-start"), "amended_at": "2026-11-17T09:00:00Z"});
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    f.put("intent", key, o);
}
fn criterion(f: &mut F, key: &str, intent: &str, state: &str) {
    f.put(
        "criterion",
        key,
        json!({"intent": oid("intent", intent), "statement": format!("{key} holds."),
        "check": "count", "state": state, "added_at": "2026-11-17T09:00:00Z"}),
    );
}
fn unit(f: &mut F, key: &str, state: &str, more: Value) {
    let mut o = json!({"lane": "objective", "class": "plain", "objective": "An invented state.", "state": state,
        "added_at": "2026-11-17T09:00:00Z", "amended_at": "2026-11-17T09:00:00Z", "rounds": 0, "writers": []});
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    f.put("unit", key, o);
}
fn finding(f: &mut F, key: &str, severity: &str, state: &str, seen: &str, more: Value) {
    let mut o = json!({"kind": "drift", "detector": "invented", "summary": format!("Summary of {key}."),
        "severity": severity, "state": state, "first_seen_at": "2026-11-16T00:00:00Z", "last_seen_at": seen});
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    f.put("finding", key, o);
}
fn hold(f: &mut F, key: &str, status: &str, set: &str, more: Value) {
    let mut o = json!({"task": oid("task", "invented-task"), "effect": format!("effect of {key}"), "until": "a condition",
        "status": status, "set_at": set});
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    f.put("hold", key, o);
}
fn run_object(f: &mut F, key: &str, status: &str, started: &str, more: Value) {
    let mut o = json!({"role": "implementer", "runtime": "claude", "model": "invented-model", "status": status,
        "started_at": started});
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    f.put("run", key, o);
}
const CONFIG: &str = "/etc/agent-platform/deliver.json";
const LAST: &str = "/var/lib/agent-platform/deliver/last.json";
/// An invented store, with the keys in an order the verb must not rely on.
fn fixture() -> F {
    let mut f = F::empty();
    request(
        &mut f,
        "q-start",
        "2026-11-17T08:00:00Z",
        "intent",
        "current",
    );
    request(&mut f, "q-b", "2026-11-17T10:30:00Z", "unrouted", "current");
    request(&mut f, "q-a", "2026-11-15T10:30:00Z", "unrouted", "current");
    request(
        &mut f,
        "q-recent",
        "2026-11-17T11:00:00Z",
        "unrouted",
        "current",
    );
    request(
        &mut f,
        "q-old-superseded",
        "2026-11-10T10:30:00Z",
        "unrouted",
        "superseded",
    );
    request(
        &mut f,
        "q-aside",
        "2026-11-10T10:30:00Z",
        "aside",
        "current",
    );
    let long = "\u{3042}".repeat(100);
    intent(
        &mut f,
        "beta-index",
        "confirmed",
        json!({"confirmed_delegated": true, "lead_session": "s-lead"}),
    );
    intent(&mut f, "alpha-report", "assumed", json!({"outcome": long}));
    intent(&mut f, "delta-draft", "draft", json!({}));
    intent(&mut f, "gamma-done", "met", json!({}));
    for (k, i, s) in [
        ("c1", "alpha-report", "open"),
        ("c2", "alpha-report", "pass"),
        ("c3", "alpha-report", "pass"),
        ("c4", "beta-index", "fail"),
        ("c5", "beta-index", "waived"),
        ("c6", "gamma-done", "pass"),
    ] {
        criterion(&mut f, k, i, s);
    }
    let a = || oid("intent", "alpha-report");
    unit(
        &mut f,
        "u-run",
        "running",
        json!({"intent": a(), "repository": "o/r", "pull_request": 7, "head": "abc123"}),
    );
    unit(
        &mut f,
        "u-ready",
        "ready",
        json!({"intent": a(), "class": "contract", "waits_on": "a unit; a limit", "depends_on": [oid("unit", "u-run")]}),
    );
    unit(
        &mut f,
        "u-stop",
        "stopped",
        json!({"intent": oid("intent", "beta-index"), "stop_cause": "ci_failed", "stop_detail": "x"}),
    );
    unit(
        &mut f,
        "u-acc",
        "accepted",
        json!({"intent": oid("intent", "beta-index"), "repository": "o/r", "pull_request": 9}),
    );
    unit(&mut f, "u-draft", "draft", json!({"intent": a()}));
    unit(
        &mut f,
        "u-merged",
        "merged",
        json!({"intent": a(), "ended_at": "2026-11-16T10:00:00Z", "merged_commit": "def456"}),
    );
    unit(
        &mut f,
        "u-small",
        "ready",
        json!({"lane": "small", "origin": oid("request", "q-start")}),
    );
    finding(
        &mut f,
        "f-info",
        "info",
        "open",
        "2026-11-17T11:50:00Z",
        json!({}),
    );
    finding(
        &mut f,
        "f-act-old",
        "act",
        "open",
        "2026-11-17T08:00:00Z",
        json!({}),
    );
    finding(
        &mut f,
        "f-act-new",
        "act",
        "open",
        "2026-11-17T11:00:00Z",
        json!({}),
    );
    finding(
        &mut f,
        "f-urgent",
        "urgent",
        "open",
        "2026-11-17T09:00:00Z",
        json!({}),
    );
    finding(
        &mut f,
        "f-urgent-planned",
        "urgent",
        "planned",
        "2026-11-17T10:00:00Z",
        json!({"unit": oid("unit", "u-run")}),
    );
    finding(
        &mut f,
        "f-act-planned",
        "act",
        "planned",
        "2026-11-17T10:00:00Z",
        json!({"intent": oid("intent", "beta-index")}),
    );
    finding(
        &mut f,
        "f-dismissed",
        "urgent",
        "dismissed",
        "2026-11-17T11:59:00Z",
        json!({}),
    );
    run_object(
        &mut f,
        "act-run-2",
        "running",
        "2026-11-17T11:00:00Z",
        json!({"unit": oid("unit", "u-run")}),
    );
    run_object(
        &mut f,
        "act-run-1",
        "running",
        "2026-11-17T10:00:00Z",
        json!({"intent": a(), "role": "lead"}),
    );
    run_object(
        &mut f,
        "act-run-done",
        "closed",
        "2026-11-17T09:00:00Z",
        json!({"unit": oid("unit", "u-run"), "state": "ok", "ended_at": "2026-11-17T09:30:00Z"}),
    );
    hold(
        &mut f,
        "h-user",
        "active",
        "2026-11-17T07:00:00Z",
        json!({}),
    );
    hold(
        &mut f,
        "h-unit",
        "active",
        "2026-11-17T08:00:00Z",
        json!({"unit": oid("unit", "u-run"), "scope": "one unit"}),
    );
    hold(
        &mut f,
        "h-intent",
        "active",
        "2026-11-17T06:00:00Z",
        json!({"intent": a()}),
    );
    hold(
        &mut f,
        "h-other",
        "active",
        "2026-11-17T05:00:00Z",
        json!({"intent": oid("intent", "beta-index")}),
    );
    hold(
        &mut f,
        "h-lifted",
        "lifted",
        "2026-11-17T04:00:00Z",
        json!({}),
    );
    f.files.insert(
        CONFIG.into(),
        json!({"state": "/var/lib/agent-platform/deliver"}).to_string(),
    );
    f.files.insert(LAST.into(), json!({"at": "2026-11-17T11:30:00Z", "end": "current", "tip": "0123abc", "target": "/usr/local/bin/ap", "code": null}).to_string());
    f
}

fn args(a: &[&str]) -> Result<ap::status::Args, Failure> {
    let a: Vec<OsString> = a.iter().map(OsString::from).collect();
    parse(&a)
}
fn status(args: &[&str]) -> (Option<i32>, Vec<u8>, Value) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("status")
        .args(args)
        .env("PATH", "/nonexistent")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let err = serde_json::from_slice(&out.stderr).unwrap_or(Value::Null);
    (out.status.code(), out.stdout, err)
}

// spec: specs/ap/status.md#R1
#[test]
fn r1_arguments_are_store_text_term_and_one_key_else_usage() {
    // `PATH` names no program: none of these reaches a store.
    let long = "k".repeat(513);
    for a in [
        vec!["--bogus"],
        vec!["--store"],
        vec!["--store", "relative"],
        vec!["--store", "/a", "--store", "/b"],
        vec!["--text", "--text"],
        vec!["--term"],
        vec!["--term", "a b"],
        vec!["--term", ""],
        vec!["--term", "a", "--term", "b"],
        vec!["--term", "unit", "some-key"],
        vec!["one", "two"],
        vec!["-x"],
        vec![long.as_str()],
        vec![""],
        vec!["a\u{1}b"],
    ] {
        let (status, out, e) = status(&a);
        assert_eq!(status, Some(2), "{a:?}");
        assert!(out.is_empty());
        assert_eq!(
            e,
            json!({"error": {"code": "usage", "field": "args", "detail": null}}),
            "{a:?}"
        );
    }
    for ok in [
        vec![],
        vec!["--text"],
        vec!["u-run", "--text", "--store", "/s"],
        vec!["--term", "head_not_reviewed"],
        vec!["--store", "/s", "--term", "A.b-c_1", "--text"],
    ] {
        assert!(args(&ok).is_ok(), "{ok:?}");
    }
    // The store is forwarded to every call, and the verb reads no standard input.
    let mut f = fixture();
    execute(&mut f, Some("/tmp/scratch/store.sqlite"), &Call::Overview).unwrap();
    assert!(f.stores.len() > 5);
    assert!(
        f.stores
            .iter()
            .all(|s| s.as_deref() == Some("/tmp/scratch/store.sqlite"))
    );
}

// spec: specs/ap/status.md#R2
#[test]
fn r2_the_verb_only_reads() {
    for call in [
        Call::Overview,
        key("alpha-report"),
        key("u-run"),
        term("unit"),
    ] {
        let mut f = fixture();
        run(&mut f, &call); // the fake panics on any command that is not a read
        let names = f.names();
        assert!(
            names
                .iter()
                .all(|n| ["get", "search", "render", "describe", "aggregate"].contains(n)),
            "{names:?}"
        );
        // Only the host's two files are read, and nothing is written (the host trait has no write).
        assert!(
            f.reads.borrow().iter().all(|p| p == CONFIG || p == LAST),
            "{:?}",
            f.reads
        );
    }
}

// spec: specs/ap/status.md#R2
#[test]
fn r2_the_log_is_read_by_the_indicators_only() {
    let mut f = indicator_store();
    indicators_of(&mut f, None);
    let reads = ["get", "search", "render", "describe", "aggregate", "log"];
    assert!(
        f.names().iter().all(|n| reads.contains(n)),
        "{:?}",
        f.names()
    );
    assert!(f.names().contains(&"log"));
    assert!(f.reads.borrow().is_empty(), "no file is read");
    for call in [Call::Overview, key("u-a"), key("vps"), term("unit")] {
        let mut f = indicator_store();
        f.put(
            "request",
            "q-start",
            json!({"topic": "t", "kind": "order", "said_at": "2026-11-17T08:00:00Z",
            "status": "current", "route": "intent", "words_verified": true}),
        );
        let _ = execute(&mut f, None, &call);
        assert!(!f.names().contains(&"log"), "{:?}", f.names());
    }
}

// spec: specs/ap/status.md#R3
#[test]
fn r3_every_list_is_complete_qualified_and_filtered_by_required_properties() {
    let mut f = fixture();
    for i in 0..40 {
        unit(
            &mut f,
            &format!("p{i:03}"),
            "merged",
            json!({"intent": oid("intent", "alpha-report")}),
        );
        request(
            &mut f,
            &format!("q-bulk-{i:03}"),
            "2026-11-17T09:00:00Z",
            "unrouted",
            "current",
        );
        finding(
            &mut f,
            &format!("f-bulk-{i:03}"),
            "info",
            "open",
            "2026-11-17T09:00:00Z",
            json!({}),
        );
    }
    let v = run(&mut f, &Call::Overview);
    assert_eq!(v["requests_unrouted"]["count"], 43);
    assert_eq!(v["findings"].as_array().unwrap().len(), 46);
    // Units of every state are read for the counts: all 40 merged ones are counted.
    let alpha = &v["intents"].as_array().unwrap()[0];
    assert_eq!(alpha["units"]["merged"], 41);
    let pages = f
        .calls
        .iter()
        .filter(|c| c.0[0] == "search" && c.1["object_type"] == "unit")
        .count();
    assert!(pages >= 3, "{pages} pages of units");
    for (args, body) in &f.calls {
        if args[0] != "search" {
            continue;
        }
        let kind = body["object_type"].as_str().unwrap();
        for c in body["where"]["all"].as_array().unwrap() {
            let p = c.as_object().unwrap().values().next().unwrap()[0]
                .as_str()
                .unwrap();
            assert_eq!(
                f.schema["object_types"][kind]["properties"][p]["required"], true,
                "{kind}.{p}"
            );
        }
    }
    // A list that is not qualified is refused naming the list.
    for (kind, list) in [
        ("request", "requests"),
        ("intent", "intents"),
        ("unit", "units"),
        ("criterion", "criteria"),
        ("run", "runs"),
        ("finding", "findings"),
        ("hold", "holds"),
    ] {
        let mut f = fixture();
        f.unqualified = Some(kind);
        let e = refusal(&mut f, &Call::Overview);
        assert_eq!(
            (e.code, e.field.as_str()),
            ("store_answer_unqualified", list),
            "{kind}"
        );
    }
}

// spec: specs/ap/status.md#R4
#[test]
fn r4_the_time_and_the_schema_are_the_first_answers_of_the_store() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    // The fake's clock moves with every answer: the first one is the verb's `at`.
    assert_eq!(v["at"], "2026-11-17T12:00:00Z");
    assert_eq!(v["schema"], 11);
    let members: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        members,
        [
            "at",
            "delivery",
            "findings",
            "holds",
            "intents",
            "needs_lead",
            "requests_unrouted",
            "runs",
            "schema",
            "units"
        ]
    );
    assert_eq!(f.calls[0].1["object_type"], "request");
    // The time is not a clock of the host: another envelope, another print.
    let mut f = fixture();
    f.schema["version"] = json!(12);
    let v = run(&mut f, &Call::Overview);
    assert_eq!(v["schema"], 12);
}

// spec: specs/ap/status.md#R5
#[test]
fn r5_unrouted_requests_are_counted_and_the_oldest_twenty_listed() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    let r = &v["requests_unrouted"];
    assert_eq!(r["count"], 3);
    assert_eq!(
        strs(&column(&r["oldest"], "request")),
        ["q-a", "q-b", "q-recent"]
    );
    assert_eq!(
        r["oldest"][0],
        json!({"request": "q-a", "topic": "walk/one", "kind": "order", "said_at": "2026-11-15T10:30:00Z", "words_verified": false})
    );
    for i in 0..30 {
        request(
            &mut f,
            &format!("q-m{i:02}"),
            &format!("2026-11-16T10:{i:02}:00Z"),
            "unrouted",
            "current",
        );
    }
    let v = run(&mut f, &Call::Overview);
    assert_eq!(v["requests_unrouted"]["count"], 33);
    let oldest = column(&v["requests_unrouted"]["oldest"], "request");
    assert_eq!(oldest.len(), 20);
    assert_eq!(strs(&oldest)[..3], ["q-a", "q-m00", "q-m01"]);
}

// spec: specs/ap/status.md#R6
#[test]
fn r6_each_open_intent_is_listed_with_its_criteria_and_units() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    let intents = v["intents"].as_array().unwrap();
    assert_eq!(
        strs(&column(&v["intents"], "intent")),
        ["alpha-report", "beta-index", "delta-draft"]
    );
    let alpha = &intents[0];
    assert_eq!(alpha["status"], "assumed");
    assert_eq!(alpha["confirmed"], "no");
    assert_eq!(alpha["lead_session"], Value::Null);
    assert_eq!(
        alpha["criteria"],
        json!({"open": 1, "pass": 2, "fail": 0, "waived": 0})
    );
    assert_eq!(
        alpha["units"],
        json!({"draft": 1, "merged": 1, "ready": 1, "running": 1})
    );
    // 200 bytes cut back to a character boundary: 66 characters of three bytes.
    let outcome = alpha["outcome"].as_str().unwrap();
    assert_eq!(outcome.len(), 198);
    assert_eq!(alpha["outcome_cut"], true);
    assert!(
        f.objects[&("intent".to_owned(), "alpha-report".to_owned())]["outcome"]
            .as_str()
            .unwrap()
            .starts_with(outcome)
    );
    let beta = &intents[1];
    assert_eq!(
        (beta["confirmed"].as_str(), beta["lead_session"].as_str()),
        (Some("by delegation"), Some("s-lead"))
    );
    assert_eq!(
        beta["criteria"],
        json!({"open": 0, "pass": 0, "fail": 1, "waived": 1})
    );
    assert_eq!(
        (
            intents[2]["outcome"].as_str(),
            intents[2]["outcome_cut"].as_bool()
        ),
        (Some("Outcome of delta-draft."), Some(false))
    );
    assert_eq!(intents[2]["units"], json!({}));
}

// spec: specs/ap/status.md#R7
#[test]
fn r7_live_units_are_ordered_and_carry_what_waits_or_stopped_them() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    assert_eq!(
        strs(&column(&v["units"], "unit")),
        ["u-stop", "u-acc", "u-run", "u-ready", "u-small"]
    );
    let u = v["units"].as_array().unwrap();
    assert_eq!(
        u[0],
        json!({"unit": "u-stop", "intent": "beta-index", "lane": "objective", "class": "plain", "state": "stopped",
        "repository": null, "pull_request": null, "head": null, "depends_on": [], "stop_cause": "ci_failed"})
    );
    assert_eq!(
        u[2],
        json!({"unit": "u-run", "intent": "alpha-report", "lane": "objective", "class": "plain", "state": "running",
        "repository": "o/r", "pull_request": 7, "head": "abc123", "depends_on": []})
    );
    assert_eq!(u[3]["waits_on"], "a unit; a limit");
    assert_eq!(u[3]["depends_on"], json!(["u-run"]));
    assert!(
        u[3].get("stop_cause").is_none()
            && u[2].get("waits_on").is_none()
            && u[0].get("waits_on").is_none()
    );
    assert_eq!(
        (u[4]["intent"].clone(), u[4]["waits_on"].clone()),
        (Value::Null, Value::Null)
    );
}

// spec: specs/ap/status.md#R8
#[test]
fn r8_running_stages_are_listed_oldest_first() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    assert_eq!(
        v["runs"],
        json!([
        {"run": "act-run-1", "unit": null, "intent": "alpha-report", "role": "lead", "runtime": "claude", "model": "invented-model", "started_at": "2026-11-17T10:00:00Z"},
        {"run": "act-run-2", "unit": "u-run", "intent": null, "role": "implementer", "runtime": "claude", "model": "invented-model", "started_at": "2026-11-17T11:00:00Z"}])
    );
}

// spec: specs/ap/status.md#R9
#[test]
fn r9_open_and_planned_findings_come_by_severity_then_newest() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    assert_eq!(
        strs(&column(&v["findings"], "finding")),
        [
            "f-urgent-planned",
            "f-urgent",
            "f-act-new",
            "f-act-planned",
            "f-act-old",
            "f-info"
        ]
    );
    assert_eq!(
        v["findings"][1],
        json!({"finding": "f-urgent", "severity": "urgent", "kind": "drift", "state": "open",
        "summary": "Summary of f-urgent.", "last_seen_at": "2026-11-17T09:00:00Z"})
    );
}

// spec: specs/ap/status.md#R10
#[test]
fn r10_active_holds_are_listed_with_what_they_name() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    assert_eq!(
        strs(&column(&v["holds"], "hold")),
        ["h-other", "h-intent", "h-user", "h-unit"]
    );
    assert_eq!(
        v["holds"][3],
        json!({"hold": "h-unit", "effect": "effect of h-unit", "until": "a condition", "unit": "u-run", "intent": null, "scope": "one unit"})
    );
    assert_eq!(
        v["holds"][2],
        json!({"hold": "h-user", "effect": "effect of h-user", "until": "a condition", "unit": null, "intent": null, "scope": null})
    );
}

// spec: specs/ap/status.md#R11
#[test]
fn r11_delivery_is_the_last_record_where_the_request_of_delivery_names_it() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    assert_eq!(
        v["delivery"],
        json!({"at": "2026-11-17T11:30:00Z", "end": "current", "tip": "0123abc", "target": "/usr/local/bin/ap"})
    );
    assert!(v.get("delivery_unreadable").is_none());
    // Another state directory: its last.json is read, a trailing slash is not doubled.
    f.files
        .insert(CONFIG.into(), json!({"state": "/srv/other/"}).to_string());
    f.files.insert(
        "/srv/other/last.json".into(),
        json!({"at": "T", "end": "failed", "tip": "t", "target": "x"}).to_string(),
    );
    assert_eq!(run(&mut f, &Call::Overview)["delivery"]["end"], "failed");
    // No request of delivery, or one without a state: the default directory.
    for config in [
        None,
        Some("{}"),
        Some("not json"),
        Some("{\"state\": \"relative\"}"),
    ] {
        let mut f = fixture();
        f.files.remove(CONFIG);
        config.map(|c| f.files.insert(CONFIG.into(), c.into()));
        assert_eq!(
            run(&mut f, &Call::Overview)["delivery"]["end"],
            "current",
            "{config:?}"
        );
    }
    // A record that cannot be read: null, and said so.
    for last in [None, Some("garbage"), Some("[1]")] {
        let mut f = fixture();
        f.files.remove(LAST);
        last.map(|l| f.files.insert(LAST.into(), l.into()));
        let v = run(&mut f, &Call::Overview);
        assert_eq!(
            (v["delivery"].clone(), v["delivery_unreadable"].clone()),
            (Value::Null, json!(true)),
            "{last:?}"
        );
    }
}

// spec: specs/ap/status.md#R12
#[test]
fn r12_what_the_lead_acts_on_first_comes_in_a_fixed_order() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    let need = |kind: &str, key: &str, why: &str| json!({"kind": kind, "key": key, "why": why});
    assert_eq!(
        v["needs_lead"],
        json!([
            need("finding", "f-urgent", "Summary of f-urgent."),
            need("unit", "u-stop", "ci_failed"),
            need("unit", "u-acc", "waits for the Lead's reading or merge"),
            need("finding", "f-act-new", "Summary of f-act-new."),
            need("finding", "f-act-old", "Summary of f-act-old."),
            need("request", "q-a", "unrouted since 2026-11-15T10:30:00Z")
        ])
    );
    // Exactly 86,400 seconds is not more than a day.
    request(
        &mut f,
        "q-edge",
        "2026-11-16T12:00:00Z",
        "unrouted",
        "current",
    );
    request(
        &mut f,
        "q-over",
        "2026-11-16T11:59:59Z",
        "unrouted",
        "current",
    );
    let v = run(&mut f, &Call::Overview);
    let keys = column(&v["needs_lead"], "key");
    assert!(
        keys.contains(&json!("q-over")) && !keys.contains(&json!("q-edge")),
        "{keys:?}"
    );
}

// spec: specs/ap/status.md#R13
#[test]
fn r13_a_key_is_an_intent_then_a_unit_else_unknown() {
    let mut f = fixture();
    assert_eq!(run(&mut f, &key("alpha-report"))["intent"], "alpha-report");
    assert_eq!(run(&mut f, &key("alpha-report")).get("also_unit"), None);
    assert_eq!(run(&mut f, &key("u-run"))["unit"], "u-run");
    let e = refusal(&mut f, &key("nothing"));
    assert_eq!((e.code, e.field.as_str()), ("unknown_key", "key"));
    // A key that names both prints the intent and says so.
    unit(&mut f, "alpha-report", "ready", json!({}));
    let v = run(&mut f, &key("alpha-report"));
    assert_eq!(
        (v["intent"].as_str(), v["also_unit"].as_bool()),
        (Some("alpha-report"), Some(true))
    );
    assert!(v.get("unit").is_none());
}

// spec: specs/ap/status.md#R14
#[test]
fn r14_an_intent_is_printed_in_full_with_its_units_findings_reviews_and_holds() {
    let mut f = fixture();
    f.put("request", "q-routed", json!({"topic": "walk/one", "kind": "order", "said_at": "2026-11-17T08:30:00Z", "status": "current",
        "route": "intent", "intent": oid("intent", "alpha-report"), "words_verified": true}));
    for (k, i, started) in [
        ("rv-old", "alpha-report", "2026-11-17T09:10:00Z"),
        ("rv-new", "alpha-report", "2026-11-17T09:20:00Z"),
        ("rv-other", "beta-index", "2026-11-17T09:30:00Z"),
    ] {
        f.put("review", k, json!({"kind": "plan", "intent": oid("intent", i), "head": "h", "round": 1, "status": "closed", "verdict": "accept",
            "started_at": started, "closed_at": "2026-11-17T09:40:00Z", "task": oid("task", "t")}));
    }
    f.put("review", "rv-page", json!({"kind": "page", "unit": oid("unit", "u-run"), "head": "h", "status": "closed", "started_at": "2026-11-17T09:00:00Z"}));
    let v = run(&mut f, &key("alpha-report"));
    // What `ap intent show` prints, intent properties and all.
    for m in [
        "intent",
        "outcome",
        "status",
        "criteria",
        "requests",
        "confirmed",
        "first_user",
        "amended_at",
    ] {
        assert!(v.get(m).is_some(), "{m}");
    }
    assert_eq!(strs(&column(&v["requests"], "request")), ["q-routed"]);
    assert_eq!(
        strs(&column(&v["criteria"], "criterion")),
        ["c1", "c2", "c3"]
    );
    // Units of every state, in the order of the page; the end of a merged one.
    assert_eq!(
        strs(&column(&v["units"], "unit")),
        ["u-run", "u-ready", "u-draft", "u-merged"]
    );
    assert_eq!(v["units"][3]["ended_at"], "2026-11-16T10:00:00Z");
    assert_eq!(v["units"][3]["merged_commit"], "def456");
    assert_eq!(v["units"][0]["merged_commit"], Value::Null);
    // Findings planned into it or its units, and every open one.
    assert_eq!(
        strs(&column(&v["findings"], "finding")),
        [
            "f-urgent-planned",
            "f-urgent",
            "f-act-new",
            "f-act-old",
            "f-info"
        ]
    );
    assert_eq!(
        strs(&column(&v["plan_reviews"], "review")),
        ["rv-new", "rv-old"]
    );
    assert_eq!(
        v["plan_reviews"][0],
        json!({"review": "rv-new", "head": "h", "round": 1, "status": "closed", "verdict": "accept",
        "started_at": "2026-11-17T09:20:00Z", "closed_at": "2026-11-17T09:40:00Z"})
    );
    // Every active hold; those that name the intent or one of its units first, marked.
    assert_eq!(
        strs(&column(&v["holds"], "hold")),
        ["h-intent", "h-unit", "h-other", "h-user"]
    );
    assert_eq!(
        column(&v["holds"], "here"),
        [json!(true), json!(true), json!(false), json!(false)]
    );
    // The reads of the page's review kind are by the required property `kind`.
    let search = f
        .calls
        .iter()
        .find(|c| c.1["object_type"] == "review")
        .unwrap();
    assert_eq!(
        search.1["where"],
        json!({"all": [{"eq": ["kind", "plan"]}]})
    );
    // Beta has a unit planned-into finding by intent.
    let beta = run(&mut f, &key("beta-index"));
    assert!(strs(&column(&beta["findings"], "finding")).contains(&"f-act-planned"));
    assert_eq!(strs(&column(&beta["units"], "unit")), ["u-stop", "u-acc"]);
}

// spec: specs/ap/status.md#R15
#[test]
fn r15_a_unit_is_printed_with_its_page_reviews_runs_holds_and_blockers() {
    let mut f = fixture();
    f.put("unit", "u-dep", json!({"lane": "objective", "class": "plain", "state": "ready", "objective": "A page | with \\ pipes\nand two lines.",
        "interface": "line one\nline two", "depends_on": [oid("unit", "u-run"), oid("unit", "u-merged"), oid("unit", "u-gone")],
        "intent": oid("intent", "alpha-report")}));
    f.put("review", "act-rv|1\\x", json!({"kind": "packet", "unit": oid("unit", "u-dep"), "head": "h1", "round": 2, "status": "closed",
        "verdict": "repair", "started_at": "2026-11-17T09:00:00Z", "closed_at": "2026-11-17T09:10:00Z", "report": "/a/report.md"}));
    f.put("review", "rv-2", json!({"kind": "page", "unit": oid("unit", "u-dep"), "head": "h0", "status": "running", "started_at": "2026-11-17T08:00:00Z"}));
    run_object(
        &mut f,
        "act-run-9",
        "closed",
        "2026-11-17T10:00:00Z",
        json!({"unit": oid("unit", "u-dep"), "state": "ok", "ended_at": "2026-11-17T10:30:00Z",
        "input_tokens": 5, "cache_read_tokens": 6, "cache_write_tokens": 7, "output_tokens": 8}),
    );
    hold(
        &mut f,
        "h-dep",
        "active",
        "2026-11-17T03:00:00Z",
        json!({"unit": oid("unit", "u-dep")}),
    );
    let v = run(&mut f, &key("u-dep"));
    // Every property of the page whole, unchanged.
    assert_eq!(v["objective"], "A page | with \\ pipes\nand two lines.");
    assert_eq!(v["interface"], "line one\nline two");
    assert_eq!(v["unit"], "u-dep");
    // Reviews in the order of the view (oldest first), the key unescaped, each read with `get`.
    assert_eq!(
        strs(&column(&v["reviews"], "review")),
        ["rv-2", "act-rv|1\\x"]
    );
    assert_eq!(
        v["reviews"][1],
        json!({"review": "act-rv|1\\x", "kind": "packet", "head": "h1", "round": 2, "status": "closed",
        "verdict": "repair", "closed_at": "2026-11-17T09:10:00Z", "report": "/a/report.md"})
    );
    assert_eq!(v["reviews"][0]["report"], Value::Null);
    assert_eq!(
        v["runs"],
        json!([{"run": "act-run-9", "role": "implementer", "model": "invented-model", "status": "closed", "state": "ok",
        "started_at": "2026-11-17T10:00:00Z", "ended_at": "2026-11-17T10:30:00Z", "tokens": {"input": 5, "cache_read": 6, "cache_write": 7, "output": 8}}])
    );
    assert_eq!(strs(&column(&v["holds"], "hold")), ["h-dep"]);
    // Dependencies that are not merged or done: their states; a missing unit has none.
    assert_eq!(
        v["blocked_by"],
        json!([{"unit": "u-run", "state": "running"}, {"unit": "u-gone", "state": null}])
    );
}

// spec: specs/ap/status.md#R16
#[test]
fn r16_reviews_and_runs_come_through_views_then_get_and_a_cut_view_is_unqualified() {
    let mut f = fixture();
    f.put("review", "rv-1", json!({"kind": "page", "unit": oid("unit", "u-run"), "head": "h", "status": "closed", "started_at": "2026-11-17T09:00:00Z"}));
    run(&mut f, &key("u-run"));
    let seen: Vec<(String, String)> = f
        .calls
        .iter()
        .map(|c| {
            (
                c.0[0].clone(),
                c.1["view"]
                    .as_str()
                    .or(c.1["id"]["object_type"].as_str())
                    .unwrap_or("")
                    .to_owned(),
            )
        })
        .collect();
    assert!(
        seen.contains(&("render".into(), "reviews_of_unit".into()))
            && seen.contains(&("render".into(), "runs_of_unit".into()))
    );
    assert!(
        seen.contains(&("get".into(), "review".into()))
            && seen.contains(&("get".into(), "run".into()))
    );
    assert!(
        !f.calls.iter().any(|c| c.0[0] == "search"
            && ["review", "run"].contains(&c.1["object_type"].as_str().unwrap()))
    );
    f.truncated = true;
    let e = refusal(&mut f, &key("u-run"));
    assert_eq!(
        (e.code, e.field.as_str()),
        ("store_answer_unqualified", "reviews_of_unit")
    );
    // A key the view lists and the store does not have is unreadable, not skipped.
    let mut f = fixture();
    f.ghost = Some("rv-ghost".into());
    let e = refusal(&mut f, &key("u-run"));
    assert_eq!(
        (e.code, e.field.as_str()),
        ("store_answer_unreadable", "review")
    );
    // A view answer whose rows are not the count it states is unreadable too.
    let mut f = fixture();
    f.skew = true;
    assert_eq!(
        refusal(&mut f, &key("u-run")).code,
        "store_answer_unreadable"
    );
}

// spec: specs/ap/status.md#R17
#[test]
fn r17_the_overview_text_is_one_golden_text() {
    let mut f = fixture();
    let v = run(&mut f, &Call::Overview);
    let text = text_of(&Call::Overview, &v);
    assert_eq!(text, GOLDEN, "{text}");
    assert!(text.is_ascii());
}
const GOLDEN: &str = "\
status 2026-11-17T12:00:00Z schema work@11
NEEDS THE LEAD
finding f-urgent: Summary of f-urgent.
unit u-stop: ci_failed
unit u-acc: waits for the Lead's reading or merge
finding f-act-new: Summary of f-act-new.
finding f-act-old: Summary of f-act-old.
request q-a: unrouted since 2026-11-15T10:30:00Z
INTENTS
alpha-report assumed (assumed) criteria 2/0/1 units draft=1 merged=1 ready=1 running=1
beta-index confirmed (by delegation) criteria 0/1/0 units accepted=1 stopped=1
delta-draft draft criteria 0/0/0 units none
UNITS
u-stop stopped plain - stopped: ci_failed
u-acc accepted plain o/r#9
u-run running plain o/r#7
u-ready ready contract - waits on a unit; a limit
u-small ready plain -
RUNNING
lead invented-model alpha-report since 2026-11-17T10:00:00Z (act-run-1)
implementer invented-model u-run since 2026-11-17T11:00:00Z (act-run-2)
FINDINGS
urgent drift planned f-urgent-planned: Summary of f-urgent-planned.
urgent drift open f-urgent: Summary of f-urgent.
act drift open f-act-new: Summary of f-act-new.
act drift planned f-act-planned: Summary of f-act-planned.
act drift open f-act-old: Summary of f-act-old.
info drift open f-info: Summary of f-info.
HOLDS
effect of h-other until a condition intent beta-index
effect of h-intent until a condition intent alpha-report
effect of h-user until a condition
effect of h-unit until a condition unit u-run
REQUESTS NOT ROUTED
3 unrouted, the oldest first
2026-11-15T10:30:00Z order walk/one q-a
2026-11-17T10:30:00Z order walk/one q-b
2026-11-17T11:00:00Z order walk/one q-recent
DELIVERY
2026-11-17T11:30:00Z current tip 0123abc target /usr/local/bin/ap
";

// spec: specs/ap/status.md#R17
#[test]
fn r17_the_text_stays_within_80_lines_and_says_what_it_cut() {
    let mut f = fixture();
    for i in 0..120 {
        unit(
            &mut f,
            &format!("p{i:03}"),
            ["ready", "running", "stopped", "accepted"][i % 4],
            json!({"intent": oid("intent", "alpha-report")}),
        );
        request(
            &mut f,
            &format!("q-bulk-{i:03}"),
            "2026-11-10T09:00:00Z",
            "unrouted",
            "current",
        );
        finding(
            &mut f,
            &format!("f-bulk-{i:03}"),
            ["urgent", "act", "info"][i % 3],
            "open",
            "2026-11-17T09:00:00Z",
            json!({}),
        );
        hold(
            &mut f,
            &format!("h-bulk-{i:03}"),
            "active",
            "2026-11-17T03:00:00Z",
            json!({}),
        );
        run_object(
            &mut f,
            &format!("act-bulk-{i:03}"),
            "running",
            "2026-11-17T09:00:00Z",
            json!({}),
        );
        intent(&mut f, &format!("bulk-intent-{i:03}"), "draft", json!({}));
    }
    let v = run(&mut f, &Call::Overview);
    let text = text_of(&Call::Overview, &v);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() <= 80, "{} lines", lines.len());
    let after = |h: &str| lines.iter().position(|l| *l == h).unwrap();
    // Each section: at most nine entries, then what it cut.
    let more = |from: &str, to: &str, total: usize| {
        let body = &lines[after(from) + 1..after(to)];
        assert_eq!(body.len(), 10, "{from}: {body:?}");
        assert_eq!(body[9], format!("(+{} more)", total - 9));
    };
    more(
        "NEEDS THE LEAD",
        "INTENTS",
        v["needs_lead"].as_array().unwrap().len(),
    );
    more("INTENTS", "UNITS", 123);
    more("UNITS", "RUNNING", 5 + 120);
    more("RUNNING", "FINDINGS", 122);
    more("FINDINGS", "HOLDS", 6 + 120);
    more("HOLDS", "REQUESTS NOT ROUTED", 4 + 120);
    let requests = &lines[after("REQUESTS NOT ROUTED") + 1..after("DELIVERY")];
    assert_eq!(requests.len(), 7, "{requests:?}");
    assert_eq!(requests[6], "(+118 more)");
    // A store without anything prints its headings and `none`.
    let mut empty = F::empty();
    let v = run(&mut empty, &Call::Overview);
    let text = text_of(&Call::Overview, &v);
    let want = [
        "NEEDS THE LEAD",
        "INTENTS",
        "UNITS",
        "RUNNING",
        "FINDINGS",
        "HOLDS",
        "REQUESTS NOT ROUTED",
    ];
    for h in want {
        let i = text.lines().position(|l| l == h).unwrap();
        assert_eq!(text.lines().nth(i + 1), Some("none"), "{h}");
    }
    assert!(text.ends_with("DELIVERY\nunreadable\n"), "{text}");
    assert_eq!(
        text.lines().next(),
        Some("status 2026-11-17T12:00:00Z schema work@11")
    );
}

// spec: specs/ap/status.md#R17
#[test]
fn r17_a_stored_value_with_a_line_break_or_control_character_is_one_line() {
    let mut f = fixture();
    finding(
        &mut f,
        "f-lines",
        "urgent",
        "open",
        "2026-11-17T11:59:00Z",
        json!({"summary": "two\nlines\u{1b}[31m red"}),
    );
    let v = run(&mut f, &Call::Overview);
    let text = text_of(&Call::Overview, &v);
    assert!(
        text.contains("urgent drift open f-lines: two lines [31m red\n"),
        "{text}"
    );
    assert!(!text.contains('\u{1b}'));
}

// spec: specs/ap/status.md#R18
#[test]
fn r18_the_text_of_a_key_is_every_member_with_its_texts_whole() {
    let mut f = fixture();
    f.put("unit", "u-page", json!({"lane": "objective", "class": "plain", "state": "ready", "objective": "A page | with \\ pipes\n\nand blank line.",
        "interface": "one line", "intent": oid("intent", "alpha-report"), "write_set": ["a/**", "b"], "note": "x\u{7}y"}));
    let v = run(&mut f, &key("u-page"));
    let text = text_of(&key("u-page"), &v);
    assert!(
        text.contains("objective:\n  A page | with \\ pipes\n  \n  and blank line.\n"),
        "{text}"
    );
    assert!(text.contains("interface: one line\n"));
    assert!(text.contains("intent: intent alpha-report\n"));
    assert!(text.contains("write_set:\n  - a/**\n  - b\n"), "{text}");
    assert!(text.contains("note: x?y\n"));
    assert!(text.contains("blocked_by: none\n") && text.contains("reviews: none\n"));
    // No bound: a text of 100,000 bytes is printed whole.
    let big = "word ".repeat(20_000);
    f.put(
        "unit",
        "u-big",
        json!({"lane": "objective", "class": "plain", "state": "ready", "objective": big}),
    );
    let v = run(&mut f, &key("u-big"));
    assert!(text_of(&key("u-big"), &v).contains(&format!("objective: {big}\n")));
    let v = run(&mut f, &key("alpha-report"));
    let text = text_of(&key("alpha-report"), &v);
    assert!(text.contains("c1 holds.") && text.contains("u-merged") && text.contains("h-intent"));
}

// spec: specs/ap/status.md#R19
#[test]
fn r19_a_word_matches_names_exact_first_and_at_most_twelve() {
    let mut f = fixture();
    let v = run(&mut f, &term("unit"));
    assert_eq!(v["word"], "unit");
    let m = v["matches"].as_array().unwrap();
    assert_eq!(m.len(), 12);
    assert_eq!(
        (
            m[0]["kind"].as_str(),
            m[0]["name"].as_str(),
            m[0]["of"].clone()
        ),
        (Some("type"), Some("unit"), Value::Null)
    );
    assert!(
        m[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("One piece of work")
    );
    let total = v["more"].as_u64().unwrap() as usize;
    assert!(total > 12);
    // Exact names: kinds in order, then `of`, then name; then the names that contain the word.
    let exact: Vec<(String, String)> = m
        .iter()
        .take_while(|x| x["name"] == "unit")
        .map(|x| {
            (
                x["kind"].as_str().unwrap().to_owned(),
                x["of"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect();
    assert!(
        exact.len() >= 3
            && exact
                .windows(2)
                .all(|w| w[0] <= w[1] || w[0].0 != w[1].0 || w[0].1 <= w[1].1),
        "{exact:?}"
    );
    // The names that contain the word follow the exact ones, by kind, `of` and name.
    let v2 = run(&mut f, &term("route_request"));
    let binding = column(&v2["matches"], "name");
    let names = strs(&binding);
    assert_eq!(
        names[..3],
        [
            "route_request",
            "route_request_to_finding",
            "route_request_to_unit"
        ]
    );
    assert!(
        v2["matches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["name"].as_str().unwrap().contains("route_request"))
    );
    // Upper case is the same word; a type, an action and a criterion's code.
    assert_eq!(run(&mut f, &term("UNIT"))["matches"], v["matches"]);
    let v = run(&mut f, &term("accept_unit"));
    assert_eq!(
        (
            v["matches"][0]["kind"].as_str(),
            v["matches"][0]["name"].as_str()
        ),
        (Some("action"), Some("accept_unit"))
    );
    let v = run(&mut f, &term("head_not_reviewed"));
    let m = v["matches"].as_array().unwrap();
    assert!(
        m.len() >= 3
            && m.iter()
                .all(|x| x["kind"] == "criterion" && x["name"] == "head_not_reviewed")
    );
    assert_eq!(strs(&column(&v["matches"], "of"))[0], "accept_unit");
    assert!(
        m[0]["text"]
            .as_str()
            .unwrap()
            .contains("must be closed accept")
    );
    // A link and a view.
    let v = run(&mut f, &term("unit_intent"));
    assert_eq!(
        (
            v["matches"][0]["kind"].as_str(),
            v["matches"][0]["text"].as_str()
        ),
        (Some("link"), Some("unit.intent -> intent"))
    );
    let v = run(&mut f, &term("reviews_of_unit"));
    assert_eq!(
        (
            v["matches"][0]["kind"].as_str(),
            v["matches"][0]["text"].as_str()
        ),
        (Some("view"), Some("review (unit)"))
    );
    // A word that matches nothing is no error.
    let v = run(&mut f, &term("nothing-like-it"));
    assert_eq!(
        (v["matches"].clone(), v["more"].clone()),
        (json!([]), json!(0))
    );
}

// spec: specs/ap/status.md#R19
#[test]
fn r19_the_name_of_a_value_type_matches_the_properties_and_parameters_it_types() {
    let mut f = fixture();
    let v = run(&mut f, &term("finding_severity"));
    let at: Vec<(&str, &str)> = v["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["kind"].as_str().unwrap(), m["of"].as_str().unwrap()))
        .collect();
    assert_eq!(at[0], ("property", "finding"));
    assert!(at.contains(&("parameter", "open_finding")));
    assert!(
        v["matches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| ["property", "parameter"].contains(&m["kind"].as_str().unwrap()))
    );
    let severity = &f.schema["object_types"]["finding"]["properties"]["severity"]["description"];
    assert_eq!(&v["matches"][0]["text"], severity);
}

// spec: specs/ap/status.md#R20
#[test]
fn r20_the_members_of_an_enumerated_property_are_read_by_aggregates_that_scan_nothing() {
    let mut f = fixture();
    let v = run(&mut f, &term("urgent"));
    let severity =
        f.schema["object_types"]["finding"]["properties"]["severity"]["description"].clone();
    assert_eq!(
        v["matches"][0],
        json!({"kind": "value", "name": "urgent", "of": "finding.severity", "text": severity})
    );
    let aggregates: Vec<&Value> = f
        .calls
        .iter()
        .filter(|c| c.0[0] == "aggregate")
        .map(|c| &c.1)
        .collect();
    assert!(aggregates.len() > 20);
    for b in &aggregates {
        let p = b["group_by"][0].as_str().unwrap();
        assert_eq!(b["where"], json!({"in": [p, []]}));
    }
    // At most one answered per distinct value type; a second property of it only after a refusal of
    // the first one's type (no derive).
    let value_type = |b: &Value| {
        let (t, p) = (
            b["object_type"].as_str().unwrap(),
            b["group_by"][0].as_str().unwrap(),
        );
        f.schema["object_types"][t]["properties"][p]["type"]["value_type"].clone()
    };
    let derives = |t: &Value| {
        let grants = f.schema["grants"][0]["derive"].as_array().unwrap();
        grants.iter().any(|d| d == t)
    };
    let mut answered = std::collections::BTreeSet::new();
    for b in &aggregates {
        let answers = derives(&b["object_type"]);
        assert!(
            !answers || answered.insert(value_type(b).to_string()),
            "{b}"
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    assert!(
        aggregates
            .iter()
            .all(|b| seen.insert((b["object_type"].to_string(), b["group_by"][0].to_string())))
    );
    let denied: Vec<&str> = f.schema["object_types"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .filter(|t| {
            !f.schema["grants"][0]["derive"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d == t)
        })
        .collect();
    for t in denied {
        assert!(
            aggregates.iter().filter(|b| b["object_type"] == t).count() <= 1,
            "{t}"
        );
    }
    // A member of a value type that is not an enumeration is not found; a refusal is no error.
    assert!(run(&mut f, &term("on")).get("matches").is_some());
    let mut f = fixture();
    f.refuse = Some(("aggregate", json!({"code": "unsupported_group_property"})));
    let v = run(&mut f, &term("urgent"));
    assert!(
        v["matches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["kind"] != "value")
    );
    assert_eq!(v["matches"], json!([]));
    let v = run(&mut f, &term("severity"));
    assert!(
        v["matches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["kind"] != "value")
    );
}

// spec: specs/ap/status.md#R20
#[test]
fn r20_a_term_makes_one_aggregate_per_value_type_and_answers_the_same_matches() {
    let mut f = fixture();
    // The matches the per-property reading gave: every member of every property typed by an
    // enumeration, of a type whose grant gives a derive.
    let schema = f.schema.clone();
    let mut today = vec![];
    for (t, o) in schema["object_types"].as_object().unwrap() {
        if !schema["grants"][0]["derive"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d == t)
        {
            continue;
        }
        for (p, v) in o["properties"].as_object().unwrap() {
            let Some(vt) = v["type"]["value_type"].as_str() else {
                continue;
            };
            for m in schema["value_types"][vt]["enum"]
                .as_array()
                .into_iter()
                .flatten()
            {
                today.push((m.as_str().unwrap().to_owned(), format!("{t}.{p}")));
            }
        }
    }
    for word in ["urgent", "waived", "planned", "stopped"] {
        let v = run(&mut f, &term(word));
        assert_eq!(v["more"], 0, "{word}: every match is shown");
        let found: std::collections::BTreeSet<(String, String)> = v["matches"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["kind"] == "value")
            .map(|m| {
                (
                    m["name"].as_str().unwrap().to_owned(),
                    m["of"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let expected: std::collections::BTreeSet<_> = today
            .iter()
            .filter(|(n, _)| n.contains(word))
            .cloned()
            .collect();
        assert!(!expected.is_empty(), "{word}");
        assert_eq!(found, expected, "{word}");
        // Calls: one `aggregate` per value type that has a property of a type with a derive, and one
        // more for each type without it that was asked before.
        let aggregates: Vec<&Value> = f
            .calls
            .iter()
            .filter(|c| c.0[0] == "aggregate")
            .map(|c| &c.1)
            .collect();
        let mut types = std::collections::BTreeSet::new();
        let mut typed = 0;
        for (t, o) in schema["object_types"].as_object().unwrap() {
            for (_, v) in o["properties"].as_object().unwrap() {
                if let Some(vt) = v["type"]["value_type"].as_str() {
                    typed += 1;
                    types.insert((vt.to_owned(), t.clone()));
                }
            }
        }
        let distinct: std::collections::BTreeSet<_> = types.iter().map(|(v, _)| v).collect();
        let denied: std::collections::BTreeSet<_> = schema["object_types"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|t| {
                !schema["grants"][0]["derive"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d == *t)
            })
            .collect();
        assert!(
            aggregates.len() <= distinct.len() + denied.len(),
            "{} {}",
            aggregates.len(),
            distinct.len()
        );
        assert!(
            aggregates.len() < typed,
            "{} of {typed} properties",
            aggregates.len()
        );
    }
}
// spec: specs/ap/status.md#R21
#[test]
fn r21_a_match_without_a_description_prints_no_description_and_its_json_text_stays_empty() {
    let mut f = fixture();
    f.schema["object_types"]["finding"]["properties"]["severity"]["description"] = json!("");
    let call = term("severity");
    let v = run(&mut f, &call);
    let m = &v["matches"][0];
    assert_eq!(
        (m["kind"].as_str(), m["of"].as_str(), m["text"].as_str()),
        (Some("property"), Some("finding"), Some(""))
    );
    let lines = text_of(&call, &v);
    assert!(
        lines.starts_with("property finding.severity: (no description)\n"),
        "{lines}"
    );
    // Its values have the property's description.
    let call = term("urgent");
    let v = run(&mut f, &call);
    assert_eq!(v["matches"][0]["text"], "");
    assert!(text_of(&call, &v).starts_with("value finding.severity.urgent: (no description)\n"));
    // A control character in a text is a space.
    f.schema["object_types"]["finding"]["properties"]["severity"]["description"] =
        json!("a\nb\u{1}c");
    let call = term("urgent");
    let v = run(&mut f, &call);
    assert!(text_of(&call, &v).starts_with("value finding.severity.urgent: a b c\n"));
}
// spec: specs/ap/status.md#R21
#[test]
fn r21_the_text_of_a_word_is_one_line_per_match_and_never_the_whole_description() {
    let mut f = fixture();
    let v = run(&mut f, &term("unit"));
    let text = text_of(&term("unit"), &v);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 13);
    assert!(
        lines[0].starts_with("type unit: One piece of work"),
        "{text}"
    );
    assert!(lines[1].starts_with("property ") && lines[1].contains(".unit: "));
    assert_eq!(lines[12], format!("(+{} more)", v["more"]));
    let described = f.calls.iter().filter(|c| c.0[0] == "describe").count();
    assert_eq!(described, 1);
    assert!(text.len() < 5_000 && v.to_string().len() < 5_000);
    let v = run(&mut f, &term("urgent"));
    assert!(
        text_of(&term("urgent"), &v)
            .starts_with("value finding.severity.urgent: info, act (plan it) or urgent")
    );
    let v = run(&mut f, &term("zzzz"));
    assert_eq!(text_of(&term("zzzz"), &v), "");
    let v = run(&mut f, &term("unit_state"));
    assert!(!text_of(&term("unit_state"), &v).contains("(+"));
    let v = run(&mut f, &term("reviews_of_unit"));
    assert_eq!(
        text_of(&term("reviews_of_unit"), &v),
        "view reviews_of_unit: review (unit)\n"
    );
    let v = run(&mut f, &term("units_live"));
    assert_eq!(
        text_of(&term("units_live"), &v),
        "view units_live: unit (no arguments)\n"
    );
}

// spec: specs/ap/status.md#R22
#[test]
fn r22_every_call_to_the_store_goes_through_the_module_store() {
    let source = include_str!("../src/status.rs");
    for direct in [".call(", "\"ont\"", "Command", "std::fs"] {
        assert!(
            !source.contains(direct),
            "status.rs reaches outside store.rs: {direct}"
        );
    }
}

// spec: specs/ap/status.md#R22
#[test]
fn r22_a_refusal_of_the_store_keeps_its_code_and_names_the_step() {
    let mut f = fixture();
    f.refuse = Some(("search", json!({"code": "store_unavailable"})));
    let e = refusal(&mut f, &Call::Overview);
    assert_eq!(
        (e.code, e.field.as_str(), e.detail["code"].clone()),
        ("store_refused", "requests", json!("store_unavailable"))
    );
    assert_eq!(ap::store::code(&e), "store_unavailable");
    let mut f = fixture();
    f.refuse = Some(("describe", json!({"code": "no_schema"})));
    let e = refusal(&mut f, &term("unit"));
    assert_eq!(
        (ap::store::code(&e).as_str(), e.field.as_str()),
        ("no_schema", "describe")
    );
    // An answer that is not the engine's is unreadable, never printed.
    let mut f = fixture();
    f.refuse = None;
    f.objects.remove(&("unit".to_owned(), "u-run".to_owned()));
    f.put("unit", "u-run", json!({"state": "running"}));
    assert!(execute(&mut f, None, &key("u-run")).is_ok());
}

// ---- `ap status --wait` (R23 to R33) ----
// The wait has a fake of its own around `F`: the store's objects follow a script (one step at each
// `pause`, which does not sleep), `systemctl list-units`, `units()`, the policy file and the units
// directory (under a temporary directory) and the clock are the fake's. `F` itself still refuses every
// program but `ont`.
use std::{
    cell::Cell,
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const PREFIX: &str = "aptrial1";
const SCRATCH: &str = "/invented/scratch/store.sqlite";

/// What the next pause changes.
type Step = Box<dyn FnOnce(&mut Wt)>;

struct Wt {
    f: F,
    dir: PathBuf,
    policy: Option<PathBuf>,
    units: bool,
    env: Vec<(String, String)>,
    /// The lines `systemctl list-units` prints; None is a failed call.
    listed: Option<String>,
    /// The store fails every call.
    down: bool,
    script: VecDeque<Step>,
    pauses: usize,
    /// The number of calls to the store before each reading: the reading's `at` is `NOW` plus it.
    starts: Vec<usize>,
    systemctl: usize,
    reads: std::cell::RefCell<Vec<String>>,
    base: Instant,
    clock: Cell<Duration>,
}
impl Wt {
    /// The intent `walk` and a policy file whose units directory is under the temporary directory.
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ap-status-wait-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let mut f = F::empty();
        intent(&mut f, "walk", "confirmed", json!({}));
        let mut w = Wt {
            f,
            policy: Some(dir.join("policy.json")),
            dir,
            units: true,
            env: vec![],
            listed: Some(String::new()),
            down: false,
            script: VecDeque::new(),
            pauses: 0,
            starts: vec![0],
            systemctl: 0,
            reads: Default::default(),
            base: Instant::now(),
            clock: Cell::default(),
        };
        w.policy_is(json!({"work": {"prefix": PREFIX, "slice": "aptrial1.slice"}}));
        w
    }
    fn units_dir(&self) -> PathBuf {
        self.dir.join("units")
    }
    /// The policy file, with the units directory added to what is given.
    fn policy_is(&mut self, mut p: Value) {
        if p.get("units").is_none() && p.is_object() {
            p["units"] = json!({"directory": self.units_dir().to_str().unwrap()});
        }
        std::fs::write(self.dir.join("policy.json"), p.to_string()).unwrap();
    }
    /// The file `started` of a unit.
    fn started(&self, key: &str, revision: &str) {
        std::fs::create_dir_all(self.units_dir().join(key)).unwrap();
        std::fs::write(self.units_dir().join(key).join("started"), revision).unwrap();
    }
    /// What the next pause changes.
    fn then(&mut self, step: impl FnOnce(&mut Wt) + 'static) {
        self.script.push_back(Box::new(step));
    }
    /// A run unit of the service manager that is live.
    fn live(&mut self, key: &str) {
        let line = format!("{PREFIX}-{key}.service loaded active running a run\n");
        self.listed.get_or_insert_with(String::new).push_str(&line);
    }
    /// The scratch store with its policy of the environment.
    fn scratch(&mut self) {
        let path = self.dir.join("policy.json");
        self.env = vec![("AP_POLICY_FILE".into(), path.to_str().unwrap().into())];
    }
    fn wait(&mut self, store: Option<&str>, timeout: u64) -> Result<Value, Failure> {
        let call = Call::Wait("walk".into(), timeout);
        execute(self, store, &call)
    }
    /// The answer of a wait that must end in one.
    fn answer(&mut self, timeout: u64) -> Value {
        match self.wait(None, timeout) {
            Ok(v) => v,
            Err(e) => panic!("{} {} {}", e.code, e.field, e.detail),
        }
    }
    /// The `at` of reading `n` (counted from 0).
    fn at(&self, n: usize) -> String {
        let s = self.starts[n] as u64;
        format!("2026-11-17T12:{:02}:{:02}Z", s / 60, s % 60)
    }
}
impl Drop for Wt {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl System for Wt {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        let broken = |p: &str| {
            Err(Failure {
                code: "process_failed",
                field: p.into(),
                detail: Value::Null,
            })
        };
        match program {
            "ont" if self.down => broken("ont"),
            "ont" => self.f.call(program, args, input, ms),
            "systemctl" => {
                let pattern = format!("{PREFIX}-*");
                assert_eq!(
                    args,
                    ["list-units", "--all", "--plain", "--no-legend", &pattern]
                );
                self.systemctl += 1;
                let Some(t) = self.listed.clone() else {
                    return broken("systemctl");
                };
                Ok(Reply {
                    success: true,
                    bytes: t.clone().into_bytes(),
                    stdout: t.into_bytes(),
                })
            }
            other => panic!("a call the wait does not make: {other}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        assert!(
            path.starts_with(self.dir.to_str().unwrap()),
            "a read of {path}"
        );
        self.reads.borrow_mut().push(path.to_owned());
        std::fs::read(path)
    }
    fn units(&self) -> bool {
        self.units
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
    fn now(&self) -> Instant {
        self.base + self.clock.get()
    }
    fn pause(&mut self, d: Duration) {
        assert_eq!(d, Duration::from_secs(30), "the pause between readings");
        self.clock.set(self.clock.get() + d);
        self.pauses += 1;
        self.starts.push(self.f.calls.len());
        if let Some(step) = self.script.pop_front() {
            step(self);
        }
    }
}

/// A unit of the intent `walk` with a repository, a head and the given members.
fn wu(f: &mut F, key: &str, state: &str, more: Value) {
    let mut o = json!({"intent": oid("intent", "walk"), "repository": "org/repo", "head": "h1"});
    for (k, v) in more.as_object().unwrap() {
        o[k] = v.clone();
    }
    unit(f, key, state, o);
}
/// Every file under `dir` with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut all = BTreeMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            all.insert(p.clone(), vec![]);
            all.extend(tree(&p));
        } else {
            all.insert(p.clone(), std::fs::read(&p).unwrap());
        }
    }
    all
}
fn timeout_at(w: &Wt, n: usize) -> Value {
    json!({"reason": "timeout", "unit": null, "finding": null, "at": w.at(n)})
}

// spec: specs/ap/status.md#R23
#[test]
fn r23_the_wait_form_is_one_intent_a_timeout_and_a_store_else_usage() {
    let wait = |a: &[&str]| match args(a).map(|a| a.call) {
        Ok(Call::Wait(i, t)) => Some((i, t)),
        _ => None,
    };
    assert_eq!(wait(&["--wait", "walk"]), Some(("walk".into(), 14_400)));
    assert_eq!(
        wait(&["--store", "/s", "--wait", "walk", "--timeout", "30"]),
        Some(("walk".into(), 30))
    );
    assert_eq!(
        wait(&["--wait", "walk", "--timeout", "86400"]).unwrap().1,
        86_400
    );
    let long = "k".repeat(513);
    for bad in [
        vec!["--wait"],
        vec!["--wait", "-x"],
        vec!["--wait", ""],
        vec!["--wait", long.as_str()],
        vec!["--wait", "a\u{1}b"],
        vec!["--wait", "a", "--text"],
        vec!["--text", "--wait", "a"],
        vec!["--wait", "a", "--term", "unit"],
        vec!["--wait", "a", "b"],
        vec!["b", "--wait", "a"],
        vec!["--wait", "a", "--wait", "b"],
        vec!["--timeout", "30"],
        vec!["--wait", "a", "--timeout"],
        vec!["--wait", "a", "--timeout", "29"],
        vec!["--wait", "a", "--timeout", "86401"],
        vec!["--wait", "a", "--timeout", "+30"],
        vec!["--wait", "a", "--timeout", "-30"],
        vec!["--wait", "a", "--timeout", "30.0"],
        vec!["--wait", "a", "--timeout", ""],
        vec!["--wait", "a", "--timeout", "30", "--timeout", "40"],
        vec!["--wait", "a", "--store", "relative"],
    ] {
        let e = args(&bad).err().unwrap_or_else(|| panic!("{bad:?} parses"));
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{bad:?}");
    }
    // The binary refuses before any call: no program is on `PATH`.
    let (code, out, e) = status(&["--wait", "walk", "--text"]);
    assert_eq!(code, Some(2));
    assert!(out.is_empty());
    assert_eq!(e["error"]["code"], "usage");
}

// spec: specs/ap/status.md#R23
#[test]
fn r23_an_intent_the_store_does_not_have_is_unknown_exit_2_at_once() {
    let mut w = Wt::new();
    let call = Call::Wait("nobody".into(), 30);
    let e = execute(&mut w, None, &call).expect_err("an unknown intent");
    assert_eq!(
        (e.code, e.field.as_str()),
        ("unknown_intent", "intent"),
        "{e:?}"
    );
    assert_eq!(
        (w.pauses, w.f.calls.len(), w.systemctl),
        (0, 1, 0),
        "one get and no wait"
    );
    assert_eq!(ap::status::exit_code(true, &e), 2);
    // The same in the binary, against a store program that answers `unavailable`.
    let bin = w.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ont = bin.join("ont");
    let script = "#!/bin/sh\nwhile read -r line; do :; done\necho '{\"status\":\"refused\",\"code\":\"unavailable\"}'\nexit 1\n";
    std::fs::write(&ont, script).unwrap();
    std::fs::set_permissions(&ont, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["status", "--wait", "nobody"])
        .env("PATH", &bin)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(e["error"]["code"], "unknown_intent");
}

// spec: specs/ap/status.md#R33
#[test]
fn r33_an_error_of_a_wait_is_exit_3_but_an_unknown_intent_and_other_verbs_are_exit_2() {
    let failure = |code| Failure {
        code,
        field: "x".into(),
        detail: Value::Null,
    };
    for code in [
        "process_failed",
        "policy_invalid",
        "reading_failed",
        "store_refused",
    ] {
        assert_eq!(ap::status::exit_code(true, &failure(code)), 3, "{code}");
        assert_eq!(ap::status::exit_code(false, &failure(code)), 2, "{code}");
    }
    assert_eq!(ap::status::exit_code(true, &failure("unknown_intent")), 2);
}

// spec: specs/ap/status.md#R24
#[test]
fn r24_the_wait_reads_and_writes_nothing_and_applies_no_action() {
    let mut w = Wt::new();
    wu(&mut w.f, "cut", "running", json!({}));
    wu(&mut w.f, "ran", "ready", json!({}));
    w.started("ran", "r#1");
    // A unit directory with a lock file the wait must not take over or remove, and no `work.lock`.
    std::fs::create_dir_all(w.units_dir().join("cut")).unwrap();
    std::fs::write(w.units_dir().join("cut").join("lock"), "").unwrap();
    let before = tree(&w.dir);
    let at = w.answer(300);
    assert_eq!(at["reason"], "unit_idle", "{at}");
    assert_eq!(
        tree(&w.dir),
        before,
        "a file or directory was made, changed or removed"
    );
    assert!(!w.units_dir().join("work.lock").exists());
    // Only the reads of the store, one `systemctl` per reading, and the policy file.
    for name in w.f.names() {
        assert!(["get", "search"].contains(&name), "ont {name}");
    }
    assert_eq!(w.systemctl, 2);
    assert!(w.reads.borrow().iter().all(|p| p.ends_with("policy.json")));
    // Where the units directory is absent it is not made.
    let mut w = Wt::new();
    wu(&mut w.f, "cut", "running", json!({}));
    assert_eq!(w.answer(30)["reason"], "unit_idle");
    assert!(!w.units_dir().exists());
}

// spec: specs/ap/status.md#R25
#[test]
fn r25_the_first_reading_and_then_one_every_30_seconds_end_in_one_answer() {
    let mut w = Wt::new();
    wu(&mut w.f, "u-1", "running", json!({"repository": null}));
    wu(&mut w.f, "u-2", "ready", json!({"repository": null}));
    w.then(|_| {});
    w.then(|w| {
        wu(
            &mut w.f,
            "u-1",
            "stopped",
            json!({"repository": null, "ended_at": "2026-11-17T12:00:00Z"}),
        )
    });
    let v = w.answer(300);
    assert_eq!(
        v,
        json!({"reason": "unit_stopped", "unit": "u-1", "finding": null, "at": w.at(2)})
    );
    assert_eq!(
        (w.pauses, w.starts.len()),
        (2, 3),
        "readings at 0, 30 and 60 seconds"
    );
    assert_eq!(w.clock.get(), Duration::from_secs(60));
    // The answer's `at` is the clock of the reading's first answer, not of a later one.
    assert_eq!(
        w.at(2),
        "2026-11-17T12:00:".to_owned() + &format!("{:02}Z", w.starts[2])
    );
    assert_ne!(v["at"], w.at(1));
    // The first reading is the intent's `get`, then the units and the open findings.
    let first: Vec<(&str, &str)> = w.f.calls[..4]
        .iter()
        .map(|c| (c.0[0].as_str(), c.1["object_type"].as_str().unwrap_or("")))
        .collect();
    assert_eq!(first[0].0, "get");
    assert_eq!(first[1..3], [("search", "unit"), ("search", "finding")]);
}

// spec: specs/ap/status.md#R26
#[test]
fn r26_a_contract_or_effect_unit_that_newly_waits_for_approve_ends_the_wait() {
    let mut w = Wt::new();
    w.policy = None; // the running unit is no cut run here
    // At the first reading: a plain unit, a unit whose head is approved and one that waits already.
    wu(&mut w.f, "p", "accepted", json!({"class": "plain"}));
    wu(
        &mut w.f,
        "a",
        "accepted",
        json!({"class": "effect", "approved_head": "h1"}),
    );
    wu(&mut w.f, "k", "accepted", json!({"class": "contract"}));
    wu(&mut w.f, "c", "running", json!({"class": "contract"}));
    // Reading 1: heads move of the plain unit, and of the approved one together with its approval.
    w.then(|w| {
        wu(
            &mut w.f,
            "p",
            "accepted",
            json!({"class": "plain", "head": "h9"}),
        );
        wu(
            &mut w.f,
            "a",
            "accepted",
            json!({"class": "effect", "head": "h2", "approved_head": "h2"}),
        );
    });
    // Reading 2: the running unit is accepted.
    w.then(|w| {
        wu(
            &mut w.f,
            "c",
            "accepted",
            json!({"class": "contract", "head": "h3"}),
        )
    });
    let v = w.answer(600);
    assert_eq!(
        v,
        json!({"reason": "unit_accepted", "unit": "c", "finding": null, "at": w.at(2)})
    );
    assert_eq!(w.pauses, 2);
    // The unit that waited at the first reading with another head is a reason too.
    let mut w = Wt::new();
    wu(
        &mut w.f,
        "k",
        "accepted",
        json!({"class": "contract", "head": "h1"}),
    );
    w.then(|w| {
        wu(
            &mut w.f,
            "k",
            "accepted",
            json!({"class": "contract", "head": "h2"}),
        )
    });
    assert_eq!(w.answer(600)["unit"], "k");
    assert_eq!(w.pauses, 1);
}

// spec: specs/ap/status.md#R27
#[test]
fn r27_a_unit_that_newly_stops_or_stops_again_ends_the_wait() {
    let mut w = Wt::new();
    wu(
        &mut w.f,
        "s",
        "stopped",
        json!({"ended_at": "2026-11-17T10:00:00Z"}),
    );
    w.then(|w| {
        wu(
            &mut w.f,
            "s",
            "stopped",
            json!({"ended_at": "2026-11-17T10:00:00Z"}),
        )
    });
    w.then(|w| {
        wu(
            &mut w.f,
            "s",
            "stopped",
            json!({"ended_at": "2026-11-17T11:00:00Z"}),
        )
    });
    let v = w.answer(600);
    assert_eq!(
        v,
        json!({"reason": "unit_stopped", "unit": "s", "finding": null, "at": w.at(2)})
    );
    assert_eq!(
        w.pauses, 2,
        "a unit already stopped at the first reading is no reason"
    );
    let mut w = Wt::new();
    wu(&mut w.f, "r", "running", json!({}));
    w.then(|w| {
        wu(
            &mut w.f,
            "r",
            "stopped",
            json!({"ended_at": "2026-11-17T11:00:00Z"}),
        )
    });
    assert_eq!(w.answer(600)["unit"], "r");
    assert_eq!(w.pauses, 1);
}

// spec: specs/ap/status.md#R28
#[test]
fn r28_an_open_finding_of_severity_act_or_urgent_that_is_new_ends_the_wait() {
    let mut w = Wt::new();
    wu(&mut w.f, "u", "running", json!({"repository": null}));
    let seen = "2026-11-17T11:00:00Z";
    finding(&mut w.f, "f-old", "act", "open", seen, json!({}));
    w.then(|w| {
        finding(
            &mut w.f,
            "f-info",
            "info",
            "open",
            "2026-11-17T11:00:00Z",
            json!({}),
        );
        finding(
            &mut w.f,
            "f-planned",
            "urgent",
            "planned",
            "2026-11-17T11:00:00Z",
            json!({}),
        );
        finding(
            &mut w.f,
            "f-closed",
            "act",
            "closed",
            "2026-11-17T11:00:00Z",
            json!({}),
        );
        finding(
            &mut w.f,
            "f-dismissed",
            "act",
            "dismissed",
            "2026-11-17T11:00:00Z",
            json!({}),
        );
    });
    // A finding of another intent's unit or of none counts: the open findings are the store's.
    w.then(|w| {
        finding(
            &mut w.f,
            "f-new",
            "urgent",
            "open",
            "2026-11-17T11:30:00Z",
            json!({"unit": oid("unit", "other")}),
        )
    });
    let v = w.answer(600);
    assert_eq!(
        v,
        json!({"reason": "finding", "unit": null, "finding": "f-new", "at": w.at(2)})
    );
    assert_eq!(w.pauses, 2);
}

// spec: specs/ap/status.md#R29
#[test]
fn r29_an_intent_whose_units_all_ended_or_that_has_none_ends_the_wait_at_once() {
    let mut w = Wt::new();
    wu(&mut w.f, "m", "merged", json!({}));
    wu(&mut w.f, "d", "done", json!({}));
    wu(&mut w.f, "x", "dropped", json!({}));
    wu(
        &mut w.f,
        "other",
        "ready",
        json!({"intent": oid("intent", "elsewhere")}),
    );
    let v = w.answer(600);
    assert_eq!(
        v,
        json!({"reason": "all_ended", "unit": null, "finding": null, "at": w.at(0)})
    );
    assert_eq!(w.pauses, 0);
    let mut w = Wt::new();
    assert_eq!(w.answer(600)["reason"], "all_ended");
    assert_eq!(w.pauses, 0);
    // One unit not ended, until it is.
    let mut w = Wt::new();
    wu(&mut w.f, "m", "merged", json!({}));
    wu(&mut w.f, "d", "draft", json!({}));
    w.then(|w| wu(&mut w.f, "d", "merged", json!({})));
    assert_eq!(w.answer(600)["reason"], "all_ended");
    assert_eq!(w.pauses, 1);
}

/// A policy and a units directory where a cut run is read: the unit `cut` runs with no run alive.
fn idle_fixture() -> Wt {
    let mut w = Wt::new();
    wu(&mut w.f, "cut", "running", json!({}));
    w
}

// spec: specs/ap/status.md#R30
#[test]
fn r30_a_unit_no_run_drives_at_two_successive_readings_is_idle() {
    let mut w = idle_fixture();
    // A run that ended with its unit as it was: `ready` and `accepted` with `started` equal to the
    // revision; and units that are not idle: no `started`, another revision, a live run.
    wu(&mut w.f, "ran", "ready", json!({}));
    wu(&mut w.f, "fin", "accepted", json!({}));
    wu(&mut w.f, "fresh", "ready", json!({}));
    wu(&mut w.f, "stale", "ready", json!({}));
    wu(&mut w.f, "hot", "running", json!({}));
    for k in ["ran", "fin"] {
        w.started(k, "r#1");
    }
    w.started("stale", "r#0");
    w.live("hot");
    let v = w.answer(600);
    assert_eq!(
        v,
        json!({"reason": "unit_idle", "unit": "cut", "finding": null, "at": w.at(1)})
    );
    assert_eq!(w.pauses, 1, "none at the first reading, one at the second");
    // The reading breaks: idle at 0, a run alive at 1, idle at 2 and 3: the reason is at 3.
    let mut w = idle_fixture();
    w.then(|w| w.live("cut"));
    w.then(|w| w.listed = Some(String::new()));
    let v = w.answer(600);
    assert_eq!(
        (v["reason"].as_str(), v["unit"].as_str()),
        (Some("unit_idle"), Some("cut"))
    );
    assert_eq!(w.pauses, 3);
}

// spec: specs/ap/status.md#R30
#[test]
fn r30_an_accepted_unit_whose_merge_run_lives_at_two_readings_gives_no_reason() {
    let mut w = Wt::new();
    wu(&mut w.f, "acc", "accepted", json!({}));
    w.started("acc", "r#1");
    w.live("acc");
    let v = w.answer(90);
    assert_eq!(v["reason"], "timeout", "{v}");
    assert_eq!(w.pauses, 3);
    // The same unit with its run gone is idle.
    let mut w = Wt::new();
    wu(&mut w.f, "acc", "accepted", json!({}));
    w.started("acc", "r#1");
    assert_eq!(w.answer(90)["reason"], "unit_idle");
}

// spec: specs/ap/status.md#R30
#[test]
fn r30_a_cut_run_of_a_unit_under_an_active_hold_gives_no_reason() {
    let hold_on = |unit: Value, intent: Value, status: &str| {
        let mut w = idle_fixture();
        hold(
            &mut w.f,
            "h-1",
            status,
            "2026-11-17T10:00:00Z",
            json!({"unit": unit, "intent": intent}),
        );
        w.answer(90)
    };
    let (unit, intent) = (oid("unit", "cut"), oid("intent", "walk"));
    assert_eq!(
        hold_on(unit.clone(), Value::Null, "active")["reason"],
        "timeout"
    );
    assert_eq!(
        hold_on(Value::Null, intent.clone(), "active")["reason"],
        "timeout"
    );
    // A hold on another unit or intent, or one lifted, holds nothing.
    let other = (oid("unit", "other"), oid("intent", "elsewhere"));
    assert_eq!(hold_on(other.0, other.1, "active")["reason"], "unit_idle");
    assert_eq!(hold_on(unit, intent, "lifted")["reason"], "unit_idle");
}

// spec: specs/ap/status.md#R31
#[test]
fn r31_unit_idle_is_given_only_where_the_policy_and_the_service_manager_say_where_to_look() {
    let idle = |w: &mut Wt, store: Option<&str>| match w.wait(store, 90) {
        Ok(v) => v["reason"].as_str().unwrap().to_owned(),
        Err(e) => panic!("{} {} {}", e.code, e.field, e.detail),
    };
    // The control: it is given here.
    assert_eq!(idle(&mut idle_fixture(), None), "unit_idle");
    // No service manager.
    let mut w = idle_fixture();
    w.units = false;
    assert_eq!(idle(&mut w, None), "timeout");
    // No policy file, a policy without `work`.
    let mut w = idle_fixture();
    w.policy = None;
    assert_eq!(idle(&mut w, None), "timeout");
    let mut w = idle_fixture();
    w.policy = Some(w.dir.join("absent.json"));
    assert_eq!(idle(&mut w, None), "timeout");
    let mut w = idle_fixture();
    w.policy_is(json!({"units": {"directory": "/invented"}}));
    assert_eq!(idle(&mut w, None), "timeout");
    // `--store` without `AP_POLICY_FILE`, and `AP_POLICY_FILE` without `--store`: the file is not read.
    let mut w = idle_fixture();
    assert_eq!(idle(&mut w, Some(SCRATCH)), "timeout");
    assert!(w.reads.borrow().is_empty(), "the policy file was read");
    assert!(w.f.stores.iter().all(|s| s.as_deref() == Some(SCRATCH)));
    let mut w = idle_fixture();
    w.scratch();
    assert_eq!(idle(&mut w, None), "timeout");
    assert!(w.reads.borrow().is_empty(), "the policy file was read");
    // `--store` with `AP_POLICY_FILE` and a policy `ap work` accepts with `--store`: given.
    let mut w = idle_fixture();
    w.scratch();
    assert_eq!(idle(&mut w, Some(SCRATCH)), "unit_idle");
    assert_eq!(w.systemctl, 2);
    // ... and one it refuses with `--store`: without `units.directory`, `work.slice` or `work.prefix`.
    let policies = [
        json!({"work": {"prefix": PREFIX, "slice": "aptrial1.slice"}, "units": {"timeouts": {}}}),
        json!({"work": {"prefix": PREFIX}, "units": {"directory": "/invented"}}),
        json!({"work": {"slice": "aptrial1.slice"}, "units": {"directory": "/invented"}}),
    ];
    for p in policies {
        let mut w = idle_fixture();
        w.scratch();
        w.policy_is(p.clone());
        assert_eq!(idle(&mut w, Some(SCRATCH)), "timeout", "{p}");
        assert_eq!(w.systemctl, 0, "no run unit is read");
    }
    // A unit without a repository, and keys that are not of `ap unit add`'s form, give none.
    let mut w = Wt::new();
    wu(&mut w.f, "cut", "running", json!({"repository": null}));
    wu(&mut w.f, "empty", "running", json!({"repository": ""}));
    for k in ["Cut_Run", "9lives", "-dash", "has space", &"k".repeat(60)] {
        wu(&mut w.f, k, "running", json!({}));
    }
    assert_eq!(idle(&mut w, None), "timeout");
}

// spec: specs/ap/status.md#R32
#[test]
fn r32_the_timeout_ends_the_wait_after_the_reading_it_falls_in() {
    let mut w = Wt::new();
    wu(&mut w.f, "u", "running", json!({"repository": null}));
    let v = w.answer(30);
    assert_eq!(v, timeout_at(&w, 1));
    assert_eq!(w.starts.len(), 2, "a reading at 0 and one at 30 seconds");
    let mut w = Wt::new();
    wu(&mut w.f, "u", "running", json!({"repository": null}));
    assert_eq!(
        w.answer(100),
        timeout_at(&w, 4),
        "the readings at 0, 30, 60, 90 and 120"
    );
    let mut w = Wt::new();
    wu(&mut w.f, "u", "running", json!({"repository": null}));
    assert_eq!(w.answer(14_400)["reason"], "timeout");
    assert_eq!(w.pauses, 480, "the default is four hours");
}

// spec: specs/ap/status.md#R32
#[test]
fn r32_of_two_reasons_at_one_reading_the_first_in_the_order_is_given_and_the_smallest_key() {
    // Accepted, stopped, a finding and a unit still running at one reading: `unit_accepted`, `a-one`.
    let mut w = Wt::new();
    wu(&mut w.f, "r", "running", json!({"repository": null}));
    w.then(|w| {
        wu(&mut w.f, "b-two", "accepted", json!({"class": "contract"}));
        wu(&mut w.f, "a-one", "accepted", json!({"class": "effect"}));
        wu(&mut w.f, "a-0", "stopped", json!({}));
        finding(
            &mut w.f,
            "f-new",
            "act",
            "open",
            "2026-11-17T11:30:00Z",
            json!({}),
        );
    });
    let v = w.answer(600);
    assert_eq!(
        (v["reason"].as_str(), v["unit"].as_str()),
        (Some("unit_accepted"), Some("a-one"))
    );
    // Without the accepted ones: `unit_stopped`, the smallest key; then `finding`.
    let mut w = Wt::new();
    wu(&mut w.f, "r", "running", json!({"repository": null}));
    w.then(|w| {
        wu(&mut w.f, "s-two", "stopped", json!({}));
        wu(&mut w.f, "s-one", "stopped", json!({}));
        finding(
            &mut w.f,
            "f-new",
            "act",
            "open",
            "2026-11-17T11:30:00Z",
            json!({}),
        );
    });
    let v = w.answer(600);
    assert_eq!(
        (v["reason"].as_str(), v["unit"].as_str()),
        (Some("unit_stopped"), Some("s-one"))
    );
    // A finding before `all_ended`: every unit merged and a finding that is new.
    let mut w = Wt::new();
    wu(&mut w.f, "m", "running", json!({"repository": null}));
    w.then(|w| {
        wu(&mut w.f, "m", "merged", json!({}));
        finding(
            &mut w.f,
            "f-new",
            "urgent",
            "open",
            "2026-11-17T11:30:00Z",
            json!({}),
        );
    });
    assert_eq!(w.answer(600)["reason"], "finding");
    // A finding before `unit_idle`, and `unit_idle` before `timeout`.
    let mut w = idle_fixture();
    w.then(|w| {
        finding(
            &mut w.f,
            "f-new",
            "act",
            "open",
            "2026-11-17T11:30:00Z",
            json!({}),
        )
    });
    assert_eq!(w.answer(30)["reason"], "finding");
    let mut w = idle_fixture();
    assert_eq!(
        w.answer(30)["reason"],
        "unit_idle",
        "the timeout falls in the reading of the second idle"
    );
}

// spec: specs/ap/status.md#R33
#[test]
fn r33_a_reading_that_fails_is_no_reason_and_the_wait_compares_with_the_first_that_succeeded() {
    // The first reading fails; the comparisons are with the second.
    let mut w = Wt::new();
    wu(&mut w.f, "k", "accepted", json!({"class": "contract"}));
    w.down = true;
    w.then(|w| w.down = false);
    assert_eq!(
        w.answer(90),
        timeout_at(&w, 3),
        "the accepted unit of the first reading that succeeded is no reason"
    );
    // A failure between two readings: the head is the same, and no reason comes of it.
    let mut w = Wt::new();
    wu(&mut w.f, "k", "accepted", json!({"class": "contract"}));
    w.then(|w| w.down = true);
    w.then(|w| w.down = false);
    assert_eq!(w.answer(90)["reason"], "timeout");
    // Failures that are not in a row never end the wait.
    let mut w = Wt::new();
    w.down = true;
    for n in 0..5 {
        w.then(move |w| w.down = n % 2 == 1);
    }
    wu(&mut w.f, "u", "running", json!({"repository": null}));
    let v = w.answer(150);
    assert_eq!(v["reason"], "timeout");
    // The idle readings are successive across a failed one.
    let mut w = idle_fixture();
    w.then(|w| w.down = true);
    w.then(|w| w.down = false);
    let v = w.answer(600);
    assert_eq!(
        (v["reason"].as_str(), v["unit"].as_str()),
        (Some("unit_idle"), Some("cut"))
    );
    assert_eq!(w.pauses, 2);
}

// spec: specs/ap/status.md#R33
#[test]
fn r33_three_failed_readings_in_a_row_end_the_wait_with_the_last_error() {
    let mut w = Wt::new();
    w.down = true;
    let e = w.wait(None, 600).expect_err("a store that is gone");
    assert_eq!((e.code, e.field.as_str()), ("process_failed", "ont"));
    assert_eq!(w.pauses, 2, "three readings");
    // The service manager failing is a failed reading too.
    let mut w = idle_fixture();
    w.listed = None;
    let e = w.wait(None, 600).expect_err("a service manager that fails");
    assert_eq!((e.code, e.field.as_str()), ("reading_failed", "list_units"));
    assert_eq!((w.pauses, w.systemctl), (2, 3));
    // A policy that `ap work` refuses is a failed reading: its error ends the wait.
    for policy in [
        json!({"work": {"slots": 99}}),
        json!({"work": "x"}),
        json!({"work": {}, "units": {"directory": "relative"}}),
    ] {
        let mut w = idle_fixture();
        w.policy_is(policy.clone());
        let e = w.wait(None, 600).expect_err("a policy that is refused");
        assert_eq!(
            (e.code, e.field.as_str()),
            ("policy_invalid", "policy"),
            "{policy}"
        );
        assert_eq!(w.pauses, 2);
        assert_eq!(ap::status::exit_code(true, &e), 3);
    }
    let mut w = idle_fixture();
    std::fs::write(w.dir.join("policy.json"), "not json").unwrap();
    assert_eq!(
        w.wait(None, 600).expect_err("not json").code,
        "policy_invalid"
    );
    // The policy's `quota` is not read: one that `ap quota` would refuse is no failure.
    let mut w = idle_fixture();
    w.policy_is(json!({"work": {"prefix": PREFIX}, "quota": "invalid"}));
    assert_eq!(w.answer(600)["reason"], "unit_idle");
}

// spec: specs/ap/status.md#R33
#[test]
fn r33_a_timeout_ends_with_the_last_error_when_no_reading_succeeded() {
    let mut w = Wt::new();
    w.down = true;
    let e = w.wait(None, 30).expect_err("no reading succeeded");
    assert_eq!((e.code, w.pauses), ("process_failed", 1));
    // A reading that succeeded before the timeout gives its `at`, though the last one failed.
    let mut w = Wt::new();
    wu(&mut w.f, "u", "running", json!({"repository": null}));
    w.then(|w| w.down = true);
    assert_eq!(w.answer(30), timeout_at(&w, 0));
}

// The indicators (R34 to R41): a store of merged units with the log rows the engine would hold.
fn log_row(f: &mut F, action: &str, unit: &str, at: &str, more: Value) {
    let mut parameters = json!({"unit": oid("unit", unit)});
    for (k, v) in more.as_object().unwrap() {
        parameters[k] = v.clone();
    }
    f.log.push(
        json!({"action_type": action, "outcome": "applied", "parameters": parameters,
        "submitted_at": at, "actor": {"agent": "ap"}}),
    );
}
fn review(
    f: &mut F,
    key: &str,
    unit: &str,
    kind: &str,
    (status, verdict): (&str, Value),
    closed: &str,
) {
    f.put(
        "review",
        key,
        json!({"kind": kind, "unit": oid("unit", unit), "head": "h", "status": status,
        "verdict": verdict, "started_at": "2026-11-17T08:00:00Z", "closed_at": closed}),
    );
}
fn token_run(f: &mut F, key: &str, unit: &str, role: &str, tokens: [Option<u64>; 4]) {
    let [i, r, w, o] = tokens;
    run_object(
        f,
        key,
        "closed",
        "2026-11-17T09:00:00Z",
        json!({"unit": oid("unit", unit), "role": role,
        "input_tokens": i, "cache_read_tokens": r, "cache_write_tokens": w, "output_tokens": o}),
    );
}
fn indicator_store() -> F {
    let mut f = F::empty();
    intent(&mut f, "vps", "confirmed", json!({}));
    intent(&mut f, "other", "confirmed", json!({}));
    let tier = json!({"runtime": "claude", "model": "m1", "effort": "high"});
    let merged = |f: &mut F, k: &str, i: &str, more: Value| {
        let mut o = json!({"state": "merged", "intent": oid("intent", i)});
        for (m, v) in more.as_object().unwrap() {
            o[m] = v.clone();
        }
        unit(f, k, "merged", o);
    };
    merged(
        &mut f,
        "u-a",
        "vps",
        json!({"class": "contract", "implementer": tier}),
    );
    merged(&mut f, "u-b", "vps", json!({}));
    merged(&mut f, "u-e", "vps", json!({}));
    merged(&mut f, "u-c", "other", json!({}));
    unit(
        &mut f,
        "u-d",
        "running",
        json!({"intent": oid("intent", "vps")}),
    );
    // u-a: a stop and a resume, a repair, one rejected round and one accepted; the rows are put in time order.
    log_row(
        &mut f,
        "start_unit",
        "u-a",
        "2026-11-17T09:00:00Z",
        json!({}),
    );
    log_row(
        &mut f,
        "start_unit",
        "u-a",
        "2026-11-17T09:05:00Z",
        json!({}),
    );
    log_row(
        &mut f,
        "stop_unit",
        "u-a",
        "2026-11-17T09:20:00Z",
        json!({"cause": "limit"}),
    );
    log_row(
        &mut f,
        "stop_unit",
        "u-a",
        "2026-11-17T09:50:00Z",
        json!({"cause": "failed_check"}),
    );
    log_row(
        &mut f,
        "merge_unit",
        "u-a",
        "2026-11-17T10:30:30Z",
        json!({"head": "h"}),
    );
    log_row(
        &mut f,
        "start_unit",
        "u-b",
        "2026-11-17T11:00:00Z",
        json!({}),
    );
    log_row(
        &mut f,
        "merge_unit",
        "u-b",
        "2026-11-17T11:20:59Z",
        json!({}),
    );
    log_row(
        &mut f,
        "start_unit",
        "u-c",
        "2026-11-17T12:00:00Z",
        json!({}),
    );
    log_row(
        &mut f,
        "merge_unit",
        "u-c",
        "2026-11-17T12:05:00Z",
        json!({}),
    );
    log_row(
        &mut f,
        "start_unit",
        "u-d",
        "2026-11-17T12:00:00Z",
        json!({}),
    );
    // The rounds of u-a: its keys are not in time order, and only a closed packet review that said accept or repair counts.
    review(
        &mut f,
        "rv-2",
        "u-a",
        "packet",
        ("closed", json!("accept")),
        "2026-11-17T10:00:00Z",
    );
    review(
        &mut f,
        "rv-9",
        "u-a",
        "packet",
        ("closed", json!("repair")),
        "2026-11-17T09:40:00Z",
    );
    review(
        &mut f,
        "rv-3",
        "u-a",
        "page",
        ("closed", json!("accept")),
        "2026-11-17T08:30:00Z",
    );
    review(
        &mut f,
        "rv-4",
        "u-a",
        "packet",
        ("closed", json!("abandoned")),
        "2026-11-17T08:40:00Z",
    );
    review(
        &mut f,
        "rv-5",
        "u-a",
        "packet",
        ("running", Value::Null),
        "",
    );
    review(
        &mut f,
        "rv-6",
        "u-a",
        "packet",
        ("revoked", json!("accept")),
        "2026-11-17T08:50:00Z",
    );
    review(
        &mut f,
        "rv-7",
        "u-b",
        "packet",
        ("closed", json!("accept")),
        "2026-11-17T11:10:00Z",
    );
    token_run(
        &mut f,
        "run-1",
        "u-a",
        "implementer",
        [Some(10), Some(20), None, Some(40)],
    );
    token_run(
        &mut f,
        "run-2",
        "u-a",
        "implementer",
        [Some(1), None, None, Some(4)],
    );
    token_run(
        &mut f,
        "run-3",
        "u-a",
        "fixer",
        [Some(5), Some(6), Some(7), Some(8)],
    );
    token_run(&mut f, "run-4", "u-a", "verifier", [None; 4]);
    f
}
fn indicators_of(f: &mut F, intent: Option<&str>) -> Value {
    run(f, &Call::Indicators(intent.map(str::to_owned)))
}

// spec: specs/ap/status.md#R34
#[test]
fn r34_the_indicators_form_takes_an_optional_intent_a_store_and_text_else_usage() {
    for bad in [
        vec!["--indicators", "--term", "unit"],
        vec!["--indicators", "--wait", "vps"],
        vec!["--indicators", "--timeout", "30"],
        vec!["--indicators", "a", "b"],
        vec!["--indicators", "--indicators"],
        vec!["--indicators", "-x"],
        vec!["--indicators", ""],
        vec!["--indicators", "--store", "relative"],
        vec!["--indicators", "--text", "--text"],
        vec!["--wait", "vps", "--indicators"],
    ] {
        assert_eq!(args(&bad).err().map(|e| e.code), Some("usage"), "{bad:?}");
    }
    for ok in [
        vec!["--indicators"],
        vec!["--indicators", "vps"],
        vec!["vps", "--text", "--store", "/s", "--indicators"],
    ] {
        assert!(args(&ok).is_ok(), "{ok:?}");
    }
    // A refusal of the form is made before any call: no program is found.
    let (status, out, e) = status(&["--indicators", "--timeout", "30"]);
    assert_eq!((status, out.is_empty()), (Some(2), true));
    assert_eq!(e["error"]["code"], "usage");
    // An intent the store does not have: `unknown_intent`, exit 2, after one read of the intent.
    let mut f = indicator_store();
    let e = refusal(&mut f, &Call::Indicators(Some("nonesuch".into())));
    assert_eq!((e.code, e.field.as_str()), ("unknown_intent", "intent"));
    assert_eq!(ap::status::exit_code(false, &e), 2);
    assert_eq!(f.names(), ["get"]);
}

// spec: specs/ap/status.md#R35
#[test]
fn r35_one_row_for_each_merged_unit_of_the_intent_or_of_the_store() {
    let mut f = indicator_store();
    let v = indicators_of(&mut f, Some("vps"));
    assert_eq!(
        v["at"], "2026-11-17T12:00:00Z",
        "the now of the first answer"
    );
    assert_eq!(strs(&column(&v["units"], "unit")), ["u-a", "u-b", "u-e"]);
    let a = &v["units"][0];
    assert_eq!(a["intent"], "vps");
    assert_eq!(a["class"], "contract");
    assert_eq!(
        a["implementer"],
        json!({"runtime": "claude", "model": "m1", "effort": "high"})
    );
    assert_eq!(v["units"][1]["implementer"], Value::Null);
    let all = indicators_of(&mut f, None);
    assert_eq!(all["at"], "2026-11-17T12:00:00Z");
    assert_eq!(
        strs(&column(&all["units"], "unit")),
        ["u-a", "u-b", "u-c", "u-e"]
    );
    assert_eq!(all["units"][2]["intent"], "other");
    assert_eq!(
        f.names()[..2],
        ["search", "log"],
        "no read of an intent without one named"
    );
    // No merged unit: no row, and the log is not read.
    let mut none = F::empty();
    let v = indicators_of(&mut none, None);
    assert_eq!(v["units"], json!([]));
    assert_eq!(none.names(), ["search"]);
}

// spec: specs/ap/status.md#R36
#[test]
fn r36_the_minutes_run_from_the_first_start_to_the_merge_rounded_down() {
    let mut f = indicator_store();
    let v = indicators_of(&mut f, Some("vps"));
    let (a, b) = (&v["units"][0], &v["units"][1]);
    // The first of two `start_unit` rows (the log answers newest first), the merge's second counted away.
    assert_eq!(a["started_at"], "2026-11-17T09:00:00Z");
    assert_eq!(a["merged_at"], "2026-11-17T10:30:30Z");
    assert_eq!(a["minutes"], 90);
    assert_eq!(
        (b["minutes"].clone(), b["started_at"].clone()),
        (json!(20), json!("2026-11-17T11:00:00Z"))
    );
    // The stops, in the order they were applied.
    assert_eq!(a["stops"], json!(["limit", "failed_check"]));
    assert_eq!(b["stops"], json!([]));
    // A merge before the start gives no minutes.
    let mut g = indicator_store();
    log_row(
        &mut g,
        "merge_unit",
        "u-b",
        "2026-11-17T10:00:00Z",
        json!({}),
    );
    g.log.rotate_right(1);
    let v = indicators_of(&mut g, Some("vps"));
    assert_eq!(v["units"][1]["minutes"], Value::Null);
}

// spec: specs/ap/status.md#R37
#[test]
fn r37_rounds_repairs_the_first_verdict_and_the_tokens_by_role() {
    let mut f = indicator_store();
    let v = indicators_of(&mut f, Some("vps"));
    let a = &v["units"][0];
    // Closed packet reviews that said accept or repair: not a page, an abandoned, a running or a revoked one.
    assert_eq!(a["rounds"], 2);
    assert_eq!(a["first_verdict"], "repair", "by closed_at, not by key");
    assert_eq!(a["repairs"], 1);
    assert_eq!(
        a["tokens"],
        json!({
        "implementer": {"input": 11, "cache_read": 20, "cache_write": null, "output": 44},
        "fixer": {"input": 5, "cache_read": 6, "cache_write": 7, "output": 8},
        "verifier": {"input": null, "cache_read": null, "cache_write": null, "output": null}})
    );
    let b = &v["units"][1];
    assert_eq!(
        (
            b["rounds"].clone(),
            b["repairs"].clone(),
            b["first_verdict"].clone()
        ),
        (json!(1), json!(0), json!("accept"))
    );
    assert_eq!(b["tokens"], json!({}));
    let e = &v["units"][2];
    assert_eq!(
        (e["rounds"].clone(), e["first_verdict"].clone()),
        (json!(0), Value::Null)
    );
    // Two reviews closed at one second go by key.
    let mut g = indicator_store();
    review(
        &mut g,
        "rv-0",
        "u-b",
        "packet",
        ("closed", json!("repair")),
        "2026-11-17T11:10:00Z",
    );
    let v = indicators_of(&mut g, Some("vps"));
    assert_eq!(v["units"][1]["first_verdict"], "repair");
    assert_eq!(v["units"][1]["rounds"], 2);
}

// spec: specs/ap/status.md#R38
#[test]
fn r38_the_log_is_read_by_action_type_to_the_end_and_the_rest_through_the_unit_views() {
    let mut f = indicator_store();
    f.log_page = 1;
    let paged = indicators_of(&mut f, Some("vps"));
    let names = f.names();
    let logs: Vec<&Value> = f
        .calls
        .iter()
        .filter(|c| c.0[0] == "log")
        .map(|c| &c.1)
        .collect();
    // One read of each action type, paged by continuation to the end (a page of one row).
    let first = |a: &str| {
        logs.iter()
            .find(|b| b["action_type"] == a)
            .map(|b| (*b).clone())
            .unwrap()
    };
    assert_eq!(
        first("start_unit"),
        json!({"action_type": "start_unit", "outcome": "applied", "parameters": true, "limit": 100})
    );
    assert_eq!(
        logs.iter()
            .filter(|b| b["action_type"] == "start_unit")
            .count(),
        5
    );
    assert_eq!(
        logs.iter()
            .filter(|b| b["action_type"] == "stop_unit")
            .count(),
        2
    );
    assert_eq!(
        logs.iter()
            .filter(|b| b["action_type"] == "merge_unit")
            .count(),
        3
    );
    assert_eq!(&names[..2], ["get", "search"]);
    // Each unit's reviews and runs as `ap status <unit>` reads them: a view, then a get of each object.
    let rendered = f.calls.iter().filter(|c| c.0[0] == "render").count();
    assert_eq!(rendered, 6, "two views of each of three units");
    f.log_page = 100;
    assert_eq!(
        indicators_of(&mut f, Some("vps")),
        paged,
        "the page size changes nothing"
    );
    // The unit of a row is read from its parameters: a row of another unit is not this unit's.
    let mut g = indicator_store();
    log_row(
        &mut g,
        "merge_unit",
        "u-zzz",
        "2026-11-17T08:00:00Z",
        json!({}),
    );
    assert_eq!(indicators_of(&mut g, Some("vps")), paged);
    // A continuation the engine repeats is unreadable, and names the log.
    let mut h = indicator_store();
    h.log_loops = true;
    let e = refusal(&mut h, &Call::Indicators(None));
    assert_eq!(
        (e.code, e.field.as_str()),
        ("store_answer_unreadable", "log")
    );
    // Nothing is written and no file is read.
    assert!(f.reads.borrow().is_empty());
}

// spec: specs/ap/status.md#R39
#[test]
fn r39_a_missing_row_or_a_withheld_log_is_null_and_marks_the_rows_cut() {
    let mut f = indicator_store();
    let v = indicators_of(&mut f, Some("vps"));
    assert!(v["units"][0].get("log_cut").is_none() && v["units"][1].get("log_cut").is_none());
    let e = &v["units"][2];
    assert_eq!(
        (
            e["started_at"].clone(),
            e["merged_at"].clone(),
            e["minutes"].clone()
        ),
        (Value::Null, Value::Null, Value::Null)
    );
    assert_eq!(e["log_cut"], true, "no start and no merge row");
    assert_eq!(e["stops"], json!([]), "no stop row is no stop");
    // A unit with a start and no merge row: the merge is null, the start stays.
    let mut g = indicator_store();
    g.log.retain(|r| {
        !(r["action_type"] == "merge_unit" && r["parameters"]["unit"]["native_key"] == "u-b")
    });
    let v = indicators_of(&mut g, Some("vps"));
    let b = &v["units"][1];
    assert_eq!(
        (
            b["started_at"].clone(),
            b["merged_at"].clone(),
            b["minutes"].clone(),
            b["log_cut"].clone()
        ),
        (
            json!("2026-11-17T11:00:00Z"),
            Value::Null,
            Value::Null,
            json!(true)
        )
    );
    // Rows the engine withheld: that action's members are null on every row, and every row is cut.
    for (action, nulls) in [
        ("start_unit", ["started_at", "minutes"]),
        ("merge_unit", ["merged_at", "minutes"]),
        ("stop_unit", ["stops", "stops"]),
    ] {
        let mut h = indicator_store();
        h.withheld.insert(action, 1);
        let v = indicators_of(&mut h, Some("vps"));
        for row in v["units"].as_array().unwrap() {
            assert_eq!(row["log_cut"], true, "{action}");
            assert!(nulls.iter().all(|m| row[*m].is_null()), "{action} {row}");
            assert!(row["rounds"].is_u64());
        }
        let others = &v["units"][0];
        let kept = ["started_at", "merged_at", "stops"]
            .iter()
            .filter(|m| !others[**m].is_null())
            .count();
        assert_eq!(kept, 2, "{action}: the other two actions are still read");
    }
}

// spec: specs/ap/status.md#R40
#[test]
fn r40_the_summary_holds_the_count_the_medians_and_the_share_of_first_accepts() {
    let mut f = indicator_store();
    let v = indicators_of(&mut f, Some("vps"));
    // Minutes of 90 and 20 (u-e has none): 55; rounds 2, 1, 0: 1; repairs 1, 0, 0: 0; first verdicts repair, accept.
    assert_eq!(
        v["summary"],
        json!({"count": 3, "median_minutes": 55, "median_rounds": 1, "median_repairs": 0,
        "first_accept_share": 0.5})
    );
    // An even count: the mean of the two middle values, a half where they add to an odd number.
    let mut g = indicator_store();
    g.objects.remove(&("unit".to_owned(), "u-e".to_owned()));
    log_row(
        &mut g,
        "start_unit",
        "u-b",
        "2026-11-17T08:00:00Z",
        json!({}),
    );
    g.log.rotate_right(1);
    // u-b now starts at 08:00 and ends at 11:20: 200 minutes; with u-a's 90: 145.
    let v = indicators_of(&mut g, Some("vps"));
    assert_eq!(v["summary"]["median_minutes"], 145);
    assert_eq!(v["summary"]["median_rounds"], 1.5);
    assert_eq!(v["summary"]["count"], 2);
    // No row: a count of 0 and null elsewhere.
    let mut none = F::empty();
    let v = indicators_of(&mut none, None);
    assert_eq!(
        v["summary"],
        json!({"count": 0, "median_minutes": null, "median_rounds": null,
        "median_repairs": null, "first_accept_share": null})
    );
    // A third that is not a half rounds to three places.
    let mut h = indicator_store();
    h.objects.remove(&("unit".to_owned(), "u-e".to_owned()));
    review(
        &mut h,
        "rv-8",
        "u-c",
        "packet",
        ("closed", json!("accept")),
        "2026-11-17T12:03:00Z",
    );
    let v = indicators_of(&mut h, None);
    // u-a repair, u-b accept, u-c accept
    assert_eq!(v["summary"]["first_accept_share"], 0.667);
}

// spec: specs/ap/status.md#R41
#[test]
fn r41_the_text_is_one_line_for_each_row_and_the_summary() {
    let mut f = indicator_store();
    let v = indicators_of(&mut f, Some("vps"));
    let text = text_of(&Call::Indicators(Some("vps".into())), &v);
    let lines = [
        "u-a vps contract claude/m1/high 2026-11-17T09:00:00Z 2026-11-17T10:30:30Z 90 2 1 repair limit,failed_check fixer:5/6/7/8,implementer:11/20/-/44,verifier:-/-/-/-",
        "u-b vps plain - 2026-11-17T11:00:00Z 2026-11-17T11:20:59Z 20 1 0 accept none -",
        "u-e vps plain - - - - 0 0 - none - log_cut",
        "summary 3 units median minutes 55 rounds 1 repairs 0 first accepted 0.5",
    ];
    assert_eq!(text, lines.join("\n") + "\n");
    let mut none = F::empty();
    let v = indicators_of(&mut none, None);
    assert_eq!(
        text_of(&Call::Indicators(None), &v),
        "summary 0 units median minutes - rounds - repairs - first accepted -\n"
    );
}

/// A request routed to an intent, said on the given minute of 2026-11-16.
fn routed(f: &mut F, key: &str, to: &str, minute: u32) {
    f.put("request", key, json!({"topic": "walk/one", "kind": "order", "said_at": format!("2026-11-16T08:{minute:02}:00Z"),
        "status": "current", "route": "intent", "intent": oid("intent", to), "words_verified": true}));
}
fn brief(f: &mut F, intent: &str) -> Vec<String> {
    let call = Call::Brief(intent.into());
    let v = run(f, &call);
    let text = text_of(&call, &v);
    assert!(text.ends_with('\n'));
    text.lines().map(str::to_owned).collect()
}
fn seven_requests(f: &mut F) {
    // Said out of key order: the newest five by `said_at` stay, oldest first.
    for (k, m) in [
        ("r-g", 6),
        ("r-a", 0),
        ("r-f", 5),
        ("r-b", 1),
        ("r-e", 4),
        ("r-c", 2),
        ("r-d", 3),
    ] {
        routed(f, k, "alpha-report", m);
    }
    routed(f, "r-beta", "beta-index", 9);
}

// spec: specs/ap/status.md#R42
#[test]
fn r42_an_intent_shows_its_newest_five_requests_and_their_count() {
    let mut f = fixture();
    seven_requests(&mut f);
    let v = run(&mut f, &key("alpha-report"));
    assert_eq!(v["requests_count"], 7);
    let kept = ["r-c", "r-d", "r-e", "r-f", "r-g"];
    assert_eq!(strs(&column(&v["requests"], "request")), kept);
    // The rows are those of `ap intent show`.
    assert_eq!(
        v["requests"][0],
        json!({"request": "r-c", "topic": "walk/one", "kind": "order", "said_at": "2026-11-16T08:02:00Z", "words_verified": true})
    );
    // `--text` prints the count and the same five.
    let text = text_of(&key("alpha-report"), &v);
    assert!(text.contains("requests_count: 7\n"), "{text}");
    assert!(!text.contains("r-a") && !text.contains("r-b"), "{text}");
    assert!(text.contains("r-g"));
    // `--requests` keeps every one, with the count.
    let all = run(&mut f, &Call::KeyAll("alpha-report".into()));
    assert_eq!(all["requests_count"], 7);
    assert_eq!(
        strs(&column(&all["requests"], "request")),
        ["r-a", "r-b", "r-c", "r-d", "r-e", "r-f", "r-g"]
    );
    let text = text_of(&Call::KeyAll("alpha-report".into()), &all);
    assert!(text.contains("r-a") && text.contains("requests_count: 7"));
    // Five or fewer are all shown; the count is there too.
    let beta = run(&mut f, &key("beta-index"));
    assert_eq!(
        (beta["requests_count"].clone(), count_of(&beta["requests"])),
        (json!(1), 1)
    );
    // A key that names only a unit has neither member, with or without the flag.
    for call in [key("u-run"), Call::KeyAll("u-run".into())] {
        let v = run(&mut f, &call);
        assert!(v.get("requests").is_none() && v.get("requests_count").is_none());
    }
    // The form: a flag with a key only, at most once.
    assert!(matches!(
        args(&["alpha-report", "--requests", "--text"])
            .unwrap()
            .call,
        Call::KeyAll(_)
    ));
    assert!(matches!(
        args(&["--requests", "--store", "/s", "u-run"])
            .unwrap()
            .call,
        Call::KeyAll(_)
    ));
    for bad in [
        vec!["--requests"],
        vec!["--requests", "--text"],
        vec!["k", "--requests", "--requests"],
        vec!["k", "--requests", "--term", "unit"],
        vec!["--term", "unit", "--requests"],
        vec!["--requests", "--indicators"],
        vec!["--requests", "--wait", "k"],
        vec!["--requests", "--brief", "k"],
    ] {
        let e = args(&bad).err().unwrap_or_else(|| panic!("{bad:?}"));
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{bad:?}");
    }
}
fn count_of(v: &Value) -> usize {
    v.as_array().unwrap().len()
}

// spec: specs/ap/status.md#R43
#[test]
fn r43_the_brief_takes_one_intent_and_a_store_and_only_reads() {
    for ok in [
        vec!["--brief", "alpha-report"],
        vec!["--store", "/s", "--brief", "alpha-report"],
        vec!["--store", "/s", "--brief", "k"],
    ] {
        assert!(matches!(args(&ok).unwrap().call, Call::Brief(_)), "{ok:?}");
    }
    let bad: Vec<Vec<&str>> = vec![
        vec!["--brief"],
        vec!["--brief", "--store", "/s"],
        vec!["--brief", "-x"],
        vec!["--brief", "a\u{1}b"],
        vec!["--brief", "a", "--brief", "b"],
        vec!["--brief", "a", "--text"],
        vec!["--brief", "a", "b"],
        vec!["--brief", "a", "--term", "unit"],
        vec!["--brief", "a", "--wait", "b"],
        vec!["--brief", "a", "--timeout", "40"],
        vec!["--brief", "a", "--indicators"],
        vec!["--brief", "a", "--requests"],
    ];
    for a in &bad {
        // Refused before any call: `PATH` names no program.
        let (status, out, e) = status(a);
        assert_eq!(status, Some(2), "{a:?}");
        assert!(out.is_empty());
        assert_eq!(
            e,
            json!({"error": {"code": "usage", "field": "args", "detail": null}}),
            "{a:?}"
        );
    }
    // Only reads, of the store and of delivery's two files; the store is forwarded to every call.
    let mut f = fixture();
    execute(
        &mut f,
        Some("/tmp/scratch/s.sqlite"),
        &Call::Brief("alpha-report".into()),
    )
    .unwrap();
    assert!(
        f.stores
            .iter()
            .all(|s| s.as_deref() == Some("/tmp/scratch/s.sqlite"))
    );
    let names = f.names();
    assert!(
        names.iter().all(|n| ["get", "search"].contains(n)),
        "{names:?}"
    );
    assert!(f.reads.borrow().iter().all(|p| p == CONFIG || p == LAST));
    assert!(f.reads.borrow().iter().any(|p| p == LAST));
    // A key that is no intent, or that names only a unit, is `unknown_intent` with exit 2.
    for k in ["no-such", "u-run"] {
        let e = refusal(&mut f, &Call::Brief(k.into()));
        assert_eq!(
            (e.code, e.field.as_str()),
            ("unknown_intent", "intent"),
            "{k}"
        );
        assert_eq!(ap::status::exit_code(false, &e), 2);
    }
}

// spec: specs/ap/status.md#R45
#[test]
fn r45_the_brief_has_the_sections_in_their_order_with_their_lines() {
    let mut f = fixture();
    let lines = brief(&mut f, "alpha-report");
    assert_eq!(
        lines,
        [
            "status alpha-report assumed, confirmed no",
            "status lead -",
            "criterion c1 open",
            "criterion c2 pass",
            "criterion c3 pass",
            "unit u-run running plain",
            "unit u-ready ready contract waits on a unit; a limit",
            "unit u-draft draft plain",
            "needs finding f-urgent: Summary of f-urgent.",
            "needs finding f-act-new: Summary of f-act-new.",
            "needs finding f-act-old: Summary of f-act-old.",
            "needs request q-a: unrouted since 2026-11-15T10:30:00Z",
            "hold effect of h-intent until a condition intent alpha-report",
            "hold effect of h-unit until a condition unit u-run scope one unit",
            "hold effect of h-user until a condition",
            "delivery 2026-11-17T11:30:00Z current tip 0123abc target /usr/local/bin/ap",
        ]
    );
    // The Lead and who confirmed it; a delegated intent.
    f.put("intent", "alpha-report", {
        let mut o = f.objects[&("intent".to_owned(), "alpha-report".to_owned())].clone();
        o["status"] = json!("confirmed");
        o["lead_session"] = json!("s-lead");
        o["confirmed_delegated"] = json!(true);
        o
    });
    let lines = brief(&mut f, "alpha-report");
    assert_eq!(
        &lines[..2],
        [
            "status alpha-report confirmed, confirmed by delegation",
            "status lead s-lead"
        ]
    );
    // A dependency that is not merged or done is named for a unit that is not `ready`; a `ready` one
    // says what it waits on, a stopped one why it stopped.
    unit(
        &mut f,
        "u-late",
        "running",
        json!({"intent": oid("intent", "alpha-report"), "depends_on": [oid("unit", "u-ready"), oid("unit", "u-merged"), oid("unit", "u-gone")]}),
    );
    unit(
        &mut f,
        "u-halt",
        "stopped",
        json!({"intent": oid("intent", "alpha-report"), "stop_cause": "ci_failed", "depends_on": [oid("unit", "u-ready")]}),
    );
    let lines = brief(&mut f, "alpha-report");
    let units: Vec<&String> = lines.iter().filter(|l| l.starts_with("unit ")).collect();
    assert_eq!(
        units,
        [
            "unit u-halt stopped plain stopped: ci_failed",
            "unit u-late running plain blocked by u-ready (ready), u-gone (missing)",
            "unit u-run running plain",
            "unit u-ready ready contract waits on a unit; a limit",
            "unit u-draft draft plain",
        ]
    );
    // An intent with nothing else still has its status, Lead and delivery.
    intent(&mut f, "empty-one", "draft", json!({}));
    assert_eq!(
        brief(&mut f, "empty-one"),
        [
            "status empty-one draft, confirmed no",
            "status lead -",
            "needs finding f-urgent: Summary of f-urgent.",
            "needs finding f-act-new: Summary of f-act-new.",
            "needs finding f-act-old: Summary of f-act-old.",
            "needs request q-a: unrouted since 2026-11-15T10:30:00Z",
            "hold effect of h-user until a condition",
            "delivery 2026-11-17T11:30:00Z current tip 0123abc target /usr/local/bin/ap",
        ]
    );
    f.files.remove(LAST);
    assert_eq!(
        brief(&mut f, "empty-one").last().unwrap(),
        "delivery unreadable"
    );
}

// spec: specs/ap/status.md#R45
#[test]
fn r45_a_section_over_its_share_is_cut_and_the_units_take_the_lines_left() {
    let mut f = fixture();
    for n in 4..=12 {
        criterion(&mut f, &format!("c{n:02}"), "alpha-report", "open");
    }
    let a = || oid("intent", "alpha-report");
    for n in 0..30 {
        unit(
            &mut f,
            &format!("u-more-{n:02}"),
            "ready",
            json!({"intent": a(), "waits_on": "a limit"}),
        );
    }
    let lines = brief(&mut f, "alpha-report");
    let of = |label: &str| lines.iter().filter(|l| l.starts_with(label)).count();
    // 12 criteria: 7 and the line that says 5 more.
    assert_eq!(of("criterion"), 8);
    assert!(lines.contains(&"criterion (+5 more)".to_owned()));
    // 4 needs and 3 holds leave 2 and 2 lines to the units.
    assert_eq!(of("needs"), 4);
    assert_eq!(of("hold"), 3);
    assert_eq!(of("unit"), 16 + 2 + 2);
    assert!(lines.contains(&"unit (+14 more)".to_owned()), "{lines:?}");
    assert_eq!(lines.len(), 2 + 8 + 20 + 4 + 3 + 1);
    // Needs and holds over their shares.
    for n in 0..10 {
        finding(
            &mut f,
            &format!("f-more-{n}"),
            "act",
            "open",
            "2026-11-17T09:00:00Z",
            json!({}),
        );
        hold(
            &mut f,
            &format!("h-more-{n}"),
            "active",
            "2026-11-17T09:00:00Z",
            json!({"intent": a()}),
        );
    }
    let lines = brief(&mut f, "alpha-report");
    let of = |label: &str| lines.iter().filter(|l| l.starts_with(label)).count();
    assert_eq!((of("needs"), of("hold"), of("unit")), (6, 5, 16));
    assert!(lines.contains(&"needs (+9 more)".to_owned()), "{lines:?}");
    assert!(lines.contains(&"hold (+9 more)".to_owned()), "{lines:?}");
    assert_eq!(lines.len(), 38);
    // The order of the sections.
    let order: Vec<&str> = lines.iter().map(|l| l.split(' ').next().unwrap()).collect();
    let mut seen: Vec<&str> = vec![];
    for l in order {
        if seen.last() != Some(&l) {
            seen.push(l);
        }
    }
    assert_eq!(
        seen,
        ["status", "criterion", "unit", "needs", "hold", "delivery"]
    );
}

// spec: specs/ap/status.md#R46
#[test]
fn r46_the_brief_names_what_needs_the_lead_the_holds_and_the_delivery_of_the_intent() {
    let mut f = fixture();
    // Stopped and accepted units of the intent come between the urgent and the act findings.
    let a = || oid("intent", "alpha-report");
    unit(
        &mut f,
        "u-s",
        "stopped",
        json!({"intent": a(), "stop_cause": "ci_failed"}),
    );
    unit(&mut f, "u-a", "accepted", json!({"intent": a()}));
    // A finding that names another intent or one of its units, and an old request, are not the intent's.
    finding(
        &mut f,
        "f-beta",
        "urgent",
        "open",
        "2026-11-17T09:00:00Z",
        json!({"intent": oid("intent", "beta-index")}),
    );
    finding(
        &mut f,
        "f-beta-unit",
        "act",
        "open",
        "2026-11-17T09:00:00Z",
        json!({"unit": oid("unit", "u-stop")}),
    );
    finding(
        &mut f,
        "f-mine",
        "urgent",
        "open",
        "2026-11-17T08:00:00Z",
        json!({"unit": oid("unit", "u-run")}),
    );
    finding(
        &mut f,
        "f-mine-intent",
        "act",
        "open",
        "2026-11-17T07:00:00Z",
        json!({"intent": a()}),
    );
    f.put("request", "q-named", json!({"topic": "t", "kind": "order", "said_at": "2026-11-01T00:00:00Z", "status": "current", "route": "unrouted", "unit": oid("unit", "u-ready"), "words_verified": true}));
    // Eight needs, the share is six: five and the line that counts the other three.
    let lines = brief(&mut f, "alpha-report");
    let needs: Vec<&str> = lines
        .iter()
        .filter(|l| l.starts_with("needs"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        needs,
        [
            "needs finding f-urgent: Summary of f-urgent.",
            "needs finding f-mine: Summary of f-mine.",
            "needs unit u-s: ci_failed",
            "needs unit u-a: waits for the Lead's reading or merge",
            "needs finding f-act-new: Summary of f-act-new.",
            "needs (+3 more)",
        ]
    );
    // Without the findings and the request that name nothing, all that remain fit.
    for k in ["f-act-new", "f-act-old"] {
        f.objects.remove(&("finding".to_owned(), k.to_owned()));
    }
    f.objects.remove(&("request".to_owned(), "q-a".to_owned()));
    let lines = brief(&mut f, "alpha-report");
    let needs: Vec<&str> = lines
        .iter()
        .filter(|l| l.starts_with("needs"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        needs,
        [
            "needs finding f-urgent: Summary of f-urgent.",
            "needs finding f-mine: Summary of f-mine.",
            "needs unit u-s: ci_failed",
            "needs unit u-a: waits for the Lead's reading or merge",
            "needs finding f-mine-intent: Summary of f-mine-intent.",
        ]
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("f-beta") || l.contains("q-named"))
    );
    // Holds: those that name the intent or its units, then those that name neither.
    let holds: Vec<&str> = lines
        .iter()
        .filter(|l| l.starts_with("hold "))
        .map(String::as_str)
        .collect();
    assert_eq!(
        holds,
        [
            "hold effect of h-intent until a condition intent alpha-report",
            "hold effect of h-unit until a condition unit u-run scope one unit",
            "hold effect of h-user until a condition",
        ]
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("delivery 2026-11-17T11:30:00Z current tip 0123abc")
    );
}

// spec: specs/ap/status.md#R44
#[test]
fn r44_the_brief_is_at_most_forty_lines_of_at_most_120_bytes_whatever_the_store_holds() {
    let mut f = fixture();
    let a = || oid("intent", "alpha-report");
    // Long values, line breaks, control characters and characters of three bytes.
    let wide = "\u{3042}".repeat(200);
    let nasty = format!("first\nsecond\u{7}third {wide}");
    for n in 0..30 {
        criterion(&mut f, &format!("c-{n:02}-{wide}"), "alpha-report", "open");
        unit(
            &mut f,
            &format!("u-{n:02}"),
            "ready",
            json!({"intent": a(), "waits_on": nasty}),
        );
        finding(
            &mut f,
            &format!("f-{n:02}"),
            "urgent",
            "open",
            "2026-11-17T09:00:00Z",
            json!({"summary": nasty}),
        );
        hold(
            &mut f,
            &format!("h-{n:02}"),
            "active",
            "2026-11-17T09:00:00Z",
            json!({"intent": a(), "effect": nasty, "until": nasty}),
        );
    }
    unit(
        &mut f,
        "u-stop-nasty",
        "stopped",
        json!({"intent": a(), "stop_cause": nasty}),
    );
    let call = Call::Brief("alpha-report".into());
    let v = run(&mut f, &call);
    let text = text_of(&call, &v);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    assert!((20..=40).contains(&lines.len()), "{}", lines.len());
    let labels = ["status", "criterion", "unit", "needs", "hold", "delivery"];
    for l in &lines {
        assert!(l.len() <= 120, "{} bytes: {l}", l.len());
        assert!(l.ends_with('\n'));
        let body = &l[..l.len() - 1];
        assert!(!body.chars().any(char::is_control), "{body:?}");
        assert!(
            labels.iter().any(|b| body.split(' ').next() == Some(*b)),
            "{body:?}"
        );
    }
    // The cut line is at a character boundary: three bytes to a character, never a half.
    assert!(lines.iter().any(|l| l.contains('\u{3042}')));
    // A stored value with a line break is one line.
    assert!(text.contains("first second third "));
    // Well under the 4800 bytes the lines allow.
    assert!(text.len() <= 40 * 120);
}
