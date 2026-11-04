//! Descriptor-relative operations; no traversal through caller-controlled links.
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use rustix::io::Errno;
use sha2::{Digest, Sha256};

use super::types::*;
use crate::Entry;

pub(super) const LIMIT: usize = 1_048_576;
const OPEN: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NONBLOCK);

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn valid_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() > 1
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
        && path[1..]
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
pub(super) fn beneath(path: &str, root: &str) -> bool {
    path.strip_prefix(root).is_some_and(|s| s.starts_with('/'))
}
pub(super) fn split(path: &str) -> Result<(&str, &str)> {
    if !valid_path(path) {
        return Err(err("invalid_path", "path"));
    }
    let (parent, name) = path
        .rsplit_once('/')
        .ok_or_else(|| err("invalid_path", "path"))?;
    Ok((if parent.is_empty() { "/" } else { parent }, name))
}
// Linux Stat field widths vary across architectures.
#[allow(clippy::unnecessary_cast)]
fn stamp(s: &fs::Stat) -> Stamp {
    Stamp {
        dev: s.st_dev,
        ino: s.st_ino,
        uid: s.st_uid,
        gid: s.st_gid,
        mode: s.st_mode,
        nlink: s.st_nlink as u64,
        len: s.st_size,
        mtime: s.st_mtime,
        mtime_nsec: s.st_mtime_nsec as i64,
        ctime: s.st_ctime,
        ctime_nsec: s.st_ctime_nsec as i64,
    }
}
fn check_owner(s: &fs::Stat, uid: u32, directory: bool) -> Result<()> {
    if (s.st_uid != uid && !(directory && s.st_uid == 0)) || s.st_mode & 0o7022 != 0 {
        return Err(err("unsafe_metadata", "metadata"));
    }
    if !directory && s.st_nlink != 1 {
        return Err(err("unsafe_metadata", "metadata"));
    }
    Ok(())
}
fn identity(path: String, s: &fs::Stat) -> DirectoryIdentity {
    DirectoryIdentity {
        path,
        dev: s.st_dev,
        ino: s.st_ino,
        uid: s.st_uid,
        gid: s.st_gid,
        mode: s.st_mode,
    }
}

pub(super) struct Dir {
    pub fd: OwnedFd,
    pub topology: Vec<DirectoryIdentity>,
}

pub(super) struct Lock(File);

impl AsFd for Lock {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A concurrent fork can inherit this open file description until exec.
        // Closing only our descriptor leaves its flock held by the child.
        // Release ownership at the end of the command even if a duplicate lives.
        let _ = self.0.unlock();
    }
}

impl Dir {
    pub fn open(path: &str, uid: u32) -> Result<Self> {
        if path != "/" && !valid_path(path) {
            return Err(err("invalid_path", "directory"));
        }
        let mut fd = fs::open("/", OPEN | OFlags::DIRECTORY, Mode::empty())
            .map_err(|_| err("unsafe_topology", "directory"))?;
        let first = fs::fstat(&fd).map_err(|_| err("io_error", "directory"))?;
        check_owner(&first, uid, true)?;
        let mut topology = vec![identity("/".into(), &first)];
        let mut current = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            fd = fs::openat(&fd, part, OPEN | OFlags::DIRECTORY, Mode::empty())
                .map_err(|_| err("unsafe_topology", "directory"))?;
            let s = fs::fstat(&fd).map_err(|_| err("io_error", "directory"))?;
            check_owner(&s, uid, true)?;
            current.push('/');
            current.push_str(part);
            topology.push(identity(current.clone(), &s));
        }
        Ok(Self { fd, topology })
    }
    pub fn private(path: &str, uid: u32) -> Result<Self> {
        let dir = Self::open(path, uid)?;
        let s = fs::fstat(&dir.fd).map_err(|_| err("io_error", "command_dir"))?;
        if s.st_uid != uid || s.st_mode & 0o7777 != 0o700 {
            return Err(err("unsafe_metadata", "command_dir"));
        }
        Ok(dir)
    }
    pub fn recheck(&self, uid: u32) -> Result<()> {
        let path = &self
            .topology
            .last()
            .ok_or_else(|| err("unsafe_topology", "directory"))?
            .path;
        if Self::open(path, uid)?.topology != self.topology {
            return Err(err("expected_before_conflict", "topology"));
        }
        Ok(())
    }
    pub fn command_lock(&self, uid: u32) -> Result<Lock> {
        // Lock the directory inode without adding another persistent artifact.
        let fd = fs::openat(&self.fd, ".", OPEN | OFlags::DIRECTORY, Mode::empty())
            .map_err(|_| err("io_error", "command_dir"))?;
        let file = File::from(fd);
        file.try_lock()
            .map_err(|_| err("lock_busy", "command_dir"))?;
        let lock = Lock(file);
        self.recheck(uid)?;
        Ok(lock)
    }
    pub fn empty(&self) -> Result<bool> {
        let entries = fs::Dir::read_from(&self.fd).map_err(|_| err("io_error", "command_dir"))?;
        for entry in entries {
            let entry = entry.map_err(|_| err("io_error", "command_dir"))?;
            if !matches!(entry.file_name().to_bytes(), b"." | b"..") {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn absent(&self, name: &str) -> Result<bool> {
        match fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Err(Errno::NOENT) => Ok(true),
            Ok(_) => Ok(false),
            Err(_) => Err(err("io_error", "target")),
        }
    }
    pub fn snapshot(&self, name: &str, uid: u32) -> Result<Snapshot> {
        let s = match fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(s) => s,
            Err(Errno::NOENT) => {
                return Ok(Snapshot {
                    entry: Entry::Missing,
                    stamp: None,
                    sha256: None,
                });
            }
            Err(_) => return Err(err("io_error", "target")),
        };
        let kind = FileType::from_raw_mode(s.st_mode);
        let entry = match kind {
            FileType::RegularFile => {
                check_owner(&s, uid, false)?;
                let fd = fs::openat(&self.fd, name, OPEN, Mode::empty())
                    .map_err(|_| err("unsafe_topology", "target"))?;
                let opened = fs::fstat(&fd).map_err(|_| err("io_error", "target"))?;
                if stamp(&s) != stamp(&opened) {
                    return Err(err("expected_before_conflict", "target"));
                }
                let mut bytes = Vec::new();
                File::from(fd)
                    .take((LIMIT + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|_| err("io_error", "target"))?;
                if bytes.len() > LIMIT {
                    return Err(err("input_too_large", "target"));
                }
                Entry::File {
                    text: String::from_utf8(bytes)
                        .map_err(|_| err("unsupported_text", "target"))?,
                }
            }
            FileType::Symlink => {
                // Symlink permissions are kernel-defined 0777; never chmod its target.
                if s.st_uid != uid || s.st_nlink != 1 {
                    return Err(err("unsafe_metadata", "target"));
                }
                let value = fs::readlinkat(&self.fd, name, Vec::new())
                    .map_err(|_| err("io_error", "target"))?;
                Entry::Symlink {
                    target: value
                        .to_str()
                        .map_err(|_| err("unsupported_text", "target"))?
                        .into(),
                }
            }
            _ => return Err(err("unsupported_target", "target")),
        };
        let after = fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| err("expected_before_conflict", "target"))?;
        if stamp(&after) != stamp(&s) {
            return Err(err("expected_before_conflict", "target"));
        }
        let digest = match &entry {
            Entry::File { text } => hash(text.as_bytes()),
            Entry::Symlink { target } => hash(target.as_bytes()),
            _ => unreachable!(),
        };
        Ok(Snapshot {
            entry,
            stamp: Some(stamp(&s)),
            sha256: Some(digest),
        })
    }
    pub fn lock(&self, uid: u32) -> Result<Lock> {
        let name = ".ap-install.lock";
        let fd = match fs::openat(
            &self.fd,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(fd) => {
                fs::fchmod(&fd, Mode::RUSR | Mode::WUSR).map_err(|_| err("io_error", "lock"))?;
                fd
            }
            Err(Errno::EXIST) => fs::openat(
                &self.fd,
                name,
                OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(|_| err("unsafe_metadata", "lock"))?,
            Err(_) => return Err(err("io_error", "lock")),
        };
        let s = fs::fstat(&fd).map_err(|_| err("io_error", "lock"))?;
        if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile
            || s.st_uid != uid
            || s.st_nlink != 1
            || s.st_mode & 0o7777 != 0o600
        {
            return Err(err("unsafe_metadata", "lock"));
        }
        let file = File::from(fd);
        file.try_lock().map_err(|_| err("lock_busy", "lock"))?;
        let lock = Lock(file);
        let now = fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| err("expected_before_conflict", "lock"))?;
        if stamp(&s) != stamp(&now) {
            return Err(err("expected_before_conflict", "lock"));
        }
        Ok(lock)
    }
    pub fn read_state(&self, uid: u32) -> Result<State> {
        let snap = self.snapshot("state.json", uid)?;
        if snap.stamp.as_ref().is_none_or(|s| s.mode & 0o7777 != 0o600) {
            return Err(err("unsafe_metadata", "state"));
        }
        let Entry::File { text } = snap.entry else {
            return Err(err("invalid_state", "state"));
        };
        serde_json::from_str(&text).map_err(|_| err("invalid_state", "state"))
    }
    pub fn save_state(&self, state: &State) -> Result<()> {
        let bytes = serde_json::to_vec(state).map_err(|_| err("invalid_state", "state"))?;
        if bytes.len() > LIMIT {
            return Err(err("input_too_large", "state"));
        }
        let name = format!(".ap-state-stage-{}", std::process::id());
        // A leftover from process death is never blindly overwritten.
        let fd = fs::openat(
            &self.fd,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| err("state_stage_exists", "state"))?;
        let mut file = File::from(fd);
        let result = (|| {
            fs::fchmod(&file, Mode::RUSR | Mode::WUSR).map_err(|_| err("io_error", "state"))?;
            file.write_all(&bytes)
                .map_err(|_| err("io_error", "state"))?;
            file.sync_all().map_err(|_| err("io_error", "state"))?;
            self.recheck(state.plan.spec.uid)?;
            fs::renameat(&self.fd, name.as_str(), &self.fd, "state.json")
                .map_err(|_| err("io_error", "state"))?;
            fs::fsync(&self.fd).map_err(|_| err("io_error", "state"))
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&self.fd, name.as_str(), AtFlags::empty());
        }
        result
    }
}

pub(super) fn source(spec: &Spec) -> Result<SourceFingerprint> {
    let (parent, name) = split(&spec.canonical_path)?;
    let dir = Dir::open(parent, spec.uid)?;
    let snap = dir.snapshot(name, spec.uid)?;
    if !matches!(snap.entry, Entry::File { .. }) {
        return Err(err("unsafe_topology", "canonical_path"));
    }
    Ok(SourceFingerprint {
        stamp: snap
            .stamp
            .ok_or_else(|| err("unsafe_topology", "canonical_path"))?,
        sha256: snap
            .sha256
            .ok_or_else(|| err("unsafe_topology", "canonical_path"))?,
    })
}

/// Verify link resolution component-by-component without following an ancestor.
pub(super) fn direct_link(
    dir: &Dir,
    name: &str,
    spec: &Spec,
    snap: &Snapshot,
    field: &str,
) -> Result<()> {
    if let Entry::Symlink { target } = &snap.entry {
        let base = &dir
            .topology
            .last()
            .ok_or_else(|| err("unsafe_topology", "target"))?
            .path;
        let mut parts: Vec<&str> = if target.starts_with('/') {
            vec![]
        } else {
            base[1..].split('/').collect()
        };
        let target_parts: Vec<_> = target.trim_start_matches('/').split('/').collect();
        for (index, part) in target_parts.iter().enumerate() {
            let part = *part;
            match part {
                "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err(err("unsupported_target", "target"));
                    }
                }
                "" => return Err(err("unsupported_target", "target")),
                _ => parts.push(part),
            }
            if index + 1 < target_parts.len() {
                // Do not lexically erase an existing symlink/non-directory or
                // nonexistent component before '..'. The kernel traverses it.
                let intermediate = format!("/{}", parts.join("/"));
                Dir::open(&intermediate, spec.uid)
                    .map_err(|_| err("unsupported_target", "target"))?;
            }
        }
        let path = format!("/{}", parts.join("/"));
        let (parent, leaf) = split(&path)?;
        let target_dir =
            Dir::open(parent, spec.uid).map_err(|_| err("unsupported_target", "target"))?;
        let target_snap = target_dir.snapshot(leaf, spec.uid)?;
        if !matches!(target_snap.entry, Entry::File { .. }) {
            return Err(err("unsupported_target", "target"));
        }
        if path != spec.canonical_path {
            return Err(err("instruction_conflict", field));
        }
        if dir.snapshot(name, spec.uid)? != *snap {
            return Err(err("expected_before_conflict", "target"));
        }
    }
    Ok(())
}

/// Prepared sibling artifact. Drop removes only this process's stage entry.
pub(super) struct Stage<'a> {
    dir: &'a Dir,
    name: String,
    pub desired: Entry,
    pub snapshot: Snapshot,
    expected_missing: bool,
}
impl<'a> Stage<'a> {
    pub fn create(
        dir: &'a Dir,
        index: usize,
        desired: &Entry,
        before: &Snapshot,
        uid: u32,
    ) -> Result<Self> {
        let name = format!(".ap-stage-{}-{index}", std::process::id());
        match desired {
            Entry::File { text } => {
                let fd = fs::openat(
                    &dir.fd,
                    name.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::RUSR | Mode::WUSR,
                )
                .map_err(|_| err("stage_exists", "target"))?;
                let mut file = File::from(fd);
                // Keep stage private until bytes and preserved metadata are complete.
                let result = (|| {
                    file.write_all(text.as_bytes())
                        .map_err(|_| err("io_error", "stage"))?;
                    if let Some(stamp) = &before.stamp {
                        let group = rustix::process::Gid::from_raw(stamp.gid);
                        fs::fchown(&file, None, Some(group))
                            .map_err(|_| err("unsafe_metadata", "stage"))?;
                        fs::fchmod(&file, Mode::from_raw_mode(stamp.mode & 0o777))
                            .map_err(|_| err("io_error", "stage"))?;
                    } else {
                        fs::fchmod(&file, Mode::from_raw_mode(0o644))
                            .map_err(|_| err("io_error", "stage"))?;
                    }
                    file.sync_all().map_err(|_| err("io_error", "stage"))
                })();
                if let Err(e) = result {
                    let _ = fs::unlinkat(&dir.fd, name.as_str(), AtFlags::empty());
                    return Err(e);
                }
            }
            Entry::Symlink { target } => fs::symlinkat(target.as_str(), &dir.fd, name.as_str())
                .map_err(|_| err("stage_exists", "target"))?,
            Entry::Missing => {
                return Ok(Self {
                    dir,
                    name,
                    desired: desired.clone(),
                    expected_missing: before.entry == Entry::Missing,
                    snapshot: Snapshot {
                        entry: Entry::Missing,
                        stamp: None,
                        sha256: None,
                    },
                });
            }
            _ => return Err(err("unsupported_target", "stage")),
        }
        let snapshot = dir.snapshot(&name, uid)?;
        if snapshot.entry != *desired {
            let _ = fs::unlinkat(&dir.fd, name.as_str(), AtFlags::empty());
            return Err(err("stage_validation", "stage"));
        }
        if let (Some(old), Some(new)) = (&before.stamp, &snapshot.stamp)
            && matches!(desired, Entry::File { .. })
            && (old.uid, old.gid, old.mode) != (new.uid, new.gid, new.mode)
        {
            let _ = fs::unlinkat(&dir.fd, name.as_str(), AtFlags::empty());
            return Err(err("stage_validation", "stage"));
        }
        fs::fsync(&dir.fd).map_err(|_| err("io_error", "stage"))?;
        Ok(Self {
            dir,
            name,
            desired: desired.clone(),
            snapshot,
            expected_missing: before.entry == Entry::Missing,
        })
    }
    pub fn replace(&self, target: &str, uid: u32) -> Result<()> {
        if matches!(self.desired, Entry::Missing) {
            fs::unlinkat(&self.dir.fd, target, AtFlags::empty())
                .map_err(|_| err("io_error", "replace"))?;
        } else {
            if self.dir.snapshot(&self.name, uid)? != self.snapshot {
                return Err(err("stage_validation", "stage"));
            }
            if self.expected_missing {
                fs::renameat_with(
                    &self.dir.fd,
                    self.name.as_str(),
                    &self.dir.fd,
                    target,
                    fs::RenameFlags::NOREPLACE,
                )
                .map_err(|_| err("expected_before_conflict", "replace"))?;
            } else {
                fs::renameat(&self.dir.fd, self.name.as_str(), &self.dir.fd, target)
                    .map_err(|_| err("io_error", "replace"))?;
            }
        }
        fs::fsync(&self.dir.fd).map_err(|_| err("io_error", "replace"))
    }
}
impl Drop for Stage<'_> {
    fn drop(&mut self) {
        if !matches!(self.desired, Entry::Missing)
            && let Ok(now) = fs::statat(&self.dir.fd, self.name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            && self
                .snapshot
                .stamp
                .as_ref()
                .is_some_and(|old| old.dev == now.st_dev && old.ino == now.st_ino)
        {
            let _ = fs::unlinkat(&self.dir.fd, self.name.as_str(), AtFlags::empty());
        }
    }
}
