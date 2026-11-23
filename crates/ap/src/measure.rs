//! `ap measure` and the functions `cost`, `flow` and `share` (`specs/ap/measure.md`): the pass `sessions`, which
//! makes every Claude Code session of this platform's agents and stages on this host a `run` of the store with its
//! list-price spend and refusals by UTC day (`measure/sessions.rs`, the counts of a day in `measure/count.rs`), and
//! the function `cost`, which answers from those runs (`measure/cost.rs`).
mod cost;
mod count;
mod sessions;

use crate::host::{Failure, Host, LIMIT, System, error};
use crate::route::{print, refusal};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::{Read, Write},
    process::ExitCode,
};

pub use count::{Counts, of_bash, of_tool, refusals};
pub use sessions::academy;

/// The usage of every pass (R1).
pub const HELP: &str = "Usage: ap measure sessions [--since <YYYY-MM-DD>] [--accounts <dir>]... [--work-root <dir>] [--platforms-root <dir>] [--stage-dir <dir>]... [--prices <file>] [--store <path>]\nOne pass: reads the Claude Code session records of each account (projects/**/*.jsonl and homes/*/projects/**/*.jsonl), the stage records and the door's record, and makes every session of route engineering a run of the store (record_session, extend_session) with its list-price spend, refusals, reads and writes by UTC day (measure_run); one JSON line. Default --since: two days before today (UTC). ap with no argument and ONT_FN_NAME=cost answers the function cost of the store; flow and share are not built yet (specs/ap/measure.md).\n";

/// The exit status of a refusal or a failure: 2 for what the caller asked wrongly, 3 for the store (R2, R9).
pub fn exit_of(f: &Failure) -> u8 {
    match f.code {
        "usage" | "invalid_input" | "unknown_intent" | "function_not_built" | "input_too_large" => {
            2
        }
        _ => 3,
    }
}

/// `ap measure <pass> [<option>...]` against the port `s` (R1): the answer line and its exit status, or the
/// refusal or failure.
pub fn run(s: &mut impl System, argv: &[OsString]) -> Result<(Value, u8), Failure> {
    match argv.split_first() {
        Some((pass, rest)) if pass == "sessions" => sessions::pass(s, rest),
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

/// The answer of the function `name` over the store `store` with the parameters `p` (R9 to R11).
pub fn answer(s: &mut impl System, name: &str, store: &str, p: &Value) -> Result<Value, Failure> {
    match name {
        "cost" => cost::cost(s, store, p),
        _ => Err(error("function_not_built", "ONT_FN_NAME")),
    }
}

/// `ap` with no argument and `ONT_FN_NAME` `cost`, `flow` or `share` (R9, R11): the parameters on standard input,
/// the store at `ONT_FN_STORE`, the answer on standard output; a refusal on standard error with exit 2, a failure of
/// the store with exit 3. `flow` and `share` are refused before anything is read.
pub fn function(name: &str) -> ExitCode {
    let result = (|| {
        if name != "cost" {
            return Err(error("function_not_built", "ONT_FN_NAME"));
        }
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
