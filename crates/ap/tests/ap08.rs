//! Fixed AP-08 obligations authored before the adapter implementation.
//! Every filesystem effect is beneath explicit AP_S03_TEST_ROOT.
#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn invoke(args: &[&str], input: &Value) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(input).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn success(args: &[&str], input: &Value) -> Value {
    let out = invoke(args, input);
    assert_eq!(out.status.code(), Some(0), "{args:?}: {:?}", out.stderr);
    assert!(out.stderr.is_empty());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut compact = serde_json::to_vec(&value).unwrap();
    compact.push(b'\n');
    // JSON object order is tested by frozen S02/S03 judges; retain exact framing here.
    assert_eq!(out.stdout.last(), Some(&b'\n'));
    assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 1);
    assert_eq!(out.stdout.len(), compact.len());
    value
}
fn refusal(args: &[&str], input: &Value, status: i32, code: &str, field: &str) {
    let out = invoke(args, input);
    assert_eq!(out.status.code(), Some(status));
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        format!("{{\"error\":{{\"code\":\"{code}\",\"field\":\"{field}\"}}}}\n").as_bytes()
    );
}
fn render_input() -> Value {
    json!({"schema_version":1,"canonical_path":"/fake/source/AGENTS.md",
        "codex":{"config_dir":"/fake/codex","instructions":{"kind":"missing"},"instruction_override":{"kind":"missing"}},
        "claude":{"config_dir":"/fake/claude","instructions":{"kind":"missing"}},
        "agy":{"config_dir":"/fake/agy","instructions":{"kind":"missing"},"gemini_alias":{"kind":"missing"}},
        "preserved":[{"path":"/fake/agy/config/hooks.json","text":"opaque ${SECRET}\n"}]})
}

#[test]
fn render_literal_four_artifacts_and_projection() {
    let mut input = render_input();
    let expected = json!({"schema_version":1,"artifacts":[
        {"path":"/fake/agy/AGENTS.md","selector":"agy_instruction_link","before":{"kind":"missing"},"after":{"kind":"symlink","target":"/fake/source/AGENTS.md"},"changed":true},
        {"path":"/fake/agy/GEMINI.md","selector":"agy_gemini_alias","before":{"kind":"missing"},"after":{"kind":"symlink","target":"/fake/source/AGENTS.md"},"changed":true},
        {"path":"/fake/claude/CLAUDE.md","selector":"claude_canonical_import","before":{"kind":"missing"},"after":{"kind":"file","text":"@/fake/source/AGENTS.md\n"},"changed":true},
        {"path":"/fake/codex/AGENTS.md","selector":"codex_instruction_link","before":{"kind":"missing"},"after":{"kind":"symlink","target":"/fake/source/AGENTS.md"},"changed":true}],
        "preserved":[{"path":"/fake/agy/config/hooks.json","text":"opaque ${SECRET}\n"}]});
    assert_eq!(success(&["render"], &input), expected);
    input["agy"]["instructions"] = json!({"kind":"symlink","target":"../source/AGENTS.md"});
    input["agy"]["gemini_alias"] = json!({"kind":"symlink","target":"/fake/source/AGENTS.md"});
    input["codex"]["instructions"] = json!({"kind":"symlink","target":"/fake/source/AGENTS.md"});
    input["claude"]["instructions"] = json!({"kind":"file","text":"@/fake/source/AGENTS.md\n"});
    let projected = success(&["render"], &input);
    for artifact in projected["artifacts"].as_array().unwrap() {
        assert_eq!(artifact["changed"], false);
        assert_eq!(artifact["before"], artifact["after"]);
    }
    assert_eq!(
        projected["artifacts"][0]["after"]["target"],
        "../source/AGENTS.md"
    );
}

#[test]
fn render_agy_refusals_and_alias_collision() {
    for (key, field) in [
        ("instructions", "agy.instructions"),
        ("gemini_alias", "agy.gemini_alias"),
    ] {
        for (entry, code, suffix) in [
            (
                json!({"kind":"file","text":"foreign"}),
                "instruction_conflict",
                "",
            ),
            (
                json!({"kind":"symlink","target":"/foreign/AGENTS.md"}),
                "instruction_conflict",
                "",
            ),
            (
                json!({"kind":"symlink","target":"AGENTS.md"}),
                "instruction_conflict",
                "",
            ),
            (
                json!({"kind":"symlink","target":"bad target"}),
                "invalid_path",
                ".target",
            ),
            (
                json!({"kind":"unsupported","reason":"symlink_chain"}),
                "unsupported_target",
                "",
            ),
        ] {
            let mut input = render_input();
            input["agy"][key] = entry;
            refusal(&["render"], &input, 3, code, &format!("{field}{suffix}"));
        }
    }
    let mut input = render_input();
    input["preserved"] = json!([{"path":"/fake/agy/GEMINI.md","text":"foreign"}]);
    refusal(&["render"], &input, 3, "path_collision", "preserved.path");
    input = render_input();
    input["canonical_path"] = json!("/fake/agy/GEMINI.md");
    refusal(&["render"], &input, 3, "path_collision", "profiles");
    input = render_input();
    input["agy"]["config_dir"] = json!("/fake/codex");
    refusal(&["render"], &input, 3, "path_collision", "profiles");
}

#[test]
fn new_profile_has_strict_admission() {
    for input in [
        {
            let mut x = render_input();
            x["agy"]["settings"] = json!({});
            x
        },
        {
            let mut x = render_input();
            x["agy"].as_object_mut().unwrap().remove("gemini_alias");
            x
        },
        {
            let mut x = render_input();
            x["agy"] = Value::Null;
            x
        },
    ] {
        refusal(&["render"], &input, 2, "invalid_input", "stdin");
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    spec: Value,
}
impl Fixture {
    fn new() -> Self {
        let parent = PathBuf::from(
            std::env::var_os("AP_S03_TEST_ROOT").expect("explicit fake root required"),
        );
        assert_eq!(
            fs::symlink_metadata(&parent).unwrap().permissions().mode() & 0o7777,
            0o700
        );
        let root = parent.join(format!(
            "ap08-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        for dir in ["source", "agy", "command"] {
            fs::create_dir(root.join(dir)).unwrap();
            fs::set_permissions(root.join(dir), fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(root.join("source/AGENTS.md"), "# Synthetic canonical\n").unwrap();
        fs::set_permissions(
            root.join("source/AGENTS.md"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        // Unselected directories deliberately do not exist.
        let spec = json!({"schema_version":1,"canonical_path":root.join("source/AGENTS.md"),
            "codex_dir":root.join("absent-codex"),"claude_dir":root.join("absent-claude"),
            "agy_dir":root.join("agy"),"scope":"agy","uid":rustix::process::geteuid().as_raw(),
            "environment":{"kind":"disposable","root":root}});
        Self { root, spec }
    }
    fn plan(&self) -> Value {
        success(&["install", "plan"], &self.spec)
    }
    fn apply(&self, plan: Value, name: &str) -> Value {
        if name != "command" {
            fs::create_dir(self.root.join(name)).unwrap();
            fs::set_permissions(self.root.join(name), fs::Permissions::from_mode(0o700)).unwrap();
        }
        success(
            &["install", "apply"],
            &json!({"plan":plan,"command_dir":self.root.join(name),"writers_quiesced":true}),
        )
    }
    fn recovery(&self) -> Value {
        json!({"command_dir":self.root.join("command"),"writers_quiesced":true})
    }
    fn doctor(&self, status: &str) {
        let d = success(&["doctor", "inspect"], &self.spec);
        assert_eq!(
            d["muse"],
            json!({"disposition":"retained_native_mechanism","canonical_path":self.spec["canonical_path"],"handoff_requires_task_state":true,"native_loading":"unverified"})
        );
        assert_eq!(d["native_loading"], "unverified");
        assert_eq!(d["hooks_executed"], 0);
        assert_eq!(d["p6_complete"], false);
        assert_eq!(d["entries"].as_array().unwrap().len(), 2);
        assert_eq!(d["entries"][0]["selector"], "agy_instruction_link");
        assert_eq!(d["entries"][1]["selector"], "agy_gemini_alias");
        for e in d["entries"].as_array().unwrap() {
            assert_eq!(e["status"], status);
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn fake_home_fresh_twice_doctor_and_recovery_preserve_opaque_files() {
    let f = Fixture::new();
    let hook = b"#!/bin/sh\ntouch \"$0.executed\"\n";
    fs::write(f.root.join("agy/fake-hook"), hook).unwrap();
    fs::set_permissions(
        f.root.join("agy/fake-hook"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let opaque = b"{\"hooks\":\"./fake-hook\",\"unknown\":\"${SECRET}\"}\n";
    fs::write(f.root.join("agy/settings.json"), opaque).unwrap();
    f.doctor("needs_install");
    let p = f.plan();
    assert_eq!(p["entries"].as_array().unwrap().len(), 2);
    for e in p["entries"].as_array().unwrap() {
        assert_eq!(e["changed"], true);
        assert_eq!(e["before"]["entry"], json!({"kind":"missing"}));
        assert_eq!(
            e["after"],
            json!({"kind":"symlink","target":f.spec["canonical_path"]})
        );
    }
    assert!(!f.root.join("agy/.ap-install.lock").exists());
    let report = f.apply(p, "command");
    assert_eq!(report["outcome"], "installed");
    for e in report["entries"].as_array().unwrap() {
        assert_eq!(e["phase"], "applied");
    }
    let mut inodes = vec![];
    for name in ["AGENTS.md", "GEMINI.md"] {
        let path = f.root.join("agy").join(name);
        assert_eq!(
            fs::read_link(&path).unwrap(),
            PathBuf::from(f.spec["canonical_path"].as_str().unwrap())
        );
        inodes.push(fs::symlink_metadata(path).unwrap().ino());
    }
    f.doctor("ready");
    let second = f.apply(f.plan(), "second-command");
    for e in second["entries"].as_array().unwrap() {
        assert_eq!(e["phase"], "unchanged");
    }
    // A no-op command has no rollback authority over another command's links.
    success(
        &["install", "recover"],
        &json!({"command_dir":f.root.join("second-command"),"writers_quiesced":true}),
    );
    for (name, inode) in ["AGENTS.md", "GEMINI.md"].iter().zip(inodes) {
        assert_eq!(
            fs::symlink_metadata(f.root.join("agy").join(name))
                .unwrap()
                .ino(),
            inode
        );
    }
    assert_eq!(
        success(&["install", "recover"], &f.recovery())["outcome"],
        "recovered"
    );
    assert_eq!(
        success(&["install", "recover"], &f.recovery())["outcome"],
        "recovered"
    );
    f.doctor("needs_install");
    for name in ["AGENTS.md", "GEMINI.md"] {
        assert!(fs::symlink_metadata(f.root.join("agy").join(name)).is_err());
    }
    assert_eq!(fs::read(f.root.join("agy/settings.json")).unwrap(), opaque);
    assert_eq!(fs::read(f.root.join("agy/fake-hook")).unwrap(), hook);
    assert!(!f.root.join("agy/fake-hook.executed").exists());
    assert_eq!(
        fs::read(f.root.join("source/AGENTS.md")).unwrap(),
        b"# Synthetic canonical\n"
    );
    assert!(!f.root.join("absent-codex").exists());
    assert!(!f.root.join("absent-claude").exists());
}

#[test]
fn existing_relative_alias_keeps_spelling_inode_and_recovery_authority() {
    for name in ["AGENTS.md", "GEMINI.md"] {
        let f = Fixture::new();
        let path = f.root.join("agy").join(name);
        symlink("../source/AGENTS.md", &path).unwrap();
        let inode = fs::symlink_metadata(&path).unwrap().ino();
        let p = f.plan();
        assert_eq!(
            p["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["changed"] == true)
                .count(),
            1
        );
        f.apply(p, "command");
        f.doctor("ready");
        success(&["install", "recover"], &f.recovery());
        assert_eq!(
            fs::read_link(&path).unwrap(),
            PathBuf::from("../source/AGENTS.md")
        );
        assert_eq!(fs::symlink_metadata(path).unwrap().ino(), inode);
    }
}

#[test]
fn drifted_entries_and_indirect_alias_are_refused_without_writes() {
    for (name, field) in [
        ("AGENTS.md", "agy.instructions"),
        ("GEMINI.md", "agy.gemini_alias"),
    ] {
        let f = Fixture::new();
        let path = f.root.join("agy").join(name);
        fs::write(&path, b"# Foreign content\n").unwrap();
        refusal(
            &["install", "plan"],
            &f.spec,
            3,
            "instruction_conflict",
            field,
        );
        refusal(
            &["doctor", "inspect"],
            &f.spec,
            3,
            "instruction_conflict",
            field,
        );
        assert_eq!(fs::read(path).unwrap(), b"# Foreign content\n");
        assert!(!f.root.join("agy/.ap-install.lock").exists());
    }
    for target in ["AGENTS.md", "../source/missing", "../source/foreign"] {
        let f = Fixture::new();
        symlink(
            f.spec["canonical_path"].as_str().unwrap(),
            f.root.join("agy/AGENTS.md"),
        )
        .unwrap();
        fs::write(f.root.join("source/foreign"), b"foreign").unwrap();
        symlink(target, f.root.join("agy/GEMINI.md")).unwrap();
        let code = if target.ends_with("foreign") {
            "instruction_conflict"
        } else {
            "unsupported_target"
        };
        let field = if code == "instruction_conflict" {
            "agy.gemini_alias"
        } else {
            "target"
        };
        refusal(&["install", "plan"], &f.spec, 3, code, field);
        assert_eq!(
            fs::read_link(f.root.join("agy/GEMINI.md")).unwrap(),
            PathBuf::from(target)
        );
    }
}

#[test]
fn recovery_preserves_drift_and_reports_conflict() {
    let f = Fixture::new();
    f.apply(f.plan(), "command");
    fs::remove_file(f.root.join("agy/GEMINI.md")).unwrap();
    fs::write(f.root.join("agy/GEMINI.md"), b"# Later foreign edit\n").unwrap();
    let out = invoke(&["install", "recover"], &f.recovery());
    assert_eq!(out.status.code(), Some(5));
    assert!(out.stderr.is_empty());
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["outcome"], "recovery_required");
    assert_eq!(
        r["error"],
        json!({"code":"owned_field_conflict","field":"target"})
    );
    assert_eq!(r["entries"][0]["phase"], "recovered");
    assert_eq!(r["entries"][1]["phase"], "conflicted");
    assert_eq!(
        fs::read(f.root.join("agy/GEMINI.md")).unwrap(),
        b"# Later foreign edit\n"
    );
    assert!(fs::symlink_metadata(f.root.join("agy/AGENTS.md")).is_err());
}

#[test]
fn agy_spec_admission_collisions_and_forged_plan_refuse() {
    let f = Fixture::new();
    let mut missing = f.spec.clone();
    missing.as_object_mut().unwrap().remove("agy_dir");
    refusal(
        &["install", "plan"],
        &missing,
        3,
        "invalid_profile",
        "agy_dir",
    );
    let mut null = f.spec.clone();
    null["agy_dir"] = Value::Null;
    refusal(&["install", "plan"], &null, 2, "invalid_input", "stdin");
    let mut nested = f.spec.clone();
    nested["agy_dir"] = json!(f.root.join("absent-codex/nested"));
    refusal(
        &["install", "plan"],
        &nested,
        3,
        "path_collision",
        "profiles",
    );
    let mut p = f.plan();
    p["entries"][1]["after"]["target"] = json!("/foreign/source");
    refusal(
        &["install", "apply"],
        &json!({"plan":p,"command_dir":f.root.join("command"),"writers_quiesced":true}),
        3,
        "invalid_plan",
        "entries",
    );
    assert!(!f.root.join("command/state.json").exists());
}

#[test]
fn legacy_prefix_profile_order_and_doctor_shape_stay_unchanged() {
    let f = Fixture::new();
    for dir in ["profile", "profile-"] {
        fs::create_dir(f.root.join(dir)).unwrap();
        fs::set_permissions(f.root.join(dir), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut spec = f.spec.clone();
    spec.as_object_mut().unwrap().remove("agy_dir");
    spec["scope"] = json!("both");
    spec["codex_dir"] = json!(f.root.join("profile"));
    spec["claude_dir"] = json!(f.root.join("profile-"));
    let p = success(&["install", "plan"], &spec);
    assert_eq!(p["entries"][0]["selector"], "codex_instruction_link");
    assert_eq!(p["entries"][1]["selector"], "claude_canonical_import");
    assert!(p["spec"].get("agy_dir").is_none());
    let d = success(&["doctor", "inspect"], &spec);
    assert!(d.get("muse").is_none());
    assert_eq!(d.as_object().unwrap().len(), 7);
}
