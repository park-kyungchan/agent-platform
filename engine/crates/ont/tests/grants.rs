//! Roles and grants (`specs/ont/action.md` R112 to R115, `specs/ont/fn.md`
//! R21 and R22, `specs/ont/schema-language.md` R28 to R30,
//! `specs/ont/store.md` R72 and R73). Temporary stores and synthetic domains
//! only. The command runs in process (`ont::run` with uid 0 passed); a
//! function is called through `ont::function::call`, whose `Context::role`
//! stands for `ONT_ROLE` (`./check` unsets it, and a test cannot set it). A
//! test's roles file names the principal `uid:<real uid of this process>`.
//! Every test takes one lock: a program written by one test must not be open
//! for writing while another test forks (`ETXTBSY`).
#![allow(clippy::unwrap_used)]
use ont::function::FnRequest;
use ont::{Answer, Context, Failure};
use serde_json::{Value, json};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

const NOW: u64 = 1000;
const ONT: &str = env!("CARGO_BIN_EXE_ont");
/// The uid a test that runs as root gives a file or starts a process under:
/// 65534 when this process's user namespace maps it, else the lowest mapped
/// uid other than 0; `None` when no other uid is mapped.
fn other_uid() -> Option<u32> {
    let map = std::fs::read_to_string("/proc/self/uid_map").ok()?;
    let ranges: Vec<(u32, u32)> = map
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace().map(|n| n.parse::<u64>().ok());
            let (inside, _, count) = (f.next()??, f.next()??, f.next()??);
            Some((u32::try_from(inside).ok()?, u32::try_from(count).ok()?))
        })
        .collect();
    let mapped = |uid: u32| {
        ranges.iter().any(|&(start, count)| {
            uid >= start && u64::from(uid) < u64::from(start) + u64::from(count)
        })
    };
    if mapped(65_534) {
        return Some(65_534);
    }
    ranges
        .iter()
        .filter_map(|&(start, count)| {
            let first = start.max(1);
            (count > 0 && mapped(first)).then_some(first)
        })
        .min()
}

static SERIAL: Mutex<()> = Mutex::new(());
fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ont-grants-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn store(&self) -> String {
        self.path("store.sqlite").display().to_string()
    }
    fn roles(&self) -> PathBuf {
        self.path("roles.json")
    }
    fn file(&self, name: &str, text: &str, mode: u32) -> String {
        let path = self.path(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path.display().to_string()
    }
    /// The program every function of the fixtures runs.
    fn program(&self) -> String {
        self.file(
            "answer.sh",
            "#!/bin/sh\ncat >/dev/null\necho '{\"ok\":true}'\n",
            0o755,
        )
    }
    /// `roles.json` with `text`, mode 0600, owned by this process.
    fn write_roles(&self, text: &str) {
        let _ = std::fs::remove_file(self.roles());
        self.file("roles.json", text, 0o600);
    }
    /// `roles.json` admitting this process's principal to `roles`.
    fn admit(&self, roles: &[&str]) {
        self.write_roles(&json!({"principals":{principal():roles}}).to_string());
    }
    /// The command in process (uid 0 passed): (exit code, stdout as JSON).
    fn run(&self, args: &[&str], stdin: &Value) -> (i32, Value) {
        let now = NOW.to_string();
        let store = self.store();
        let mut a: Vec<String> = ["--store", store.as_str(), "--now", now.as_str()]
            .iter()
            .map(|s| s.to_string())
            .collect();
        a.extend(args.iter().map(|s| s.to_string()));
        let out = ont::run(&a, stdin.to_string().as_bytes(), 0, 0);
        let value = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|_| panic!("stdout {:?} stderr {:?}", out.stdout, out.stderr));
        (out.code, value)
    }
    fn install(&self, schemas: &[Value]) {
        for (i, s) in schemas.iter().enumerate() {
            let file = self.path(&format!("schema-{i}.json"));
            std::fs::write(&file, s.to_string()).unwrap();
            let (code, out) = self.run(&["schema", "install", file.to_str().unwrap()], &json!({}));
            assert_eq!(code, 0, "{out}");
        }
    }
    fn apply(&self, request: Value) -> (i32, Value) {
        self.run(&["action", "apply"], &request)
    }
    fn validate(&self, request: Value) -> (i32, Value) {
        self.run(&["action", "validate"], &request)
    }
    /// `ont fn` through the library: `Ok(answer)` or `Err((code, refusal))`.
    fn call(&self, name: &str, domain: Option<&str>, role: Option<&str>) -> Result<Value, Value> {
        let mut ctx = Context::new(PathBuf::from(self.store()), NOW);
        ctx.role = role.map(str::to_string);
        let req = FnRequest {
            name: name.to_string(),
            domain: domain.map(str::to_string),
            parameters: None,
        };
        match ont::function::call(&ctx, &req) {
            Ok(Answer::Plain(v)) => Ok(v),
            Ok(_) => panic!("not a plain answer"),
            Err(Failure::Refused(r)) => {
                let mut v = Value::Object(r.extra);
                v["code"] = json!(r.code);
                Err(v)
            }
            Err(Failure::Internal(i)) => panic!("{i:?}"),
        }
    }
    /// The lines of `refusals.jsonl`, parsed.
    fn lines(&self) -> Vec<Value> {
        match std::fs::read_to_string(self.path("refusals.jsonl")) {
            Ok(text) => text
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
    /// The rows `ont log` returns for `request`.
    fn log(&self, request: Value) -> Value {
        let (code, out) = self.run(&["log"], &request);
        assert_eq!(code, 0, "{out}");
        out["result"].clone()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The real and effective uid of this process.
fn uids() -> (u32, u32) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status.lines().find_map(|l| l.strip_prefix("Uid:")).unwrap();
    let mut f = line.split_whitespace().map(|u| u.parse::<u32>().unwrap());
    (f.next().unwrap(), f.next().unwrap())
}
fn principal() -> String {
    format!("uid:{}", uids().0)
}
fn is_root() -> bool {
    uids() == (0, 0)
}

/// A schema of `domain` with the object type `<domain>_note`, the actions
/// `write_note` and `review_note`, the functions `write_fn` and
/// `review_fn` and the given `grants`.
fn schema(domain: &str, program: &str, grants: Value) -> Value {
    let note = format!("{domain}_note");
    let action = json!({"parameters":{"label":{"type":"string","required":true}},
        "rules":[{"create_object":note,"set":{"label":"$label"}}]});
    let function = json!({"parameters":{},"program":program,"answer_bytes":64,"timeout_ms":10_000});
    json!({
        "format":"ontology-schema/v1","domain":domain,"version":1,
        "namespace":{"instance":"test","account":"root"},
        "object_types":{note.clone():{"owner":"ontology","primary_key":{"from":"$action_id"},
            "properties":{"label":{"type":"string","required":true}}}},
        "action_types":{"write_note":action,"review_note":action},
        "functions":{"write_fn":function,"review_fn":function},
        "grants":grants
    })
}
/// A role-checked domain: every call reads the notes; the role `scribe`
/// applies `write_note` and calls `write_fn`, the role `auditor` applies
/// `review_note` and calls `review_fn`.
fn checked(domain: &str, program: &str) -> Value {
    let note = format!("{domain}_note");
    schema(
        domain,
        program,
        json!([
            {"principal":"agent","read":[note],"actions":[]},
            {"principal":"agent","role":"scribe","actions":["write_note"],"functions":["write_fn"]},
            {"principal":"agent","role":"auditor","actions":["review_note"],"functions":["review_fn"]}
        ]),
    )
}
/// A domain that is not role-checked: one grant, `write_note` only.
fn plain(domain: &str, program: &str) -> Value {
    let note = format!("{domain}_note");
    schema(
        domain,
        program,
        json!([{"principal":"agent","read":[note],"actions":["write_note"]}]),
    )
}

fn write(role: Option<&str>, request_id: Option<&str>) -> Value {
    let mut r = json!({"action_type":"write_note","parameters":{"label":"x"}});
    if let Some(role) = role {
        r["role"] = json!(role);
    }
    if let Some(id) = request_id {
        r["request_id"] = json!(id);
    }
    r
}

#[test]
fn outside_the_roles_grant_is_refused_not_granted_and_logged() {
    // spec: specs/ont/action.md#R114
    // spec: specs/ont/action.md#R115
    // spec: specs/ont/fn.md#R21
    // spec: specs/ont/fn.md#R22
    // spec: specs/ont/schema-language.md#R29
    // spec: specs/ont/action.md#R16
    let _serial = serial();
    let t = Temp::new();
    let program = t.program();
    t.install(&[checked("alpha", &program)]);
    t.admit(&["scribe", "auditor"]);

    // The action of `scribe`, in the other role and with no role.
    for role in [Some("auditor"), None] {
        let (code, out) = t.apply(write(role, None));
        assert_eq!(
            (code, out["code"].as_str()),
            (2, Some("not_granted")),
            "{out}"
        );
        let (code, out) = t.validate(write(role, None));
        assert_eq!(
            (code, out["code"].as_str()),
            (2, Some("not_granted")),
            "{out}"
        );
    }
    let lines = t.lines();
    assert_eq!(lines.len(), 2, "validate records nothing: {lines:?}");
    assert_eq!(lines[0]["code"], "not_granted");
    assert_eq!(lines[0]["action_type"], "write_note");
    assert_eq!(lines[0]["actor"]["role"], "auditor");
    assert!(lines[1]["actor"].get("role").is_none(), "{}", lines[1]);

    // The function of `scribe`, in the other role and with no role.
    for role in [Some("auditor"), None] {
        let out = t.call("write_fn", None, role).unwrap_err();
        assert_eq!(out["code"], "not_granted", "{out}");
        assert_eq!(out["function"], "write_fn");
        assert_eq!(out["domain"], "alpha");
    }
    let lines = t.lines();
    assert_eq!(lines.len(), 4, "{lines:?}");
    for line in &lines[2..] {
        let keys: Vec<&str> = line
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "actor",
                "code",
                "domain",
                "function",
                "outcome",
                "schema_version",
                "submitted_at"
            ]
        );
        assert_eq!(line["code"], "not_granted");
        assert_eq!(line["function"], "write_fn");
        assert_eq!(line["outcome"], "refused");
        assert_eq!(line["schema_version"], 1);
        assert_eq!(line["submitted_at"], NOW);
        assert_eq!(line["actor"]["principal"], principal());
        assert_eq!(line["actor"]["actor_trust"], "self_reported");
    }
    assert_eq!(lines[2]["actor"]["role"], "auditor");
    assert!(lines[3]["actor"].get("role").is_none());

    // In its own role each passes.
    let (code, out) = t.apply(write(Some("scribe"), Some("w-1")));
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["outcome"], "applied");
    assert_eq!(
        t.call("write_fn", None, Some("scribe")).unwrap()["answer"],
        json!({"ok":true})
    );
    let (code, out) =
        t.apply(json!({"action_type":"review_note","parameters":{"label":"y"},"role":"auditor"}));
    assert_eq!(code, 0, "{out}");
    assert!(t.call("review_fn", None, Some("auditor")).is_ok());

    // A retry of the applied request under the other role: the grant check
    // comes before the replay.
    let (code, out) = t.apply(write(Some("auditor"), Some("w-1")));
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("not_granted")),
        "{out}"
    );
    let (code, out) = t.apply(write(Some("scribe"), Some("w-1")));
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["result"]["outcome"], "already_applied");
    assert_eq!(t.lines().len(), 5);

    // `action list` and `ont log` take no role: every grant admits them.
    let (code, out) = t.run(&["action", "list"], &json!({}));
    assert_eq!(code, 0, "{out}");
    let listed: Vec<&str> = out["result"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action_type"].as_str().or(a["name"].as_str()).unwrap())
        .collect();
    assert!(
        listed.contains(&"write_note") && listed.contains(&"review_note"),
        "{out}"
    );
    for role in [Some("scribe"), Some("auditor"), None] {
        let mut ctx = Context::new(PathBuf::from(t.store()), NOW);
        ctx.role = role.map(str::to_string);
        let req = ont::log::LogRequest::parse(b"{}").unwrap();
        let Ok(Answer::Enveloped { result, .. }) = ont::log::log(&ctx, &req) else {
            panic!("log refused");
        };
        let types: Vec<&str> = result["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["action_type"].as_str().unwrap())
            .collect();
        assert_eq!(types, ["review_note", "write_note"], "{result}");
        let req = ont::action::ListActionsRequest::parse(b"{}").unwrap();
        assert!(ont::action::list_actions(&ctx, &req).is_ok());
    }
    // The refused rows: the action lines are returned, the two function
    // lines (no `action_type`) are counted in `skipped`.
    let refused = t.log(json!({"outcome":"refused"}));
    assert_eq!(refused["rows"].as_array().unwrap().len(), 3, "{refused}");
    assert_eq!(refused["skipped"], 2, "{refused}");
}

#[test]
fn role_the_file_does_not_admit_is_refused() {
    // spec: specs/ont/action.md#R112
    // spec: specs/ont/action.md#R113
    // spec: specs/ont/action.md#R115
    // spec: specs/ont/fn.md#R22
    let _serial = serial();
    let t = Temp::new();
    let program = t.program();
    t.install(&[checked("alpha", &program)]);
    let mut expected_lines = 0;
    let mut refused = |t: &Temp, detail: &str| {
        let (code, out) = t.validate(write(Some("scribe"), None));
        assert_eq!(
            (code, out["code"].as_str()),
            (2, Some("role_not_admitted")),
            "{out}"
        );
        assert_eq!(out["field"], "/role");
        assert_eq!(out["detail"], detail, "{out}");
        assert!(!out.to_string().contains("scribe"), "{out}");
        let (code, out) = t.apply(write(Some("scribe"), None));
        assert_eq!(
            (code, out["code"].as_str()),
            (2, Some("role_not_admitted")),
            "{out}"
        );
        assert_eq!(out["detail"], detail, "{out}");
        assert!(!out.to_string().contains("scribe"), "{out}");
        expected_lines += 1;
        let lines = t.lines();
        assert_eq!(lines.len(), expected_lines, "{lines:?}");
        let last = lines.last().unwrap();
        assert_eq!(last["code"], "role_not_admitted");
        assert_eq!(last["actor"]["role"], "scribe");
        let applied = t.log(json!({}));
        assert_eq!(applied["rows"], json!([]), "nothing is applied");
    };

    // No file.
    refused(&t, "roles_file_absent");
    // The principal is not listed, or not with this role.
    t.write_roles(&json!({"principals":{"uid:4000000001":["scribe"]}}).to_string());
    refused(&t, "role_not_listed");
    t.admit(&["auditor"]);
    refused(&t, "role_not_listed");
    // Unusable: a symbolic link to a good file.
    let good = t.file(
        "good-roles.json",
        &json!({"principals":{principal():["scribe"]}}).to_string(),
        0o600,
    );
    std::fs::remove_file(t.roles()).unwrap();
    std::os::unix::fs::symlink(&good, t.roles()).unwrap();
    refused(&t, "roles_file_unusable");
    std::fs::remove_file(t.roles()).unwrap();
    // A write bit for group.
    t.admit(&["scribe"]);
    std::fs::set_permissions(t.roles(), std::fs::Permissions::from_mode(0o620)).unwrap();
    refused(&t, "roles_file_unusable");
    // Faults of form.
    let p = principal();
    for text in [
        "not json".to_string(),
        "[]".to_string(),
        json!({"principals":{p.clone():"scribe"}}).to_string(),
        json!({"principals":{p.clone():[7]}}).to_string(),
        json!({"principals":{p.clone():["scribe"]},"other":1}).to_string(),
        json!({"principals":{p.clone():["scribe", "x".repeat(65)]}}).to_string(),
        json!({"principals":{"agent":["scribe"]}}).to_string(),
        json!({}).to_string(),
        format!(r#"{{"principals":{{"{p}":["scribe"],"{p}":["scribe"]}}}}"#),
    ] {
        t.write_roles(&text);
        refused(&t, "roles_file_unusable");
    }
    // Over 65,536 bytes, though of the form.
    let mut text = json!({"principals":{p.clone():["scribe"]}}).to_string();
    text.push_str(&" ".repeat(65_537 - text.len()));
    t.write_roles(&text);
    refused(&t, "roles_file_unusable");
    // Owned by another account (only root can give the file away).
    if is_root()
        && let Some(uid) = other_uid()
    {
        t.admit(&["scribe"]);
        std::os::unix::fs::chown(t.roles(), Some(uid), None).unwrap();
        refused(&t, "roles_file_unusable");
    }
    // A FIFO is refused at once: no call waits on it.
    std::fs::remove_file(t.roles()).unwrap();
    let status = Command::new("mkfifo").arg(t.roles()).status().unwrap();
    assert!(status.success());
    let store = t.store();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let now = NOW.to_string();
        let args: Vec<String> = ["--store", &store, "--now", &now, "action", "validate"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = ont::run(
            &args,
            write(Some("scribe"), None).to_string().as_bytes(),
            0,
            0,
        );
        let _ = tx.send(out.stdout);
    });
    let out: Value =
        serde_json::from_str(&rx.recv_timeout(Duration::from_secs(30)).unwrap()).unwrap();
    assert_eq!(out["code"], "role_not_admitted", "{out}");
    assert_eq!(out["detail"], "roles_file_unusable", "{out}");
    std::fs::remove_file(t.roles()).unwrap();

    // `fn` resolves the role the same way, and records its refusal.
    let out = t.call("write_fn", None, Some("scribe")).unwrap_err();
    assert_eq!(out["code"], "role_not_admitted", "{out}");
    assert_eq!(
        (out["field"].as_str(), out["detail"].as_str()),
        (Some("/role"), Some("roles_file_absent"))
    );
    assert_eq!(
        (out["function"].as_str(), out["domain"].as_str()),
        (Some("write_fn"), Some("alpha"))
    );
    let lines = t.lines();
    assert_eq!(lines.len(), expected_lines + 1);
    assert_eq!(lines.last().unwrap()["code"], "role_not_admitted");
    assert_eq!(lines.last().unwrap()["function"], "write_fn");

    // A claimed role outside R14's bound: `invalid_request` before the role
    // is resolved, with no line.
    let long = "r".repeat(65);
    for out in [
        t.validate(write(Some(&long), None)).1,
        t.apply(write(Some(&long), None)).1,
        t.call("write_fn", None, Some(&long)).unwrap_err(),
    ] {
        assert_eq!(out["code"], "invalid_request", "{out}");
        assert_eq!(out["field"], "/role", "{out}");
    }
    assert_eq!(t.lines().len(), expected_lines + 1);
}

#[test]
fn domain_without_role_grants_answers_as_before() {
    // spec: specs/ont/schema-language.md#R30
    // spec: specs/ont/schema-language.md#R29
    // spec: specs/ont/action.md#R114
    // spec: specs/ont/fn.md#R21
    let _serial = serial();
    let t = Temp::new();
    let program = t.program();
    t.install(&[plain("plain", &program)]);
    // A claimed role passes with no file; the refusal is `action_forbidden`.
    let (code, before) = t.validate(write(Some("scribe"), Some("v-1")));
    assert_eq!(code, 0, "{before}");
    let (code, out) =
        t.validate(json!({"action_type":"review_note","parameters":{"label":"x"},"role":"scribe"}));
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("action_forbidden")),
        "{out}"
    );
    // A roles file that would refuse the role is not read: the answer is the
    // same byte for byte.
    t.write_roles("not json");
    std::fs::set_permissions(t.roles(), std::fs::Permissions::from_mode(0o666)).unwrap();
    let (code, after) = t.validate(write(Some("scribe"), Some("v-1")));
    assert_eq!(code, 0, "{after}");
    assert_eq!(before.to_string(), after.to_string());
    let (code, out) = t.apply(write(Some("scribe"), None));
    assert_eq!(code, 0, "{out}");
    let row = &t.log(json!({}))["rows"][0];
    assert_eq!(
        row["actor"]["role"], "scribe",
        "the role stays the logged claim"
    );
    // Every function is callable, in any role and in none.
    for (name, role) in [("write_fn", Some("auditor")), ("review_fn", None)] {
        assert_eq!(
            t.call(name, None, role).unwrap()["answer"],
            json!({"ok":true})
        );
    }
    std::fs::remove_file(t.roles()).unwrap();

    // A store with a domain of each form.
    let t = Temp::new();
    let program = t.program();
    t.install(&[checked("alpha", &program), plain("plain", &program)]);
    t.admit(&["auditor"]);
    // A call found in `plain` gets no grant that names a role: the role
    // grant of `alpha` that lists `review_note` admits nothing there.
    let (code, out) = t.validate(json!({"action_type":"review_note","domain":"plain","parameters":{"label":"x"},"role":"auditor"}));
    assert_eq!(
        (code, out["code"].as_str()),
        (2, Some("action_forbidden")),
        "{out}"
    );
    let (code, out) = t.validate(json!({"action_type":"review_note","domain":"alpha","parameters":{"label":"x"},"role":"auditor"}));
    assert_eq!(code, 0, "{out}");
    // `plain` reads no roles file: a role the file does not admit passes.
    let (code, out) = t.apply(json!({"action_type":"write_note","domain":"plain","parameters":{"label":"x"},"role":"scribe"}));
    assert_eq!(code, 0, "{out}");
    assert!(t.call("review_fn", Some("plain"), Some("scribe")).is_ok());
    let out = t
        .call("review_fn", Some("alpha"), Some("scribe"))
        .unwrap_err();
    assert_eq!(out["code"], "role_not_admitted", "{out}");
    // The bound principal of a call without a resolved role holds no grant
    // that names a role; `Principal::bind` keeps every grant.
    let mut store = ont_store::Store::open_schema_existing(Path::new(&t.store())).unwrap();
    let set = store.schema_set().unwrap().clone();
    let call = ont::principal::Principal::bind_role(&set, None).unwrap();
    assert!(call.view().entries.iter().all(|e| e.role.is_none()));
    assert!(call.view().action_types("alpha").is_empty());
    let every = ont::principal::Principal::bind(&set).unwrap();
    assert!(every.view().action_types("alpha").contains("review_note"));
    let auditor = ont::principal::Principal::bind_role(&set, Some("auditor")).unwrap();
    assert_eq!(
        auditor
            .view()
            .action_types("alpha")
            .into_iter()
            .collect::<Vec<_>>(),
        ["review_note"]
    );
    assert!(ont::principal::role_checked(set.schema("alpha").unwrap()));
    assert!(!ont::principal::role_checked(set.schema("plain").unwrap()));
}

#[test]
fn one_account_may_claim_each_role_the_file_admits() {
    // spec: specs/ont/store.md#R73
    let _serial = serial();
    let t = Temp::new();
    let program = t.program();
    t.install(&[checked("alpha", &program)]);
    t.admit(&["scribe", "auditor"]);
    // One OS account, two roles, one call after the other: the role is a
    // claim the file admits, not a boundary between the callers.
    let (code, out) = t.apply(write(Some("scribe"), None));
    assert_eq!(code, 0, "{out}");
    let (code, out) =
        t.apply(json!({"action_type":"review_note","parameters":{"label":"y"},"role":"auditor"}));
    assert_eq!(code, 0, "{out}");
    assert!(t.call("write_fn", None, Some("scribe")).is_ok());
    assert!(t.call("review_fn", None, Some("auditor")).is_ok());
    let rows = t.log(json!({}))["rows"].clone();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for (row, role) in rows.iter().zip(["auditor", "scribe"]) {
        assert_eq!(row["actor"]["role"], role);
        assert_eq!(row["actor"]["principal"], principal());
        assert_eq!(row["actor"]["actor_trust"], "self_reported");
        assert!(
            row["actor"]["claimed"]
                .as_array()
                .unwrap()
                .contains(&json!("role"))
        );
    }
    // The account opens the store file without `ont`.
    assert!(std::fs::File::open(t.store()).is_ok());
}

#[test]
fn store_files_have_no_bit_for_group_or_others() {
    // spec: specs/ont/store.md#R72
    let _serial = serial();
    let t = Temp::new();
    let program = t.program();
    t.install(&[checked("alpha", &program)]);
    t.admit(&["scribe"]);
    let (code, out) = t.apply(write(Some("scribe"), None));
    assert_eq!(code, 0, "{out}");
    let (code, out) = t.apply(write(None, None));
    assert_eq!(code, 2, "{out}");
    for name in ["store.sqlite", "edits.jsonl", "refusals.jsonl"] {
        let mode = std::fs::metadata(t.path(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}: {mode:o}");
    }
}

#[test]
fn another_account_cannot_open_the_store() {
    // spec: specs/ont/store.md#R72
    let _serial = serial();
    if !is_root() {
        eprintln!("skipped: starting a process under another uid needs root");
        return;
    }
    let Some(uid) = other_uid() else {
        eprintln!("skipped: no uid other than 0 is mapped in this user namespace");
        return;
    };
    let t = Temp::new();
    let program = t.program();
    t.install(&[checked("alpha", &program)]);
    // A copy of `ont` in the test's directory: the build directory may be
    // closed to others.
    let ont = t.path("ont");
    std::fs::copy(ONT, &ont).unwrap();
    std::fs::set_permissions(&ont, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |program: &Path, args: &[&str]| {
        use std::os::unix::process::CommandExt;
        Command::new(program)
            .args(args)
            .uid(uid)
            .env_remove("ONT_ROLE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    };
    let store = t.store();
    let out = match run(&ont, &["--store", &store, "describe"]) {
        Ok(out) => out,
        // EPERM: a stage that holds no capability is uid 0 too.
        Err(e) if e.raw_os_error() == Some(1) => {
            eprintln!("skipped: the start under another uid is refused (EPERM)");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(answer["code"], "principal_not_trusted", "{answer}");
    // A plain open of the store file by that uid fails: the file's mode,
    // not `ont`, keeps the account out.
    let out = run(Path::new("/bin/cat"), &[&store]).unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(std::fs::File::open(&store).is_ok());
}
