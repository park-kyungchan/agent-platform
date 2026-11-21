//! `ap policy check`: the host's policy file run through every reader `ap` has of it, without an
//! effect (`specs/ap/policy.md` R12 to R16).
use crate::host::{Answer, Failure, Host, System, error};
use crate::unit::{drive, object, page_review};
use crate::{quota, run, work};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

/// The readers, in the order the check runs them (R12).
const READERS: [&str; 7] = [
    "models",
    "quota",
    "units",
    "reviews",
    "repositories",
    "work",
    "check",
];

/// The file as read once: every reader meets this answer for the path, never the file system.
struct Once {
    file: PathBuf,
    read: Result<Vec<u8>, ErrorKind>,
}
impl System for Once {
    fn call(&mut self, program: &str, _: &[String], _: &Value, _: u64) -> Answer {
        Err(error("process_failed", program))
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if Path::new(path) != self.file {
            return Err(ErrorKind::InvalidInput.into());
        }
        self.read.clone().map_err(Into::into)
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(self.file.clone())
    }
}

/// A refusal and the reader that gave it (None: the check's own, before any reader).
#[derive(Debug)]
pub struct Refusal {
    pub reader: Option<&'static str>,
    pub failure: Failure,
}

/// The check (R12): the file at `file`, else the host's path, through every reader, stopping at
/// the first refusal. `s` is used for the path and the one read.
pub fn check(s: &impl System, file: Option<&Path>) -> Result<Value, Refusal> {
    let absent = || Refusal {
        reader: None,
        failure: error("policy_absent", "policy"),
    };
    let file = match file {
        Some(f) => f.to_owned(),
        None => s.policy_file().ok_or_else(absent)?,
    };
    let read = match file.to_str() {
        Some(path) => s.read(path).map_err(|e| e.kind()),
        // The readers name this refusal themselves, unless nothing is there.
        None if std::fs::metadata(&file).is_err_and(|e| e.kind() == ErrorKind::NotFound) => {
            Err(ErrorKind::NotFound)
        }
        None => Err(ErrorKind::InvalidData),
    };
    if read == Err(ErrorKind::NotFound) {
        return Err(absent());
    }
    let sha256 = read.as_ref().ok().map(|b| crate::host::digest(b));
    let w = Once { file, read };
    let at = |reader, result: Result<(), Failure>| {
        result.map_err(|failure| Refusal {
            reader: Some(reader),
            failure,
        })
    };
    at("models", run::policy(&w).map(drop))?;
    at("quota", quota::config(&w).map(drop))?;
    // The same reading of the file as one object, for the readers of a member.
    let value = work::policy(&w).map_err(|failure| Refusal {
        reader: Some("models"),
        failure,
    })?;
    let value = value.unwrap_or_default();
    at("units", drive::units(&value).map(drop))?;
    at("reviews", page_review::policy(&w).map(drop))?;
    match value.get("repositories") {
        None => {}
        Some(Value::Object(all)) => {
            for key in all.keys() {
                at("repositories", object::clone_of(&w, key).map(drop))?;
            }
        }
        // Not an object: the reader refuses for any name.
        Some(_) => at("repositories", object::clone_of(&w, "").map(drop))?,
    }
    at("work", work::work(&value).map(drop))?;
    at("check", crate::check::hosts(&value).map(drop))?;
    let path = w.file.to_str().unwrap_or_default();
    Ok(json!({"status": "ok", "path": path, "sha256": sha256, "readers": READERS}))
}

pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = match argv {
        [word] if word == "check" => check(&Host, None),
        [word, file] if word == "check" => check(&Host, Some(Path::new(file))),
        _ => Err(Refusal {
            reader: None,
            failure: error("usage", "args"),
        }),
    };
    let (value, status) = match result {
        Ok(v) => (v, 0),
        Err(Refusal { reader, failure: e }) => (
            json!({"error": {"code": e.code, "field": e.field, "detail": e.detail, "reader": reader}}),
            2,
        ),
    };
    let written = if status == 0 {
        writeln!(std::io::stdout(), "{value}")
    } else {
        writeln!(std::io::stderr(), "{value}")
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
