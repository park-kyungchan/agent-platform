//! Every call of `ap request`, `ap intent`, `ap status`, the stage runner and the unit object (`ap unit
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
/// The members of the envelope of the first answer a verb read: the engine's clock (seconds since
/// 1970) and the version of the schema `work`.
#[derive(Default)]
pub struct Seen {
    pub now: Option<u64>,
    pub schema: Option<u64>,
}
impl Seen {
    /// Notes the envelope of an answer, unless an earlier one was noted.
    fn note(&mut self, v: &Value) {
        if self.now.is_none() {
            (self.now, self.schema) = (v["now"].as_u64(), v["schema"]["work"]["version"].as_u64());
        }
    }
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
/// Schema version 11 on: an action of `ap unit run`, `ap unit reject` on a unit (`start_unit`,
/// `resume_unit`, `record_head`, `stop_unit`, `accept_unit`, `reopen_unit`, `merge_unit`), without a
/// role.
pub fn unit_action(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    action: &str,
    rid: String,
    p: Value,
) -> Result<Applied, Failure> {
    apply(s, store, actor, action, rid, p)
}
/// `ap unit approve`: the Lead's approval of a unit's accepted head, with the role `lead`.
pub fn approve_unit(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    p: Value,
    lead: &Lead,
) -> Result<Applied, Failure> {
    apply_as(s, store, actor, "approve_unit", rid, p, Some(lead))
}
/// The Lead proceeds on a draft: with `approve_unit`, the only actions that carry the role `lead`.
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
/// table it renders, the cell unescaped (`\|`, `\\`); a truncated rendering, or one whose rows differ from its
/// count, is not read.
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
        let key = first_cell(line).unwrap_or_default();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return Err(unreadable());
        }
        keys.push(key);
    }
    match v["rows"].as_u64() {
        Some(n) if n as usize == keys.len() => Ok(keys),
        _ => Err(unreadable()),
    }
}
/// The rows of a view (`ont render`), each a map from a column's header to its cell, unescaped (`\x`
/// gives `x`); a truncated rendering, or one whose rows differ from its count, is not read.
pub fn view_rows(
    s: &mut impl System,
    store: Option<&str>,
    view: &str,
    args: Value,
) -> Result<Vec<Map<String, Value>>, Failure> {
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
    let mut lines = table.lines().skip_while(|l| !l.starts_with("| "));
    let header = lines.next().and_then(cells).ok_or_else(unreadable)?;
    if lines.next().is_none_or(|l| !l.starts_with("| --- |")) {
        return Err(unreadable());
    }
    let mut rows = vec![];
    for line in lines.take_while(|l| l.starts_with("| ")) {
        let row = cells(line).filter(|r| r.len() == header.len());
        let row = row.ok_or_else(unreadable)?.into_iter().map(Value::from);
        rows.push(header.iter().cloned().zip(row).collect());
    }
    match v["rows"].as_u64() {
        Some(n) if n as usize == rows.len() => Ok(rows),
        _ => Err(unreadable()),
    }
}
/// The cells of a rendered row `| a | b |`, each without the one space on either side.
fn cells(row: &str) -> Option<Vec<String>> {
    let mut chars = row.strip_prefix('|')?.strip_suffix('|')?.chars();
    let (mut all, mut cell) = (vec![], String::new());
    while let Some(c) = chars.next() {
        match c {
            '\\' => cell.push(chars.next()?),
            '|' => all.push(std::mem::take(&mut cell)),
            c => cell.push(c),
        }
    }
    all.push(cell);
    let trim = |c: String| {
        c.strip_prefix(' ')
            .and_then(|c| c.strip_suffix(' '))
            .map(str::to_owned)
    };
    all.into_iter().map(trim).collect()
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
        let v = call(s, store, &["search"], &q)?;
        seen.note(&v);
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
/// Every head of `kind` the store answers, page by page (`page` objects a page), and whether every page was
/// qualified: unlike `search`, a page that is not (a head past its freshness) is read and counted
/// (`specs/ap/route.md` R17). Each page is one attempt of 1.5 seconds and a `store_busy` answer one more.
pub fn search_every(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    page: u64,
) -> Result<(Vec<Found>, bool), Failure> {
    let mut q = json!({"object_type": kind, "limit": page});
    let mut found = vec![];
    let mut qualified = true;
    loop {
        let v = call_in(s, store, &["search"], &q, (1, 1_500))?;
        let r = &v["result"];
        qualified &= r["basis"]["qualified"] == true;
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
            Value::Null => return Ok((found, qualified)),
            c if c == &q["continuation"] => return Err(error("store_answer_unreadable", kind)),
            c => q["continuation"] = c.clone(),
        }
    }
}
/// `ont index` of the named sources (`specs/ap/route.md` R13), bound to `ms`: the engine's answer whether every
/// source held (`status` `ok`) or one failed (`source_failed`, each source's own `status`, `code` and `cause`);
/// any other refusal is `store_refused`.
pub fn index(
    s: &mut impl System,
    store: Option<&str>,
    sources: &[&str],
    ms: u64,
) -> Result<Value, Failure> {
    let mut command = vec!["index"];
    for source in sources {
        command.extend(["--source", source]);
    }
    let reply = ont_within(s, &json!({ "store": store }), &command, &Value::Null, ms)?;
    let v: Value = serde_json::from_slice(&reply.bytes)
        .map_err(|_| error("store_answer_unreadable", "index"))?;
    let answered = (v["status"] == "ok" || v["status"] == "source_failed")
        && v["result"]["sources"].is_array();
    if answered {
        return Ok(v);
    }
    if v["status"] == "refused" && v["code"].is_string() {
        return Err(fail(REFUSED, "index", v));
    }
    Err(error("store_answer_unreadable", "index"))
}
/// The keys of the criteria added to the intent `key`, oldest first, as the engine's log of the action
/// `add_criterion` gives them (newest first; `added_at` is only to the second, so it cannot order the
/// criteria of one call). Every page is read; a criterion the log does not hold (a scan the engine
/// limited) is not listed.
pub fn criteria_logged(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
) -> Result<Vec<String>, Failure> {
    let mut q = json!({"action_type": "add_criterion", "outcome": "applied", "parameters": true,
        "limit": 200});
    let mut newest_first = vec![];
    loop {
        let v = call(s, store, &["log"], &q)?;
        let r = &v["result"];
        let unreadable = || error("store_answer_unreadable", "log");
        for row in r["rows"].as_array().ok_or_else(unreadable)? {
            if row["parameters"]["intent"]["native_key"] != key {
                continue;
            }
            for e in row["edits"].as_array().into_iter().flatten() {
                let made = e["change"] == "initial" && e["id"]["object_type"] == "criterion";
                if let Some(k) = e["id"]["native_key"].as_str().filter(|_| made) {
                    newest_first.push(k.to_owned());
                }
            }
        }
        match &r["continuation"] {
            Value::Null => break,
            c if c == &q["continuation"] => return Err(unreadable()),
            c => q["continuation"] = c.clone(),
        }
    }
    newest_first.reverse();
    Ok(newest_first)
}
/// The applied rows of the action log of one `action_type` (`ap status --indicators`), newest first as
/// the engine gives them, and how many rows the engine withheld in all.
#[derive(Default)]
pub struct Logged {
    pub rows: Vec<Value>,
    pub withheld: u64,
}
/// Every applied row of `action` with its parameters, over every page of 100; a continuation the engine
/// repeats, or an answer without `rows` or without a `withheld` count, is `store_answer_unreadable`.
pub fn logged(s: &mut impl System, store: Option<&str>, action: &str) -> Result<Logged, Failure> {
    let mut q =
        json!({"action_type": action, "outcome": "applied", "parameters": true, "limit": 100});
    let (mut rows, mut withheld) = (vec![], 0);
    loop {
        let v = call(s, store, &["log"], &q)?;
        let r = &v["result"];
        let unreadable = || error("store_answer_unreadable", "log");
        rows.extend(r["rows"].as_array().ok_or_else(unreadable)?.iter().cloned());
        withheld += r["withheld"].as_u64().ok_or_else(unreadable)?;
        match &r["continuation"] {
            Value::Null => return Ok(Logged { rows, withheld }),
            c if c == &q["continuation"] => return Err(unreadable()),
            c => q["continuation"] = c.clone(),
        }
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
