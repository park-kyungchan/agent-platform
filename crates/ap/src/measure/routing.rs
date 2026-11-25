//! The pass `ap measure routing` (`specs/ap/routing.md` R9 to R12): once a day, with the policy's member `routing`,
//! the stop-and-repair rate of each tier on each kind of work over the bar's window, from the function `flow` by
//! tier over every confirmed intent, and the findings of the detector `ap-routing` that bar a tier (`open_finding`,
//! `see_finding_again`, `reopen_finding`, `close_finding`). It applies nothing else.
use super::cost::DAY;
use super::sessions::day_start;
use crate::host::{Failure, System, digest, error, id};
use crate::store::{self, Actor};
use crate::unit::routing::{self, DETECTOR, KEY_PREFIX, bar_key};
use crate::watch::utc;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, time::UNIX_EPOCH};

/// The sums of one tier on one kind: merged units and those stopped or repaired.
#[derive(Default, Clone, Copy)]
struct Sum {
    merged: u64,
    stopped_or_repaired: u64,
}

/// The options of the pass: the day and the store, each at most once, as `ap measure kpis` takes them (R9).
fn options(argv: &[OsString]) -> Result<(Option<String>, Option<String>), Failure> {
    let usage = || error("usage", "args");
    let mut words = vec![];
    for a in argv {
        words.push(a.to_str().ok_or_else(usage)?.to_owned());
    }
    let (mut day, mut store) = (None, None);
    let mut it = words.into_iter();
    while let Some(w) = it.next() {
        let slot = match w.as_str() {
            "--day" => &mut day,
            "--store" => &mut store,
            _ => return Err(usage()),
        };
        let value = it.next().ok_or_else(usage)?;
        if slot.replace(value).is_some() {
            return Err(usage());
        }
    }
    Ok((day, store))
}

/// The pass (R9 to R12): the answer line and its exit status.
pub(super) fn pass(s: &mut impl System, argv: &[OsString]) -> Result<(Value, u8), Failure> {
    let (day, store) = options(argv)?;
    if store.as_deref().is_some_and(|p| !p.starts_with('/')) {
        return Err(error("invalid_input", "store"));
    }
    let now = s
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let at = utc(now);
    let today = at[..10].to_owned();
    let day = day.unwrap_or_else(|| today.clone());
    let from = day_start(&day)
        .filter(|_| day <= today && day.len() == 10)
        .ok_or_else(|| error("invalid_input", "day"))?;
    // Without the member nothing is read of the store and nothing applied (R9).
    let Some(r) = routing::of_host(s)? else {
        return Ok((json!({"at": at, "day": day, "skipped": "no routing"}), 0));
    };
    let (since, until) = (
        utc(from.saturating_sub(r.bar.window_days * DAY))[..10].to_owned(),
        utc(from.saturating_sub(DAY))[..10].to_owned(),
    );
    let st = store.as_deref();
    // The reads come before any effect: a failed one ends the pass with nothing applied (R10).
    let mut intents = store::search_every(s, st, "intent", store::PAGE)?.0;
    intents.retain(|(_, f)| f["status"] == "confirmed");
    intents.sort_by(|a, b| a.0.cmp(&b.0));
    let mut sums: BTreeMap<(String, String, String), Sum> = BTreeMap::new();
    for (intent, _) in &intents {
        let p = json!({"intent": intent, "since": since, "until": until, "by": "tier"});
        let answer = super::flow::flow(s, st, &p)?;
        for g in answer["groups"].as_array().into_iter().flatten() {
            let (Some(runtime), Some(model), Some(kind)) =
                (g["runtime"].as_str(), g["model"].as_str(), g["kind"].as_str())
            else {
                continue;
            };
            let sum = sums
                .entry((runtime.to_owned(), model.to_owned(), kind.to_owned()))
                .or_default();
            sum.merged += g["merged"].as_u64().unwrap_or(0);
            sum.stopped_or_repaired += g["stopped_or_repaired"].as_u64().unwrap_or(0);
        }
    }
    let all = vec![json!({"eq": ["detector", DETECTOR]})];
    let found: BTreeMap<String, Value> = store::search(s, st, "finding", all)?
        .into_iter()
        .filter(|(k, f)| k.starts_with(KEY_PREFIX) && f["detector"] == DETECTOR)
        .collect();
    let session = format!("measure-{now}");
    let actor = Actor {
        agent: "ap-measure",
        session: &session,
        role: "measure",
    };
    let window = format!("{since}..{until}");
    let mut done: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for k in ["opened", "seen", "reopened", "closed"] {
        done.insert(k, vec![]);
    }
    let (mut unchanged, mut failed) = (0, vec![]);
    for ((runtime, model, kind), sum) in &sums {
        if sum.merged < r.bar.min_merged {
            continue;
        }
        let key = bar_key(runtime, model, kind);
        let rate = sum.stopped_or_repaired as f64 / sum.merged as f64;
        let counts = format!(
            "{} stopped or repaired of {} merged, {window}",
            sum.stopped_or_repaired, sum.merged
        );
        let evidence = format!(
            "stop_repair_rate {rate:.3} over the bar {}: {counts} (ont fn flow by tier over the confirmed intents)",
            r.bar.rate
        );
        let state = found.get(&key).and_then(|f| f["state"].as_str());
        let finding = id("finding", &key);
        let above = rate > r.bar.rate;
        let (action, word, p) = match (above, state) {
            (true, None) => {
                let summary = format!(
                    "The tier {runtime} {model} stopped or was repaired on units of kind {kind} above the bar of the routing map: no unit of that kind starts on it until the Lead closes or dismisses this finding."
                );
                let p = json!({"finding": finding, "kind": "incident", "detector": DETECTOR,
                    "summary": summary, "evidence": evidence, "severity": "act"});
                ("open_finding", "opened", p)
            }
            (true, Some("open" | "planned")) => (
                "see_finding_again",
                "seen",
                json!({"finding": finding, "evidence": evidence}),
            ),
            (true, Some("closed")) => (
                "reopen_finding",
                "reopened",
                json!({"finding": finding, "evidence": evidence}),
            ),
            (false, Some("open" | "planned")) => {
                let reason = format!(
                    "stop_repair_rate {rate:.3} at or under the bar {}: {counts}",
                    r.bar.rate
                );
                (
                    "close_finding",
                    "closed",
                    json!({"finding": finding, "reason": reason}),
                )
            }
            // A dismissed finding is the Lead's waiver; a closed one under the bar stays closed.
            _ => continue,
        };
        let rid = format!(
            "routing-{}-{word}-{}",
            day.replace('-', ""),
            &digest(format!("{runtime}\n{model}\n{kind}").as_bytes())[..16]
        );
        match store::unit_action(s, st, &actor, action, rid, p) {
            Ok(a) if a.outcome == "applied" => {
                if let Some(list) = done.get_mut(word) {
                    list.push(key);
                }
            }
            Ok(_) => unchanged += 1,
            Err(f) if store::code_of(&f) == "request_id_reused" => unchanged += 1,
            Err(f) => {
                failed.push(json!({"finding": key, "action": action, "code": store::code_of(&f)}))
            }
        }
    }
    let status = if failed.is_empty() { 0 } else { 3 };
    let line = json!({"at": at, "day": day, "since": since, "until": until,
        "opened": done["opened"], "seen": done["seen"], "reopened": done["reopened"],
        "closed": done["closed"], "unchanged": unchanged, "failed": failed});
    Ok((line, status))
}
