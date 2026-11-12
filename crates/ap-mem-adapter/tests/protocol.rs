use ap_mem_adapter::{cli, protocol::serve};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        use std::os::unix::fs::DirBuilderExt;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::var_os("AP_S03_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        loop {
            let path = parent.join(format!(
                "ap-mem-grammar-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("temporary directory: {e}"),
            }
        }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
use std::process::{Command, Stdio};

fn parse(args: &[&str]) -> Result<cli::Command, &'static str> {
    cli::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

fn run(args: &[&str], input: &str) -> std::process::Output {
    run_in(
        args,
        input,
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")),
    )
}

fn run_in(args: &[&str], input: &str, cwd: &Path) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap-mem-adapter"))
        .args(args)
        .current_dir(cwd)
        .envs([
            ("AP_MEM_WRITE_GRANT", "allow"),
            ("AP_MEM_GRANT", "allow"),
            ("AP_MEM_AUTHORIZED", "true"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn unknown_publish_rpc_no_effect() {
    let mut input = String::new();
    for (id, method, params) in [
        (1, "publish", json!({"task":"x"})),
        (
            2,
            "tools/call",
            json!({"name":"mem_publish","arguments":{"task":"x"}}),
        ),
        (3, "tools/list", json!({})),
        (
            4,
            "tools/call",
            json!({"name":"mem_tree_read","arguments":{}}),
        ),
        (
            5,
            "tools/call",
            json!({"name":"mem_tree_read","arguments":{},"_meta":{"progressToken":"p1"}}),
        ),
        (
            6,
            "tools/call",
            json!({"name":"mem_tree_read","arguments":{},"_meta":"invalid"}),
        ),
        (
            7,
            "tools/call",
            json!({"name":"mem_tree_read","arguments":{"_meta":{}}}),
        ),
    ] {
        input.push_str(
            &(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string() + "\n"),
        );
    }
    input.push_str("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
    let temporary = TempDir::new();
    let output = run_in(&["mcp"], &input, &temporary.0);
    assert_eq!(std::fs::read_dir(&temporary.0).unwrap().count(), 0);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(replies.len(), 7);
    assert_eq!(replies[0]["error"]["code"], -32601);
    assert_eq!(replies[1]["error"]["message"], "unknown_tool");
    assert_eq!(replies[2]["result"]["tools"].as_array().unwrap().len(), 5);
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(
        replies[3]["result"]["content"][0]["text"],
        "current_memory_unavailable"
    );
    assert_eq!(replies[4]["result"], replies[3]["result"]);
    assert_eq!(replies[5]["error"]["message"], "invalid_params");
    assert_eq!(replies[6]["error"]["message"], "invalid_arguments");
    let mut result = Vec::new();
    serve(
        format!("{}\n{{}}\n", "x".repeat(65537)).as_bytes(),
        &mut result,
    )
    .unwrap();
    let result = String::from_utf8(result).unwrap();
    assert!(result.contains("request_too_large"));
    assert!(result.contains("invalid_request"));
}

#[test]
fn publish_cli_exact_args() {
    assert_eq!(parse(&["mcp"]), Ok(cli::Command::Mcp));
    for task in ["a", "P10.p1b:_-09", &"a".repeat(64)] {
        assert_eq!(
            parse(&["publish", "--task", task]),
            Ok(cli::Command::Publish { task: task.into() })
        );
    }
    for args in [
        vec![],
        vec!["publish"],
        vec!["publish", "--task"],
        vec!["mcp", "extra"],
        vec!["publish", "--task", "x", "--task", "y"],
    ] {
        assert!(parse(&args).is_err());
    }
    for task in ["", "a b", "한글", "x/y", "a\n", &"a".repeat(65)] {
        assert!(parse(&["publish", "--task", task]).is_err());
    }
    let output = run(&["publish", "--task", "P10"], "");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"publication_unavailable\n");
}

#[test]
fn undo_flag_rejected() {
    for subcommand in ["undo", "cancel", "force"] {
        assert!(parse(&[subcommand]).is_err());
        assert!(parse(&[subcommand, "--task", "x"]).is_err());
    }
    assert!(parse(&["publish", "--force", "--task", "x"]).is_err());
    assert!(parse(&["publish", "--task=x"]).is_err());
    for flag in ["--undo", "--cancel", "--force", "undo", "cancel"] {
        assert!(parse(&["publish", "--task", "x", flag]).is_err());
    }
}

#[test]
fn commit_arg_rejected() {
    for args in [
        vec!["publish", "abc123", "--task", "x"],
        vec!["publish", "--task", "x", "--commit", "abc123"],
        vec![
            "publish",
            "--task",
            "x",
            "0123456789abcdef0123456789abcdef01234567",
        ],
    ] {
        assert!(parse(&args).is_err());
    }
}

#[test]
fn record_pair_arg_rejected() {
    assert!(parse(&["publish", "--task", "x", "a", "b"]).is_err());
    for flag in [
        "--replace",
        "--predecessor",
        "--successor",
        "--record",
        "--body",
        "--bank",
        "--url",
        "--auth",
    ] {
        assert!(parse(&["publish", "--task", "x", flag, "value"]).is_err());
    }
}

#[test]
fn no_state_parser() {
    let temporary = TempDir::new();
    let absent = run_in(&["publish", "--task", "x"], "", &temporary.0);
    std::fs::write(
        temporary.0.join("STATE.md"),
        "Authority: memory writes forbidden. Grant: denied.\n",
    )
    .unwrap();
    let denied = run_in(&["publish", "--task", "x"], "", &temporary.0);
    assert_eq!(absent.status.code(), denied.status.code());
    assert_eq!(absent.stdout, denied.stdout);
    assert_eq!(absent.stderr, denied.stderr);
    assert_eq!(denied.stderr, b"publication_unavailable\n");
    for flag in [
        "--state",
        "--state-path",
        "--grant",
        "--permit",
        "--authorized",
    ] {
        assert!(parse(&["publish", "--task", "x", flag, "value"]).is_err());
    }
}
