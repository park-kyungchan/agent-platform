use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(args)
        .output()
        .unwrap()
}

fn assert_version_line(output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(text.matches('\n').count(), 1, "stdout: {text:?}");
    let line = text.strip_suffix('\n').unwrap();
    assert!(!line.contains('\r'), "stdout: {text:?}");
    let parts: Vec<&str> = line.split(' ').collect();
    assert_eq!(parts.len(), 3, "line: {line:?}");
    assert_eq!(parts[0], "ap", "line: {line:?}");
    assert!(!parts[1].is_empty(), "line: {line:?}");
    let revision = parts[2];
    if revision != "unknown" {
        let hex = revision.strip_suffix("-dirty").unwrap_or(revision);
        assert_eq!(hex.len(), 40, "line: {line:?}");
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "line: {line:?}"
        );
    }
}

#[test]
fn version_flag_prints_build_revision() {
    assert_version_line(&run(&["--version"]));
}

#[test]
fn version_word_prints_build_revision() {
    assert_version_line(&run(&["version"]));
}

#[test]
fn version_revision_matches_git_head_when_available() {
    let git = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    if !git.status.success() {
        return;
    }
    let Ok(head) = String::from_utf8(git.stdout) else {
        return;
    };
    let head = head.trim().to_ascii_lowercase();
    if head.len() != 40 || !head.chars().all(|c| c.is_ascii_hexdigit()) {
        return;
    }
    let output = run(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    let line = text.strip_suffix('\n').unwrap();
    let revision = line.split(' ').nth(2).unwrap();
    let revision = revision.strip_suffix("-dirty").unwrap_or(revision);
    assert_eq!(revision, head, "line: {line:?}");
}

#[test]
fn no_arguments_keeps_usage_error() {
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
    assert_eq!(
        output.stderr,
        b"{\"error\":{\"code\":\"usage\",\"field\":\"args\"}}\n"
    );
}
