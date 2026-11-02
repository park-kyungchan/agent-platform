use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};
use std::process::ExitCode;

use super::{ApplyRequest, RecoverRequest, Spec};

const HELP: &str = "Usage: ap install plan|apply|recover < request.json\nap doctor inspect < spec.json\nExplicit Linux profiles/source; disposable or externally coordinated writers.\nNo hook execution; native loading requires separate evidence.\n";

fn fail(code: &str, field: &str, status: u8) -> ExitCode {
    let error = serde_json::json!({"error":{"code":code,"field":field}});
    let mut bytes = serde_json::to_vec(&error).unwrap_or_default();
    bytes.push(b'\n');
    if io::stderr().lock().write_all(&bytes).is_err() {
        return ExitCode::from(4);
    }
    ExitCode::from(status)
}
fn read<T: DeserializeOwned>() -> std::result::Result<T, ExitCode> {
    let mut bytes = Vec::new();
    if io::stdin()
        .lock()
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(fail("io_error", "stdin", 4));
    }
    if bytes.len() > 1_048_576 {
        return Err(fail("input_too_large", "stdin", 2));
    }
    serde_json::from_slice(&bytes).map_err(|_| fail("invalid_input", "stdin", 2))
}
fn write<T: Serialize>(value: &T, status: u8) -> ExitCode {
    let mut bytes = match serde_json::to_vec(value) {
        Ok(bytes) => bytes,
        Err(_) => return fail("internal_error", "stdout", 70),
    };
    bytes.push(b'\n');
    if io::stdout().lock().write_all(&bytes).is_err() {
        return fail("io_error", "stdout", 4);
    }
    ExitCode::from(status)
}
/// Called only for additive two-word native commands. S02 admission stays intact.
pub fn dispatch(args: &[std::ffi::OsString]) -> Option<ExitCode> {
    if args.len() != 2 {
        return None;
    }
    if args[0] == "install" && args[1] == "--help" {
        return Some(if io::stdout().lock().write_all(HELP.as_bytes()).is_ok() {
            ExitCode::SUCCESS
        } else {
            fail("io_error", "stdout", 4)
        });
    }
    let selected = if args[0] == "install" && args[1] == "plan" {
        0
    } else if args[0] == "install" && args[1] == "apply" {
        1
    } else if args[0] == "install" && args[1] == "recover" {
        2
    } else if args[0] == "doctor" && args[1] == "inspect" {
        3
    } else {
        return None;
    };
    Some(match selected {
        0 => match read::<Spec>() {
            Err(code) => code,
            Ok(spec) => match super::plan(&spec) {
                Ok(value) => write(&value, 0),
                Err(e) => fail(&e.code, &e.field, 3),
            },
        },
        1 => match read::<ApplyRequest>() {
            Err(code) => code,
            Ok(request) => match super::apply(&request) {
                Ok(value) => {
                    let status = if value.outcome == "installed" { 0 } else { 5 };
                    write(&value, status)
                }
                Err(e) => fail(&e.code, &e.field, 3),
            },
        },
        2 => match read::<RecoverRequest>() {
            Err(code) => code,
            Ok(request) => match super::recover(&request) {
                Ok(value) => {
                    let status = if value.outcome == "recovered" { 0 } else { 5 };
                    write(&value, status)
                }
                Err(e) => fail(&e.code, &e.field, 3),
            },
        },
        _ => match read::<Spec>() {
            Err(code) => code,
            Ok(spec) => match super::doctor(&spec) {
                Ok(value) => write(&value, 0),
                Err(e) => fail(&e.code, &e.field, 3),
            },
        },
    })
}
