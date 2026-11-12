use ap_mem_adapter::{cli, protocol::serve};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn parse(args: &[&str]) -> Result<cli::Command, &'static str> {
    cli::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

fn run(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap-mem-adapter"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
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
    let output = run(&["mcp"], &input);
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
    for flag in ["--undo", "--cancel", "--force", "undo", "cancel"] {
        assert!(parse(&["publish", "--task", "x", flag]).is_err());
    }
}

#[test]
fn commit_arg_rejected() {
    for args in [
        vec!["publish", "abc123", "--task", "x"],
        vec!["publish", "--task", "x", "--commit", "abc123"],
    ] {
        assert!(parse(&args).is_err());
    }
}

#[test]
fn record_pair_arg_rejected() {
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
    // A valid command works in a directory without a task STATE or grant.
    let output = run(&["publish", "--task", "x"], "");
    assert_eq!(output.stderr, b"publication_unavailable\n");
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
