use std::io::{self, Read, Write};
use std::process::ExitCode;

use ap::{ErrorCode, RenderRequest};
use serde::Serialize;

const INPUT_LIMIT: u64 = 1_048_576;
const HELP: &str = "Usage: ap render < request.json\nPure JSON proposal on stdout; no file access or installation.\n";
const WORK_HELP: &str = "Usage: ap work <start|wait|close> < request.json\nOne JSON request; admission through ont and lifecycle through herdr.\n";
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
    if args.first().is_some_and(|arg| arg == "work") {
        if args.len() == 2 && args[1] == "--help" {
            return match io::stdout().lock().write_all(WORK_HELP.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => fail("io_error", "stdout", 4),
            };
        }
        return ap::work::cli(&args[1..]);
    }
    #[cfg(target_os = "linux")]
    if args.first().is_some_and(|arg| arg == "pane") {
        return ap::pane::cli(&args[1..]);
    }
    #[cfg(target_os = "linux")]
    if let Some(result) = ap::native::cli::dispatch(&args) {
        return result;
    }
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return if io::stdout().lock().write_all(HELP.as_bytes()).is_ok() {
            ExitCode::SUCCESS
        } else {
            fail("io_error", "stdout", 4)
        };
    }
    if args.len() != 1 || args[0] != "render" {
        return fail("usage", "args", 2);
    }
    let mut input = Vec::new();
    if io::stdin()
        .lock()
        .take(INPUT_LIMIT + 1)
        .read_to_end(&mut input)
        .is_err()
    {
        return fail("io_error", "stdin", 4);
    }
    if input.len() as u64 > INPUT_LIMIT {
        return fail("input_too_large", "stdin", 2);
    }
    let request: RenderRequest = match serde_json::from_slice(&input) {
        Ok(request) => request,
        Err(_) => return fail("invalid_input", "stdin", 2),
    };
    match ap::render(&request) {
        Ok(plan) => {
            let mut output = match serde_json::to_vec(&plan) {
                Ok(bytes) => bytes,
                Err(_) => return fail("internal_error", "stdout", 70),
            };
            output.push(b'\n');
            if io::stdout().lock().write_all(&output).is_err() {
                return fail("io_error", "stdout", 4);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            let status = if error.code == ErrorCode::RendererUnimplemented {
                70
            } else {
                3
            };
            // Serialization supplies the enum spelling without a second hand-maintained map.
            let code = match serde_json::to_value(&error.code) {
                Ok(serde_json::Value::String(code)) => code,
                _ => return fail("internal_error", "renderer", 70),
            };
            fail(&code, &error.field, status)
        }
    }
}
