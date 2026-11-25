//! `ap measure` and the functions `cost`, `flow` and `share` (`specs/ap/measure.md`): the pass `sessions`, which
//! makes every Claude Code session of this platform's agents and stages on this host a `run` of the store with its
//! list-price spend, refusals, reads and writes by UTC day (`measure/sessions.rs`, the counts of a day in
//! `measure/count.rs`), the functions `cost`, `flow` and `share`, which answer from those runs and the merged units
//! (`measure/cost.rs`, `measure/flow.rs`, `measure/share.rs`), and the pass `kpis`, which records each KPI of a
//! confirmed intent once a day from them (`measure/kpis.rs`), and the pass `routing`, which opens and closes the
//! findings that bar a tier of the routing map from a kind of work (`measure/routing.rs`, `specs/ap/routing.md`).
mod cost;
mod count;
mod flow;
mod kpis;
mod routing;
mod sessions;
mod share;

use crate::host::{Failure, Host, LIMIT, System, error};
use crate::route::{print, refusal};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::{Read, Write},
    process::ExitCode,
};

pub use count::{Counts, of_bash, of_tool, refusals};
pub use kpis::{TABLE, evidence_text, value_text};
pub use sessions::academy;

/// The usage of every pass (R1).
pub const HELP: &str = "Usage: ap measure sessions [--since <YYYY-MM-DD>] [--accounts <dir>]... [--work-root <dir>] [--platforms-root <dir>] [--stage-dir <dir>]... [--prices <file>] [--store <path>]\nOne pass: reads the Claude Code session records of each account (projects/**/*.jsonl and homes/*/projects/**/*.jsonl), the stage records and the door's record, and makes every session of route engineering a run of the store (record_session, extend_session) with its list-price spend, refusals, reads and writes by UTC day (measure_run); one JSON line. Default --since: two days before today (UTC).\nUsage: ap measure kpis [--day <YYYY-MM-DD>] [--store <path>]\nOne pass: records once a day one measurement of each KPI of a confirmed intent whose name is in the table of specs/ap/measure.md, over the seven days before --day (default: today, UTC), from the functions cost, flow and share (record_measurement); one JSON line. A KPI measured that day is left as it is.\nUsage: ap measure routing [--day <YYYY-MM-DD>] [--store <path>]\nOne pass, with the policy's member routing: over the bar's window before --day (default: today, UTC), each tier's stop-and-repair rate on each kind of work from the function flow by tier over the confirmed intents, and the findings of the detector ap-routing that bar a tier above the rate (open_finding, see_finding_again, reopen_finding, close_finding); one JSON line. ap with no argument and ONT_FN_NAME=cost, flow or share answers that function of the store.\n";

/// The exit status of a refusal or a failure: 2 for what the caller asked wrongly, 3 for the store (R2, R9).
pub fn exit_of(f: &Failure) -> u8 {
    match f.code {
        "usage" | "invalid_input" | "unknown_intent" | "input_too_large" | "policy_invalid" => 2,
        _ => 3,
    }
}

/// `ap measure <pass> [<option>...]` against the port `s` (R1): the answer line and its exit status, or the
/// refusal or failure.
pub fn run(s: &mut impl System, argv: &[OsString]) -> Result<(Value, u8), Failure> {
    match argv.split_first() {
        Some((pass, rest)) if pass == "sessions" => sessions::pass(s, rest),
        Some((pass, rest)) if pass == "kpis" => kpis::pass(s, rest),
        Some((pass, rest)) if pass == "routing" => routing::pass(s, rest),
        _ => Err(error("usage", "args")),
    }
}

/// `ap measure <arguments>`: `--help` alone prints the usage; else one pass.
pub fn cli(argv: &[OsString]) -> ExitCode {
    if matches!(argv, [h] if h == "--help") {
        return match std::io::stdout().write_all(HELP.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(4),
        };
    }
    match run(&mut Host, argv) {
        Ok((v, status)) => print(&v, true, status),
        Err(e) => print(&refusal(&e), false, exit_of(&e)),
    }
}

/// The answer of the function `name` over the store `store` (the engine's default when none) with the parameters `p`
/// (R9 to R13); a name that is not `cost`, `flow` or `share` is `usage` (field `ONT_FN_NAME`).
pub(crate) fn answer_in(
    s: &mut impl System,
    name: &str,
    store: Option<&str>,
    p: &Value,
) -> Result<Value, Failure> {
    match name {
        "cost" => cost::cost(s, store, p),
        "flow" => flow::flow(s, store, p),
        "share" => share::share(s, store, p),
        _ => Err(error("usage", "ONT_FN_NAME")),
    }
}

/// The answer of the function `name` over the store `store` with the parameters `p` (R9 to R13).
pub fn answer(s: &mut impl System, name: &str, store: &str, p: &Value) -> Result<Value, Failure> {
    answer_in(s, name, Some(store), p)
}

/// `ap` with no argument and `ONT_FN_NAME` `cost`, `flow` or `share` (R9): the parameters on standard input, the
/// store at `ONT_FN_STORE`, the answer on standard output; a refusal on standard error with exit 2, a failure of the
/// store with exit 3.
pub fn function(name: &str) -> ExitCode {
    let result = (|| {
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error("io_error", "stdin"))?;
        if bytes.len() as u64 > LIMIT {
            return Err(error("input_too_large", "stdin"));
        }
        let p: Value =
            serde_json::from_slice(&bytes).map_err(|_| error("invalid_input", "stdin"))?;
        let store = std::env::var("ONT_FN_STORE")
            .ok()
            .filter(|v| v.starts_with('/'))
            .ok_or_else(|| error("invalid_input", "ONT_FN_STORE"))?;
        answer(&mut Host, name, &store, &p)
    })();
    match result {
        Ok(v) => print(&v, true, 0),
        Err(e) => print(&refusal(&e), false, exit_of(&e)),
    }
}
