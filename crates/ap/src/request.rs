//! `ap request`: one message of the user recorded as a `request` of the store with its words, routed,
//! and the current requests of a topic listed (README "One request", `specs/ap/request.md`).
use crate::host::{Failure, Host, LIMIT, System, digest, error, fail, id};
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
/// The prefixes of a harness or channel envelope (R7 (b)): a message that begins with one is no human's.
const ENVELOPES: [&str; 8] = [
    "<event>",
    "<event ",
    "<task-notification>",
    "<task-notification ",
    "<agent-message>",
    "<agent-message ",
    "<command-message>",
    "<local-command-",
];
fn envelope(text: &str) -> bool {
    ENVELOPES.iter().any(|p| text.starts_with(p))
}
/// The words of a session record at the second `at`: the strings of the user's lines of that second
/// (`specs/ap/request.md` R7 (a) to (c)), byte-equal ones counted once; exactly one is the words.
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
            Some("user") => {
                let human = match &v["origin"] {
                    Value::Null => true,
                    o => o["kind"] == "human",
                };
                let row = v["rowSource"].as_str();
                if !human || matches!(row, Some("harness" | "local" | "hook")) {
                    continue;
                }
                &v["message"]["content"]
            }
            Some("queue-operation") if v["operation"] == "enqueue" => &v["content"],
            Some("attachment")
                if v["attachment"]["type"] == "queued_command"
                    && v["attachment"]["origin"]["kind"] == "human" =>
            {
                &v["attachment"]["prompt"]
            }
            _ => continue,
        };
        let (Some(time), Some(w)) = (v["timestamp"].as_str(), said.as_str()) else {
            continue;
        };
        let by_user = v["type"] == "user";
        if (by_user || !envelope(w)) && time.starts_with(at) && !found.iter().any(|f| f == w) {
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
            if bytes.len() as u64 > LIMIT {
                return Err(error("input_too_large", way));
            }
            let text = String::from_utf8(bytes).map_err(|_| error("words_not_utf8", way))?;
            (text, None, typed_at()?)
        }
        _ => {
            let w = &r[way];
            only(w, "record at", "words_from.")?;
            require_none(r, "said_at")?;
            let record = w["record"].as_str().filter(|p| admitted(s, p));
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
/// A session record whose words count as verified (R25): `<home>/.claude/projects/` and one or more
/// components, none empty, `.` or `..`, no control character, at most 400 bytes. `<home>` is the port's
/// (the passwd entry of the effective user), not `HOME` or `CLAUDE_CONFIG_DIR`; the string is compared,
/// no link is resolved; without a home nothing is admitted.
fn admitted(s: &impl System, path: &str) -> bool {
    let Some(home) = s.home() else {
        return false;
    };
    let Some(home) = home.to_str() else {
        return false;
    };
    let dir = format!("{}/.claude/projects/", home.trim_end_matches('/'));
    path.len() <= 400
        && !path.chars().any(char::is_control)
        && path.strip_prefix(&dir).is_some_and(|rest| {
            rest.split('/')
                .all(|c| !c.is_empty() && c != "." && c != "..")
        })
}
fn require_none(r: &Value, k: &str) -> Result<(), Failure> {
    r.get(k).map_or(Ok(()), |_| Err(bad(k)))
}
/// A route: aside with its reason, or to the subject of type `kind` (`intent`, `unit`, `finding`, or `host` as an
/// inherited route) with native key `key`.
enum Route {
    To { kind: &'static str, key: String },
    Aside(String),
}
/// Exactly one of `intent` (an intent's key), `aside` (a reason of 1 to 600 bytes), `unit` (a unit's
/// key) and `finding` (a finding's native key) of `v`.
fn route_of(v: &Value, at: &str) -> Result<Route, Failure> {
    let given: Vec<_> = ["intent", "aside", "unit", "finding"]
        .into_iter()
        .filter(|k| v.get(*k).is_some())
        .collect();
    let [way] = given[..] else {
        return Err(bad("route"));
    };
    let max = if way == "aside" { 600 } else { 512 };
    let value = sized(&v[way], &format!("{at}{way}"), max)?.to_owned();
    Ok(match way {
        "aside" => Route::Aside(value),
        "intent" => Route::To {
            kind: "intent",
            key: value,
        },
        "unit" => Route::To {
            kind: "unit",
            key: value,
        },
        _ => Route::To {
            kind: "finding",
            key: value,
        },
    })
}
/// The intent a route names exists and is open (R12; R17 for an inherited route).
fn check(s: &mut impl System, store: Option<&str>, route: &Route, f: &str) -> Result<(), Failure> {
    let Route::To {
        kind: "intent",
        key,
    } = route
    else {
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
    let rid = format!("{rid}-route");
    at(
        match route {
            Route::To { kind, key: to } => {
                let subject =
                    store::subject_of(kind, to).ok_or_else(|| error("route_unknown", "route"))?;
                store::route_request(s, store, actor, rid, (key, subject))
            }
            Route::Aside(why) => store::set_request_aside(s, store, actor, rid, (key, why)),
        },
        "route",
    )
}
fn actor(r: &Value) -> Result<Actor<'_>, Failure> {
    let agent = shaped(r, "agent", 64, "._:-")?;
    let session = shaped(r, "session", 64, "._:-")?;
    Ok(Actor {
        agent,
        session,
        role: "lead",
    })
}
/// The route a request had: to its subject (a route written before version 24 names its intent, unit or
/// finding), aside with its reason, or none.
fn inherited(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
) -> Result<Option<Route>, Failure> {
    // The head only: `ont get` answers every observation of the request, which passes 1 MiB for a
    // request routed often.
    let superseded = vec![json!({"eq": ["status", "superseded"]})];
    let rows = at(store::search(s, store, "request", superseded), "supersedes")?;
    let old = rows.into_iter().find(|(k, _)| k == key);
    let (_, old) = old.ok_or_else(|| error("store_answer_unreadable", "supersedes"))?;
    let text = |k: &str| {
        old[k]
            .as_str()
            .or(old[k]["native_key"].as_str())
            .map(str::to_owned)
    };
    let held = |k: &str| text(k).ok_or_else(|| error("store_answer_unreadable", "supersedes"));
    let to = |kind: &'static str| held(kind).map(|key| Some(Route::To { kind, key }));
    Ok(match old["route"].as_str() {
        Some("unrouted") => None,
        Some("intent") => to("intent")?,
        Some("aside") => Some(Route::Aside(held("route_reason")?)),
        Some("unit") => to("unit")?,
        Some("finding") => to("finding")?,
        Some("subject") => {
            let kind = old["subject"]["object_type"].as_str().unwrap_or_default();
            let key = old["subject"]["native_key"].as_str().unwrap_or_default();
            let kind = ["intent", "unit", "finding", "host"]
                .into_iter()
                .find(|k| *k == kind);
            let kind = kind.ok_or_else(|| error("store_answer_unreadable", "supersedes"))?;
            Some(Route::To {
                kind,
                key: key.to_owned(),
            })
        }
        // Never read as unrouted (R24).
        _ => return Err(fail("route_unknown", "supersedes", old["route"].clone())),
    })
}
/// The head of the request `key` this call created, from `ont search` of the requests of its
/// `topic` and `said_at` (the head only: `ont get` answers every observation of an object, which
/// passes 1 MiB for a request routed often); no such row is `store_answer_unreadable`. Not filtered
/// by `session`: it is optional, and the engine answers a search filtered on an optional property
/// `qualified: false` once a scanned request lacks it.
fn read_back(
    s: &mut impl System,
    store: Option<&str>,
    (topic, said_at): (&str, &str),
    key: &str,
) -> Result<Value, Failure> {
    let eq = |k: &str, v: &str| json!({"eq": [k, v]});
    let all = vec![eq("topic", topic), eq("said_at", said_at)];
    let rows = at(store::search(s, store, "request", all), "request")?;
    let row = rows.into_iter().find(|(k, _)| k == key);
    Ok(row
        .ok_or_else(|| error("store_answer_unreadable", "request"))?
        .1)
}
/// The parameters of `record_request` and `supersede_request` but `request`.
fn params(
    w: &Words,
    (reading, kind, topic): (&str, &str, &str),
    session: &str,
    runtime: Option<&str>,
) -> Value {
    let mut p = json!({"words": w.text, "words_verified": w.verified, "truncated": w.truncated, "reading": reading,
        "kind": kind, "topic": topic, "said_at": w.said_at, "session": session});
    if let Some(record) = &w.record {
        p["record"] = json!(record);
    }
    if let Some(runtime) = runtime {
        p["runtime"] = json!(runtime);
    }
    p
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
    let mut p = params(&w, (reading, kind, topic), actor.session, runtime);
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
    let back = read_back(s, store, (topic, &w.said_at), &key)?;
    if back["words"].as_str() != Some(w.text.as_str()) {
        return Err(error("readback_differs", "words"));
    }
    let route = match (route, old) {
        (None, Some(old)) => {
            let route = inherited(s, store, old)?;
            // R17: the intent an inherited route names is checked as R12 does, after the supersede.
            if let Some(route) = &route {
                check(s, store, route, "route")?;
            }
            route
        }
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
        // A stored route `subject` is listed as the subject's type, its key in the member of that name.
        let subject = (f["route"] == "subject")
            .then(|| {
                (
                    f["subject"]["object_type"].as_str(),
                    &f["subject"]["native_key"],
                )
            })
            .and_then(|(t, k)| t.map(|t| (t, k)));
        let named = |member: &str| match subject {
            Some((t, k)) if t == member => k.clone(),
            Some(_) => Value::Null,
            None => f[member]["native_key"].clone(),
        };
        let route = subject.map_or_else(|| f["route"].clone(), |(t, _)| json!(t));
        json!({"request": key, "topic": f["topic"], "kind": f["kind"], "said_at": f["said_at"],
            "words_verified": f["words_verified"], "truncated": f["truncated"] == true, "route": route,
            "intent": named("intent"), "unit": named("unit"), "finding": named("finding"), "host": named("host"),
            "route_reason": if aside { f["route_reason"].clone() } else { Value::Null },
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
            way @ ("intent" | "unit" | "finding" | "host") => format!(" -> {way} {}", g(way)),
            "aside" => format!(" -> aside ({})", g("route_reason")),
            other => format!(" -> {other}"),
        });
        out.push_str(&format!("\n{}\n{}\n\n", g("reading"), g("words")));
    }
    out
}
/// The reading the hook stores: no agent has read the message yet.
const HOOK_READING: &str =
    "Recorded by the hook of the CLI; kind and reading are not yet an agent's.";
/// A hook's input (R26): one object with a string `hook_event_name` and no `op`.
pub fn is_hook(r: &Value) -> bool {
    r.get("op").is_none() && r.get("hook_event_name").is_some_and(Value::is_string)
}
/// The hook form (R26 to R29): the one line it prints, or none. It never ends as a refusal.
pub fn hook(s: &mut impl System, store: Option<&str>, r: &Value) -> Option<String> {
    let event = r["hook_event_name"].as_str()?;
    let remote = match crate::client::resolve(s, store) {
        Ok(endpoint) => endpoint,
        Err(f) => {
            return (event == "UserPromptSubmit").then(|| {
                let path = r["transcript_path"].as_str().filter(|p| admitted(s, p));
                let record = path.map_or(String::new(), |p| format!("; its session record is {p}"));
                let code = store::code(&f);
                format!(
                    "ap request: not recorded by the hook ({code}). If this message is the user's, record it \
                     with ap request{record}."
                )
            });
        }
    };
    match (event, remote) {
        ("UserPromptSubmit", Some(ep)) => {
            let mut transport = crate::client::Transport::new(s, ep);
            Some(prompt_hook(&mut transport, store, r))
        }
        ("UserPromptSubmit", None) => Some(prompt_hook(s, store, r)),
        ("Stop", Some(ep)) => {
            let mut transport = crate::client::Transport::new(s, ep);
            stop_hook(&mut transport, store, r)
        }
        ("Stop", None) => stop_hook(s, store, r),
        _ => None,
    }
}
/// `UserPromptSubmit` (R27): the prompt recorded, verified or not, or why it is not.
fn prompt_hook(s: &mut impl System, store: Option<&str>, r: &Value) -> String {
    let path = r["transcript_path"].as_str().filter(|p| admitted(s, p));
    match prompt_recorded(s, store, r, path) {
        Ok((key, Some(at))) => {
            let path = serde_json::to_string(path.unwrap_or_default()).unwrap_or_default();
            format!(
                "ap request: recorded {key} (verified, unrouted) by the hook. Do not record this message again: \
                 route it (op route, request {key}), or supersede it with your kind and reading and \
                 \"words_from\":{{\"record\":{path},\"at\":\"{at}\"}}."
            )
        }
        Ok((key, None)) => format!(
            "ap request: recorded {key} (unverified, unrouted) by the hook. Do not record this message again: \
             route it (op route, request {key}), or supersede it with your kind and reading."
        ),
        Err(code) => {
            let record = path.map_or(String::new(), |p| format!("; its session record is {p}"));
            format!(
                "ap request: not recorded by the hook ({code}). If this message is the user's, record it \
                 with ap request{record}."
            )
        }
    }
}
/// A prompt that is a slash command: its first token (up to the first ASCII whitespace byte) is `/` and
/// one or more bytes of ASCII letters, digits, `_`, `:` or `-`.
fn slash_command(prompt: &str) -> bool {
    let end = prompt
        .bytes()
        .position(|b| b.is_ascii_whitespace())
        .unwrap_or(prompt.len());
    let token = &prompt.as_bytes()[..end];
    token.len() > 1
        && token[0] == b'/'
        && token[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_:-".contains(b))
}
/// The request's key, and the second of its line when verified (None: recorded unverified at the port's
/// wall second), or the code of the end `not recorded`.
fn prompt_recorded(
    s: &mut impl System,
    store: Option<&str>,
    r: &Value,
    path: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let invalid = || "invalid_input".to_owned();
    let session = shaped(r, "session_id", 64, "._:-").map_err(|_| invalid())?;
    let path = path.ok_or_else(invalid)?;
    let prompt = r["prompt"]
        .as_str()
        .filter(|p| !p.is_empty())
        .ok_or_else(invalid)?;
    let not_found = || "words_not_found".to_owned();
    // The skip rules come before any file read or call to the store.
    if r.get("agent_id").is_some_and(|a| !a.is_null()) || envelope(prompt) || slash_command(prompt)
    {
        return Err(not_found());
    }
    let line = match s.read(path) {
        Ok(bytes) => prompt_line(&bytes, prompt).map(|at| (bytes, at)),
        Err(_) => None,
    };
    let (verified, at) = match line {
        Some((bytes, at)) => {
            match from_record(&bytes, &at) {
                Ok(w) if w == prompt => {}
                Ok(_) => return Err(not_found()),
                Err(f) => return Err(f.code.to_owned()),
            }
            (true, at)
        }
        None => {
            let secs = s
                .wall()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            (false, crate::watch::utc(secs)[..19].to_owned())
        }
    };
    let (text, truncated) = bound(prompt.to_owned());
    let w = Words {
        text,
        verified,
        truncated,
        record: verified.then(|| path.to_owned()),
        said_at: format!("{at}Z"),
    };
    let topic = hook_topic(s, r);
    let p = params(&w, (HOOK_READING, "order", &topic), session, Some("claude"));
    let rid = format!(
        "hook-{}",
        &digest(format!("{session}\n{at}").as_bytes())[..40]
    );
    let actor = Actor {
        agent: "hook",
        session,
        role: "lead",
    };
    let failed = |f: Failure| store::code(&f);
    let applied =
        store::record_request(s, store, &actor, format!("{rid}-record"), p).map_err(failed)?;
    if applied.outcome != "applied" {
        // A request that already exists is an earlier message's with the same text (or, unverified, a
        // second prompt of the same words in the same second).
        return Err(not_found());
    }
    let key = applied.created().and_then(|i| i["native_key"].as_str());
    let key = key
        .ok_or_else(|| "store_answer_unreadable".to_owned())?
        .to_owned();
    let back = read_back(s, store, (&topic, &w.said_at), &key).map_err(failed)?;
    if back["words"].as_str() != Some(w.text.as_str()) {
        return Err("readback_differs".to_owned());
    }
    Ok((key, verified.then_some(at)))
}
/// The second (`YYYY-MM-DDTHH:MM:SS`) of the prompt's line: the last line of a typed prompt (`origin.kind`
/// `human`) whose string is `prompt`, when no answer of the main thread follows it.
fn prompt_line(bytes: &[u8], prompt: &str) -> Option<String> {
    let mut found = None;
    let mut answered = false;
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let flagged = |k: &str| v[k] == true;
        match v["type"].as_str() {
            Some("assistant") if !flagged("isSidechain") => answered = true,
            Some("user")
                if !["isMeta", "isCompactSummary", "isSidechain"]
                    .into_iter()
                    .any(flagged)
                    && v["origin"]["kind"] == "human"
                    && v["message"]["content"].as_str() == Some(prompt) =>
            {
                found = Some(v["timestamp"].clone());
                answered = false;
            }
            _ => {}
        }
    }
    let time = found.filter(|_| !answered)?;
    let at = time.as_str()?.as_bytes().get(..19)?;
    let at = std::str::from_utf8(at).ok().filter(|a| utc(a, false))?;
    Some(at.to_owned())
}
/// The topic of a hook's request: the first component of `cwd` below the work root, else its last
/// component, each byte outside the topic set as `-`, at most 120 bytes; `hook` when that is empty.
fn hook_topic(s: &impl System, r: &Value) -> String {
    let Some(cwd) = r["cwd"].as_str().map(std::path::Path::new) else {
        return "hook".to_owned();
    };
    let root = s.work_root();
    let below = cwd
        .strip_prefix(&root)
        .ok()
        .and_then(|rest| match rest.components().next()? {
            std::path::Component::Normal(c) => Some(c),
            _ => None,
        });
    let name = below.or_else(|| cwd.file_name()).unwrap_or_default();
    let keep = |c: u8| c.is_ascii_alphanumeric() || b"._:/-".contains(&c);
    let mut topic: String = (name.as_encoded_bytes().iter())
        .map(|c| if keep(*c) { *c as char } else { '-' })
        .collect();
    topic.truncate(120);
    if topic.is_empty() {
        "hook".to_owned()
    } else {
        topic
    }
}
/// `Stop` (R28): the current unrouted requests of the session, said once; nothing on any failure.
fn stop_hook(s: &mut impl System, store: Option<&str>, r: &Value) -> Option<String> {
    if r["stop_hook_active"] == true {
        return None;
    }
    let session = r["session_id"].as_str()?;
    let eq = |k: &str, v: &str| json!({"eq": [k, v]});
    let all = vec![eq("route", "unrouted"), eq("status", "current")];
    let mut found: Vec<Found> = store::search(s, store, "request", all).ok()?;
    found.retain(|(_, f)| f["session"] == session);
    if found.is_empty() {
        return None;
    }
    found.sort_by_key(|o| {
        (
            o.1["said_at"].as_str().unwrap_or_default().to_owned(),
            o.0.clone(),
        )
    });
    let keys: Vec<&str> = found.iter().take(5).map(|o| o.0.as_str()).collect();
    let reason = format!(
        "ap request: {} unrouted request(s) of this session: {}. Route each (op route) or set it aside, \
         then end the turn.",
        found.len(),
        keys.join(", ")
    );
    Some(json!({"decision": "block", "reason": reason}).to_string())
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
    let result = match crate::client::resolve(s, store) {
        Ok(Some(endpoint)) => {
            let mut transport = crate::client::Transport::new(s, endpoint);
            run(&mut transport, store, r, &mut done)
        }
        Ok(None) => run(s, store, r, &mut done),
        Err(f) => Err(f),
    };
    result.map_err(|f| refusal(f, done))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let called = store::input(argv, "list").map_err(|f| refusal(f, None));
    // The hook form (R29): status 0, nothing on standard error, at most one line.
    if let Ok((store, _, r)) = &called
        && is_hook(r)
    {
        let line = hook(&mut Host, store.as_deref(), r);
        let written = line.map_or(Ok(()), |l| writeln!(std::io::stdout(), "{l}"));
        return ExitCode::from(if written.is_ok() { 0 } else { 4 });
    }
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
