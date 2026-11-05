//! Native locked, checked, append-only journal. Never repairs/truncates history.
use crate::{Error, Result, digest, filesystem, model::*, strict};
use rustix::fs::{FlockOperation, Mode, OFlags, flock, fstat, openat};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub measurement_id: String,
    pub journal_path: String,
    pub line_number: u64,
    pub byte_offset: u64,
    pub row_bytes: u64,
    pub row_sha256: String,
    pub journal_prefix_bytes: u64,
    pub journal_prefix_sha256: String,
    pub capture_status: String,
}
pub fn row(snapshot: &Snapshot) -> Result<Vec<u8>> {
    snapshot.validate()?;
    let mut b = strict::bytes(snapshot)?;
    b.push(b'\n');
    if b.len() > LINE_LIMIT {
        return Err(Error("row_bounds"));
    }
    Ok(b)
}
fn same_inode(a: &File, b: &File) -> Result<bool> {
    let a = fstat(a).map_err(|_| Error("journal_read"))?;
    let b = fstat(b).map_err(|_| Error("journal_read"))?;
    Ok(a.st_dev == b.st_dev && a.st_ino == b.st_ino)
}
fn check_binding(parent: &filesystem::DirectoryChain, name: &str, f: &File) -> Result<()> {
    parent.verify().map_err(|_| Error("journal_replaced"))?;
    let fd = openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| Error("journal_replaced"))?;
    let named = filesystem::regular(fd).map_err(|_| Error("journal_replaced"))?;
    if !same_inode(f, &named)? {
        return Err(Error("journal_replaced"));
    }
    Ok(())
}
pub fn append(path: &str, snapshot: &Snapshot) -> Result<Receipt> {
    append_with_sync(path, snapshot, &mut |f, _| {
        f.sync_all().map_err(|_| Error("append_unknown"))
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncTarget {
    Journal,
    Parent,
}
fn append_with_sync(
    path: &str,
    snapshot: &Snapshot,
    sync: &mut impl FnMut(&File, SyncTarget) -> Result<()>,
) -> Result<Receipt> {
    if std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        != Some("snapshots-v2.jsonl")
    {
        return Err(Error("invalid_journal_name"));
    }
    let bytes = row(snapshot)?;
    let (parent, name) = filesystem::parent(path).map_err(|_| Error("journal_open"))?;
    let fd = openat(
        &parent,
        name.as_str(),
        OFlags::RDWR
            | OFlags::APPEND
            | OFlags::CREATE
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|_| Error("journal_open"))?;
    let mut f = filesystem::regular(fd).map_err(|_| Error("journal_open"))?;
    // Nonblocking native lock: a caller can reconcile/retry a busy journal;
    // there is no unbounded wait or custom coordination file.
    flock(&f, FlockOperation::NonBlockingLockExclusive).map_err(|_| Error("journal_busy"))?;
    check_binding(&parent, &name, &f)?;
    if f.metadata().map_err(|_| Error("journal_read"))?.len() > JOURNAL_LIMIT as u64 {
        return Err(Error("journal_bounds"));
    }
    let mut before = Vec::new();
    (&mut f)
        .take(JOURNAL_LIMIT as u64 + 1)
        .read_to_end(&mut before)
        .map_err(|_| Error("journal_read"))?;
    if before.len() > JOURNAL_LIMIT || (!before.is_empty() && before.last() != Some(&b'\n')) {
        return Err(Error("journal_truncated"));
    }
    let mut ids = BTreeSet::new();
    let mut line = 0u64;
    let mut offset = 0usize;
    let mut existing = None;
    for b in before.split_inclusive(|b| *b == b'\n') {
        line += 1;
        if line > 4096 || b.len() > LINE_LIMIT {
            return Err(Error("journal_bounds"));
        }
        let prior: Snapshot =
            strict::parse(b, LINE_LIMIT).map_err(|_| Error("journal_invalid_row"))?;
        prior.validate().map_err(|_| Error("journal_invalid_row"))?;
        if !ids.insert(prior.measurement_id.clone()) {
            return Err(Error("journal_duplicate_measurement_id"));
        }
        if prior.measurement_id == snapshot.measurement_id {
            if b != bytes {
                return Err(Error("journal_measurement_id_conflict"));
            }
            existing = Some((line, offset, b.len()));
        }
        offset += b.len();
    }
    // Validate all ancestors again after prior-row parsing, immediately before
    // either a new append or the persistence attempt for an existing row.
    check_binding(&parent, &name, &f)?;
    let (line_number, byte_offset, prefix_len) = if let Some((n, o, len)) = existing {
        (n, o, o + len)
    } else {
        if line >= 4096 || before.len().saturating_add(bytes.len()) > JOURNAL_LIMIT {
            return Err(Error("journal_bounds"));
        }
        f.write_all(&bytes).map_err(|_| Error("append_unknown"))?;
        (line + 1, before.len(), before.len() + bytes.len())
    };
    // Once write_all may have run (or an existing row is being persisted), every
    // failure is uncertain append. Keep this mapping outside the whole boundary
    // so future fallible checks cannot leak request/schema error exit codes.
    let after = (|| -> Result<Vec<u8>> {
        sync(&f, SyncTarget::Journal)?;
        let dir = openat(
            &parent,
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Error("journal_sync"))?;
        sync(&File::from(dir), SyncTarget::Parent)?;
        check_binding(&parent, &name, &f)?;
        f.seek(SeekFrom::Start(0))
            .map_err(|_| Error("journal_read"))?;
        let mut after = Vec::new();
        (&mut f)
            .take(JOURNAL_LIMIT as u64 + 1)
            .read_to_end(&mut after)
            .map_err(|_| Error("journal_read"))?;
        if after.len() > JOURNAL_LIMIT
            || after.get(byte_offset..byte_offset + bytes.len()) != Some(bytes.as_slice())
            || !after.starts_with(&before)
        {
            return Err(Error("journal_readback"));
        }
        check_binding(&parent, &name, &f)?;
        Ok(after)
    })()
    .map_err(|_| Error("append_unknown"))?;
    Ok(Receipt {
        measurement_id: snapshot.measurement_id.clone(),
        journal_path: path.into(),
        line_number,
        byte_offset: byte_offset as u64,
        row_bytes: bytes.len() as u64,
        row_sha256: digest(&bytes),
        journal_prefix_bytes: prefix_len as u64,
        journal_prefix_sha256: digest(&after[..prefix_len]),
        capture_status: snapshot.capture_status.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "kpi-journal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> String {
            self.0.join("snapshots-v2.jsonl").to_str().unwrap().into()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn snapshot() -> Snapshot {
        let interval = Interval { start: 1, end: 1 };
        let source = || Source::new(Status::NotPerformed, "not_performed", 1, 1);
        let artifact = ArtifactRef::new("synthetic.json", b"{}");
        let guard = |kind: &str, unit: &str| GuardrailSample {
            kind: kind.into(),
            value: K::NotPerformed,
            unit: unit.into(),
            total: if kind == "recall" {
                K::NotPerformed
            } else {
                K::Unknown
            },
            free_gib: K::Unknown,
            source: source(),
            source_qualified: false,
            qualification: Qualification::NotPerformed,
            contract: artifact.clone(),
            recording: K::NotPerformed,
        };
        let s = Snapshot {
            schema_version: SCHEMA.into(),
            measurement_id: "synthetic-retry".into(),
            capture_status: "failed".into(),
            capture: interval,
            producer: Producer {
                revision: PRODUCER.into(),
                source_commit: "a".repeat(40),
                source_tree: "b".repeat(40),
                binary_sha256: "c".repeat(64),
                build_evidence: artifact.clone(),
                git_version: "synthetic".into(),
                gh_version: "synthetic".into(),
            },
            definition: DefinitionPin {
                revision: DEFINITION.into(),
                algorithm_revision: ALGORITHM.into(),
                contract: artifact.clone(),
                manifest: ManifestPin {
                    source_id: "ws-replan-plan-a-v1-manifest".into(),
                    sha256: "d".repeat(64),
                    bytes: 2,
                },
                request: artifact.clone(),
            },
            kpi_legacy_removal: Sample {
                value: K::NotPerformed,
                source: source(),
                source_qualified: false,
                qualification: Qualification::NotPerformed,
            },
            kpi_mcp_parity: Sample {
                value: K::NotPerformed,
                source: source(),
                source_qualified: false,
                qualification: Qualification::NotPerformed,
            },
            guard_ci_minutes_month: guard("billing", "linux_minutes"),
            guard_disk_free_gib: guard("disk", "bytes"),
            guard_escaped_defects_week: guard("defects", "count"),
            guard_recall_regression: guard("recall", "fraction"),
            repositories: vec![Repository {
                manifest_key: "synthetic".into(),
                identity: Identity {
                    github_id: 7,
                    full_name: "synthetic/repo".into(),
                },
                ref_name: "refs/heads/main".into(),
                ref_read: source(),
                commit_sha: K::Unknown,
                tree_sha: K::Unknown,
                tree_read: source(),
                backend: "not_performed".into(),
                population: Population {
                    total: 1,
                    path_set_sha256: "e".repeat(64),
                },
                removed: K::Unknown,
                leaf_path_set_sha256: K::Unknown,
                absent_path_set_sha256: K::Unknown,
                membership: K::Unknown,
                source_qualified: false,
                qualification: Qualification::NotPerformed,
            }],
            artifacts: vec![artifact],
        };
        s.validate().unwrap();
        s
    }
    #[test]
    fn retry_after_uncertain_write_syncs_both_targets_without_rewriting() {
        for failure in [SyncTarget::Journal, SyncTarget::Parent] {
            let dir = Scratch::new();
            let path = dir.path();
            let snapshot = snapshot();
            let failed = append_with_sync(&path, &snapshot, &mut |f, target| {
                if target == failure {
                    Err(Error("append_unknown"))
                } else {
                    f.sync_all().map_err(|_| Error("append_unknown"))
                }
            });
            assert_eq!(failed.err(), Some(Error("append_unknown")));
            let before = std::fs::read(&path).unwrap();
            assert_eq!(before, row(&snapshot).unwrap());
            let mut synced = vec![];
            let receipt = append_with_sync(&path, &snapshot, &mut |f, target| {
                synced.push(target);
                f.sync_all().map_err(|_| Error("append_unknown"))
            })
            .unwrap();
            assert_eq!(synced, vec![SyncTarget::Journal, SyncTarget::Parent]);
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(receipt.line_number, 1);
            assert_eq!(receipt.row_sha256, digest(&before));
        }
    }
    #[test]
    fn existing_row_cannot_acknowledge_a_failed_sync() {
        for failure in [SyncTarget::Journal, SyncTarget::Parent] {
            let dir = Scratch::new();
            let path = dir.path();
            let snapshot = snapshot();
            append(&path, &snapshot).unwrap();
            let before = std::fs::read(&path).unwrap();
            let result = append_with_sync(&path, &snapshot, &mut |f, target| {
                if target == failure {
                    Err(Error("append_unknown"))
                } else {
                    f.sync_all().map_err(|_| Error("append_unknown"))
                }
            });
            assert_eq!(result.err(), Some(Error("append_unknown")));
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
    #[test]
    fn post_append_special_file_and_parent_replacement_are_append_unknown() {
        for parent_swap in [false, true] {
            for phase in [SyncTarget::Journal, SyncTarget::Parent] {
                let root = Scratch::new();
                let parent = root.0.join("parent");
                let old = root.0.join("saved");
                std::fs::create_dir(&parent).unwrap();
                let path = parent.join("snapshots-v2.jsonl");
                let saved = root.0.join("saved-row");
                let s = snapshot();
                let result = append_with_sync(path.to_str().unwrap(), &s, &mut |f, target| {
                    if target == phase {
                        if parent_swap {
                            std::fs::rename(&parent, &old).unwrap();
                            std::fs::create_dir(&parent).unwrap();
                        } else {
                            std::fs::rename(&path, &saved).unwrap();
                            std::fs::create_dir(&path).unwrap();
                        }
                    }
                    f.sync_all().map_err(|_| Error("injected_sync_error"))
                });
                let err = result.err().unwrap();
                assert_eq!(err, Error("append_unknown"));
                assert_eq!(err.exit_code(), 4);
                let saved = if parent_swap {
                    old.join("snapshots-v2.jsonl")
                } else {
                    saved
                };
                assert_eq!(std::fs::read(saved).unwrap(), row(&s).unwrap());
            }
        }
    }
    #[test]
    fn all_persistence_failures_use_uncertain_append_namespace() {
        let root = Scratch::new();
        for target in [SyncTarget::Journal, SyncTarget::Parent] {
            let result = append_with_sync(&root.path(), &snapshot(), &mut |_, phase| {
                if phase == target {
                    Err(Error("not_regular_file"))
                } else {
                    Ok(())
                }
            });
            assert_eq!(result.err(), Some(Error("append_unknown")));
        }
    }
    #[test]
    fn invalid_prior_json_and_schema_are_journal_errors_without_writes() {
        let root = Scratch::new();
        let mut invalid = serde_json::to_value(snapshot()).unwrap();
        invalid["capture_status"] = serde_json::json!("invalid");
        let mut bytes = serde_json::to_vec(&invalid).unwrap();
        bytes.push(b'\n');
        for before in [b"not-json\n".to_vec(), b"{}\n".to_vec(), bytes] {
            std::fs::write(root.path(), &before).unwrap();
            let err = append(&root.path(), &snapshot()).err().unwrap();
            assert_eq!(err, Error("journal_invalid_row"));
            assert_eq!(err.exit_code(), 4);
            assert_eq!(std::fs::read(root.path()).unwrap(), before);
        }
    }
}
