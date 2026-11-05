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
pub fn append(path: &str, snapshot: &Snapshot) -> Result<Receipt> {
    if std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        != Some("snapshots-v2.jsonl")
    {
        return Err(Error("invalid_journal_name"));
    }
    let bytes = row(snapshot)?;
    let (parent, name) = filesystem::parent(path)?;
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
    let mut f = filesystem::regular(fd)?;
    // Nonblocking native lock: a caller can reconcile/retry a busy journal;
    // there is no unbounded wait or custom coordination file.
    flock(&f, FlockOperation::NonBlockingLockExclusive).map_err(|_| Error("journal_busy"))?;
    let check = || -> Result<()> {
        let fd = openat(
            &parent,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Error("journal_replaced"))?;
        if !same_inode(&f, &filesystem::regular(fd)?)? {
            return Err(Error("journal_replaced"));
        }
        Ok(())
    };
    check()?;
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
        let prior: Snapshot = strict::parse(b, LINE_LIMIT)?;
        prior.validate()?;
        if !ids.insert(prior.measurement_id.clone()) {
            return Err(Error("duplicate_measurement_id"));
        }
        if prior.measurement_id == snapshot.measurement_id {
            if b != bytes {
                return Err(Error("measurement_id_conflict"));
            }
            existing = Some((line, offset, b.len()));
        }
        offset += b.len();
    }
    let (line_number, byte_offset, prefix_len) = if let Some((n, o, len)) = existing {
        (n, o, o + len)
    } else {
        if line >= 4096 || before.len().saturating_add(bytes.len()) > JOURNAL_LIMIT {
            return Err(Error("journal_bounds"));
        }
        // The held parent descriptor prevents ancestor redirection; verify the
        // final name again immediately before the trusted append operation.
        let fd = openat(
            &parent,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Error("journal_replaced"))?;
        if !same_inode(&f, &filesystem::regular(fd)?)? {
            return Err(Error("journal_replaced"));
        }
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| Error("append_unknown"))?;
        let dir = openat(
            &parent,
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Error("append_unknown"))?;
        File::from(dir)
            .sync_all()
            .map_err(|_| Error("append_unknown"))?;
        (line + 1, before.len(), before.len() + bytes.len())
    };
    f.seek(SeekFrom::Start(0))
        .map_err(|_| Error("append_unknown"))?;
    let mut after = Vec::new();
    (&mut f)
        .take(JOURNAL_LIMIT as u64 + 1)
        .read_to_end(&mut after)
        .map_err(|_| Error("append_unknown"))?;
    if after.len() > JOURNAL_LIMIT
        || after.get(byte_offset..byte_offset + bytes.len()) != Some(bytes.as_slice())
        || !after.starts_with(&before)
    {
        return Err(Error("append_unknown"));
    }
    let fd = openat(
        &parent,
        name.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| Error("append_unknown"))?;
    if !same_inode(&f, &filesystem::regular(fd)?)? {
        return Err(Error("append_unknown"));
    }
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
