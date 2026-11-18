//! `ap status`: what is asked, planned, running, waiting and broken, read from the store alone; one
//! intent or unit in full; and what a word of the life cycle means (README "The status",
//! `specs/ap/status.md`). It writes nothing.
use crate::host::{Failure, Host, System, error};
use crate::store::{self, Found, Seen, at};
use crate::unit::drive::{self, key_form};
use crate::work;
use serde_json::{Value, json};
use std::{cmp::Reverse, ffi::OsString, io::Write, process::ExitCode};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};

/// The request of delivery on this host (its member `state`), and the directory it defaults to.
const DELIVER: &str = "/etc/agent-platform/deliver.json";
const STATE: &str = "/var/lib/agent-platform/deliver";
const DAY: u64 = 86_400;
const UNITS: [&str; 8] = [
    "stopped", "accepted", "running", "ready", "draft", "merged", "done", "dropped",
];
const SEVERITY: [&str; 3] = ["urgent", "act", "info"];
const KINDS: [&str; 8] = [
    "type",
    "action",
    "property",
    "parameter",
    "value",
    "link",
    "view",
    "criterion",
];
const MATCHES: usize = 12;
/// Entries a section of the text form holds, and the oldest requests it names.
const ENTRIES: usize = 9;
const OLDEST: usize = 5;

pub enum Call {
    Overview,
    Key(String),
    Term(String),
    /// `--wait <intent>` with its timeout in seconds.
    Wait(String, u64),
    /// `--indicators`, of one intent or of every merged unit.
    Indicators(Option<String>),
}
pub struct Args {
    pub store: Option<String>,
    pub text: bool,
    pub call: Call,
}
/// `--store <absolute path>`, `--text`, `--term <word>` and one key, each at most once; or `--wait <intent>`
/// with `--timeout <seconds>` and `--store`; or `--indicators` with an optional intent, `--text` and `--store`.
pub fn parse(argv: &[OsString]) -> Result<Args, Failure> {
    let usage = || error("usage", "args");
    let (mut store, mut text, mut term, mut key) = (None, false, None, None);
    let (mut wait, mut timeout, mut indicators) = (None, None, false);
    let plain = |k: &str| {
        !k.starts_with('-') && (1..=512).contains(&k.len()) && !k.chars().any(char::is_control)
    };
    let mut it = argv.iter().map(|a| a.to_str());
    while let Some(arg) = it.next() {
        match arg.ok_or_else(usage)? {
            "--text" if !text => text = true,
            "--store" if store.is_none() => {
                let p = it.next().flatten().filter(|p| p.starts_with('/'));
                store = Some(p.ok_or_else(usage)?.to_owned());
            }
            "--term" if term.is_none() => {
                let word = |w: &&str| {
                    let ok = |c: char| c.is_ascii_alphanumeric() || "_-.".contains(c);
                    (1..=80).contains(&w.len()) && w.chars().all(ok)
                };
                term = Some(
                    it.next()
                        .flatten()
                        .filter(word)
                        .ok_or_else(usage)?
                        .to_owned(),
                );
            }
            "--indicators" if !indicators => indicators = true,
            "--wait" if wait.is_none() => {
                wait = Some(it.next().flatten().filter(|k| plain(k)).ok_or_else(usage)?);
            }
            "--timeout" if timeout.is_none() => {
                let digits = |t: &&str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
                let n = it.next().flatten().filter(digits);
                let n = n.and_then(|t| t.parse::<u64>().ok());
                timeout = Some(n.filter(|n| (30..=86_400).contains(n)).ok_or_else(usage)?);
            }
            k if key.is_none() && plain(k) => key = Some(k.to_owned()),
            _ => return Err(usage()),
        }
    }
    let call = match (term, key, wait, timeout, indicators) {
        (None, None, None, None, false) => Call::Overview,
        (None, Some(k), None, None, false) => Call::Key(k),
        (Some(w), None, None, None, false) => Call::Term(w),
        (None, None, Some(i), t, false) if !text => Call::Wait(i.to_owned(), t.unwrap_or(14_400)),
        (None, k, None, None, true) => Call::Indicators(k),
        _ => return Err(usage()),
    };
    Ok(Args { store, text, call })
}

fn eq(k: &str, v: &str) -> Value {
    json!({"eq": [k, v]})
}
fn any(k: &str, v: &[&str]) -> Value {
    json!({"in": [k, v]})
}
/// Every object of `kind` the filter matches, over every page; a page that is not qualified is
/// `store_answer_unqualified` (from `store.rs`), and the failure names the list.
fn list(
    s: &mut impl System,
    store: Option<&str>,
    kind: &str,
    name: &str,
    all: Vec<Value>,
) -> Result<Vec<Found>, Failure> {
    at(store::search(s, store, kind, all), name)
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}
/// The native key a reference names, or null.
fn key_of(v: &Value) -> Value {
    v["native_key"].clone()
}
fn keys_of(v: &Value) -> Value {
    v.as_array()
        .into_iter()
        .flatten()
        .map(key_of)
        .collect::<Vec<_>>()
        .into()
}
fn rank(order: &[&str], v: &Value) -> usize {
    order.iter().position(|o| v == o).unwrap_or(order.len())
}
/// The first `max` bytes of `t`, cut back to a character boundary, and whether it was cut.
fn cut(t: &str, max: usize) -> (&str, bool) {
    let mut end = max.min(t.len());
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    (&t[..end], end < t.len())
}

/// Seconds since 1970 as `YYYY-MM-DDTHH:MM:SSZ`.
fn utc(secs: u64) -> String {
    let z = (secs / DAY) as i64 + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let r = secs % DAY;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        r / 3600,
        r % 3600 / 60,
        r % 60
    )
}
/// The seconds since 1970 of `YYYY-MM-DDTHH:MM:SSZ`.
fn epoch(t: &str) -> Option<u64> {
    let fixed = [
        (4, '-'),
        (7, '-'),
        (10, 'T'),
        (13, ':'),
        (16, ':'),
        (19, 'Z'),
    ];
    let shaped = t.len() == 20 && fixed.iter().all(|&(i, c)| t.as_bytes()[i] == c as u8);
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok().filter(|n| *n >= 0);
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, sec) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    if !shaped || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = y - i64::from(m <= 2);
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    u64::try_from(days * DAY as i64 + h * 3600 + mi * 60 + sec).ok()
}

fn request_row((k, f): &Found) -> Value {
    json!({"request": k, "topic": f["topic"], "kind": f["kind"], "said_at": f["said_at"],
        "words_verified": f["words_verified"]})
}
/// A unit as point R7 gives it; `ended` adds the end of a unit that is not live.
fn unit_row((k, f): &Found, ended: bool) -> Value {
    let mut o = json!({"unit": k, "intent": key_of(&f["intent"]), "lane": f["lane"], "class": f["class"],
        "state": f["state"], "repository": f["repository"], "pull_request": f["pull_request"],
        "head": f["head"], "depends_on": keys_of(&f["depends_on"])});
    match f["state"].as_str() {
        Some("ready") => o["waits_on"] = f["waits_on"].clone(),
        Some("stopped") => o["stop_cause"] = f["stop_cause"].clone(),
        _ => {}
    }
    if ended {
        (o["ended_at"], o["merged_commit"]) = (f["ended_at"].clone(), f["merged_commit"].clone());
    }
    o
}
fn finding_row((k, f): &Found) -> Value {
    json!({"finding": k, "severity": f["severity"], "kind": f["kind"], "state": f["state"],
        "summary": f["summary"], "last_seen_at": f["last_seen_at"]})
}
fn hold_row((k, f): &Found) -> Value {
    json!({"hold": k, "effect": f["effect"], "until": f["until"], "unit": key_of(&f["unit"]),
        "intent": key_of(&f["intent"]), "scope": f["scope"]})
}
fn sort_units(units: &mut [Found]) {
    units.sort_by_key(|(k, f)| (rank(&UNITS, &f["state"]), k.clone()));
}
fn sort_findings(findings: &mut [Found]) {
    let order = |(k, f): &Found| {
        (
            rank(&SEVERITY, &f["severity"]),
            Reverse(text(&f["last_seen_at"]).to_owned()),
            k.clone(),
        )
    };
    findings.sort_by_key(order);
}
fn sort_holds(holds: &mut [Found]) {
    holds.sort_by_key(|(k, f)| (text(&f["set_at"]).to_owned(), k.clone()));
}
fn active_holds(s: &mut impl System, store: Option<&str>) -> Result<Vec<Found>, Failure> {
    list(s, store, "hold", "holds", vec![eq("status", "active")])
}
fn all_units(s: &mut impl System, store: Option<&str>) -> Result<Vec<Found>, Failure> {
    list(s, store, "unit", "units", vec![any("state", &UNITS)])
}
fn open_findings(s: &mut impl System, store: Option<&str>) -> Result<Vec<Found>, Failure> {
    let all = vec![any("state", &["open", "planned"])];
    list(s, store, "finding", "findings", all)
}

/// The record of the last delivery, read through the host: the path from the request of delivery.
fn delivery(s: &impl System) -> Option<Value> {
    let state = s.read(DELIVER).ok();
    let state = state.and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let state = state.and_then(|v| {
        v["state"]
            .as_str()
            .filter(|p| p.starts_with('/'))
            .map(str::to_owned)
    });
    let path = format!(
        "{}/last.json",
        state.as_deref().unwrap_or(STATE).trim_end_matches('/')
    );
    let record: Value = serde_json::from_slice(&s.read(&path).ok()?).ok()?;
    let at = |k: &str| record[k].clone();
    record.is_object().then(
        || json!({"at": at("at"), "end": at("end"), "tip": at("tip"), "target": at("target")}),
    )
}

fn overview(s: &mut impl System, store: Option<&str>) -> Result<Value, Failure> {
    let mut seen = Seen::default();
    let unrouted = vec![eq("status", "current"), eq("route", "unrouted")];
    let mut requests = at(
        store::search_seen(s, store, "request", unrouted, &mut seen),
        "requests",
    )?;
    let (Some(now), Some(schema)) = (seen.now, seen.schema) else {
        return Err(error("store_answer_unreadable", "now"));
    };
    requests.sort_by_key(|(k, f)| (text(&f["said_at"]).to_owned(), k.clone()));
    let status = any("status", &["draft", "assumed", "confirmed"]);
    let mut intents = list(s, store, "intent", "intents", vec![status])?;
    intents.sort_by_key(|(k, _)| k.clone());
    let mut every = all_units(s, store)?;
    sort_units(&mut every);
    let mut intent_rows = vec![];
    for (key, f) in &intents {
        let mine = vec![json!({"eq": ["intent", crate::host::id("intent", key)]})];
        let criteria = list(s, store, "criterion", "criteria", mine)?;
        let count = |state: &str| criteria.iter().filter(|c| c.1["state"] == state).count();
        let mut units = serde_json::Map::new();
        for (_, u) in every
            .iter()
            .filter(|(_, u)| u["intent"]["native_key"] == key.as_str())
        {
            let n = units.entry(text(&u["state"])).or_insert(json!(0));
            *n = json!(n.as_u64().unwrap_or(0) + 1);
        }
        let (outcome, outcome_cut) = cut(text(&f["outcome"]), 200);
        intent_rows.push(json!({"intent": key, "status": f["status"], "confirmed": store::confirmed(f),
            "lead_session": f["lead_session"], "outcome": outcome, "outcome_cut": outcome_cut,
            "criteria": {"open": count("open"), "pass": count("pass"), "fail": count("fail"), "waived": count("waived")},
            "units": units}));
    }
    let live = ["ready", "running", "accepted", "stopped"];
    let live: Vec<&Found> = every
        .iter()
        .filter(|(_, f)| live.iter().any(|l| f["state"] == *l))
        .collect();
    let mut runs = list(s, store, "run", "runs", vec![eq("status", "running")])?;
    runs.sort_by_key(|(k, f)| (text(&f["started_at"]).to_owned(), k.clone()));
    let mut findings = open_findings(s, store)?;
    sort_findings(&mut findings);
    let mut holds = active_holds(s, store)?;
    sort_holds(&mut holds);
    let delivered = delivery(s);
    let mut need = vec![];
    let mut push = |kind: &str, (k, why): (&String, &Value)| {
        need.push(json!({"kind": kind, "key": k, "why": why}))
    };
    let state = |f: &Found, v: &str| f.1["state"] == v;
    let severe = |f: &&Found, v: &str| f.1["severity"] == v && f.1["state"] == "open";
    findings
        .iter()
        .filter(|f| severe(f, "urgent"))
        .for_each(|f| push("finding", (&f.0, &f.1["summary"])));
    live.iter()
        .filter(|u| state(u, "stopped"))
        .for_each(|u| push("unit", (&u.0, &u.1["stop_cause"])));
    let wait = json!("waits for the Lead's reading or merge");
    live.iter()
        .filter(|u| state(u, "accepted"))
        .for_each(|u| push("unit", (&u.0, &wait)));
    findings
        .iter()
        .filter(|f| severe(f, "act"))
        .for_each(|f| push("finding", (&f.0, &f.1["summary"])));
    for r in requests
        .iter()
        .filter(|(_, f)| epoch(text(&f["said_at"])).is_some_and(|t| t + DAY < now))
    {
        push(
            "request",
            (
                &r.0,
                &json!(format!("unrouted since {}", text(&r.1["said_at"]))),
            ),
        );
    }
    let mut answer = json!({"at": utc(now), "schema": schema, "needs_lead": need,
        "requests_unrouted": {"count": requests.len(), "oldest": requests.iter().take(20).map(request_row).collect::<Vec<_>>()},
        "intents": intent_rows,
        "units": live.iter().map(|u| unit_row(u, false)).collect::<Vec<_>>(),
        "runs": runs.iter().map(|(k, f)| json!({"run": k, "unit": key_of(&f["unit"]), "intent": key_of(&f["intent"]),
            "role": f["role"], "runtime": f["runtime"], "model": f["model"], "started_at": f["started_at"]})).collect::<Vec<_>>(),
        "findings": findings.iter().map(finding_row).collect::<Vec<_>>(),
        "holds": holds.iter().map(hold_row).collect::<Vec<_>>(),
        "delivery": delivered});
    if delivered.is_none() {
        answer["delivery_unreadable"] = json!(true);
    }
    Ok(answer)
}

/// An intent or a unit in full; a key that names neither is `unknown_key`.
fn keyed(s: &mut impl System, store: Option<&str>, key: &str) -> Result<Value, Failure> {
    let intent = at(store::get(s, store, "intent", key), "intent")?;
    let unit = at(store::get(s, store, "unit", key), "unit")?;
    match (intent, unit) {
        (Some(i), also) => {
            let mut v = intent_in_full(s, store, key, i)?;
            if also.is_some() {
                v["also_unit"] = json!(true);
            }
            Ok(v)
        }
        (None, Some(u)) => unit_in_full(s, store, key, u),
        (None, None) => Err(error("unknown_key", "key")),
    }
}
fn intent_in_full(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    i: Value,
) -> Result<Value, Failure> {
    let mut v = store::intent_shown(s, store, key, i)?;
    let mut mine: Vec<Found> = all_units(s, store)?;
    mine.retain(|(_, u)| u["intent"]["native_key"] == key);
    sort_units(&mut mine);
    let names: Vec<&String> = mine.iter().map(|u| &u.0).collect();
    let mut findings = open_findings(s, store)?;
    let planned = |f: &Found| {
        f.1["intent"]["native_key"] == key
            || f.1["unit"]["native_key"]
                .as_str()
                .is_some_and(|u| names.iter().any(|n| *n == u))
    };
    findings.retain(|f| f.1["state"] == "open" || planned(f));
    sort_findings(&mut findings);
    let mut reviews = list(s, store, "review", "plan_reviews", vec![eq("kind", "plan")])?;
    reviews.retain(|(_, r)| r["intent"]["native_key"] == key);
    reviews.sort_by_key(|(k, r)| (Reverse(text(&r["started_at"]).to_owned()), k.clone()));
    let mut holds = active_holds(s, store)?;
    let here = |f: &Found| {
        f.1["intent"]["native_key"] == key
            || f.1["unit"]["native_key"]
                .as_str()
                .is_some_and(|u| names.iter().any(|n| *n == u))
    };
    sort_holds(&mut holds);
    holds.sort_by_key(|h| !here(h));
    v["units"] = mine.iter().map(|u| unit_row(u, true)).collect();
    v["findings"] = findings.iter().map(finding_row).collect();
    v["plan_reviews"] = reviews.iter().map(|(k, r)| json!({"review": k, "head": r["head"], "round": r["round"],
        "status": r["status"], "verdict": r["verdict"], "started_at": r["started_at"], "closed_at": r["closed_at"]})).collect();
    v["holds"] = holds
        .iter()
        .map(|h| {
            let mut o = hold_row(h);
            o["here"] = json!(here(h));
            o
        })
        .collect();
    Ok(v)
}
/// The objects a view of a unit lists, each read with `get`.
fn of_unit(
    s: &mut impl System,
    store: Option<&str>,
    (view, kind): (&str, &str),
    key: &str,
) -> Result<Vec<Found>, Failure> {
    let keys = at(store::view_keys(s, store, view, json!({"unit": key})), view)?;
    let mut found = vec![];
    for k in keys {
        let o = at(store::get(s, store, kind, &k), kind)?;
        found.push((k, o.ok_or_else(|| error("store_answer_unreadable", kind))?));
    }
    Ok(found)
}
fn unit_in_full(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    mut v: Value,
) -> Result<Value, Failure> {
    let reviews = of_unit(s, store, ("reviews_of_unit", "review"), key)?;
    let runs = of_unit(s, store, ("runs_of_unit", "run"), key)?;
    let mut holds = active_holds(s, store)?;
    holds.retain(|h| h.1["unit"]["native_key"] == key);
    sort_holds(&mut holds);
    let mut blocked = vec![];
    for dep in v["depends_on"].as_array().cloned().unwrap_or_default() {
        let name = text(&dep["native_key"]).to_owned();
        let state = at(store::get(s, store, "unit", &name), "unit")?.map(|u| u["state"].clone());
        if !state
            .as_ref()
            .is_some_and(|st| st == "merged" || st == "done")
        {
            blocked.push(json!({"unit": name, "state": state}));
        }
    }
    v["unit"] = json!(key);
    v["reviews"] = reviews.iter().map(|(k, r)| json!({"review": k, "kind": r["kind"], "head": r["head"],
        "round": r["round"], "status": r["status"], "verdict": r["verdict"], "closed_at": r["closed_at"],
        "report": r["report"]})).collect();
    v["runs"] = runs.iter().map(|(k, r)| json!({"run": k, "role": r["role"], "model": r["model"],
        "status": r["status"], "state": r["state"], "started_at": r["started_at"], "ended_at": r["ended_at"],
        "tokens": {"input": r["input_tokens"], "cache_read": r["cache_read_tokens"],
            "cache_write": r["cache_write_tokens"], "output": r["output_tokens"]}})).collect();
    v["holds"] = holds.iter().map(hold_row).collect();
    v["blocked_by"] = json!(blocked);
    Ok(v)
}

/// One thing the word may match: its kind (an index of `KINDS`), name, owner, text and the name of
/// the value type that types it.
struct Hit {
    kind: usize,
    name: String,
    of: Option<String>,
    text: String,
    typed: Option<String>,
}
/// The name of the value type a declared type is, or an array of.
fn value_type(t: &Value) -> Option<&str> {
    t["value_type"]
        .as_str()
        .or_else(|| t["array"]["value_type"].as_str())
}
fn term(s: &mut impl System, store: Option<&str>, word: &str) -> Result<Value, Failure> {
    let d = at(store::describe(s, store), "describe")?;
    let all = |v: &Value, k: &str| v[k].as_array().cloned().unwrap_or_default();
    let mut hits = vec![];
    let mut add = |kind: &str, v: &Value, of: Option<&str>, text: String| {
        let kind = KINDS.iter().position(|k| *k == kind).unwrap_or(0);
        let typed = value_type(&v["type"]).map(str::to_owned);
        hits.push(Hit {
            kind,
            name: self::text(&v["name"]).to_owned(),
            of: of.map(str::to_owned),
            text,
            typed,
        });
    };
    let described = |v: &Value| text(&v["description"]).to_owned();
    let mut enumerated = vec![];
    for t in all(&d, "object_types") {
        add("type", &t, None, described(&t));
        for p in all(&t, "properties") {
            add("property", &p, Some(text(&t["name"])), described(&p));
            if p["type"]["value_type"].is_string() {
                enumerated.push((text(&t["name"]).to_owned(), p));
            }
        }
    }
    for a in all(&d, "action_types") {
        add("action", &a, None, described(&a));
        for p in all(&a, "parameters") {
            add("parameter", &p, Some(text(&a["name"])), described(&p));
        }
        for c in all(&a, "criteria") {
            add(
                "criterion",
                &json!({"name": c["code"]}),
                Some(text(&a["name"])),
                text(&c["message"]).to_owned(),
            );
        }
    }
    for l in all(&d, "link_types") {
        let via = format!(
            "{}.{} -> {}",
            text(&l["from"]),
            text(&l["via"]),
            text(&l["to"])
        );
        add("link", &l, Some(text(&l["from"])), via);
    }
    for v in all(&d, "views") {
        let args: Vec<&str> = v["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let args = if args.is_empty() {
            "no arguments".to_owned()
        } else {
            args.join(", ")
        };
        add(
            "view",
            &v,
            None,
            format!("{} ({args})", text(&v["object_type"])),
        );
    }
    // The members belong to the value type: one `ont aggregate` per distinct value type, and a second
    // property of the type only after a `derive_forbidden` (its object type's grant gives no derive).
    let mut denied: Vec<String> = vec![];
    let mut members: Vec<(String, Vec<String>)> = vec![];
    let mut done: Vec<String> = vec![];
    for (kind, p) in &enumerated {
        let typed = value_type(&p["type"]).unwrap_or_default().to_owned();
        if denied.contains(kind) || done.contains(&typed) {
            continue;
        }
        match store::members(s, store, kind, text(&p["name"])) {
            Ok(found) => members.push((typed.clone(), found)),
            Err(f) if f.code == store::REFUSED && f.detail["code"] == "derive_forbidden" => {
                denied.push(kind.clone());
                continue;
            }
            Err(f) if f.code == store::REFUSED => {}
            Err(f) => return Err(f),
        }
        done.push(typed);
    }
    for (kind, p) in &enumerated {
        let typed = value_type(&p["type"]).unwrap_or_default();
        let found = members.iter().find(|(t, _)| t == typed).map(|(_, m)| m);
        for m in found.into_iter().flatten() {
            let of = format!("{kind}.{}", text(&p["name"]));
            add("value", &json!({"name": m}), Some(&of), described(p));
        }
    }
    let w = word.to_ascii_lowercase();
    let names = |h: &Hit| {
        [
            Some(h.name.to_ascii_lowercase()),
            h.typed.as_ref().map(|t| t.to_ascii_lowercase()),
        ]
    };
    let mut found: Vec<(bool, Hit)> = hits
        .into_iter()
        .filter_map(|h| {
            let n = names(&h);
            let n = n.iter().flatten();
            let exact = n.clone().any(|n| *n == w);
            (exact || n.into_iter().any(|n| n.contains(&w))).then_some((exact, h))
        })
        .collect();
    found.sort_by(|(a, x), (b, y)| (!a, x.kind, &x.of, &x.name).cmp(&(!b, y.kind, &y.of, &y.name)));
    let more = found.len().saturating_sub(MATCHES);
    let matches: Vec<Value> = found
        .iter()
        .take(MATCHES)
        .map(|(_, h)| json!({"kind": KINDS[h.kind], "name": h.name, "of": h.of, "text": h.text}))
        .collect();
    Ok(json!({"word": word, "matches": matches, "more": more}))
}

pub fn execute(s: &mut impl System, store: Option<&str>, call: &Call) -> Result<Value, Failure> {
    match call {
        Call::Overview => overview(s, store),
        Call::Key(key) => keyed(s, store, key),
        Call::Term(word) => term(s, store, word),
        Call::Wait(intent, timeout) => wait(s, store, intent, *timeout),
        Call::Indicators(intent) => indicators(s, store, intent.as_deref()),
    }
}

/// The sum of one token member over the runs of a role, null where every run has none.
fn token_sum(runs: &[&Found], member: &str) -> Value {
    let mut all = runs.iter().filter_map(|r| r.1[member].as_u64()).peekable();
    match all.peek() {
        Some(_) => json!(all.sum::<u64>()),
        None => Value::Null,
    }
}
/// The median of the values (the mean of the two middle ones for an even count), null for none.
fn median(mut v: Vec<u64>) -> Value {
    v.sort_unstable();
    match (v.len(), v.len() / 2) {
        (0, _) => Value::Null,
        (n, m) if !n.is_multiple_of(2) => json!(v[m]),
        (_, m) if (v[m - 1] + v[m]).is_multiple_of(2) => json!((v[m - 1] + v[m]) / 2),
        (_, m) => json!((v[m - 1] + v[m]) as f64 / 2.0),
    }
}
/// The rows of the action log that name the unit, oldest first (the log answers newest first).
fn logged_of<'a>(l: &'a store::Logged, unit: &'a str) -> impl Iterator<Item = &'a Value> {
    let named = move |r: &&Value| r["parameters"]["unit"]["native_key"] == unit;
    l.rows.iter().rev().filter(named)
}
/// The time of the first logged row that names the unit; none where the engine withheld rows.
fn first_at<'a>(l: &'a store::Logged, unit: &'a str) -> Option<&'a str> {
    let row = logged_of(l, unit).next();
    row.and_then(|r| r["submitted_at"].as_str())
        .filter(|_| l.withheld == 0)
}
/// The indicators of one merged unit, read from its log rows, reviews and runs (README "The status").
fn indicator(
    s: &mut impl System,
    store: Option<&str>,
    (key, u): &Found,
    [began, merged, stopped]: [&store::Logged; 3],
) -> Result<Value, Failure> {
    let mut reviews = of_unit(s, store, ("reviews_of_unit", "review"), key)?;
    reviews.retain(|(_, r)| {
        let said = ["accept", "repair"].iter().any(|v| r["verdict"] == *v);
        r["kind"] == "packet" && r["status"] == "closed" && said
    });
    reviews.sort_by_key(|(k, r)| (text(&r["closed_at"]).to_owned(), k.clone()));
    let runs = of_unit(s, store, ("runs_of_unit", "run"), key)?;
    let mut tokens = serde_json::Map::new();
    for role in runs
        .iter()
        .map(|r| text(&r.1["role"]))
        .collect::<BTreeSet<_>>()
    {
        let mine: Vec<&Found> = runs.iter().filter(|r| r.1["role"] == role).collect();
        tokens.insert(role.to_owned(), json!({"input": token_sum(&mine, "input_tokens"),
            "cache_read": token_sum(&mine, "cache_read_tokens"), "cache_write": token_sum(&mine, "cache_write_tokens"),
            "output": token_sum(&mine, "output_tokens")}));
    }
    // A log the engine withheld rows of reads nothing; a missing row is a member that is null.
    let (started_at, merged_at) = (first_at(began, key), first_at(merged, key));
    let minutes = (epoch(started_at.unwrap_or_default()))
        .zip(epoch(merged_at.unwrap_or_default()))
        .and_then(|(from, to)| to.checked_sub(from))
        .map(|secs| secs / 60);
    let stops = (stopped.withheld == 0).then(|| {
        logged_of(stopped, key)
            .map(|r| r["parameters"]["cause"].clone())
            .collect::<Vec<_>>()
    });
    let mut row = json!({"unit": key, "intent": key_of(&u["intent"]), "class": u["class"],
        "implementer": match u["implementer"].is_object() {
            true => json!({"runtime": u["implementer"]["runtime"], "model": u["implementer"]["model"], "effort": u["implementer"]["effort"]}),
            false => Value::Null },
        "started_at": started_at, "merged_at": merged_at, "minutes": minutes, "rounds": reviews.len(),
        "repairs": runs.iter().filter(|r| r.1["role"] == "fixer").count(),
        "first_verdict": reviews.first().map(|r| r.1["verdict"].clone()), "stops": stops, "tokens": tokens});
    let cut = [began, merged, stopped].iter().any(|l| l.withheld > 0);
    if cut || started_at.is_none() || merged_at.is_none() {
        row["log_cut"] = json!(true);
    }
    Ok(row)
}
/// `ap status --indicators`: one row for each merged unit (of the intent when one is named), from the log
/// of `start_unit`, `merge_unit` and `stop_unit`, and the unit's reviews and runs; it writes nothing.
fn indicators(
    s: &mut impl System,
    store: Option<&str>,
    intent: Option<&str>,
) -> Result<Value, Failure> {
    let mut seen = Seen::default();
    if let Some(i) = intent {
        match at(store::head_now(s, store, "intent", i), "intent")? {
            Some((_, _, now)) => seen.now = now,
            None => return Err(error("unknown_intent", "intent")),
        }
    }
    let merged = vec![eq("state", "merged")];
    let mut units = at(
        store::search_seen(s, store, "unit", merged, &mut seen),
        "units",
    )?;
    let now = seen
        .now
        .ok_or_else(|| error("store_answer_unreadable", "now"))?;
    units.retain(|(_, u)| intent.is_none_or(|i| u["intent"]["native_key"] == i));
    units.sort_by_key(|(k, _)| k.clone());
    let mut logs = [
        store::Logged::default(),
        store::Logged::default(),
        store::Logged::default(),
    ];
    if !units.is_empty() {
        for (log, action) in logs
            .iter_mut()
            .zip(["start_unit", "merge_unit", "stop_unit"])
        {
            *log = at(store::logged(s, store, action), "log")?;
        }
    }
    let [began, merged_log, stopped] = &logs;
    let mut rows = vec![];
    for unit in &units {
        rows.push(indicator(s, store, unit, [began, merged_log, stopped])?);
    }
    let numbers = |m: &str| {
        rows.iter()
            .filter_map(|r| r[m].as_u64())
            .collect::<Vec<_>>()
    };
    let verdicts: Vec<&Value> = rows
        .iter()
        .map(|r| &r["first_verdict"])
        .filter(|v| v.is_string())
        .collect();
    let share = (!verdicts.is_empty()).then(|| {
        let accepted = verdicts.iter().filter(|v| **v == "accept").count();
        (accepted as f64 / verdicts.len() as f64 * 1000.0).round() / 1000.0
    });
    let summary = json!({"count": rows.len(), "median_minutes": median(numbers("minutes")),
        "median_rounds": median(numbers("rounds")), "median_repairs": median(numbers("repairs")),
        "first_accept_share": share});
    Ok(json!({"at": utc(now), "units": rows, "summary": summary}))
}

/// One reading of the wait: the store's clock, the intent's units by key, the keys of the open `act` or
/// `urgent` findings and the units that look idle.
#[derive(Clone)]
struct Reading {
    at: u64,
    units: Vec<Found>,
    findings: BTreeSet<String>,
    idle: BTreeSet<String>,
}
/// The units directory and run-unit prefix where `unit_idle` may be given, from the policy as `ap work`
/// reads it (a scratch store is never read against the host's units directory).
fn watched(s: &impl System, store: Option<&str>) -> Result<Option<(PathBuf, String)>, Failure> {
    let policy = if store.is_some() == work::variable(s, "AP_POLICY_FILE").is_some() {
        work::policy(s)?
    } else {
        None
    };
    let w = policy.as_ref().map(work::work).transpose()?.flatten();
    let (Some(policy), Some(w)) = (policy, w) else {
        return Ok(None);
    };
    if work::scratch(&policy, store).is_err() {
        return Ok(None);
    }
    let (directory, _) = drive::units(&policy)?;
    Ok(s.units().then(|| (PathBuf::from(directory), w.prefix)))
}
/// The units of the intent that no run drives and nothing will start: a cut `running` unit, or a `ready`
/// or `accepted` one whose file `started` equals its revision, under no active hold; only a unit with a
/// repository and a key of `ap unit add`'s form.
fn idle(
    s: &mut impl System,
    store: Option<&str>,
    intent: &str,
    units: &[Found],
) -> Result<BTreeSet<String>, Failure> {
    let Some((root, prefix)) = watched(s, store)? else {
        return Ok(BTreeSet::new());
    };
    let (mut given, mut cut) = (vec![], BTreeSet::new());
    for (k, u) in units
        .iter()
        .filter(|(k, u)| key_form(k) && !text(&u["repository"]).is_empty())
    {
        match u["state"].as_str() {
            Some("running") => {
                cut.insert(k.clone());
                given.push((k.clone(), String::new()));
            }
            Some("ready" | "accepted") => {
                let head = at(store::head(s, store, "unit", k), "unit")?;
                given.extend(head.map(|(revision, _)| (k.clone(), revision)));
            }
            _ => {}
        }
    }
    let seen = work::observe(s, &root, &prefix, &given)?;
    let mut idle: BTreeSet<String> = (given.into_iter().map(|g| g.0))
        .filter(|k| !seen.in_flight.contains(k) && (cut.contains(k) || seen.started.contains(k)))
        .collect();
    if !idle.is_empty() {
        let held = active_holds(s, store)?;
        let named = |k: &String, h: &Found| {
            h.1["unit"]["native_key"] == *k || h.1["intent"]["native_key"] == intent
        };
        idle.retain(|k| !held.iter().any(|h| named(k, h)));
    }
    Ok(idle)
}
fn reading(s: &mut impl System, store: Option<&str>, intent: &str) -> Result<Reading, Failure> {
    let Some((_, _, now)) = at(store::head_now(s, store, "intent", intent), "intent")? else {
        return Err(error("unknown_intent", "intent"));
    };
    let at = now.ok_or_else(|| error("store_answer_unreadable", "now"))?;
    let mut units = all_units(s, store)?;
    units.retain(|(_, u)| u["intent"]["native_key"] == intent);
    units.sort_by_key(|(k, _)| k.clone());
    let severe = |f: &Found| f.1["state"] == "open" && f.1["severity"] != "info";
    let findings = open_findings(s, store)?
        .into_iter()
        .filter(severe)
        .map(|f| f.0)
        .collect();
    let idle = idle(s, store, intent, &units)?;
    Ok(Reading {
        at,
        units,
        findings,
        idle,
    })
}
fn answer(reason: &str, unit: Option<&String>, finding: Option<&String>, at: u64) -> Value {
    json!({"reason": reason, "unit": unit, "finding": finding, "at": utc(at)})
}
/// The first reason that holds at `r` (the order of the specification) against the first reading `b`.
fn reason(r: &Reading, b: &Reading, before: &BTreeSet<String>) -> Option<Value> {
    let was = |k: &String, state: &str, m: &str| {
        (b.units.iter().find(|u| u.0 == *k && u.1["state"] == state)).map(|u| u.1[m].clone())
    };
    let waits = |f: &Value| {
        ["contract", "effect"].iter().any(|c| f["class"] == *c) && f["approved_head"] != f["head"]
    };
    let unit = |state: &str, new: &dyn Fn(&Found) -> bool| {
        (r.units.iter().find(|u| u.1["state"] == state && new(u))).map(|u| &u.0)
    };
    let say =
        |why, unit: Option<&String>, finding: Option<&String>| answer(why, unit, finding, r.at);
    let ended = |u: &Found| {
        ["merged", "done", "dropped"]
            .iter()
            .any(|e| u.1["state"] == *e)
    };
    [
        unit("accepted", &|(k, f)| {
            waits(f) && was(k, "accepted", "head").as_ref() != Some(&f["head"])
        })
        .map(|k| say("unit_accepted", Some(k), None)),
        unit("stopped", &|(k, f)| {
            was(k, "stopped", "ended_at").as_ref() != Some(&f["ended_at"])
        })
        .map(|k| say("unit_stopped", Some(k), None)),
        (r.findings.difference(&b.findings).next()).map(|f| say("finding", None, Some(f))),
        r.units
            .iter()
            .all(ended)
            .then(|| say("all_ended", None, None)),
        (r.idle.intersection(before).next()).map(|k| say("unit_idle", Some(k), None)),
    ]
    .into_iter()
    .flatten()
    .next()
}
/// `ap status --wait`: reads every 30 seconds until a reason holds or the timeout passes; one answer. A
/// reading that fails is no reason; the third in a row ends the wait with its error.
fn wait(
    s: &mut impl System,
    store: Option<&str>,
    intent: &str,
    timeout: u64,
) -> Result<Value, Failure> {
    let began = s.now();
    let (mut first, mut before, mut failed, mut last_at, mut last) =
        (None, BTreeSet::new(), 0, None, None);
    loop {
        match reading(s, store, intent) {
            Err(f) if f.code == "unknown_intent" || failed == 2 => return Err(f),
            Err(f) => (failed, last) = (failed + 1, Some(f)),
            Ok(r) => {
                failed = 0;
                last_at = Some(r.at);
                let b = first.get_or_insert_with(|| r.clone());
                if let Some(v) = reason(&r, b, &before) {
                    return Ok(v);
                }
                before = r.idle;
            }
        }
        if s.now().duration_since(began) >= Duration::from_secs(timeout) {
            let last = || last.unwrap_or_else(|| error("store_answer_unreadable", "now"));
            return last_at
                .map(|at| answer("timeout", None, None, at))
                .ok_or_else(last);
        }
        s.pause(Duration::from_secs(30));
    }
}

/// A stored value on one line: line breaks and other control characters are spaces.
fn one(t: &str) -> String {
    t.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn count(v: &Value) -> usize {
    v.as_array().map_or(0, Vec::len)
}
fn section(out: &mut Vec<String>, heading: &str, entries: Vec<String>) {
    out.push(heading.to_owned());
    if entries.is_empty() {
        out.push("none".to_owned());
    }
    let more = entries.len().saturating_sub(ENTRIES);
    out.extend(entries.into_iter().take(ENTRIES));
    if more > 0 {
        out.push(format!("(+{more} more)"));
    }
}
fn overview_text(v: &Value) -> String {
    let g = |x: &Value, k: &str| one(x[k].as_str().unwrap_or("-"));
    let each = |k: &str, f: &dyn Fn(&Value) -> String| -> Vec<String> {
        v[k].as_array().into_iter().flatten().map(f).collect()
    };
    let mut out = vec![format!("status {} schema work@{}", g(v, "at"), v["schema"])];
    section(
        &mut out,
        "NEEDS THE LEAD",
        each("needs_lead", &|n| {
            format!("{} {}: {}", g(n, "kind"), g(n, "key"), g(n, "why"))
        }),
    );
    section(
        &mut out,
        "INTENTS",
        each("intents", &|i| {
            let by = match (i["status"].as_str(), i["confirmed"] == "by delegation") {
                (_, true) => " (by delegation)",
                (Some("assumed"), _) => " (assumed)",
                _ => "",
            };
            let c = |k: &str| i["criteria"][k].to_string();
            let units: Vec<String> = i["units"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, n)| format!("{k}={n}"))
                .collect();
            format!(
                "{} {}{by} criteria {}/{}/{} units {}",
                g(i, "intent"),
                g(i, "status"),
                c("pass"),
                c("fail"),
                c("open"),
                if units.is_empty() {
                    "none".into()
                } else {
                    units.join(" ")
                }
            )
        }),
    );
    section(
        &mut out,
        "UNITS",
        each("units", &|u| {
            let at = match u["pull_request"].as_u64() {
                Some(n) => format!("{}#{n}", g(u, "repository")),
                None => g(u, "repository"),
            };
            let more = match (
                u["state"].as_str(),
                u["waits_on"].as_str(),
                u["stop_cause"].as_str(),
            ) {
                (Some("ready"), Some(w), _) => format!(" waits on {}", one(w)),
                (Some("stopped"), _, Some(c)) => format!(" stopped: {}", one(c)),
                _ => String::new(),
            };
            format!(
                "{} {} {} {at}{more}",
                g(u, "unit"),
                g(u, "state"),
                g(u, "class")
            )
        }),
    );
    section(
        &mut out,
        "RUNNING",
        each("runs", &|r| {
            let on = r["unit"].as_str().or(r["intent"].as_str()).unwrap_or("-");
            format!(
                "{} {} {} since {} ({})",
                g(r, "role"),
                g(r, "model"),
                one(on),
                g(r, "started_at"),
                g(r, "run")
            )
        }),
    );
    section(
        &mut out,
        "FINDINGS",
        each("findings", &|f| {
            format!(
                "{} {} {} {}: {}",
                g(f, "severity"),
                g(f, "kind"),
                g(f, "state"),
                g(f, "finding"),
                g(f, "summary")
            )
        }),
    );
    section(
        &mut out,
        "HOLDS",
        each("holds", &|h| {
            let on = |k: &str| {
                h[k].as_str()
                    .map(|x| format!(" {k} {}", one(x)))
                    .unwrap_or_default()
            };
            format!(
                "{} until {}{}{}",
                g(h, "effect"),
                g(h, "until"),
                on("unit"),
                on("intent")
            )
        }),
    );
    let n = v["requests_unrouted"]["count"].as_u64().unwrap_or(0) as usize;
    out.push("REQUESTS NOT ROUTED".into());
    out.push(if n == 0 {
        "none".into()
    } else {
        format!("{n} unrouted, the oldest first")
    });
    for r in v["requests_unrouted"]["oldest"]
        .as_array()
        .into_iter()
        .flatten()
        .take(OLDEST)
    {
        out.push(format!(
            "{} {} {} {}",
            g(r, "said_at"),
            g(r, "kind"),
            g(r, "topic"),
            g(r, "request")
        ));
    }
    if n > OLDEST {
        out.push(format!("(+{} more)", n - OLDEST));
    }
    out.push("DELIVERY".into());
    out.push(match v["delivery"].is_object() {
        true => format!(
            "{} {} tip {} target {}",
            g(&v["delivery"], "at"),
            g(&v["delivery"], "end"),
            g(&v["delivery"], "tip"),
            g(&v["delivery"], "target")
        ),
        false => "unreadable".into(),
    });
    out.join("\n") + "\n"
}
/// A stored text for the block form: a control character but the line break is `?`.
fn block(t: &str) -> String {
    t.chars()
        .map(|c| if c.is_control() && c != '\n' { '?' } else { c })
        .collect()
}
/// Every member as a labelled line; a text with a line break is its label and its lines after two spaces.
fn labelled(v: &Value, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let scalar = |x: &Value| match x {
        Value::Null => Some("-".to_owned()),
        Value::String(t) if !t.contains('\n') => Some(block(t)),
        Value::Object(o) if o.contains_key("object_type") && o.contains_key("native_key") => {
            Some(format!(
                "{} {}",
                text(&x["object_type"]),
                block(text(&x["native_key"]))
            ))
        }
        Value::String(_) | Value::Array(_) | Value::Object(_) => None,
        other => Some(other.to_string()),
    };
    let members: Vec<(String, &Value)> = match v {
        Value::Object(o) => o.iter().map(|(k, x)| (k.clone(), x)).collect(),
        _ => v
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| ("-".to_owned(), x))
            .collect(),
    };
    for (k, x) in members {
        let label = if v.is_array() {
            "-".to_owned()
        } else {
            format!("{k}:")
        };
        match (scalar(x), x) {
            (Some(t), _) => out.push_str(&format!("{pad}{label} {t}\n")),
            (None, Value::String(t)) => {
                out.push_str(&format!("{pad}{label}\n"));
                t.split('\n')
                    .for_each(|l| out.push_str(&format!("{pad}  {}\n", block(l))));
            }
            (None, x) if count(x) == 0 && x.is_array() => {
                out.push_str(&format!("{pad}{label} none\n"))
            }
            (None, x) => {
                out.push_str(&format!("{pad}{label}\n"));
                labelled(x, depth + 1, out);
            }
        }
    }
}
/// One line a row of the indicators and a last line of the summary, a null member `-`.
fn indicators_text(v: &Value) -> String {
    let g = |x: &Value| match x {
        Value::Null => "-".to_owned(),
        Value::String(t) => one(t),
        other => other.to_string(),
    };
    let mut out = String::new();
    for r in v["units"].as_array().into_iter().flatten() {
        let who = &r["implementer"];
        let who = match who.is_object() {
            true => ["runtime", "model", "effort"].map(|m| g(&who[m])).join("/"),
            false => "-".to_owned(),
        };
        let stops = match r["stops"].as_array() {
            Some(s) if s.is_empty() => "none".to_owned(),
            Some(s) => s.iter().map(&g).collect::<Vec<_>>().join(","),
            None => "-".to_owned(),
        };
        let tokens: Vec<String> = (r["tokens"].as_object().into_iter().flatten())
            .map(|(role, t)| {
                let n = ["input", "cache_read", "cache_write", "output"].map(|m| g(&t[m]));
                format!("{}:{}", one(role), n.join("/"))
            })
            .collect();
        let tokens = if tokens.is_empty() {
            "-".to_owned()
        } else {
            tokens.join(",")
        };
        let cut = if r["log_cut"] == true { " log_cut" } else { "" };
        out += &format!(
            "{} {} {} {who} {} {} {} {} {} {} {stops} {tokens}{cut}\n",
            g(&r["unit"]),
            g(&r["intent"]),
            g(&r["class"]),
            g(&r["started_at"]),
            g(&r["merged_at"]),
            g(&r["minutes"]),
            g(&r["rounds"]),
            g(&r["repairs"]),
            g(&r["first_verdict"]),
        );
    }
    let m = &v["summary"];
    out + &format!(
        "summary {} units median minutes {} rounds {} repairs {} first accepted {}\n",
        g(&m["count"]),
        g(&m["median_minutes"]),
        g(&m["median_rounds"]),
        g(&m["median_repairs"]),
        g(&m["first_accept_share"]),
    )
}
/// The plain lines of an answer.
pub fn text_of(call: &Call, v: &Value) -> String {
    match call {
        Call::Indicators(_) => indicators_text(v),
        Call::Overview => overview_text(v),
        Call::Wait(..) => format!("{v}\n"),
        Call::Key(_) => {
            let mut out = String::new();
            labelled(v, 0, &mut out);
            out
        }
        Call::Term(_) => {
            let mut out = String::new();
            for m in v["matches"].as_array().into_iter().flatten() {
                let of = m["of"]
                    .as_str()
                    .map(|o| format!("{o}."))
                    .unwrap_or_default();
                let said = match text(&m["text"]) {
                    "" => "(no description)".to_owned(),
                    t => one(t),
                };
                out.push_str(&format!(
                    "{} {of}{}: {said}\n",
                    text(&m["kind"]),
                    text(&m["name"]),
                ));
            }
            match v["more"].as_u64() {
                Some(n) if n > 0 => out.push_str(&format!("(+{n} more)\n")),
                _ => {}
            }
            out
        }
    }
}

/// The exit status of a refusal: 3 for a wait's error but `unknown_intent`, else 2.
pub fn exit_code(waiting: bool, f: &Failure) -> u8 {
    2 + u8::from(waiting && f.code != "unknown_intent")
}

pub fn cli(argv: &[OsString]) -> ExitCode {
    let parsed = parse(argv);
    let waiting = parsed
        .as_ref()
        .is_ok_and(|a| matches!(a.call, Call::Wait(..)));
    let result = parsed.and_then(|a| {
        let v = execute(&mut Host, a.store.as_deref(), &a.call)?;
        Ok(if a.text {
            text_of(&a.call, &v)
        } else {
            format!("{v}\n")
        })
    });
    let (written, status) = match result {
        Ok(out) => (std::io::stdout().write_all(out.as_bytes()), 0),
        Err(f) => {
            let v =
                json!({"error": {"code": store::code(&f), "field": f.field, "detail": f.detail}});
            (writeln!(std::io::stderr(), "{v}"), exit_code(waiting, &f))
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
