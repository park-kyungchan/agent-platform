//! Every call of `ap request` to the store engine `ont`: one function per action it applies and one
//! per read. An answer `store_busy` is asked again up to five times after a short wait; nothing else is.
use crate::host::{Failure, System, error, fail, id, ont_within};
use serde_json::{Map, Value, json};
use std::time::Duration;

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
        if !reply.success || v["status"] != "ok" || !v["result"].is_object() {
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
        self.created_of("request")
    }
    /// The id of the object of `kind` the action created.
    pub fn created_of(&self, kind: &str) -> Option<&Value> {
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
    let body = json!({"action_type": action, "request_id": request_id, "agent": actor.agent,
        "session": actor.session, "parameters": parameters});
    let v = call(s, store, &["action", "apply", "--client", "ap"], &body)?;
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
/// The fields of the head of one object (`kind`, `key`) by `ont get`, each the value or null; `None`
/// when the store does not have the object (its answer `unavailable`).
pub fn get(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    key: &str,
) -> Result<Option<Value>, Failure> {
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
    Ok(Some(values(fields)))
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
