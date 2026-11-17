//! `ap request`: one message of the user recorded as a `request` of the store with its words, routed,
//! and the current requests of a topic listed (README "One request", `specs/ap/request.md`).
use crate::host::{Failure, Host, System, error, fail, id};
use crate::store::{self, Actor, Applied, Found, at, bad, only, shaped, sized};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Write, process::ExitCode};

/// A refusal of the call: `after` is `record` once this call recorded the request, `request` its key.
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<&'static str>,
    pub request: Option<String>,
}
const RECORD: &str = "op request_id agent session runtime topic kind reading words words_file words_from said_at route";
const KINDS: [&str; 7] = [
    "order",
    "decision",
    "preference",
    "correction",
    "stop",
    "answer",
    "delegation",
];
/// Words over these bounds (bytes, and bytes of their JSON string form) are stored as their first `CUT` bytes.
const MAX_WORDS: usize = 24_000;
const MAX_JSON: usize = 48_000;
const CUT: usize = 4_000;
/// `YYYY-MM-DDTHH:MM:SS`, with a final `Z` when `z`.
fn utc(t: &str, z: bool) -> bool {
    let form = &"dddd-dd-ddTdd:dd:ddZ"[..19 + z as usize];
    t.len() == form.len()
        && form.bytes().zip(t.bytes()).all(|(f, c)| {
            if f == b'd' {
                c.is_ascii_digit()
            } else {
                f == c
            }
        })
}
struct Words {
    text: String,
    verified: bool,
    truncated: bool,
    record: Option<String>,
    said_at: String,
}
/// The words of a session record at the second `at`: the strings of the user's lines of that second
/// (`ap row`'s rule, without the harness's lines), byte-equal ones counted once; exactly one is the words.
fn from_record(bytes: &[u8], at: &str) -> Result<String, Failure> {
    let mut found: Vec<String> = vec![];
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if ["isMeta", "isCompactSummary", "isSidechain"]
            .iter()
            .any(|k| v[*k] == true)
        {
            continue;
        }
        let said = match v["type"].as_str() {
            Some("user") => &v["message"]["content"],
            Some("queue-operation") if v["operation"] == "enqueue" => &v["content"],
            _ => continue,
        };
        let (Some(time), Some(w)) = (v["timestamp"].as_str(), said.as_str()) else {
            continue;
        };
        if time.starts_with(at) && !found.iter().any(|f| f == w) {
            found.push(w.to_owned());
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(fail("words_not_found", "words_from", json!(0))),
        n => Err(fail("words_ambiguous", "words_from", json!(n))),
    }
}
/// The words within the bound whole; over it their first `CUT` bytes, cut back to a character boundary.
fn bound(text: String) -> (String, bool) {
    let json = serde_json::to_string(&text).map_or(usize::MAX, |j| j.len() - 2);
    if text.len() <= MAX_WORDS && json <= MAX_JSON {
        return (text, false);
    }
    let mut n = CUT;
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    (text[..n].to_owned(), true)
}
/// Exactly one of `words_from`, `words` and `words_file`, never changed but by the bound.
fn words(s: &impl System, r: &Value) -> Result<Words, Failure> {
    let given: Vec<_> = ["words", "words_file", "words_from"]
        .into_iter()
        .filter(|k| r.get(*k).is_some())
        .collect();
    let [way] = given[..] else {
        return Err(bad(given.last().unwrap_or(&"words")));
    };
    let typed_at = || {
        let t = r["said_at"].as_str().filter(|t| utc(t, true));
        t.map(str::to_owned).ok_or_else(|| bad("said_at"))
    };
    let (text, record, said_at) = match way {
        "words" => (
            r[way].as_str().ok_or_else(|| bad(way))?.to_owned(),
            None,
            typed_at()?,
        ),
        "words_file" => {
            let path = r[way].as_str().filter(|p| p.starts_with('/'));
            let bytes = path.and_then(|p| s.read(p).ok()).ok_or_else(|| bad(way))?;
            let text = String::from_utf8(bytes).map_err(|_| error("words_not_utf8", way))?;
            (text, None, typed_at()?)
        }
        _ => {
            let w = &r[way];
            only(w, "record at", "words_from.")?;
            require_none(r, "said_at")?;
            let record = w["record"]
                .as_str()
                .filter(|p| p.starts_with('/') && p.len() <= 400);
            let record = record.ok_or_else(|| bad("words_from.record"))?;
            let at = w["at"]
                .as_str()
                .filter(|a| utc(a, false))
                .ok_or_else(|| bad("words_from.at"))?;
            let bytes = s.read(record).map_err(|_| bad("words_from.record"))?;
            (
                from_record(&bytes, at)?,
                Some(record.to_owned()),
                format!("{at}Z"),
            )
        }
    };
    if text.is_empty() {
        return Err(error("words_empty", way));
    }
    let (text, truncated) = bound(text);
    let verified = record.is_some();
    Ok(Words {
        text,
        verified,
        truncated,
        record,
        said_at,
    })
}
fn require_none(r: &Value, k: &str) -> Result<(), Failure> {
    r.get(k).map_or(Ok(()), |_| Err(bad(k)))
}
enum Route {
    Intent(String),
    Aside(String),
    Unit(String),
    Finding(String),
}
/// Exactly one of `intent` (an intent's key), `aside` (a reason of 1 to 600 bytes), `unit` (a unit's
/// key) and `finding` (a finding's native key) of `v`.
fn route_of(v: &Value, at: &str) -> Result<Route, Failure> {
    let whole = if at.is_empty() { "intent" } else { "route" };
    let given: Vec<_> = ["intent", "aside", "unit", "finding"]
        .into_iter()
        .filter(|k| v.get(*k).is_some())
        .collect();
    let [way] = given[..] else {
        return Err(bad(whole));
    };
    let max = if way == "aside" { 600 } else { 512 };
    let value = sized(&v[way], &format!("{at}{way}"), max)?.to_owned();
    Ok(match way {
        "intent" => Route::Intent(value),
        "aside" => Route::Aside(value),
        "unit" => Route::Unit(value),
        _ => Route::Finding(value),
    })
}
/// Before anything is applied: the intent a route names exists and is open.
fn check(s: &mut impl System, store: Option<&str>, route: &Route, f: &str) -> Result<(), Failure> {
    let Route::Intent(key) = route else {
        return Ok(());
    };
    let intent =
        at(store::get(s, store, "intent", key), f)?.ok_or_else(|| error("unknown_intent", f))?;
    match intent["status"].as_str() {
        Some("met" | "retired") => Err(error("intent_closed", f)),
        _ => Ok(()),
    }
}
fn apply_route(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: &str,
    key: &str,
    route: &Route,
) -> Result<Applied, Failure> {
    let (request, rid) = (id("request", key), format!("{rid}-route"));
    at(
        match route {
            Route::Intent(k) => store::route_request(
                s,
                store,
                actor,
                rid,
                json!({"request": request, "intent": id("intent", k)}),
            ),
            Route::Aside(why) => store::set_request_aside(
                s,
                store,
                actor,
                rid,
                json!({"request": request, "reason": why}),
            ),
            Route::Unit(k) => store::route_request_to_unit(
                s,
                store,
                actor,
                rid,
                json!({"request": request, "unit": id("unit", k)}),
            ),
            Route::Finding(k) => store::route_request_to_finding(
                s,
                store,
                actor,
                rid,
                json!({"request": request, "finding": id("finding", k)}),
            ),
        },
        "route",
    )
}
fn actor(r: &Value) -> Result<Actor<'_>, Failure> {
    let agent = shaped(r, "agent", 64, "._:-")?;
    let session = shaped(r, "session", 64, "._:-")?;
    Ok(Actor { agent, session })
}
/// The route a request had: to its intent, aside with its reason, or none.
fn inherited(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
) -> Result<Option<Route>, Failure> {
    let old = at(store::get(s, store, "request", key), "supersedes")?;
    let old = old.ok_or_else(|| error("store_answer_unreadable", "supersedes"))?;
    let text = |k: &str| {
        old[k]
            .as_str()
            .or(old[k]["native_key"].as_str())
            .map(str::to_owned)
    };
    let held = |k: &str| text(k).ok_or_else(|| error("store_answer_unreadable", "supersedes"));
    Ok(match old["route"].as_str() {
        Some("unrouted") => None,
        Some("intent") => Some(Route::Intent(held("intent")?)),
        Some("aside") => Some(Route::Aside(held("route_reason")?)),
        Some("unit") => Some(Route::Unit(held("unit")?)),
        Some("finding") => Some(Route::Finding(held("finding")?)),
        // Never read as unrouted (R24).
        _ => return Err(fail("route_unknown", "supersedes", old["route"].clone())),
    })
}
/// `record`, and `supersede` of the current request `request`.
fn record(
    s: &mut impl System,
    store: Option<&str>,
    r: &Value,
    done: &mut Option<String>,
) -> Result<Value, Failure> {
    let supersede = r["op"] == "supersede";
    only(
        r,
        &[RECORD, " request"][..1 + supersede as usize].concat(),
        "",
    )?;
    let rid = shaped(r, "request_id", 50, "._-")?;
    let actor = actor(r)?;
    let runtime = r
        .get("runtime")
        .map(|_| shaped(r, "runtime", 40, "._-"))
        .transpose()?;
    let topic = shaped(r, "topic", 120, "._:/-")?;
    let kind = r["kind"]
        .as_str()
        .filter(|k| KINDS.contains(k))
        .ok_or_else(|| bad("kind"))?;
    let reading = sized(&r["reading"], "reading", 1000)?;
    let old = supersede
        .then(|| sized(&r["request"], "request", 512))
        .transpose()?;
    let route = r
        .get("route")
        .map(|v| only(v, "intent aside unit finding", "route.").and_then(|_| route_of(v, "route.")))
        .transpose()?;
    let w = words(s, r)?;
    if let Some(route) = &route {
        check(s, store, route, "route.intent")?;
    }
    let mut p = json!({"words": w.text, "words_verified": w.verified, "truncated": w.truncated, "reading": reading,
        "kind": kind, "topic": topic, "said_at": w.said_at, "session": actor.session});
    if let Some(record) = &w.record {
        p["record"] = json!(record);
    }
    if let Some(runtime) = runtime {
        p["runtime"] = json!(runtime);
    }
    let applied = match old {
        Some(old) => {
            p["request"] = id("request", old);
            at(
                store::supersede_request(s, store, &actor, format!("{rid}-supersede"), p),
                "supersede",
            )?
        }
        None => at(
            store::record_request(s, store, &actor, format!("{rid}-record"), p),
            "record",
        )?,
    };
    let key = applied.created().and_then(|i| i["native_key"].as_str());
    let key = key
        .ok_or_else(|| error("store_answer_unreadable", "record"))?
        .to_owned();
    *done = Some(key.clone());
    let back = at(store::get(s, store, "request", &key), "request")?;
    if back.as_ref().and_then(|b| b["words"].as_str()) != Some(w.text.as_str()) {
        return Err(error("readback_differs", "words"));
    }
    let route = match (route, old) {
        (None, Some(old)) => inherited(s, store, old)?,
        (route, _) => route,
    };
    let routed = match route {
        Some(route) => Some(apply_route(s, store, &actor, rid, &key, &route)?.outcome),
        None => None,
    };
    let mut answer = json!({"request": key, "record": applied.outcome, "route": routed,
        "words_bytes": w.text.len(), "words_verified": w.verified, "truncated": w.truncated});
    if let Some(old) = old {
        answer["supersedes"] = json!(old);
    }
    Ok(answer)
}
/// `route`: the request `request` to an intent or aside.
fn route(s: &mut impl System, store: Option<&str>, r: &Value) -> Result<Value, Failure> {
    only(
        r,
        "op request_id agent session request intent aside unit finding",
        "",
    )?;
    let rid = shaped(r, "request_id", 50, "._-")?;
    let actor = actor(r)?;
    let key = sized(&r["request"], "request", 512)?;
    let route = route_of(r, "")?;
    check(s, store, &route, "intent")?;
    let applied = apply_route(s, store, &actor, rid, key, &route)?;
    Ok(json!({"request": key, "route": applied.outcome}))
}
/// `list`: the current requests of a topic, newest first, or the unrouted ones, oldest first.
fn list(s: &mut impl System, store: Option<&str>, r: &Value) -> Result<Value, Failure> {
    only(r, "op topic", "")?;
    let topic = r
        .get("topic")
        .map(|_| shaped(r, "topic", 120, "._:/-"))
        .transpose()?;
    let eq = |k: &str, v: &str| json!({"eq": [k, v]});
    let all = match topic {
        Some(t) => vec![eq("topic", t), eq("status", "current")],
        None => vec![eq("route", "unrouted"), eq("status", "current")],
    };
    let mut found = at(store::search(s, store, "request", all), "list")?;
    let order = |o: &Found| {
        (
            o.1["said_at"].as_str().unwrap_or_default().to_owned(),
            o.0.clone(),
        )
    };
    found.sort_by_key(order);
    if topic.is_some() {
        found.reverse();
    }
    let row = |(key, f): Found| {
        let aside = f["route"] == "aside";
        json!({"request": key, "topic": f["topic"], "kind": f["kind"], "said_at": f["said_at"],
            "words_verified": f["words_verified"], "truncated": f["truncated"] == true, "route": f["route"],
            "intent": f["intent"]["native_key"], "unit": f["unit"]["native_key"],
            "finding": f["finding"]["native_key"], "route_reason": if aside { f["route_reason"].clone() } else { Value::Null },
            "reading": f["reading"], "words": f["words"]})
    };
    Ok(json!({"requests": found.into_iter().map(row).collect::<Vec<_>>()}))
}
/// The lines of `list --text`: per request its first line, its reading and its words, then an empty line.
pub fn text(v: &Value) -> String {
    let mut out = String::new();
    for q in v["requests"].as_array().into_iter().flatten() {
        let g = |k: &str| q[k].as_str().unwrap_or_default();
        out.push_str(&format!("{} {} {}", g("said_at"), g("kind"), g("topic")));
        if q["words_verified"] != true {
            out.push_str(" [not verified]");
        }
        if q["truncated"] == true {
            out.push_str(" [truncated]");
        }
        out.push_str(&match g("route") {
            way @ ("intent" | "unit" | "finding") => format!(" -> {way} {}", g(way)),
            "aside" => format!(" -> aside ({})", g("route_reason")),
            other => format!(" -> {other}"),
        });
        out.push_str(&format!("\n{}\n{}\n\n", g("reading"), g("words")));
    }
    out
}
fn run(
    s: &mut impl System,
    store: Option<&str>,
    r: &Value,
    done: &mut Option<String>,
) -> Result<Value, Failure> {
    if !r.is_object() {
        return Err(bad("stdin"));
    }
    match r["op"].as_str() {
        Some("record" | "supersede") => record(s, store, r, done),
        Some("route") => route(s, store, r),
        Some("list") => list(s, store, r),
        _ => Err(bad("op")),
    }
}
fn refusal(f: Failure, done: Option<String>) -> Refusal {
    let code = store::code(&f);
    let after = done.as_ref().map(|_| match code.as_str() {
        "route_unknown" => "supersede",
        _ => "record",
    });
    Refusal {
        code,
        field: f.field,
        detail: f.detail,
        after,
        request: done,
    }
}
/// The call: the answer, or the refusal with how far the call went.
pub fn execute(s: &mut impl System, store: Option<&str>, r: &Value) -> Result<Value, Refusal> {
    let mut done = None;
    run(s, store, r, &mut done).map_err(|f| refusal(f, done))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let called = store::input(argv, "list").map_err(|f| refusal(f, None));
    let result = called.and_then(|(store, text, r)| {
        let v = execute(&mut Host, store.as_deref(), &r)?;
        Ok(if text {
            self::text(&v)
        } else {
            format!("{v}\n")
        })
    });
    let (written, status) = match result {
        Ok(out) => (std::io::stdout().write_all(out.as_bytes()), 0),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail, "after": e.after, "request": e.request}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
