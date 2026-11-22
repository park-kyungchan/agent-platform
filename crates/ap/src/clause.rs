//! `ap clause index` and the function `clause` (`specs/ap/clause.md`): each numbered sentence (`- R<n>:`) of a
//! `specs/**/*.md` file at `origin/main` of the repositories named, with the tests that cite it, written as lines
//! of the store's record source `clauses`, and the answer `ont fn clause` gives from the current objects: the
//! sentence, its tests and the units that name it. The options, the Git reads, the directory, its files and the
//! function's harness are those of `ap route index` (`crate::route`), shared.
use crate::host::{Failure, Host, System, digest, error};
use crate::route::{self, Options};
use crate::store;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

/// The directory the host store's source `clauses` reads (R1).
pub const DIRECTORY: &str = "/var/lib/agent-platform/clause";
/// The record source of the clauses.
pub const SOURCE: &str = "clauses";
/// A file larger than this is not indexed, and not read for citations (R3, R4).
pub const FILE_BYTES: u64 = 1_000_000;
/// The citations a clause shows (R4).
pub const TESTS_SHOWN: usize = 100;
/// The units the function shows: one head is at most 65,536 bytes, an answer of `ont` at most 1 MiB (R9).
pub const UNITS_SHOWN: u64 = 12;
const TEMPORARY_PREFIX: &str = ".ap-clause.";
const INDEX_MS: u64 = 600_000;
/// Objects asked per search page: one clause is at most about 10 KB.
const PAGE: u64 = 50;

/// The arguments after `index` (R1): those of `ap route index` without `--instructions`.
pub fn options(argv: &[OsString]) -> Result<Options, Failure> {
    route::options_of(argv, DIRECTORY, false)
}

/// The source of a file of the directory and its stem: `<stem>.clauses.jsonl`.
pub fn source_of(name: &str) -> Option<(&'static str, &str)> {
    name.strip_suffix(".clauses.jsonl").map(|stem| (SOURCE, stem))
}

/// The rule of a requirement line, `R<digits>`: the line begins with `- R`, digits and `:` (R3).
pub fn rule_of(line: &str) -> Option<String> {
    let rest = line.strip_prefix("- R")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && rest[digits..].starts_with(':')).then(|| format!("R{}", &rest[..digits]))
}

/// One clause of a file: its rule and its lines, the first and the last, counted from 1 (R3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub rule: String,
    pub first_line: usize,
    pub last_line: usize,
}

/// The clauses of a file's lines, and how many later lines of a rule already seen were not indexed (R3).
pub fn clauses(lines: &[&str]) -> (Vec<Clause>, usize) {
    let ends = |l: &str| l.trim().is_empty() || rule_of(l).is_some() || route::atx_heading(l);
    let mut seen = BTreeSet::new();
    let (mut out, mut duplicates) = (vec![], 0);
    for (i, line) in lines.iter().enumerate() {
        let Some(rule) = rule_of(line) else { continue };
        if !seen.insert(rule.clone()) {
            duplicates += 1;
            continue;
        }
        let next = lines[i + 1..].iter().position(|l| ends(l));
        let last = next.map_or(lines.len(), |n| i + 1 + n);
        out.push(Clause {
            rule,
            first_line: i + 1,
            last_line: last,
        });
    }
    (out, duplicates)
}

/// The citations of a line: `spec: `, a path `specs/….md`, `#R` and digits (`CITE_RE` of
/// `ci/check-spec-trace.py`), each as its path and rule (R4).
pub fn citations(line: &str) -> Vec<(String, String)> {
    const WORD: &str = "spec: ";
    let mut out = vec![];
    let mut from = 0;
    while let Some(at) = line[from..].find(WORD) {
        let start = from + at + WORD.len();
        from = from + at + 1;
        let rest = &line[start..];
        let run = rest
            .find(|c: char| c.is_whitespace() || c == '#')
            .unwrap_or(rest.len());
        let path = &rest[..run];
        // `specs/`, at least one character, `.md`.
        if !path.starts_with("specs/") || !path.ends_with(".md") || path.len() < "specs/x.md".len() {
            continue;
        }
        let Some(after) = rest[run..].strip_prefix("#R") else {
            continue;
        };
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        out.push((path.to_owned(), format!("R{}", &after[..digits])));
        from = start + run + 2 + digits;
    }
    out
}

/// The key of a clause (R3): `<owner>/<name>:<path>#R<n>`.
pub fn key(repository: &str, path: &str, rule: &str) -> String {
    format!("{repository}:{path}#{rule}")
}

/// The parts of a key of that form: repository, path and rule; none for another form (R9).
pub fn parts(key: &str) -> Option<(&str, &str, &str)> {
    let (repository, rest) = key.split_once(':')?;
    let (path, rule) = rest.rsplit_once('#')?;
    let ok = route::repository_form(repository)
        && !path.is_empty()
        && rule.strip_prefix('R').is_some_and(|n| {
            !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
        });
    ok.then_some((repository, path, rule))
}

/// The records of one repository at its `origin/main` and its duplicates (R2 to R4); the reason when the
/// repository keeps its previous file.
fn repository_records(
    s: &mut impl System,
    repository: &str,
    clone: &str,
    now: u64,
) -> Result<(Vec<Value>, usize), &'static str> {
    let (revision, entries) = route::fetched_tree(s, clone)?;
    let mut specs = vec![];
    let mut cited: BTreeMap<(String, String), Vec<(String, usize)>> = BTreeMap::new();
    for (mode, kind, object, size, path) in entries {
        let markdown = path.ends_with(".md");
        let spec = markdown && path.starts_with("specs/");
        if !route::regular(&mode, &kind) || size > FILE_BYTES || (markdown && !spec) {
            continue;
        }
        let bytes = route::blob(s, clone, &object)?;
        if spec {
            specs.push((path, bytes));
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (n, line) in text.split('\n').enumerate() {
            for (spec, rule) in citations(line) {
                cited.entry((spec, rule)).or_default().push((path.clone(), n + 1));
            }
        }
    }
    let mut records = vec![];
    let mut duplicates = 0;
    for (path, bytes) in specs {
        let text = String::from_utf8_lossy(&bytes);
        let all = route::lines(&text);
        let (found, twice) = clauses(&all);
        duplicates += twice;
        if found.is_empty() {
            continue;
        }
        let commit = route::last_commit(s, clone, &revision, &path)?;
        for c in found {
            let lines = &all[c.first_line - 1..c.last_line];
            let mut tests = cited
                .get(&(path.clone(), c.rule.clone()))
                .cloned()
                .unwrap_or_default();
            tests.sort();
            let total = tests.len();
            let shown: Vec<String> = (tests.iter().take(TESTS_SHOWN))
                .map(|(p, n)| format!("{p}:{n}"))
                .collect();
            records.push(json!({"type": "clause", "key": key(repository, &path, &c.rule),
                "observed_at": now, "repository": repository, "path": path, "rule": c.rule,
                "commit": commit, "line": c.first_line, "digest": digest(lines.join("\n").as_bytes()),
                "text": route::text_of(lines), "tests": shown, "tests_total": total}));
        }
    }
    Ok((records, duplicates))
}

/// One run of `ap clause index` (R2 to R6, R12): the answer line and whether the index of the source held.
pub fn index(s: &mut impl System, o: &Options) -> Result<(Value, bool), Failure> {
    let mut clones = vec![];
    for r in &o.repositories {
        clones.push(crate::unit::object::clone_of(s, r)?);
    }
    let dir = PathBuf::from(o.directory.as_deref().unwrap_or(DIRECTORY));
    route::directory(&dir)?;
    let now = route::seconds(s);
    let (mut kept, mut keep, mut duplicates) = (vec![], BTreeSet::new(), 0);
    for (repository, clone) in o.repositories.iter().zip(&clones) {
        let stem = route::stem_of(repository);
        keep.insert(stem.clone());
        let name = format!("{stem}.clauses.jsonl");
        match repository_records(s, repository, clone, now) {
            Ok((records, twice)) => {
                route::write_file(&dir, TEMPORARY_PREFIX, &name, &records)?;
                duplicates += twice;
            }
            Err(reason) => {
                let previous = file(&dir.join(&name));
                kept.push(
                    json!({"repository": repository, "reason": reason, "previous": previous}),
                );
            }
        }
    }
    let removed = match o.prune {
        true => route::prune_files(&dir, &keep, TEMPORARY_PREFIX, source_of)?,
        false => vec![],
    };
    route::secure(&dir, source_of)?;
    let answer = store::index(s, o.store.as_deref(), &[SOURCE], INDEX_MS)?;
    let counts = route::counted(&dir, source_of, &[SOURCE]);
    let (sources, held) = route::summary(&answer, &counts, &[SOURCE]);
    Ok((
        json!({"status": if held { "ok" } else { "index_failed" }, "directory": dir.to_string_lossy(),
            "sources": sources, "duplicates": duplicates, "kept": kept, "removed": removed}),
        held,
    ))
}

/// Whether `path` is a regular file (not through a link).
fn file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// `ap clause <arguments>` (R1): `index` and its options.
pub fn cli(argv: &[OsString]) -> ExitCode {
    let Some((_, rest)) = argv.split_first().filter(|(f, _)| *f == "index") else {
        return route::print(&route::refusal(&error("usage", "args")), false, 2);
    };
    match options(rest) {
        Ok(o) => route::ended(index(&mut Host, &o)),
        Err(e) => route::print(&route::refusal(&e), false, 2),
    }
}

/// The parameter of the function (R8): `key`, a string of 1 to 512 bytes.
fn parameter(p: &Value) -> Result<&str, Failure> {
    let o = p
        .as_object()
        .ok_or_else(|| route::invalid("parameters", "is not an object"))?;
    if let Some(k) = o.keys().find(|k| *k != "key") {
        return Err(route::invalid(k, "is not a parameter of clause"));
    }
    (o.get("key").and_then(Value::as_str))
        .filter(|k| (1..=512).contains(&k.len()))
        .ok_or_else(|| route::invalid("key", "is not a string of 1 to 512 bytes"))
}

/// The function's answer (R7 to R9) from the current objects of `store`: the clause of `key`, its tests, and the
/// units of every state that name it.
pub fn answer(s: &mut impl System, store: &str, p: &Value) -> Result<Value, Failure> {
    let key = parameter(p)?;
    let none = json!({"clause": null, "units": [], "units_total": 0});
    let Some((repository, path, rule)) = parts(key) else {
        return Ok(none);
    };
    let filter = json!({"all": [{"eq": ["repository", repository]}, {"eq": ["path", path]},
        {"eq": ["rule", rule]}]});
    let (found, qualified) =
        store::search_every_where(s, Some(store), "clause", Some(filter.clone()), PAGE)?;
    let mut v = match found.into_iter().next() {
        None => none,
        Some((native, f)) => {
            let clause = json!({"key": native, "repository": f["repository"], "path": f["path"],
                "rule": f["rule"], "line": f["line"], "commit": f["commit"], "digest": f["digest"],
                "text": f["text"], "tests": f["tests"], "tests_total": f["tests_total"]});
            let (units, matched) = store::around(
                s,
                Some(store),
                ("clause", filter),
                &["clause_units"],
                UNITS_SHOWN,
            )?;
            let units: Vec<Value> = (units.iter())
                .map(|(k, u)| {
                    json!({"unit": k, "state": u["state"], "class": u["class"], "wave": u["wave"]})
                })
                .collect();
            json!({"clause": clause, "units": units, "units_total": matched})
        }
    };
    if !qualified {
        v["stale"] = json!(true);
    }
    Ok(v)
}

/// `ap` with no argument and `ONT_FN_NAME=clause` (R7).
pub fn function() -> ExitCode {
    route::function_of(answer)
}
