//! `ap route index` and the function `route` (`specs/ap/route.md`): the sections of the Markdown this host
//! works from and the components of the repositories' maps, written as lines of the store's record sources
//! `sections` and `components`, and the answer `ont fn route` gives from their current objects.
mod parse;
mod rank;
pub(crate) use parse::atx_heading;
pub use parse::{
    HEADING_BYTES, SEPARATOR, Section, TEXT_BYTES, cut, key, lines, sections, text_of,
};
pub use rank::{Matched, score, stem, words};

use crate::host::{Failure, Host, LIMIT, System, digest, error, fail};
use crate::store;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, UNIX_EPOCH},
};

/// The directory the host store's sources read (R2).
pub const DIRECTORY: &str = "/var/lib/agent-platform/route";
/// The file of the shared instructions' sections in the directory (R6).
pub const INSTRUCTIONS: &str = "instructions";
/// A Markdown file larger than this, or with a longer path, is not indexed (R11).
pub const FILE_BYTES: u64 = 1_000_000;
pub const PATH_BYTES: usize = 1024;
/// Answers of the function: the default and the bounds of `limit` (R16).
pub const LIMIT_DEFAULT: u64 = 8;
pub const LIMIT_MAX: u64 = 20;
/// The answer stays under the function's `answer_bytes` (65,536) with this many bytes (R18).
pub const ANSWER_BYTES: usize = 60_000;
/// Objects asked per search page: one head is at most about 17 KB (4,000 bytes of text and 1,000 of heading
/// written as the title too, each escaped at most twice), so 50 heads fit the 1 MiB bound of one answer.
const PAGE: u64 = 50;
const FETCH_MS: u64 = 300_000;
const GIT_MS: u64 = 60_000;
const INDEX_MS: u64 = 600_000;
/// A temporary file of `ap` in the directory older than this is removed by `--prune` (R8,
/// `specs/ap/clause.md` R5).
const TEMPORARY_AGE: Duration = Duration::from_secs(3600);
const TEMPORARY_PREFIX: &str = ".ap-route.";
const TEMPORARY_SUFFIX: &str = ".tmp";

/// The arguments of `ap route index` (R1).
#[derive(Debug, Default, Clone)]
pub struct Options {
    pub directory: Option<String>,
    pub store: Option<String>,
    pub prune: bool,
    pub repositories: Vec<String>,
    pub instructions: Option<String>,
}

pub(crate) fn invalid(field: &str, detail: &str) -> Failure {
    fail("invalid_input", field, json!(detail))
}

pub(crate) fn absolute(path: &str) -> bool {
    Path::new(path).is_absolute() && !path.contains('\0')
}

/// `owner/name` of GitHub: an owner of ASCII letters, digits and `-`, a name of those and `.` and `_`.
pub(crate) fn repository_form(name: &str) -> bool {
    let Some((owner, repo)) = name.split_once('/') else {
        return false;
    };
    let ok = |s: &str, extra: &[u8]| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
    };
    ok(owner, b"-") && ok(repo, b"-._") && repo != "." && repo != ".."
}

/// The arguments after `index` (R1 to R3); every refusal comes before any effect.
pub fn options(argv: &[OsString]) -> Result<Options, Failure> {
    options_of(argv, DIRECTORY, true)
}

/// The arguments after `index` of a command that writes the files of record sources into one directory, whose
/// default is `default` (the one the host store's sources read): `ap route index`, and `ap clause index` without
/// `--instructions` (`specs/ap/clause.md` R1).
pub(crate) fn options_of(
    argv: &[OsString],
    default: &str,
    instructions: bool,
) -> Result<Options, Failure> {
    let mut o = Options::default();
    let mut i = 0;
    let usage = || error("usage", "args");
    while i < argv.len() {
        let flag = argv[i].to_str().ok_or_else(usage)?;
        if flag == "--prune" {
            if o.prune {
                return Err(usage());
            }
            o.prune = true;
            i += 1;
            continue;
        }
        let value = argv.get(i + 1).ok_or_else(usage)?;
        let field = flag.trim_start_matches('-');
        let value = value
            .to_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid(field, "is empty or not UTF-8"))?
            .to_owned();
        let slot = match flag {
            "--directory" => &mut o.directory,
            "--store" => &mut o.store,
            "--instructions" if instructions => &mut o.instructions,
            "--repository" => {
                o.repositories.push(value);
                i += 2;
                continue;
            }
            _ => return Err(usage()),
        };
        if slot.replace(value).is_some() {
            return Err(usage());
        }
        i += 2;
    }
    for r in &o.repositories {
        let name = r.split_once('/').map_or(r.as_str(), |(_, n)| n);
        if name.starts_with("education-") {
            return Err(invalid(
                "repository",
                "an education- repository is the Academy's",
            ));
        }
        if !repository_form(r) {
            return Err(invalid("repository", "is not owner/name"));
        }
    }
    let named: BTreeSet<&String> = o.repositories.iter().collect();
    if named.len() != o.repositories.len() {
        return Err(invalid("repository", "is named twice"));
    }
    if let Some(d) = &o.directory
        && !absolute(d)
    {
        return Err(invalid("directory", "is not an absolute path"));
    }
    if let Some(d) = &o.instructions
        && !absolute(d)
    {
        return Err(invalid("instructions", "is not an absolute path"));
    }
    let default = o
        .directory
        .as_deref()
        .is_none_or(|d| d.trim_end_matches('/') == default);
    match (default, o.store.is_some()) {
        (true, true) => Err(invalid(
            "directory",
            "a scratch store needs a directory other than the host's",
        )),
        (false, false) => Err(invalid(
            "directory",
            "a directory other than the host's needs a scratch store",
        )),
        _ => Ok(o),
    }
}

/// The file stem of a repository's files: `owner__name` (an owner has no `_`).
pub fn stem_of(repository: &str) -> String {
    repository.replacen('/', "__", 1)
}

/// One call of `git` in a clone; its standard output when it exits 0.
pub(crate) fn git(s: &mut impl System, clone: &str, words: &[&str], ms: u64) -> Result<Vec<u8>, ()> {
    let mut a: Vec<String> = vec!["--literal-pathspecs".into(), "-C".into(), clone.into()];
    a.extend(words.iter().map(|w| (*w).to_owned()));
    match s.call("git", &a, &Value::Null, ms) {
        Ok(r) if r.success => Ok(r.stdout),
        _ => Err(()),
    }
}

pub(crate) fn hex40(bytes: &[u8]) -> Option<String> {
    let t = String::from_utf8_lossy(bytes).trim().to_owned();
    (t.len() == 40 && t.bytes().all(|b| b.is_ascii_hexdigit())).then_some(t)
}

/// The last commit at `revision` that changed `path` (R5).
pub(crate) fn last_commit(
    s: &mut impl System,
    clone: &str,
    revision: &str,
    path: &str,
) -> Result<String, &'static str> {
    let out = git(
        s,
        clone,
        &["log", "-1", "--format=%H", revision, "--", path],
        GIT_MS,
    )
    .map_err(|_| "log_failed")?;
    hex40(&out).ok_or("log_failed")
}

/// The records of one file's sections (R4, R5, R9 to R12).
fn section_records(
    origin: Option<&str>,
    path: &str,
    bytes: &[u8],
    commit: Option<&str>,
    now: u64,
) -> Vec<Value> {
    let text = String::from_utf8_lossy(bytes);
    let all = lines(&text);
    let name = path.rsplit('/').next().unwrap_or(path);
    let file_digest = digest(bytes);
    sections(&all, name)
        .into_iter()
        .map(|sec| {
            let mut r = Map::new();
            r.insert("type".into(), json!("section"));
            r.insert(
                "key".into(),
                json!(key(
                    origin.unwrap_or(INSTRUCTIONS),
                    path,
                    &sec.heading,
                    sec.ordinal
                )),
            );
            r.insert("observed_at".into(), json!(now));
            if let Some(o) = origin {
                r.insert("repository".into(), json!(o));
            }
            r.insert("path".into(), json!(path));
            r.insert("heading".into(), json!(sec.heading));
            r.insert("first_line".into(), json!(sec.first_line));
            r.insert("last_line".into(), json!(sec.last_line));
            if let Some(c) = commit {
                r.insert("commit".into(), json!(c));
            }
            r.insert("digest".into(), json!(file_digest));
            r.insert(
                "text".into(),
                json!(text_of(&all[sec.first_line - 1..sec.last_line])),
            );
            Value::Object(r)
        })
        .collect()
}

/// One entry of `git ls-tree -r -l -z`: mode, type, object, size, path.
pub(crate) type Entry = (String, String, String, u64, String);
/// The entries of `git ls-tree -r -l -z`.
pub(crate) fn tree_entries(listing: &[u8]) -> Vec<Entry> {
    listing
        .split(|b| *b == 0)
        .filter_map(|e| {
            let tab = e.iter().position(|b| *b == b'\t')?;
            let meta = std::str::from_utf8(&e[..tab]).ok()?;
            let path = std::str::from_utf8(&e[tab + 1..]).ok()?;
            let [mode, kind, object, size] = meta.split_whitespace().collect::<Vec<_>>()[..] else {
                return None;
            };
            Some((
                mode.to_owned(),
                kind.to_owned(),
                object.to_owned(),
                size.parse().unwrap_or(u64::MAX),
                path.to_owned(),
            ))
        })
        .collect()
}

/// The components of a repository's `map.json` (R7).
fn component_records(
    repository: &str,
    bytes: &[u8],
    commit: &str,
    now: u64,
) -> Result<Vec<Value>, &'static str> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "map_unreadable")?;
    let all = v["components"].as_array().ok_or("map_unreadable")?;
    let mut names = BTreeSet::new();
    let mut out = vec![];
    for c in all {
        let name = c["name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .ok_or("map_unreadable")?;
        if !names.insert(name.to_owned()) {
            return Err("map_unreadable");
        }
        let purpose = match &c["purpose"] {
            Value::Null => "",
            Value::String(p) => p.as_str(),
            _ => return Err("map_unreadable"),
        };
        let list = |member: &str| -> Result<Value, &'static str> {
            match &c[member] {
                Value::Null => Ok(json!([])),
                Value::Array(a) if a.iter().all(Value::is_string) => Ok(json!(a)),
                _ => Err("map_unreadable"),
            }
        };
        out.push(json!({"type": "component", "key": key(repository, "map.json", name, 1),
            "observed_at": now, "repository": repository, "name": name, "purpose": cut(purpose, TEXT_BYTES),
            "paths": list("paths")?, "entry": list("entry")?, "spec": list("spec")?, "tests": list("tests")?,
            "commit": commit}));
    }
    Ok(out)
}

/// `git fetch origin main` in the clone, the commit `origin/main` then names and the entries of its tree (R4,
/// `specs/ap/clause.md` R2); the reason when the repository keeps its previous files.
pub(crate) fn fetched_tree(
    s: &mut impl System,
    clone: &str,
) -> Result<(String, Vec<Entry>), &'static str> {
    git(s, clone, &["fetch", "--quiet", "origin", "main"], FETCH_MS).map_err(|_| "fetch_failed")?;
    let revision = git(
        s,
        clone,
        &["rev-parse", "--verify", "--quiet", "origin/main^{commit}"],
        GIT_MS,
    )
    .ok()
    .and_then(|o| hex40(&o))
    .ok_or("revision_unreadable")?;
    let listing = git(
        s,
        clone,
        &["ls-tree", "-r", "-l", "-z", "--full-tree", &revision],
        GIT_MS,
    )
    .map_err(|_| "listing_failed")?;
    Ok((revision, tree_entries(&listing)))
}

/// The bytes of one object of the clone (R4).
pub(crate) fn blob(s: &mut impl System, clone: &str, object: &str) -> Result<Vec<u8>, &'static str> {
    git(s, clone, &["cat-file", "blob", object], GIT_MS).map_err(|_| "read_failed")
}

/// Whether an entry of the tree is a regular file (mode `100644` or `100755`) (R5).
pub(crate) fn regular(mode: &str, kind: &str) -> bool {
    kind == "blob" && (mode == "100644" || mode == "100755")
}

/// The records of one repository at its `origin/main` after `git fetch origin main` (R4, R5, R7); the
/// reason when the repository keeps its previous files.
fn repository_records(
    s: &mut impl System,
    repository: &str,
    clone: &str,
    now: u64,
) -> Result<(Vec<Value>, Vec<Value>), &'static str> {
    let (revision, entries) = fetched_tree(s, clone)?;
    let mut sections = vec![];
    let mut components = vec![];
    for (mode, kind, object, size, path) in entries {
        let regular = regular(&mode, &kind);
        if regular && path == "map.json" {
            let bytes = blob(s, clone, &object)?;
            let commit = last_commit(s, clone, &revision, &path)?;
            components = component_records(repository, &bytes, &commit, now)?;
        }
        if !regular || !path.ends_with(".md") || size > FILE_BYTES || path.len() > PATH_BYTES {
            continue;
        }
        let bytes = blob(s, clone, &object)?;
        let commit = last_commit(s, clone, &revision, &path)?;
        sections.extend(section_records(
            Some(repository),
            &path,
            &bytes,
            Some(&commit),
            now,
        ));
    }
    Ok((sections, components))
}

/// Every `*.md` regular file under `dir`, not through a link and not under a name that begins with `.`.
fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let meta = std::fs::symlink_metadata(e.path())?;
        if meta.is_dir() {
            markdown_files(&e.path(), out)?;
        } else if meta.is_file() && name.ends_with(".md") && meta.len() <= FILE_BYTES {
            out.push(e.path());
        }
    }
    Ok(())
}

/// The records of the shared instructions, read on disk (R6).
fn instruction_records(
    s: &mut impl System,
    dir: &str,
    now: u64,
) -> Result<Vec<Value>, &'static str> {
    let root = dir.trim_end_matches('/');
    let root = if root.is_empty() { "/" } else { root };
    if !std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()) {
        return Err("instructions_unreadable");
    }
    let mut files = vec![];
    markdown_files(Path::new(root), &mut files).map_err(|_| "instructions_unreadable")?;
    let mut out = vec![];
    for f in files {
        let Some(path) = f.to_str().filter(|p| p.len() <= PATH_BYTES) else {
            continue;
        };
        let bytes = s.read(path).map_err(|_| "instructions_unreadable")?;
        out.extend(section_records(None, path, &bytes, None, now));
    }
    Ok(out)
}

/// The mode of the directory and of the files of the sources in it (R8, `specs/ap/clause.md` R5).
const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

/// The directory a command writes (R8, `specs/ap/clause.md` R5): made with mode 0700 when it is absent, set to
/// 0700 when it has another mode, refused when it is not a directory (a symbolic link included).
pub(crate) fn directory(dir: &Path) -> Result<(), Failure> {
    let io = || error("io_error", "directory");
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => {
            if m.permissions().mode() & 0o7777 != DIRECTORY_MODE {
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(DIRECTORY_MODE))
                    .map_err(|_| io())?;
            }
            Ok(())
        }
        Ok(_) => Err(invalid("directory", "is not a directory")),
        Err(_) => {
            std::fs::DirBuilder::new()
                .mode(DIRECTORY_MODE)
                .create(dir)
                .map_err(|_| io())?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(DIRECTORY_MODE))
                .map_err(|_| io())
        }
    }
}

/// One file of the directory written whole under a temporary name and renamed, mode 0600 (R8).
fn write_whole(dir: &Path, name: &str, records: &[Value]) -> Result<(), Failure> {
    write_file(dir, TEMPORARY_PREFIX, name, records)
}

/// One file of a command's directory written whole under the temporary name `<prefix><pid>.<name>.tmp` and
/// renamed, mode 0600 (R8, `specs/ap/clause.md` R5).
pub(crate) fn write_file(
    dir: &Path,
    prefix: &str,
    name: &str,
    records: &[Value],
) -> Result<(), Failure> {
    let io = || error("io_error", "directory");
    let tmp = dir.join(format!(
        "{prefix}{}.{name}{TEMPORARY_SUFFIX}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tmp);
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(FILE_MODE)
            .open(&tmp)?;
        let mut text = String::new();
        for r in records {
            text.push_str(&r.to_string());
            text.push('\n');
        }
        f.write_all(text.as_bytes())?;
        f.set_permissions(std::fs::Permissions::from_mode(FILE_MODE))?;
        f.sync_all()?;
        std::fs::rename(&tmp, dir.join(name))
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.map_err(|_| io())
}

/// The source a file of the directory belongs to, and its stem: `<stem>.sections.jsonl` or
/// `<stem>.components.jsonl`.
pub fn source_of(name: &str) -> Option<(&'static str, &str)> {
    if let Some(stem) = name.strip_suffix(".sections.jsonl") {
        return Some(("sections", stem));
    }
    name.strip_suffix(".components.jsonl")
        .map(|stem| ("components", stem))
}

/// The source of a file of a command's directory and its stem, when the file is one of the command's sources.
pub(crate) type Owned = fn(&str) -> Option<(&'static str, &str)>;

/// `--prune` (R8): the regular files of the directory of a source whose stem is not kept, and `ap`'s own
/// temporary files older than an hour, removed; nothing else. The names removed.
fn prune(dir: &Path, keep: &BTreeSet<String>) -> Result<Vec<String>, Failure> {
    prune_files(dir, keep, TEMPORARY_PREFIX, source_of)
}

/// `--prune` of a command whose files `owned` names and whose temporary files begin with `prefix` (R8,
/// `specs/ap/clause.md` R5).
pub(crate) fn prune_files(
    dir: &Path,
    keep: &BTreeSet<String>,
    prefix: &str,
    owned: Owned,
) -> Result<Vec<String>, Failure> {
    let io = || error("io_error", "directory");
    let mut removed = vec![];
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|_| io())?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let stale = name.starts_with(prefix)
            && name.ends_with(TEMPORARY_SUFFIX)
            && meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .is_some_and(|a| a > TEMPORARY_AGE);
        let other = owned(&name).is_some_and(|(_, stem)| !keep.contains(stem));
        if stale || other {
            std::fs::remove_file(e.path()).map_err(|_| io())?;
            removed.push(name);
        }
    }
    Ok(removed)
}

/// Every regular file of the command's sources directly in the directory, a kept one included, set to mode 0600
/// (R8, `specs/ap/clause.md` R5).
pub(crate) fn secure(dir: &Path, owned: Owned) -> Result<(), Failure> {
    let io = || error("io_error", "directory");
    for e in std::fs::read_dir(dir).map_err(|_| io())?.flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        if owned(&name).is_none() {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if meta.is_file() && meta.permissions().mode() & 0o7777 != FILE_MODE {
            std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(FILE_MODE))
                .map_err(|_| io())?;
        }
    }
    Ok(())
}

/// The records each source's files hold in the directory after the run.
pub(crate) fn counted(dir: &Path, owned: Owned, names: &[&str]) -> Map<String, Value> {
    let mut counts = Map::new();
    for name in names {
        counts.insert((*name).into(), json!(0));
    }
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let Some((source, _)) = owned(&name) else {
            continue;
        };
        if !std::fs::symlink_metadata(e.path()).is_ok_and(|m| m.is_file()) {
            continue;
        }
        let n = std::fs::read(e.path()).map_or(0, |b| {
            b.split(|c| *c == b'\n').filter(|l| !l.is_empty()).count()
        });
        counts[source] = json!(counts[source].as_u64().unwrap_or(0) + n as u64);
    }
    counts
}

/// One run of `ap route index` (R4 to R14): the answer line and whether the index of both sources held.
pub fn index(s: &mut impl System, o: &Options) -> Result<(Value, bool), Failure> {
    let mut clones = vec![];
    for r in &o.repositories {
        clones.push(crate::unit::object::clone_of(s, r)?);
    }
    let dir = PathBuf::from(o.directory.as_deref().unwrap_or(DIRECTORY));
    directory(&dir)?;
    let now = seconds(s);
    let mut kept = vec![];
    let mut keep = BTreeSet::new();
    for (repository, clone) in o.repositories.iter().zip(&clones) {
        let stem = stem_of(repository);
        keep.insert(stem.clone());
        match repository_records(s, repository, clone, now) {
            Ok((sections, components)) => {
                write_whole(&dir, &format!("{stem}.sections.jsonl"), &sections)?;
                write_whole(&dir, &format!("{stem}.components.jsonl"), &components)?;
            }
            Err(reason) => {
                let previous = source_of_files(&dir, &stem);
                kept.push(
                    json!({"repository": repository, "reason": reason, "previous": previous}),
                );
            }
        }
    }
    if let Some(i) = &o.instructions {
        keep.insert(INSTRUCTIONS.to_owned());
        match instruction_records(s, i, now) {
            Ok(sections) => {
                write_whole(&dir, &format!("{INSTRUCTIONS}.sections.jsonl"), &sections)?
            }
            Err(reason) => {
                let previous = source_of_files(&dir, INSTRUCTIONS);
                kept.push(json!({"repository": null, "instructions": i, "reason": reason, "previous": previous}));
            }
        }
    }
    let removed = if o.prune { prune(&dir, &keep)? } else { vec![] };
    secure(&dir, source_of)?;
    let names = ["sections", "components"];
    let answer = store::index(s, o.store.as_deref(), &names, INDEX_MS)?;
    let (sources, held) = summary(&answer, &counted(&dir, source_of, &names), &names);
    let directory = dir.to_string_lossy();
    Ok((
        json!({"status": if held { "ok" } else { "index_failed" }, "directory": directory,
            "sources": sources, "kept": kept, "removed": removed}),
        held,
    ))
}

/// The run's time in seconds since 1970.
pub(crate) fn seconds(s: &impl System) -> u64 {
    s.wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Per source of `names`, the records its files hold and the index answer's `status`, `code`, `appended`,
/// `renewed` and `retracted` (R13, `specs/ap/clause.md` R6); whether every source held.
pub(crate) fn summary(
    answer: &Value,
    counts: &Map<String, Value>,
    names: &[&str],
) -> (Map<String, Value>, bool) {
    let mut sources = Map::new();
    let mut held = answer["status"] == "ok";
    for name in names {
        let name = *name;
        let Some(one) = answer["result"]["sources"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["source"] == name))
        else {
            held = false;
            sources.insert(
                name.into(),
                json!({"records": counts[name], "status": null}),
            );
            continue;
        };
        held &= one["status"] == "ok";
        sources.insert(
            name.into(),
            json!({"records": counts[name], "status": one["status"],
            "code": one.get("code"), "appended": one["appended"], "renewed": one["renewed"],
            "retracted": one["appended"]["retraction"]}),
        );
    }
    (sources, held)
}

/// Whether the directory holds a file of the stem from an earlier run.
fn source_of_files(dir: &Path, stem: &str) -> bool {
    ["sections", "components"].iter().any(|k| {
        std::fs::symlink_metadata(dir.join(format!("{stem}.{k}.jsonl"))).is_ok_and(|m| m.is_file())
    })
}

pub(crate) fn print(value: &Value, out: bool, status: u8) -> ExitCode {
    let written = if out {
        writeln!(std::io::stdout(), "{value}")
    } else {
        writeln!(std::io::stderr(), "{value}")
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}

pub(crate) fn refusal(e: &Failure) -> Value {
    json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}})
}

/// `ap route <arguments>` (R1, R14): `index` and its options.
pub fn cli(argv: &[OsString]) -> ExitCode {
    let Some((_, rest)) = argv.split_first().filter(|(f, _)| *f == "index") else {
        return print(&refusal(&error("usage", "args")), false, 2);
    };
    let o = match options(rest) {
        Ok(o) => o,
        Err(e) => return print(&refusal(&e), false, 2),
    };
    ended(index(&mut Host, &o))
}

/// The end of an index run (R13, `specs/ap/clause.md` R6): the answer line, exit 0 when every source held, else 3;
/// a refusal of R1 to R3 exit 2, a failure of its own exit 3.
pub(crate) fn ended(run: Result<(Value, bool), Failure>) -> ExitCode {
    match run {
        Ok((v, true)) => print(&v, true, 0),
        Ok((v, false)) => print(&v, true, 3),
        Err(e)
            if matches!(
                e.code,
                "invalid_input" | "usage" | "policy_invalid" | "repository_not_mapped"
            ) =>
        {
            print(&refusal(&e), false, 2)
        }
        Err(e) => print(&refusal(&e), false, 3),
    }
}

/// One candidate answer of the function.
struct Candidate {
    kind: &'static str,
    repository: Value,
    path: String,
    first_line: Value,
    last_line: Value,
    heading: String,
    commit: Value,
    text: String,
}

fn candidate(kind: &'static str, fields: &Value) -> Option<Candidate> {
    let text = |k: &str| fields[k].as_str().map(str::to_owned);
    Some(if kind == "section" {
        Candidate {
            kind,
            repository: fields["repository"].clone(),
            path: text("path")?,
            first_line: json!(fields["first_line"].as_u64()?),
            last_line: json!(fields["last_line"].as_u64()?),
            heading: text("heading")?,
            commit: fields["commit"].clone(),
            text: text("text").unwrap_or_default(),
        }
    } else {
        Candidate {
            kind,
            repository: fields["repository"].clone(),
            path: "map.json".into(),
            first_line: Value::Null,
            last_line: Value::Null,
            heading: text("name")?,
            commit: fields["commit"].clone(),
            text: text("purpose").unwrap_or_default(),
        }
    })
}

/// The parameters of the function (R16): `words` and `limit`.
fn parameters(p: &Value) -> Result<(String, usize), Failure> {
    let o = p
        .as_object()
        .ok_or_else(|| invalid("parameters", "is not an object"))?;
    if let Some(k) = o.keys().find(|k| *k != "words" && *k != "limit") {
        return Err(invalid(k, "is not a parameter of route"));
    }
    let words = o
        .get("words")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("words", "is not a string"))?;
    let limit = match o.get("limit") {
        None | Some(Value::Null) => LIMIT_DEFAULT,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=LIMIT_MAX).contains(n))
            .ok_or_else(|| invalid("limit", "is not a whole number from 1 to 20"))?,
    };
    Ok((words.to_owned(), limit as usize))
}

/// The function's answer (R15 to R18) from the current objects of `section` and `component` of `store`.
pub fn answer(s: &mut impl System, store: &str, p: &Value) -> Result<Value, Failure> {
    let (question, limit) = parameters(p)?;
    let mut all = vec![];
    let mut qualified = true;
    for kind in ["section", "component"] {
        let (found, q) = store::search_every(s, Some(store), kind, PAGE)?;
        qualified &= q;
        for (_, fields) in found {
            all.extend(candidate(kind, &fields));
        }
    }
    let matched: Vec<Matched> = all
        .iter()
        .map(|c| Matched {
            heading: &c.heading,
            path: if c.kind == "section" { &c.path } else { "" },
            text: &c.text,
        })
        .collect();
    let scores = score(&matched, &question);
    let mut order: Vec<usize> = (0..all.len()).filter(|i| scores[*i].0 > 0.0).collect();
    let line = |c: &Candidate| c.first_line.as_u64().unwrap_or(0);
    order.sort_by(|a, b| {
        let (x, y) = (&all[*a], &all[*b]);
        scores[*b]
            .0
            .total_cmp(&scores[*a].0)
            .then_with(|| x.path.cmp(&y.path))
            .then_with(|| line(x).cmp(&line(y)))
            .then_with(|| x.repository.as_str().cmp(&y.repository.as_str()))
    });
    let mut chosen: Vec<usize> = vec![];
    for i in order {
        if chosen.len() == limit {
            break;
        }
        let c = &all[i];
        let twin = chosen.iter().position(|k| {
            let o = &all[*k];
            o.kind == "section" && c.kind == "section" && !c.text.is_empty() && o.text == c.text
        });
        if let Some(t) = twin {
            // The installed copy of the shared instructions is kept over the content/ copy it came from.
            let o = &all[chosen[t]];
            if c.repository.is_null() && !o.repository.is_null() && o.path.starts_with("content/") {
                chosen[t] = i;
            }
            continue;
        }
        let inside = chosen.iter().any(|k| {
            let o = &all[*k];
            o.kind == "section"
                && c.kind == "section"
                && o.repository == c.repository
                && o.path == c.path
                && line(o) <= line(c)
                && c.last_line.as_u64() <= o.last_line.as_u64()
        });
        if !inside {
            chosen.push(i);
        }
    }
    let mut answers: Vec<Value> = chosen
        .iter()
        .map(|i| {
            let c = &all[*i];
            json!({"kind": c.kind, "repository": c.repository, "path": c.path, "first_line": c.first_line,
                "last_line": c.last_line, "heading": c.heading, "commit": c.commit, "matched": scores[*i].1,
                "score": (scores[*i].0 * 1000.0).round() / 1000.0})
        })
        .collect();
    let shaped = |answers: &[Value]| {
        let mut v = json!({ "answers": answers });
        if !qualified {
            v["stale"] = json!(true);
        }
        v
    };
    while answers.len() > 1 && shaped(&answers).to_string().len() > ANSWER_BYTES {
        answers.pop();
    }
    Ok(shaped(&answers))
}

/// `ap` with no argument and `ONT_FN_NAME=route` (R15): the parameters on standard input, the answer
/// on standard output; a refusal on standard error with exit 2, a failure of the store with exit 3.
pub fn function() -> ExitCode {
    function_of(answer)
}

/// A function `ap` answers (R15, `specs/ap/clause.md` R7): the parameters, one JSON value of standard input, and
/// the store at `ONT_FN_STORE` given to `answer`.
pub(crate) fn function_of(
    answer: impl FnOnce(&mut Host, &str, &Value) -> Result<Value, Failure>,
) -> ExitCode {
    let result = (|| {
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error("io_error", "stdin"))?;
        if bytes.len() as u64 > LIMIT {
            return Err(error("input_too_large", "stdin"));
        }
        let p: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("stdin", "is not JSON"))?;
        let store = std::env::var("ONT_FN_STORE")
            .ok()
            .filter(|v| absolute(v))
            .ok_or_else(|| invalid("ONT_FN_STORE", "is not an absolute path"))?;
        answer(&mut Host, &store, &p)
    })();
    match result {
        Ok(v) => print(&v, true, 0),
        Err(e) if e.code == "invalid_input" || e.code == "input_too_large" => {
            print(&refusal(&e), false, 2)
        }
        Err(e) => print(&refusal(&e), false, 3),
    }
}
