use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The text the revision of the engine must print (`ont --version`) is this, then the pin.
const VERSION_PREFIX: &str = "ont 0.1.0 ";
/// Where the engine of a pin is installed besides `PATH` (`ci/engine-pin.sh`, `check`).
const INSTALLED: &str = "/usr/local/bin";
/// The most bytes of an engine's output a failure line shows.
const SHOWN: usize = 600;

fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn git_dir_for(manifest_dir: &Path) -> Option<PathBuf> {
    let mut current = Some(manifest_dir);
    while let Some(dir) = current {
        let candidate = dir.join(".git");
        if candidate.is_file() {
            let text = fs::read_to_string(&candidate).ok()?;
            for line in text.lines() {
                if let Some(rest) = line.trim().strip_prefix("gitdir:") {
                    let rest = rest.trim();
                    if rest.is_empty() {
                        return None;
                    }
                    let path = PathBuf::from(rest);
                    return Some(if path.is_absolute() {
                        path
                    } else {
                        dir.join(path)
                    });
                }
            }
            return None;
        }
        if candidate.is_dir() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn emit_rerun(manifest_dir: &Path) {
    let workspace_root = run_git(manifest_dir, &["rev-parse", "--show-toplevel"])
        .map(|text| PathBuf::from(text.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            manifest_dir
                .parent()
                .and_then(|parent| parent.parent())
                .map(|path| path.to_path_buf())
                .unwrap_or_else(|| manifest_dir.to_path_buf())
        });
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("Cargo.lock").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates").display()
    );
    let Some(git_dir) = git_dir_for(manifest_dir) else {
        return;
    };
    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());
    let Ok(head) = fs::read_to_string(&head_path) else {
        return;
    };
    let Some(reference) = head.trim().strip_prefix("ref:") else {
        return;
    };
    let reference = reference.trim();
    if reference.is_empty() {
        return;
    }
    let ref_path = match fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => {
            let common = common.trim();
            if common.is_empty() {
                git_dir.join(reference)
            } else {
                let base = PathBuf::from(common);
                let base = if base.is_absolute() {
                    base
                } else {
                    git_dir.join(base)
                };
                let candidate = base.join(reference);
                if candidate.exists() {
                    candidate
                } else {
                    git_dir.join(reference)
                }
            }
        }
        Err(_) => git_dir.join(reference),
    };
    println!("cargo:rerun-if-changed={}", ref_path.display());
}

fn build_revision(manifest_dir: &Path) -> String {
    let repo_root = run_git(manifest_dir, &["rev-parse", "--show-toplevel"])
        .map(|text| PathBuf::from(text.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| manifest_dir.to_path_buf());
    let Some(commit) = run_git(&repo_root, &["rev-parse", "HEAD"]) else {
        return "unknown".to_string();
    };
    let commit = commit.trim().to_ascii_lowercase();
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return "unknown".to_string();
    }
    let dirty =
        run_git(&repo_root, &["status", "--porcelain"]).is_some_and(|text| !text.trim().is_empty());
    if dirty {
        format!("{commit}-dirty")
    } else {
        commit
    }
}

/// One refusal of this script: `<code>@crates/ap/build.rs: <detail>` on standard error and a non-zero exit,
/// before `work.rs` is written.
fn refuse(code: &str, detail: &str) -> ! {
    eprintln!("{code}@crates/ap/build.rs: {detail}");
    std::process::exit(1)
}

/// The workspace root of this tree: two directories above the manifest of `ap`.
fn root_of(manifest_dir: &Path) -> PathBuf {
    let up = manifest_dir.parent().and_then(Path::parent);
    up.unwrap_or(manifest_dir).to_path_buf()
}

/// The pin of `ontology/ENGINE-PIN` of this tree: 40 lowercase hexadecimal digits without whitespace.
fn pin_of(file: &Path) -> String {
    let Ok(text) = fs::read_to_string(file) else {
        refuse("engine_pin_invalid", "ontology/ENGINE-PIN cannot be read");
    };
    let pin: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let shape = pin.len() == 40 && pin.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
    if !shape {
        refuse(
            "engine_pin_invalid",
            "ontology/ENGINE-PIN is not 40 lowercase hexadecimal digits",
        );
    }
    pin
}

fn executable(path: &Path) -> bool {
    let meta = fs::metadata(path);
    meta.is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The files `name` names in the directories of `PATH`, in order.
fn on_path(name: &str) -> Vec<PathBuf> {
    let paths = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&paths)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .filter(|path| executable(path))
        .take(1)
        .collect()
}

/// What `--version` of the engine at `path` printed on standard output without its final line feeds, when
/// it exited 0; standard input is empty.
fn version_of(path: &Path) -> Option<String> {
    let output = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim_end_matches('\n').to_owned())
}

/// Which rule of two trees names the engine (`specs/ap/deliver.md` R66): the pin of
/// `ontology/ENGINE-PIN`, whose engine prints exactly `ont 0.1.0 <pin>`; or the Git tree id of `engine/`
/// at `HEAD`, whose engine prints `ont <version> <id>`.
enum Rule {
    Pin(String),
    Tree(String),
}

impl Rule {
    fn id(&self) -> &str {
        match self {
            Rule::Pin(id) | Rule::Tree(id) => id,
        }
    }
    /// Whether a printed version line names this rule's engine.
    fn names(&self, printed: &str) -> bool {
        match self {
            Rule::Pin(pin) => printed == format!("{VERSION_PREFIX}{pin}"),
            Rule::Tree(id) => {
                let words: Vec<&str> = printed.split(' ').collect();
                words.len() == 3 && words[0] == "ont" && !words[1].is_empty() && words[2] == id
            }
        }
    }
    /// What the refusal `engine_differs` says the engine should name.
    fn wanted(&self) -> String {
        match self {
            Rule::Pin(pin) => format!("the pin is {pin}"),
            Rule::Tree(id) => format!("the tree id of engine/ at HEAD is {id}"),
        }
    }
}

/// The engine of `rule`: the path `ONT_BIN` names when it is set and not empty, and then no other; else
/// `ont-<id>` of `PATH`, `/usr/local/bin/ont-<id>`, `ont` of `PATH` (and, under the tree rule,
/// `/usr/local/bin/ont`). Only an executable file whose `--version` names the rule's engine is taken.
fn engine_of(rule: &Rule) -> PathBuf {
    let named = env::var_os("ONT_BIN").filter(|path| !path.is_empty());
    let pinned = format!("ont-{}", rule.id());
    let (looked, listed): (String, Vec<PathBuf>) = match named {
        Some(path) => (
            format!("ONT_BIN {}", PathBuf::from(&path).display()),
            vec![PathBuf::from(path)],
        ),
        None => {
            let mut all = on_path(&pinned);
            all.push(Path::new(INSTALLED).join(&pinned));
            all.extend(on_path("ont"));
            let mut looked = format!("{pinned} and ont on PATH, {INSTALLED}/{pinned}");
            if let Rule::Tree(_) = rule {
                all.push(Path::new(INSTALLED).join("ont"));
                looked.push_str(&format!(", {INSTALLED}/ont"));
            }
            (looked, all)
        }
    };
    let mut candidates: Vec<PathBuf> = vec![];
    for path in listed.into_iter().filter(|path| executable(path)) {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    if candidates.is_empty() {
        refuse("engine_missing", &format!("no engine at {looked}"));
    }
    let mut seen = vec![];
    for path in &candidates {
        let printed = version_of(path);
        if printed.as_deref().is_some_and(|text| rule.names(text)) {
            return path.clone();
        }
        let revision = printed.map(|text| match text.strip_prefix(VERSION_PREFIX) {
            Some(revision) => revision.to_owned(),
            None => text,
        });
        let revision = revision.unwrap_or_else(|| "none".to_owned());
        seen.push(format!("{} prints {revision}", path.display()));
    }
    refuse(
        "engine_differs",
        &format!("{}; {}", seen.join("; "), rule.wanted()),
    )
}

/// The Git tree id of `engine/` at `HEAD` of the repository whose top level is `root`, read at that top
/// level (`git ls-tree HEAD -- engine`, the id `git rev-parse HEAD:engine` prints); the engine `./check` or delivery provides must name it.
fn tree_id(root: &Path) -> String {
    let top = run_git(root, &["rev-parse", "--show-toplevel"])
        .and_then(|text| fs::canonicalize(text.trim()).ok());
    if top.is_none() || top != fs::canonicalize(root).ok() {
        refuse(
            "engine_differs",
            "the tree id of engine/ cannot be read: the workspace is not the top level of a Git repository",
        );
    }
    // `git ls-tree HEAD -- engine` names the entry with its type: only a tree counts.
    let entry = run_git(root, &["ls-tree", "HEAD", "--", "engine"]).unwrap_or_default();
    let id = entry
        .strip_prefix("040000 tree ")
        .and_then(|rest| rest.trim_end().strip_suffix("\tengine"))
        .map(str::to_owned);
    match id {
        Some(id)
            if id.len() == 40 && id.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) =>
        {
            id
        }
        _ => refuse(
            "engine_differs",
            "the tree id of engine/ at HEAD cannot be read (git rev-parse HEAD:engine)",
        ),
    }
}

/// The first `"code":"<text>"` of the refusal line of the engine.
fn refusal_code(line: &str) -> &str {
    let rest = line.split_once("\"code\":\"").map(|(_, rest)| rest);
    rest.and_then(|rest| rest.split('"').next())
        .unwrap_or("unknown")
}

fn shown(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    match text.char_indices().nth(SHOWN) {
        Some((end, _)) => format!("{}...", &text[..end]),
        None => text.to_owned(),
    }
}

/// The module `ont schema gen` makes from the schema of this tree, unchanged, as `$OUT_DIR/work.rs`.
fn generate(engine: &Path, schema: &Path) {
    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        refuse("schema_gen_failed", "OUT_DIR is not set");
    };
    let output = Command::new(engine)
        .args(["schema", "gen"])
        .arg(schema)
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        refuse("schema_gen_failed", "the engine cannot be run");
    };
    match output.status.code() {
        Some(0) => {
            if fs::write(out_dir.join("work.rs"), &output.stdout).is_err() {
                refuse("schema_gen_failed", "work.rs cannot be written");
            }
        }
        Some(2) => {
            let line = shown(&output.stdout);
            let code = refusal_code(&line);
            refuse("schema_gen_failed", &format!("{code}: {line}"));
        }
        other => {
            let status = other.map_or("a signal".to_owned(), |c| format!("exit {c}"));
            let mut text = shown(&output.stdout);
            text.push_str(&shown(&output.stderr));
            refuse("schema_gen_failed", &format!("{status}: {text}"));
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    if !manifest_dir.as_os_str().is_empty() {
        emit_rerun(&manifest_dir);
        println!(
            "cargo:rustc-env=AP_BUILD_REVISION={}",
            build_revision(&manifest_dir)
        );
    } else {
        println!("cargo:rustc-env=AP_BUILD_REVISION=unknown");
    }
    let root = root_of(&manifest_dir);
    let (schema, pin_file) = (
        root.join("ontology/work.schema.json"),
        root.join("ontology/ENGINE-PIN"),
    );
    println!("cargo:rerun-if-changed={}", schema.display());
    println!("cargo:rerun-if-env-changed=ONT_BIN");
    println!("cargo:rerun-if-env-changed=PATH");
    // The rule of two trees (`specs/ap/deliver.md` R66): a tree that holds `ontology/ENGINE-PIN` keeps
    // the pin's rule; one without it that holds `engine/` names its engine by the tree id of `engine/`.
    let rule = if fs::symlink_metadata(&pin_file).is_ok() {
        println!("cargo:rerun-if-changed={}", pin_file.display());
        Rule::Pin(pin_of(&pin_file))
    } else if fs::symlink_metadata(root.join("engine")).is_ok_and(|m| m.is_dir()) {
        // `HEAD` and its ref are watched by `emit_rerun`: the tree id moves only with them.
        Rule::Tree(tree_id(&root))
    } else {
        refuse(
            "engine_missing",
            "the tree holds neither ontology/ENGINE-PIN nor engine/",
        )
    };
    let engine = engine_of(&rule);
    generate(&engine, &schema);
}
