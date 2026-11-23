//! The function `cost` (`specs/ap/measure.md` R9, R10): what a window of UTC days cost, from the runs' `days`, the
//! merged units of `ap status --indicators` and the API credit's runs. It reads the store only.
use super::sessions::day_start;
use crate::host::{Failure, System, error};
use crate::status::{self, Call};
use crate::store;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const BY: [&str; 4] = ["role", "intent", "unit", "day"];
const STAGES: [&str; 4] = ["implementer", "fixer", "verifier", "reviewer"];
const LEAD: [&str; 2] = ["lead", "writer"];
pub(super) const DAY: u64 = 86_400;

struct Window {
    since: String,
    until: String,
    by: String,
    intent: Option<String>,
}

/// The window of the parameters `o`: `since` and `until`, UTC days of the calendar, `since` not after `until`, at most
/// 92 days (R9). `flow` and `share` read their window as `cost` does.
pub(super) fn window(o: &serde_json::Map<String, Value>) -> Result<(String, String), Failure> {
    let invalid = |f: &str| error("invalid_input", f);
    let day = |name: &str| -> Result<(String, u64), Failure> {
        let d = o
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(name))?;
        let t = (d.len() == 10)
            .then(|| day_start(d))
            .flatten()
            .ok_or_else(|| invalid(name))?;
        Ok((d.to_owned(), t))
    };
    let (since, from) = day("since")?;
    let (until, to) = day("until")?;
    if to < from || (to - from) / DAY + 1 > 92 {
        return Err(invalid("until"));
    }
    Ok((since, until))
}

fn parameters(p: &Value) -> Result<Window, Failure> {
    let invalid = |f: &str| error("invalid_input", f);
    let o = p.as_object().ok_or_else(|| invalid("stdin"))?;
    if let Some(k) = o
        .keys()
        .find(|k| !["since", "until", "by", "intent"].contains(&k.as_str()))
    {
        return Err(invalid(k));
    }
    let (since, until) = window(o)?;
    let by = match o.get("by") {
        None | Some(Value::Null) => "role".to_owned(),
        Some(Value::String(b)) if BY.contains(&b.as_str()) => b.clone(),
        Some(_) => return Err(invalid("by")),
    };
    let intent = match o.get("intent") {
        None | Some(Value::Null) => None,
        Some(Value::String(i)) if !i.is_empty() && i.len() <= 512 => Some(i.clone()),
        Some(_) => return Err(invalid("intent")),
    };
    Ok(Window {
        since,
        until,
        by,
        intent,
    })
}

fn key_of(v: &Value) -> Option<String> {
    v["native_key"].as_str().map(str::to_owned)
}
/// Micro-dollars as USD rounded to 0.01 once.
fn usd(micros: i64) -> f64 {
    ((micros as f64) / 10_000.0).round() / 100.0
}
/// A sum over the merged units, rounded to 0.01; null when none merged.
fn per(n: f64, merged: usize) -> Value {
    match merged {
        0 => Value::Null,
        m => json!((n / m as f64 * 100.0).round() / 100.0),
    }
}
/// Micro-dollars over the merged units, in USD rounded to 0.01 once; null when none merged.
fn per_usd(micros: i64, merged: usize) -> Value {
    match merged {
        0 => Value::Null,
        m => json!(usd((micros as f64 / m as f64).round() as i64)),
    }
}

#[derive(Default)]
struct Group {
    micros: i64,
    messages: u64,
    runs: BTreeSet<String>,
}

/// `cost` over the store `st` (the engine's default when none) with the parameters `p`.
pub(super) fn cost(s: &mut impl System, st: Option<&str>, p: &Value) -> Result<Value, Failure> {
    let w = parameters(p)?;
    if let Some(i) = &w.intent
        && store::get(s, st, "intent", i)?.is_none()
    {
        return Err(error("unknown_intent", "intent"));
    }
    let (runs, _) = store::search_every(s, st, "run", store::PAGE)?;
    let needs_units = w.intent.is_some() || w.by == "intent";
    let units: BTreeMap<String, Value> = match needs_units {
        true => store::search_every(s, st, "unit", store::PAGE)?
            .0
            .into_iter()
            .collect(),
        false => BTreeMap::new(),
    };
    let intent_of = |f: &Value| -> Option<String> {
        key_of(&f["intent"]).or_else(|| {
            let unit = key_of(&f["unit"])?;
            key_of(&units.get(&unit)?["intent"])
        })
    };
    let mine: Vec<&(String, Value)> = runs
        .iter()
        .filter(|(_, f)| w.intent.is_none() || intent_of(f) == w.intent)
        .collect();
    let in_window = |d: &str| w.since.as_str() <= d && d <= w.until.as_str();
    let mut groups: BTreeMap<Option<String>, Group> = BTreeMap::new();
    let (mut total, mut stages, mut lead, mut lead_refusals) = (0i64, 0i64, 0i64, 0u64);
    for (k, f) in &mine {
        let role = f["role"].as_str().unwrap_or_default();
        for d in f["days"].as_array().into_iter().flatten() {
            let Some(day) = d["day"].as_str().filter(|d| in_window(d)) else {
                continue;
            };
            let micros = d["usd_micros"].as_i64().unwrap_or(0);
            let key = match w.by.as_str() {
                "role" => Some(role.to_owned()),
                "intent" => intent_of(f),
                "unit" => key_of(&f["unit"]),
                _ => Some(day.to_owned()),
            };
            let g = groups.entry(key).or_default();
            g.micros += micros;
            g.messages += d["messages"].as_u64().unwrap_or(0);
            g.runs.insert(k.clone());
            total += micros;
            if STAGES.contains(&role) {
                stages += micros;
            }
            if LEAD.contains(&role) {
                lead += micros;
            }
            if role == "lead" {
                lead_refusals += d["refusals"].as_u64().unwrap_or(0);
            }
        }
    }
    let mut listed: Vec<Value> = vec![];
    let mut last = None;
    for (key, g) in groups {
        let v =
            json!({"key": key, "usd": usd(g.micros), "messages": g.messages, "runs": g.runs.len()});
        match key {
            Some(_) => listed.push(v),
            None => last = Some(v),
        }
    }
    listed.extend(last);
    let indicators = status::execute(s, st, &Call::Indicators(w.intent.clone()))?;
    let merged = indicators["units"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            r["merged_at"]
                .as_str()
                .and_then(|m| m.get(..10))
                .is_some_and(in_window)
        })
        .count();
    let from = day_start(&w.since).unwrap_or(0);
    let to = day_start(&w.until).unwrap_or(0) + DAY;
    let batch: Vec<Value> = runs
        .iter()
        .filter(|(_, f)| f["role"] == "batch")
        .map(|(_, f)| f.clone())
        .collect();
    let (mut settled, mut reserved) = (0i64, 0i64);
    for t in crate::api::tally(&batch, (from, to)).values() {
        settled = settled.saturating_add(t.settled);
        reserved = reserved.saturating_add(t.reserved);
    }
    let unmeasured = mine
        .iter()
        .filter(|(_, f)| {
            f["measured_at"].is_null()
                && f["started_at"]
                    .as_str()
                    .and_then(|t| t.get(..10))
                    .is_some_and(in_window)
        })
        .count();
    let share = match total {
        0 => Value::Null,
        t => json!((lead as f64 / t as f64 * 1000.0).round() / 10.0),
    };
    Ok(
        json!({"since": w.since, "until": w.until, "by": w.by, "intent": w.intent, "groups": listed,
        "total_usd": usd(total), "merged": merged,
        "usd_a_pr": per_usd(total, merged), "usd_stages_a_pr": per_usd(stages, merged),
        "usd_lead_a_pr": per_usd(lead, merged), "lead_share": share,
        "lead_refusals_a_pr": per(lead_refusals as f64, merged),
        "api": {"settled_usd": usd(settled), "reserved_usd": usd(reserved)}, "unmeasured": unmeasured}),
    )
}
