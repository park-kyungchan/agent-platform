//! `ap policy check` against a fake host (`specs/ap/policy.md` R12 to R16): the fake answers the one
//! read of the file and refuses every program call; nothing here touches the host's policy file.
use ap::host::{Answer, Failure, System};
use ap::policy::{Refusal, check};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const PATH: &str = "/scratch/policy.json";

struct Fake {
    file: Option<Result<Vec<u8>, ErrorKind>>,
    reads: Cell<u32>,
    calls: Vec<String>,
}
impl Fake {
    fn with(bytes: &[u8]) -> Self {
        Fake {
            file: Some(Ok(bytes.to_vec())),
            reads: Cell::new(0),
            calls: vec![],
        }
    }
    fn json(v: &Value) -> Self {
        Self::with(v.to_string().as_bytes())
    }
}
impl System for Fake {
    fn call(&mut self, program: &str, _: &[String], _: &Value, _: u64) -> Answer {
        self.calls.push(program.to_owned());
        Err(Failure {
            code: "process_failed",
            field: program.to_owned(),
            detail: Value::Null,
        })
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        assert_eq!(path, PATH, "a reader read another path");
        self.reads.set(self.reads.get() + 1);
        match &self.file {
            Some(Ok(b)) => Ok(b.clone()),
            Some(Err(k)) => Err((*k).into()),
            None => Err(ErrorKind::NotFound.into()),
        }
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(PATH.into())
    }
}

/// The shape of the host's policy file: every member any reader reads.
fn host() -> Value {
    json!({
        "models": {"allowed": ["claude-opus-4-1", "sonnet"], "never": ["muse-spark-*"]},
        "accounts": {
            "claude": ["/root/.claude"],
            "claude/sonnet": ["/root/.claude-a", "/root/.claude-b"]
        },
        "quota": {"slice": "agents.slice", "busy_limit": 95, "margin": 5, "floor": 60, "ceiling": 170},
        "units": {"directory": "/var/lib/units", "timeouts": {"implementer": 3600}},
        "reviews": {"runtime": "claude", "model": "opus", "account": "/root/.claude", "task": "t"},
        "repositories": {
            "park-kyungchan/agent-platform": {"clone": "/srv/agent-platform"},
            "park-kyungchan/other": {"clone": "/srv/other"}
        },
        "work": {"slots": 3, "slice": "agents.slice", "prefix": "ap-unit"},
        "hosts": {"pc": {"ssh": "u@pc", "work": "/home/u/work/ap-checks", "busy_load": 0.75}}
    })
}

/// The refusal of the host's shape with `member` set to `value`: (reader, code, field, detail).
fn refused(member: &str, value: Value) -> (Option<&'static str>, String, String, Value) {
    let mut p = host();
    p[member] = value;
    let fake = Fake::json(&p);
    let Refusal { reader, failure } = check(&fake, None).unwrap_err();
    assert_eq!(fake.reads.get(), 1, "the file is read once");
    assert!(fake.calls.is_empty());
    (
        reader,
        failure.code.to_owned(),
        failure.field,
        failure.detail,
    )
}
fn policy_invalid(
    reader: &'static str,
    detail: &str,
) -> (Option<&'static str>, String, String, Value) {
    (
        Some(reader),
        "policy_invalid".into(),
        "policy".into(),
        json!(detail),
    )
}

// spec: specs/ap/policy.md#R12
// spec: specs/ap/policy.md#R13
// spec: specs/ap/policy.md#R16
#[test]
fn r12_the_host_shape_passes_every_reader_after_one_read() {
    let bytes = host().to_string();
    let fake = Fake::with(bytes.as_bytes());
    let answer = check(&fake, None).unwrap();
    let digest = format!("{:x}", Sha256::digest(bytes.as_bytes()));
    assert_eq!(
        answer,
        json!({"status": "ok", "path": PATH, "sha256": digest,
            "readers": ["models", "quota", "units", "reviews", "repositories", "work", "check"]})
    );
    assert_eq!(fake.reads.get(), 1);
    assert!(fake.calls.is_empty());
    // A file without the optional members passes as well: only `models` and `accounts` are required.
    let mut p = host();
    for m in ["quota", "units", "reviews", "repositories", "work", "hosts"] {
        p.as_object_mut().unwrap().remove(m);
    }
    assert_eq!(check(&Fake::json(&p), None).unwrap()["status"], "ok");
}

// spec: specs/ap/policy.md#R14
#[test]
fn r14_one_planted_fault_per_reader_is_refused_by_that_reader() {
    assert_eq!(
        refused("models", json!({"allowed": [], "never": []})),
        policy_invalid(
            "models",
            "models.allowed is not an array of the allowed length"
        )
    );
    assert_eq!(
        refused("accounts", json!({"claude": ["relative"]})),
        policy_invalid("models", "accounts.claude holds an entry of the wrong form")
    );
    assert_eq!(
        refused("quota", json!({"slice": "bad"})),
        policy_invalid(
            "quota",
            "quota.slice is not a slice name of a-z0-9 and .slice"
        )
    );
    assert_eq!(
        refused("units", json!({"timeouts": {"reviewer": 3600}})),
        policy_invalid(
            "units",
            "units.timeouts.reviewer is not a timeout of 1 to 86,400 seconds"
        )
    );
    assert_eq!(
        refused("reviews", json!({"bogus": 1})),
        policy_invalid("reviews", "reviews.bogus is not a member of reviews")
    );
    assert_eq!(
        refused("repositories", json!({"a/b": {"clone": "/x"}, "c/d": {}})),
        (
            Some("repositories"),
            "repository_not_mapped".into(),
            "repository".into(),
            Value::Null
        )
    );
    assert_eq!(
        refused("repositories", json!(["a/b"])),
        policy_invalid("repositories", "repositories is not an object")
    );
    assert_eq!(
        refused("repositories", json!({"a/b": {"clone": "relative"}})),
        policy_invalid(
            "repositories",
            "repositories.a/b.clone is not an absolute path"
        )
    );
    assert_eq!(
        refused("work", json!({"slots": 0})),
        policy_invalid("work", "work.slots is not an integer from 1 to 16")
    );
    assert_eq!(
        refused(
            "hosts",
            json!({"pc": {"ssh": "nobody", "work": "/w", "busy_load": 0.5}})
        ),
        policy_invalid("check", "hosts.pc.ssh is not user@host")
    );
}

// spec: specs/ap/policy.md#R12
#[test]
fn r12_the_first_refusal_in_the_order_of_the_readers_stops_the_check() {
    let mut p = host();
    p["hosts"] = json!({"vps": {}});
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("check"));
    p["work"] = json!({"slots": 0});
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("work"));
    p["reviews"] = json!({"bogus": 1});
    p["units"] = json!({"nope": 1});
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("units"));
    p["quota"] = json!({"slice": "bad"});
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("quota"));
    p["models"] = json!(1);
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("models"));
}

// spec: specs/ap/policy.md#R14
#[test]
fn r14_a_file_that_is_not_json_is_refused_under_models() {
    for (bytes, detail) in [
        (&b"{not json"[..], "file is not JSON"),
        (b"[1]", "file is not one JSON object"),
    ] {
        let Refusal { reader, failure } = check(&Fake::with(bytes), None).unwrap_err();
        assert_eq!(reader, Some("models"));
        assert_eq!(
            (failure.code, failure.field.as_str(), failure.detail),
            ("policy_invalid", "policy", json!(detail))
        );
    }
    let mut fake = Fake::with(b"");
    fake.file = Some(Err(ErrorKind::PermissionDenied));
    let Refusal { reader, failure } = check(&fake, None).unwrap_err();
    assert_eq!(reader, Some("models"));
    assert_eq!(failure.detail, json!("file is unreadable"));
    let huge = vec![b' '; 1_048_577];
    let Refusal { reader, failure } = check(&Fake::with(&huge), None).unwrap_err();
    assert_eq!(reader, Some("models"));
    assert_eq!(failure.detail, json!("file is too large"));
}

// spec: specs/ap/policy.md#R15
#[test]
fn r15_an_absent_file_is_policy_absent() {
    let mut fake = Fake::with(b"");
    fake.file = None;
    let Refusal { reader, failure } = check(&fake, None).unwrap_err();
    assert_eq!(reader, None);
    assert_eq!(
        (failure.code, failure.field.as_str(), failure.detail),
        ("policy_absent", "policy", Value::Null)
    );
    assert!(fake.calls.is_empty());
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ap-policy-check-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn ap(args: &[&str], env: Option<&Path>) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
    c.args(args)
        .stdin(Stdio::null())
        .env_remove("AP_POLICY_FILE");
    if let Some(p) = env {
        c.env("AP_POLICY_FILE", p);
    }
    c.output().unwrap()
}
fn listing(dir: &Path) -> Vec<(String, u64)> {
    let mut all: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                e.metadata().unwrap().len(),
            )
        })
        .collect();
    all.sort();
    all
}

// spec: specs/ap/policy.md#R13
// spec: specs/ap/policy.md#R14
// spec: specs/ap/policy.md#R15
// spec: specs/ap/policy.md#R16
#[test]
fn r13_the_command_prints_the_answer_and_writes_nothing() {
    let dir = scratch("cli");
    let file = dir.join("policy.json");
    std::fs::write(&file, host().to_string()).unwrap();
    let before = listing(&dir);
    for (args, env) in [
        (vec!["policy", "check", file.to_str().unwrap()], None),
        (vec!["policy", "check"], Some(file.as_path())),
    ] {
        let out = ap(&args, env);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["path"], file.to_str().unwrap());
        assert!(out.stderr.is_empty());
    }
    // A refusal: exit 2, stdout empty, the error with its reader on stderr.
    let mut p = host();
    p["units"] = json!({"timeouts": {"reviewer": 3600}});
    let bad = dir.join("bad.json");
    std::fs::write(&bad, p.to_string()).unwrap();
    let before = {
        let mut b = before;
        b.push(("bad.json".into(), std::fs::metadata(&bad).unwrap().len()));
        b.sort();
        b
    };
    let out = ap(&["policy", "check", bad.to_str().unwrap()], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        e,
        json!({"error": {"code": "policy_invalid", "field": "policy", "reader": "units",
            "detail": "units.timeouts.reviewer is not a timeout of 1 to 86,400 seconds"}})
    );
    // An absent file and a wrong form.
    let gone = dir.join("gone.json");
    let out = ap(&["policy", "check", gone.to_str().unwrap()], None);
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        e,
        json!({"error": {"code": "policy_absent", "field": "policy", "detail": null, "reader": null}})
    );
    let out = ap(&["policy", "check", "a", "b"], None);
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(e["error"]["code"], "usage");
    let out = ap(&["policy"], None);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(listing(&dir), before, "the check wrote a file");
    let _ = std::fs::remove_dir_all(&dir);
}
