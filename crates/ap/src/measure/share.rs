//! The function `share` (`specs/ap/measure.md` R12, R13): the share of the write steps done as store actions and of
//! the fact reads done as store queries, by role and for the groups of stages and of sessions, summed from the `days`
//! the pass `sessions` stored. It reads the store only.
use super::flow::parameters;
use crate::host::{Failure, System, error};
use crate::store;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const STAGES: [&str; 5] = ["implementer", "fixer", "verifier", "reviewer", "writer"];

#[derive(Default, Clone, Copy)]
struct Tally {
    runs: u64,
    file_reads: u64,
    store_reads: u64,
    file_writes: u64,
    store_writes: u64,
}
impl Tally {
    fn add(&mut self, o: &Tally) {
        self.runs += o.runs;
        self.file_reads = self.file_reads.saturating_add(o.file_reads);
        self.store_reads = self.store_reads.saturating_add(o.store_reads);
        self.file_writes = self.file_writes.saturating_add(o.file_writes);
        self.store_writes = self.store_writes.saturating_add(o.store_writes);
    }
    fn value(&self, name: (&str, &str)) -> Value {
        json!({name.0: name.1, "runs": self.runs, "file_reads": self.file_reads, "store_reads": self.store_reads,
            "file_writes": self.file_writes, "store_writes": self.store_writes,
            "read_share": percent(self.store_reads, self.file_reads),
            "write_share": percent(self.store_writes, self.file_writes)})
    }
}
/// The store's count over the store's plus the file's, in percent to one decimal; null at 0.
fn percent(store: u64, file: u64) -> Value {
    match store.saturating_add(file) {
        0 => Value::Null,
        t => json!((store as f64 / t as f64 * 1000.0).round() / 10.0),
    }
}

/// `share` over the store `st` (the engine's default when none) with the parameters `p`.
pub(super) fn share(s: &mut impl System, st: Option<&str>, p: &Value) -> Result<Value, Failure> {
    let (since, until, intent) = parameters(p, false)?;
    if let Some(i) = &intent
        && store::get(s, st, "intent", i)?.is_none()
    {
        return Err(error("unknown_intent", "intent"));
    }
    let (runs, _) = store::search_every(s, st, "run", store::PAGE)?;
    let units: BTreeMap<String, Value> = match intent {
        Some(_) => store::search_every(s, st, "unit", store::PAGE)?
            .0
            .into_iter()
            .collect(),
        None => BTreeMap::new(),
    };
    let key_of = |v: &Value| v["native_key"].as_str().map(str::to_owned);
    let intent_of = |f: &Value| -> Option<String> {
        key_of(&f["intent"]).or_else(|| key_of(&units.get(&key_of(&f["unit"])?)?["intent"]))
    };
    let in_window = |d: &str| since.as_str() <= d && d <= until.as_str();
    let mut roles: BTreeMap<String, Tally> = BTreeMap::new();
    for (_, f) in runs
        .iter()
        .filter(|(_, f)| intent.is_none() || intent_of(f) == intent)
    {
        let count = |d: &Value, m: &str| d[m].as_u64().unwrap_or(0);
        let mut mine = Tally::default();
        for d in f["days"].as_array().into_iter().flatten() {
            if !d["day"].as_str().is_some_and(in_window) {
                continue;
            }
            mine.runs = 1;
            mine.file_reads = mine.file_reads.saturating_add(count(d, "file_reads"));
            mine.store_reads = mine.store_reads.saturating_add(count(d, "store_reads"));
            mine.file_writes = mine.file_writes.saturating_add(count(d, "file_writes"));
            mine.store_writes = mine.store_writes.saturating_add(count(d, "store_writes"));
        }
        if mine.runs > 0 {
            let role = f["role"].as_str().unwrap_or_default().to_owned();
            roles.entry(role).or_default().add(&mine);
        }
    }
    let (mut stages, mut sessions) = (Tally::default(), Tally::default());
    for (role, t) in &roles {
        match (STAGES.contains(&role.as_str()), role.as_str()) {
            (true, _) => stages.add(t),
            (false, "batch") => {}
            (false, _) => sessions.add(t),
        }
    }
    Ok(json!({"since": since, "until": until, "intent": intent,
        "roles": roles.iter().map(|(r, t)| t.value(("role", r))).collect::<Vec<_>>(),
        "groups": [stages.value(("group", "stages")), sessions.value(("group", "sessions"))]}))
}
