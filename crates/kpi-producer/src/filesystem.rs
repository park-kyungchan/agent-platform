//! Held-descriptor traversal; no symlinks, special files or check-then-open.
use crate::{Error, Result, digest, model::ArtifactRef};
use rustix::fd::OwnedFd;
use rustix::fs::{FileType, Mode, OFlags, fstat, mkdirat, open, openat};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path};

fn names(path: &str) -> Result<Vec<&std::ffi::OsStr>> {
    let mut parts = Path::new(path).components();
    if parts.next() != Some(Component::RootDir) {
        return Err(Error("invalid_path"));
    }
    parts
        .map(|c| {
            if let Component::Normal(n) = c {
                Ok(n)
            } else {
                Err(Error("invalid_path"))
            }
        })
        .collect()
}
fn flags() -> OFlags {
    OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC
}
pub fn directory(path: &str) -> Result<OwnedFd> {
    let mut d = open("/", flags(), Mode::empty()).map_err(|_| Error("source_unavailable"))?;
    for n in names(path)? {
        d = openat(&d, n, flags(), Mode::empty()).map_err(|_| Error("source_unavailable"))?;
    }
    Ok(d)
}
pub fn parent(path: &str) -> Result<(OwnedFd, String)> {
    names(path)?;
    let p = Path::new(path);
    let name = p
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(Error("invalid_path"))?;
    let d = directory(
        p.parent()
            .and_then(|s| s.to_str())
            .ok_or(Error("invalid_path"))?,
    )?;
    Ok((d, name.into()))
}
pub fn regular(fd: OwnedFd) -> Result<File> {
    let s = fstat(&fd).map_err(|_| Error("source_unavailable"))?;
    if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile {
        return Err(Error("not_regular_file"));
    }
    Ok(File::from(fd))
}
pub fn read(path: &str, limit: usize) -> Result<Vec<u8>> {
    let (d, n) = parent(path)?;
    let fd = openat(
        &d,
        n.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| Error("source_unavailable"))?;
    bounded(regular(fd)?, limit)
}
pub fn bounded(mut f: File, limit: usize) -> Result<Vec<u8>> {
    let before = f.metadata().map_err(|_| Error("source_unavailable"))?;
    if before.len() > limit as u64 {
        return Err(Error("source_bounds"));
    }
    let mut out = Vec::new();
    (&mut f)
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| Error("source_unavailable"))?;
    let after = f.metadata().map_err(|_| Error("source_unavailable"))?;
    if out.len() > limit
        || before.len() != after.len()
        || after.len() != out.len() as u64
        || before.modified().ok() != after.modified().ok()
    {
        return Err(Error("source_changed"));
    }
    Ok(out)
}
pub struct Evidence {
    dir: OwnedFd,
    parent: OwnedFd,
    name: String,
    pub references: Vec<ArtifactRef>,
    bytes: u64,
    limit: u64,
}
impl Evidence {
    /// Parent must already exist. Each capture directory is create-new, never reused.
    pub fn create(root: &str, id: &str, limit: u64) -> Result<Self> {
        if !crate::model::measurement_id(id) {
            return Err(Error("invalid_measurement_id"));
        }
        let parent = directory(root)?;
        mkdirat(&parent, id, Mode::RUSR | Mode::WUSR | Mode::XUSR)
            .map_err(|_| Error("evidence_exists_or_unavailable"))?;
        let dir = openat(&parent, id, flags(), Mode::empty())
            .map_err(|_| Error("evidence_unavailable"))?;
        Ok(Self {
            dir,
            parent,
            name: id.into(),
            references: Vec::new(),
            bytes: 0,
            limit,
        })
    }
    pub fn retain(&mut self, name: &str, bytes: &[u8]) -> Result<ArtifactRef> {
        let a = ArtifactRef::new(name, bytes);
        if !a.valid() || self.references.iter().any(|r| r.path == name) {
            return Err(Error("invalid_artifact"));
        }
        let count = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(Error("evidence_bounds"))?;
        if count > self.limit {
            return Err(Error("evidence_bounds"));
        }
        let fd = openat(
            &self.dir,
            name,
            OFlags::WRONLY
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| Error("evidence_write"))?;
        let mut f = regular(fd)?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| Error("evidence_write"))?;
        self.bytes = count;
        self.references.push(a.clone());
        Ok(a)
    }
    pub fn read(&self, a: &ArtifactRef) -> Result<Vec<u8>> {
        if !a.valid() || a.bytes > self.limit {
            return Err(Error("invalid_artifact"));
        }
        let fd = openat(
            &self.dir,
            a.path.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Error("evidence_read"))?;
        let b = bounded(regular(fd)?, a.bytes as usize)?;
        if b.len() as u64 != a.bytes || digest(&b) != a.sha256 {
            return Err(Error("evidence_mismatch"));
        }
        Ok(b)
    }
    pub fn verify(&self) -> Result<()> {
        self.verify_with(&mut sync_directory)
    }
    fn verify_with(&self, sync: &mut impl FnMut(&OwnedFd) -> Result<()>) -> Result<()> {
        let named = openat(&self.parent, self.name.as_str(), flags(), Mode::empty())
            .map_err(|_| Error("evidence_replaced"))?;
        let a = fstat(&named).map_err(|_| Error("evidence_replaced"))?;
        let b = fstat(&self.dir).map_err(|_| Error("evidence_replaced"))?;
        if a.st_dev != b.st_dev || a.st_ino != b.st_ino {
            return Err(Error("evidence_replaced"));
        }
        for a in &self.references {
            self.read(a)?;
        }
        sync(&self.dir)?;
        // The capture directory is a new entry in this *different* parent.
        // Its own fsync cannot make that parent's entry durable.
        sync(&self.parent)
    }
}
fn sync_directory(directory: &OwnedFd) -> Result<()> {
    // O_PATH cannot fsync. Reopen the held directory for durability.
    let d = openat(
        directory,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| Error("evidence_sync"))?;
    File::from(d).sync_all().map_err(|_| Error("evidence_sync"))
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
                "kpi-sync-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn identity(fd: &OwnedFd) -> (u64, u64) {
        let s = fstat(fd).unwrap();
        (s.st_dev, s.st_ino)
    }
    #[test]
    fn evidence_sync_orders_contents_before_parent_entry() {
        let root = Scratch::new();
        let e = Evidence::create(root.0.to_str().unwrap(), "capture-a", 100).unwrap();
        let mut calls = vec![];
        e.verify_with(&mut |d| {
            calls.push(identity(d));
            sync_directory(d)
        })
        .unwrap();
        assert_eq!(calls, vec![identity(&e.dir), identity(&e.parent)]);
    }
    #[test]
    fn failed_parent_sync_does_not_verify_evidence() {
        let root = Scratch::new();
        let e = Evidence::create(root.0.to_str().unwrap(), "capture-a", 100).unwrap();
        let parent = identity(&e.parent);
        assert_eq!(
            e.verify_with(&mut |d| if identity(d) == parent {
                Err(Error("injected_parent_sync"))
            } else {
                sync_directory(d)
            }),
            Err(Error("injected_parent_sync"))
        );
    }
}
