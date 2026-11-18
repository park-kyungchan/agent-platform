//! Every call of `ap request`, `ap intent`, the stage runner and the unit object (`ap unit
//! add|amend|ready|drop|page|review`) to the store engine `ont`: one function per action they apply and one
//! per read. An answer `store_busy` is asked again up to five times after a short wait; nothing else is. Also the request form both verbs share: the arguments, the members and their shapes.
use crate::host::{Failure, LIMIT, System, error, fail, id, ont_within};
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
/// One call bound to 30 seconds; a `store_busy` answer is called again, at most five more times.
fn call(
    s: &mut impl System,
    store: Option<&str>,
    command: &[&str],
    body: &Value,
) -> Result<Value, Failure> {
    call_in(s, store, command, body, (RETRIES, 30_000))
}
/// One call with its own `(retries, milliseconds)`: the stage runner makes one attempt of 15 seconds.
fn call_in(
    s: &mut impl System,
    store: Option<&str>,
    command: &[&str],
    body: &Value,
    within: (usize, u64),
) -> Result<Value, Failure> {
    let v = answer_in(s, store, command, body, within)?;
    if !v["result"].is_object() {
        return Err(error("store_answer_unreadable", command[0]));
    }
    Ok(v)
}
/// One call whose answer is `ok`, with or without a `result` object (`render` has none).
fn answer_in(
    s: &mut impl System,
    store: Option<&str>,
    command: &[&str],
    body: &Value,
    (retries, ms): (usize, u64),
) -> Result<Value, Failure> {
    let ctx = json!({ "store": store });
    let mut tries = 0;
    loop {
        let reply = ont_within(s, &ctx, command, body, ms)?;
        let v: Value = serde_json::from_slice(&reply.bytes)
            .map_err(|_| error("store_answer_unreadable", command[0]))?;
        if v["code"] == "store_busy" && tries < retries {
            tries += 1;
            s.pause(WAIT);
            continue;
        }
        let refused = v["status"] == "refused" && v["code"].is_string();
        if refused {
            return Err(fail(REFUSED, command[0], v));
        }
        if !reply.success || v["status"] != "ok" {
            return Err(error("store_answer_unreadable", command[0]));
        }
        return Ok(v);
    }
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
    let mut body = body(actor, action, request_id, parameters);
    if let Some(lead) = lead {
        body["role"] = json!("lead");
        body["expected"] = json!([{"id": lead.intent, "revision": lead.revision}]);
    }
    send(s, store, &body)
}
/// The action request as it is sent, without a role and without `expected`.
pub fn body(actor: &Actor, action: &str, request_id: String, parameters: Value) -> Value {
    json!({"action_type": action, "request_id": request_id, "agent": actor.agent,
        "session": actor.session, "parameters": parameters})
}
/// One apply of a whole action request.
fn send(s: &mut impl System, store: Option<&str>, body: &Value) -> Result<Applied, Failure> {
    let v = call(s, store, &["action", "apply", "--client", "ap"], body)?;
    applied(&v)
}
fn applied(v: &Value) -> Result<Applied, Failure> {
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
/// The stage runner's own action (`specs/ap/run-object.md`): one attempt bound to 15 seconds, as the
/// agent `ap-run` with the stage's `role`.
fn once(
    s: &mut impl System,
    store: Option<&str>,
    action: &str,
    (rid, role, parameters): (&str, &str, &Value),
) -> Result<Applied, Failure> {
    let body = json!({"action_type": action, "request_id": rid, "agent": "ap-run", "role": role,
        "parameters": parameters});
    let v = call_in(
        s,
        store,
        &["action", "apply", "--client", "ap"],
        &body,
        (0, 15_000),
    )?;
    applied(&v)
}
/// Schema version 11 on: a `running` run of a stage.
pub fn start_run(
    s: &mut impl System,
    store: Option<&str>,
    call: (&str, &str, &Value),
) -> Result<Applied, Failure> {
    once(s, store, "start_run", call)
}
/// Schema version 11 on: the run closed with the stage's end.
pub fn close_run(
    s: &mut impl System,
    store: Option<&str>,
    call: (&str, &str, &Value),
) -> Result<Applied, Failure> {
    once(s, store, "close_run", call)
}
/// True only when one `ont get` of the object answers `unavailable`; any other answer or failure of
/// that read (one attempt of 15 seconds) is not an absence.
pub fn absent(s: &mut impl System, store: Option<&str>, kind: &str, key: &str) -> bool {
    let body = json!({ "id": id(kind, key) });
    let reply = call_in(s, store, &["get"], &body, (0, 15_000));
    matches!(reply, Err(f) if f.code == REFUSED && f.detail["code"] == "unavailable")
}
/// The code of a failed call: the engine's for a refusal, else the failure's own.
pub fn code_of(f: &Failure) -> String {
    match f.code == REFUSED {
        true => f.detail["code"].as_str().unwrap_or(REFUSED).to_owned(),
        false => f.code.to_owned(),
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
    add_unit,
    amend_unit,
    drop_unit,
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
    end_delegation,
    close_unit_review
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
/// Schema version 11 on: one `ont action validate` of a whole action request (`ap unit review`
/// reads the key its start will make); its answer as the engine gives it.
pub fn validate(s: &mut impl System, store: Option<&str>, body: &Value) -> Result<Value, Failure> {
    call(s, store, &["action", "validate", "--client", "ap"], body)
}
/// Schema version 11 on: a review of a unit's page or an intent's plan started, the whole request
/// (with `expected` for a page) as `validate` was given it.
pub fn start_unit_review(
    s: &mut impl System,
    store: Option<&str>,
    body: &Value,
) -> Result<Applied, Failure> {
    send(s, store, body)
}
/// `ready_unit` without a role, with the unit's id and the revision it was read at as `expected`: a
/// unit changed since that read ends `revision_conflict`.
pub fn ready_unit(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
    revision: &str,
) -> Result<Applied, Failure> {
    let mut body = body(actor, "ready_unit", rid, p);
    body["expected"] = json!([{"id": body["parameters"]["unit"], "revision": revision}]);
    send(s, store, &body)
}
/// The native keys of the rows of a view (`ont render`), read from the first column (`Key`) of the
/// table it renders; a truncated rendering, or one whose rows differ from its count, is not read.
pub fn view_keys(
    s: &mut impl System,
    store: Option<&str>,
    view: &str,
    args: Value,
) -> Result<Vec<String>, Failure> {
    let body = json!({"view": view, "args": args});
    let v = answer_in(s, store, &["render"], &body, (RETRIES, 30_000))?;
    let unreadable = || error("store_answer_unreadable", view);
    if v["truncated"] != false {
        return Err(fail(
            "store_answer_unqualified",
            view,
            v["truncated"].clone(),
        ));
    }
    let table = v["markdown"].as_str().ok_or_else(unreadable)?;
    let mut lines = table.lines().skip_while(|l| !l.starts_with("| Key |"));
    if lines.next().is_none() || lines.next().is_none_or(|l| !l.starts_with("| --- |")) {
        return Err(unreadable());
    }
    let mut keys = vec![];
    for line in lines.take_while(|l| l.starts_with("| ")) {
        let key = line[2..].split(" |").next().unwrap_or_default();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return Err(unreadable());
        }
        keys.push(key.to_owned());
    }
    match v["rows"].as_u64() {
        Some(n) if n as usize == keys.len() => Ok(keys),
        _ => Err(unreadable()),
    }
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
    Ok(head_now(s, store, kind, key)?.map(|(revision, fields, _)| (revision, fields)))
}
/// The head as `head` reads it, with the `now` of the answer (seconds since the epoch) when it has one.
pub fn head_now(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    key: &str,
) -> Result<Option<(String, Value, Option<u64>)>, Failure> {
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
    Ok(Some((head.to_owned(), values(fields), v["now"].as_u64())))
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
    let mut q = json!({"object_type": kind, "where": {"all": all}, "limit": PAGE});
    let mut found = vec![];
    loop {
        let v = call(s, store, &["search"], &q)?;
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
