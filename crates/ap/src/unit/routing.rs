//! The routing map (`specs/ap/routing.md`): the policy's optional member `routing` (the order of the tiers, the
//! size bound, the lowest implementer and verifier tier for each kind of work and size, and the bar on a tier's
//! stop-and-repair rate), its reader, the live bars (the open or planned findings of the detector `ap-routing`)
//! and the three tests a unit's tiers meet where `ap unit add`, `amend` and `ready` apply and where the dispatch
//! pass would start a stage: `tier_not_in_order`, `tier_below_map` and `tier_over_bar`.
use crate::host::{Failure, System, fail};
use crate::store::{self, work::values};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The detector of the bars' findings (R6).
pub const DETECTOR: &str = "ap-routing";
/// The beginning of a bar's finding key (R6).
pub const KEY_PREFIX: &str = "routing-bar-";

/// One tier of the order: `[runtime, model]`.
pub type Pair = (String, String);

/// The lowest implementer and verifier tier of one size, as indexes into the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lowest {
    pub implementer: usize,
    pub verifier: usize,
}
/// One row of the map: the lowest tiers of a small and of a large unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub small: Lowest,
    pub large: Lowest,
}
/// The bar on a tier's stop-and-repair rate (R2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub rate: f64,
    pub window_days: u64,
    pub min_merged: u64,
}
/// The member `routing` as read (R2).
#[derive(Debug, Clone, PartialEq)]
pub struct Routing {
    pub order: Vec<Pair>,
    pub large_lines: u64,
    pub map: BTreeMap<String, Row>,
    pub bar: Bar,
}

fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// The members of `v`, which must be exactly `names`.
fn exactly(v: &Value, names: &[&str], at: &str) -> Result<(), Failure> {
    let o = v
        .as_object()
        .ok_or_else(|| invalid(at, "is not an object"))?;
    for k in o.keys() {
        if !names.contains(&k.as_str()) {
            return Err(invalid(&format!("{at}.{k}"), "is not a member"));
        }
    }
    for n in names {
        if !o.contains_key(*n) {
            return Err(invalid(&format!("{at}.{n}"), "is missing"));
        }
    }
    Ok(())
}
/// A pair `[runtime, model]`: two texts of 1 to 80 bytes, as a unit's tier holds them.
fn pair(v: &Value, at: &str) -> Result<Pair, Failure> {
    fn word(w: &Value) -> Option<&str> {
        w.as_str().filter(|t| (1..=80).contains(&t.len()))
    }
    match v.as_array().map(Vec::as_slice) {
        Some([r, m]) => match (word(r), word(m)) {
            (Some(r), Some(m)) => Ok((r.to_owned(), m.to_owned())),
            _ => Err(invalid(at, "is not a pair of two texts of 1 to 80 bytes")),
        },
        _ => Err(invalid(at, "is not a pair [runtime, model]")),
    }
}
/// A tier of the map: a pair of the order, as its index.
fn tier_in(order: &[Pair], v: &Value, at: &str) -> Result<usize, Failure> {
    let p = pair(v, at)?;
    order
        .iter()
        .position(|o| *o == p)
        .ok_or_else(|| invalid(at, "is not a pair of routing.order"))
}
fn lowest(order: &[Pair], v: &Value, at: &str) -> Result<Lowest, Failure> {
    exactly(v, &["implementer", "verifier"], at)?;
    Ok(Lowest {
        implementer: tier_in(order, &v["implementer"], &format!("{at}.implementer"))?,
        verifier: tier_in(order, &v["verifier"], &format!("{at}.verifier"))?,
    })
}
fn integer(v: &Value, range: std::ops::RangeInclusive<u64>, at: &str) -> Result<u64, Failure> {
    v.as_u64().filter(|n| range.contains(n)).ok_or_else(|| {
        invalid(
            at,
            &format!(
                "is not an integer from {} to {}",
                range.start(),
                range.end()
            ),
        )
    })
}

/// The reader `routing` (R2): None when the policy has no member `routing`; `policy_invalid` (field `policy`, the
/// detail beginning with the member) for any other form than R2's.
pub fn routing(policy: &Value) -> Result<Option<Routing>, Failure> {
    let Some(r) = policy.get("routing") else {
        return Ok(None);
    };
    exactly(r, &["order", "large_lines", "map", "bar"], "routing")?;
    let list = r["order"]
        .as_array()
        .filter(|l| (2..=20).contains(&l.len()))
        .ok_or_else(|| invalid("routing.order", "is not a list of 2 to 20 pairs"))?;
    let mut order: Vec<Pair> = vec![];
    for (i, v) in list.iter().enumerate() {
        let p = pair(v, &format!("routing.order.{i}"))?;
        if order.contains(&p) {
            return Err(invalid(&format!("routing.order.{i}"), "is named twice"));
        }
        order.push(p);
    }
    let large_lines = integer(&r["large_lines"], 100..=5_000, "routing.large_lines")?;
    let rows = r["map"]
        .as_object()
        .ok_or_else(|| invalid("routing.map", "is not an object"))?;
    let mut map = BTreeMap::new();
    for (kind, row) in rows {
        let at = format!("routing.map.{kind}");
        if kind != "default" && values::UnitKind::parse(kind).is_none() {
            return Err(invalid(&at, "is not a unit kind of the schema or default"));
        }
        exactly(row, &["small", "large"], &at)?;
        let small = lowest(&order, &row["small"], &format!("{at}.small"))?;
        let large = lowest(&order, &row["large"], &format!("{at}.large"))?;
        map.insert(kind.clone(), Row { small, large });
    }
    if !map.contains_key("default") {
        return Err(invalid("routing.map.default", "is missing"));
    }
    let b = &r["bar"];
    exactly(b, &["rate", "window_days", "min_merged"], "routing.bar")?;
    let rate = b["rate"]
        .as_f64()
        .filter(|x| *x > 0.0 && *x <= 1.0)
        .ok_or_else(|| invalid("routing.bar.rate", "is not a number above 0 and at most 1"))?;
    let bar = Bar {
        rate,
        window_days: integer(&b["window_days"], 1..=92, "routing.bar.window_days")?,
        min_merged: integer(&b["min_merged"], 1..=100, "routing.bar.min_merged")?,
    };
    Ok(Some(Routing {
        order,
        large_lines,
        map,
        bar,
    }))
}

/// The member `routing` of the host's policy file for `ap unit add`, `amend` and `ready` (R3): None when the file
/// is absent or has no member; `policy_invalid` (field `policy`) when it cannot be read, is not one JSON object or
/// the member is of another form.
pub fn of_host(s: &impl System) -> Result<Option<Routing>, Failure> {
    match crate::work::policy(s)? {
        Some(p) => routing(&p),
        None => Ok(None),
    }
}

/// The key of the bar's finding of a tier and a kind (R6).
pub fn bar_key(runtime: &str, model: &str, kind: &str) -> String {
    format!("{KEY_PREFIX}{runtime}-{model}-{kind}")
}

/// The live bars (R6): the keys of the findings of the detector `ap-routing` whose key begins `routing-bar-` and
/// whose state is `open` or `planned`, from one search of findings.
pub fn live_bars(s: &mut impl System, store: Option<&str>) -> Result<BTreeSet<String>, Failure> {
    let all = vec![json!({"eq": ["detector", DETECTOR]})];
    let found = store::at(store::search(s, store, "finding", all), "findings")?;
    Ok(found
        .into_iter()
        .filter(|(k, f)| {
            k.starts_with(KEY_PREFIX)
                && f["detector"] == DETECTOR
                && (f["state"] == "open" || f["state"] == "planned")
        })
        .map(|(k, _)| k)
        .collect())
}

/// Whether the unit is checked (R3): it has a kind and both tiers, each with a runtime and a model.
pub fn checked(u: &Value) -> bool {
    let tier = |k: &str| u[k]["runtime"].is_string() && u[k]["model"].is_string();
    u["kind"].is_string() && tier("implementer") && tier("verifier")
}

fn pair_value(p: &Pair) -> Value {
    json!([p.0, p.1])
}

/// The three tests of a unit's tiers (R3 to R5), the implementer's before the verifier's: the first refusal, or
/// Ok for a unit that passes or is not checked. `live` are the live bars.
pub fn judge(r: &Routing, live: &BTreeSet<String>, u: &Value) -> Result<(), Failure> {
    if !checked(u) {
        return Ok(());
    }
    let kind = u["kind"].as_str().unwrap_or_default();
    let row = r.map.get(kind).or_else(|| r.map.get("default"));
    let Some(row) = row else {
        return Ok(());
    };
    let large = u["estimate_lines"]
        .as_u64()
        .is_some_and(|n| n >= r.large_lines);
    let lowest = if large { row.large } else { row.small };
    for (field, low) in [
        ("implementer", lowest.implementer),
        ("verifier", lowest.verifier),
    ] {
        let t = &u[field];
        let p: Pair = (
            t["runtime"].as_str().unwrap_or_default().to_owned(),
            t["model"].as_str().unwrap_or_default().to_owned(),
        );
        let Some(at) = r.order.iter().position(|o| *o == p) else {
            let order: Vec<Value> = r.order.iter().map(pair_value).collect();
            return Err(fail("tier_not_in_order", field, json!({"order": order})));
        };
        if at < low {
            return Err(fail(
                "tier_below_map",
                field,
                json!({"lowest": pair_value(&r.order[low])}),
            ));
        }
        if field == "implementer" {
            let key = bar_key(&p.0, &p.1, kind);
            if live.contains(&key) {
                let free = r.order[low..]
                    .iter()
                    .find(|o| !live.contains(&bar_key(&o.0, &o.1, kind)))
                    .map_or(Value::Null, pair_value);
                return Err(fail(
                    "tier_over_bar",
                    "implementer",
                    json!({"finding": key, "lowest": free}),
                ));
            }
        }
    }
    Ok(())
}

/// The `waits_on` of the dispatch pass for a refusal of `judge` (R8).
pub fn wait_text(f: &Failure) -> String {
    match f.code {
        "tier_over_bar" => format!(
            "tier over bar: {}",
            f.detail["finding"].as_str().unwrap_or_default()
        ),
        "tier_below_map" => "tier below map".into(),
        _ => "tier not in order".into(),
    }
}

/// The check of `ap unit add`, `amend` and `ready` (R3, R7): with the host's member `routing`, one search of the
/// findings and the three tests over the unit `u` as the op leaves it; nothing without the member or for a unit
/// without a kind and both tiers.
pub fn check_unit(s: &mut impl System, store: Option<&str>, u: &Value) -> Result<(), Failure> {
    let Some(r) = of_host(s)? else {
        return Ok(());
    };
    if !checked(u) {
        return Ok(());
    }
    let live = live_bars(s, store)?;
    judge(&r, &live, u)
}
