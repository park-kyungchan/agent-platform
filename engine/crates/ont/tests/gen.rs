//! `ont schema gen FILE` (`specs/ont/gen.md` R13): the text of
//! `ont_gen::generate` on standard output, its refusals as one JSON line, no
//! uid check, no store, no standard input. Synthetic schemas and scratch
//! directories only.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Temp(PathBuf);
impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ont-gen-cli-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.display().to_string()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> (String, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ont-gen/tests/fixtures")
        .join(format!("{name}.schema.json"));
    let bytes = std::fs::read(&path).unwrap();
    (path.display().to_string(), bytes)
}

fn gen_run(args: &[&str], real: u32, effective: u32) -> ont::Outcome {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    // A standard input that holds a request must not be read.
    ont::run(&args, &b"{\"unread\":true}"[..], real, effective)
}

fn refusal(out: &ont::Outcome) -> Value {
    assert_eq!(out.code, 2, "{}", out.stdout);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout.matches('\n').count(), 1, "{}", out.stdout);
    let body: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(body["status"], "refused", "{body}");
    assert!(body["trust_limit"].is_string(), "{body}");
    body
}

fn colliding() -> Vec<u8> {
    let thing = json!({
        "owner": "ontology",
        "primary_key": {"from": "$action_id"},
        "properties": {"label": {"type": "string", "required": true}}
    });
    json!({
        "format": "ontology-schema/v1",
        "domain": "sample",
        "version": 1,
        "namespace": {"instance": "test", "account": "sample"},
        "object_types": {"a_b": thing, "a__b": thing},
        "grants": []
    })
    .to_string()
    .into_bytes()
}

// spec: specs/ont/gen.md#R13
#[test]
fn the_output_is_the_generated_text_byte_for_byte() {
    for name in ["example", "hostile"] {
        let (path, bytes) = fixture(name);
        let want = ont_gen::generate(&bytes).unwrap();
        let out = gen_run(&["schema", "gen", &path], 1000, 1000);
        assert_eq!(out.code, 0, "{}", out.stdout);
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout, want, "{name}");
        assert!(!out.stdout.ends_with("\n\n"));
    }
}

// spec: specs/ont/gen.md#R13
#[test]
fn a_refused_schema_gives_the_violations_of_the_generator() {
    let t = Temp::new("refused");
    let cases: [(&str, Vec<u8>); 2] = [
        ("collision", colliding()),
        ("invalid", b"not json".to_vec()),
    ];
    for (name, bytes) in cases {
        let want = ont_gen::generate(&bytes).unwrap_err();
        assert!(!want.is_empty());
        let file = t.file(&format!("{name}.schema.json"), &bytes);
        let out = gen_run(&["schema", "gen", &file], 1000, 1000);
        let body = refusal(&out);
        assert_eq!(body["code"], "schema_gen_refused", "{body}");
        assert_eq!(body["violations"], json!(want), "{name}");
        assert!(body.get("result").is_none());
    }
    let out = gen_run(
        &[
            "schema",
            "gen",
            &t.file("collision.schema.json", &colliding()),
        ],
        1000,
        1000,
    );
    assert_eq!(refusal(&out)["violations"][0]["code"], "name_collision");
}

// spec: specs/ont/gen.md#R13
#[test]
fn a_missing_file_is_unreadable_with_its_path() {
    let t = Temp::new("missing");
    let path = t.0.join("absent.schema.json").display().to_string();
    let body = refusal(&gen_run(&["schema", "gen", &path], 1000, 1000));
    assert_eq!(body["code"], "schema_file_unreadable", "{body}");
    assert_eq!(body["path"], path);
}

// spec: specs/ont/gen.md#R13
#[test]
fn the_form_is_one_file_and_no_option_of_its_own() {
    let (path, _) = fixture("example");
    for args in [
        vec!["schema", "gen"],
        vec!["schema", "gen", &path, &path],
        vec!["schema", "gen", &path, "--client", "x"],
        vec!["schema", "gen", &path, "--write", "x"],
    ] {
        let body = refusal(&gen_run(&args, 0, 0));
        assert_eq!(body["code"], "invalid_argument", "{args:?} {body}");
    }
    for args in [vec!["help"], vec!["schema", "gen", "--help"]] {
        let out = gen_run(&args, 1000, 1000);
        let help: Value = serde_json::from_str(&out.stdout).unwrap();
        let entry = help["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["command"] == "schema gen")
            .unwrap();
        assert_eq!(entry["form"], "schema gen FILE");
        assert_eq!(entry["stdin_json"], false);
        assert_eq!(entry["options"], json!([]));
    }
}

// spec: specs/ont/gen.md#R13
#[test]
fn it_answers_alike_for_every_uid_and_the_other_commands_keep_the_check() {
    let (path, bytes) = fixture("example");
    let want = ont_gen::generate(&bytes).unwrap();
    for (real, effective) in [(0, 0), (1000, 1000), (0, 1000), (1000, 0)] {
        let out = gen_run(&["schema", "gen", &path], real, effective);
        assert_eq!((out.code, out.stdout.as_str()), (0, want.as_str()));
    }
    let out = gen_run(&["schema", "check", &path], 1000, 1000);
    assert_eq!(refusal(&out)["code"], "principal_not_trusted");
}

// spec: specs/ont/gen.md#R13
#[test]
fn the_checks_before_the_uid_check_hold() {
    let (path, _) = fixture("example");
    let body = refusal(&gen_run(
        &["--now", "5", "schema", "gen", &path],
        1000,
        1000,
    ));
    assert_eq!(body["code"], "now_requires_store", "{body}");
    let body = refusal(&gen_run(
        &[
            "--now",
            "5",
            "--store",
            ont::DEFAULT_STORE,
            "schema",
            "gen",
            &path,
        ],
        1000,
        1000,
    ));
    assert_eq!(body["code"], "now_at_default_store", "{body}");
}

// spec: specs/ont/gen.md#R13
#[test]
fn a_store_path_is_never_created() {
    let t = Temp::new("store");
    let (path, bytes) = fixture("example");
    let store = t.0.join("ontology.sqlite");
    let out = gen_run(
        &[
            "--store",
            &store.display().to_string(),
            "schema",
            "gen",
            &path,
        ],
        1000,
        1000,
    );
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, ont_gen::generate(&bytes).unwrap());
    assert!(!store.exists());
    let before: Vec<_> = std::fs::read_dir(&t.0).unwrap().collect();
    assert!(before.is_empty());
    // A store path whose directory is absent is not a refusal either.
    let out = gen_run(
        &[
            "--store",
            "/nonexistent-dir/x.sqlite",
            "schema",
            "gen",
            &path,
        ],
        1000,
        1000,
    );
    assert_eq!(out.code, 0);
}

// spec: specs/ont/gen.md#R13
#[test]
fn the_binary_answers_with_its_standard_input_left_open() {
    let t = Temp::new("binary");
    let (path, bytes) = fixture("example");
    let want = ont_gen::generate(&bytes).unwrap();
    let refused = t.file("bad.schema.json", b"not json");
    for (file, code) in [(path, 0), (refused, 2)] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ont"))
            .args(["schema", "gen", &file])
            .env("ONT_FN_READ_ONLY", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let writer = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                drop(writer);
                reader.join().unwrap();
                panic!("schema gen waited on standard input: {file}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        drop(writer);
        let out = reader.join().unwrap();
        let mut err = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        assert!(err.is_empty(), "{err}");
        assert_eq!(status.code(), Some(code), "{file}");
        if code == 0 {
            assert_eq!(String::from_utf8(out).unwrap(), want);
        } else {
            let body: Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(body["code"], "schema_gen_refused");
        }
    }
}
