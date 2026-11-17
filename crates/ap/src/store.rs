//! Every call of `ap request` and `ap intent` to the store engine `ont`: one function per action they
//! apply and one per read. An answer `store_busy` is asked again up to five times after a short wait;
//! nothing else is. Also the request form both verbs share: the arguments, the members and their shapes.
use crate::host::{Failure, LIMIT, System, error, fail, id, ont};
use serde_json::{Map, Value, json};
use std::{ffi::OsString, io::Read, time::Duration};

/// The code of a refusal of the store; the refusal object is the failure's `detail`.
pub const REFUSED: &str = "store_refused";
const RETRIES: usize = 5;
const WAIT: Duration = Duration::from_millis(200);
/// Objects asked per search page. Every answer of `ont` must fit `host::LIMIT` (1 MiB): one head is
/// at most the engine's 65,536 canonical bytes plus about 1.1 KB of field wrappers and metadata, so
/// 15 heads (about 1,000,000 bytes) fit with the envelope; a larger page fails once requests are long.
pub const PAGE: u64 = 15;

/// Who applies: the request's `agent` and `session`, with the client `ap`.
pub struct Actor<'a> {
    pub agent: &'a str,
    pub session: &'a str,
}
/// The role `lead` of an apply, with the revision of the intent it was decided on: the apply passes it
/// as `expected`, so an intent changed since (a Lead renamed) ends `revision_conflict`.
pub struct Lead {
    pub intent: Value,
    pub revision: String,
}
/// The members of the envelope of the first answer a verb read: the engine's clock (seconds since
/// 1970) and the version of the schema `work`.
#[derive(Default)]
pub struct Seen {
    pub now: Option<u64>,
    pub schema: Option<u64>,
}
/// One call; a `store_busy` answer is called again, at most five more times. The answer, parsed, with
/// whether the process succeeded.
fn exchange(
    s: &mut impl System,
    store: Option<&str>,
    command: &[&str],
    body: &Value,
) -> Result<(bool, Value), Failure> {
    let ctx = json!({ "store": store });
    let mut tries = 0;
    loop {
        let reply = ont(s, &ctx, command, body)?;
        let v: Value = serde_json::from_slice(&reply.bytes)
            .map_err(|_| error("store_answer_unreadable", command[0]))?;
        if v["code"] == "store_busy" && tries < RETRIES {
            tries += 1;
            s.pause(WAIT);
            continue;
        }
        let refused = v["status"] == "refused" && v["code"].is_string();
        if refused {
            return Err(fail(REFUSED, command[0], v));
        }
        return Ok((reply.success, v));
    }
}
fn call(
    s: &mut impl System,
    store: Option<&str>,
    command: &[&str],
    body: &Value,
) -> Result<Value, Failure> {
    call_seen(s, store, command, body, &mut Seen::default())
}
/// `call`, noting the envelope of the first answer in `seen`.
fn call_seen(
    s: &mut impl System,
    store: Option<&str>,
    command: &[&str],
    body: &Value,
    seen: &mut Seen,
) -> Result<Value, Failure> {
    let (success, v) = exchange(s, store, command, body)?;
    if !success || v["status"] != "ok" || !v["result"].is_object() {
        return Err(error("store_answer_unreadable", command[0]));
    }
    if seen.now.is_none() {
        (seen.now, seen.schema) = (v["now"].as_u64(), v["schema"]["work"]["version"].as_u64());
    }
    Ok(v)
}
/// An applied action: its outcome (`applied` or `already_applied`) and its edits.
pub struct Applied {
    pub outcome: String,
    pub edits: Vec<Value>,
}
impl Applied {
    /// The id of the request the action created.
    pub fn created(&self) -> Option<&Value> {
        self.made("request")
    }
    /// The id of the object of type `kind` the action created.
    pub fn made(&self, kind: &str) -> Option<&Value> {
        self.edits
            .iter()
            .find(|e| e["change"] == "initial" && e["id"]["object_type"] == kind)
            .map(|e| &e["id"])
    }
}
fn apply(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    action: &str,
    request_id: String,
    parameters: Value,
) -> Result<Applied, Failure> {
    apply_as(s, store, actor, action, request_id, parameters, None)
}
/// An apply; with `lead`, the role `lead` and the intent's revision as `expected`.
fn apply_as(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    action: &str,
    request_id: String,
    parameters: Value,
    lead: Option<&Lead>,
) -> Result<Applied, Failure> {
    let mut body = json!({"action_type": action, "request_id": request_id, "agent": actor.agent,
        "session": actor.session, "parameters": parameters});
    if let Some(lead) = lead {
        body["role"] = json!("lead");
        body["expected"] = json!([{"id": lead.intent, "revision": lead.revision}]);
    }
    let v = call(s, store, &["action", "apply", "--client", "ap"], &body)?;
    let outcome = v["result"]["outcome"].as_str().unwrap_or_default();
    let edits = v["result"]["edits"].as_array();
    match (outcome, edits) {
        ("applied" | "already_applied", Some(edits)) => Ok(Applied {
            outcome: outcome.to_owned(),
            edits: edits.clone(),
        }),
        _ => Err(error("store_answer_unreadable", "action")),
    }
}
pub fn record_request(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, "record_request", rid, p)
}
pub fn supersede_request(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, "supersede_request", rid, p)
}
pub fn route_request(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, "route_request", rid, p)
}
pub fn set_request_aside(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, "set_request_aside", rid, p)
}
/// Schema version 11 on: the request answered by a unit.
pub fn route_request_to_unit(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, "route_request_to_unit", rid, p)
}
/// Schema version 11 on: the request reports a finding.
pub fn route_request_to_finding(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, "route_request_to_finding", rid, p)
}
/// The actions of `ap intent` that carry no role.
macro_rules! actions {
    ($($action:ident),*) => {$(
        pub fn $action(
            s: &mut impl System,
            store: Option<&str>,
            actor: &Actor,
            rid: String,
            p: Value,
        ) -> Result<Applied, Failure> {
            apply(s, store, actor, stringify!($action), rid, p)
        }
    )*};
}
actions!(
    draft_intent,
    amend_intent,
    note_intent,
    name_lead,
    confirm_intent,
    add_criterion,
    check_criterion,
    waive_criterion,
    close_intent,
    retire_intent,
    end_delegation
);
/// The Lead proceeds on a draft: the only actions that carry the role `lead`.
pub fn assume_intent(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
    lead: &Lead,
) -> Result<Applied, Failure> {
    apply_as(s, store, actor, "assume_intent", rid, p, Some(lead))
}
pub fn confirm_intent_delegated(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
    lead: &Lead,
) -> Result<Applied, Failure> {
    apply_as(
        s,
        store,
        actor,
        "confirm_intent_delegated",
        rid,
        p,
        Some(lead),
    )
}
/// The fields of the head of one object (`kind`, `key`) by `ont get`, each the value or null; `None`
/// when the store does not have the object (its answer `unavailable`).
pub fn get(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    key: &str,
) -> Result<Option<Value>, Failure> {
    Ok(head(s, store, kind, key)?.map(|(_, fields)| fields))
}
/// The revision of the head of one object and its fields, as `get` reads them.
pub fn head(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    key: &str,
) -> Result<Option<(String, Value)>, Failure> {
    let object = id(kind, key);
    let v = match call(s, store, &["get"], &json!({ "id": object })) {
        Err(f) if f.code == REFUSED && f.detail["code"] == "unavailable" => return Ok(None),
        other => other?,
    };
    let unreadable = || error("store_answer_unreadable", kind);
    let objects = v["result"]["objects"].as_array().ok_or_else(unreadable)?;
    let [o] = objects.as_slice() else {
        return Err(unreadable());
    };
    let head = o["head"]
        .as_str()
        .filter(|_| o["id"] == object)
        .ok_or_else(unreadable)?;
    let observation = o["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|ob| ob["revision"] == head)
        .ok_or_else(unreadable)?;
    let fields = observation["fields"].as_object().ok_or_else(unreadable)?;
    Ok(Some((head.to_owned(), values(fields))))
}
/// The fields as one object, each the value or null.
fn values(fields: &Map<String, Value>) -> Value {
    let value = |v: &Value| match v["kind"] == "value" {
        true => v["value"].clone(),
        false => Value::Null,
    };
    fields.iter().map(|(k, v)| (k.clone(), value(v))).collect()
}
/// One object of a search: its native key and the fields of its head.
pub type Found = (String, Value);
/// Every object of `kind` the filter `all` matches, over every page: its native key and the fields
/// of its head. Each page must be qualified (`store_answer_unqualified`).
pub fn search(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    all: Vec<Value>,
) -> Result<Vec<Found>, Failure> {
    search_seen(s, store, kind, all, &mut Seen::default())
}
/// `search`, noting the envelope of the first answer in `seen`.
pub fn search_seen(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    all: Vec<Value>,
    seen: &mut Seen,
) -> Result<Vec<Found>, Failure> {
    let mut q = json!({"object_type": kind, "where": {"all": all}, "limit": PAGE});
    let mut found = vec![];
    loop {
        let v = call_seen(s, store, &["search"], &q, seen)?;
        let r = &v["result"];
        if r["basis"]["qualified"] != true {
            return Err(fail("store_answer_unqualified", kind, r["basis"].clone()));
        }
        for o in r["objects"]
            .as_array()
            .ok_or_else(|| error("store_answer_unreadable", kind))?
        {
            let key = o["id"]["native_key"].as_str();
            let fields = o["head"]["fields"].as_object();
            let (Some(key), Some(fields)) = (key, fields) else {
                return Err(error("store_answer_unreadable", kind));
            };
            found.push((key.to_owned(), values(fields)));
        }
        match &r["continuation"] {
            Value::Null => return Ok(found),
            c if c == &q["continuation"] => return Err(error("store_answer_unreadable", kind)),
            c => q["continuation"] = c.clone(),
        }
    }
}
/// The keys a view lists (its first column, unescaped), by one `ont render`. An answer cut at the
/// engine's bound (`truncated`) is `store_answer_unqualified`.
pub fn view_keys(
    s: &mut impl System,
    store: Option<&str>,
    view: &str,
    args: Value,
) -> Result<Vec<String>, Failure> {
    let unreadable = || error("store_answer_unreadable", view);
    let body = json!({"view": view, "args": args});
    let (success, v) = exchange(s, store, &["render"], &body)?;
    let markdown = v["markdown"]
        .as_str()
        .filter(|_| success && v["status"] == "ok");
    let markdown = markdown.ok_or_else(unreadable)?;
    if v["truncated"] != false {
        return Err(fail(
            "store_answer_unqualified",
            view,
            json!({"truncated": true}),
        ));
    }
    let rows = markdown.lines().skip(3).take_while(|l| l.starts_with("| "));
    let keys: Vec<String> = rows.filter_map(first_cell).collect();
    match v["rows"].as_u64() == Some(keys.len() as u64) && keys.iter().all(|k| !k.is_empty()) {
        true => Ok(keys),
        false => Err(unreadable()),
    }
}
/// The first cell of a row of a rendered table: `| a | b |` gives `a`; `\|` and `\\` are unescaped.
fn first_cell(row: &str) -> Option<String> {
    let mut chars = row.strip_prefix("| ")?.chars().peekable();
    let mut cell = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => cell.push(chars.next().filter(|e| matches!(e, '|' | '\\'))?),
            ' ' if chars.peek() == Some(&'|') => return Some(cell),
            c => cell.push(c),
        }
    }
    None
}
/// The installed schema as `ont describe` answers it: its `result`, read whole.
pub fn describe(s: &mut impl System, store: Option<&str>) -> Result<Value, Failure> {
    let mut v = call(s, store, &["describe"], &json!({}))?;
    Ok(v["result"].take())
}
/// The members of the enumerated property `property` of `kind`, by one `ont aggregate` that scans no
/// object (the empty `in`); none when the answer lists no group. A refusal (a type whose grant gives
/// no derive) is the caller's to skip.
pub fn members(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    property: &str,
) -> Result<Vec<String>, Failure> {
    let body = json!({"object_type": kind, "group_by": [property], "metrics": ["count"],
        "where": {"in": [property, []]}});
    let v = call(s, store, &["aggregate"], &body)?;
    let groups = v["result"]["groups"].as_array().into_iter().flatten();
    Ok(groups
        .filter_map(|g| g["key"][property].as_str().map(str::to_owned))
        .collect())
}
/// How `ap intent show` gives who confirmed an intent: `no` unless it is `confirmed` or `met`.
pub fn confirmed(v: &Value) -> &'static str {
    match (v["status"].as_str(), v["confirmed_delegated"] == true) {
        (Some("confirmed" | "met"), true) => "by delegation",
        (Some("confirmed" | "met"), false) => "by the user",
        _ => "no",
    }
}
/// The intent `key`, whose fields `v` were read, as `ap intent show` prints it: with `intent`, its
/// `criteria` (by `added_at`), the `requests` routed to it (by `said_at`) and `confirmed`.
pub fn intent_shown(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    mut v: Value,
) -> Result<Value, Failure> {
    let by = |t: &'static str| {
        move |o: &Found| (o.1[t].as_str().unwrap_or_default().to_owned(), o.0.clone())
    };
    let eq = |k: &str, v: Value| json!({"eq": [k, v]});
    let mut criteria = at(
        search(s, store, "criterion", vec![eq("intent", id("intent", key))]),
        "criteria",
    )?;
    criteria.sort_by_key(by("added_at"));
    let all = vec![eq("status", json!("current")), eq("route", json!("intent"))];
    let mut requests = at(search(s, store, "request", all), "requests")?;
    requests.retain(|(_, f)| f["intent"]["native_key"] == key);
    requests.sort_by_key(by("said_at"));
    let criterion = |(k, f): Found| {
        json!({"criterion": k, "statement": f["statement"], "check": f["check"], "baseline": f["baseline"],
            "state": f["state"], "evidence": f["evidence"], "checked_at": f["checked_at"]})
    };
    let request = |(k, f): Found| {
        json!({"request": k, "topic": f["topic"], "kind": f["kind"], "said_at": f["said_at"],
            "words_verified": f["words_verified"]})
    };
    v["intent"] = json!(key);
    v["criteria"] = criteria.into_iter().map(criterion).collect();
    v["requests"] = requests.into_iter().map(request).collect();
    v["confirmed"] = json!(confirmed(&v));
    Ok(v)
}
pub fn bad(f: &str) -> Failure {
    error("invalid_input", f)
}
/// The members of `v` are among `allowed`; another is `invalid_input` with its name after `at`.
pub fn only(v: &Value, allowed: &str, at: &str) -> Result<(), Failure> {
    let obj = v.as_object().ok_or_else(|| bad(at.trim_end_matches('.')))?;
    let other = obj.keys().find(|k| !allowed.split(' ').any(|a| a == *k));
    other.map_or(Ok(()), |k| Err(bad(&format!("{at}{k}"))))
}
/// A string of 1 to `max` bytes of ASCII letters, digits and the bytes of `extra`.
pub fn shaped<'a>(r: &'a Value, k: &str, max: usize, extra: &str) -> Result<&'a str, Failure> {
    let ok = |c: u8| c.is_ascii_alphanumeric() || extra.as_bytes().contains(&c);
    r[k].as_str()
        .filter(|v| (1..=max).contains(&v.len()) && v.bytes().all(ok))
        .ok_or_else(|| bad(k))
}
/// A string of 1 to `max` bytes; one too long says its size.
pub fn sized<'a>(v: &'a Value, f: &str, max: usize) -> Result<&'a str, Failure> {
    let t = v.as_str().filter(|t| !t.is_empty()).ok_or_else(|| bad(f))?;
    let detail = json!({"bytes": t.len(), "limit": max});
    (t.len() <= max)
        .then_some(t)
        .ok_or_else(|| fail("invalid_input", f, detail))
}
/// The failure of a step, named by the step when it came from the store.
pub fn at<T>(r: Result<T, Failure>, step: &str) -> Result<T, Failure> {
    r.map_err(|mut f| {
        if f.code == REFUSED || f.code.starts_with("store_answer") {
            f.field = step.into();
        }
        f
    })
}
/// The code a refusal says: the store's own (the first failed criterion's for `submission_criteria`),
/// else the failure's.
pub fn code(f: &Failure) -> String {
    let first = f.detail["failed"][0]
        .as_str()
        .filter(|_| f.detail["code"] == "submission_criteria");
    match f.code == REFUSED {
        true => first.or(f.detail["code"].as_str()).unwrap_or(REFUSED),
        false => f.code,
    }
    .to_owned()
}
/// `--store <path>` and `--text`, each at most once, and the request of stdin; `--text` only with the
/// `op` `text_op`.
pub fn input(argv: &[OsString], text_op: &str) -> Result<(Option<String>, bool, Value), Failure> {
    let (mut store, mut text) = (None, false);
    let mut args = argv.iter().map(|a| a.to_str());
    while let Some(arg) = args.next() {
        match (arg, &store) {
            (Some("--text"), _) if !text => text = true,
            (Some("--store"), None) => match args.next() {
                Some(Some(p)) if p.starts_with('/') => store = Some(p.to_owned()),
                _ => return Err(error("usage", "args")),
            },
            _ => return Err(error("usage", "args")),
        }
    }
    let mut bytes = vec![];
    let read = std::io::stdin().take(LIMIT + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > LIMIT {
        return Err(error("input_too_large", "stdin"));
    }
    let r: Value = serde_json::from_slice(&bytes).map_err(|_| bad("stdin"))?;
    if text && r["op"] != text_op {
        return Err(error("usage", "args"));
    }
    Ok((store, text, r))
}
