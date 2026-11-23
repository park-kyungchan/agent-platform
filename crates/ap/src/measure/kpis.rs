//! The pass `ap measure kpis` (`specs/ap/measure.md` R14, R15): once a day, one `measurement` of each KPI of a
//! confirmed intent whose name is in the table, over the seven days before, from the functions `cost`, `flow` and
//! `share`, with the function call as its evidence. Its only effect is `record_measurement`.
use super::cost::DAY;
use super::sessions::day_start;
use crate::host::{Failure, System, error, id};
use crate::store::{self, Actor};
use crate::watch::utc;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    time::UNIX_EPOCH,
};

/// The KPIs a pass measures (R14): the name, the function it asks and the member of the answer it takes.
pub const TABLE: [(&str, &str, &str); 13] = [
    ("usd_a_pr", "cost", "usd_a_pr"),
    ("usd_stages", "cost", "usd_stages_a_pr"),
    ("usd_lead", "cost", "usd_lead_a_pr"),
    ("lead_share", "cost", "lead_share"),
    ("lead_refusals", "cost", "lead_refusals_a_pr"),
    ("minutes_to_merge", "flow", "median_minutes"),
    ("merged_a_day", "flow", "merged_a_day"),
    ("stops", "flow", "stops"),
    ("page_looks", "flow", "page_looks"),
    ("looks_after_accept", "flow", "looks_after_accept"),
    ("first_accept", "flow", "first_accept"),
    ("store_writes", "share", "write_share"),
    ("store_queries", "share", "read_share"),
];
const VALUE_MAX: usize = 200;
const EVIDENCE_MAX: usize = 300;

/// A number as the function answers it; `n/a` for null or anything else.
fn number(v: &Value) -> String {
    match v {
        Value::Number(_) => v.to_string(),
        _ => "n/a".to_owned(),
    }
}

/// The text of the `stops` KPI: the causes whole, then `...` when the list was cut to fit (R15).
fn stops_text(answer: &Value, window: &str) -> Result<String, &'static str> {
    let head = format!(
        "{} of {} stopped: ",
        number(&answer["units_stopped"]),
        number(&answer["merged"])
    );
    let mut items: Vec<String> = answer["stops"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(cause, n)| format!("{cause} {}", number(n)))
        .collect();
    items.sort();
    let whole = match items.is_empty() {
        true => format!("{head}none ({window})"),
        false => format!("{head}{} ({window})", items.join(", ")),
    };
    if whole.len() <= VALUE_MAX {
        return Ok(whole);
    }
    (0..items.len())
        .rev()
        .map(|k| match k {
            0 => format!("{head}... ({window})"),
            k => format!("{head}{}, ... ({window})", items[..k].join(", ")),
        })
        .find(|t| t.len() <= VALUE_MAX)
        .ok_or("value_too_long")
}

/// The value text of the KPI `name` from the answer of its function, over the window `since` to `until` (R15); the
/// code of a measurement that fails when the text is over 200 bytes (`value_too_long`) or the name is not in the table.
pub fn value_text(
    name: &str,
    answer: &Value,
    (since, until): (&str, &str),
) -> Result<String, &'static str> {
    let window = format!("{since}..{until}");
    let (_, function, member) = TABLE
        .iter()
        .find(|(n, _, _)| *n == name)
        .ok_or("unknown_kpi")?;
    let text = match (*function, name) {
        (_, "stops") => return stops_text(answer, &window),
        ("share", _) => {
            let of = |group: &str| {
                let found = answer["groups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|g| g["group"] == group);
                number(found.map_or(&Value::Null, |g| &g[member]))
            };
            format!("sessions {} % stages {} % ({window})", of("sessions"), of("stages"))
        }
        _ => format!(
            "{} ({window}, {} merged)",
            number(&answer[member]),
            number(&answer["merged"])
        ),
    };
    match text.len() <= VALUE_MAX {
        true => Ok(text),
        false => Err("value_too_long"),
    }
}

/// The evidence of a measurement: the call of the function `function` with the parameters `parameters`, as
/// `ont fn` takes them; `evidence_too_long` over 300 bytes (R15).
pub fn evidence_text(function: &str, parameters: &Value) -> Result<String, &'static str> {
    let text = format!("ont fn {function} {parameters}");
    match text.len() <= EVIDENCE_MAX {
        true => Ok(text),
        false => Err("evidence_too_long"),
    }
}

/// The options of the pass: the day and the store, each at most once (R14).
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

/// The pass (R14, R15): the answer line and its exit status.
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
    let (since, until) = (
        utc(from.saturating_sub(7 * DAY))[..10].to_owned(),
        utc(from.saturating_sub(DAY))[..10].to_owned(),
    );
    let st = store.as_deref();
    // The reads come before any effect: a failed one ends the pass with nothing applied.
    let mut kpis = store::search_every(s, st, "kpi", store::PAGE)?.0;
    kpis.sort_by(|a, b| a.0.cmp(&b.0));
    let mut confirmed: BTreeMap<String, bool> = BTreeMap::new();
    let mut skipped = 0;
    let mut todo = vec![];
    for (key, f) in kpis {
        let name = f["name"].as_str().unwrap_or_default().to_owned();
        let intent = f["intent"]["native_key"].as_str().map(str::to_owned);
        let entry = TABLE.iter().find(|(n, _, _)| *n == name);
        let (Some(intent), Some(entry)) = (intent, entry) else {
            skipped += 1;
            continue;
        };
        if !confirmed.contains_key(&intent) {
            let found = store::get(s, st, "intent", &intent)?;
            confirmed.insert(
                intent.clone(),
                found.is_some_and(|i| i["status"] == "confirmed"),
            );
        }
        match confirmed[&intent] {
            true => todo.push((key, name, intent, entry.1)),
            false => skipped += 1,
        }
    }
    let session = format!("measure-{now}");
    let actor = Actor {
        agent: "ap-measure",
        session: &session,
    };
    // Each function runs once for an intent in a pass.
    let mut answers: BTreeMap<(&str, String), Result<Value, String>> = BTreeMap::new();
    let (mut measured, mut failed, mut unchanged) = (vec![], vec![], 0);
    for (key, name, intent, function) in todo {
        let parameters = json!({"intent": intent, "since": since, "until": until});
        let answer = answers
            .entry((function, intent.clone()))
            .or_insert_with(|| {
                super::answer_in(s, function, st, &parameters)
                    .map_err(|f| store::code_of(&f))
            })
            .clone();
        let made = answer.and_then(|a| {
            let value = value_text(&name, &a, (&since, &until)).map_err(str::to_owned)?;
            let evidence = evidence_text(function, &parameters).map_err(str::to_owned)?;
            Ok((value, evidence))
        });
        let (value, evidence) = match made {
            Ok(v) => v,
            Err(code) => {
                failed.push(json!({"kpi": key, "name": name, "code": code}));
                continue;
            }
        };
        let rid = format!("measure-{key}-{}", day.replace('-', ""));
        let p = json!({"kpi": id("kpi", &key), "value": value, "observed_at": at, "evidence": evidence});
        match store::unit_action(s, st, &actor, "record_measurement", rid, p) {
            Ok(a) if a.outcome == "applied" => {
                measured.push(json!({"kpi": key, "name": name, "value": value}));
            }
            Ok(_) => unchanged += 1,
            Err(f) if store::code_of(&f) == "request_id_reused" => unchanged += 1,
            Err(f) => failed.push(json!({"kpi": key, "name": name, "code": store::code_of(&f)})),
        }
    }
    let status = if failed.is_empty() { 0 } else { 3 };
    let line = json!({"at": at, "day": day, "since": since, "until": until, "measured": measured,
        "unchanged": unchanged, "skipped": skipped, "failed": failed});
    Ok((line, status))
}
