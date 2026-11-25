//! `ap status`: what is asked, planned, running, waiting and broken, read from the store alone; one
//! intent or unit in full; and what a word of the life cycle means (README "The status",
//! `specs/ap/status.md`). It writes nothing.
use crate::host::{Failure, Host, System, error};
use crate::store::{self, Found, Seen, at};
use crate::unit::drive::{self, key_form};
use crate::work;
use serde_json::{Value, json};
use std::{cmp::Reverse, ffi::OsString, io::Write, process::ExitCode};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, Instant},
};

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
/// The requests of an intent `ap status <intent>` names without `--requests`.
const NEWEST: usize = 5;

pub enum Call {
    Overview,
    Key(String),
    /// A key with `--requests`: an intent's every request, not the newest five.
    KeyAll(String),
    Term(String),
    /// `--wait <intent>` with its timeout in seconds.
    Wait(String, u64),
    /// `--indicators`, of one intent or of every merged unit.
    Indicators(Option<String>),
    /// `--brief <intent>`: the forty lines a Lead reads first.
    Brief(String),
    /// `--brief` without an intent, with `--directory <path>` or without: the block of each open intent of
    /// that directory, or of the working directory.
    Open(Option<String>),
    /// `--page`: the status page, `--text`'s lines then every open intent's brief, flow and cost, and the
    /// flow, the credit's spend and the capacity of all.
    Page,
}
pub struct Args {
    pub store: Option<String>,
    pub text: bool,
    pub call: Call,
}
/// `--store <absolute path>`, `--text`, `--term <word>`, one key and `--requests` (with a key only), each at
/// most once; or `--wait <intent>` with `--timeout <seconds>` and `--store`; or `--indicators` with an optional
/// intent, `--text` and `--store`; or `--brief` with an intent or with `--directory <absolute path>`, and
/// `--store`; or `--page` with `--store` alone.
pub fn parse(argv: &[OsString]) -> Result<Args, Failure> {
    let usage = || error("usage", "args");
    let (mut store, mut text, mut term, mut key) = (None, false, None, None);
    let (mut wait, mut timeout, mut indicators) = (None, None, false);
    let (mut brief, mut requests, mut page, mut directory) = (None, false, false, None);
    let plain = |k: &str| {
        !k.starts_with('-') && (1..=512).contains(&k.len()) && !k.chars().any(char::is_control)
    };
    let mut it = argv.iter().map(|a| a.to_str()).peekable();
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
            "--requests" if !requests => requests = true,
            "--page" if !page => page = true,
            "--brief" if brief.is_none() => {
                let named = it.next_if(|a| !a.is_some_and(|a| a.starts_with('-')));
                brief = Some(match named {
                    Some(a) => Some(a.filter(|k| plain(k)).ok_or_else(usage)?),
                    None => None,
                });
            }
            "--directory" if directory.is_none() => {
                let absolute = |d: &&str| d.starts_with('/') && !d.chars().any(char::is_control);
                directory = Some(it.next().flatten().filter(absolute).ok_or_else(usage)?);
            }
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
    if directory.is_some() && brief.is_none() {
        return Err(usage());
    }
    let call = match (term, key, wait, timeout, indicators, brief, requests, page) {
        (None, None, None, None, false, None, false, false) => Call::Overview,
        (None, Some(k), None, None, false, None, false, false) => Call::Key(k),
        (None, Some(k), None, None, false, None, true, false) => Call::KeyAll(k),
        (Some(w), None, None, None, false, None, false, false) => Call::Term(w),
        (None, None, Some(i), t, false, None, false, false) if !text => {
            Call::Wait(i.to_owned(), t.unwrap_or(14_400))
        }
        (None, k, None, None, true, None, false, false) => Call::Indicators(k),
        (None, None, None, None, false, Some(Some(i)), false, false)
            if !text && directory.is_none() =>
        {
            Call::Brief(i.to_owned())
        }
        (None, None, None, None, false, Some(None), false, false) if !text => {
            Call::Open(directory.map(str::to_owned))
        }
        (None, None, None, None, false, None, false, true) if !text => Call::Page,
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
        "wave": f["wave"], "state": f["state"], "repository": f["repository"], "pull_request": f["pull_request"],
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
/// What a hold is about: the type and the native key of its `subject` (R10).
fn subject_of(f: &Value) -> Option<(&str, &str)> {
    let subject = &f["subject"];
    Some((
        subject["object_type"].as_str()?,
        subject["native_key"].as_str()?,
    ))
}
/// Whether the subject of the hold `f` is the unit or the intent `key` of that `kind`.
fn names_of(f: &Value, kind: &str, key: &str) -> bool {
    subject_of(f) == Some((kind, key))
}
fn hold_row((k, f): &Found) -> Value {
    let subject = subject_of(f);
    let named = |kind: &str| subject.filter(|s| s.0 == kind).map(|s| s.1);
    json!({"hold": k, "effect": f["effect"], "until": f["until"],
        "subject": subject.map(|(t, k)| json!({"type": t, "key": k})),
        "unit": named("unit"), "intent": named("intent"), "scope": f["scope"]})
}
/// ` w<wave>` after a unit's class for a unit that has a wave, else nothing (R17, R45).
fn wave(w: &Value) -> String {
    w.as_i64().map_or(String::new(), |n| format!(" w{n}"))
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

/// The file of `ap` that a delivery record names as its `target` when the call delivered `ap`, not
/// the engine (`specs/ap/work.md` R26).
pub(crate) const AP_TARGET: &str = "/usr/local/bin/ap";
/// What this host's delivery files hold: the request of delivery (one JSON object, or none when it
/// cannot be read), the state directory it names (the default without one), and the record of the
/// last delivery (one JSON object, or none). The delivery line reads it, and the dispatch pass for a
/// unit with `depends_delivered` (`specs/ap/work.md` R26).
#[derive(Clone, Debug)]
pub(crate) struct DeliveryFiles {
    pub request: Option<Value>,
    pub state: String,
    pub record: Option<Value>,
}
/// The two files read through the host: the request, then the record at the path it names.
pub(crate) fn delivery_files(s: &impl System) -> DeliveryFiles {
    let request = s.read(DELIVER).ok();
    let request = request.and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let request = request.filter(Value::is_object);
    let state = request.as_ref().and_then(|v| {
        v["state"]
            .as_str()
            .filter(|p| p.starts_with('/'))
            .map(str::to_owned)
    });
    let state = state
        .as_deref()
        .unwrap_or(STATE)
        .trim_end_matches('/')
        .to_owned();
    let path = format!("{state}/last.json");
    let record = s.read(&path).ok();
    let record = record.and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    DeliveryFiles {
        request,
        state,
        record: record.filter(Value::is_object),
    }
}
/// The record of the last delivery, read through the host: the path from the request of delivery.
fn delivery(s: &impl System) -> Option<Value> {
    let record = delivery_files(s).record?;
    let at = |k: &str| record[k].clone();
    Some(json!({"at": at("at"), "end": at("end"), "tip": at("tip"), "target": at("target")}))
}

fn overview(s: &mut impl System, store: Option<&str>, remote: bool) -> Result<Value, Failure> {
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
    let delivered = if remote { None } else { delivery(s) };
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
/// With `every` an intent keeps all its requests, else the newest `NEWEST` and their count.
fn keyed(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    every: bool,
) -> Result<Value, Failure> {
    let intent = at(store::get(s, store, "intent", key), "intent")?;
    let unit = at(store::get(s, store, "unit", key), "unit")?;
    match (intent, unit) {
        (Some(i), also) => {
            let mut v = intent_in_full(s, store, key, i)?;
            newest_requests(&mut v, every);
            if also.is_some() {
                v["also_unit"] = json!(true);
            }
            Ok(v)
        }
        (None, Some(u)) => unit_in_full(s, store, key, u),
        (None, None) => Err(error("unknown_key", "key")),
    }
}
/// The member `requests` of an intent keeps its newest `NEWEST` rows (they come oldest first) unless `every`;
/// `requests_count` is the count of all of them.
fn newest_requests(v: &mut Value, every: bool) {
    let Some(rows) = v["requests"].as_array_mut() else {
        return;
    };
    let all = rows.len();
    if !every {
        rows.drain(..all.saturating_sub(NEWEST));
    }
    v["requests_count"] = json!(all);
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
        names_of(&f.1, "intent", key)
            || subject_of(&f.1).is_some_and(|(t, u)| t == "unit" && names.iter().any(|n| *n == u))
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
    holds.retain(|h| names_of(&h.1, "unit", key));
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

/// The lines of the brief, and the bytes of one with its line break.
const BRIEF_LINES: usize = 40;
const BRIEF_BYTES: usize = 120;
/// The lines each section of a block may use; they sum to `BRIEF_LINES` (R44, R45).
const CAP_STATUS: usize = 2;
const CAP_NEXT: usize = 3;
const CAP_CRITERIA: usize = 4;
const CAP_UNITS: usize = 18;
const CAP_NEEDS: usize = 6;
const CAP_HOLDS: usize = 6;
const CAP_DELIVERY: usize = 1;
const _: () = assert!(
    CAP_STATUS + CAP_NEXT + CAP_CRITERIA + CAP_UNITS + CAP_NEEDS + CAP_HOLDS + CAP_DELIVERY
        == BRIEF_LINES
);

/// A section's lines cut to `cap`: past it, the last line is `<label> (+N more)` for the entries not shown,
/// and it counts inside the cap.
fn capped(label: &str, mut lines: Vec<String>, cap: usize) -> Vec<String> {
    if lines.len() > cap {
        let keep = cap.saturating_sub(1);
        let more = lines.len() - keep;
        lines.truncate(keep);
        lines.push(format!("{label} (+{more} more)"));
    }
    lines
}
/// The own entries of a section cut to `cap` less the line that counts what lies elsewhere, which follows them
/// only when it is given (R45).
fn sectioned(label: &str, own: Vec<String>, elsewhere: Option<String>, cap: usize) -> Vec<String> {
    let room = cap - usize::from(elsewhere.is_some());
    let mut lines = capped(label, own, room);
    lines.extend(elsewhere);
    lines
}
/// The states in `order` with their counts, as `<state> <n>` joined by `, `, then `deferred <n>` when `off` is
/// above 0; `none` for nothing.
fn counted<'a>(order: &[&str], states: impl Iterator<Item = &'a Value>, off: usize) -> String {
    let states: Vec<&Value> = states.collect();
    let mut parts: Vec<String> = order
        .iter()
        .filter_map(|o| {
            let n = states.iter().filter(|s| **s == *o).count();
            (n > 0).then(|| format!("{o} {n}"))
        })
        .collect();
    if off > 0 {
        parts.push(format!("deferred {off}"));
    }
    match parts.is_empty() {
        true => "none".to_owned(),
        false => parts.join(", "),
    }
}
/// What a block reads besides its intent: the units, findings and holds of the store, the delivery record
/// and the store's clock.
struct Facts {
    now: u64,
    units: Vec<Found>,
    findings: Vec<Found>,
    holds: Vec<Found>,
    delivered: Option<Value>,
}
fn facts(s: &mut impl System, store: Option<&str>, remote: bool) -> Result<Facts, Failure> {
    let mut seen = Seen::default();
    let mut units = at(
        store::search_seen(s, store, "unit", vec![any("state", &UNITS)], &mut seen),
        "units",
    )?;
    let now = seen
        .now
        .ok_or_else(|| error("store_answer_unreadable", "now"))?;
    sort_units(&mut units);
    let mut findings: Vec<Found> = open_findings(s, store)?
        .into_iter()
        .filter(|f| f.1["state"] == "open")
        .collect();
    findings.sort_by_key(|(k, f)| (Reverse(text(&f["last_seen_at"]).to_owned()), k.clone()));
    let mut holds = active_holds(s, store)?;
    sort_holds(&mut holds);
    let delivered = if remote { None } else { delivery(s) };
    Ok(Facts {
        now,
        units,
        findings,
        holds,
        delivered,
    })
}
/// The notes cut into at most `CAP_NEXT` lines: each a `next ` and the next bytes of the notes, cut back to a
/// character boundary; `next -` for none.
fn next_lines(notes: &Value) -> Vec<String> {
    let all = one(notes.as_str().unwrap_or_default());
    let mut rest = all.as_str();
    let mut lines = vec![];
    while lines.len() < CAP_NEXT && !rest.is_empty() {
        let (head, _) = cut(rest, BRIEF_BYTES - 1 - "next ".len());
        rest = &rest[head.len()..];
        if !head.trim().is_empty() {
            lines.push(format!("next {}", head.trim_end()));
        }
    }
    if lines.is_empty() {
        lines.push("next -".to_owned());
    }
    lines
}
/// One block of the brief: the sections of R45 for the intent `key`, whose stored fields are `i`.
fn brief_block(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    i: &Value,
    facts: &Facts,
) -> Result<Vec<String>, Failure> {
    let mine = vec![json!({"eq": ["intent", crate::host::id("intent", key)]})];
    let criteria = list(s, store, "criterion", "criteria", mine)?;
    let value = |v: &Value| one(v.as_str().unwrap_or("-"));
    let units: Vec<&Found> = (facts.units.iter())
        .filter(|(_, u)| u["intent"]["native_key"] == key)
        .collect();
    let named = |u: &str| units.iter().any(|n| n.0 == u);

    let confirmed = store::confirmed(i);
    let status = vec![
        format!(
            "status {key} {}, confirmed {confirmed}",
            value(&i["status"])
        ),
        format!("status lead {}", value(&i["lead_session"])),
    ];
    let next = next_lines(&i["notes"]);

    let states = ["open", "pass", "fail", "waived"];
    let mut failed: Vec<&Found> = criteria.iter().filter(|c| c.1["state"] == "fail").collect();
    failed.sort_by_key(|(k, c)| (text(&c["added_at"]).to_owned(), k.clone()));
    let failed: Vec<String> = (failed.iter())
        .map(|(k, _)| format!("criterion {} fail", one(k)))
        .collect();
    let mut criterion = vec![format!(
        "criterion {}",
        counted(&states, criteria.iter().map(|c| &c.1["state"]), 0)
    )];
    criterion.extend(capped("criterion", failed, CAP_CRITERIA - 1));

    // The units: counts, then what is stopped, waits, is owed, runs and is ready, each by key.
    let deferred = |u: &&Found| !u.1["deferred_by"].is_null();
    let live: Vec<&Found> = units.iter().copied().filter(|u| !deferred(u)).collect();
    let on = |state: &'static str| live.iter().filter(move |u| u.1["state"] == state);
    let class = |u: &Found| value(&u.1["class"]);
    let stopped = on("stopped").map(|u| {
        let cause = u.1["stop_cause"].as_str().filter(|c| !c.is_empty());
        let cause = one(cause.unwrap_or("-"));
        format!("unit {} stopped {} stopped: {cause}", one(&u.0), class(u))
    });
    let accepted = on("accepted").map(|u| {
        let (head, approved) = (u.1["head"].as_str(), u.1["approved_head"].as_str());
        let by_lead = ["contract", "effect"].iter().any(|c| u.1["class"] == *c);
        let wait = match by_lead && (approved.is_none() || approved != head) {
            true => {
                let short: String = head.unwrap_or("-").chars().take(7).collect();
                format!("waits lead: approve {}", one(&short))
            }
            false => "waits: merge".to_owned(),
        };
        format!("unit {} accepted {} {wait}", one(&u.0), class(u))
    });
    let owed = on("merged")
        .filter(|u| {
            let given = u.1["use"].as_str().filter(|t| !t.is_empty());
            given.is_some_and(|t| !t.starts_with("none:")) && u.1["used_at"].is_null()
        })
        .map(|u| format!("unit {} use owed", one(&u.0)));
    let running = on("running").map(|u| {
        let began = u.1["started_at"].as_str().and_then(epoch);
        let minutes = began.map_or("-".to_owned(), |t| {
            (facts.now.saturating_sub(t) / 60).to_string()
        });
        format!("unit {} running {} {minutes} min", one(&u.0), class(u))
    });
    let ready = on("ready").map(|u| {
        let waits = match u.1["waits_on"].as_str() {
            Some(w) => format!(" waits on {}", one(w)),
            None => String::new(),
        };
        format!("unit {} ready {}{waits}", one(&u.0), class(u))
    });
    let mut off: Vec<&Found> = units.iter().copied().filter(|u| deferred(u)).collect();
    off.sort_by_key(|u| u.0.clone());
    let put_off = off.iter().map(|u| format!("unit {} deferred", one(&u.0)));
    let entries: Vec<String> = stopped
        .chain(accepted)
        .chain(owed)
        .chain(running)
        .chain(ready)
        .chain(put_off)
        .collect();
    let mut unit = vec![format!(
        "unit {}",
        counted(
            &UNITS,
            live.iter().map(|u| &u.1["state"]),
            units.len() - live.len()
        )
    )];
    unit.extend(capped("unit", entries, CAP_UNITS - 1));

    // The findings of the intent, newest first, and how many others of severity act or urgent are open.
    let about = |f: &Found| {
        let subject = subject_of(&f.1)
            .is_some_and(|(t, k)| (t == "intent" && k == key) || (t == "unit" && named(k)));
        let planned = f.1["unit"]["native_key"].as_str().is_some_and(named)
            || f.1["intent"]["native_key"] == key;
        subject || planned
    };
    let (own, other): (Vec<&Found>, Vec<&Found>) = facts.findings.iter().partition(|f| about(f));
    let found: Vec<String> = (own.iter())
        .map(|(k, f)| {
            format!(
                "needs finding {} {}: {}",
                one(k),
                value(&f["severity"]),
                one(text(&f["summary"]))
            )
        })
        .collect();
    let others = (other.iter())
        .filter(|f| f.1["severity"] == "act" || f.1["severity"] == "urgent")
        .count();
    let elsewhere = (others > 0).then(|| format!("needs (+{others} elsewhere: ap status)"));
    let needs = sectioned("needs", found, elsewhere, CAP_NEEDS);

    // The holds of the intent, its units and the hosts, oldest first.
    let (own, other): (Vec<&Found>, Vec<&Found>) = facts.holds.iter().partition(|h| {
        names_of(&h.1, "intent", key)
            || subject_of(&h.1).is_some_and(|(t, k)| t == "host" || (t == "unit" && named(k)))
    });
    let held: Vec<String> = (own.iter())
        .map(|h| {
            let scope = match h.1["scope"].as_str() {
                Some(x) => format!(" scope {}", one(x)),
                None => String::new(),
            };
            let subject =
                subject_of(&h.1).map_or(String::new(), |(t, k)| format!(" {t} {}", one(k)));
            format!(
                "hold {} until {}{subject}{scope}",
                value(&h.1["effect"]),
                value(&h.1["until"])
            )
        })
        .collect();
    let elsewhere = (!other.is_empty()).then(|| format!("hold (+{} elsewhere)", other.len()));
    let holds = sectioned("hold", held, elsewhere, CAP_HOLDS);

    let last = vec![match &facts.delivered {
        Some(d) => format!(
            "delivery {} {} tip {} target {}",
            value(&d["at"]),
            value(&d["end"]),
            value(&d["tip"]),
            value(&d["target"])
        ),
        None => "delivery unreadable".to_owned(),
    }];
    debug_assert!(
        status.len() <= CAP_STATUS && next.len() <= CAP_NEXT && last.len() <= CAP_DELIVERY
    );
    Ok([status, next, criterion, unit, needs, holds, last]
        .into_iter()
        .flatten()
        .take(BRIEF_LINES)
        .map(|l| cut(&l, BRIEF_BYTES - 1).0.to_owned())
        .collect())
}
/// `ap status --brief <intent>`: one block of at most `BRIEF_LINES` lines of `BRIEF_BYTES` bytes, in the
/// sections of R45. It reads what `ap status <intent>` and `ap status` read, and writes nothing.
fn brief(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    remote: bool,
) -> Result<Value, Failure> {
    let Some(i) = at(store::get(s, store, "intent", key), "intent")? else {
        return Err(error("unknown_intent", "intent"));
    };
    let facts = facts(s, store, remote)?;
    Ok(json!({"brief": brief_block(s, store, key, &i, &facts)?}))
}
/// `ap status --brief` without an intent: the block of each open intent whose directory is the one asked, or
/// the working directory, a final `/` dropped from each, by key and separated by an empty line.
fn open_briefs(
    s: &mut impl System,
    store: Option<&str>,
    directory: Option<&str>,
    remote: bool,
) -> Result<Value, Failure> {
    let dir = match directory {
        Some(d) => d.to_owned(),
        None => {
            let io = || error("io_error", "directory");
            let cwd = s.cwd().map_err(|_| io())?;
            cwd.to_str().ok_or_else(io)?.to_owned()
        }
    };
    let trimmed = |d: &str| d.strip_suffix('/').unwrap_or(d).to_owned();
    let status = any("status", &["draft", "assumed", "confirmed"]);
    let mut intents = list(s, store, "intent", "intents", vec![status])?;
    intents.retain(|(_, i)| {
        i["directory"]
            .as_str()
            .is_some_and(|d| trimmed(d) == trimmed(&dir))
    });
    intents.sort_by_key(|(k, _)| k.clone());
    if intents.is_empty() {
        let line = format!("status no open intent of {}", one(&dir));
        return Ok(json!({"brief": [cut(&line, BRIEF_BYTES - 1).0]}));
    }
    let facts = facts(s, store, remote)?;
    let mut lines: Vec<String> = vec![];
    for (n, (key, i)) in intents.iter().enumerate() {
        if n > 0 {
            lines.push(String::new());
        }
        lines.extend(brief_block(s, store, key, i, &facts)?);
    }
    Ok(json!({"brief": lines}))
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

fn execute_with(
    s: &mut impl System,
    store: Option<&str>,
    call: &Call,
    remote: Option<crate::client::Endpoint>,
) -> Result<Value, Failure> {
    if let Some(endpoint) = remote {
        let mut transport = crate::client::Transport::new(s, endpoint);
        return dispatch(&mut transport, store, call, true);
    }
    dispatch(s, store, call, false)
}

fn dispatch(
    s: &mut impl System,
    store: Option<&str>,
    call: &Call,
    remote: bool,
) -> Result<Value, Failure> {
    match call {
        Call::Overview => overview(s, store, remote),
        Call::Key(key) => keyed(s, store, key, false),
        Call::KeyAll(key) => keyed(s, store, key, true),
        Call::Brief(intent) => brief(s, store, intent, remote),
        Call::Open(directory) => open_briefs(s, store, directory.as_deref(), remote),
        Call::Term(word) => term(s, store, word),
        Call::Wait(intent, timeout) => wait(s, store, intent, *timeout, remote),
        Call::Indicators(intent) => indicators(s, store, intent.as_deref()),
        Call::Page => page(s, store, remote),
    }
}

pub fn execute(s: &mut impl System, store: Option<&str>, call: &Call) -> Result<Value, Failure> {
    let remote = crate::client::resolve(s, store)?;
    execute_with(s, store, call, remote)
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
/// The messages a run counts: the sum of its `days`' `messages`, 0 for a run without days (R35).
fn messages_of(run: &Value) -> u64 {
    let days = run["days"].as_array().into_iter().flatten();
    days.filter_map(|d| d["messages"].as_u64()).sum()
}
/// Who ran a unit's implementer work (R35): `{runtime, model}` of its run of role `implementer` with the most
/// messages, then the latest `started_at`, then the smallest key; the model is `served_by` when the run has one.
/// Null when the unit has no such run.
fn ran_of(runs: &[Found]) -> Value {
    let order = |a: &&Found, b: &&Found| {
        messages_of(&a.1)
            .cmp(&messages_of(&b.1))
            .then_with(|| text(&a.1["started_at"]).cmp(text(&b.1["started_at"])))
            .then_with(|| b.0.cmp(&a.0))
    };
    let implementer = runs.iter().filter(|r| r.1["role"] == "implementer");
    match implementer.max_by(order) {
        Some((_, r)) => {
            let model = match text(&r["served_by"]) {
                "" => r["model"].clone(),
                served => json!(served),
            };
            json!({"runtime": r["runtime"], "model": model})
        }
        None => Value::Null,
    }
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
    // A review `carry_review` made is a verdict but not a round: it is left out of the count alone.
    let rounds = reviews
        .iter()
        .filter(|(_, r)| r["carried_from"].is_null())
        .count();
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
        "ran": ran_of(&runs), "kind": u["kind"].clone(),
        "started_at": started_at, "merged_at": merged_at, "minutes": minutes, "rounds": rounds,
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
    let summary = summarize(&rows.iter().collect::<Vec<_>>());
    Ok(json!({"at": utc(now), "units": rows, "summary": summary}))
}
/// The summary of rows of the indicators (R40): their count, the medians and the first-accept share.
fn summarize(rows: &[&Value]) -> Value {
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
    json!({"count": rows.len(), "median_minutes": median(numbers("minutes")),
        "median_rounds": median(numbers("rounds")), "median_repairs": median(numbers("repairs")),
        "first_accept_share": share})
}

/// No step of the page begins once this long has passed since its first call of the store (R48).
const PAGE_BOUND: Duration = Duration::from_secs(600);
/// What a section of the page holds, or the code that says why it cannot be read (R53).
type Step<T> = Result<T, String>;
fn unreadable() -> String {
    "store_answer_unreadable".to_owned()
}
/// One step of the page: `page_bound` when the bound has passed before it begins, else its own result, a
/// failure as the code R22 passes on.
fn bounded<S: System, T>(
    s: &mut S,
    began: Instant,
    step: impl FnOnce(&mut S) -> Result<T, Failure>,
) -> Step<T> {
    if s.now().saturating_duration_since(began) >= PAGE_BOUND {
        return Err("page_bound".to_owned());
    }
    step(s).map_err(|f| store::code(&f))
}
/// The members of an answer, null ones included; one that is missing makes the section unreadable.
fn fields<'a>(v: &'a Value, names: &[&str]) -> Step<Vec<&'a Value>> {
    names
        .iter()
        .map(|n| v.get(n).ok_or_else(unreadable))
        .collect()
}
fn listed<'a>(v: &'a Value, name: &str) -> Step<&'a Vec<Value>> {
    v.get(name).and_then(Value::as_array).ok_or_else(unreadable)
}
/// Millionths of a USD as USD to the cent, in integers, half away from zero.
fn usd(v: &Value) -> Step<String> {
    let micros = i128::from(v.as_i64().ok_or_else(unreadable)?);
    let cents = (micros.abs() + 5_000) / 10_000;
    let sign = if micros < 0 && cents > 0 { "-" } else { "" };
    Ok(format!("{sign}{}.{:02}", cents / 100, cents % 100))
}
/// A load written in hundredths as `<n / 100>.<n % 100 in two digits>`; null is `-`.
fn load(v: &Value) -> Step<String> {
    if v.is_null() {
        return Ok("-".to_owned());
    }
    let n = v.as_u64().ok_or_else(unreadable)?;
    Ok(format!("{}.{:02}", n / 100, n % 100))
}
/// The UTC day `YYYY-MM-DD` of a time in seconds since 1970.
fn day(secs: u64) -> String {
    utc(secs).chars().take(10).collect()
}
/// The flow line of R50 over rows of the indicators.
fn flow_line(label: &str, rows: &[&Value], (since, until): (&str, &str)) -> String {
    let summary = summarize(rows);
    let in_week = |r: &&&Value| {
        let day = r["merged_at"].as_str().and_then(|t| t.get(..10));
        day.is_some_and(|d| since <= d && d <= until)
    };
    let cut = if rows.iter().any(|r| r["log_cut"] == true) {
        " log_cut"
    } else {
        ""
    };
    format!(
        "flow {label} {} merged, {} in the week, {}{cut}",
        cell(&summary["count"]),
        rows.iter().filter(in_week).count(),
        median_tail(&summary)
    )
}
/// The lines of R50 or R51 for an answer of `cost`: one for each group, then the total.
fn cost_lines(label: &str, a: &Value, all: bool) -> Step<Vec<String>> {
    let mut out = vec![];
    for g in listed(a, "groups")? {
        let [key, usd, runs] = fields(g, &["key", "usd", "runs"])?[..] else {
            return Err(unreadable());
        };
        let (key, usd, runs) = (cell(key), cell(usd), cell(runs));
        out.push(format!("cost {label} {key} {usd} USD {runs} runs"));
    }
    let names = [
        "total_usd",
        "merged",
        "usd_a_pr",
        "lead_share",
        "unmeasured",
    ];
    let got = fields(a, &names[..if all { 5 } else { 4 }])?;
    let [total, merged, per, share] = [0, 1, 2, 3].map(|i| cell(got[i]));
    let end = if all {
        format!(" unmeasured {}", cell(got[4]))
    } else {
        String::new()
    };
    out.push(format!(
        "cost {label} total {total} USD, merged {merged}, a pr {per}, lead share {share}{end}"
    ));
    Ok(out)
}
/// The lines of a section that may not be read: its lines, or the one line with the code.
fn section_lines(name: &str, lines: Step<Vec<String>>) -> Vec<String> {
    lines.unwrap_or_else(|code| vec![format!("{name} unreadable {code}")])
}
/// The `api` lines of R51 for an answer of `api_spend`.
fn api_lines(month: &str, a: &Value) -> Step<Vec<String>> {
    let mut out = vec![];
    for j in listed(a, "jobs")? {
        let names = [
            "job",
            "settled_usd_micros",
            "reserved_usd_micros",
            "counted_usd_micros",
        ];
        let [job, settled, reserved, counted] = fields(j, &names)?[..] else {
            return Err(unreadable());
        };
        let [runs, running] = fields(j, &["runs", "running"])?[..] else {
            return Err(unreadable());
        };
        out.push(format!(
            "api {month} {} settled {} reserved {} counted {} USD, {} runs, {} running",
            cell(job),
            usd(settled)?,
            usd(reserved)?,
            usd(counted)?,
            cell(runs),
            cell(running)
        ));
    }
    let names = [
        "settled_usd_micros",
        "reserved_usd_micros",
        "counted_usd_micros",
    ];
    let [settled, reserved, counted] = fields(a, &names)?[..] else {
        return Err(unreadable());
    };
    out.push(format!(
        "api {month} total settled {} reserved {} counted {} USD",
        usd(settled)?,
        usd(reserved)?,
        usd(counted)?
    ));
    Ok(out)
}
/// The lines of R52 for an answer of `capacity`.
fn capacity_lines(a: &Value) -> Step<Vec<String>> {
    let mut out = vec!["HOSTS".to_owned()];
    for h in listed(a, "hosts")? {
        let names = [
            "host", "machine", "node", "answers", "stale", "free", "cpus",
        ];
        let [host, machine, node, answers, stale, free, cpus] = fields(h, &names)?[..] else {
            return Err(unreadable());
        };
        let [l, available, seen] = fields(h, &["load_x100", "available_mb", "seen_at"])?[..] else {
            return Err(unreadable());
        };
        out.push(format!(
            "host {} machine {} node {} answers {} stale {} free {} cpus {} load {} available {} MB seen {}",
            cell(host),
            cell(machine),
            cell(node),
            cell(answers),
            cell(stale),
            cell(free),
            cell(cpus),
            load(l)?,
            cell(available),
            cell(seen)
        ));
    }
    for m in listed(a, "machines")? {
        let names = ["machine", "up", "free", "cpus", "load_x100", "available_mb"];
        let [machine, up, free, cpus, l, available] = fields(m, &names)?[..] else {
            return Err(unreadable());
        };
        out.push(format!(
            "machine {} up {} free {} cpus {} load {} available {} MB",
            cell(machine),
            cell(up),
            cell(free),
            cell(cpus),
            load(l)?,
            cell(available)
        ));
    }
    let can: Vec<String> = listed(a, "can_start")?.iter().map(cell).collect();
    let can = if can.is_empty() {
        "none".to_owned()
    } else {
        can.join(",")
    };
    out.push(format!("can start {can}"));
    out.push("ACCOUNTS".to_owned());
    for c in listed(a, "accounts")? {
        let names = ["account", "runtime", "allowance", "source", "at"];
        let [account, runtime, allowance, source, at] = fields(c, &names)?[..] else {
            return Err(unreadable());
        };
        out.push(format!(
            "account {} {} allowance {} source {} at {}",
            cell(account),
            cell(runtime),
            cell(allowance),
            cell(source),
            cell(at)
        ));
    }
    Ok(out)
}
/// `ap status --page`: the lines of `--text`, the week, then each open intent's brief, flow and cost, then the
/// flow, cost and credit's spend of all and the capacity (R47 to R53). Only a failure of the reads of R4
/// fails the form; every other section that cannot be read is one line with its code.
fn page(s: &mut impl System, store: Option<&str>, remote: bool) -> Result<Value, Failure> {
    let began = s.now();
    let head = overview(s, store, remote)?;
    let now = (head["at"].as_str().and_then(epoch))
        .ok_or_else(|| error("store_answer_unreadable", "now"))?;
    let (since, until) = (day(now.saturating_sub(6 * DAY)), day(now));
    let month: String = until.chars().take(7).collect();
    let intents: Vec<String> = (head["intents"].as_array().into_iter().flatten())
        .filter_map(|i| i["intent"].as_str().map(str::to_owned))
        .collect();
    // The calls, in the order of R48; the lines print in the order of R49.
    let capacity = bounded(s, began, |s| {
        store::function(s, store, "capacity", &json!({}))
    });
    let spend = bounded(s, began, |s| {
        store::function(s, store, "api_spend", &json!({ "month": month }))
    });
    let briefs: Vec<Step<Value>> = (intents.iter())
        .map(|i| bounded(s, began, |s| brief(s, store, i, remote)))
        .collect();
    let rows = bounded(s, began, |s| indicators(s, store, None));
    let window = json!({"since": since, "until": until, "by": "role"});
    let cost_all = bounded(s, began, |s| store::function(s, store, "cost", &window));
    let costs: Vec<Step<Value>> = (intents.iter())
        .map(|i| {
            let mut mine = window.clone();
            mine["intent"] = json!(i);
            bounded(s, began, |s| store::function(s, store, "cost", &mine))
        })
        .collect();

    let all_rows: Step<Vec<&Value>> = match &rows {
        Ok(v) => listed(v, "units").map(|u| u.iter().collect()),
        Err(code) => Err(code.clone()),
    };
    let flow = |label: &str, only: Option<&str>| match &all_rows {
        Ok(r) => {
            let mine: Vec<&Value> = (r.iter().copied())
                .filter(|u| only.is_none_or(|i| u["intent"].as_str() == Some(i)))
                .collect();
            vec![flow_line(label, &mine, (&since, &until))]
        }
        Err(code) => vec![format!("flow {label} unreadable {code}")],
    };
    let mut lines: Vec<String> = overview_text(&head).lines().map(str::to_owned).collect();
    lines.push(format!("WEEK {since} {until}"));
    for ((i, brief), cost) in intents.iter().zip(&briefs).zip(&costs) {
        let label = one(i);
        lines.push(format!("INTENT {label}"));
        match brief {
            Ok(v) => {
                let said = text_of(&Call::Brief(i.clone()), v);
                lines.extend(said.lines().map(str::to_owned));
            }
            Err(code) => lines.push(format!("brief {label} unreadable {code}")),
        }
        lines.extend(flow(&label, Some(i)));
        let cost = cost.clone().and_then(|a| cost_lines(&label, &a, false));
        lines.extend(section_lines(&format!("cost {label}"), cost));
    }
    lines.push("ALL".to_owned());
    lines.extend(flow("all", None));
    let cost = cost_all.and_then(|a| cost_lines("all", &a, true));
    lines.extend(section_lines("cost all", cost));
    let spent = spend.and_then(|a| api_lines(&month, &a));
    lines.extend(section_lines(&format!("api {month}"), spent));
    let capacity = capacity.and_then(|a| capacity_lines(&a));
    lines.extend(section_lines("capacity", capacity));
    Ok(json!({ "page": lines }))
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
    remote: bool,
) -> Result<BTreeSet<String>, Failure> {
    if remote {
        return Ok(BTreeSet::new());
    }
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
        let named =
            |k: &String, h: &Found| names_of(&h.1, "unit", k) || names_of(&h.1, "intent", intent);
        idle.retain(|k| !held.iter().any(|h| named(k, h)));
    }
    Ok(idle)
}
fn reading(
    s: &mut impl System,
    store: Option<&str>,
    intent: &str,
    remote: bool,
) -> Result<Reading, Failure> {
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
    let idle = idle(s, store, intent, &units, remote)?;
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
    remote: bool,
) -> Result<Value, Failure> {
    let began = s.now();
    let (mut first, mut before, mut failed, mut last_at, mut last) =
        (None, BTreeSet::new(), 0, None, None);
    loop {
        match reading(s, store, intent, remote) {
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
                "{} {} {}{} {at}{more}",
                g(u, "unit"),
                g(u, "state"),
                g(u, "class"),
                wave(&u["wave"])
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
            let subject = match (h["subject"]["type"].as_str(), h["subject"]["key"].as_str()) {
                (Some(t), Some(k)) => format!(" {t} {}", one(k)),
                _ => String::new(),
            };
            format!("{} until {}{}", g(h, "effect"), g(h, "until"), subject)
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
/// A value on one line: a null is `-`, a string with its control characters as spaces, anything else as its
/// JSON writes it.
fn cell(x: &Value) -> String {
    match x {
        Value::Null => "-".to_owned(),
        Value::String(t) => one(t),
        other => other.to_string(),
    }
}
/// The summary of R41 from `median minutes` to the end, a null member `-`.
fn median_tail(m: &Value) -> String {
    format!(
        "median minutes {} rounds {} repairs {} first accepted {}",
        cell(&m["median_minutes"]),
        cell(&m["median_rounds"]),
        cell(&m["median_repairs"]),
        cell(&m["first_accept_share"]),
    )
}
/// One line a row of the indicators and a last line of the summary, a null member `-`.
fn indicators_text(v: &Value) -> String {
    let g = cell;
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
    out + &format!("summary {} units {}\n", g(&m["count"]), median_tail(m))
}
/// The plain lines of an answer.
pub fn text_of(call: &Call, v: &Value) -> String {
    match call {
        Call::Indicators(_) => indicators_text(v),
        Call::Overview => overview_text(v),
        Call::Wait(..) => format!("{v}\n"),
        Call::Brief(_) | Call::Open(_) => {
            let lines = v["brief"].as_array().into_iter().flatten();
            lines.map(|l| format!("{}\n", text(l))).collect()
        }
        Call::Page => {
            let lines = v["page"].as_array().into_iter().flatten();
            lines.map(|l| format!("{}\n", text(l))).collect()
        }
        Call::Key(_) | Call::KeyAll(_) => {
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

/// The exit status of a refusal: 3 for a wait's error but `unknown_intent` and
/// `store_url_invalid`, else 2.
pub fn exit_code(waiting: bool, f: &Failure) -> u8 {
    2 + u8::from(waiting && !matches!(f.code, "unknown_intent" | "store_url_invalid"))
}

pub fn cli(argv: &[OsString]) -> ExitCode {
    let parsed = parse(argv);
    let configured = parsed.and_then(|a| {
        let remote = crate::client::resolve(&Host, a.store.as_deref())?;
        Ok((a, remote))
    });
    let waiting = configured
        .as_ref()
        .is_ok_and(|(a, _)| matches!(a.call, Call::Wait(..)));
    let result = configured.and_then(|(a, remote)| {
        let v = execute_with(&mut Host, a.store.as_deref(), &a.call, remote)?;
        Ok(
            if a.text || matches!(a.call, Call::Brief(_) | Call::Open(_) | Call::Page) {
                text_of(&a.call, &v)
            } else {
                format!("{v}\n")
            },
        )
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
