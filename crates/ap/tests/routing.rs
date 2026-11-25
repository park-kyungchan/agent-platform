//! The routing map (`specs/ap/routing.md`) against a fake host: `ap unit add` and `amend` with the policy's member
//! `routing` (the three refusals, the size, the row, the live bars, no check without the member), and the pass
//! `ap measure routing` over the merged units of a confirmed intent (the findings it opens, sees again, reopens and
//! closes). The fake `ont` keeps a store in memory (search, get, log, render and the actions the tests apply); the
//! policy file is written under a scratch directory. No test reads the host's store or policy file.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Reply, System};
use ap::measure;
use ap::unit::object::{Refusal, arguments, execute};
use ap::unit::routing::{self, bar_key};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const HAIKU: &str = "claude-haiku-5-5";
const SONNET: &str = "claude-sonnet-5-5";
const OPUS: &str = "claude-opus-5-5";

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn reply(v: &Value) -> Answer {
    let bytes = v.to_string().into_bytes();
    Ok(Reply {
        success: v["status"] == "ok",
        stdout: bytes.clone(),
        bytes,
    })
}
fn wrap(f: &Value) -> Value {
    (f.as_object().unwrap().iter())
        .map(|(n, v)| (n.clone(), json!({"kind": "value", "value": v})))
        .collect()
}

/// The routing map of the tests: three Claude tiers; `schema` needs the strongest, the rest the weakest to implement
/// a small unit and the middle one to verify; a bar of 0.4 over seven days and three merged units.
fn map() -> Value {
    json!({"order": [["claude", HAIKU], ["claude", SONNET], ["claude", OPUS]],
        "large_lines": 400,
        "map": {
            "default": {"small": {"implementer": ["claude", HAIKU], "verifier": ["claude", SONNET]},
                "large": {"implementer": ["claude", SONNET], "verifier": ["claude", SONNET]}},
            "schema": {"small": {"implementer": ["claude", OPUS], "verifier": ["claude", OPUS]},
                "large": {"implementer": ["claude", OPUS], "verifier": ["claude", OPUS]}}},
        "bar": {"rate": 0.4, "window_days": 7, "min_merged": 3}})
}

/// The fake host: the wall clock, a policy file in a scratch directory, and a store in memory.
struct Fake {
    dir: PathBuf,
    wall: u64,
    policy: Option<PathBuf>,
    objects: BTreeMap<(String, String), Value>,
    requests: BTreeMap<String, (Value, Value)>,
    log: Vec<Value>,
    calls: Vec<(Vec<String>, Value)>,
    /// The finding actions the store refuses, by key.
    refuse: Vec<String>,
    fail_search: bool,
}
impl Fake {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ap-routing-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = Self {
            dir,
            wall: ap::watch::epoch("2026-11-23T00:20:00Z").unwrap(),
            policy: None,
            objects: BTreeMap::new(),
            requests: BTreeMap::new(),
            log: vec![],
            calls: vec![],
            refuse: vec![],
            fail_search: false,
        };
        f.put(
            "intent",
            "gadget",
            json!({"status": "confirmed", "outcome": "o", "first_user": "u"}),
        );
        f
    }
    /// The policy file with the member `routing` set to `routing` (none for null), or the bytes `raw`.
    fn policy(&mut self, routing: Value) {
        let mut p = json!({"models": {"allowed": [HAIKU, SONNET, OPUS], "never": []},
            "accounts": {"claude": ["/scratch/a"]}});
        if !routing.is_null() {
            p["routing"] = routing;
        }
        self.raw_policy(p.to_string().as_bytes());
    }
    fn raw_policy(&mut self, bytes: &[u8]) {
        let path = self.dir.join("policy.json");
        std::fs::write(&path, bytes).unwrap();
        self.policy = Some(path);
    }
    fn put(&mut self, kind: &str, key: &str, fields: Value) {
        self.objects.insert((kind.into(), key.into()), fields);
    }
    fn obj(&self, kind: &str, key: &str) -> &Value {
        &self.objects[&(kind.to_owned(), key.to_owned())]
    }
    fn finding(&mut self, key: &str, state: &str) {
        self.put(
            "finding",
            key,
            json!({"detector": "ap-routing", "kind": "incident", "severity": "act", "state": state,
                "summary": "s", "first_seen_at": "2026-11-01T00:00:00Z", "last_seen_at": "2026-11-01T00:00:00Z"}),
        );
    }
    fn names(&self) -> Vec<String> {
        self.calls
            .iter()
            .map(|(a, b)| match a[0].as_str() {
                "action" => format!("apply {}", b["action_type"].as_str().unwrap()),
                "search" => format!("search {}", b["object_type"].as_str().unwrap()),
                "get" => format!("get {}", b["id"]["object_type"].as_str().unwrap()),
                other => other.to_owned(),
            })
            .collect()
    }
    fn applies(&self) -> Vec<(String, String, Value)> {
        self.calls
            .iter()
            .filter(|(a, _)| a[0] == "action")
            .map(|(_, b)| {
                (
                    b["action_type"].as_str().unwrap().to_owned(),
                    b["request_id"].as_str().unwrap().to_owned(),
                    b["parameters"].clone(),
                )
            })
            .collect()
    }
    fn ok(&self, result: Value) -> Answer {
        reply(
            &json!({"status": "ok", "now": self.wall, "schema": {"work": {"version": 25}}, "result": result}),
        )
    }
    fn search(&self, b: &Value) -> Answer {
        if self.fail_search {
            return reply(&json!({"status": "refused", "code": "store_unreadable"}));
        }
        let kind = b["object_type"].as_str().unwrap();
        let all = b["where"]["all"].as_array().cloned().unwrap_or_default();
        let found: Vec<Value> = self
            .objects
            .iter()
            .filter(|((k, _), f)| {
                k == kind
                    && all
                        .iter()
                        .all(|c| f[c["eq"][0].as_str().unwrap()] == c["eq"][1])
            })
            .map(|((k, key), f)| json!({"id": oid(k, key), "head": {"fields": wrap(f)}}))
            .collect();
        self.ok(json!({"basis": {"qualified": true}, "objects": found, "continuation": null}))
    }
    fn get(&self, b: &Value) -> Answer {
        let id = &b["id"];
        let key = (
            id["object_type"].as_str().unwrap().to_owned(),
            id["native_key"].as_str().unwrap().to_owned(),
        );
        let Some(f) = self.objects.get(&key) else {
            return reply(&json!({"status": "refused", "code": "unavailable"}));
        };
        self.ok(json!({"objects": [{"id": id, "head": "r1", "observations": [{"revision": "r1", "fields": wrap(f)}]}]}))
    }
    fn apply(&mut self, b: &Value) -> Answer {
        let (action, rid, p) = (
            b["action_type"].as_str().unwrap().to_owned(),
            b["request_id"].as_str().unwrap().to_owned(),
            b["parameters"].clone(),
        );
        assert!(rid.len() <= 64, "{rid}");
        if let Some((before, edits)) = self.requests.get(&rid) {
            if *before != p {
                return reply(&json!({"status": "refused", "code": "request_id_reused"}));
            }
            let edits = edits.clone();
            return self.ok(json!({"outcome": "already_applied", "edits": edits}));
        }
        let now = ap::watch::utc(self.wall);
        let (kind, key) = match action.as_str() {
            "add_unit" | "amend_unit" => {
                assert_eq!(b["role"], "lead");
                let key = p["unit"]["native_key"].as_str().unwrap().to_owned();
                let mut set = p.clone();
                set.as_object_mut().unwrap().remove("unit");
                let unit = self
                    .objects
                    .entry(("unit".into(), key.clone()))
                    .or_insert_with(|| json!({"state": "draft", "added_at": now}));
                for (n, v) in set.as_object().unwrap() {
                    unit[n] = v.clone();
                }
                unit["amended_at"] = json!(now);
                ("unit", key)
            }
            "open_finding" | "see_finding_again" | "reopen_finding" | "close_finding" => {
                // The pass claims `measure` as the agent `ap-measure` (specs/ap/roles.md R3).
                assert_eq!(
                    (b["agent"].as_str(), b["role"].as_str()),
                    (Some("ap-measure"), Some("measure"))
                );
                let key = p["finding"]["native_key"].as_str().unwrap().to_owned();
                assert!(key.starts_with("routing-bar-"), "{key}");
                if self.refuse.contains(&key) {
                    return reply(&json!({"status": "refused", "code": "invalid_parameter"}));
                }
                let state = self
                    .objects
                    .get(&("finding".into(), key.clone()))
                    .map(|f| f["state"].clone());
                let live = state
                    .as_ref()
                    .is_some_and(|s| s == "open" || s == "planned");
                match action.as_str() {
                    "open_finding" => {
                        assert!(state.is_none(), "{key} exists");
                        let mut f = p.clone();
                        f.as_object_mut().unwrap().remove("finding");
                        f["state"] = json!("open");
                        f["first_seen_at"] = json!(now);
                        f["last_seen_at"] = json!(now);
                        self.put("finding", &key, f);
                    }
                    "see_finding_again" | "close_finding" if !live => {
                        let failed = json!({"status": "refused", "code": "submission_criteria", "failed": ["finding_not_live"]});
                        return reply(&failed);
                    }
                    "reopen_finding" if live => {
                        let failed = json!({"status": "refused", "code": "submission_criteria", "failed": ["finding_live"]});
                        return reply(&failed);
                    }
                    _ => {
                        let f = self
                            .objects
                            .get_mut(&("finding".into(), key.clone()))
                            .unwrap();
                        match action.as_str() {
                            "close_finding" => {
                                f["state"] = json!("closed");
                                f["reason"] = p["reason"].clone();
                            }
                            _ => {
                                f["state"] = json!("open");
                                f["evidence"] = p["evidence"].clone();
                                f["last_seen_at"] = json!(now);
                            }
                        }
                    }
                }
                ("finding", key)
            }
            other => panic!("action {other}"),
        };
        let edits = json!([{"change": "update", "id": oid(kind, &key), "revision": "r1"}]);
        self.requests.insert(rid, (p, edits.clone()));
        self.ok(json!({"outcome": "applied", "edits": edits}))
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _ms: u64) -> Answer {
        assert_eq!(program, "ont");
        let mut args = args.to_vec();
        if let Some(i) = args.iter().position(|a| a == "--store") {
            assert!(args[i + 1].starts_with("/scratch/"), "a scratch store only");
            args.truncate(i);
        }
        self.calls.push((args.clone(), input.clone()));
        match args[0].as_str() {
            "search" => self.search(input),
            "get" => self.get(input),
            "action" => {
                assert_eq!(args[1], "apply");
                self.apply(input)
            }
            "log" => {
                let action = input["action_type"].clone();
                let rows: Vec<Value> = (self.log.iter())
                    .filter(|r| r["action_type"] == action)
                    .cloned()
                    .collect();
                self.ok(json!({"rows": rows, "withheld": 0, "continuation": null}))
            }
            "render" => {
                let unit = input["args"]["unit"].as_str().unwrap_or_default();
                let kind = match input["view"].as_str() {
                    Some("reviews_of_unit") => "review",
                    _ => "run",
                };
                let keys: Vec<&String> = (self.objects.iter())
                    .filter(|((k, _), f)| k == kind && f["unit"]["native_key"] == unit)
                    .map(|((_, key), _)| key)
                    .collect();
                let table: String = keys.iter().map(|k| format!("| {k} |\n")).collect();
                reply(
                    &json!({"status": "ok", "now": self.wall, "truncated": false, "markdown": format!("| Key |\n| --- |\n{table}"), "rows": keys.len()}),
                )
            }
            other => panic!("ont {other}"),
        }
    }
    fn environment(&self) -> Vec<(String, String)> {
        vec![("HOME".into(), "/scratch/home".into())]
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.wall)
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
}

fn op(f: &mut Fake, words: &[&str], r: Value) -> Result<Value, Box<Refusal>> {
    let mut argv: Vec<OsString> = words.iter().map(OsString::from).collect();
    argv.extend(["--store", "/scratch/store.sqlite"].map(OsString::from));
    let call = arguments(&argv).unwrap();
    execute(f, &call, &r).map(|out| serde_json::from_str(&out).unwrap())
}
fn tier(model: &str) -> Value {
    json!({"runtime": "claude", "model": model, "effort": "high"})
}
/// The request of `add` of a unit of `kind` (none for null) with the two tiers' models.
fn unit(key: &str, kind: &str, implementer: &str, verifier: &str) -> Value {
    let mut r = json!({"request_id": format!("a-{key}"), "agent": "lead", "session": "s-lead",
        "unit": key, "lane": "objective", "class": "plain", "intent": "gadget",
        "objective": "The gadget list holds every gadget once.", "use": "The Lead lists the gadgets.",
        "acceptance": [{"check": "test -f list.txt", "kind": "fails_on_base"}],
        "write_set": ["src/list/**"], "implementer": tier(implementer), "verifier": tier(verifier)});
    if !kind.is_empty() {
        r["kind"] = json!(kind);
    }
    r
}
fn refusal(e: &Refusal) -> (String, String, Value) {
    (e.code.clone(), e.field.clone(), e.detail.clone())
}
fn pair(model: &str) -> Value {
    json!(["claude", model])
}

// spec: specs/ap/routing.md#R3
// spec: specs/ap/routing.md#R5
// spec: specs/ap/routing.md#R14
#[test]
fn add_tier_below_map_names_the_rows_lowest_tier_by_kind_and_size() {
    let mut f = Fake::new();
    f.policy(map());
    // The real run of the page: a unit of kind schema with a sonnet implementer.
    let e = op(&mut f, &["add"], unit("u-schema", "schema", SONNET, OPUS)).unwrap_err();
    assert_eq!(
        refusal(&e),
        (
            "tier_below_map".into(),
            "implementer".into(),
            json!({"lowest": pair(OPUS)})
        )
    );
    // Nothing applied, one search of findings and no function: the check costs one search.
    assert_eq!(f.names(), ["search finding"]);
    assert_eq!(
        f.calls[0].1["where"]["all"],
        json!([{"eq": ["detector", "ap-routing"]}])
    );
    // The verifier's tier, by the row `default` of a kind the map does not name.
    let e = op(&mut f, &["add"], unit("u-rust", "rust", HAIKU, HAIKU)).unwrap_err();
    assert_eq!(
        refusal(&e),
        (
            "tier_below_map".into(),
            "verifier".into(),
            json!({"lowest": pair(SONNET)})
        )
    );
    // Both fail: the implementer's is reported.
    let e = op(&mut f, &["add"], unit("u-both", "schema", HAIKU, HAIKU)).unwrap_err();
    assert_eq!(
        (e.field.as_str(), &e.detail),
        ("implementer", &json!({"lowest": pair(OPUS)}))
    );
    // The size: large from `large_lines` on.
    let mut large = unit("u-large", "rust", HAIKU, SONNET);
    large["estimate_lines"] = json!(400);
    let e = op(&mut f, &["add"], large.clone()).unwrap_err();
    assert_eq!(
        refusal(&e),
        (
            "tier_below_map".into(),
            "implementer".into(),
            json!({"lowest": pair(SONNET)})
        )
    );
    large["estimate_lines"] = json!(399);
    let a = op(&mut f, &["add"], large).unwrap();
    assert_eq!(a["state"], "draft");
    // At or above the row passes.
    let a = op(&mut f, &["add"], unit("u-ok", "schema", OPUS, OPUS)).unwrap();
    assert_eq!(a["outcome"], "applied");
    assert!(
        !f.names().iter().any(|n| n == "log" || n == "render"),
        "no function is called"
    );
}

// spec: specs/ap/routing.md#R1
// spec: specs/ap/routing.md#R5
#[test]
fn add_tier_not_in_order_names_the_order() {
    let mut f = Fake::new();
    f.policy(map());
    let order = json!({"order": [pair(HAIKU), pair(SONNET), pair(OPUS)]});
    let mut r = unit("u-codex", "rust", HAIKU, SONNET);
    r["implementer"] = json!({"runtime": "codex", "model": OPUS, "effort": "high"});
    let e = op(&mut f, &["add"], r).unwrap_err();
    assert_eq!(
        refusal(&e),
        (
            "tier_not_in_order".into(),
            "implementer".into(),
            order.clone()
        )
    );
    let e = op(
        &mut f,
        &["add"],
        unit("u-other", "rust", HAIKU, "claude-other"),
    )
    .unwrap_err();
    assert_eq!(
        refusal(&e),
        ("tier_not_in_order".into(), "verifier".into(), order)
    );
    assert!(f.applies().is_empty());
}

// spec: specs/ap/routing.md#R4
// spec: specs/ap/routing.md#R5
// spec: specs/ap/routing.md#R6
#[test]
fn add_tier_over_bar_names_the_finding_and_the_lowest_tier_without_a_bar() {
    let mut f = Fake::new();
    f.policy(map());
    f.finding(&bar_key("claude", SONNET, "rust"), "open");
    let e = op(&mut f, &["add"], unit("u1", "rust", SONNET, SONNET)).unwrap_err();
    assert_eq!(
        refusal(&e),
        (
            "tier_over_bar".into(),
            "implementer".into(),
            json!({"finding": "routing-bar-claude-claude-sonnet-5-5-rust", "lowest": pair(HAIKU)})
        )
    );
    // The bar holds the implementer only, and only on its kind.
    op(&mut f, &["add"], unit("u2", "rust", HAIKU, SONNET)).unwrap();
    op(&mut f, &["add"], unit("u3", "script", SONNET, SONNET)).unwrap();
    // A planned finding is a bar too; the lowest is the first pair from the row's on without one.
    f.finding(&bar_key("claude", HAIKU, "rust"), "planned");
    let e = op(&mut f, &["add"], unit("u4", "rust", HAIKU, SONNET)).unwrap_err();
    assert_eq!(
        e.detail,
        json!({"finding": "routing-bar-claude-claude-haiku-5-5-rust", "lowest": pair(OPUS)})
    );
    f.finding(&bar_key("claude", OPUS, "rust"), "open");
    let e = op(&mut f, &["add"], unit("u5", "rust", OPUS, SONNET)).unwrap_err();
    assert_eq!(e.detail["lowest"], Value::Null);
    // A closed or dismissed finding is no bar (the Lead lifted it), and neither is a finding of another detector.
    f.finding(&bar_key("claude", OPUS, "rust"), "dismissed");
    op(&mut f, &["add"], unit("u6", "rust", OPUS, SONNET)).unwrap();
    f.finding(&bar_key("claude", OPUS, "rust"), "closed");
    op(&mut f, &["add"], unit("u7", "rust", OPUS, SONNET)).unwrap();
    f.finding(&bar_key("claude", OPUS, "rust"), "open");
    f.objects
        .get_mut(&("finding".into(), bar_key("claude", OPUS, "rust")))
        .unwrap()["detector"] = json!("watch:x");
    op(&mut f, &["add"], unit("u8", "rust", OPUS, SONNET)).unwrap();
    // A search that fails refuses the op, nothing applied.
    f.fail_search = true;
    let before = f.applies().len();
    let e = op(&mut f, &["add"], unit("u9", "rust", OPUS, SONNET)).unwrap_err();
    assert_eq!(e.code, "store_unreadable");
    assert_eq!(f.applies().len(), before);
}

// spec: specs/ap/routing.md#R3
#[test]
fn amend_tests_the_unit_as_the_amendment_leaves_it() {
    let mut f = Fake::new();
    // Added before the map existed.
    f.policy(Value::Null);
    op(&mut f, &["add"], unit("u-old", "schema", SONNET, SONNET)).unwrap();
    f.policy(map());
    let notes = json!({"request_id": "m1", "agent": "lead", "session": "s-lead", "unit": "u-old", "notes": "More."});
    let e = op(&mut f, &["amend"], notes).unwrap_err();
    assert_eq!(
        refusal(&e),
        (
            "tier_below_map".into(),
            "implementer".into(),
            json!({"lowest": pair(OPUS)})
        )
    );
    let tiers = json!({"request_id": "m2", "agent": "lead", "session": "s-lead", "unit": "u-old",
        "implementer": tier(OPUS), "verifier": tier(OPUS)});
    let a = op(&mut f, &["amend"], tiers).unwrap();
    assert_eq!(a["outcome"], "applied");
    assert_eq!(f.obj("unit", "u-old")["implementer"]["model"], OPUS);
    // The kind the amendment sets is the one tested.
    let kind = json!({"request_id": "m3", "agent": "lead", "session": "s-lead", "unit": "u-old",
        "kind": "rust", "implementer": tier(HAIKU), "verifier": tier(SONNET)});
    op(&mut f, &["amend"], kind).unwrap();
}

// spec: specs/ap/routing.md#R3
// spec: specs/ap/routing.md#R14
#[test]
fn without_the_routing_member_nothing_is_refused_or_searched() {
    let mut f = Fake::new();
    // No policy file at all.
    op(&mut f, &["add"], unit("u1", "schema", HAIKU, HAIKU)).unwrap();
    // A file without the member.
    f.policy(Value::Null);
    op(&mut f, &["add"], unit("u2", "schema", HAIKU, HAIKU)).unwrap();
    assert!(
        !f.names().iter().any(|n| n.starts_with("search")),
        "{:?}",
        f.names()
    );
    // With the member, a unit without a kind is not checked and searches nothing.
    f.policy(map());
    op(&mut f, &["add"], unit("u3", "", HAIKU, HAIKU)).unwrap();
    assert!(
        !f.names().iter().any(|n| n.starts_with("search")),
        "{:?}",
        f.names()
    );
    // A policy file that cannot be read as one object is refused before the apply.
    f.raw_policy(b"{not json");
    let e = op(&mut f, &["add"], unit("u4", "schema", OPUS, OPUS)).unwrap_err();
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("policy_invalid", "policy")
    );
    // A member of another form too.
    f.policy(json!({"order": []}));
    let e = op(&mut f, &["add"], unit("u5", "schema", OPUS, OPUS)).unwrap_err();
    assert_eq!(
        (e.code.as_str(), e.field.as_str()),
        ("policy_invalid", "policy")
    );
    assert!(f.obj("unit", "u3").is_object());
    assert!(!f.objects.contains_key(&("unit".into(), "u4".into())));
}

// spec: specs/ap/routing.md#R5
// spec: specs/ap/routing.md#R8
#[test]
fn judge_and_the_wait_texts_of_the_dispatch_pass() {
    let r = routing::routing(&json!({"routing": map()}))
        .unwrap()
        .unwrap();
    let live = [bar_key("claude", HAIKU, "rust")].into_iter().collect();
    let u = |kind: &str, i: &str, v: &str| json!({"kind": kind, "implementer": tier(i), "verifier": tier(v)});
    let text = |u: Value| {
        routing::judge(&r, &live, &u)
            .err()
            .map(|e| routing::wait_text(&e))
    };
    assert_eq!(
        text(u("rust", HAIKU, SONNET)),
        Some("tier over bar: routing-bar-claude-claude-haiku-5-5-rust".into())
    );
    assert_eq!(
        text(u("schema", SONNET, OPUS)),
        Some("tier below map".into())
    );
    assert_eq!(
        text(u("rust", "other", SONNET)),
        Some("tier not in order".into())
    );
    assert_eq!(text(u("rust", SONNET, SONNET)), None);
    // Without a kind or a tier, nothing is tested.
    assert_eq!(
        text(json!({"implementer": tier("other"), "verifier": tier("other")})),
        None
    );
    assert_eq!(
        text(json!({"kind": "rust", "implementer": tier("other")})),
        None
    );
    assert!(routing::routing(&json!({})).unwrap().is_none());
}

// ---- The pass `ap measure routing` ----

/// The merged units of the window: `(key, kind, implementer model, stopped, repaired)`.
type Merged = (&'static str, &'static str, &'static str, bool, bool);
fn merged(f: &mut Fake, intent: &str, rows: &[Merged]) {
    for (u, kind, model, stopped, repaired) in rows {
        f.put(
            "unit",
            u,
            json!({"state": "merged", "intent": oid("intent", intent), "class": "plain", "kind": kind}),
        );
        f.log.push(json!({"action_type": "start_unit", "submitted_at": "2026-11-17T08:00:00Z", "parameters": {"unit": oid("unit", u)}}));
        f.log.push(json!({"action_type": "merge_unit", "submitted_at": "2026-11-17T09:00:00Z", "parameters": {"unit": oid("unit", u)}}));
        if *stopped {
            f.log.push(json!({"action_type": "stop_unit", "submitted_at": "2026-11-17T08:30:00Z", "parameters": {"unit": oid("unit", u), "cause": "ci_failed"}}));
        }
        f.put("review", &format!("{u}-k"), json!({"unit": oid("unit", u), "kind": "packet", "status": "closed", "verdict": "accept", "closed_at": "2026-11-17T08:40:00Z"}));
        let run = |role: &str, model: &str| json!({"role": role, "runtime": "claude", "model": model, "status": "closed", "started_at": "2026-11-17T08:00:00Z", "unit": oid("unit", u), "days": [{"day": "2026-11-17", "messages": 3}]});
        f.put("run", &format!("{u}-impl"), run("implementer", model));
        if *repaired {
            f.put("run", &format!("{u}-fix"), run("fixer", OPUS));
        }
    }
}
fn bar_store(f: &mut Fake) {
    f.policy(map());
    merged(
        f,
        "gadget",
        &[
            // haiku on schema: 2 of 3, above the bar, no finding yet: opened.
            ("h1", "schema", HAIKU, true, false),
            ("h2", "schema", HAIKU, false, true),
            ("h3", "schema", HAIKU, false, false),
            // sonnet on rust: 1 of 4, under the bar, its finding open: closed.
            ("s1", "rust", SONNET, false, false),
            ("s2", "rust", SONNET, false, false),
            ("s3", "rust", SONNET, false, false),
            ("s4", "rust", SONNET, true, true),
            // opus on script: 2 of 3, its finding closed: reopened.
            ("o1", "script", OPUS, true, false),
            ("o2", "script", OPUS, true, false),
            ("o3", "script", OPUS, false, false),
            // opus on typescript: 3 of 3, its finding open: seen again.
            ("t1", "typescript", OPUS, true, false),
            ("t2", "typescript", OPUS, true, false),
            ("t3", "typescript", OPUS, true, false),
            // haiku on config: 3 of 3, its finding dismissed: nothing.
            ("c1", "config", HAIKU, true, false),
            ("c2", "config", HAIKU, true, false),
            ("c3", "config", HAIKU, true, false),
        ],
    );
    // A confirmed intent of its own: its sums add to the first's (sonnet on removal, 2 + 1 merged, 3 stopped).
    f.put("intent", "widget", json!({"status": "confirmed"}));
    merged(
        f,
        "widget",
        &[
            ("w1", "removal", SONNET, true, false),
            ("w2", "removal", SONNET, true, false),
        ],
    );
    merged(f, "gadget", &[("w3", "removal", SONNET, true, false)]);
    // An intent that is not confirmed is not asked: its units would bar haiku on import.
    f.put("intent", "closed", json!({"status": "closed"}));
    merged(
        f,
        "closed",
        &[
            ("x1", "import", HAIKU, true, false),
            ("x2", "import", HAIKU, true, false),
            ("x3", "import", HAIKU, true, false),
        ],
    );
    f.finding(&bar_key("claude", SONNET, "rust"), "open");
    f.finding(&bar_key("claude", OPUS, "script"), "closed");
    f.finding(&bar_key("claude", OPUS, "typescript"), "open");
    f.finding(&bar_key("claude", HAIKU, "config"), "dismissed");
}
fn pass(f: &mut Fake, args: &[&str]) -> (Value, u8) {
    let argv: Vec<OsString> = std::iter::once("routing")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    measure::run(f, &argv).unwrap()
}

// spec: specs/ap/routing.md#R9
// spec: specs/ap/routing.md#R10
// spec: specs/ap/routing.md#R11
// spec: specs/ap/routing.md#R12
// spec: specs/ap/measure.md#R18
#[test]
fn routing_pass_opens_sees_reopens_and_closes_the_bars_of_the_window() {
    let mut f = Fake::new();
    bar_store(&mut f);
    let (line, status) = pass(&mut f, &["--store", "/scratch/store.sqlite"]);
    assert_eq!(status, 0, "{line}");
    let haiku_schema = bar_key("claude", HAIKU, "schema");
    let sonnet_removal = bar_key("claude", SONNET, "removal");
    assert_eq!(
        line,
        json!({"at": "2026-11-23T00:20:00Z", "day": "2026-11-23", "since": "2026-11-16", "until": "2026-11-22",
            "opened": [haiku_schema, sonnet_removal], "seen": [bar_key("claude", OPUS, "typescript")],
            "reopened": [bar_key("claude", OPUS, "script")], "closed": [bar_key("claude", SONNET, "rust")],
            "unchanged": 0, "failed": []})
    );
    // The opened finding: detector, kind, severity, and an evidence with the rate, the two counts and the window.
    let opened = f.obj("finding", &haiku_schema).clone();
    assert_eq!(
        (
            opened["detector"].as_str(),
            opened["kind"].as_str(),
            opened["severity"].as_str(),
            opened["state"].as_str()
        ),
        (
            Some("ap-routing"),
            Some("incident"),
            Some("act"),
            Some("open")
        )
    );
    let evidence = opened["evidence"].as_str().unwrap();
    for part in [
        "0.667",
        "2 stopped or repaired of 3 merged",
        "2026-11-16..2026-11-22",
    ] {
        assert!(evidence.contains(part), "{evidence}");
    }
    let reason = f.obj("finding", &bar_key("claude", SONNET, "rust"))["reason"]
        .as_str()
        .unwrap()
        .to_owned();
    for part in [
        "0.250",
        "1 stopped or repaired of 4 merged",
        "2026-11-16..2026-11-22",
    ] {
        assert!(reason.contains(part), "{reason}");
    }
    assert_eq!(
        f.obj("finding", &bar_key("claude", HAIKU, "config"))["state"],
        "dismissed"
    );
    assert!(
        !f.objects
            .contains_key(&("finding".into(), bar_key("claude", HAIKU, "import")))
    );
    // Only the four actions, only to keys of the routing bars; the request ids are fixed by day, action and tier.
    let applies = f.applies();
    assert_eq!(applies.len(), 5);
    for (action, rid, p) in &applies {
        assert!(
            [
                "open_finding",
                "see_finding_again",
                "reopen_finding",
                "close_finding"
            ]
            .contains(&action.as_str())
        );
        assert!(
            p["finding"]["native_key"]
                .as_str()
                .unwrap()
                .starts_with("routing-bar-")
        );
        assert!(rid.starts_with("routing-20261123-"), "{rid}");
    }
    // One search of findings, one of intents, and flow by tier once for each confirmed intent.
    let names = f.names();
    assert_eq!(names.iter().filter(|n| *n == "search finding").count(), 1);
    assert_eq!(names.iter().filter(|n| *n == "search intent").count(), 1);
    let merges = f
        .calls
        .iter()
        .filter(|(a, b)| a[0] == "log" && b["action_type"] == "merge_unit")
        .count();
    assert_eq!(merges, 2);

    // A second pass of the day repeats no action: what it already did is unchanged, the newly open are seen.
    let (again, status) = pass(&mut f, &["--store", "/scratch/store.sqlite"]);
    assert_eq!(status, 0, "{again}");
    assert_eq!(again["opened"], json!([]));
    assert_eq!(again["closed"], json!([]));
    assert_eq!(again["unchanged"], 1, "{again}");
    // The Lead dismisses a bar: the pass leaves it for good.
    f.objects
        .get_mut(&("finding".into(), haiku_schema.clone()))
        .unwrap()["state"] = json!("dismissed");
    f.wall += 86_400;
    let (later, _) = pass(&mut f, &["--store", "/scratch/store.sqlite"]);
    assert!(!later.to_string().contains(&haiku_schema), "{later}");
}

// spec: specs/ap/routing.md#R9
// spec: specs/ap/routing.md#R12
#[test]
fn routing_pass_without_the_member_reads_nothing_and_counts_a_refused_apply() {
    let mut f = Fake::new();
    bar_store(&mut f);
    f.policy(Value::Null);
    let (line, status) = pass(&mut f, &[]);
    assert_eq!(
        (status, line),
        (
            0,
            json!({"at": "2026-11-23T00:20:00Z", "day": "2026-11-23", "skipped": "no routing"})
        )
    );
    f.policy = None;
    let (line, _) = pass(&mut f, &[]);
    assert_eq!(line["skipped"], "no routing");
    assert!(f.calls.is_empty());
    // The options of kpis: a day after today, a day off the calendar, an unknown option.
    for (args, code, field) in [
        (&["--day", "2026-11-24"][..], "invalid_input", "day"),
        (&["--day", "2026-02-30"][..], "invalid_input", "day"),
        (&["--store", "relative"][..], "invalid_input", "store"),
        (&["--when"][..], "usage", "args"),
        (
            &["--day", "2026-11-20", "--day", "2026-11-20"][..],
            "usage",
            "args",
        ),
    ] {
        let argv: Vec<OsString> = std::iter::once("routing")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        let e = measure::run(&mut f, &argv).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), (code, field), "{args:?}");
        assert_eq!(measure::exit_of(&e), 2);
    }
    // A policy file of another form is refused, exit 2.
    f.raw_policy(b"[]");
    let argv = vec![OsString::from("routing")];
    let e = measure::run(&mut f, &argv).unwrap_err();
    assert_eq!((e.code, measure::exit_of(&e)), ("policy_invalid", 2));
    // A refused apply counts in `failed` and the pass goes on; exit 3.
    f.policy(map());
    f.refuse.push(bar_key("claude", HAIKU, "schema"));
    let (line, status) = pass(&mut f, &["--day", "2026-11-23"]);
    assert_eq!(status, 3);
    assert_eq!(
        line["failed"],
        json!([{"finding": bar_key("claude", HAIKU, "schema"), "action": "open_finding", "code": "invalid_parameter"}])
    );
    assert_eq!(line["closed"], json!([bar_key("claude", SONNET, "rust")]));
    // A search that fails ends the pass before any apply.
    let mut g = Fake::new();
    bar_store(&mut g);
    g.fail_search = true;
    let e = measure::run(&mut g, &[OsString::from("routing")]).unwrap_err();
    assert_eq!(measure::exit_of(&e), 3);
    assert!(g.applies().is_empty());
}

// spec: specs/ap/routing.md#R13
#[test]
fn routing_pass_runs_third_in_the_nightly_service() {
    let service = include_str!("../../../deploy/ap-measure.service");
    let steps: Vec<&str> = service
        .lines()
        .filter(|l| l.starts_with("ExecStart="))
        .collect();
    assert_eq!(
        steps,
        [
            "ExecStart=-/usr/local/bin/ap measure sessions",
            "ExecStart=-/usr/local/bin/ap measure kpis",
            "ExecStart=/usr/local/bin/ap measure routing",
        ]
    );
    // The help names the pass.
    assert!(
        measure::HELP.contains("Usage: ap measure routing [--day <YYYY-MM-DD>] [--store <path>]")
    );
}
