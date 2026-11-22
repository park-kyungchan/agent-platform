use std::io::{self, Write};
use std::process::ExitCode;

use serde::Serialize;

const HELP: &str = "Usage: ap <verb> [< request.json]\n";
/// Every verb the dispatch below knows: the verb, its request form, the heading of the section of
/// `crates/ap/README.md` that documents it. `ap --help` prints one line of this table per verb.
const VERBS: [(&str, &str, &str); 21] = [
    ("version", "version | --version", "The older verbs"),
    ("run", "run < request.json", "One stage"),
    ("gate", "gate < request.json", "The gate"),
    (
        "review",
        "review <start|close|revoke> < request.json",
        "One review",
    ),
    (
        "unit",
        "unit [<page>] < request.json | unit add|amend|ready|drop [--store <path>] < request.json | unit page <key> [--for implementer|verifier|reviewer] [--digest] [--store <path>] | unit review [--store <path>] < request.json | unit run <key> [--store <path>] | unit write <key> [--store <path>] | unit approve|reject [--store <path>] < request.json",
        "One unit",
    ),
    ("accept", "accept <page>", "The page's block"),
    ("deliver", "deliver [check] < request.json", "Delivery"),
    (
        "backlog",
        "backlog <task key> [--text] [--store <path>]",
        "The board",
    ),
    (
        "state",
        "state <task directory> [--text]",
        "The task's state",
    ),
    ("row", "row [--store <path>] < row.json", "One row"),
    (
        "request",
        "request [--store <path>] [--text] < request.json",
        "One request",
    ),
    (
        "intent",
        "intent [--store <path>] [--text] < request.json",
        "One intent",
    ),
    (
        "status",
        "status [<key> [--requests] | --term <word>] [--text] [--store <path>], or ap status --brief <intent> [--store <path>], or ap status --wait <intent> [--timeout <seconds>] [--store <path>], or ap status --indicators [<intent>] [--text] [--store <path>]",
        "The status",
    ),
    ("quota", "quota", "The CPU quota"),
    ("work", "work [--store <path>]", "The dispatch pass"),
    ("policy", "policy check [<file>]", "The policy check"),
    (
        "watch",
        "watch [--record-login --totp-file <path> [--replace]]",
        "The watch",
    ),
    (
        "check",
        "check <dir> [--timeout <seconds>] [-- <command> [<arg>...]]",
        "One check",
    ),
    ("fn", "fn <name> < request.json", "The door"),
    (
        "route",
        "route index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]... [--instructions <dir>]",
        "Routes",
    ),
    (
        "clause",
        "clause index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]...",
        "Clauses",
    ),
];
fn verb_line(verb: &str, form: &str, heading: &str) -> String {
    format!("  {verb}: ap {form} (README \"{heading}\")\n")
}
const RUN_HELP: &str = "Usage: ap run < request.json\nOne headless Claude Code stage in a transient scope of agents.slice; one record <out>.run.json.\n";
const GATE_HELP: &str = "Usage: ap gate < request.json\nWaits on a pull request's head for its required checks, and for an automatic review when the request names its reviewer; one JSON answer.\n";
const UNIT_HELP: &str = "Usage: ap unit < request.json | ap unit [--compose-only] <page>\n       ap unit add|amend|ready|drop [--store <path>] < request.json\n       ap unit page <key> [--for implementer|verifier|reviewer] [--digest] [--store <path>]\n       ap unit review [--store <path>] < request.json\n       ap unit run <key> [--store <path>]\n       ap unit write <key> [--store <path>]\n       ap unit approve|reject [--store <path>] < request.json\nOne unit of work once from its page to the merge; one record <out>.unit.json. With a page, the request and prompts are composed from it and units.json; --compose-only prints them only.\nadd, amend, ready and drop write the unit's page as an object of the store (ready after its acceptance checks ran on the base); page prints the rendered page, or with --digest its SHA-256; review has the page of a unit or the plan of an intent reviewed by a fresh session and records the review; run drives one unit of the store to stopped, accepted or merged, its repairs and updates inside, and removes the worktree of an ended unit; approve and reject record the Lead's word on an accepted unit; write has a fresh session write the page of a draft unit inside its write set, applies it as amend does and has it reviewed (README \"The unit object\", \"The page writer\").\n";
const DELIVER_HELP: &str = "Usage: ap deliver [check] < request.json\nInstalls the accepted tip of main (ap and content) and records the attempt in the store; one JSON record. With check it only checks the request and prints {\"status\":\"ok\"} or the refusal record; it writes nothing.\n";
const BACKLOG_HELP: &str = "Usage: ap backlog <task key> [--text] [--store <path>]\nPrints the board of a task: its order, each item's refusal, the unplaced items and the untriaged rows; reads only.\n";
const STATE_HELP: &str = "Usage: ap state <task directory> [--text]\nPrints the unit calls, stages, limits and streams of a task directory from its records and Git; reads only.\n";
const REQUEST_HELP: &str = "Usage: ap request [--store <path>] [--text] < request.json\nRecords a message of the user as a request of the store with its words, routes it, or lists the current requests of a topic; one JSON answer.\n";
const INTENT_HELP: &str = "Usage: ap intent [--store <path>] [--text] < request.json\nDrafts, amends, leads, confirms and closes the intent of an objective with its success criteria, or shows it; one JSON answer.\n";
const STATUS_HELP: &str = "Usage: ap status [<key> [--requests] | --term <word>] [--text] [--store <path>]\n       ap status --brief <intent> [--store <path>]\n       ap status --wait <intent> [--timeout <seconds>] [--store <path>]\n       ap status --indicators [<intent>] [--text] [--store <path>]\nPrints what is asked, planned, running, waiting and broken from the store alone, one intent or unit in full (an intent with its count of requests and the newest five, or all of them with --requests), or what a word of the life cycle means; reads only. With --brief it prints at most 40 short lines on an intent: its status and Lead, criteria, units that are not ended, what needs the Lead, holds and the last delivery. With --wait it returns, in one JSON answer, when a unit of the intent needs the Lead (unit_accepted, unit_stopped, unit_idle), a finding of severity act or urgent opens, every unit has ended, or the timeout (30 to 86400 seconds, default 14400) passes. With --indicators it prints, for each merged unit, its minutes from the first start to the merge, review rounds, repairs, first verdict, stop causes and tokens by role, and their medians; reads only.\n";
const QUOTA_HELP: &str = "Usage: ap quota\nOne turn: sets the runtime CPU quota of the slice the policy file names to what the host's busy limit leaves after the load outside it; one JSON answer.\n";
const WORK_HELP: &str = "Usage: ap work [--store <path>]\nOne dispatch pass: starts each unit of the store that may start now as a run of ap unit run in a transient unit of the service manager, and notes on every ready unit that does not start what it waits on; one JSON answer.\n";
const POLICY_HELP: &str = "Usage: ap policy check [<file>]\nRuns the policy file (the given one, else AP_POLICY_FILE, else /etc/agent-platform/policy.json) through every reader ap has of it and prints {\"status\":\"ok\",...} or the first refusal with its reader; it writes nothing.\n";
const WATCH_HELP: &str = "Usage: ap watch [--record-login --totp-file <path> [--replace]]\nOne pass of the detectors delivery, login, services, host and records: opens, sees again, reopens and closes the findings of the store whose detector is watch:<name>, and prints one JSON answer. With --record-login it records the login reading the detector login compares with (login.json) and prints it.\n";
const CHECK_HELP: &str = "Usage: ap check <dir> [--timeout <seconds>] [-- <command> [<arg>...]]\nRuns a repository's check (default ./check; a given command without a shell) for the worktree <dir>: on the local PC when the policy file names it in hosts.pc and it answers, is not busy and has at least hosts.pc.min_available_mb (default 2048) MB available (the smaller of its Linux's MemAvailable and Windows' free memory), else in <dir> on this host. The output streams back, the exit status is the command's (124 at the timeout, 1 to 14400 seconds, default 2700), and one record line is appended to AP_CHECKS_LOG (default /var/lib/agent-platform/checks.jsonl), with what the probe read of the PC and the lowest MemAvailable of its Linux while the command ran there.\n";
const ROUTE_HELP: &str = "Usage: ap route index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]... [--instructions <dir>]\nWrites one record per heading of each *.md file at origin/main of each repository named (after git fetch origin main) and of the instructions directory on disk, and one per component of each repository's map.json, into the directory the store's sources sections and components read (default /var/lib/agent-platform/route; another one only with --store, a scratch store), removes with --prune the files of repositories no longer named, runs ont index of the two sources and prints one JSON line. ap with no argument and ONT_FN_NAME=route in its environment answers the function route of the store (specs/ap/route.md).\n";
const CLAUSE_HELP: &str = "Usage: ap clause index [--directory <dir>] [--store <path>] [--prune] [--repository <owner/name>]...\nWrites one record per numbered sentence (- R<n>:) of each specs/**/*.md file at origin/main of each repository named (after git fetch origin main), with the tests that cite it, into the directory the store's source clauses reads (default /var/lib/agent-platform/clause; another one only with --store, a scratch store), removes with --prune the files of repositories no longer named, runs ont index of the source and prints one JSON line. ap with no argument and ONT_FN_NAME=clause in its environment answers the function clause of the store (specs/ap/clause.md).\n";
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
    // The verbs without a help of their own (`version`, `accept`, `row`) answer with their line of the table.
    if args.len() == 2
        && args[1] == "--help"
        && let Some((verb, form, heading)) = VERBS
            .iter()
            .find(|(verb, ..)| matches!(*verb, "version" | "accept" | "row") && args[0] == *verb)
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
    if args.first().is_some_and(|arg| arg == "accept") {
        return ap::page::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "row") {
        return ap::row::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "request") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(REQUEST_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::request::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "intent") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(INTENT_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::intent::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "status") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(STATUS_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::status::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "quota") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(QUOTA_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::quota::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "work") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(WORK_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::work::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "policy") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(POLICY_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::policy::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "watch") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(WATCH_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::watch::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "check") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(CHECK_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::check::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "fn") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(ap::door::FN_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::door::cli(&args[1..]);
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
    if args.first().is_some_and(|arg| arg == "state") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(STATE_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::state::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "route") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(ROUTE_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::route::cli(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "clause") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(CLAUSE_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::clause::cli(&args[1..]);
    }
    // No argument: the function the store's engine names in the environment (`specs/ont/fn.md` R8), when `ap`
    // answers it; else the usage refusal below.
    if args.is_empty() && std::env::var_os("ONT_FN_NAME").is_some_and(|name| name == "route") {
        return ap::route::function();
    }
    if args.is_empty() && std::env::var_os("ONT_FN_NAME").is_some_and(|name| name == "horizon") {
        return ap::work::function();
    }
    if args.is_empty() && std::env::var_os("ONT_FN_NAME").is_some_and(|name| name == "clause") {
        return ap::clause::function();
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
