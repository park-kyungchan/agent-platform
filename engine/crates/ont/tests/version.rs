#![allow(clippy::unwrap_used)]
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ont"))
        .args(args)
        .output()
        .unwrap()
}

fn hex40(text: &str) -> bool {
    text.len() == 40
        && text
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn assert_version_line(output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(text.matches('\n').count(), 1, "stdout: {text:?}");
    let line = text.strip_suffix('\n').unwrap();
    assert!(!line.contains('\r'), "stdout: {text:?}");
    let parts: Vec<&str> = line.split(' ').collect();
    assert_eq!(parts.len(), 3, "line: {line:?}");
    assert_eq!(parts[0], "ont", "line: {line:?}");
    assert!(!parts[1].is_empty(), "line: {line:?}");
    // A tree id of `engine/` or `unknown`; never a commit with a dirty mark.
    let revision = parts[2];
    assert!(revision == "unknown" || hex40(revision), "line: {line:?}");
}

fn git_at(dir: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The id the rule of the build names (`engine/crates/ont/build.rs`): `ONT_TREE_ID` of the build when
/// it holds 40 hexadecimal digits; else, at the repository's top level, `HEAD:engine` when `engine/` is
/// unmodified; else `unknown`.
fn expected_revision() -> String {
    if let Some(id) = option_env!("ONT_TREE_ID").map(str::to_ascii_lowercase)
        && hex40(&id)
    {
        return id;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(top) = git_at(manifest, &["rev-parse", "--show-toplevel"]) else {
        return "unknown".into();
    };
    let top = std::path::PathBuf::from(top);
    let ours = std::fs::canonicalize(top.join("engine/crates/ont")).ok();
    if ours.is_none() || ours != std::fs::canonicalize(manifest).ok() {
        return "unknown".into();
    }
    match git_at(&top, &["status", "--porcelain", "--", "engine"]) {
        Some(status) if status.is_empty() => (),
        _ => return "unknown".into(),
    }
    match git_at(&top, &["rev-parse", "HEAD:engine"]) {
        Some(id) if hex40(&id) => id,
        _ => "unknown".into(),
    }
}

#[test]
fn version_flag_prints_build_revision() {
    assert_version_line(&run(&["--version"]));
}

#[test]
fn version_names_the_tree_id_of_engine_or_unknown() {
    let output = run(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    let line = text.strip_suffix('\n').unwrap();
    let revision = line.split(' ').nth(2).unwrap();
    assert_eq!(revision, expected_revision(), "line: {line:?}");
}

#[test]
fn spawned_describe_refuses_a_non_root_principal() {
    // /proc/self/status: "Uid: real, effective, ...".
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let fields = text
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .map(|l| {
            l[4..]
                .split_whitespace()
                .filter_map(|f| f.parse::<u32>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (real, effective) = (
        fields.first().copied().unwrap_or(0),
        fields.get(1).copied().unwrap_or(0),
    );
    if real == 0 && effective == 0 {
        return; // The refusal is covered at library level in cli.rs.
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ont"))
        .args(["--store", "/nonexistent/store.sqlite", "describe"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["code"], "principal_not_trusted");
}

#[test]
fn version_schema_check_and_backup_do_not_wait_on_open_stdin() {
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("ont-stdin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = dir.join("s.sqlite").display().to_string();
    let schema = dir.join("example.json");
    std::fs::write(
        &schema,
        include_bytes!("../../ont-core/tests/schema-fixtures/example.json"),
    )
    .unwrap();
    let schema = schema.display().to_string();
    // Create a store through the library so that `backup` has one.
    let args: Vec<String> = ["--store", &store, "schema", "install", &schema]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let _ = ont::run(&args, &b""[..], 0, 0);
    for argv in [
        vec!["--version"],
        vec!["--store", &store, "schema", "check", &schema],
        vec!["--store", &store, "backup"],
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ont"))
            .args(&argv)
            .stdin(Stdio::piped()) // left open: the child must not read it
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if start.elapsed() > std::time::Duration::from_secs(20) {
                let _ = child.kill();
                panic!("{argv:?} waited on stdin");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(status.code().is_some(), "{argv:?}");
        drop(child.stdin.take());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
