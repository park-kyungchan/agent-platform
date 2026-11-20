//! Tests of `ont fn`, the function declarations, `ont_fn` and the read-only
//! rule. Temporary directories and temporary stores only. The programs are
//! synthetic shell scripts written by each test into its own directory.
//!
//! The calls run in process (`ont::run` with uid 0 passed, or the library
//! functions), so they hold in CI where the test user is not root; a test
//! that needs the built binary to read a store checks for root first, since
//! the binary refuses any other principal (`principal_not_trusted`). Every
//! test takes one lock: a script written by one test thread must not be
//! open for writing while another thread forks (`ETXTBSY`).
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const NOW: u64 = 1000;
const ONT: &str = env!("CARGO_BIN_EXE_ont");

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
            "ont-fn-{}-{nonce}-{}",
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
    /// A shell script `name` with mode 0755.
    fn program(&self, name: &str, body: &str) -> String {
        self.file(name, &format!("#!/bin/sh\n{body}\n"), 0o755)
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
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Real = effective uid 0, as `main.rs` reads it.
fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|f| f.split_whitespace().take(2).all(|u| u == "0"))
}

/// A schema of `domain` with one object type, one action and `functions`;
/// `granted` false gives the principal no grant (the domain is not listed).
fn schema(domain: &str, functions: Value, granted: bool) -> Value {
    let note = format!("{domain}_note");
    let grants = if granted {
        json!([{"principal":"agent","scope":{},"read":[note],"derive":[],"actions":["create_note"]}])
    } else {
        json!([])
    };
    json!({
        "format":"ontology-schema/v1","domain":domain,"version":1,
        "namespace":{"instance":"test","account":"root"},
        "object_types":{note.clone():{"owner":"ontology","primary_key":{"from":"$action_id"},
            "properties":{"label":{"type":"string","required":true}}}},
        "action_types":{"create_note":{"parameters":{"label":{"type":"string","required":true}},
            "rules":[{"create_object":note,"set":{"label":"$label"}}]}},
        "functions":functions,
        "grants":grants
    })
}

/// A function entry with the two parameters `word` (required) and `count`.
fn function(program: &str, answer_bytes: u64, timeout_ms: u64) -> Value {
    json!({
        "description":"A synthetic function.",
        "parameters":{
            "word":{"type":"string","required":true,"description":"A word."},
            "count":{"type":"integer","required":false}
        },
        "program":program,"answer_bytes":answer_bytes,"timeout_ms":timeout_ms
    })
}

/// The subcommand in process (uid 0 passed): (exit code, stdout as JSON).
fn sub(store: &str, args: &[&str], stdin: &[u8]) -> (i32, Value) {
    let now = NOW.to_string();
    let mut a: Vec<String> = ["--store", store, "--now", &now]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(args.iter().map(|s| s.to_string()));
    let out = ont::run(&a, stdin, 0, 0);
    let value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|_| panic!("stdout {:?} stderr {:?}", out.stdout, out.stderr));
    (out.code, value)
}

/// A store with `schemas` installed.
fn install(t: &Temp, schemas: &[Value]) -> String {
    let store = t.store();
    for (i, s) in schemas.iter().enumerate() {
        let file = t.path(&format!("schema-{i}.json"));
        std::fs::write(&file, s.to_string()).unwrap();
        let (code, out) = sub(&store, &["schema", "install", file.to_str().unwrap()], b"");
        assert_eq!(code, 0, "{out}");
    }
    store
}

fn call(store: &str, name: &str, request: Value) -> (i32, Value) {
    sub(store, &["fn", name], request.to_string().as_bytes())
}

/// A refusal of `ont fn`: the shape of the other read commands' refusals.
fn refused(answer: &(i32, Value), code: &str) -> Value {
    let (exit, body) = answer;
    assert_eq!(*exit, 2, "{body}");
    assert_eq!(body["status"], "refused", "{body}");
    assert_eq!(body["code"], code, "{body}");
    assert!(body["trust_limit"].is_string(), "{body}");
    body.clone()
}

fn violations(answer: &Value) -> BTreeSet<(String, String)> {
    answer["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["code"].as_str().unwrap().to_string(),
                v["pointer"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn schema_check_refuses_each_malformed_member_once() {
    // spec: specs/ont/fn.md#R1
    // spec: specs/ont/fn.md#R2
    let _g = serial();
    let t = Temp::new();
    let functions = json!({
        "no_program":{"parameters":{},"answer_bytes":10,"timeout_ms":10},
        "bounds":{"parameters":{},"program":"/bin/true","answer_bytes":0,"timeout_ms":600_001},
        "relative":{"parameters":{},"program":"bin/true","answer_bytes":1_048_576,"timeout_ms":600_000,"extra":1},
        "no_parameters":{"program":"/bin/true","answer_bytes":1,"timeout_ms":1},
        "params":{"parameters":{
            "now":{"type":"string","required":true},
            "bad":{"type":"text","required":true},
            "noreq":{"type":"string"},
            "dangling":{"type":{"ref":"missing"},"required":false},
            "deep":{"type":{"array":{"value_type":"missing"}},"required":false},
            "loose":{"type":"string","required":true,"max_bytes":3}
        },"program":"/bin/true","answer_bytes":1,"timeout_ms":1},
        "notobj":3,
        "Bad":{"parameters":{},"program":"/bin/true","answer_bytes":1,"timeout_ms":1}
    });
    let file = t.path("bad.json");
    std::fs::write(&file, schema("demo", functions, true).to_string()).unwrap();
    let answer = sub(
        &t.store(),
        &["schema", "check", file.to_str().unwrap()],
        b"",
    );
    let body = refused(&answer, "schema_violation");
    let f = |code: &str, p: &str| (code.to_string(), p.to_string());
    assert_eq!(
        violations(&body),
        BTreeSet::from([
            f("invalid_function", "/functions/no_program/program"),
            f("invalid_function", "/functions/bounds/answer_bytes"),
            f("invalid_function", "/functions/bounds/timeout_ms"),
            f("invalid_function", "/functions/relative/program"),
            f("invalid_function", "/functions/relative/extra"),
            f("invalid_function", "/functions/no_parameters/parameters"),
            f("invalid_function", "/functions/params/parameters/now"),
            f("invalid_function", "/functions/params/parameters/bad/type"),
            f(
                "invalid_function",
                "/functions/params/parameters/noreq/required"
            ),
            f(
                "invalid_function",
                "/functions/params/parameters/dangling/type"
            ),
            f("invalid_function", "/functions/params/parameters/deep/type"),
            f(
                "invalid_function",
                "/functions/params/parameters/loose/max_bytes"
            ),
            f("invalid_function", "/functions/notobj"),
            f("invalid_name", "/functions/Bad"),
        ])
    );
    // One diagnostic per fault: no pointer repeats.
    assert_eq!(
        body["violations"].as_array().unwrap().len(),
        violations(&body).len()
    );
    assert!(
        body["rule"].is_string() && body["field"].is_string(),
        "{body}"
    );
    // The same entries are refused by install, which writes nothing.
    let (code, out) = sub(
        &t.store(),
        &["schema", "install", file.to_str().unwrap()],
        b"",
    );
    assert_eq!((code, out["code"].as_str()), (2, Some("schema_violation")));
    // Every declared parameter type form is accepted.
    let good = json!({"typed":{"parameters":{
        "s":{"type":"string","required":true},
        "list":{"type":{"array":"integer"},"required":false},
        "pair":{"type":{"struct":{"a":"string","b":"boolean"}},"required":false},
        "target":{"type":{"ref":"demo_note"},"required":false},
        "on":{"type":"date","required":false}
    },"program":"/bin/true","answer_bytes":1_048_576,"timeout_ms":600_000}});
    std::fs::write(&file, schema("demo", good, true).to_string()).unwrap();
    let (code, out) = sub(
        &t.store(),
        &["schema", "check", file.to_str().unwrap()],
        b"",
    );
    assert_eq!(code, 0, "{out}");
}

#[test]
fn empty_functions_check_and_install_as_before_and_entries_install() {
    // spec: specs/ont/fn.md#R3
    let _g = serial();
    let t = Temp::new();
    let file = t.path("empty.json");
    std::fs::write(&file, schema("demo", json!({}), true).to_string()).unwrap();
    let (code, out) = sub(
        &t.store(),
        &["schema", "check", file.to_str().unwrap()],
        b"",
    );
    assert_eq!((code, &out["clean"]), (0, &json!(true)), "{out}");
    let (code, out) = sub(
        &t.store(),
        &["schema", "install", file.to_str().unwrap()],
        b"",
    );
    assert_eq!(code, 0, "{out}");
    let (_, described) = sub(&t.store(), &["describe"], b"");
    assert_eq!(described["result"]["functions"], json!([]));
    // A newer version that declares a function installs and opens.
    let mut next = schema("demo", json!({"echo":function("/bin/cat", 10, 10)}), true);
    next["version"] = json!(2);
    std::fs::write(&file, next.to_string()).unwrap();
    let (code, out) = sub(
        &t.store(),
        &["schema", "install", file.to_str().unwrap()],
        b"",
    );
    assert_eq!(code, 0, "{out}");
    let (code, described) = sub(&t.store(), &["describe"], b"");
    assert_eq!(code, 0, "{described}");
    assert_eq!(described["result"]["functions"][0]["name"], "echo");
}

#[test]
fn describe_lists_each_function_with_its_signature_and_not_its_program() {
    // spec: specs/ont/fn.md#R18
    let _g = serial();
    let t = Temp::new();
    let program = t.program("echo", "cat");
    let store = install(
        &t,
        &[
            schema("demo", json!({"echo":function(&program, 64, 500)}), true),
            schema(
                "hidden",
                json!({"secret":function(&program, 64, 500)}),
                false,
            ),
        ],
    );
    let (code, out) = sub(&store, &["describe"], b"");
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out["result"]["functions"],
        json!([{
            "domain":"demo","name":"echo","description":"A synthetic function.",
            "parameters":[
                {"name":"count","description":null,"type":"integer","required":false},
                {"name":"word","description":"A word.","type":"string","required":true}
            ],
            "answer_bytes":64,"timeout_ms":500
        }])
    );
    assert!(!out.to_string().contains(&program));
}

#[test]
fn a_function_answers_within_its_bounds() {
    // spec: specs/ont/fn.md#R8
    // spec: specs/ont/fn.md#R13
    let _g = serial();
    let t = Temp::new();
    let program = t.program(
        "probe",
        r#"params=$(cat)
names=$(env | sed 's/=.*//' | tr '\n' ' ')
printf '{"params":%s,"argc":%s,"pwd":"%s","store":"%s","name":"%s","read_only":"%s","env":"%s"}' \
  "$params" "$#" "$(pwd)" "$ONT_FN_STORE" "$ONT_FN_NAME" "$ONT_FN_READ_ONLY" "$names""#,
    );
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"probe":function(&program, 4096, 5000)}),
            true,
        )],
    );
    let (code, out) = call(
        &store,
        "probe",
        json!({"parameters":{"word":"hi","count":null}}),
    );
    assert_eq!(code, 0, "{out}");
    let keys: BTreeSet<&str> = out
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["function", "domain", "answer", "elapsed_ms"])
    );
    assert_eq!(
        (&out["function"], &out["domain"]),
        (&json!("probe"), &json!("demo"))
    );
    assert!(out["elapsed_ms"].is_u64());
    let answer = &out["answer"];
    assert_eq!(answer["params"], json!({"word":"hi"}));
    assert_eq!(answer["argc"], 0);
    assert_eq!(answer["pwd"], "/");
    assert_eq!(answer["store"], store);
    assert_eq!(answer["name"], "probe");
    assert_eq!(answer["read_only"], "1");
    // The shell adds its own variables (PWD, SHLVL, _ and OLDPWD); the rest
    // is exactly what the engine passed.
    let env: BTreeSet<&str> = answer["env"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .filter(|n| !matches!(*n, "PWD" | "SHLVL" | "_" | "OLDPWD"))
        .collect();
    assert_eq!(
        env,
        BTreeSet::from([
            "PATH",
            "HOME",
            "LANG",
            "ONT_FN_STORE",
            "ONT_FN_NAME",
            "ONT_FN_READ_ONLY"
        ])
    );
}

#[test]
fn the_request_is_one_strict_object() {
    // spec: specs/ont/fn.md#R4
    let _g = serial();
    let t = Temp::new();
    let program = t.program("const", "cat >/dev/null; printf '[1]'");
    let free = json!({"parameters":{},"program":program,"answer_bytes":16,"timeout_ms":5000});
    let store = install(&t, &[schema("demo", json!({"constant":free}), true)]);
    for empty in [&b""[..], b"  \n\t", b"{}"] {
        let (code, out) = sub(&store, &["fn", "constant"], empty);
        assert_eq!((code, &out["answer"]), (0, &json!([1])), "{out}");
    }
    let body = refused(
        &call(&store, "constant", json!({"name":"constant"})),
        "invalid_request",
    );
    assert!(body["field"].is_string());
    refused(&sub(&store, &["fn", "constant"], b"[]"), "invalid_request");
    refused(&sub(&store, &["fn", "constant"], b"{"), "invalid_request");
    let body = refused(
        &call(&store, "constant", json!({"parameters":[]})),
        "invalid_request",
    );
    assert_eq!(body["field"], "/parameters");
}

#[test]
fn the_function_is_found_among_the_domains_describe_lists() {
    // spec: specs/ont/fn.md#R5
    let _g = serial();
    let t = Temp::new();
    let program = t.program("echo", "cat");
    let store = install(
        &t,
        &[
            schema("demo", json!({"echo":function(&program, 256, 5000)}), true),
            schema("other", json!({"echo":function(&program, 256, 5000)}), true),
            schema(
                "hidden",
                json!({"only_hidden":function(&program, 256, 5000)}),
                false,
            ),
        ],
    );
    let params = json!({"word":"w"});
    refused(&call(&store, "missing", json!({})), "unknown_function");
    refused(
        &call(&store, "only_hidden", json!({"parameters":params})),
        "unknown_function",
    );
    refused(
        &call(
            &store,
            "only_hidden",
            json!({"parameters":params,"domain":"hidden"}),
        ),
        "unknown_function",
    );
    let body = refused(
        &call(&store, "echo", json!({"parameters":params})),
        "function_ambiguous",
    );
    assert_eq!(body["domains"], json!(["demo", "other"]));
    assert_eq!(body["field"], "/domain");
    for domain in ["demo", "other"] {
        let (code, out) = call(&store, "echo", json!({"parameters":params,"domain":domain}));
        assert_eq!(code, 0, "{out}");
        assert_eq!(out["domain"], domain);
        assert_eq!(out["answer"], params);
    }
    refused(
        &call(
            &store,
            "echo",
            json!({"parameters":params,"domain":"hidden"}),
        ),
        "unknown_function",
    );
    refused(
        &call(
            &t.path("absent.sqlite").display().to_string(),
            "echo",
            json!({}),
        ),
        "store_not_found",
    );
}

#[test]
fn parameters_are_checked_with_the_codes_of_action_parameters() {
    // spec: specs/ont/fn.md#R6
    let _g = serial();
    let t = Temp::new();
    let marker = t.path("ran");
    let program = t.program("echo", &format!("touch '{}'; cat", marker.display()));
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"echo":function(&program, 256, 5000)}),
            true,
        )],
    );
    for (parameters, field, rule) in [
        (json!({}), "/parameters/word", "required parameter"),
        (
            json!({"word":"a","extra":1}),
            "/parameters/extra",
            "parameter must be declared by describe",
        ),
        (json!({"word":1}), "/parameters/word", ""),
        (json!({"word":"a","count":"2"}), "/parameters/count", ""),
    ] {
        let body = refused(
            &call(&store, "echo", json!({"parameters":parameters})),
            "invalid_parameter",
        );
        assert_eq!(body["field"], field, "{body}");
        assert_eq!(body["parameter"], field.rsplit('/').next().unwrap());
        if !rule.is_empty() {
            assert_eq!(body["rule"], rule);
        }
    }
    // The program did not start for a refused call.
    assert!(!marker.exists());
    let (code, out) = call(&store, "echo", json!({"parameters":{"word":"a","count":2}}));
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["answer"], json!({"count":2,"word":"a"}));
    assert!(marker.exists());
}

#[test]
fn the_program_file_is_checked_before_it_starts() {
    // spec: specs/ont/fn.md#R7
    let _g = serial();
    let t = Temp::new();
    std::fs::create_dir(t.path("dir")).unwrap();
    let cases: Vec<(String, &str)> = vec![
        (t.path("absent").display().to_string(), "not_found"),
        (t.path("dir").display().to_string(), "not_regular_file"),
        (
            t.file("plain", "#!/bin/sh\necho 1\n", 0o644),
            "not_executable",
        ),
        (
            t.file("group", "#!/bin/sh\necho 1\n", 0o775),
            "writable_by_group_or_others",
        ),
        (
            t.file("others", "#!/bin/sh\necho 1\n", 0o757),
            "writable_by_group_or_others",
        ),
        (t.file("garbage", "not a program\n", 0o755), "start_failed"),
    ];
    let mut functions = serde_json::Map::new();
    for (i, (program, _)) in cases.iter().enumerate() {
        functions.insert(format!("f{i}"), function(program, 16, 5000));
    }
    let store = install(&t, &[schema("demo", Value::Object(functions), true)]);
    for (i, (_, detail)) in cases.iter().enumerate() {
        let body = refused(
            &call(&store, &format!("f{i}"), json!({"parameters":{"word":"w"}})),
            "function_program_unusable",
        );
        assert_eq!(body["detail"], *detail, "{body}");
        assert_eq!(body["function"], format!("f{i}"));
        assert_eq!(body["domain"], "demo");
    }
    assert_eq!(
        ont::function::program_condition("bin/true"),
        Err("not_absolute")
    );
    if is_root() {
        // Owned by neither uid 0 nor the caller's real uid.
        let program = t.program("foreign", "echo 1");
        std::os::unix::fs::chown(&program, Some(65_534), None).unwrap();
        assert_eq!(
            ont::function::program_condition(&program),
            Err("owner_not_trusted")
        );
    }
}

#[test]
fn an_answer_over_its_bound_is_refused_and_standard_error_is_only_counted() {
    // spec: specs/ont/fn.md#R9
    let _g = serial();
    let t = Temp::new();
    let endless = t.program("endless", "cat >/dev/null; yes");
    let long = t.program(
        "long",
        "cat >/dev/null; printf '\"%s\"' $(head -c 200 /dev/zero | tr '\\0' a)",
    );
    let exact = t.program("exact", "cat >/dev/null; printf '\"abcd\"'");
    let noisy = t.program(
        "noisy",
        "cat >/dev/null; head -c 300000 /dev/zero >&2; printf '{\"ok\":true}'",
    );
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({
                "endless":function(&endless, 1000, 20_000),
                "long":function(&long, 100, 20_000),
                "exact":function(&exact, 6, 20_000),
                "noisy":function(&noisy, 11, 20_000)
            }),
            true,
        )],
    );
    let p = json!({"parameters":{"word":"w"}});
    let started = Instant::now();
    let body = refused(
        &call(&store, "endless", p.clone()),
        "function_answer_too_large",
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(body["answer_bytes"], 1000);
    refused(
        &call(&store, "long", p.clone()),
        "function_answer_too_large",
    );
    let (code, out) = call(&store, "exact", p.clone());
    assert_eq!((code, &out["answer"]), (0, &json!("abcd")), "{out}");
    let (code, out) = call(&store, "noisy", p);
    assert_eq!((code, &out["answer"]), (0, &json!({"ok":true})), "{out}");
}

#[test]
fn past_its_time_the_program_group_is_killed() {
    // spec: specs/ont/fn.md#R10
    let _g = serial();
    let t = Temp::new();
    let pid_file = t.path("child.pid");
    let slow = t.program(
        "slow",
        &format!("sleep 30 &\necho $! > '{}'\nsleep 30", pid_file.display()),
    );
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"slow":function(&slow, 16, 300)}),
            true,
        )],
    );
    let started = Instant::now();
    let body = refused(
        &call(&store, "slow", json!({"parameters":{"word":"w"}})),
        "function_timeout",
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(body["timeout_ms"], 300);
    // The program's own child was in its group and is gone too.
    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .to_string();
    let gone = |pid: &str| {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| {
                s.rsplit(')')
                    .next()
                    .unwrap_or("")
                    .trim_start()
                    .starts_with('Z')
            })
            .unwrap_or(true)
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !gone(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(gone(&pid), "process {pid} outlived the group kill");
}

#[test]
fn a_nonzero_exit_is_a_failure_with_its_code_and_stderr_count() {
    // spec: specs/ont/fn.md#R11
    let _g = serial();
    let t = Temp::new();
    let failing = t.program(
        "failing",
        "cat >/dev/null; echo '{}'; echo oops >&2; exit 3",
    );
    let killed = t.program("killed", "cat >/dev/null; kill -9 $$");
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"failing":function(&failing, 16, 5000),"killed":function(&killed, 16, 5000)}),
            true,
        )],
    );
    let p = json!({"parameters":{"word":"w"}});
    let body = refused(&call(&store, "failing", p.clone()), "function_failed");
    assert_eq!(body["detail"], json!({"exit":3,"stderr_bytes":5}));
    let body = refused(&call(&store, "killed", p), "function_failed");
    assert_eq!(body["detail"], json!({"exit":null,"stderr_bytes":0}));
}

#[test]
fn an_answer_that_is_not_one_json_value_is_refused() {
    // spec: specs/ont/fn.md#R12
    let _g = serial();
    let t = Temp::new();
    let mut functions = serde_json::Map::new();
    for (i, out) in ["echo hello", "true", "echo 1 2", "printf '{\"a\":'"]
        .iter()
        .enumerate()
    {
        let program = t.program(&format!("p{i}"), &format!("cat >/dev/null; {out}"));
        functions.insert(format!("p{i}"), function(&program, 64, 5000));
    }
    let store = install(&t, &[schema("demo", Value::Object(functions), true)]);
    for i in 0..4 {
        refused(
            &call(&store, &format!("p{i}"), json!({"parameters":{"word":"w"}})),
            "function_answer_not_json",
        );
    }
}

#[test]
fn the_pipes_are_served_without_a_thread() {
    // spec: specs/ont/fn.md#R14
    let _g = serial();
    let t = Temp::new();
    // Writes 300 KB to stderr before it reads a 60 KB input: one blocking
    // pipe at a time would wait forever.
    let busy = t.program("busy", "head -c 300000 /dev/zero >&2; wc -c | tr -d ' \\n'");
    let threads = t.program("threads", "cat >/dev/null; ls /proc/$PPID/task | wc -l");
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"busy":function(&busy, 32, 20_000),"threads":function(&threads, 32, 20_000)}),
            true,
        )],
    );
    let word = "w".repeat(60_000);
    let (code, out) = call(&store, "busy", json!({"parameters":{"word":word}}));
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["answer"], json!({"word":word}).to_string().len());
    if is_root() {
        // The program's parent is the `ont` process: it has one thread.
        let output = Command::new(ONT)
            .args(["--store", &store, "fn", "threads"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(br#"{"parameters":{"word":"w"}}"#)?;
                child.wait_with_output()
            })
            .unwrap();
        let out: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(out["answer"], 1, "{out}");
    }
}

/// The built binary with `ONT_FN_READ_ONLY` set to `value`: (exit, stdout).
fn spawn_read_only(value: &str, store: &str, args: &[&str], stdin: &[u8]) -> (i32, Value) {
    let mut child = Command::new(ONT)
        .env("ONT_FN_READ_ONLY", value)
        .args(["--store", store])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A refusal can come before `ont` reads its input: a closed pipe here
    // is no failure of the test.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap_or(Value::Null),
    )
}

fn store_files(t: &Temp) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(&t.0)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| (p.display().to_string(), std::fs::read(&p).unwrap()))
        .collect();
    files.sort();
    files
}

#[test]
fn inside_a_program_writing_subcommands_are_refused_before_the_uid_check() {
    // spec: specs/ont/fn.md#R15
    let _g = serial();
    let t = Temp::new();
    let store = install(&t, &[schema("demo", json!({}), true)]);
    let schema_file = t.path("schema-0.json").display().to_string();
    let board = t.file("board.md", "board\n", 0o644);
    let before = store_files(&t);
    let apply = br#"{"action_type":"create_note","parameters":{"label":"x"},"request_id":"r-1"}"#;
    let commit = "0".repeat(40);
    let writing: Vec<(Vec<&str>, &[u8])> = vec![
        (vec!["schema", "install", &schema_file], b""),
        (
            vec![
                "schema", "install", "--from", "/", "--commit", &commit, "--path", "x.json",
            ],
            b"",
        ),
        (vec!["backup"], b""),
        (vec!["compact"], b""),
        (vec!["index"], b""),
        (vec!["action", "apply"], apply),
        (
            vec![
                "render",
                "--write",
                &board,
                "--section",
                "v",
                "--preimage-dir",
                "/tmp",
            ],
            b"",
        ),
    ];
    for (args, stdin) in &writing {
        let (code, out) = spawn_read_only("1", &store, args, stdin);
        assert_eq!(
            (code, out["code"].as_str()),
            (2, Some("read_only_call")),
            "{args:?}: {out}"
        );
        assert_eq!(out["status"], "refused");
    }
    assert_eq!(store_files(&t), before);
    assert!(!t.path("refusals.jsonl").exists());
    assert!(!t.path("backup").exists());
    // Another value of the variable is not read only, and a read command is
    // not refused read_only_call: as root it answers, else the uid check
    // refuses it as before.
    let (_, out) = spawn_read_only("0", &store, &["action", "apply"], apply);
    assert_ne!(out["code"], "read_only_call", "{out}");
    let (code, out) = spawn_read_only("1", &store, &["describe"], b"{}");
    if is_root() {
        assert_eq!((code, &out["status"]), (0, &json!("ok")), "{out}");
    } else {
        assert_eq!(out["code"], "principal_not_trusted", "{out}");
    }
}

#[test]
fn a_write_from_inside_a_program_is_refused() {
    // spec: specs/ont/fn.md#R15
    let _g = serial();
    let t = Temp::new();
    let writer = t.program(
        "writer",
        &format!(
            r#"cat >/dev/null
echo '{{"action_type":"create_note","parameters":{{"label":"x"}},"request_id":"r-1"}}' | '{ONT}' --store "$ONT_FN_STORE" action apply || true"#
        ),
    );
    let reader = t.program(
        "reader",
        &format!(
            r#"cat >/dev/null; echo '{{}}' | '{ONT}' --store "$ONT_FN_STORE" describe || true"#
        ),
    );
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"writer":function(&writer, 4096, 20_000),"reader":function(&reader, 65_536, 20_000)}),
            true,
        )],
    );
    let p = json!({"parameters":{"word":"w"}});
    let (code, out) = call(&store, "writer", p.clone());
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["answer"]["code"], "read_only_call", "{out}");
    let log = ont_store::Store::open_schema_existing(Path::new(&store))
        .unwrap()
        .action_log()
        .unwrap()
        .len();
    assert_eq!(log, 0);
    assert!(!t.path("refusals.jsonl").exists());
    let (code, out) = call(&store, "reader", p);
    assert_eq!(code, 0, "{out}");
    if is_root() {
        assert_eq!(out["answer"]["status"], "ok", "{out}");
    } else {
        assert_eq!(out["answer"]["code"], "principal_not_trusted", "{out}");
    }
}

#[test]
fn read_only_library_operations_refuse_before_writing_and_reads_work() {
    // spec: specs/ont/fn.md#R16
    let _g = serial();
    let t = Temp::new();
    let program = t.program("echo", "cat");
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"echo":function(&program, 256, 5000)}),
            true,
        )],
    );
    let board = t.file("board.md", "board\n", 0o644);
    let before = store_files(&t);
    let mut ctx = ont::Context::new(PathBuf::from(&store), NOW);
    ctx.read_only = true;
    let refused_ro = |r: Result<ont::Answer, ont::Failure>| match r {
        Err(ont::Failure::Refused(r)) => assert_eq!(r.code, "read_only_call"),
        other => panic!("{other:?}"),
    };
    let apply = ont::action::ApplyActionRequest::parse(
        br#"{"action_type":"create_note","parameters":{"label":"x"},"request_id":"r-1"}"#,
    )
    .unwrap();
    refused_ro(ont::action::apply_action(&ctx, &apply));
    refused_ro(ont::schema::install(
        &ctx,
        &ont::schema::InstallRequest {
            file: t.path("schema-0.json"),
            source_repository: None,
            source_commit: None,
        },
    ));
    refused_ro(ont::schema::install_commit(
        &ctx,
        &ont::schema::CommitInstallRequest {
            checkout: PathBuf::from("/"),
            commit: "0".repeat(40),
            path: "x.json".into(),
        },
    ));
    refused_ro(ont::backup::backup(&ctx, &Default::default()));
    refused_ro(ont::compact::compact(&ctx, &Default::default()));
    refused_ro(ont::index::index(&ctx, &Default::default()));
    refused_ro(ont::render::write_file(
        &ctx,
        Path::new(&board),
        &ont::render::RenderRequest {
            op: None,
            view: "v".into(),
            args: Default::default(),
        },
        &t.0,
    ));
    // A missing store is not even looked at.
    let mut absent = ont::Context::new(t.path("absent.sqlite"), NOW);
    absent.read_only = true;
    refused_ro(ont::action::apply_action(&absent, &apply));
    refused_ro(ont::index::index(&absent, &Default::default()));
    assert!(!t.path("absent.sqlite").exists());
    assert_eq!(store_files(&t), before);
    assert!(!t.path("refusals.jsonl").exists());
    // The read operations work in a read-only context.
    let ok = |r: Result<ont::Answer, ont::Failure>| assert!(r.is_ok(), "{r:?}");
    ok(ont::describe::describe(&ctx, &Default::default()));
    ok(ont::action::validate_action(
        &ctx,
        &ont::action::ValidateActionRequest::parse(
            br#"{"action_type":"create_note","parameters":{"label":"x"}}"#,
        )
        .unwrap(),
    ));
    ok(ont::search::search(
        &ctx,
        &ont::search::SearchRequest::parse(br#"{"object_type":"demo_note"}"#).unwrap(),
    ));
    ok(ont::log::log(
        &ctx,
        &ont::log::LogRequest::parse(b"{}").unwrap(),
    ));
    ok(ont::function::call(
        &ctx,
        &ont::function::FnRequest::parse(br#"{"name":"echo","parameters":{"word":"w"}}"#).unwrap(),
    ));
    assert_eq!(store_files(&t), before);
}

fn tool(server: &mut ont::mcp::Server<'_>, name: &str, arguments: Value) -> (bool, String) {
    let response = server
        .handle_line(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":name,"arguments":arguments}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    (
        response["result"]["isError"].as_bool().unwrap(),
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string(),
    )
}

fn config(store: &str, read_only: bool) -> ont::mcp::Config {
    ont::mcp::Config {
        store: store.into(),
        now: Some(NOW),
        client: "test".into(),
        read_only,
    }
}

#[test]
fn a_read_only_server_refuses_the_writing_tools_before_it_opens_a_store() {
    // spec: specs/ont/fn.md#R17
    let _g = serial();
    let t = Temp::new();
    let absent = t.path("absent.sqlite").display().to_string();
    let mut server = ont::mcp::Server::new(config(&absent, true), &ont::cli::execute);
    let apply = json!({"action_type":"create_note","parameters":{"label":"x"},"request_id":"r-1"});
    for (name, arguments) in [
        ("ont_action_apply", apply.clone()),
        ("ont_index", json!({})),
    ] {
        let (is_error, text) = tool(&mut server, name, arguments);
        let body: Value = serde_json::from_str(&text).unwrap();
        assert!(is_error);
        assert_eq!(body["code"], "read_only_call", "{name}: {body}");
    }
    // A read tool still reaches the store.
    let (_, text) = tool(&mut server, "ont_describe", json!({}));
    assert!(text.contains("store_not_found"), "{text}");
    assert!(!t.path("absent.sqlite").exists());
    assert!(!t.path("refusals.jsonl").exists());
    // On a store: the writing tool writes nothing, reads and ont_fn answer.
    let program = t.program("echo", "cat");
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"echo":function(&program, 256, 5000)}),
            true,
        )],
    );
    let before = store_files(&t);
    let mut server = ont::mcp::Server::new(config(&store, true), &ont::cli::execute);
    let (is_error, text) = tool(&mut server, "ont_action_apply", apply);
    assert!(is_error && text.contains("read_only_call"), "{text}");
    let (is_error, text) = tool(
        &mut server,
        "ont_fn",
        json!({"name":"echo","parameters":{"word":"w"}}),
    );
    assert!(!is_error, "{text}");
    assert_eq!(store_files(&t), before);
    // The binary keeps its uid check at the start of `ont mcp`.
    let mut child = Command::new(ONT)
        .env("ONT_FN_READ_ONLY", "1")
        .args(["--store", &store, "mcp", "--client", "test"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // As a non-root user the binary refuses before it reads its input.
    let _ = child.stdin.take().unwrap().write_all(
        format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"ont_action_apply","arguments":
                    {"action_type":"create_note","parameters":{"label":"x"},"request_id":"r-2"}}})
        )
        .as_bytes(),
    );
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    if is_root() {
        assert!(text.contains("read_only_call"), "{text}");
    } else {
        assert!(text.contains("principal_not_trusted"), "{text}");
    }
    assert_eq!(store_files(&t), before);
}

#[test]
fn the_mcp_tool_ont_fn_runs_the_same_operation() {
    // spec: specs/ont/fn.md#R19
    let _g = serial();
    let t = Temp::new();
    let echo = t.program("echo", "cat");
    let big = t.program(
        "big",
        "cat >/dev/null; printf '\"'; head -c 300000 /dev/zero | tr '\\0' a; printf '\"'",
    );
    let store = install(
        &t,
        &[schema(
            "demo",
            json!({"echo":function(&echo, 256, 5000),"big":function(&big, 1_048_576, 20_000)}),
            true,
        )],
    );
    let listed = ont::mcp::tools_list();
    let spec = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ont_fn")
        .unwrap()
        .clone();
    assert_eq!(spec["inputSchema"]["required"], json!(["name"]));
    let properties: BTreeSet<&str> = spec["inputSchema"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(properties, BTreeSet::from(["name", "domain", "parameters"]));
    let mut server = ont::mcp::Server::new(config(&store, false), &ont::cli::execute);
    let without_time = |mut v: Value| {
        v.as_object_mut().unwrap().remove("elapsed_ms");
        v
    };
    let arguments = json!({"name":"echo","parameters":{"word":"w"}});
    let (is_error, text) = tool(&mut server, "ont_fn", arguments);
    assert!(!is_error, "{text}");
    let (code, out) = call(&store, "echo", json!({"parameters":{"word":"w"}}));
    assert_eq!(code, 0);
    assert_eq!(
        without_time(serde_json::from_str(&text).unwrap()),
        without_time(out)
    );
    // A refusal is the subcommand's line.
    let (is_error, text) = tool(&mut server, "ont_fn", json!({"name":"missing"}));
    assert!(is_error);
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        call(&store, "missing", json!({})).1
    );
    let (is_error, text) = tool(&mut server, "ont_fn", json!({"name":"echo","extra":1}));
    assert!(is_error && text.contains("invalid_request"), "{text}");
    // Over the response bound of the server, as every tool's answer.
    let (code, out) = call(&store, "big", json!({"parameters":{"word":"w"}}));
    assert_eq!(code, 0);
    assert_eq!(out["answer"].as_str().unwrap().len(), 300_000);
    let (is_error, text) = tool(
        &mut server,
        "ont_fn",
        json!({"name":"big","parameters":{"word":"w"}}),
    );
    assert!(is_error);
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["code"],
        "response_too_large"
    );
}

#[test]
fn help_lists_the_form_of_fn() {
    // spec: specs/ont/fn.md#R20
    let _g = serial();
    let out = ont::run(&["help".to_string()], &b""[..], 0, 0);
    let help: Value = serde_json::from_str(&out.stdout).unwrap();
    let entry = help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "fn")
        .unwrap()
        .clone();
    assert_eq!(entry["form"], "fn NAME");
    assert_eq!(entry["stdin_json"], true);
    let out = ont::run(&["fn".to_string(), "--help".to_string()], &b""[..], 0, 0);
    assert!(out.stdout.contains("fn NAME"), "{}", out.stdout);
    // Without a name the command is not known.
    let out = ont::run(
        &[
            "--store".into(),
            "/nonexistent-ont-fn/s.sqlite".into(),
            "fn".into(),
        ],
        &b"{}"[..],
        0,
        0,
    );
    assert!(out.stdout.contains("invalid_argument"), "{}", out.stdout);
}
