//! `ap policy check` against a fake host (`specs/ap/policy.md` R12 to R16): the fake answers the one
//! read of the file and refuses every program call; nothing here touches the host's policy file.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
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
        "work": {"slots": 3, "slice": "agents.slice", "prefix": "ap-unit", "pages": 2, "page_prefix": "ap-page"},
        "hosts": {"pc": {"ssh": "u@pc", "work": "/home/u/work/ap-checks", "busy_load": 0.75}},
        "writer": {"model": "opus", "effort": "high", "permission_mode": "auto", "directory": "/var/lib/pages"},
        "api": {"key_file": "/etc/agent-platform/credentials/anthropic-api-key",
            "stops_usd": {"standard": 160, "soak": 185}, "day_cap_usd": 40,
            "jobs": {
                "drift": {"cap_usd": 60, "models": ["claude-sonnet-5-5", "claude-opus-5-5"], "max_tokens": 8000, "max_requests": 100},
                "merge-review": {"cap_usd": 45, "models": ["claude-opus-5-5", "claude-fable-5-2"], "max_tokens": 8000, "max_requests": 100},
                "mining": {"cap_usd": 18, "models": ["claude-sonnet-5-5", "claude-fable-5-2"], "max_tokens": 8000, "max_requests": 400},
                "evals": {"cap_usd": 39, "models": ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5", "claude-fable-5-2"], "max_tokens": 8000, "max_requests": 300}}},
        "store_url": "http://100.95.133.99:7432"
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
            "readers": ["models", "quota", "units", "reviews", "repositories", "work", "check", "writer", "api", "client"]})
    );
    assert_eq!(fake.reads.get(), 1);
    assert!(fake.calls.is_empty());
    // A file without the optional members passes as well: only `models` and `accounts` are required.
    let mut p = host();
    for m in [
        "quota",
        "units",
        "reviews",
        "repositories",
        "work",
        "hosts",
        "writer",
        "api",
        "store_url",
    ] {
        p.as_object_mut().unwrap().remove(m);
    }
    assert_eq!(check(&Fake::json(&p), None).unwrap()["status"], "ok");
    // The PC's optional memory bound is taken by the reader `check`.
    let mut p = host();
    p["hosts"]["pc"]["min_available_mb"] = json!(4096);
    assert_eq!(check(&Fake::json(&p), None).unwrap()["status"], "ok");
    // The stage account of a look on the PC: `stages` with `ssh` and `work` passes every reader.
    let mut p = host();
    p["hosts"]["pc"]["stages"] = json!({"ssh": "stage@pc", "work": "/home/stage/work"});
    assert_eq!(check(&Fake::json(&p), None).unwrap()["status"], "ok");
}

// spec: specs/ap/policy.md#R12
// spec: specs/ap/check.md#R16
#[test]
fn a_stages_member_that_lacks_work_is_refused_by_the_reader_check() {
    let pc = json!({"ssh": "u@pc", "work": "/w", "busy_load": 0.5, "stages": {"ssh": "stage@pc"}});
    let (reader, code, field, detail) = refused("hosts", json!({ "pc": pc }));
    assert_eq!(
        (reader, code.as_str(), field.as_str()),
        (Some("check"), "policy_invalid", "policy")
    );
    assert!(
        detail.as_str().unwrap().starts_with("hosts.pc.stages"),
        "{detail}"
    );
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
    assert_eq!(
        refused(
            "hosts",
            json!({"pc": {"ssh": "u@pc", "work": "/w", "busy_load": 0.5, "min_available_mb": -1}})
        ),
        policy_invalid(
            "check",
            "hosts.pc.min_available_mb is not a whole number from 0 to 1048576"
        )
    );
    assert_eq!(
        refused(
            "writer",
            json!({"model": "opus", "effort": "high", "permission_mode": "auto", "timeout_seconds": 0})
        ),
        policy_invalid(
            "writer",
            "writer.timeout_seconds is not 1 to 86,400 seconds"
        )
    );
    assert_eq!(
        refused(
            "api",
            json!({"key_file": "/etc/agent-platform/credentials/anthropic-api-key",
                "stops_usd": {"standard": 160, "soak": 185}, "day_cap_usd": 40,
                "jobs": {"drift": {"cap_usd": 60, "models": ["gpt-6.1-sol"], "max_tokens": 8000, "max_requests": 100}}})
        ),
        policy_invalid(
            "api",
            "api.jobs.drift.models is not 1 to 8 distinct Claude API model identifiers with an input and an output price"
        )
    );
    assert_eq!(
        refused("store_url", json!("http://127.0.0.1:7432")),
        policy_invalid("client", "store_url is not http://<tailnet-ip>:<port>")
    );
}

// spec: specs/ap/policy.md#R12
// spec: specs/ap/policy.md#R13
// spec: specs/ap/policy.md#R19
#[test]
fn r19_store_url_is_validated_by_client_after_api() {
    for valid in [
        "http://100.64.0.1:1",
        "http://100.95.133.99:7432",
        "http://100.127.255.254:65535",
        "http://[fd7a:115c:a1e0::1]:7432",
        "http://[fd7a:115c:a1e0:ab12::34cd]:80",
    ] {
        let mut p = host();
        p["store_url"] = json!(valid);
        let ans = check(&Fake::json(&p), None).unwrap();
        assert_eq!(ans["status"], "ok", "{valid}");
    }
    for bad in [
        json!(7432),
        json!(null),
        json!(true),
        json!(["http://100.95.133.99:7432"]),
        json!({"url": "http://100.95.133.99:7432"}),
        json!(""),
        json!("http://127.0.0.1:7432"),
        json!("http://0.0.0.0:7432"),
        json!("http://100.63.255.255:7432"),
        json!("http://100.128.0.0:7432"),
        json!("https://100.95.133.99:7432"),
        json!("http://vps.ts.net:7432"),
        json!("http://100.95.133.99:7432/v1"),
        json!("http://100.95.133.99:7432?x=1"),
        json!("http://100.95.133.99:7432#frag"),
        json!("http://user@100.95.133.99:7432"),
        json!("http://100.95.133.99:0"),
        json!("http://100.95.133.99:65536"),
        json!("http://100.95.133.99:07432"),
        json!("http://[::1]:7432"),
        json!("http://[fd7a:115c:a1e1::1]:7432"),
        json!("http://100.95.133.99:7432 "),
        json!("http://100.95.133.99:7432\n"),
    ] {
        let (reader, code, field, detail) = refused("store_url", bad.clone());
        assert_eq!(
            (reader, code.as_str(), field.as_str()),
            (Some("client"), "policy_invalid", "policy"),
            "{bad}"
        );
        assert!(
            detail.as_str().unwrap().starts_with("store_url"),
            "{bad}: {detail}"
        );
    }
    // `client` runs after `api`: a fault in both `api` and `store_url` is refused by `api`.
    let mut p = host();
    p["api"] = json!("bad");
    p["store_url"] = json!("http://127.0.0.1:7432");
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("api"));
}

// spec: specs/ap/policy.md#R17
// spec: specs/ap/policy.md#R18
// spec: specs/ap/policy.md#R12
#[test]
fn api_key_accounts_of_each_form() {
    let stage = "/etc/agent-platform/credentials/anthropic-stage-key";
    let batch = "/etc/agent-platform/credentials/anthropic-api-key";
    let plain = check(&Fake::json(&host()), None).unwrap();
    // A valid member, five entries at most, answers as the same file without it: the same readers.
    let mut p = host();
    p["api_key_accounts"] = json!({"/root/.claude-api": stage});
    let with = check(&Fake::json(&p), None).unwrap();
    assert_eq!(with["readers"], plain["readers"]);
    assert_eq!(with["status"], "ok");
    // A key of the member need not be listed in `accounts`; a key under another name is no fault.
    p["api_key_accounts"] = json!({
        "/root/.claude": stage, "/a": stage, "/b": stage, "/c": "/etc/agent-platform/credentials/x", "/d": stage});
    assert_eq!(
        check(&Fake::json(&p), None).unwrap()["readers"],
        plain["readers"]
    );
    // Without the member `api`, the batch key's name is any other.
    let mut q = host();
    q.as_object_mut().unwrap().remove("api");
    q["api_key_accounts"] = json!({"/root/.claude-api": batch});
    assert_eq!(check(&Fake::json(&q), None).unwrap()["status"], "ok");
    // Each other form is `policy_invalid` of the reader `models`, its detail beginning with the member
    // or the entry.
    let first = |v: Value| {
        let (reader, code, field, detail) = refused("api_key_accounts", v);
        assert_eq!(
            (reader, code.as_str(), field.as_str()),
            (Some("models"), "policy_invalid", "policy")
        );
        detail
            .as_str()
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .to_owned()
    };
    let member = "api_key_accounts";
    let entry = |k: &str| format!("api_key_accounts.{k}");
    for value in [
        json!("on"),
        json!([stage]),
        json!(null),
        json!({}),
        json!(7),
    ] {
        assert_eq!(first(value.clone()), member, "{value}");
    }
    let six: serde_json::Map<String, Value> =
        (1..=6).map(|n| (format!("/k{n}"), json!(stage))).collect();
    assert_eq!(first(Value::Object(six)), member);
    for key in [
        "relative/dir",
        "",
        "/root/.claude-api/",
        "/",
        "/a/../b",
        "/a\u{0}b",
    ] {
        assert_eq!(first(json!({ (key): stage })), entry(key), "{key}");
    }
    for value in [
        json!("anthropic-stage-key"),
        json!("/etc/agent-platform/credentials/"),
        json!("/etc/agent-platform/credentials/a/b"),
        json!("/etc/agent-platform/credentials/."),
        json!("/etc/agent-platform/credentials/.."),
        json!("/etc/other/anthropic-stage-key"),
        json!("/etc/agent-platform/credentials-x/key"),
        json!(7),
        json!(null),
        json!(["x"]),
        json!({"a": 1}),
    ] {
        assert_eq!(
            first(json!({"/root/.claude-api": value.clone()})),
            entry("/root/.claude-api"),
            "{value}"
        );
    }
    // A value equal to `api.key_file` (the host's `api` member) is refused under its entry, and the
    // member's other entries pass.
    assert_eq!(
        first(json!({"/root/.claude-api": batch})),
        entry("/root/.claude-api")
    );
    assert_eq!(first(json!({"/ok": stage, "/bad": batch})), entry("/bad"));
}

// spec: specs/ap/policy.md#R12
// spec: specs/ap/policy.md#R13
#[test]
fn r12_the_reader_api_runs_after_writer() {
    // A member api of a wrong form beside a fault of `writer`: the reader `writer` comes first.
    let mut p = host();
    p["api"] = json!("on");
    let Refusal { reader, failure } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("api"));
    assert_eq!(failure.detail, json!("api is not an object"));
    p["writer"] = json!("opus");
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("writer"));
}

// spec: specs/ap/policy.md#R12
// spec: specs/ap/policy.md#R13
#[test]
fn r12_the_reader_writer_runs_last() {
    // A writer of a wrong form beside a fault of `hosts`: the reader `check` comes first.
    let mut p = host();
    p["writer"] = json!("opus");
    let Refusal { reader, failure } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("writer"));
    assert_eq!(failure.detail, json!("writer is not an object"));
    p["hosts"] = json!({"vps": {}});
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("check"));
    // The prefix of the writers' units is judged by the reader `work`, only with a writer.
    let mut p = host();
    p["work"]["page_prefix"] = json!("ap-unit");
    let Refusal { reader, .. } = check(&Fake::json(&p), None).unwrap_err();
    assert_eq!(reader, Some("work"));
    p.as_object_mut().unwrap().remove("writer");
    assert_eq!(check(&Fake::json(&p), None).unwrap()["status"], "ok");
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

// spec: specs/ap/policy.md#R20
// spec: specs/ap/unit-run.md#R3
#[test]
fn r20_approval_member_of_units_is_read_by_its_form() {
    let good = json!({"contract": "verifier", "guarded": ["ci/**", "check", "crates/ap/src/unit/drive.rs"], "sample": 2});
    for approval in [
        good.clone(),
        json!({"contract": "lead", "guarded": ["a"], "sample": 100}),
    ] {
        let mut p = host();
        p["units"]["approval"] = approval;
        let fake = Fake::json(&p);
        assert_eq!(check(&fake, None).unwrap()["status"], "ok");
        assert!(fake.calls.is_empty());
    }
    let many: Vec<String> = (0..61).map(|i| format!("p{i}")).collect();
    let cases = [
        (json!("verifier"), "units.approval is not an object"),
        (
            json!({"contract": "verifier", "guarded": ["a"], "sample": 2, "more": 1}),
            "units.approval.more is not a member of this form",
        ),
        (
            json!({"guarded": ["a"], "sample": 2}),
            "units.approval.contract is not lead or verifier",
        ),
        (
            json!({"contract": "policy", "guarded": ["a"], "sample": 2}),
            "units.approval.contract is not lead or verifier",
        ),
        (
            json!({"contract": "lead", "sample": 2}),
            "units.approval.guarded is not 1 to 60 patterns of the write set",
        ),
        (
            json!({"contract": "lead", "guarded": [], "sample": 2}),
            "units.approval.guarded is not 1 to 60 patterns of the write set",
        ),
        (
            json!({"contract": "lead", "guarded": many, "sample": 2}),
            "units.approval.guarded is not 1 to 60 patterns of the write set",
        ),
        (
            json!({"contract": "lead", "guarded": ["crates/ap/src/*"], "sample": 2}),
            "units.approval.guarded is not 1 to 60 patterns of the write set",
        ),
        (
            json!({"contract": "lead", "guarded": ["a", 1], "sample": 2}),
            "units.approval.guarded is not 1 to 60 patterns of the write set",
        ),
        (
            json!({"contract": "lead", "guarded": ["a"]}),
            "units.approval.sample is not an integer of 2 to 100",
        ),
        (
            json!({"contract": "lead", "guarded": ["a"], "sample": 1}),
            "units.approval.sample is not an integer of 2 to 100",
        ),
        (
            json!({"contract": "lead", "guarded": ["a"], "sample": 101}),
            "units.approval.sample is not an integer of 2 to 100",
        ),
        (
            json!({"contract": "lead", "guarded": ["a"], "sample": "3"}),
            "units.approval.sample is not an integer of 2 to 100",
        ),
    ];
    for (approval, detail) in cases {
        let mut units = host()["units"].clone();
        units["approval"] = approval.clone();
        assert_eq!(
            refused("units", units),
            policy_invalid("units", detail),
            "{approval}"
        );
    }
}
