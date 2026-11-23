//! The items of `ap::host` that the verbs share: the real `Host`, the JSON command line and the
//! session id that `ap run` takes from the kernel.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Host, System};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

#[test]
fn host_call_boundaries() {
    assert!(
        Host.call("/missing-ap-fixture", &[], &Value::Null, 100)
            .is_err()
    );
    let out = Host
        .call(
            "sh",
            &[
                "-c".into(),
                "printf '{\"code\":\"fixture\"}' >&2; exit 1".into(),
            ],
            &Value::Null,
            1000,
        )
        .unwrap();
    assert!(!out.success);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.bytes).unwrap()["code"],
        "fixture"
    );
    assert!(Host.call("sleep", &["2".into()], &Value::Null, 10).is_err());
}

#[test]
fn host_call_answers_with_stderr_when_stdout_is_empty() {
    let out = Host
        .call(
            "sh",
            &["-c".into(), "printf diagnostic >&2".into()],
            &Value::Null,
            1000,
        )
        .unwrap();
    assert!(out.success);
    assert!(out.stdout.is_empty());
    assert_eq!(out.bytes, b"diagnostic");
}

/// The command line of `ap review` is the JSON command line of the host: one argument, one JSON
/// object of at most 1 MiB on stdin, a refusal on stderr with nothing on stdout.
#[test]
fn json_cli_refuses_a_bad_stdin() {
    for input in [b"{} {}".to_vec(), b"[]".to_vec(), vec![b' '; 1_048_577]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(["review", "start"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&input).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        let v: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert!(v["error"]["code"].is_string());
    }
}

/// `ap run` reads its session id from the kernel; a file that is not a UUID is a failed launch.
struct Kernel {
    uuid: Vec<u8>,
}
impl System for Kernel {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        Host.call(program, args, input, ms)
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if path == "/proc/sys/kernel/random/uuid" {
            return Ok(self.uuid.clone());
        }
        Host.read(path)
    }
    fn units(&self) -> bool {
        true
    }
}

#[test]
fn a_session_id_that_is_not_a_uuid_fails_the_launch() {
    for (n, uuid) in [
        &b""[..],
        b"not-a-uuid\n",
        b"11111111-2222-4333-8444-55555555555G\n",
    ]
    .into_iter()
    .enumerate()
    {
        let t: PathBuf =
            std::env::temp_dir().join(format!("ap-host-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["account", "cwd"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        std::fs::write(t.join("prompt.md"), "Review.\n").unwrap();
        let p = |rel: &str| t.join(rel).to_str().unwrap().to_owned();
        let request = json!({"role":"reviewer","account":p("account"),"model":"sonnet","effort":"high",
            "permission_mode":"auto","cwd":p("cwd"),"prompt":p("prompt.md"),"out":p("out")});
        let (record, status) = ap::run::execute(
            &mut Kernel {
                uuid: uuid.to_vec(),
            },
            &request,
        )
        .unwrap();
        assert_eq!(
            (record["state"].as_str(), record["unmet"].as_str(), status),
            (Some("failed"), Some("launch"), 3),
            "{record}"
        );
        let _ = std::fs::remove_dir_all(&t);
    }
}

/// A fake program written to a temporary directory: the file `name` runs `body` under `sh`.
fn fake(name: &str, body: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("ap-host-limit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, path)
}

/// Shell text that prints one JSON string of `n` bytes in all.
fn json_of(n: usize) -> String {
    format!(
        "printf '\"'; head -c {} /dev/zero | tr '\\0' a; printf '\"'",
        n - 2
    )
}

#[test]
fn answer_over_one_mib_is_read() {
    let n = 2 * 1_048_576;
    let (dir, path) = fake("ont", &json_of(n));
    let out = Host
        .call(path.to_str().unwrap(), &[], &Value::Null, 20_000)
        .unwrap();
    assert!(out.success);
    assert_eq!(out.stdout.len(), n);
    assert!(serde_json::from_slice::<Value>(&out.stdout).is_ok());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn answer_over_eight_mib_is_process_failed() {
    let (dir, path) = fake("ont", &json_of(9 * 1_048_576));
    let out = Host.call(path.to_str().unwrap(), &[], &Value::Null, 20_000);
    assert_eq!(out.err().unwrap().code, "process_failed");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn error_over_one_mib_is_process_failed() {
    let (dir, path) = fake("ont", "head -c 2097152 /dev/zero | tr '\\0' a >&2; exit 1");
    let out = Host.call(path.to_str().unwrap(), &[], &Value::Null, 20_000);
    assert_eq!(out.err().unwrap().code, "process_failed");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn other_program_keeps_one_mib() {
    let (dir, path) = fake("other", &json_of(2 * 1_048_576));
    let out = Host.call(path.to_str().unwrap(), &[], &Value::Null, 20_000);
    assert_eq!(out.err().unwrap().code, "process_failed");
    let _ = std::fs::remove_dir_all(dir);
}
