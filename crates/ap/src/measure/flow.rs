//! The function `flow` (`specs/ap/measure.md` R11): how an intent's merged units of a window flowed, from the rows of
//! `ap status --indicators` and the units' reviews of kind `page`. It reads the store only.
use super::cost::{DAY, window};
use super::sessions::day_start;
use crate::host::{Failure, System, error};
use crate::status::{self, Call};
use crate::store;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The parameters of `flow` and `share` (R9, R11, R12): the window and the intent, which `flow` requires and `share`
/// may leave out; no other member (`flow`'s `by` is read apart, R17).
pub(super) fn parameters(
    p: &Value,
    intent_required: bool,
) -> Result<(String, String, Option<String>), Failure> {
    let invalid = |f: &str| error("invalid_input", f);
    let o = p.as_object().ok_or_else(|| invalid("stdin"))?;
    if let Some(k) = o
        .keys()
        .find(|k| !["since", "until", "intent"].contains(&k.as_str()))
    {
        return Err(invalid(k));
    }
    let (since, until) = window(o)?;
    let intent = match o.get("intent") {
        None | Some(Value::Null) if !intent_required => None,
        Some(Value::String(i)) if !i.is_empty() && i.len() <= 512 => Some(i.clone()),
        _ => return Err(invalid("intent")),
    };
    Ok((since, until, intent))
}

/// The median of the values (the mean of the two middle ones for an even count), null for none: R40's.
fn median(mut v: Vec<u64>) -> Value {
    v.sort_unstable();
    match (v.len(), v.len() / 2) {
        (0, _) => Value::Null,
        (n, m) if !n.is_multiple_of(2) => json!(v[m]),
        (_, m) if (v[m - 1] + v[m]).is_multiple_of(2) => json!((v[m - 1] + v[m]) / 2),
        (_, m) => json!((v[m - 1] + v[m]) as f64 / 2.0),
    }
}
/// `n` over `of` rounded to `places` (1, 100 or 1000 steps); null with nothing to divide by.
fn ratio(n: usize, of: usize, steps: f64) -> Value {
    match of {
        0 => Value::Null,
        d => json!((n as f64 / d as f64 * steps).round() / steps),
    }
}

/// The group of a unit in `flow` by `tier` (R17): its runtime, model and kind, each ordered with null last.
type Tier<'a> = [(bool, &'a str); 3];

/// `flow`'s groups by tier (R17) over the rows of the indicators: one group for each runtime, model and kind of
/// the rows' `ran` and `kind`, a null one last in its member, as `cost`'s null key is (R10).
fn tiers(rows: &[&Value]) -> Value {
    fn part(v: &Value) -> (bool, &str) {
        (v.as_str().is_none(), v.as_str().unwrap_or_default())
    }
    let mut groups: BTreeMap<Tier, Vec<&Value>> = BTreeMap::new();
    for r in rows {
        let key = [
            part(&r["ran"]["runtime"]),
            part(&r["ran"]["model"]),
            part(&r["kind"]),
        ];
        groups.entry(key).or_default().push(r);
    }
    groups
        .into_values()
        .map(|g| {
            let n = g.len();
            // A row whose `stops` is null (a cut log) counts as no stop (R17).
            let stopped = |r: &Value| r["stops"].as_array().is_some_and(|c| !c.is_empty());
            let repairs = |r: &Value| r["repairs"].as_u64().unwrap_or(0);
            let count = |f: &dyn Fn(&Value) -> bool| g.iter().filter(|r| f(r)).count();
            let either = count(&|r| stopped(r) || repairs(r) > 0);
            json!({"runtime": g[0]["ran"]["runtime"], "model": g[0]["ran"]["model"], "kind": g[0]["kind"],
                "merged": n, "first_accept": ratio(count(&|r| r["first_verdict"] == "accept"), n, 1000.0),
                "units_stopped": count(&stopped),
                "repairs": g.iter().map(|r| repairs(r)).sum::<u64>(),
                "stopped_or_repaired": either, "stop_repair_rate": ratio(either, n, 1000.0)})
        })
        .collect()
}

/// One page look of a unit: when it closed, its key and its verdict.
type Look = (String, String, String);

/// `flow` over the store `st` (the engine's default when none) with the parameters `p`.
pub(super) fn flow(s: &mut impl System, st: Option<&str>, p: &Value) -> Result<Value, Failure> {
    let mut rest = p.clone();
    let by = rest.as_object_mut().and_then(|o| o.remove("by"));
    let (since, until, intent) = parameters(&rest, true)?;
    let tier = match by {
        None | Some(Value::Null) => false,
        Some(Value::String(b)) if b == "tier" => true,
        Some(_) => return Err(error("invalid_input", "by")),
    };
    let intent = intent.unwrap_or_default();
    if store::get(s, st, "intent", &intent)?.is_none() {
        return Err(error("unknown_intent", "intent"));
    }
    let indicators = status::execute(s, st, &Call::Indicators(Some(intent.clone())))?;
    let in_window = |d: &str| since.as_str() <= d && d <= until.as_str();
    let rows: Vec<&Value> = indicators["units"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            r["merged_at"]
                .as_str()
                .and_then(|m| m.get(..10))
                .is_some_and(in_window)
        })
        .collect();
    let merged = rows.len();
    let days = (day_start(&until).unwrap_or(0) - day_start(&since).unwrap_or(0)) / DAY + 1;
    let numbers = rows
        .iter()
        .filter_map(|r| r["minutes"].as_u64())
        .collect::<Vec<_>>();
    let verdicts: Vec<&Value> = rows
        .iter()
        .map(|r| &r["first_verdict"])
        .filter(|v| v.is_string())
        .collect();
    let accepted = verdicts.iter().filter(|v| **v == "accept").count();
    let mut stops: BTreeMap<String, u64> = BTreeMap::new();
    let mut stopped = 0;
    for r in &rows {
        let causes = r["stops"].as_array().map_or(&[][..], Vec::as_slice);
        stopped += usize::from(!causes.is_empty());
        for c in causes.iter().filter_map(Value::as_str) {
            *stops.entry(c.to_owned()).or_default() += 1;
        }
    }
    // The page looks of the merged units: their closed reviews of kind `page`, in the order of their closing.
    let mine: BTreeSet<&str> = rows.iter().filter_map(|r| r["unit"].as_str()).collect();
    let mut looks: BTreeMap<String, Vec<Look>> = BTreeMap::new();
    if merged > 0 {
        for (key, r) in store::search_every(s, st, "review", store::PAGE)?.0 {
            let unit = r["unit"]["native_key"]
                .as_str()
                .filter(|u| mine.contains(u));
            if let Some(unit) = unit
                && r["kind"] == "page"
                && r["status"] == "closed"
            {
                let text = |m: &str| r[m].as_str().unwrap_or_default().to_owned();
                looks.entry(unit.to_owned()).or_default().push((
                    text("closed_at"),
                    key,
                    text("verdict"),
                ));
            }
        }
    }
    let (mut total, mut after, mut first) = (0, 0, 0);
    for l in looks.values_mut() {
        l.sort();
        total += l.len();
        first += usize::from(l[0].2 == "accept");
        if let Some(n) = l.iter().position(|k| k.2 == "accept") {
            after += l.len() - n - 1;
        }
    }
    let mut answer = json!({"intent": intent, "since": since, "until": until, "merged": merged,
        "merged_a_day": (merged as f64 / days as f64 * 100.0).round() / 100.0,
        "median_minutes": median(numbers),
        "packet_first_accept": ratio(accepted, verdicts.len(), 1000.0),
        "units_stopped": stopped, "stops": stops,
        "page_looks": ratio(total, merged, 100.0),
        "looks_after_accept": ratio(after, merged, 100.0),
        "first_accept": ratio(first, merged, 1000.0),
        "log_cut": rows.iter().any(|r| r["log_cut"] == true)});
    if tier {
        answer["groups"] = tiers(&rows);
    }
    Ok(answer)
}
