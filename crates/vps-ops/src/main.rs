use std::path::PathBuf;
fn main() {
    let mut args = std::env::args();
    let program = args.next().unwrap_or_default();
    let mut arguments: Vec<String> = args.collect();
    let name = std::path::Path::new(&program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let alias = match name {
        "hermes-hostctl" => Some("hostctl"),
        "hermes-canary-control" => Some("canary"),
        "hermes-agent-auto-update" | "hermes-agent-update-latest" => Some("release-update"),
        "hermes-main-report" => Some("main-report"),
        _ => None,
    };
    if let Some(a) = alias {
        arguments.insert(0, a.into());
    }
    let root = std::env::var_os("VPS_OPS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let root = root.canonicalize().unwrap_or(root);
    let mut runner = vps_ops::runner::ProcessRunner;
    match vps_ops::cli::dispatch(&mut runner, &arguments, &root) {
        Ok(o) => {
            print!("{}", o.stdout);
            eprint!("{}", o.stderr);
            std::process::exit(o.code)
        }
        Err(e) => {
            eprintln!("vps_ops_failed@crates/vps-ops/src/main.rs:2: {e}");
            std::process::exit(1)
        }
    }
}
