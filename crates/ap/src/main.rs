use std::io::{self, Write};
use std::process::ExitCode;

use serde::Serialize;

const HELP: &str = "Usage: ap <verb> [< request.json]\n";
/// Every verb the dispatch below knows: the verb, its request form, the heading of the section of
/// `crates/ap/README.md` that documents it. `ap --help` prints one line of this table per verb.
const VERBS: [(&str, &str, &str); 7] = [
    ("version", "version | --version", "The older verbs"),
    ("run", "run < request.json", "One stage"),
    ("gate", "gate < request.json", "The gate"),
    (
        "review",
        "review <start|close|revoke> < request.json",
        "One review",
    ),
    ("unit", "unit < request.json", "One unit"),
    ("deliver", "deliver < request.json", "Delivery"),
    (
        "backlog",
        "backlog <task key> [--text] [--store <path>]",
        "The board",
    ),
];
fn verb_line(verb: &str, form: &str, heading: &str) -> String {
    format!("  {verb}: ap {form} (README \"{heading}\")\n")
}
const RUN_HELP: &str = "Usage: ap run < request.json\nOne headless Claude Code stage in a transient scope of agents.slice; one record <out>.run.json.\n";
const GATE_HELP: &str = "Usage: ap gate < request.json\nWaits on a pull request's head for its required checks and the automatic review; one JSON answer.\n";
const UNIT_HELP: &str = "Usage: ap unit < request.json\nOne unit of work once from its page to the merge; one record <out>.unit.json.\n";
const DELIVER_HELP: &str = "Usage: ap deliver < request.json\nInstalls the accepted tip of main (ap and content) and records the attempt in the store; one JSON record.\n";
const BACKLOG_HELP: &str = "Usage: ap backlog <task key> [--text] [--store <path>]\nPrints the board of a task: its order, each item's refusal, the unplaced items and the untriaged rows; reads only.\n";
const BUILD_REVISION: &str = match option_env!("AP_BUILD_REVISION") {
    Some(value) => value,
    None => "unknown",
};

#[derive(Serialize)]
struct Failure<'a> {
    error: Detail<'a>,
}

#[derive(Serialize)]
struct Detail<'a> {
    code: &'a str,
    field: &'a str,
}

fn fail(code: &str, field: &str, status: u8) -> ExitCode {
    let value = Failure {
        error: Detail { code, field },
    };
    // All diagnostics are fixed codes/selectors. Never echo input or an I/O/parser error.
    if serde_json::to_writer(io::stderr().lock(), &value).is_err()
        || io::stderr().lock().write_all(b"\n").is_err()
    {
        return ExitCode::from(4);
    }
    ExitCode::from(status)
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "version") {
        let line = format!("ap {} {}\n", env!("CARGO_PKG_VERSION"), BUILD_REVISION);
        return if io::stdout().lock().write_all(line.as_bytes()).is_ok() {
            ExitCode::SUCCESS
        } else {
            fail("io_error", "stdout", 4)
        };
    }
    // The verbs without a help of their own answer with their line of the table.
    if args.len() == 2
        && args[1] == "--help"
        && let Some((verb, form, heading)) = VERBS
            .iter()
            .find(|(verb, ..)| *verb == "version" && args[0] == *verb)
    {
        return match io::stdout()
            .lock()
            .write_all(verb_line(verb, form, heading).as_bytes())
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => fail("io_error", "stdout", 4),
        };
    }
    if args.first().is_some_and(|arg| arg == "review") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout()
                .lock()
                .write_all(b"Usage: ap review <start|close|revoke> < request.json\n")
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::review::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "gate") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(GATE_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::gate::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "run") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(RUN_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::run::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "unit") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(UNIT_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::unit::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "deliver") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(DELIVER_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::deliver::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "backlog") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(BACKLOG_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::backlog::cli(&args[1..]);
    }
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        let mut text = format!("{HELP}Verbs; the section is of crates/ap/README.md:\n");
        for (verb, form, heading) in VERBS {
            text.push_str(&verb_line(verb, form, heading));
        }
        return if io::stdout().lock().write_all(text.as_bytes()).is_ok() {
            ExitCode::SUCCESS
        } else {
            fail("io_error", "stdout", 4)
        };
    }
    fail("usage", "args", 2)
}
