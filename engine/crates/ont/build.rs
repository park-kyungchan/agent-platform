use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// The top level of the repository around `manifest_dir`, as Git names it; None without Git.
fn top_level(manifest_dir: &Path) -> Option<PathBuf> {
    run_git(manifest_dir, &["rev-parse", "--show-toplevel"])
        .map(|text| PathBuf::from(text.trim()))
        .filter(|path| !path.as_os_str().is_empty())
}

fn emit_rerun(manifest_dir: &Path) {
    // The engine's own sources, not `engine/` whole, which holds `engine/target`.
    let engine = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir);
    for part in ["crates", "Cargo.toml", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={}", engine.join(part).display());
    }
    println!("cargo:rerun-if-env-changed=ONT_TREE_ID");
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

fn hex40(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// The id `ont --version` names: the Git tree id of `engine/` at the commit the engine is built from
/// (`specs/ap/deliver.md` R66). `ONT_TREE_ID` when it holds 40 hexadecimal digits; else, from Git at the
/// repository's top level, `HEAD:engine` when `git status --porcelain -- engine` prints nothing and the
/// manifest is that tree's `engine/crates/ont`; else `unknown` (no Git, or a modified `engine/`: a dirty
/// build names no tree).
fn build_revision(manifest_dir: &Path) -> String {
    if let Ok(id) = env::var("ONT_TREE_ID") {
        let id = id.to_ascii_lowercase();
        if hex40(&id) {
            return id;
        }
    }
    let unknown = || "unknown".to_string();
    let Some(top) = top_level(manifest_dir) else {
        return unknown();
    };
    // Every Git command runs at the top level, never from `engine/` or a crate directory.
    let ours = fs::canonicalize(top.join("engine/crates/ont")).ok();
    if ours.is_none() || ours != fs::canonicalize(manifest_dir).ok() {
        return unknown();
    }
    let Some(status) = run_git(&top, &["status", "--porcelain", "--", "engine"]) else {
        return unknown();
    };
    if !status.trim().is_empty() {
        return unknown();
    }
    match run_git(&top, &["rev-parse", "HEAD:engine"]) {
        Some(id) if hex40(id.trim()) => id.trim().to_owned(),
        _ => unknown(),
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    if !manifest_dir.as_os_str().is_empty() {
        emit_rerun(&manifest_dir);
        println!(
            "cargo:rustc-env=ONT_BUILD_REVISION={}",
            build_revision(&manifest_dir)
        );
    } else {
        println!("cargo:rustc-env=ONT_BUILD_REVISION=unknown");
    }
}
