#![cfg(target_os = "linux")]
use ap::{
    Entry,
    native::{
        Environment,
        content::{self, ContentInput, ContentSpec, OWNED, RenderContentRequest},
    },
};
use std::{
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let base = std::env::var_os("AP_S03_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        let path = base.join(format!(
            "content-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn spec(&self) -> ContentSpec {
        ContentSpec {
            schema_version: 1,
            agents_dir: self.0.join("agents").to_str().unwrap().into(),
            uid: rustix::process::geteuid().as_raw(),
            environment: Environment::Disposable {
                root: self.0.to_str().unwrap().into(),
            },
        }
    }
    fn fill(&self) {
        for (relative, text) in OWNED {
            let path = self.0.join("agents").join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request() -> RenderContentRequest {
    RenderContentRequest {
        schema_version: 1,
        agents_dir: "/does-not-exist/agents".into(),
        entries: OWNED
            .iter()
            .map(|(relative, _)| ContentInput {
                relative_path: (*relative).into(),
                before: Entry::Missing,
            })
            .collect(),
    }
}
#[test]
fn pure_render_fixed_complete_inventory_and_idempotence() {
    let mut req = request();
    let plan = content::render(&req).unwrap();
    assert_eq!(plan.artifacts.len(), 27);
    assert!(
        plan.artifacts
            .iter()
            .all(|a| a.changed && a.path.starts_with("/does-not-exist/agents/"))
    );
    assert!(plan.artifacts.windows(2).all(|p| p[0].path < p[1].path));
    req.entries = plan
        .artifacts
        .iter()
        .map(|a| ContentInput {
            relative_path: a.relative_path.clone(),
            before: a.after.clone(),
        })
        .collect();
    assert!(
        content::render(&req)
            .unwrap()
            .artifacts
            .iter()
            .all(|a| !a.changed)
    );
    req.entries[0].before = Entry::File {
        text: "changed content".into(),
    };
    assert_eq!(
        content::render(&req)
            .unwrap()
            .artifacts
            .iter()
            .filter(|a| a.changed)
            .count(),
        1
    );
}
#[test]
fn render_refuses_unknown_duplicate_incomplete_and_foreign_shapes() {
    let mut req = request();
    req.entries.pop();
    assert_eq!(
        content::render(&req).unwrap_err().code,
        "content_set_mismatch"
    );
    for unknown in [
        "settings.json",
        "skills/explain-code-clearly/SKILL.md",
        "skills/explain-code-clearly/agents/openai.yaml",
    ] {
        let mut req = request();
        req.entries[0].relative_path = unknown.into();
        assert!(content::render(&req).is_err());
    }
    let mut req = request();
    req.entries[0] = req.entries[1].clone();
    assert!(content::render(&req).is_err());
    for entry in [
        Entry::Symlink {
            target: "/foreign".into(),
        },
        Entry::File { text: "\0".into() },
    ] {
        let mut req = request();
        req.entries[0].before = entry;
        assert!(content::render(&req).is_err());
    }
    let mut req = request();
    req.agents_dir = "/root/../foreign".into();
    assert!(content::render(&req).is_err());
}
#[test]
fn doctor_missing_ready_drift_is_read_only_and_opaque_files_are_untouched() {
    let f = Fixture::new();
    let spec = f.spec();
    let missing = content::doctor(&spec).unwrap();
    assert_eq!(missing.entries.len(), 27);
    assert!(missing.entries.iter().all(|e| e.status == "missing"));
    assert!(!f.0.join("agents").exists());
    f.fill();
    let opaque = f.0.join("agents/settings.json");
    fs::write(&opaque, [0xff, 0x00]).unwrap();
    let hook = f.0.join("agents/event-hook");
    fs::write(&hook, "#!/bin/sh\nexit 99\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let ready = content::doctor(&spec).unwrap();
    assert!(
        ready
            .entries
            .iter()
            .all(|e| e.status == "ready" && e.observed_sha256.as_ref() == Some(&e.expected_sha256))
    );
    let path = f.0.join("agents/runtime-references/skills.md");
    fs::write(&path, "edited\n").unwrap();
    let drift = content::doctor(&spec).unwrap();
    assert_eq!(
        drift.entries.iter().filter(|e| e.status == "drift").count(),
        1
    );
    assert_eq!(
        drift.entries.iter().filter(|e| e.status == "ready").count(),
        26
    );
    assert_eq!(fs::read(&path).unwrap(), b"edited\n");
    assert_eq!(fs::read(&opaque).unwrap(), [0xff, 0x00]);
    assert_eq!(drift.hooks_executed, 0);
    assert_eq!(drift.native_loading, "unverified");
    assert!(!drift.p6_complete);
    assert!(!f.0.join("agents/.ap-install.lock").exists());
}
#[test]
fn doctor_refuses_unsafe_paths_links_hardlinks_and_metadata() {
    let f = Fixture::new();
    f.fill();
    let spec = f.spec();
    let path = f.0.join("agents/AGENTS.md");
    fs::remove_file(&path).unwrap();
    symlink("/foreign", &path).unwrap();
    assert_eq!(
        content::doctor(&spec).unwrap_err().code,
        "unsupported_target"
    );
    fs::remove_file(&path).unwrap();
    fs::write(&path, "public").unwrap();
    fs::hard_link(&path, f.0.join("duplicate")).unwrap();
    assert_eq!(content::doctor(&spec).unwrap_err().code, "unsafe_metadata");
    fs::remove_file(f.0.join("duplicate")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(content::doctor(&spec).unwrap_err().code, "unsafe_metadata");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let mut invalid = spec.clone();
    invalid.uid = invalid.uid.wrapping_add(1);
    assert_eq!(content::doctor(&invalid).unwrap_err().code, "uid_mismatch");
    invalid = spec.clone();
    invalid.agents_dir = "/elsewhere".into();
    assert!(content::doctor(&invalid).is_err());
    fs::rename(f.0.join("agents/runtime-references"), f.0.join("moved")).unwrap();
    symlink(f.0.join("moved"), f.0.join("agents/runtime-references")).unwrap();
    assert_eq!(content::doctor(&spec).unwrap_err().code, "unsafe_topology");
}
#[test]
fn explicit_cli_boundary_rejects_unknown_fields_and_has_no_apply() {
    let f = Fixture::new();
    let spec = f.spec();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["doctor", "content"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    cmd.stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&spec).unwrap())
        .unwrap();
    let result = cmd.wait_with_output().unwrap();
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["entries"].as_array().unwrap().len(), 27);
    let mut value = serde_json::to_value(spec).unwrap();
    value["settings_path"] = "/foreign".into();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["doctor", "content"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    cmd.stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&value).unwrap())
        .unwrap();
    let result = cmd.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "{\"error\":{\"code\":\"invalid_input\",\"field\":\"stdin\"}}\n"
    );
    let result = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["install", "content"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}
