//! Refusal records (`refusals.jsonl`), the action-log read `ont log` /
//! `ont_log`, and the actor's `role`: temporary stores only.
#![allow(clippy::unwrap_used)]
use ont::{Context, Failure};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const FIXTURE: &[u8] = include_bytes!("fixtures/action-work.json");
const NOW: u64 = 1000;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ont-refusal-log-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn store(&self) -> String {
        self.0.join("store.sqlite").display().to_string()
    }
    fn refusals(&self) -> PathBuf {
        self.0.join("refusals.jsonl")
    }
    /// The lines of `refusals.jsonl` (none when it is absent).
    fn lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.refusals())
            .map(|s| s.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One command at `now`: (exit code, stdout, stderr).
fn raw(t: &Temp, now: u64, cmd: &[&str], stdin: &[u8]) -> (i32, String, String) {
    let now = now.to_string();
    let store = t.store();
    let mut args: Vec<String> = ["--store", &store, "--now", &now]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(cmd.iter().map(|s| s.to_string()));
    let out = ont::run(&args, stdin, 0, 0);
    (out.code, out.stdout, out.stderr)
}
fn run_at(t: &Temp, now: u64, cmd: &[&str], stdin: &Value) -> (i32, Value) {
    let (code, stdout, stderr) = raw(t, now, cmd, stdin.to_string().as_bytes());
    let value = if stdout.trim().is_empty() {
        json!({ "stderr": stderr })
    } else {
        serde_json::from_str(stdout.trim()).unwrap()
    };
    (code, value)
}
fn run(t: &Temp, cmd: &[&str], stdin: &Value) -> (i32, Value) {
    run_at(t, NOW, cmd, stdin)
}
fn apply(t: &Temp, request: Value) -> (i32, Value) {
    run(t, &["action", "apply"], &request)
}
fn log(t: &Temp, request: Value) -> Value {
    let (code, answer) = run(t, &["log"], &request);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["status"], "ok", "{answer}");
    answer["result"].clone()
}
fn fixture() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}
fn install(t: &Temp, schema: &Value) {
    let file = t.0.join("schema.json");
    std::fs::write(&file, serde_json::to_vec(schema).unwrap()).unwrap();
    let (code, out, _) = raw(t, NOW, &["schema", "install", file.to_str().unwrap()], b"");
    assert_eq!(code, 0, "{out}");
}
/// The fixture plus `lead_tag`, whose criterion is the actor's role `lead`.
fn with_lead_tag(mut schema: Value) -> Value {
    schema["action_types"]["lead_tag"] = json!({
        "description":"Create a tag; only a lead may.",
        "parameters":{"label":{"type":"string","required":true}},
        "submission_criteria":[{"code":"lead_only","message":"Only a lead may.","all":[{"actor":{"role":"lead"}}]}],
        "rules":[{"create_object":"tag","set":{"label":"$label"}}]
    });
    schema["grants"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!("lead_tag"));
    schema
}
fn installed() -> Temp {
    let t = Temp::new();
    install(&t, &with_lead_tag(fixture()));
    t
}
/// A probe with `kind` "x": `check_probe_kind` on it fails `kind_is_note`.
fn probe(t: &Temp) -> Value {
    let (code, answer) = apply(
        t,
        json!({"action_type":"create_probe","parameters":{"kind":"x","note":"n","count":1},"request_id":"probe-1"}),
    );
    assert_eq!(code, 0, "{answer}");
    answer["result"]["edits"][0]["id"].clone()
}
fn canonical(v: &Value) -> String {
    serde_json::to_string(&ont::action::canonical(v)).unwrap()
}

// A1, A2: each refusal after the action type was found appends one
// canonical line with the members of the page, in mode 0600.
#[test]
fn refused_applies_append_one_canonical_line_each() {
    let t = installed();
    let id = probe(&t);
    let (code, _) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":5},"agent":"a1"}),
    );
    assert_eq!(code, 2);
    let (code, answer) = apply(
        &t,
        json!({"action_type":"check_probe_kind","parameters":{"probe":id},"request_id":"k1","session":"s1","role":"lead"}),
    );
    assert_eq!(answer["code"], "submission_criteria", "{answer}");
    assert_eq!(code, 2);
    let (_, answer) = apply(
        &t,
        json!({"action_type":"check_probe_kind","parameters":{"probe":id},"expected":[{"id":id,"revision":"r9"}]}),
    );
    assert_eq!(answer["code"], "revision_conflict", "{answer}");
    let (code, _) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"a"},"request_id":"t1"}),
    );
    assert_eq!(code, 0);
    let (_, answer) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"b"},"request_id":"t1"}),
    );
    assert_eq!(answer["code"], "request_id_reused", "{answer}");
    let (_, answer) = apply(&t, json!({"action_type":"create_tag","parameters":[]}));
    assert_eq!(answer["code"], "invalid_request", "{answer}");
    assert_eq!(answer["field"], "/parameters", "{answer}");

    let lines = t.lines();
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert_eq!(
        lines[0],
        r#"{"action_type":"create_tag","actor":{"actor_trust":"self_reported","agent":"a1","client":"cli","principal":"agent"},"code":"invalid_parameter","domain":"work","outcome":"refused","parameters":{"label":5},"schema_version":1,"submitted_at":1000}"#
    );
    assert_eq!(
        lines[1],
        format!(
            r#"{{"action_type":"check_probe_kind","actor":{{"actor_trust":"self_reported","client":"cli","principal":"agent","role":"lead","session":"s1"}},"code":"submission_criteria","domain":"work","failed":["kind_is_note"],"outcome":"refused","parameters":{{"probe":{}}},"request_id":"k1","schema_version":1,"submitted_at":1000}}"#,
            canonical(&id)
        )
    );
    let codes: Vec<String> = lines
        .iter()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["code"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        codes,
        [
            "invalid_parameter",
            "submission_criteria",
            "revision_conflict",
            "request_id_reused",
            "invalid_request"
        ]
    );
    for line in &lines {
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(&canonical(&value), line, "canonical");
        assert!(value.get("failed").is_none() || value["code"] == "submission_criteria");
    }
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(t.refusals())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert!(t.0.join("refusals.jsonl.lock").exists());
}

// A1: a grant refusal is recorded as well.
#[test]
fn a_grant_refusal_is_recorded() {
    let t = Temp::new();
    let mut schema = fixture();
    schema["grants"][0]["actions"] = json!(["create_probe"]);
    install(&t, &schema);
    let (_, answer) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"x"}}),
    );
    assert_eq!(answer["code"], "action_forbidden", "{answer}");
    let lines = t.lines();
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(r#""code":"action_forbidden""#),
        "{}",
        lines[0]
    );
}

// A1: the append waits for the exclusive lock on `refusals.jsonl.lock`, and
// runs after the transaction has ended (another apply commits meanwhile).
#[test]
fn the_append_takes_the_lock_after_the_transaction() {
    use std::os::unix::fs::OpenOptionsExt;
    let t = installed();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(t.0.join("refusals.jsonl.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let store = t.store();
    let refused = std::thread::spawn(move || {
        let args: Vec<String> = ["--store", &store, "action", "apply"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        ont::run(
            &args,
            &br#"{"action_type":"create_tag","parameters":{"label":5}}"#[..],
            0,
            0,
        )
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(t.lines().is_empty(), "written while the lock is held");
    let (code, answer) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"meanwhile"},"request_id":"m1"}),
    );
    assert_eq!(code, 0, "the refused apply holds no transaction: {answer}");
    assert_eq!(answer["result"]["outcome"], "applied");
    assert!(!refused.is_finished(), "the append waits for the lock");
    drop(lock);
    let out = refused.join().unwrap();
    assert_eq!(out.code, 2, "{}", out.stdout);
    assert_eq!(t.lines().len(), 1);
}

// A3: nothing is recorded before the type is known, for validate, or for a
// replay that answers `already_applied`.
#[test]
fn nothing_is_recorded_before_the_type_is_known_for_validate_or_a_replay() {
    let t = installed();
    for (request, code, field) in [
        (
            json!({"action_type":"create_tag","parameters":{"label":"a"},"request_id":"bad id"}),
            "invalid_request",
            "/request_id",
        ),
        (
            json!({"action_type":"create_tag","parameters":{"label":"a"},"agent":"bad agent"}),
            "invalid_request",
            "/agent",
        ),
        (
            json!({"action_type":"create_tag","parameters":{"label":"a"},"role":"bad role"}),
            "invalid_request",
            "/role",
        ),
        (
            json!({"action_type":"no_such_action","parameters":{}}),
            "unknown_action",
            "/action_type",
        ),
        (json!({"parameters":{}}), "invalid_request", "/action_type"),
    ] {
        let (exit, answer) = apply(&t, request);
        assert_eq!(exit, 2);
        assert_eq!(answer["code"], code, "{answer}");
        assert_eq!(answer["field"], field, "{answer}");
    }
    let (_, answer) = run(
        &t,
        &["action", "validate"],
        &json!({"action_type":"create_tag","parameters":{"label":5}}),
    );
    assert_eq!(answer["code"], "invalid_parameter", "{answer}");
    let request =
        json!({"action_type":"create_tag","parameters":{"label":"once"},"request_id":"r1"});
    assert_eq!(apply(&t, request.clone()).1["result"]["outcome"], "applied");
    assert_eq!(apply(&t, request).1["result"]["outcome"], "already_applied");
    assert!(!t.refusals().exists());
}

// A3: a store failure is not recorded.
#[test]
fn store_busy_is_not_recorded() {
    let t = installed();
    let sql = rusqlite::Connection::open(t.store()).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE").unwrap();
    // A valid request reaches the write, which waits and is refused busy.
    let (_, answer) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"ok"}}),
    );
    assert_eq!(answer["code"], "store_busy", "{answer}");
    sql.execute_batch("ROLLBACK").unwrap();
    assert!(!t.refusals().exists());
}

// A4: the answer is the same whether or not the line could be appended.
#[test]
fn a_failed_append_leaves_the_answer_byte_for_byte() {
    let written = installed();
    let failing = installed();
    let elsewhere = failing.0.join("elsewhere");
    std::fs::write(&elsewhere, b"").unwrap();
    std::os::unix::fs::symlink(&elsewhere, failing.refusals()).unwrap();
    for t in [&written, &failing] {
        probe(t);
    }
    let id = probe(&written);
    for request in [
        json!({"action_type":"check_probe_kind","parameters":{"probe":id},"request_id":"k1"}),
        json!({"action_type":"create_tag","parameters":{"label":5}}),
    ] {
        let a = raw(
            &written,
            NOW,
            &["action", "apply"],
            request.to_string().as_bytes(),
        );
        let b = raw(
            &failing,
            NOW,
            &["action", "apply"],
            request.to_string().as_bytes(),
        );
        assert_eq!(a.0, 2, "{}", a.1);
        assert_eq!(a, b);
    }
    assert_eq!(written.lines().len(), 2);
    assert_eq!(std::fs::read(&elsewhere).unwrap(), b"");
}

// A5: the library's apply records, and so does the MCP tool.
#[test]
fn the_library_and_the_mcp_tool_record() {
    let t = installed();
    let mut ctx = Context::new(PathBuf::from(t.store()), NOW);
    ctx.client = "domain-server".into();
    ctx.role = Some("lib-role".into());
    let request = ont::action::ApplyActionRequest {
        op: None,
        action_type: "create_tag".into(),
        parameters: json!({"label":5}),
        agent: None,
        session: None,
        expected: None,
        request_id: None,
    };
    match ont::action::apply_action(&ctx, &request) {
        Err(Failure::Refused(r)) => assert_eq!(r.code, "invalid_parameter"),
        other => panic!("{other:?}"),
    }
    let config = ont::mcp::Config {
        store: t.store().into(),
        now: Some(NOW),
        client: "test".into(),
    };
    let mut server = ont::mcp::Server::new(config, &ont::cli::execute);
    let response = server
        .handle_line(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ont_action_apply",
                "arguments":{"action_type":"create_tag","parameters":{"label":6},"request_id":"m1","role":"mcp-role"}}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    let lines: Vec<Value> = t
        .lines()
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["actor"]["client"], "domain-server");
    assert_eq!(lines[0]["actor"]["role"], "lib-role");
    assert_eq!(lines[1]["actor"]["client"], "test");
    assert_eq!(lines[1]["actor"]["role"], "mcp-role");
    assert_eq!(lines[1]["request_id"], "m1");
}

// A6: `ont backup` copies only the store, and neither it nor `ont compact`
// reads or changes `refusals.jsonl`.
#[test]
fn backup_and_compact_leave_the_refusals_alone() {
    let t = installed();
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":5}}),
    );
    // A damaged line: a reader would choke on it, these commands never read it.
    std::fs::OpenOptions::new()
        .append(true)
        .open(t.refusals())
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"\xff not json\n"))
        .unwrap();
    let before = std::fs::read(t.refusals()).unwrap();
    let (code, answer) = run(&t, &["backup"], &json!({}));
    assert_eq!(code, 0, "{answer}");
    let copy = PathBuf::from(answer["result"]["path"].as_str().unwrap());
    let names: Vec<String> = std::fs::read_dir(copy.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.contains("refusal")), "{names:?}");
    let (code, answer) = run(&t, &["compact"], &json!({}));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(std::fs::read(t.refusals()).unwrap(), before);
}

// B2, B3: applied rows come from the table, newest first, with the members of
// the page; `parameters` only on request.
#[test]
fn log_reads_applied_rows_newest_first_from_the_table() {
    let t = installed();
    let mut applied = Vec::new();
    for (now, label, role) in [
        (1000, "a", None),
        (2000, "b", Some("verifier")),
        (3000, "c", None),
    ] {
        let mut request = json!({"action_type":"create_tag","parameters":{"label":label},"request_id":label,"agent":"ag"});
        if let Some(role) = role {
            request["role"] = json!(role);
        }
        let (code, answer) = run_at(&t, now, &["action", "apply"], &request);
        assert_eq!(code, 0, "{answer}");
        applied.push(answer["result"].clone());
    }
    std::fs::remove_file(t.0.join("edits.jsonl")).unwrap();
    let result = log(&t, json!({}));
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0],
        json!({"action_type":"create_tag","domain":"work",
            "actor":{"actor_trust":"self_reported","agent":"ag","client":"cli","principal":"agent"},
            "submitted_at":"1970-01-01T00:50:00Z","outcome":"applied",
            "action_id":applied[2]["action_id"],"edits":applied[2]["edits"]})
    );
    assert_eq!(rows[1]["actor"]["role"], "verifier");
    assert_eq!(rows[2]["action_id"], applied[0]["action_id"]);
    assert_eq!(result["continuation"], Value::Null);
    assert_eq!(result["withheld"], 0);
    assert_eq!(result["skipped"], 0);
    assert_eq!(result["scan_limited"], false);
    let with = log(&t, json!({"parameters":true,"limit":1}));
    assert_eq!(with["rows"][0]["parameters"], json!({"label":"c"}));
    assert!(rows.iter().all(|r| r.get("parameters").is_none()));
}

// B1: filters, paging and the refusals of the request.
#[test]
fn log_filters_pages_and_refuses_malformed_requests() {
    let t = installed();
    for (now, label, role) in [
        (1000, "a", "lead"),
        (2000, "b", "verifier"),
        (3000, "c", "lead"),
    ] {
        let (code, answer) = run_at(
            &t,
            now,
            &["action", "apply"],
            &json!({"action_type":"create_tag","parameters":{"label":label},"request_id":label,"role":role}),
        );
        assert_eq!(code, 0, "{answer}");
    }
    let labels = |result: &Value| -> Vec<String> {
        result["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["parameters"]["label"].as_str().unwrap().to_string())
            .collect()
    };
    let p = |extra: Value| {
        let mut request = json!({"parameters":true});
        for (k, v) in extra.as_object().unwrap() {
            request[k] = v.clone();
        }
        log(&t, request)
    };
    assert_eq!(labels(&p(json!({"actor":{"role":"lead"}}))), ["c", "a"]);
    assert_eq!(
        labels(&p(json!({"actor":{"role":"lead","client":"cli"}}))),
        ["c", "a"]
    );
    assert_eq!(
        labels(&p(json!({"actor":{"agent":"nobody"}}))),
        Vec::<String>::new()
    );
    assert_eq!(
        labels(&p(
            json!({"since":"1970-01-01T00:33:20Z","until":"1970-01-01T00:50:00Z"})
        )),
        ["b"]
    );
    assert_eq!(
        labels(&p(json!({"since":"1970-01-01T00:33:20Z"}))),
        ["c", "b"]
    );
    assert_eq!(
        labels(&p(json!({"action_type":"create_probe"}))),
        Vec::<String>::new()
    );
    assert_eq!(
        labels(&p(json!({"action_type":"create_tag","outcome":"applied"}))),
        ["c", "b", "a"]
    );
    // Paging: one row per call, as search pages.
    let mut seen = Vec::new();
    let mut continuation = Value::Null;
    loop {
        let result = p(json!({"limit":1,"continuation":continuation}));
        seen.extend(labels(&result));
        continuation = result["continuation"].clone();
        if continuation.is_null() {
            break;
        }
    }
    assert_eq!(seen, ["c", "b", "a"]);
    let refused_token = {
        apply(
            &t,
            json!({"action_type":"create_tag","parameters":{"label":5}}),
        );
        apply(
            &t,
            json!({"action_type":"create_tag","parameters":{"label":6}}),
        );
        log(&t, json!({"outcome":"refused","limit":1}))["continuation"].clone()
    };
    assert!(refused_token.is_string());
    for (request, code, field) in [
        (json!({"unknown":1}), "invalid_request", ""),
        (json!({"outcome":"maybe"}), "invalid_request", "/outcome"),
        (json!({"since":"2026-11-17"}), "invalid_request", "/since"),
        (
            json!({"until":"2026-02-30T00:00:00Z"}),
            "invalid_request",
            "/until",
        ),
        (json!({"actor":{"role":5}}), "invalid_request", ""),
        (json!({"parameters":"yes"}), "invalid_request", ""),
        (json!({"limit":0}), "invalid_request", "/limit"),
        (json!({"limit":1001}), "limit_too_large", "/limit"),
        (
            json!({"continuation":"garbage"}),
            "invalid_continuation",
            "/continuation",
        ),
        (
            json!({"continuation":refused_token}),
            "invalid_continuation",
            "/continuation",
        ),
    ] {
        let (exit, answer) = run(&t, &["log"], &request);
        assert_eq!(exit, 2, "{request}");
        assert_eq!(answer["code"], code, "{request}: {answer}");
        if !field.is_empty() {
            assert_eq!(answer["field"], field, "{request}: {answer}");
        } else {
            assert!(answer["field"].is_string(), "{request}: {answer}");
        }
    }
}

// B2, B3: refused rows come from `refusals.jsonl`; a missing file is zero
// rows, a bad line is skipped and counted, an incomplete last line unread.
#[test]
fn log_reads_refusals_and_skips_bad_lines() {
    let t = installed();
    let empty = log(&t, json!({"outcome":"refused"}));
    assert_eq!(empty["rows"], json!([]));
    assert_eq!(empty["skipped"], 0);
    let id = probe(&t);
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":5},"agent":"a1"}),
    );
    let append = |bytes: &[u8]| {
        std::fs::OpenOptions::new()
            .append(true)
            .open(t.refusals())
            .and_then(|mut f| std::io::Write::write_all(&mut f, bytes))
            .unwrap();
    };
    append(b"not json\n[1,2]\n");
    run_at(
        &t,
        2000,
        &["action", "apply"],
        &json!({"action_type":"check_probe_kind","parameters":{"probe":id},"request_id":"k1","role":"lead"}),
    );
    append(br#"{"incomplete":"#);
    let result = log(&t, json!({"outcome":"refused"}));
    assert_eq!(result["skipped"], 2, "{result}");
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        json!({"action_type":"check_probe_kind","domain":"work",
            "actor":{"actor_trust":"self_reported","client":"cli","principal":"agent","role":"lead"},
            "submitted_at":"1970-01-01T00:33:20Z","outcome":"refused","code":"submission_criteria",
            "failed":["kind_is_note"],"request_id":"k1"})
    );
    assert_eq!(
        rows[1],
        json!({"action_type":"create_tag","domain":"work",
            "actor":{"actor_trust":"self_reported","agent":"a1","client":"cli","principal":"agent"},
            "submitted_at":"1970-01-01T00:16:40Z","outcome":"refused","code":"invalid_parameter"})
    );
    let with = log(&t, json!({"outcome":"refused","parameters":true}));
    assert_eq!(with["rows"][1]["parameters"], json!({"label":5}));
    // The next append completes the damaged line first: it is skipped.
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":7}}),
    );
    let result = log(&t, json!({"outcome":"refused"}));
    assert_eq!(result["skipped"], 3, "{result}");
    assert_eq!(result["rows"].as_array().unwrap().len(), 3);
}

/// `fixture()` at `version` with the grant's lists replaced.
fn version(mut schema: Value, version: u64, actions: &[&str], read: &[&str]) -> Value {
    schema["version"] = json!(version);
    schema["grants"][0]["actions"] = json!(actions);
    schema["grants"][0]["read"] = json!(read);
    schema["grants"][0]["derive"] = json!(read);
    schema
}

// B1 admission: the grant of the action type, the read grant of every edited
// type; a removed action type is admitted by its edits' read grants alone.
#[test]
fn log_withholds_rows_outside_the_grants_and_keeps_removed_types_readable() {
    let t = Temp::new();
    install(&t, &fixture());
    let id = probe(&t);
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"a"},"request_id":"t1"}),
    );
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":5}}),
    );
    apply(
        &t,
        json!({"action_type":"check_probe_kind","parameters":{"probe":id}}),
    );
    let all = ["task", "hold", "note", "tag", "widget", "probe"];
    let kinds = |result: &Value| -> Vec<String> {
        result["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["action_type"].as_str().unwrap().to_string())
            .collect()
    };
    let applied = log(&t, json!({}));
    assert_eq!(kinds(&applied), ["create_tag", "create_probe"]);
    assert_eq!(applied["withheld"], 0);
    // Version 2: no grant of create_tag, no read grant of probe.
    let actions = ["create_probe", "check_probe_kind"];
    install(&t, &version(fixture(), 2, &actions, &all[..5]));
    let applied = log(&t, json!({}));
    assert_eq!(kinds(&applied), Vec::<String>::new());
    assert_eq!(applied["withheld"], 2, "{applied}");
    let refused = log(&t, json!({"outcome":"refused"}));
    assert_eq!(kinds(&refused), ["check_probe_kind"]);
    assert_eq!(refused["withheld"], 1, "{refused}");
    // The filters apply before admission: a filtered-out row is not counted.
    assert_eq!(
        log(&t, json!({"action_type":"create_probe"}))["withheld"],
        1
    );
    // Version 3 removes create_tag: its rows are admitted by the read grant
    // of their edits (tag) alone; probe stays unreadable.
    let mut v3 = version(fixture(), 3, &actions, &all[..5]);
    v3["action_types"]
        .as_object_mut()
        .unwrap()
        .remove("create_tag");
    install(&t, &v3);
    let applied = log(&t, json!({}));
    assert_eq!(kinds(&applied), ["create_tag"]);
    assert_eq!(applied["withheld"], 1);
    let refused = log(&t, json!({"outcome":"refused"}));
    assert_eq!(kinds(&refused), ["check_probe_kind", "create_tag"]);
    assert_eq!(refused["withheld"], 0);
    // B1 amended: the refused row of the removed type never carries its
    // parameters; the row of a defined type does when asked.
    let refused = log(&t, json!({"outcome":"refused","parameters":true}));
    let rows = refused["rows"].as_array().unwrap();
    assert_eq!(rows[1]["action_type"], "create_tag");
    assert!(rows[1].get("parameters").is_none(), "{}", rows[1]);
    assert_eq!(rows[1]["parameters_withheld"], true);
    assert_eq!(rows[0]["parameters"], json!({"probe":id}));
    assert!(rows[0].get("parameters_withheld").is_none());
    let plain = log(&t, json!({"outcome":"refused"}));
    assert_eq!(plain["rows"][1]["parameters_withheld"], true);
    assert!(plain["rows"][1].get("parameters").is_none());
}

// B1 bound: one call reads at most 10,000 lines of the file.
#[test]
fn log_stops_at_the_scan_bound_of_the_file() {
    let t = installed();
    let line = r#"{"action_type":"create_tag","actor":{"actor_trust":"self_reported","client":"cli","principal":"agent"},"code":"invalid_parameter","domain":"work","outcome":"refused","parameters":{"label":5},"schema_version":1,"submitted_at":1000}"#;
    let mut bytes = String::new();
    for _ in 0..30_000 {
        bytes.push_str(line);
        bytes.push('\n');
    }
    std::fs::write(t.refusals(), bytes).unwrap();
    let mut continuation = Value::Null;
    for call in 0..3 {
        let result = log(
            &t,
            json!({"outcome":"refused","action_type":"none","continuation":continuation}),
        );
        assert_eq!(result["rows"], json!([]));
        assert_eq!(result["scan_limited"], call < 2, "{call}: {result}");
        continuation = result["continuation"].clone();
        assert_eq!(continuation.is_null(), call == 2, "{call}: {result}");
    }
    let page = log(&t, json!({"outcome":"refused","limit":1000}));
    assert_eq!(page["rows"].as_array().unwrap().len(), 1000);
    assert_eq!(page["scan_limited"], false);
    assert!(page["continuation"].is_string());
}

// B4: the usage lists `log` with its fields.
#[test]
fn help_lists_log_with_its_fields() {
    let out = ont::run(&["help".into()], &b""[..], 0, 0);
    let help: Value = serde_json::from_str(&out.stdout).unwrap();
    let entry = help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "log")
        .unwrap()
        .clone();
    assert_eq!(entry["stdin_json"], true);
    let fields = entry["fields"].as_str().unwrap();
    for member in [
        "outcome?",
        "applied|refused",
        "action_type?",
        "since?",
        "until?",
        "actor?",
        "parameters?",
        "limit?: 1-1000",
        "continuation?",
    ] {
        assert!(fields.contains(member), "{member}: {fields}");
    }
    let out = ont::run(&["log".into(), "--help".into()], &b""[..], 0, 0);
    let sub: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(sub["commands"], json!([entry]));
}

// B5: `ont_log` answers the line `ont log` prints.
#[test]
fn the_mcp_tool_answers_what_the_subcommand_prints() {
    let t = installed();
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"a"},"request_id":"t1"}),
    );
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":5}}),
    );
    let config = ont::mcp::Config {
        store: t.store().into(),
        now: Some(NOW),
        client: "test".into(),
    };
    let mut server = ont::mcp::Server::new(config, &ont::cli::execute);
    for request in [
        json!({}),
        json!({"outcome":"refused","parameters":true}),
        json!({"limit":0}),
    ] {
        let response = server
            .handle_line(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ont_log","arguments":request}})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let (code, stdout, _) = raw(&t, NOW, &["log"], request.to_string().as_bytes());
        assert_eq!(
            response["result"]["content"][0]["text"].as_str().unwrap(),
            stdout.trim_end()
        );
        assert_eq!(response["result"]["isError"], code != 0);
    }
}

// C1, C2: the request's role wins over ONT_ROLE; the role reaches the action
// log row, `edits.jsonl` and `refusals.jsonl`.
#[test]
fn the_role_comes_from_the_request_or_the_environment() {
    // Set the environment only in a child, never in the threaded test process.
    if std::env::var_os("ONT_ROLE_TEST").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "the_role_comes_from_the_request_or_the_environment",
                "--nocapture",
            ])
            .env("ONT_ROLE", "env-role")
            .env("ONT_ROLE_TEST", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("test result: ok. 1 passed;"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let t = installed();
    for (request_id, role) in [
        ("e1", None),
        ("e2", Some(json!("req-role"))),
        ("e3", Some(Value::Null)),
    ] {
        let mut request = json!({"action_type":"create_tag","parameters":{"label":request_id},"request_id":request_id});
        if let Some(role) = role {
            request["role"] = role;
        }
        let (code, answer) = apply(&t, request);
        assert_eq!(code, 0, "{answer}");
        assert_eq!(answer["result"]["actor_trust"], "self_reported");
    }
    let mut db = ont_store::Store::open_schema_existing(std::path::Path::new(&t.store())).unwrap();
    let log = db.action_log().unwrap();
    let roles: Vec<&str> = log
        .iter()
        .map(|(_, e)| e.actor["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["env-role", "req-role", "env-role"]);
    let edits = std::fs::read_to_string(t.0.join("edits.jsonl")).unwrap();
    let expected: Vec<String> = log
        .iter()
        .map(|(s, e)| ont::action::log_line(*s, e))
        .collect();
    assert_eq!(edits.lines().collect::<Vec<_>>(), expected);
    assert!(edits.contains(r#""role":"req-role""#));
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":5},"role":"req-role"}),
    );
    apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":6}}),
    );
    let lines: Vec<Value> = t
        .lines()
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["actor"]["role"], "req-role");
    assert_eq!(lines[1]["actor"]["role"], "env-role");
    // MCP: the call's `role` wins; without it, the environment.
    let config = ont::mcp::Config {
        store: t.store().into(),
        now: Some(NOW),
        client: "test".into(),
    };
    let mut server = ont::mcp::Server::new(config, &ont::cli::execute);
    for (request_id, role) in [("m1", Some("mcp-role")), ("m2", None)] {
        let mut arguments = json!({"action_type":"create_tag","parameters":{"label":request_id},"request_id":request_id});
        if let Some(role) = role {
            arguments["role"] = json!(role);
        }
        server
            .handle_line(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ont_action_apply","arguments":arguments}})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
    }
    let log = db.action_log().unwrap();
    let roles: Vec<&str> = log[3..]
        .iter()
        .map(|(_, e)| e.actor["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["mcp-role", "env-role"]);
}

// C1: the bounds of `role` are those of `agent`, refused at `/role`.
#[test]
fn the_role_has_the_bounds_of_agent() {
    let t = installed();
    let long = "r".repeat(65);
    for role in [
        json!(""),
        json!("a b"),
        json!(long),
        json!(5),
        json!(["lead"]),
    ] {
        for verb in ["apply", "validate"] {
            let (code, answer) = run(
                &t,
                &["action", verb],
                &json!({"action_type":"create_tag","parameters":{"label":"a"},"request_id":"r1","role":role}),
            );
            assert_eq!(code, 2, "{role}");
            assert_eq!(
                answer,
                json!({"status":"refused","code":"invalid_request","field":"/role",
                    "rule":"1 to 64 ASCII characters: letters, digits, '.', '_', ':', '-'",
                    "trust_limit":ont::TRUST_LIMIT}),
                "{verb} {role}"
            );
        }
    }
    let (code, _) = apply(
        &t,
        json!({"action_type":"create_tag","parameters":{"label":"a"},"request_id":"r1","role":"a.b_c:d-1"}),
    );
    assert_eq!(code, 0);
    let config = ont::mcp::Config {
        store: t.store().into(),
        now: Some(NOW),
        client: "test".into(),
    };
    let mut server = ont::mcp::Server::new(config, &ont::cli::execute);
    for role in [json!(5), json!("a b")] {
        let response = server
            .handle_line(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ont_action_validate",
                    "arguments":{"action_type":"create_tag","parameters":{"label":"a"},"role":role}}})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let text: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(text["field"], "/role", "{text}");
    }
    let mut ctx = Context::new(PathBuf::from(t.store()), NOW);
    ctx.role = Some("bad role".into());
    let request = ont::action::ValidateActionRequest {
        op: None,
        action_type: "create_tag".into(),
        parameters: json!({"label":"a"}),
        agent: None,
        session: None,
        expected: None,
        request_id: None,
    };
    match ont::action::validate_action(&ctx, &request) {
        Err(Failure::Refused(r)) => assert_eq!(r.extra["field"], "/role"),
        other => panic!("{other:?}"),
    }
}

// C2: a criterion on the actor's role holds for that actor, in validate and
// apply alike.
#[test]
fn a_role_criterion_holds_for_that_actor() {
    let t = installed();
    let request = |role: Option<&str>| {
        let mut r = json!({"action_type":"lead_tag","parameters":{"label":"l"},"request_id":"l1"});
        if let Some(role) = role {
            r["role"] = json!(role);
        }
        r
    };
    for (role, valid) in [(None, false), (Some("worker"), false), (Some("lead"), true)] {
        let (_, answer) = run(&t, &["action", "validate"], &request(role));
        assert_eq!(answer["result"]["valid"], valid, "{role:?} {answer}");
    }
    let (code, answer) = apply(&t, request(Some("worker")));
    assert_eq!(code, 2);
    assert_eq!(answer["failed"], json!(["lead_only"]), "{answer}");
    let (code, answer) = apply(&t, request(Some("lead")));
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["result"]["outcome"], "applied");
}

// C2: the role enters neither the derived action id nor, when absent, any
// line; C3: the trust stays `self_reported`.
#[test]
fn without_a_role_ids_and_lines_are_as_before() {
    let with = installed();
    let without = installed();
    let request = json!({"action_type":"create_tag","parameters":{"label":"same"},"request_id":"same","agent":"ag"});
    let mut with_role = request.clone();
    with_role["role"] = json!("lead");
    let (_, a) = apply(&with, with_role);
    let (_, b) = apply(&without, request);
    assert_eq!(a["result"]["action_id"], b["result"]["action_id"]);
    assert_eq!(a["result"]["edits"], b["result"]["edits"]);
    assert_eq!(a["result"]["actor_trust"], "self_reported");
    let mut db =
        ont_store::Store::open_schema_existing(std::path::Path::new(&without.store())).unwrap();
    let (seq, entry) = db.action_log().unwrap().remove(0);
    assert_eq!(
        entry.actor,
        json!({"actor_trust":"self_reported","agent":"ag","client":"cli","principal":"agent"})
    );
    assert_eq!(
        std::fs::read_to_string(without.0.join("edits.jsonl")).unwrap(),
        format!("{}\n", ont::action::log_line(seq, &entry))
    );
    let mut db =
        ont_store::Store::open_schema_existing(std::path::Path::new(&with.store())).unwrap();
    let entry = db.action_log().unwrap().remove(0).1;
    assert_eq!(entry.actor["role"], "lead");
    assert_eq!(entry.actor["actor_trust"], "self_reported");
}
