//! `ap unit add|amend|ready|drop|page`: a unit's page as an object of the store, made ready only after
//! its acceptance checks ended on the base as their kinds require (`specs/ap/unit.md`).
use super::drive::{protected, protected_tree};
use super::render::{self, Role};
use crate::host::{Failure, Host, LIMIT, System, error, fail, id, new_session_id};
use crate::review::{Reading, WRITE_SET_FORMS, outside, write_set_pattern};
use crate::store::{self, Actor, at, bad, shaped, sized};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
};

/// The first arguments that select this module; a page's path is absolute, so none is one.
pub const WORDS: [&str; 5] = ["add", "amend", "ready", "drop", "page"];
/// The bound of a whole action request: the engine takes 65,536 bytes.
const PAGE_LIMIT: usize = 60_000;
/// The texts of the page with the schema's bounds.
const TEXTS: [(&str, usize); 9] = [
    ("objective", 2000),
    ("interface", 20_000),
    ("invariants", 3000),
    ("not_built", 1500),
    ("notes", 3000),
    ("verifier_questions", 4000),
    ("report", 1000),
    ("repository", 200),
    ("branch", 200),
];
/// The page properties `amend` takes; `add` takes `lane`, `intent` and `origin` too.
const PAGE: &str = "class objective interface invariants acceptance not_built write_set read_first specs notes verifier_questions report repository branch implementer verifier estimate_lines depends_on wave";
/// The bounds of `wave` (R41).
const WAVES: std::ops::RangeInclusive<u64> = 1..=99;
/// Runs the command after the output file with both outputs into that file and standard input from
/// `/dev/null`, and prints its status as the last line on the standard output it was given. The
/// command does not hold that output: a background process of a check cannot keep the call open or
/// print a status of its own; an output file that cannot be opened prints no status.
const WRAPPER: &str =
    "o=$1; shift; exec 3>&1 >\"$o\" 2>&1 </dev/null || exit 0; \"$@\" 3>&-; echo \"$?\" >&3";
/// The bound of one check, and of the call that runs it.
const CHECK_SECONDS: &str = "300";
const CALL_MS: u64 = 330_000;
const FETCH_MS: u64 = 120_000;
const OUTPUT_TAIL: usize = 2000;
/// The states `systemctl is-active` answers while something of a unit may live.
const ALIVE: [&str; 5] = [
    "active",
    "activating",
    "deactivating",
    "reloading",
    "refreshing",
];

/// A refusal: `after` the op's suffix once its action applied, `unit` the key the request names,
/// `fields` every member that breaks a rule of R2 to R16 when the refusal is one of these (R37).
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<String>,
    pub unit: Option<String>,
    pub fields: Option<Value>,
}
/// One call by its arguments: the op, `--store`, and for `page` the key, `--for` and `--digest`.
#[derive(Debug)]
pub struct Call {
    pub op: &'static str,
    pub store: Option<String>,
    pub key: Option<String>,
    pub role: Option<Role>,
    pub digest: bool,
}
/// The arguments after `unit` (R1): any other form is `usage`.
pub fn arguments(argv: &[OsString]) -> Result<Call, Failure> {
    let usage = || error("usage", "args");
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    let words = words.ok_or_else(usage)?;
    let (first, rest) = words.split_first().ok_or_else(usage)?;
    let op = *WORDS.iter().find(|w| *w == first).ok_or_else(usage)?;
    let mut call = Call {
        op,
        store: None,
        key: None,
        role: None,
        digest: false,
    };
    let mut rest = rest.iter();
    if op == "page" {
        let key = rest
            .next()
            .filter(|k| (1..=512).contains(&k.len()) && !k.starts_with('-'));
        call.key = Some(key.ok_or_else(usage)?.to_string());
    }
    while let Some(arg) = rest.next() {
        match *arg {
            "--store" if call.store.is_none() => {
                let path = rest.next().filter(|p| p.starts_with('/'));
                call.store = Some(path.ok_or_else(usage)?.to_string());
            }
            "--for" if op == "page" && call.role.is_none() && !call.digest => {
                let role = rest.next().and_then(|r| Role::named(r));
                call.role = Some(role.ok_or_else(usage)?);
            }
            "--digest" if op == "page" && call.role.is_none() && !call.digest => call.digest = true,
            _ => return Err(usage()),
        }
    }
    Ok(call)
}
/// The call: what it prints on standard output, or the refusal with how far it went.
pub fn execute(s: &mut impl System, call: &Call, r: &Value) -> Result<String, Box<Refusal>> {
    let store = call.store.as_deref();
    let (mut after, mut fields) = (None, None);
    let (out, unit) = match call.op {
        "page" => {
            let key = call.key.as_deref().unwrap_or_default();
            let page = render::page(s, store, key, call.role, call.digest);
            (page, call.key.clone())
        }
        op => {
            let answer = write(s, store, op, r, &mut after, &mut fields).map(|v| format!("{v}\n"));
            (answer, r["unit"].as_str().map(str::to_owned))
        }
    };
    out.map_err(|f| {
        Box::new(Refusal {
            code: store::code(&f),
            field: f.field,
            detail: f.detail,
            after,
            unit,
            fields,
        })
    })
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let refused = |f: Failure| {
        Box::new(Refusal {
            code: f.code.into(),
            field: f.field,
            detail: f.detail,
            after: None,
            unit: None,
            fields: None,
        })
    };
    let result = arguments(argv).map_err(refused).and_then(|call| {
        let r = match call.op {
            "page" => Value::Null,
            _ => request().map_err(refused)?,
        };
        execute(&mut Host, &call, &r)
    });
    let (written, status) = match result {
        Ok(out) => (std::io::stdout().write_all(out.as_bytes()), 0),
        Err(e) => {
            let mut v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail,
                "after": e.after, "unit": e.unit}});
            if let Some(fields) = e.fields {
                v["error"]["fields"] = fields;
            }
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// The request of standard input: one JSON value of at most 1 MiB.
pub(super) fn request() -> Result<Value, Failure> {
    let mut bytes = vec![];
    let read = std::io::stdin().take(LIMIT + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > LIMIT {
        return Err(error("input_too_large", "stdin"));
    }
    serde_json::from_slice(&bytes).map_err(|_| bad("stdin"))
}
/// A string of at most `max` bytes, never cut (an empty one is the store's to refuse); none when
/// absent and not `required`.
fn bounded<'a>(
    r: &'a Value,
    k: &str,
    max: usize,
    required: bool,
) -> Result<Option<&'a str>, Failure> {
    match r.get(k) {
        None if !required => Ok(None),
        Some(Value::String(t)) if t.len() <= max => Ok(Some(t)),
        Some(Value::String(t)) => Err(fail(
            "invalid_input",
            k,
            json!({"bytes": t.len(), "limit": max}),
        )),
        _ => Err(bad(k)),
    }
}
/// The key of a new unit (R6).
fn new_key(r: &Value) -> Result<&str, Failure> {
    let ok = |t: &&str| {
        (1..=59).contains(&t.len())
            && t.starts_with(|c: char| c.is_ascii_lowercase())
            && t.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    };
    r["unit"].as_str().filter(ok).ok_or_else(|| bad("unit"))
}
fn one_of(r: &Value, k: &str, values: &[&str]) -> Result<Value, Failure> {
    let v = r[k].as_str().filter(|v| values.contains(v));
    v.map(|v| json!(v)).ok_or_else(|| bad(k))
}
/// The rule a refused member breaks, with its bytes and its bound where the rule has them (R37).
type Rule = (&'static str, Option<usize>, Option<usize>);
const FORM: Rule = ("form", None, None);
const REQUIRED: Rule = ("required", None, None);
/// The most bytes of a branch (R10); R7's bound of 200 is the schema's.
const BRANCH_BYTES: usize = 100;
/// The rule of a refused text of at most `max` bytes: absent or empty `required`, a string over `max`
/// bytes `max_bytes`, else `form`.
fn text_rule(v: Option<&Value>, max: usize) -> Rule {
    match v {
        None => REQUIRED,
        Some(Value::String(t)) if t.is_empty() => REQUIRED,
        Some(Value::String(t)) if t.len() > max => ("max_bytes", Some(t.len()), Some(max)),
        _ => FORM,
    }
}
/// The rule of a refused choice among strings: absent or empty `required`, another string `one_of`,
/// else `form`.
fn choice_rule(v: Option<&Value>) -> Rule {
    match v {
        None => REQUIRED,
        Some(Value::String(t)) if t.is_empty() => REQUIRED,
        Some(Value::String(_)) => ("one_of", None, None),
        _ => FORM,
    }
}
/// Every member of a request that breaks a rule of R2 to R16, in the order the checks of the base
/// refuse them: the first one's failure as the base gives it, and one entry per member (R37).
#[derive(Default)]
struct Breaks {
    first: Option<Failure>,
    fields: Vec<Value>,
}
impl Breaks {
    /// The value of a check that passed; none for one refused, its member noted with `rule` unless
    /// it is already named.
    fn take<T>(&mut self, checked: Result<T, Failure>, rule: Rule) -> Option<T> {
        let f = match checked {
            Ok(v) => return Some(v),
            Err(f) => f,
        };
        if !self.fields.iter().any(|e| e["field"] == f.field) {
            let (rule, bytes, limit) = rule;
            self.fields
                .push(json!({"field": f.field, "rule": rule, "bytes": bytes, "limit": limit}));
            self.first.get_or_insert(f);
        }
        None
    }
    /// A check of the text `v` of at most `max` bytes, refused by the rule of `text_rule`.
    fn text<T>(&mut self, checked: Result<T, Failure>, v: Option<&Value>, max: usize) -> Option<T> {
        let rule = match checked {
            Ok(_) => FORM,
            Err(_) => text_rule(v, max),
        };
        self.take(checked, rule)
    }
    fn refuse(&mut self, field: &str, rule: Rule) {
        self.take::<()>(Err(bad(field)), rule);
    }
    /// A check of the choice `k` of `r` among `values` (R7, R14).
    fn choice(&mut self, r: &Value, k: &str, field: &str, values: &[&str]) -> Option<Value> {
        let checked = one_of(r, k, values).map_err(|_| bad(field));
        self.take(checked, choice_rule(r.get(k)))
    }
    /// The members of the object `v` among `allowed`, each other one `unknown_member` with its name
    /// after `at` (where the base's `only` names the first); false, with one entry `form`, for a `v`
    /// that is not an object.
    fn members(&mut self, v: &Value, allowed: &str, at: &str) -> bool {
        let obj = v.as_object().ok_or_else(|| bad(at.trim_end_matches('.')));
        let Some(obj) = self.take(obj, FORM) else {
            return false;
        };
        for k in obj.keys().filter(|k| !allowed.split(' ').any(|a| a == *k)) {
            self.refuse(&format!("{at}{k}"), ("unknown_member", None, None));
        }
        true
    }
    /// An array `k` of `r` of at most `max` entries, which are not looked into when `k` is not an
    /// array.
    fn list<'a>(&mut self, r: &'a Value, k: &str, max: usize) -> Option<&'a Vec<Value>> {
        let list = self.take(r[k].as_array().ok_or_else(|| bad(k)), FORM)?;
        if list.len() > max {
            self.refuse(k, ("max_entries", None, Some(max)));
        }
        Some(list)
    }
    /// The refusal of the first member, with every entry; none when no member breaks a rule.
    fn refusal(self) -> Option<(Failure, Value)> {
        self.first.map(|f| (f, json!(self.fields)))
    }
}
/// An array of at most `max` strings of 1 to `bytes` bytes without a control character, each
/// admitted by `ok`.
fn entries(
    b: &mut Breaks,
    r: &Value,
    k: &str,
    max: usize,
    bytes: usize,
    ok: fn(&str) -> bool,
) -> Option<Value> {
    let list = b.list(r, k, max)?;
    for (i, e) in list.iter().enumerate() {
        let within = e
            .as_str()
            .filter(|e| (1..=bytes).contains(&e.len()) && !e.chars().any(char::is_control));
        let checked = within.filter(|e| ok(e));
        let mut refused = bad(&format!("{k}.{i}"));
        if k == "write_set" && text_rule(Some(e), bytes) == FORM {
            // An entry refused by the rule `form` names both forms (R11): a text of neither form, one
            // with a control character, and a value that is no string.
            refused.detail = json!({"forms": WRITE_SET_FORMS});
        }
        b.text(checked.ok_or(refused), Some(e), bytes);
    }
    Some(json!(list))
}
/// `<owner>/<name>` (R9).
fn repository_ok(t: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    matches!(t.split_once('/'), Some((o, n)) if part(o) && part(n) && n != "." && n != "..")
}
/// A branch a unit's run may check out, push to and delete (R10).
pub fn branch_ok(b: &str) -> bool {
    let first = b.split('/').next().unwrap_or_default();
    (1..=100).contains(&b.len())
        && b.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c))
        && !b.starts_with(['-', '/'])
        && !b.contains("..")
        && !["origin", "refs", "heads", "remotes", "tags"].contains(&first)
        && b != "main"
        && b != "HEAD"
        && !b.ends_with(['/', '.'])
        && b.split('/')
            .all(|c| !c.is_empty() && !c.starts_with('.') && !c.ends_with(".lock"))
}
/// The patterns of a stored or requested `write_set`: its entries that are strings, in order (the
/// store types the property as an array of strings; another entry is skipped, as `outside` skips it).
fn patterns(list: Option<&Value>) -> Vec<&str> {
    let entries = list.and_then(Value::as_array).into_iter().flatten();
    entries.filter_map(Value::as_str).collect()
}
/// The refusal of the patterns `hit`, at most ten of them shown, with their number.
fn pattern_refusal(code: &'static str, hit: &[&str], more: (&str, Value)) -> Failure {
    let mut detail =
        json!({"patterns": hit.iter().take(10).collect::<Vec<_>>(), "count": hit.len()});
    detail[more.0] = more.1;
    fail(code, "write_set", detail)
}
/// R38: a stored pattern of neither form of R11 refuses `ready` and the look at a page.
fn write_set_form(patterns: &[&str]) -> Result<(), Failure> {
    let hit: Vec<&str> = (patterns.iter().copied())
        .filter(|p| !write_set_pattern(p).grammar)
        .collect();
    match hit.is_empty() {
        true => Ok(()),
        false => Err(pattern_refusal(
            "write_set_form",
            &hit,
            ("forms", json!(WRITE_SET_FORMS)),
        )),
    }
}
/// Whether a pattern of the grammar can only match protected paths (R39): an exact path that R36
/// protects, or a directory that is, or lies under, a protected tree.
fn only_protected(pattern: &str) -> bool {
    let p = write_set_pattern(pattern);
    p.grammar
        && match p.reading {
            Reading::Exact(path) => protected(path),
            Reading::Below(dir) => protected_tree(dir),
        }
}
/// R39: a unit of class `plain` whose pattern can only match protected paths is refused; the class
/// `contract` or `effect` never is. A text of neither form is not judged (R38 refuses it).
fn plain_writes_protected(class: Option<&str>, patterns: &[&str]) -> Result<(), Failure> {
    let hit: Vec<&str> = (patterns.iter().copied())
        .filter(|p| class == Some("plain") && only_protected(p))
        .collect();
    match hit.is_empty() {
        true => Ok(()),
        false => Err(pattern_refusal(
            "plain_writes_protected",
            &hit,
            ("needs", json!("class contract, lane objective")),
        )),
    }
}
/// R38 and then R39 on the stored page of `u`, as `ready` and the look at a page judge it.
pub(super) fn write_set_judged(u: &Value) -> Result<(), Failure> {
    let stored = patterns(u.get("write_set"));
    write_set_form(&stored)?;
    plain_writes_protected(u["class"].as_str(), &stored)
}
/// The most failures R46's refusal lists.
const CITATIONS_LISTED: usize = 50;
/// One failed citation of R46: its code, its field, its entry and, for a clause no entry names, the
/// first of its tests that lies in the write set.
struct Cited {
    code: &'static str,
    field: String,
    entry: String,
    test: Option<String>,
}
/// An entry of the tree of the base: its mode and its object.
struct Node {
    mode: String,
    object: String,
}
/// The file of the tree `base` at `path`, when the tree has an entry there, read from the objects of
/// the clone only: no checkout is read and nothing is added (R46). A text that is no relative path of
/// the tree names none.
fn node(s: &mut impl System, clone: &str, base: &str, path: &str) -> Result<Option<Node>, Failure> {
    if !path
        .split('/')
        .all(|c| !c.is_empty() && c != "." && c != "..")
    {
        return Ok(None);
    }
    let words = [
        "-C",
        clone,
        "--literal-pathspecs",
        "ls-tree",
        "-z",
        "--full-tree",
        base,
        "--",
        path,
    ];
    let listed = git_bytes(s, &words, "ls-tree")?;
    for line in listed.split(|b| *b == 0) {
        let line = String::from_utf8_lossy(line);
        let Some((head, name)) = line.split_once('\t') else {
            continue;
        };
        let parts: Vec<&str> = head.split(' ').collect();
        if let [mode, _, object] = parts[..]
            && name == path
        {
            return Ok(Some(Node {
                mode: mode.to_owned(),
                object: object.to_owned(),
            }));
        }
    }
    Ok(None)
}
/// The lines of a blob of the clone: its bytes split at line feeds, a final line feed ending the last
/// line, an empty blob having none.
fn lines(s: &mut impl System, clone: &str, object: &str) -> Result<u64, Failure> {
    let bytes = git_bytes(s, &["-C", clone, "cat-file", "blob", object], "cat-file")?;
    let feeds = bytes.iter().filter(|b| **b == b'\n').count() as u64;
    Ok(feeds + u64::from(bytes.last().is_some_and(|b| *b != b'\n')))
}
/// The path and the range of a `read_first` entry (R46 a): its first word, the text before its first
/// space; when the word ends with `:<from>-<to>` that range's `to`, else the whole word is the path. One
/// final `/` of the path is dropped.
fn read_first_word(entry: &str) -> (&str, Option<u64>) {
    let word = entry.split(' ').next().unwrap_or_default();
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
    let ranged = word.rsplit_once(':').and_then(|(path, range)| {
        let (from, to) = range.split_once('-')?;
        if !digits(from) || !digits(to) {
            return None;
        }
        // At least 1 and not past `to`, compared as numbers of any length.
        let (from, to) = (from.trim_start_matches('0'), to.trim_start_matches('0'));
        let ordered = !from.is_empty() && (from.len(), from) <= (to.len(), to);
        ordered.then(|| (path, to.parse::<u64>().unwrap_or(u64::MAX)))
    });
    let (path, to) = match ranged {
        Some((path, to)) => (path, Some(to)),
        None => (word, None),
    };
    (path.strip_suffix('/').unwrap_or(path), to)
}
/// R46: the citations of a page on the base `base` of the clone: every `read_first` entry names a path of
/// the base, and a range inside the file; every clause a `specs` entry names is a `clause` of the store,
/// unless its file is in the write set; every clause of the store whose test is in the write set and whose
/// file is not is named. All failures come in one refusal. A unit without a repository is not judged.
pub(super) fn citations_judged(
    s: &mut impl System,
    store: Option<&str>,
    u: &Value,
    clone: &str,
    base: &str,
) -> Result<(), Failure> {
    let Some(repository) = u["repository"].as_str().filter(|r| !r.is_empty()) else {
        return Ok(());
    };
    let set: Vec<String> = patterns(u.get("write_set"))
        .iter()
        .map(|p| (*p).to_owned())
        .collect();
    let inside = |path: &str| outside(&set, &[path]).is_empty();
    let filter = vec![json!({"eq": ["repository", repository]})];
    let found = at(store::search(s, store, "clause", filter), "clause")?;
    let clauses: BTreeMap<&str, &Value> = found.iter().map(|(k, f)| (k.as_str(), f)).collect();
    let mut failed: Vec<Cited> = vec![];
    let listed = |name: &str| -> Vec<(usize, &str)> {
        let all = u.get(name).and_then(Value::as_array).into_iter().flatten();
        all.enumerate()
            .filter_map(|(i, e)| Some((i, e.as_str()?)))
            .collect()
    };
    // (a) The first word of each `read_first` entry.
    let mut seen: BTreeMap<String, Option<Node>> = BTreeMap::new();
    let mut counted: BTreeMap<String, u64> = BTreeMap::new();
    for (i, entry) in listed("read_first") {
        let (path, to) = read_first_word(entry);
        if !seen.contains_key(path) {
            seen.insert(path.to_owned(), node(s, clone, base, path)?);
        }
        let code = match (&seen[path], to) {
            (None, _) => Some("read_first_missing"),
            (Some(_), None) => None,
            (Some(n), Some(to)) => {
                let file = n.mode == "100644" || n.mode == "100755";
                if file && !counted.contains_key(&n.object) {
                    counted.insert(n.object.clone(), lines(s, clone, &n.object)?);
                }
                (!file || counted[&n.object] < to).then_some("read_first_range")
            }
        };
        if let Some(code) = code {
            let field = format!("read_first.{i}");
            failed.push(Cited {
                code,
                field,
                entry: entry.to_owned(),
                test: None,
            });
        }
    }
    // (b) The clauses the entries `<path>#R<n>` of `specs` name.
    let specs = listed("specs");
    for (i, entry) in &specs {
        let Some((path, rule)) = entry.rsplit_once('#') else {
            continue;
        };
        let numbered = (rule.strip_prefix('R'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()));
        let named = numbered && path.starts_with("specs/") && path.ends_with(".md");
        let key = crate::clause::key(repository, path, rule);
        if named && !inside(path) && !clauses.contains_key(key.as_str()) {
            let field = format!("specs.{i}");
            failed.push(Cited {
                code: "clause_missing",
                field,
                entry: (*entry).to_owned(),
                test: None,
            });
        }
    }
    // (c) The clauses whose test is written and whose file is not, each named by an entry.
    let names: BTreeSet<&str> = specs.iter().map(|(_, e)| *e).collect();
    for (key, fields) in &clauses {
        let unreadable = || error("store_answer_unreadable", "clause");
        let path = fields["path"].as_str().ok_or_else(unreadable)?;
        let tests = fields["tests"].as_array().ok_or_else(unreadable)?;
        let written = |t: &&str| t.rsplit_once(':').is_some_and(|(p, _)| inside(p));
        let test = tests.iter().filter_map(Value::as_str).find(written);
        let rule = key.rsplit_once('#').map_or("", |(_, r)| r);
        // `<path>#R<n>` of the clause, or an entry without `#` that is its file or a directory holding it.
        let holds = |e: &&str| {
            let dir = e.strip_suffix('/').unwrap_or(e);
            !e.contains('#')
                && !dir.is_empty()
                && path
                    .strip_prefix(dir)
                    .is_some_and(|t| t.is_empty() || t.starts_with('/'))
        };
        let named = names.contains(format!("{path}#{rule}").as_str()) || names.iter().any(holds);
        if let (false, false, Some(test)) = (inside(path), named, test) {
            let entry = (*key).to_owned();
            let field = "specs".to_owned();
            failed.push(Cited {
                code: "clause_unnamed",
                field,
                entry,
                test: Some(test.to_owned()),
            });
        }
    }
    let Some(first) = failed.first() else {
        return Ok(());
    };
    let item = |c: &Cited| {
        let mut item = json!({"code": c.code, "field": c.field, "entry": c.entry});
        if let Some(test) = &c.test {
            item["test"] = json!(test);
        }
        item
    };
    let shown: Vec<Value> = failed.iter().take(CITATIONS_LISTED).map(item).collect();
    let detail = json!({"failed": shown, "count": failed.len()});
    Err(fail(first.code, &first.field, detail))
}
fn read_first_ok(e: &str) -> bool {
    let Some((path, range)) = e.rsplit_once(':') else {
        return true;
    };
    let numbers = range.split_once('-').and_then(|(a, b)| {
        let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
        (digits(a) && digits(b)).then(|| (a.parse::<u64>().ok(), b.parse::<u64>().ok()))
    });
    match numbers {
        None => true,
        Some((Some(from), Some(to))) => !path.is_empty() && 1 <= from && from <= to,
        Some(_) => false,
    }
}
fn spec_ok(e: &str) -> bool {
    match e.rsplit_once('#') {
        None => true,
        Some((path, r)) => {
            let n = r.strip_prefix('R').unwrap_or_default();
            !path.is_empty()
                && !path.contains('#')
                && !n.is_empty()
                && n.bytes().all(|c| c.is_ascii_digit())
        }
    }
}
/// A list property: its name, its most entries, the most bytes of one, and the form of one.
type List = (&'static str, usize, usize, fn(&str) -> bool);
/// The acceptance checks (R14).
fn acceptance(b: &mut Breaks, r: &Value) -> Option<Value> {
    let list = b.list(r, "acceptance", 20)?;
    for (i, c) in list.iter().enumerate() {
        let at = format!("acceptance.{i}.");
        if !b.members(c, "check kind", &at) {
            continue;
        }
        let field = format!("{at}check");
        let check = b.text(sized(&c["check"], &field, 600), c.get("check"), 600);
        if check.is_some_and(|t| t.contains(['\n', '\r'])) {
            b.refuse(&field, ("one_line", None, None));
        }
        let kinds = ["fails_on_base", "invariant"];
        b.choice(c, "kind", &format!("{at}kind"), &kinds);
    }
    Some(json!(list))
}
/// A tier: exactly `runtime`, `model` and `effort`, three strings of 1 to 80 bytes.
fn tier(b: &mut Breaks, r: &Value, k: &str) -> Option<Value> {
    let t = &r[k];
    let at = format!("{k}.");
    if !b.members(t, "runtime model effort", &at) {
        return None;
    }
    for m in ["runtime", "model", "effort"] {
        let checked = t[m].as_str().filter(|v| (1..=80).contains(&v.len()));
        b.text(
            checked.ok_or_else(|| bad(&format!("{at}{m}"))),
            t.get(m),
            80,
        );
    }
    Some(t.clone())
}
/// The parameters of `add_unit` or `amend_unit` from the request, every rule but the dependencies'
/// existence checked, each member that breaks one noted in `b`; the keys of `depends_on`.
fn page_parameters(b: &mut Breaks, r: &Value, op: &str, key: &str) -> (Value, Vec<String>) {
    let mut p = Map::from_iter([("unit".to_owned(), id("unit", key))]);
    let add = op == "add";
    if add {
        if let Some(lane) = b.choice(r, "lane", "lane", &["small", "objective"]) {
            p.insert("lane".into(), lane);
        }
        for (k, kind) in [("intent", "intent"), ("origin", "request")] {
            if r.get(k).is_some()
                && let Some(t) = b.text(sized(&r[k], k, 512), r.get(k), 512)
            {
                p.insert(k.into(), id(kind, t));
            }
        }
    }
    if (add || r.get("class").is_some())
        && let Some(class) = b.choice(r, "class", "class", &["plain", "contract", "effect"])
    {
        p.insert("class".into(), class);
    }
    for (k, max) in TEXTS {
        let bound = if k == "branch" { BRANCH_BYTES } else { max };
        let checked = bounded(r, k, max, add && k == "objective");
        if let Some(t) = b.text(checked, r.get(k), bound).flatten() {
            p.insert(k.into(), json!(t));
        }
    }
    let form = |k: &str, ok: fn(&str) -> bool| {
        let t = p.get(k).map(|v| v.as_str().unwrap_or_default());
        t.is_some_and(|t| !ok(t)).then(|| bad(k))
    };
    for (k, ok, bound) in [
        ("repository", repository_ok as fn(&str) -> bool, 200),
        ("branch", branch_ok, BRANCH_BYTES),
    ] {
        if let Some(f) = form(k, ok) {
            b.text::<()>(Err(f), r.get(k), bound);
        }
    }
    let lists: [List; 3] = [
        ("write_set", 60, 200, |e| write_set_pattern(e).grammar),
        ("read_first", 40, 300, read_first_ok),
        ("specs", 20, 300, spec_ok),
    ];
    for (k, max, bytes, ok) in lists {
        if r.get(k).is_some()
            && let Some(list) = entries(b, r, k, max, bytes, ok)
        {
            p.insert(k.into(), list);
        }
    }
    if r.get("acceptance").is_some()
        && let Some(list) = acceptance(b, r)
    {
        p.insert("acceptance".into(), list);
    }
    for k in ["implementer", "verifier"] {
        if r.get(k).is_some()
            && let Some(t) = tier(b, r, k)
        {
            p.insert(k.into(), t);
        }
    }
    if let Some(n) = r.get("estimate_lines") {
        let n = n.as_u64().filter(|n| *n <= i64::MAX as u64);
        if let Some(n) = b.take(n.ok_or_else(|| bad("estimate_lines")), FORM) {
            p.insert("estimate_lines".into(), json!(n));
        }
    }
    if let Some(n) = r.get("wave") {
        let n = n.as_u64().filter(|n| WAVES.contains(n));
        if let Some(n) = b.take(n.ok_or_else(|| bad("wave")), FORM) {
            p.insert("wave".into(), json!(n));
        }
    }
    let mut depends = vec![];
    if r.get("depends_on").is_some()
        && let Some(list) = b.list(r, "depends_on", 20)
    {
        for (i, d) in list.iter().enumerate() {
            let field = format!("depends_on.{i}");
            if let Some(d) = b.text(sized(d, &field, 512), Some(d), 512) {
                depends.push(d.to_owned());
            }
        }
        let ids: Vec<Value> = depends.iter().map(|d| id("unit", d)).collect();
        p.insert("depends_on".into(), json!(ids));
    }
    (p.into(), depends)
}
/// The clauses the entries `<path>#R<n>` of `specs` name in the unit's own `repository`, each once, in the
/// entries' order, as references of type `clause` (R43); none without a repository.
fn named_clauses(repository: Option<&str>, specs: Option<&Value>) -> Vec<Value> {
    let Some(repository) = repository.filter(|r| !r.is_empty()) else {
        return vec![];
    };
    let mut keys: Vec<String> = vec![];
    let entries = specs.and_then(Value::as_array).into_iter().flatten();
    for e in entries.filter_map(Value::as_str) {
        let Some((path, rule)) = e.rsplit_once('#') else {
            continue;
        };
        let numbered = (rule.strip_prefix('R'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()));
        let key = crate::clause::key(repository, path, rule);
        if numbered && path.starts_with("specs/") && path.ends_with(".md") && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.iter().map(|k| id("clause", k)).collect()
}
/// The size of the action request as it is sent (R8).
fn page_size(actor: &Actor, action: &str, rid: &str, p: &Value) -> Result<(), Failure> {
    let size = serde_json::to_vec(&store::body(actor, action, rid.to_owned(), p.clone()));
    let size = size.map_or(usize::MAX, |b| b.len());
    match size > PAGE_LIMIT {
        true => Err(fail(
            "page_too_large",
            "page",
            json!({"bytes": size, "limit": PAGE_LIMIT}),
        )),
        false => Ok(()),
    }
}
/// The engine gives the role of `ONT_ROLE` to a request that carries none (R4).
pub(crate) fn lead_in_environment(s: &impl System) -> bool {
    s.environment()
        .iter()
        .any(|(k, v)| k == "ONT_ROLE" && v == "lead")
}
/// The members each writing op admits besides `request_id`, `agent` and `session`.
fn members(op: &str) -> String {
    match op {
        "add" => format!("unit lane intent origin {PAGE}"),
        "amend" => format!("unit {PAGE}"),
        "ready" => "unit".into(),
        _ => "unit reason".into(),
    }
}
/// A writing op (R2 to R28); `fields` every member that breaks a rule of R2 to R16 (R37).
fn write(
    s: &mut impl System,
    store: Option<&str>,
    op: &str,
    r: &Value,
    after: &mut Option<String>,
    fields: &mut Option<Value>,
) -> Result<Value, Failure> {
    if !r.is_object() {
        return Err(bad("stdin"));
    }
    let mut b = Breaks::default();
    if op == "amend" {
        for k in ["lane", "intent", "origin"] {
            if r.get(k).is_some() {
                b.refuse(k, ("not_admitted", None, None));
            }
        }
    }
    b.members(r, &format!("request_id agent session {}", members(op)), "");
    let rid = b.text(shaped(r, "request_id", 44, "._-"), r.get("request_id"), 44);
    let agent = b.text(shaped(r, "agent", 64, "._:-"), r.get("agent"), 64);
    let session = b.text(shaped(r, "session", 64, "._:-"), r.get("session"), 64);
    let key = match op {
        "add" => b.text(new_key(r), r.get("unit"), 59),
        _ => b.text(sized(&r["unit"], "unit", 512), r.get("unit"), 512),
    };
    let key = key.unwrap_or_default();
    let (mut p, depends) = match op {
        "add" | "amend" => page_parameters(&mut b, r, op, key),
        "drop" => {
            let reason = bounded(r, "reason", 600, true);
            let reason = b.text(reason, r.get("reason"), 600).flatten();
            let reason = reason.unwrap_or_default();
            (json!({"unit": id("unit", key), "reason": reason}), vec![])
        }
        _ => (json!({"unit": id("unit", key)}), vec![]),
    };
    if let Some((first, all)) = b.refusal() {
        *fields = Some(all);
        return Err(first);
    }
    let actor = Actor {
        agent: agent.unwrap_or_default(),
        session: session.unwrap_or_default(),
    };
    let rid = format!("{}-{op}", rid.unwrap_or_default());
    let action = format!("{}_unit", if op == "add" { "add" } else { op });
    // The clauses the page names and the digest of the page as the action leaves it (R42, R43): for `add` from
    // the request; for `amend` the clauses the request alone names and a digest of its length, until the read.
    let names_clauses = r.get("specs").is_some() || r.get("repository").is_some();
    let clauses = named_clauses(p["repository"].as_str(), p.get("specs"));
    if (op == "add" && !clauses.is_empty()) || (op == "amend" && names_clauses) {
        p["clauses"] = json!(clauses);
    }
    match op {
        "add" => p["page_digest"] = json!(render::page_digest(key, &p)),
        "amend" => p["page_digest"] = json!("0".repeat(64)),
        _ => {}
    }
    page_size(&actor, &action, &rid, &p)?;
    if lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    if op == "ready" {
        return ready(s, store, &actor, rid, key, after);
    }
    if op == "add" {
        plain_writes_protected(p["class"].as_str(), &patterns(p.get("write_set")))?;
    }
    // `amend` and `drop` of a unit the store does not have: the actions would answer another refusal.
    let mut revision = None;
    if op != "add" {
        let Some((read, stored)) = at(store::head(s, store, "unit", key), "unit")? else {
            return Err(error("unknown_unit", "unit"));
        };
        revision = Some(read);
        // The class and the write set the unit would have: the request's where it gives them (R39).
        if op == "amend" && (r.get("class").is_some() || r.get("write_set").is_some()) {
            let class = p.get("class").unwrap_or(&stored["class"]);
            let set = p.get("write_set").or(stored.get("write_set"));
            plain_writes_protected(class.as_str(), &patterns(set))?;
        }
        if op == "amend" {
            // The page as the amendment leaves it: the unit as read, the request's members in their places.
            let mut page = stored.clone();
            for (k, v) in p.as_object().into_iter().flatten() {
                page[k] = v.clone();
            }
            if names_clauses {
                let named = named_clauses(page["repository"].as_str(), page.get("specs"));
                p["clauses"] = json!(named);
            }
            p["page_digest"] = json!(render::page_digest(key, &page));
            page_size(&actor, &action, &rid, &p)?;
        }
    }
    for d in &depends {
        if at(store::get(s, store, "unit", d), "depends_on")?.is_none() {
            return Err(fail("unknown_dependency", "depends_on", json!(d)));
        }
    }
    let applied = match op {
        "add" => store::add_unit(s, store, &actor, rid, p),
        "amend" => {
            let revision = revision.unwrap_or_default();
            store::amend_unit_at(s, store, &actor, rid, p, &revision)
        }
        _ => store::drop_unit(s, store, &actor, rid, p),
    };
    let applied = match at(applied, op) {
        Err(f) if op == "add" && store::code(&f) == "revision_conflict" => {
            return Err(fail("unit_exists", "unit", f.detail));
        }
        other => other?,
    };
    *after = Some(op.to_owned());
    let back = read_back(s, store, key)?;
    let mut answer = json!({"unit": key, "outcome": applied.outcome, "state": back["state"],
        "amended_at": back["amended_at"]});
    if op == "amend" {
        // True when the class needs a page review and none accepted names the digest read back (R17).
        let outdated = (back["class"] == "contract" || back["class"] == "effect")
            && !accepted_page(s, store, key, &render::page_digest(key, &back))?;
        answer["page_review_outdated"] = json!(outdated);
    }
    Ok(answer)
}
fn read_back(s: &mut impl System, store: Option<&str>, key: &str) -> Result<Value, Failure> {
    let back = at(store::get(s, store, "unit", key), "unit")?;
    back.ok_or_else(|| error("store_answer_unreadable", "unit"))
}
/// `ready` (R20 to R28): the unit read once, its page review, its checks on the base, the apply.
fn ready(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    key: &str,
    after: &mut Option<String>,
) -> Result<Value, Failure> {
    let read = at(store::head(s, store, "unit", key), "unit")?;
    let (revision, u) = read.ok_or_else(|| error("unknown_unit", "unit"))?;
    // The digest goes with every unit, so that the store names its first failed criterion (R21).
    let digest = render::page_digest(key, &u);
    let mut p = json!({"unit": id("unit", key), "page_digest": digest});
    if u["lane"] == "objective" {
        p["intent"] = u["intent"].clone();
    }
    let (mut base, mut checks) = (Value::Null, vec![]);
    if u["state"] == "draft" {
        write_set_judged(&u)?;
        // The digest the unit stores is the page's as `add` or `amend` left it (R44).
        if let Some(stored) = u["page_digest"].as_str()
            && stored != digest
        {
            let detail = json!({"digest": digest, "stored": stored});
            return Err(fail("page_digest_differs", "unit", detail));
        }
        if (u["class"] == "contract" || u["class"] == "effect")
            && !accepted_page(s, store, key, &digest)?
        {
            return Err(fail("page_not_reviewed", "unit", json!({"digest": digest})));
        }
        (base, checks) = on_base(s, store, &u)?;
    }
    let applied = at(
        store::ready_unit(s, store, actor, rid, p, &revision),
        "ready",
    )?;
    *after = Some("ready".into());
    let back = read_back(s, store, key)?;
    Ok(
        json!({"unit": key, "outcome": applied.outcome, "state": back["state"], "base": base,
        "checks": checks}),
    )
}
/// Some review of kind `page` of the unit, closed `accept`, names `digest` as its head, whenever it
/// started (R17, R21).
fn accepted_page(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    digest: &str,
) -> Result<bool, Failure> {
    let keys = at(
        store::view_keys(s, store, "reviews_of_unit", json!({"unit": key})),
        "reviews",
    )?;
    for k in keys {
        let review = at(store::get(s, store, "review", &k), "reviews")?;
        let Some(v) = review else { continue };
        if v["kind"] == "page"
            && v["status"] == "closed"
            && v["verdict"] == "accept"
            && v["head"] == digest
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// The clone of `repository` named by the host's policy file (R22).
pub fn clone_of(s: &impl System, repository: &str) -> Result<String, Failure> {
    let unmapped = || error("repository_not_mapped", "repository");
    let file = s.policy_file().ok_or_else(unmapped)?;
    let Some(path) = file.to_str() else {
        let missing =
            std::fs::metadata(&file).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        return Err(if missing {
            unmapped()
        } else {
            invalid("file", "path is not valid UTF-8")
        });
    };
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(unmapped()),
        Err(_) => return Err(invalid("file", "is unreadable")),
    };
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("file", "is not JSON"))?;
    let v = v
        .as_object()
        .ok_or_else(|| invalid("file", "is not one JSON object"))?;
    let Some(all) = v.get("repositories") else {
        return Err(unmapped());
    };
    let all = all
        .as_object()
        .ok_or_else(|| invalid("repositories", "is not an object"))?;
    let member = format!("repositories.{repository}");
    let Some(entry) = all.get(repository) else {
        return Err(unmapped());
    };
    let entry = entry
        .as_object()
        .ok_or_else(|| invalid(&member, "is not an object"))?;
    let Some(clone) = entry.get("clone") else {
        return Err(unmapped());
    };
    let absolute = |p: &&str| {
        Path::new(p).is_absolute() && !p.contains('\0') && !p.split('/').any(|c| c == "..")
    };
    let clone = clone.as_str().filter(absolute);
    let clone =
        clone.ok_or_else(|| invalid(&format!("{member}.clone"), "is not an absolute path"))?;
    Ok(clone.to_owned())
}
/// A new temporary directory of mode 0700, removed when dropped, whatever is in it.
struct Scratch(PathBuf);
impl Scratch {
    fn new(s: &impl System) -> Result<Self, Failure> {
        let name = new_session_id(s).map_err(|_| error("io_error", "temporary"))?;
        let path = std::env::temp_dir().join(format!("ap-ready-{name}"));
        let mut b = std::fs::DirBuilder::new();
        b.mode(0o700)
            .create(&path)
            .map_err(|_| error("io_error", "temporary"))?;
        Ok(Self(path))
    }
    fn at(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.0).is_err() {
            // A check may have taken the permissions of a directory it made: give them back, again.
            writable(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
/// Every directory below `dir` (not through a link) readable, writable and searchable by its owner.
pub(super) fn writable(dir: &Path) {
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            writable(&e.path());
        }
    }
}
fn git_step(s: &mut impl System, words: &[&str], step: &str) -> Result<String, Failure> {
    let a: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let reply = s.call("git", &a, &Value::Null, 60_000);
    let reply = reply.ok().filter(|r| r.success);
    let reply = reply.ok_or_else(|| fail("git_failed", "clone", json!(step)))?;
    Ok(String::from_utf8_lossy(&reply.stdout).trim().to_owned())
}
/// The standard output of one Git step of the clone, as it is; a step that fails is `git_failed`.
fn git_bytes(s: &mut impl System, words: &[&str], step: &str) -> Result<Vec<u8>, Failure> {
    let a: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let reply = s.call("git", &a, &Value::Null, 60_000);
    let reply = reply.ok().filter(|r| r.success);
    let reply = reply.ok_or_else(|| fail("git_failed", "clone", json!(step)))?;
    Ok(reply.stdout)
}
/// `git fetch origin main` in the clone within its bound (R23): the commit `origin/main` then names.
pub(super) fn fetch(s: &mut impl System, clone: &str) -> Result<String, Failure> {
    let fetch = ["-C", clone, "fetch", "--quiet", "origin", "main"];
    let fetch: Vec<String> = fetch.iter().map(|w| (*w).to_owned()).collect();
    let fetched = s.call("git", &fetch, &Value::Null, FETCH_MS);
    if !fetched.is_ok_and(|r| r.success) {
        return Err(error("fetch_failed", "clone"));
    }
    let named = ["-C", clone, "rev-parse", "--verify", "origin/main^{commit}"];
    git_step(s, &named, "rev-parse")
}
/// A throwaway copy of the clone at `dir` (R23): shared objects, no remote, a detached checkout of
/// `base`. Nothing is added to the clone.
pub(super) fn copy(s: &mut impl System, clone: &str, dir: &str, base: &str) -> Result<(), Failure> {
    let copy = ["clone", "--quiet", "--shared", "--no-checkout", clone, dir];
    git_step(s, &copy, "clone")?;
    git_step(s, &["-C", dir, "remote", "remove", "origin"], "remote")?;
    let checkout = ["-C", dir, "checkout", "--quiet", "--detach", base];
    git_step(s, &checkout, "checkout").map(|_| ())
}
/// The acceptance checks of `u` on the base, in a throwaway copy (R23 to R27): the base and the checks.
fn on_base(
    s: &mut impl System,
    store: Option<&str>,
    u: &Value,
) -> Result<(Value, Vec<Value>), Failure> {
    let Some(repository) = u["repository"].as_str().filter(|r| !r.is_empty()) else {
        return checks_on_base(s, u, None, None);
    };
    // The fetch, then the citations on the base (R46), then the copy and the checks.
    let clone = clone_of(s, repository)?;
    let base = fetch(s, &clone)?;
    citations_judged(s, store, u, &clone, &base)?;
    checks_on_base(s, u, Some(&clone), Some(&base))
}
/// The criteria of the store's `ready_unit` on the page itself, in the store's order, judged from the
/// unit as read (`ap unit review`, `specs/ap/unit-review.md` R23): the first that fails, with every
/// failed code in `detail`.
pub(super) fn complete(u: &Value) -> Result<(), Failure> {
    let absent = |k: &str| u.get(k).is_none_or(Value::is_null);
    let empty = |k: &str| absent(k) || u[k].as_array().is_some_and(Vec::is_empty);
    let mut failed = vec![];
    if empty("write_set") {
        failed.push("write_set_missing");
    }
    if empty("acceptance") {
        failed.push("acceptance_missing");
    }
    if absent("implementer") || absent("verifier") {
        failed.push("tiers_missing");
    }
    match failed.first() {
        Some(code) => Err(fail(code, "unit", json!({"failed": failed}))),
        None => Ok(()),
    }
}
/// The acceptance checks of `u` (R23 to R27): with `clone`, in a throwaway copy of it at `fetched` (the
/// commit `origin/main` named after a fetch the caller made), or at the commit a fetch made here names;
/// without, in an empty directory. The base and the checks.
pub(super) fn checks_on_base(
    s: &mut impl System,
    u: &Value,
    clone: Option<&str>,
    fetched: Option<&str>,
) -> Result<(Value, Vec<Value>), Failure> {
    let scratch = Scratch::new(s)?;
    let dir = scratch.at("base");
    let base = match clone {
        Some(clone) => {
            let base = match fetched {
                Some(commit) => commit.to_owned(),
                None => fetch(s, clone)?,
            };
            copy(s, clone, &dir, &base)?;
            Some(base)
        }
        None => {
            std::fs::create_dir(&dir).map_err(|_| error("io_error", "temporary"))?;
            None
        }
    };
    let list = u["acceptance"].as_array().cloned().unwrap_or_default();
    let mut checks = vec![];
    for (i, c) in list.iter().enumerate() {
        let (index, kind) = (i + 1, c["kind"].as_str().unwrap_or_default());
        let check = c["check"].as_str().unwrap_or_default();
        let output = scratch.at(&format!("check-{index}.out"));
        let started = s.now();
        let base = base.as_deref().unwrap_or_default();
        let status = run_check(s, &dir, (base, base), &output, check)?;
        let seconds = s.now().saturating_duration_since(started).as_secs();
        let code = match (status, kind) {
            (None, _) => Some("acceptance_not_runnable"),
            (Some(124 | 137), _) => Some("acceptance_timeout"),
            (Some(125..=127), "fails_on_base") => Some("acceptance_not_runnable"),
            (Some(n), "fails_on_base") if n >= 200 => Some("acceptance_not_runnable"),
            (Some(0), "fails_on_base") => Some("acceptance_passes_on_base"),
            (Some(0), _) | (Some(_), "fails_on_base") => None,
            (Some(_), _) => Some("invariant_fails_on_base"),
        };
        if let Some(code) = code {
            let detail = json!({"index": index, "kind": kind, "check": check, "status": status,
                "output": tail(&output)});
            return Err(fail(code, "acceptance", detail));
        }
        checks.push(json!({"index": index, "kind": kind, "status": status, "seconds": seconds}));
    }
    drop(scratch);
    Ok((json!(base), checks))
}
/// The last bytes of a check's output, cut forward to a character boundary: the file is opened and read
/// from its last `OUTPUT_TAIL` bytes by a seek, never whole (a check may write without bound).
fn tail(output: &str) -> String {
    let last = || -> std::io::Result<Vec<u8>> {
        let mut file = std::fs::File::open(output)?;
        let size = file.metadata()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(OUTPUT_TAIL as u64)))?;
        let mut bytes = vec![];
        file.take(OUTPUT_TAIL as u64).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let bytes = last().unwrap_or_default();
    let mut from = 0;
    while from < bytes.len() && (bytes[from] & 0xC0) == 0x80 {
        from += 1;
    }
    String::from_utf8_lossy(&bytes[from..]).into_owned()
}
/// The command of one check after the wrapper (R24): no directory or environment of the caller's but
/// `PATH` and `HOME`.
pub fn check_command(
    s: &impl System,
    dir: &str,
    (base, head): (&str, &str),
    output: &str,
    check: &str,
) -> Vec<String> {
    let env = s.environment();
    let var = |k: &str| {
        env.iter()
            .find(|(n, _)| n == k)
            .map_or("", |(_, v)| v.as_str())
    };
    let words = [
        "-c",
        WRAPPER,
        "sh",
        output,
        "env",
        "-i",
        "-C",
        dir,
        &format!("PATH={}", var("PATH")),
        &format!("HOME={}", var("HOME")),
        &format!("AP_BASE={base}"),
        &format!("AP_HEAD={head}"),
        "timeout",
        "-k",
        "10",
        CHECK_SECONDS,
        "bash",
        "-c",
        check,
    ];
    words.iter().map(|w| (*w).to_owned()).collect()
}
/// The `bwrap` words of a check's view (`specs/ap/run-limits.md` R12): the root read-only, a `/tmp`
/// of its own, its directory written but the `.git` in it, the hidden paths, no capability.
fn view(dir: &str, hidden: Vec<String>) -> Vec<String> {
    let git = format!("{dir}/.git");
    let mut words: Vec<String> = [
        crate::run::BWRAP,
        "--unshare-pid",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--ro-bind",
        "/proc/sys",
        "/proc/sys",
        "--tmpfs",
        "/tmp",
        "--bind",
        dir,
        dir,
    ]
    .iter()
    .map(|w| (*w).to_owned())
    .collect();
    if Path::new(&git).symlink_metadata().is_ok() {
        words.extend(["--ro-bind".to_owned(), git.clone(), git]);
    }
    words.extend(hidden);
    let tail = ["--cap-drop", "ALL", "--chdir", dir, "--"];
    words.extend(tail.iter().map(|w| (*w).to_owned()));
    words
}
/// One check through the port, inside a transient unit where units are used (R24, R25); its status,
/// when the wrapper printed one. `ap unit run` gives the merge base and the head (`AP_BASE`, `AP_HEAD`).
pub(super) fn run_check(
    s: &mut impl System,
    dir: &str,
    commits: (&str, &str),
    output: &str,
    check: &str,
) -> Result<Option<i64>, Failure> {
    let mut command = check_command(s, dir, commits, output, check);
    let (reply, unit) = if s.units() {
        // The view of the check: its directory and its own `/tmp` written, nothing else (R25,
        // `specs/ap/run-limits.md` R12).
        let hidden = crate::run::hidden_words(s);
        let hidden = hidden.ok_or_else(|| error("limits_unavailable", "acceptance"))?;
        let at = command.len() - 7;
        command.splice(at..at, view(dir, hidden));
        let id = new_session_id(s).map_err(|_| error("io_error", "unit"))?;
        let unit = format!("ap-check-{id}");
        let mut argv: Vec<String> = [
            "--wait",
            "--pipe",
            "--collect",
            "--quiet",
            "--expand-environment=no",
            "--unit",
            &unit,
            "--slice",
            "agents.slice",
            "-p",
            "RuntimeMaxSec=320s",
            "-p",
            "TimeoutStopSec=10s",
            "--",
            "sh",
        ]
        .iter()
        .map(|w| (*w).to_owned())
        .collect();
        argv.extend(command);
        (
            s.call("systemd-run", &argv, &Value::Null, CALL_MS),
            Some(unit),
        )
    } else {
        (s.call("sh", &command, &Value::Null, CALL_MS), None)
    };
    if let Some(unit) = unit
        && !stopped(s, &unit)
    {
        return Err(fail("check_not_stopped", "acceptance", json!(unit)));
    }
    let printed = reply
        .ok()
        .map(|r| String::from_utf8_lossy(&r.stdout).into_owned());
    let last = printed
        .as_deref()
        .and_then(|p| p.trim_end_matches('\n').rsplit('\n').next());
    Ok(last.and_then(|l| l.trim().parse().ok()))
}
/// Stops a check's unit and reads it as ended: `systemctl is-active` answers no live state.
fn stopped(s: &mut impl System, unit: &str) -> bool {
    let words = |w: &[&str]| w.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>();
    let _ = s.call("systemctl", &words(&["stop", unit]), &Value::Null, 60_000);
    let reply = s.call(
        "systemctl",
        &words(&["is-active", unit]),
        &Value::Null,
        10_000,
    );
    let state = reply.map(|r| String::from_utf8_lossy(&r.stdout).trim().to_owned());
    state.is_ok_and(|st| !st.is_empty() && !ALIVE.contains(&st.as_str()))
}
