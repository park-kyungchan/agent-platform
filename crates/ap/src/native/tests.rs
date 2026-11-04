//! Faults are injected inside the module, never through a production CLI option.
//! Requires explicit AP_S03_TEST_ROOT; no /tmp/global/native fallback.
use super::super::recovery::{recover, recover_observed};
use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    spec: Spec,
}
impl Fixture {
    fn agy() -> Self {
        let mut f = Self::new();
        let dir = f.root.join("c-agy");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        f.spec.agy_dir = Some(dir.to_str().unwrap().into());
        f.spec.scope = Scope::Agy;
        f
    }
    fn new() -> Self {
        let parent = PathBuf::from(
            std::env::var_os("AP_S03_TEST_ROOT").expect("explicit AP_S03_TEST_ROOT required"),
        );
        Dir::private(
            parent.to_str().unwrap(),
            rustix::process::geteuid().as_raw(),
        )
        .unwrap();
        let root = parent.join(format!(
            "rust-s03-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["source", "a-codex", "b-claude", "command"] {
            let p = root.join(name);
            fs::create_dir(&p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(root.join("source/AGENTS.md"), "# Synthetic canonical\n").unwrap();
        fs::set_permissions(
            root.join("source/AGENTS.md"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let s = |name: &str| root.join(name).to_str().unwrap().to_owned();
        let spec = Spec {
            schema_version: 1,
            canonical_path: s("source/AGENTS.md"),
            codex_dir: s("a-codex"),
            claude_dir: s("b-claude"),
            scope: Scope::Both,
            uid: rustix::process::geteuid().as_raw(),
            environment: Environment::Disposable {
                root: root.to_str().unwrap().into(),
            },
            agy_dir: None,
        };
        Self { root, spec }
    }
    fn request(&self) -> ApplyRequest {
        ApplyRequest {
            plan: plan(&self.spec).unwrap(),
            command_dir: self.root.join("command").to_str().unwrap().into(),
            writers_quiesced: true,
        }
    }
    fn recovery(&self) -> RecoverRequest {
        RecoverRequest {
            command_dir: self.root.join("command").to_str().unwrap().into(),
            writers_quiesced: true,
        }
    }
    fn state(&self) -> State {
        Dir::private(self.recovery().command_dir.as_str(), self.spec.uid)
            .unwrap()
            .read_state(self.spec.uid)
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
impl<F: FnMut(Point, usize) -> Result<()>> Observer for F {
    fn event(&mut self, p: Point, i: usize) -> Result<()> {
        self(p, i)
    }
}
fn fault_phases(id: &str) -> Vec<Phase> {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../s03/conformance/cases-v2.json")).unwrap();
    let case = corpus["internal_fault_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    serde_json::from_value(case["expected"].clone()).unwrap()
}
fn phases(report: &InstallReport) -> Vec<Phase> {
    report.entries.iter().map(|e| e.phase).collect()
}

#[test]
fn recheck_conflict_and_unattempted_recovery_have_no_write_authority() {
    let f = Fixture::new();
    let request = f.request();
    let path = f.root.join("a-codex/AGENTS.md");
    let mut hook = |point, index| {
        if point == Point::BeforeRecheck && index == 0 {
            fs::write(&path, "# Unowned concurrent edit\n").unwrap();
        }
        Ok(())
    };
    let result = apply_observed(&request, &mut hook).unwrap();
    assert_eq!(phases(&result), fault_phases("recheck-edit"));
    recover(&f.recovery()).unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "# Unowned concurrent edit\n"
    );
}
#[test]
fn after_rename_intent_and_after_persist_differ() {
    for (id, target) in [
        ("after-first-replace", Point::AfterReplace),
        ("after-first-persist", Point::AfterPersist),
    ] {
        let f = Fixture::new();
        let request = f.request();
        let mut hook = |point, index| {
            if point == target && index == 0 {
                Err(err("injected_failure", "test"))
            } else {
                Ok(())
            }
        };
        let result = apply_observed(&request, &mut hook).unwrap();
        assert_eq!(phases(&result), fault_phases(id));
        assert!(f.root.join("a-codex/AGENTS.md").is_symlink());
        recover(&f.recovery()).unwrap();
        assert!(!f.root.join("a-codex/AGENTS.md").exists());
        assert!(!f.root.join("b-claude/CLAUDE.md").exists());
    }
}
#[test]
fn later_entry_failure_preserves_exact_partial_progress() {
    let f = Fixture::new();
    let request = f.request();
    let mut hook = |point, index| {
        if point == Point::BeforeRecheck && index == 1 {
            fs::write(f.root.join("b-claude/CLAUDE.md"), "# Edited\n").unwrap();
        }
        Ok(())
    };
    let result = apply_observed(&request, &mut hook).unwrap();
    assert_eq!(phases(&result), fault_phases("later-file-fails"));
    recover(&f.recovery()).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("b-claude/CLAUDE.md")).unwrap(),
        "# Edited\n"
    );
}
#[test]
fn replaced_ancestor_and_late_override_refuse() {
    for id in ["ancestor-replaced", "late-override"] {
        let f = Fixture::new();
        let request = f.request();
        let mut hook = |point, index| {
            if point == Point::BeforeRecheck && index == 0 {
                if id == "ancestor-replaced" {
                    fs::rename(f.root.join("a-codex"), f.root.join("displaced")).unwrap();
                    fs::create_dir(f.root.join("a-codex")).unwrap();
                } else {
                    fs::write(f.root.join("a-codex/AGENTS.override.md"), "# vendor\n").unwrap();
                }
            }
            Ok(())
        };
        let result = apply_observed(&request, &mut hook).unwrap();
        assert_eq!(phases(&result), fault_phases(id));
        assert!(!f.root.join("a-codex/AGENTS.md").exists());
    }
}
#[test]
fn recover_preserves_later_text_mode_and_unrelated_configs() {
    let f = Fixture::new();
    fs::write(f.root.join("b-claude/CLAUDE.md"), "# Original").unwrap();
    fs::set_permissions(
        f.root.join("b-claude/CLAUDE.md"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let config = f.root.join("b-claude/settings.json");
    let opaque = b"{\"mcp\":\"${SECRET}\",\"hooks\":[1,2],\"unknown\":true}\n";
    fs::write(&config, opaque).unwrap();
    apply(&f.request()).unwrap();
    let target = f.root.join("b-claude/CLAUDE.md");
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let mut bytes = fs::read(&target).unwrap();
    bytes.extend_from_slice(b"# Later\n");
    fs::write(&target, bytes).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(recover(&f.recovery()).unwrap().outcome, "recovered");
    assert_eq!(fs::read(&target).unwrap(), b"# Original\n# Later\n");
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read(config).unwrap(), opaque);
}
#[test]
fn staged_inode_fingerprint_refuses_ambiguous_prepared_edit() {
    let f = Fixture::new();
    let request = f.request();
    let mut hook = |point, index| {
        if point == Point::AfterReplace && index == 0 {
            Err(err("injected", "test"))
        } else {
            Ok(())
        }
    };
    apply_observed(&request, &mut hook).unwrap();
    let link = f.root.join("a-codex/AGENTS.md");
    fs::rename(&link, f.root.join("old-link")).unwrap();
    symlink(&f.spec.canonical_path, &link).unwrap();
    let result = recover(&f.recovery()).unwrap();
    assert_eq!(result.outcome, "recovery_required");
    assert!(link.is_symlink());
    assert_eq!(recover(&f.recovery()).unwrap().outcome, "recovery_required");
    assert!(link.is_symlink());
}

// Exact test-process restart, no model/native runtime session and no production fault flags.
#[test]
fn crash_worker() {
    let Some(root) = std::env::var_os("AP_S03_CRASH_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let spec: Spec = serde_json::from_slice(&fs::read(root.join("spec.json")).unwrap()).unwrap();
    let mode = std::env::var("AP_S03_CRASH_MODE").unwrap();
    let command_dir = root.join("command").to_str().unwrap().to_owned();
    let mut hook = |point, index| {
        if (mode == "install" && point == Point::AfterReplace && index == 0)
            || (mode == "recover" && point == Point::RecoverReplace && index == 1)
        {
            std::process::exit(77);
        }
        Ok(())
    };
    if mode == "install" {
        apply_observed(
            &ApplyRequest {
                plan: plan(&spec).unwrap(),
                command_dir,
                writers_quiesced: true,
            },
            &mut hook,
        )
        .unwrap();
    } else {
        recover_observed(
            &RecoverRequest {
                command_dir,
                writers_quiesced: true,
            },
            &mut hook,
        )
        .unwrap();
    }
    panic!("fault point not reached");
}
#[test]
fn process_death_install_and_recovery_reconcile_without_reapply() {
    for mode in ["install", "recover"] {
        let f = Fixture::new();
        fs::write(
            f.root.join("spec.json"),
            serde_json::to_vec(&f.spec).unwrap(),
        )
        .unwrap();
        if mode == "recover" {
            apply(&f.request()).unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native::install::tests::crash_worker",
                "--nocapture",
            ])
            .env("AP_S03_CRASH_ROOT", &f.root)
            .env("AP_S03_CRASH_MODE", mode)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(77));
        let id = if mode == "install" {
            "killed-after-rename"
        } else {
            "recovery-killed-after-rename"
        };
        assert_eq!(f.state().phases, fault_phases(id));
        assert_eq!(recover(&f.recovery()).unwrap().outcome, "recovered");
        assert!(!f.root.join("a-codex/AGENTS.md").exists());
        assert!(!f.root.join("b-claude/CLAUDE.md").exists());
    }
}

#[test]
fn completed_recovery_does_not_remove_a_later_same_value_install() {
    let f = Fixture::new();
    apply(&f.request()).unwrap();
    recover(&f.recovery()).unwrap();
    let path = f.root.join("b-claude/CLAUDE.md");
    let later = format!(
        "@{}\n# Another command owns this now\n",
        f.spec.canonical_path
    );
    fs::write(&path, &later).unwrap();
    assert_eq!(recover(&f.recovery()).unwrap().outcome, "recovered");
    assert_eq!(fs::read_to_string(path).unwrap(), later);
}

#[test]
fn command_directory_lock_refuses_cross_scope_reuse() {
    let f = Fixture::new();
    let request = f.request();
    let dir = Dir::private(&request.command_dir, f.spec.uid).unwrap();
    let _lock = dir.command_lock(f.spec.uid).unwrap();
    let error = apply(&request).unwrap_err();
    assert_eq!(error, err("lock_busy", "command_dir"));
    assert!(dir.absent("state.json").unwrap());
}

#[test]
fn completed_command_releases_lock_while_inherited_descriptor_remains_open() {
    let f = Fixture::new();
    let dir = Dir::private(&f.recovery().command_dir, f.spec.uid).unwrap();
    let lock = dir.command_lock(f.spec.uid).unwrap();
    // A concurrent fork inherits this same open file description until exec,
    // even though the descriptor has CLOEXEC. Duplication makes that lifetime
    // deterministic without a timing-sensitive fork or production fault hook.
    let inherited = rustix::io::fcntl_dupfd_cloexec(&lock, 0).unwrap();
    assert_eq!(
        dir.command_lock(f.spec.uid).err().unwrap(),
        err("lock_busy", "command_dir")
    );
    drop(lock);
    let next = dir.command_lock(f.spec.uid).unwrap();
    drop(inherited);
    assert_eq!(
        dir.command_lock(f.spec.uid).err().unwrap(),
        err("lock_busy", "command_dir")
    );
    drop(next);
    dir.command_lock(f.spec.uid).unwrap();
}

#[test]
fn completed_command_releases_profile_lock_while_inherited_descriptor_remains_open() {
    let f = Fixture::new();
    let dir = Dir::open(&f.spec.codex_dir, f.spec.uid).unwrap();
    let lock = dir.lock(f.spec.uid).unwrap();
    let inherited = rustix::io::fcntl_dupfd_cloexec(&lock, 0).unwrap();
    assert_eq!(
        dir.lock(f.spec.uid).err().unwrap(),
        err("lock_busy", "lock")
    );
    drop(lock);
    let next = dir.lock(f.spec.uid).unwrap();
    drop(inherited);
    assert_eq!(
        dir.lock(f.spec.uid).err().unwrap(),
        err("lock_busy", "lock")
    );
    drop(next);
    dir.lock(f.spec.uid).unwrap();
}

#[test]
fn fixed_refusal_identity_is_not_only_a_status() {
    let f = Fixture::new();
    let mut wrong = f.spec.clone();
    wrong.uid += 1;
    assert_eq!(plan(&wrong).unwrap_err(), err("uid_mismatch", "uid"));
    fs::write(
        f.root.join("a-codex/AGENTS.override.md"),
        "# Vendor override\n",
    )
    .unwrap();
    assert_eq!(
        plan(&f.spec).unwrap_err(),
        err("instruction_conflict", "codex.instruction_override")
    );
}

#[test]
fn lexical_dotdot_must_not_hide_a_traversed_symlink_or_missing_directory() {
    for route in ["symlink", "missing"] {
        let f = Fixture::new();
        if route == "symlink" {
            symlink(f.root.join("source"), f.root.join("a-codex/route")).unwrap();
        }
        symlink(
            "route/../../source/AGENTS.md",
            f.root.join("a-codex/AGENTS.md"),
        )
        .unwrap();
        assert_eq!(
            plan(&f.spec).unwrap_err(),
            err("unsupported_target", "target")
        );
    }
}

#[test]
fn agy_partial_apply_preserves_unattempted_foreign_alias() {
    let f = Fixture::agy();
    let request = f.request();
    let alias = f.root.join("c-agy/GEMINI.md");
    let mut hook = |point, index| {
        if point == Point::BeforeRecheck && index == 1 {
            fs::write(&alias, "# Foreign concurrent edit\n").unwrap();
        }
        Ok(())
    };
    let result = apply_observed(&request, &mut hook).unwrap();
    assert_eq!(result.outcome, "partial");
    assert_eq!(phases(&result), [Phase::Applied, Phase::Conflicted]);
    assert_eq!(
        result.error,
        Some(err("expected_before_conflict", "target"))
    );
    assert_eq!(recover(&f.recovery()).unwrap().outcome, "recovered");
    assert!(fs::symlink_metadata(f.root.join("c-agy/AGENTS.md")).is_err());
    assert_eq!(fs::read(alias).unwrap(), b"# Foreign concurrent edit\n");
}

#[test]
fn agy_rechecks_ancestors_and_shares_one_profile_lock() {
    let f = Fixture::agy();
    let request = f.request();
    let dir = Dir::open(f.spec.agy_dir.as_ref().unwrap(), f.spec.uid).unwrap();
    let lock = dir.lock(f.spec.uid).unwrap();
    assert_eq!(apply(&request).unwrap_err(), err("lock_busy", "lock"));
    drop(lock);
    let mut hook = |point, index| {
        if point == Point::BeforeRecheck && index == 1 {
            fs::rename(f.root.join("c-agy"), f.root.join("displaced-agy")).unwrap();
            fs::create_dir(f.root.join("c-agy")).unwrap();
        }
        Ok(())
    };
    let r = apply_observed(&request, &mut hook).unwrap();
    assert_eq!(phases(&r), [Phase::Applied, Phase::Conflicted]);
    assert_eq!(
        r.error,
        Some(err("expected_before_conflict", "dependencies"))
    );
    assert!(fs::symlink_metadata(f.root.join("displaced-agy/AGENTS.md")).is_ok());
    assert!(fs::symlink_metadata(f.root.join("displaced-agy/GEMINI.md")).is_err());
    assert!(fs::symlink_metadata(f.root.join("c-agy/GEMINI.md")).is_err());
}

#[test]
fn agy_process_death_install_and_recovery_reconcile_without_reapply() {
    for mode in ["install", "recover"] {
        let f = Fixture::agy();
        fs::write(
            f.root.join("spec.json"),
            serde_json::to_vec(&f.spec).unwrap(),
        )
        .unwrap();
        if mode == "recover" {
            apply(&f.request()).unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native::install::tests::crash_worker",
                "--nocapture",
            ])
            .env("AP_S03_CRASH_ROOT", &f.root)
            .env("AP_S03_CRASH_MODE", mode)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(77));
        let expected = if mode == "install" {
            vec![Phase::Prepared, Phase::Pending]
        } else {
            vec![Phase::Applied, Phase::Recovering]
        };
        assert_eq!(f.state().phases, expected);
        assert_eq!(recover(&f.recovery()).unwrap().outcome, "recovered");
        for name in ["AGENTS.md", "GEMINI.md"] {
            assert!(fs::symlink_metadata(f.root.join("c-agy").join(name)).is_err());
        }
    }
}
