//! `ap status`: what is asked, planned, running, waiting and broken, read from the store alone; one
//! intent or unit in full; and what a word of the life cycle means (README "The status",
//! `specs/ap/status.md`). It writes nothing.
use crate::host::{Failure, Host, System, error};
use crate::store::{self, Found, Seen, at};
use serde_json::{Value, json};
use std::{cmp::Reverse, ffi::OsString, io::Write, process::ExitCode};

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
}
pub struct Args {
    pub store: Option<String>,
    pub text: bool,
    pub call: Call,
}
/// `--store <absolute path>`, `--text`, `--term <word>` and one key, each at most once.
pub fn parse(argv: &[OsString]) -> Result<Args, Failure> {
    let usage = || error("usage", "args");
    let (mut store, mut text, mut term, mut key) = (None, false, None, None);
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
            k if key.is_none()
                && !k.starts_with('-')
                && (1..=512).contains(&k.len())
                && !k.chars().any(char::is_control) =>
            {
                key = Some(k.to_owned())
            }
            _ => return Err(usage()),
        }
    }
    let call = match (term, key) {
        (None, None) => Call::Overview,
        (None, Some(k)) => Call::Key(k),
        (Some(w), None) => Call::Term(w),
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
/// Every object of `kind` the filter matches, the failure naming the list.
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
    let mut denied: Vec<String> = vec![];
    for (kind, p) in enumerated {
        if denied.contains(&kind) {
            continue;
        }
        let name = text(&p["name"]).to_owned();
        match store::members(s, store, &kind, &name) {
            Ok(found) => found.iter().for_each(|m| {
                add(
                    "value",
                    &json!({"name": m}),
                    Some(&format!("{kind}.{name}")),
                    described(&p),
                );
            }),
            Err(f) if f.code == store::REFUSED => {
                if f.detail["code"] == "derive_forbidden" {
                    denied.push(kind);
                }
            }
            Err(f) => return Err(f),
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
/// The plain lines of an answer.
pub fn text_of(call: &Call, v: &Value) -> String {
    match call {
        Call::Overview => overview_text(v),
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
                out.push_str(&format!(
                    "{} {of}{}: {}\n",
                    text(&m["kind"]),
                    text(&m["name"]),
                    one(text(&m["text"]))
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

pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = parse(argv).and_then(|a| {
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
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
