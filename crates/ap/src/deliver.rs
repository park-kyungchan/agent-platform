//! `ap deliver`: one call brings the pair `ap` and content of this repository to the accepted tip
//! of `main` and leaves its record in the store (README "Delivery"). Every program it starts goes
//! through `System`; the lock is the only file it holds, `<state>` the only directory it writes.
use crate::review::{accepting, field, objects};
use crate::run::{stamp, write};
use crate::work::{Host, LIMIT, System, args, complete_id, digest, id, ont, refused};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Seek, Write},
    os::unix::process::ExitStatusExt,
    path::{Component, Path},
    process::ExitCode,
    time::SystemTime,
};

const MEMBERS: &str = "repository url state build_lock by reason past rollback";
const TARGET: &str = "/usr/local/bin/ap";
const CLIENT: [&str; 4] = ["action", "apply", "--client", "ap-deliver"];
const FREE_MIB: u64 = 10_240;

/// A refusal: its code and its `detail`.
struct No(&'static str, Value);
type Step<T> = Result<T, No>;
fn store_unreadable(store: Value) -> No {
    No("store_unreadable", json!({ "store": store }))
}

/// One call: the request, the paths it names and the installers' variables, and the record.
struct Call<'a> {
    r: &'a Value,
    state: String,
    clone: String,
    dir: String,
    backup: String,
    dst: String,
    bk: String,
    env: Vec<(String, String)>,
    rec: Value,
    /// `by` of the record and the attempt: the request's, or the timer's invocation.
    by: Value,
    /// The host lock; installers inherit it.
    lock: Option<File>,
}
/// What one installer did: its exit (128 plus the signal for a signal), its code, and for the
/// content installer the tarball it printed.
struct Ran {
    exit: Option<i32>,
    code: Value,
    tarball: Option<String>,
}

fn hex40(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}
fn absolute(p: &str) -> bool {
    let path = Path::new(p);
    path.is_absolute() && !p.contains('\0') && !path.components().any(|c| c == Component::ParentDir)
}
/// The member that makes the request unusable, if one does.
fn check(r: &Value) -> Result<(), String> {
    let o = r.as_object().ok_or("stdin")?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.split(' ').any(|m| m == *k)) {
        return Err(k.clone());
    }
    let text = |k: &str, max: usize| r[k].as_str().filter(|v| !v.is_empty() && v.len() <= max);
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    let name = text("repository", 200).and_then(|v| v.split_once('/'));
    let named = name.is_some_and(|(o, n)| {
        part(o) && part(n) && ![".", "..", "target", "last.json"].contains(&n)
    });
    let url =
        text("url", 4096).is_some_and(|u| !u.starts_with('-') && !u.chars().any(char::is_control));
    // Three forms: a Lead's install, a Lead's rollback (no `past`), the timer's (none of the four).
    let lead = ["by", "reason", "past", "rollback"]
        .iter()
        .any(|k| o.contains_key(*k));
    let checks = [
        ("repository", named),
        ("url", url),
        ("state", text("state", 4096).is_some_and(absolute)),
        ("build_lock", text("build_lock", 4096).is_some_and(absolute)),
        ("rollback", r.get("rollback").is_none_or(|v| *v == true)),
        ("by", !lead || text("by", 200).is_some()),
        ("reason", !lead || text("reason", 400).is_some()),
        (
            "past",
            r.get("past").is_none_or(|p| {
                !o.contains_key("rollback")
                    && p.as_array().is_some_and(|a| {
                        a.len() <= 50 && a.iter().all(|c| c.as_str().is_some_and(hex40))
                    })
            }),
        ),
    ];
    match checks.iter().find(|(_, ok)| !ok) {
        Some((k, _)) => Err((*k).to_owned()),
        None => Ok(()),
    }
}
/// A variable of the installers, as they read it: unset or empty takes the default.
fn var(env: &[(String, String)], key: &str, default: &str) -> String {
    let v = env
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, v)| v.as_str());
    let v = if v.is_empty() { default } else { v };
    let trimmed = v.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        trimmed.into()
    }
}
/// The host lock, taken without waiting; None when another holds it or it cannot be opened.
fn take(path: &str) -> Option<File> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .ok()?;
    rustix::fs::flock(&f, rustix::fs::FlockOperation::NonBlockingLockExclusive).ok()?;
    Some(f)
}
/// The stdout of a program that exited 0.
fn run(s: &mut impl System, program: &str, argv: Vec<String>, ms: u64) -> Option<String> {
    let reply = s
        .call(program, &argv, &Value::Null, ms)
        .ok()
        .filter(|r| r.success)?;
    String::from_utf8(reply.stdout).ok()
}
/// `git -C <clone>`; a fetch may take ten minutes, anything else one.
fn git(s: &mut impl System, c: &Call, words: &[&str]) -> Option<String> {
    let mut argv = args(&["-C", &c.clone]);
    argv.extend(args(words));
    let ms = if words[0] == "fetch" { 600_000 } else { 60_000 };
    run(s, "git", argv, ms)
}
/// The strings of an array member (`past`).
fn strs(v: &Value) -> Vec<&str> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
/// A string property of an object of a search's answer, empty when absent.
fn prop(o: &Value, k: &str) -> String {
    field(o, k).as_str().unwrap_or_default().to_owned()
}
/// The commit `ap --version` of the installed file names.
fn version(s: &mut impl System, c: &Call) -> Option<String> {
    let out = run(s, &format!("{}/ap", c.dir), args(&["--version"]), 10_000)?;
    let words: Vec<&str> = out.lines().next()?.split_whitespace().collect();
    (words.len() == 3 && words[0] == "ap" && hex40(words[2])).then(|| words[2].to_owned())
}
/// Whether the content tree passes `content/SHA256SUMS` of `commit`, read from the clone.
fn tree_at(s: &mut impl System, c: &Call, commit: &str) -> bool {
    let Some(sums) = git(s, c, &["show", &format!("{commit}:content/SHA256SUMS")]) else {
        return false;
    };
    let mut lines = sums.lines().filter(|l| !l.is_empty()).peekable();
    lines.peek().is_some()
        && lines.all(|l| match (l.get(..64), l.get(64..66), l.get(66..)) {
            (Some(hash), Some("  " | " *"), Some(path)) => s
                .read(&format!("{}/{path}", c.dst))
                .is_ok_and(|b| digest(&b) == hash),
            _ => false,
        })
}
/// Whether the pair is at `commit`: the file names it and the tree passes its manifest. A tree can
/// pass the manifests of several commits, so every read names the one commit it asks about.
fn pair_at(s: &mut impl System, c: &Call, file: &Option<String>, commit: &str) -> bool {
    file.as_deref() == Some(commit) && tree_at(s, c, commit)
}
/// The store's code of an answer that did not apply.
fn store_code(v: &Value) -> Value {
    let failed = strs(&v["failed"]);
    if !failed.is_empty() {
        json!(failed.join(" "))
    } else if v["result"]["outcome"] == "already_applied" {
        json!("already_applied")
    } else {
        v["code"].clone()
    }
}
/// One action of the client and agent `ap-deliver`; only the answer `applied` is Ok.
fn apply(s: &mut impl System, c: &Call, action: &str, request: String, p: Value) -> Step<Value> {
    let body = json!({"action_type": action, "request_id": request, "agent": "ap-deliver", "parameters": p});
    let answer = ont(s, c.r, &CLIENT, &body).ok();
    let v: Value = answer
        .and_then(|reply| serde_json::from_slice(&reply.bytes).ok())
        .ok_or_else(|| store_unreadable(Value::Null))?;
    if v["status"] == "ok" && v["result"]["outcome"] == "applied" {
        return Ok(v);
    }
    match refused(&v) || v["result"]["outcome"] == "already_applied" {
        true => Err(No("store_refused", json!({"store": store_code(&v)}))),
        false => Err(store_unreadable(store_code(&v))),
    }
}
fn search(
    s: &mut impl System,
    c: &Call,
    kind: &str,
    filter: &[(&str, Value)],
) -> Step<(Vec<Value>, Value)> {
    let all = filter.iter().map(|(k, v)| json!({"eq": [k, v]})).collect();
    objects(s, c.r, kind, all).map_err(|_| store_unreadable(Value::Null))
}
/// `ok` or `failed`: the index run of the installed files.
fn index(s: &mut impl System, c: &Call) -> &'static str {
    let reply = ont(
        s,
        c.r,
        &["index", "--source", "installed_binaries"],
        &Value::Null,
    )
    .ok();
    let v = reply
        .filter(|r| r.success)
        .and_then(|r| serde_json::from_slice::<Value>(&r.stdout).ok());
    let ok = v.is_some_and(|v| {
        v["status"] == "ok"
            && v["result"]["sources"]
                .as_array()
                .is_some_and(|a| a.iter().all(|x| x["status"] == "ok"))
    });
    if ok { "ok" } else { "failed" }
}
/// The kept copy of the file whose digest is `sha`, as `install-ap.sh` names it.
fn kept(c: &Call, sha: &str) -> String {
    format!(
        "{}/ap.before-{}",
        c.backup,
        sha.get(..7).unwrap_or_default()
    )
}
/// The oldest tarball of the content backup directory stamped later than `since` (UTC, RFC 3339; both
/// to the second), by the stamp in its name `agents-before-<YYYYmmddTHHMMSSZ>-<pid>.tar`. A tarball of
/// the second of `since` may be the previous call's, so it is never the handle.
fn oldest(dir: &str, since: &str) -> Option<String> {
    let since: String = since.chars().filter(|c| *c != '-' && *c != ':').collect();
    let names = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok());
    let stamped = names.filter_map(|n| {
        let stamp = n
            .strip_prefix("agents-before-")?
            .strip_suffix(".tar")?
            .split('-')
            .next()?;
        (stamp.len() == 16 && stamp > since.as_str()).then(|| (stamp.to_owned(), n.clone()))
    });
    stamped.min().map(|(_, n)| format!("{dir}/{n}"))
}
/// The file handle of an attempt: the kept copy named by `from_sha256`, only when its whole digest
/// is `from_sha256`.
fn kept_copy(s: &impl System, c: &Call, sha: &str) -> Option<String> {
    let copy = kept(c, sha);
    let same = s
        .read(&copy)
        .is_ok_and(|b| !sha.is_empty() && digest(&b) == sha);
    same.then_some(copy)
}
/// Removes the members that are null: the store takes an absent parameter, not a null one.
fn drop_nulls(p: &mut Value) {
    if let Some(o) = p.as_object_mut() {
        o.retain(|_, v| !v.is_null());
    }
}
/// The request id of an attempt: `ap.<commit>.<attempt>`, `ap.<commit>.rb<attempt>` for a rollback.
fn request_id(kind: &str, commit: &str, attempt: &Value) -> String {
    let rb = if kind == "rollback" { "rb" } else { "" };
    format!("ap.{commit}.{rb}{attempt}")
}
/// The first installer code of an attempt's rows.
fn first_code(ran: &[Ran]) -> Value {
    let code = ran.iter().map(|r| r.code.clone()).find(|c| !c.is_null());
    code.unwrap_or(Value::Null)
}
/// Where the pair is after a read decided `outcome` (not `installed`): `from`, or `neither`.
fn left_at<'a>(outcome: &str, from: &'a str) -> &'a str {
    if outcome == "failed" { "neither" } else { from }
}
type Handles = (Option<String>, Option<String>);
/// The command's rollback of a forward attempt to `from` with the attempt's handles: the file when it
/// is not at `from`, then the tree when it is not at `from`; then the read. `file` is the last read.
fn restore(
    s: &mut impl System,
    c: &mut Call,
    file: Option<String>,
    from: &str,
    handles: &Handles,
    ran: &mut Vec<Ran>,
) -> (&'static str, Option<String>) {
    if let Some(copy) = &handles.0
        && file.as_deref() != Some(from)
    {
        ran.push(installer(s, c, "ap-rollback", &["rollback", copy]));
    }
    if let Some(t) = &handles.1
        && !tree_at(s, c, from)
    {
        ran.push(installer(s, c, "content-rollback", &["rollback", t]));
    }
    let file = version(s, c);
    let at = pair_at(s, c, &file, from);
    (if at { "failed_restored" } else { "failed" }, file)
}
/// `close_install` of one attempt with its handles (absent members dropped), and the record's members
/// of the attempt. The record's `end` is the outcome, or `unrecorded` when the store did not apply it.
fn close(s: &mut impl System, c: &mut Call, request: &str, mut p: Value, h: Handles) {
    (p["rollback_binary"], p["rollback_content"]) = (json!(h.0), json!(h.1));
    for k in [
        "install",
        "outcome",
        "to",
        "code",
        "rollback_binary",
        "rollback_content",
    ] {
        c.rec[k] = p[k].clone();
    }
    drop_nulls(&mut p);
    let closed = apply(s, c, "close_install", format!("{request}.close"), p).is_ok();
    let end = if closed {
        c.rec["outcome"].clone()
    } else {
        json!("unrecorded")
    };
    c.rec["end"] = end;
}
/// Step 2: the running attempt of a dead call, closed by a read of the pair; a forward attempt at
/// neither of its commits is repaired. `Ok(true)`: the call ends here (`failed` or `unrecorded`).
fn dead(s: &mut impl System, c: &mut Call, a: &Value) -> Step<bool> {
    let f = |k: &str| prop(a, k);
    let (commit, from, kind) = (f("commit"), f("from"), f("kind"));
    let request = request_id(&kind, &commit, field(a, "attempt"));
    let running = || No("attempt_running", json!({"install": a["id"]}));
    let file = version(s, c).filter(|v| (*v == from || *v == commit) && tree_at(s, c, v));
    let mut p = json!({"install": a["id"], "to": file});
    match file {
        Some(v) if v == from => {
            (p["outcome"], p["code"]) = (json!("not_started"), json!("dead_call"))
        }
        Some(_) => {
            p["outcome"] = json!("installed");
            (p["rollback_binary"], p["rollback_content"]) = (
                json!(kept_copy(s, c, &f("from_sha256"))),
                json!(oldest(&c.bk, &f("started_at"))),
            );
            drop_nulls(&mut p);
        }
        None => {
            // A dead rollback, or a clone with no manifest of either commit, is not repaired.
            let sums =
                |s: &mut _, x: &str| git(s, c, &["show", &format!("{x}:content/SHA256SUMS")]);
            if kind != "forward" || sums(s, &commit).is_none() || sums(s, &from).is_none() {
                return Err(running());
            }
            return Ok(repair(s, c, a, &request));
        }
    }
    apply(s, c, "close_install", format!("{request}.close"), p.clone()).map_err(|_| running())?;
    c.rec["closed"] = json!({"install": a["id"], "outcome": p["outcome"]});
    Ok(false)
}
/// The repair of a dead forward attempt at neither of its commits: its installers again for the part
/// not at its commit (only when the clone's `origin/main` is that commit), the read, else the
/// command's rollback with the attempt's handles. True when the call ends here.
fn repair(s: &mut impl System, c: &mut Call, a: &Value, request: &str) -> bool {
    let f = |k: &str| prop(a, k);
    let (commit, from) = (f("commit"), f("from"));
    let origin = git(
        s,
        c,
        &["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"],
    );
    // The content handle is taken before any installer: the dead call's oldest tarball, else the
    // repair's own only when the tree was at `from` (a tarball of a tree at no manifest never is).
    let early = oldest(&c.bk, &f("started_at"));
    let at_from = early.is_none() && tree_at(s, c, &from);
    let (mut ran, mut printed) = (vec![], None);
    if origin.as_deref().map(str::trim) == Some(commit.as_str()) {
        if version(s, c).as_deref() != Some(commit.as_str()) {
            ran.push(installer(s, c, "ap", &["install", &commit]));
        }
        if ran.iter().all(|r| r.exit == Some(0)) && !tree_at(s, c, &commit) {
            let r = installer(s, c, "content", &["install", &commit]);
            printed = r.tarball.clone().filter(|_| at_from);
            ran.push(r);
        }
    }
    let tarball = early.or(printed.filter(|t| s.exists(t).unwrap_or(false)));
    let h = (kept_copy(s, c, &f("from_sha256")), tarball);
    let file = version(s, c);
    let (outcome, to) = if pair_at(s, c, &file, &commit) {
        ("installed", commit.as_str())
    } else {
        let (outcome, file) = restore(s, c, file, &from, &h, &mut ran);
        let at = outcome == "failed" && pair_at(s, c, &file, &commit);
        (
            outcome,
            if at {
                commit.as_str()
            } else {
                left_at(outcome, &from)
            },
        )
    };
    let p = json!({"install": a["id"], "outcome": outcome, "to": to, "code": first_code(&ran)});
    close(s, c, request, p, h);
    (c.rec["closed"], c.rec["attempt"], c.rec["from"]) = (
        json!({"install": a["id"], "outcome": outcome}),
        field(a, "attempt").clone(),
        json!(from),
    );
    c.rec["index"] = json!({"before": null, "after": index(s, c)});
    let ends = c.rec["end"] != "installed" && c.rec["end"] != "failed_restored";
    if !ends {
        for k in [
            "install",
            "attempt",
            "from",
            "outcome",
            "to",
            "code",
            "rollback_binary",
            "rollback_content",
            "end",
        ] {
            c.rec[k] = record()[k].clone();
        }
    }
    ends
}
/// Step 6: the head of the tip, its accepting review, and the accepting reviews oldest first.
fn accepted(
    s: &mut impl System,
    c: &Call,
    from: &str,
    tip: &str,
) -> Step<(String, Value, Vec<Value>)> {
    let past = strs(&c.r["past"]);
    let range = format!("{from}..{tip}");
    let lines = git(s, c, &["rev-list", "--first-parent", "--parents", &range]).unwrap_or_default();
    let mut chain: Vec<Vec<&str>> = lines.lines().map(|l| l.split(' ').collect()).collect();
    chain.reverse();
    let no = |commit: &str, why: &str| No("not_accepted", json!({"commit": commit, "why": why}));
    if chain.first().and_then(|l| l.get(1)) != Some(&from)
        || chain.last().map(|l| l[0]) != Some(tip)
    {
        return Err(no(from, "not_on_chain"));
    }
    if let Some(p) = past.iter().find(|p| !chain.iter().any(|l| l[0] == **p)) {
        return Err(No("past_not_on_chain", json!({ "commit": p })));
    }
    let repository = c.r["repository"].as_str().unwrap_or_default();
    let (mut reviews, mut last) = (vec![], (String::new(), Value::Null));
    for l in &chain {
        let commit = l[0];
        if commit != tip && past.contains(&commit) {
            continue;
        }
        if l.len() != 3 {
            return Err(no(commit, "not_a_merge"));
        }
        let trees = [format!("{commit}^{{tree}}"), format!("{commit}^2^{{tree}}")];
        let trees = git(s, c, &["rev-parse", &trees[0], &trees[1]]).unwrap_or_default();
        let trees: Vec<&str> = trees.split_whitespace().collect();
        if trees.len() != 2 || trees[0] != trees[1] {
            return Err(no(commit, "tree_differs"));
        }
        let found =
            accepting(s, c.r, repository, l[2], None).map_err(|_| store_unreadable(Value::Null))?;
        let closed = |o: &&Value| prop(o, "closed_at");
        let review = found
            .iter()
            .max_by_key(closed)
            .ok_or_else(|| no(commit, "no_accepting_review"))?;
        if commit == tip {
            let report = field(review, "report")
                .as_str()
                .and_then(|p| s.read(p).ok());
            let report = report.ok_or_else(|| no(commit, "report_missing"))?;
            if field(review, "report_sha256").as_str() != Some(digest(&report).as_str()) {
                return Err(no(commit, "report_differs"));
            }
        }
        reviews.push(review["id"].clone());
        last = (l[2].to_owned(), review["id"].clone());
    }
    Ok((last.0, last.1, reviews))
}
/// One installer from the clone, with the command's environment (and so its lock) and the build
/// directory; its stdout and stderr go to unlinked files of `<state>`.
fn installer(s: &mut impl System, c: &mut Call, name: &str, words: &[&str]) -> Ran {
    let script = if name.starts_with("ap") {
        "install-ap.sh"
    } else {
        "install-content.sh"
    };
    let mut argv = args(&["bash", &format!("{}/scripts/{script}", c.clone)]);
    argv.extend(args(words));
    let mut env: Vec<_> = c
        .env
        .iter()
        .filter(|(k, _)| k != "CARGO_TARGET_DIR")
        .cloned()
        .collect();
    env.push(("CARGO_TARGET_DIR".into(), format!("{}/target", c.state)));
    let scratch = || {
        (0..8).find_map(|n| {
            let p = format!("{}/.deliver-{}-{n}.tmp", c.state, std::process::id());
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&p)
                .ok()?;
            std::fs::remove_file(&p).ok().map(|()| f)
        })
    };
    let back = |mut f: File| {
        let mut b = vec![];
        let _ = f.rewind().and_then(|()| f.read_to_end(&mut b));
        String::from_utf8_lossy(&b).into_owned()
    };
    let ran = (|| {
        let (out, err) = (scratch()?, scratch()?);
        let files = [
            File::open("/dev/null").ok()?,
            out.try_clone().ok()?,
            err.try_clone().ok()?,
        ];
        // The installer inherits the lock: it stays held while one lives.
        let fd = |flags| c.lock.as_ref().map(|l| rustix::io::fcntl_setfd(l, flags));
        fd(rustix::io::FdFlags::empty());
        let child = s.launch(&argv, &env, &c.clone, files);
        fd(rustix::io::FdFlags::CLOEXEC);
        let status = child.ok()?.wait().ok()?;
        let exit = status.code().or_else(|| status.signal().map(|n| 128 + n))?;
        Some((exit, back(out), back(err)))
    })();
    let (exit, out, err) = match ran {
        Some((exit, out, err)) => (Some(exit), out, err),
        None => (None, String::new(), String::new()),
    };
    let ap_code = || {
        err.lines().rev().find_map(|l| {
            let (word, rest) = l.split_once('@')?;
            let word_ok = !word.is_empty()
                && word
                    .bytes()
                    .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'));
            (word_ok && rest.starts_with("install-ap.sh:")).then(|| json!(word))
        })
    };
    let code = match (name.starts_with("ap"), exit) {
        (_, Some(0) | None) => Value::Null,
        (true, _) => ap_code().unwrap_or(Value::Null),
        (false, Some(2)) => json!("content_refused"),
        (false, Some(1)) => json!("content_failed_restored"),
        (false, Some(n)) => json!(format!("content_exit_{n}")),
    };
    let after = |text: &str, marker: &str| {
        let path = text
            .lines()
            .rev()
            .find_map(|l| l.split_once(marker).map(|(_, p)| p.trim().to_owned()));
        path.filter(|p| p.starts_with('/'))
    };
    let tarball = match (name, exit) {
        ("content", Some(0)) => after(&out, "backup="),
        ("content", Some(1)) => after(&err, "kept copy "),
        _ => None,
    };
    if let Some(rows) = c.rec["installers"].as_array_mut() {
        rows.push(json!({"name": name, "exit": exit, "code": code}));
    }
    Ran {
        exit,
        code,
        tarball,
    }
}
/// Steps 2 to 12 of README "Delivery"; a refusal ends the call, every other end is in the record.
fn deliver(s: &mut impl System, c: &mut Call) -> Step<()> {
    let target = id("installed_artifact", TARGET);
    let (running, answer) = search(
        s,
        c,
        "install",
        &[("target", target.clone()), ("status", json!("running"))],
    )?;
    let schema = answer["schema"]["work"]["version"].clone();
    let started = |o: &&Value| prop(o, "started_at");
    if let Some(a) = running.iter().max_by_key(started)
        && dead(s, c, a)?
    {
        return Ok(());
    }
    if c.r.get("rollback").is_some() {
        return rollback(s, c, &target);
    }
    // 3. The clone at the fetched tip.
    let url = c.r["url"].as_str().unwrap_or_default();
    let refspec = "+refs/heads/main:refs/remotes/origin/main";
    let fetched = run(s, "git", args(&["init", "--quiet", &c.clone]), 30_000).is_some()
        && git(s, c, &["fetch", "--quiet", "--no-tags", url, refspec]).is_some();
    let tip = fetched.then(|| {
        git(
            s,
            c,
            &["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"],
        )
    });
    let tip = tip
        .flatten()
        .map(|t| t.trim().to_owned())
        .filter(|t| hex40(t));
    let tip = tip.ok_or(No("fetch_failed", Value::Null))?;
    git(s, c, &["checkout", "--quiet", "--force", "--detach", &tip])
        .ok_or(No("fetch_failed", Value::Null))?;
    c.rec["tip"] = json!(tip);
    // 4. The pair.
    let file = version(s, c);
    let from = match file.filter(|v| tree_at(s, c, v)) {
        Some(v) if v == tip => {
            c.rec["end"] = json!("current");
            return Ok(());
        }
        Some(v) => v,
        None => return Err(differs(s, c, &target)),
    };
    c.rec["from"] = json!(from);
    // 5. The engine and the schema the tip names.
    let pin =
        git(s, c, &["show", &format!("{tip}:ontology/ENGINE-PIN")]).map(|p| p.trim().to_owned());
    let engine = run(s, "ont", args(&["--version"]), 10_000);
    let engine = engine.and_then(|l| l.split_whitespace().nth(2).map(str::to_owned));
    if pin.is_none() || pin != engine {
        return Err(No(
            "engine_differs",
            json!({"tip": pin, "installed": engine}),
        ));
    }
    let tip_schema = git(s, c, &["show", &format!("{tip}:ontology/work.schema.json")]);
    let tip_schema = tip_schema
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| v["version"].clone());
    let tip_schema = tip_schema.unwrap_or(Value::Null);
    if tip_schema.is_null() || tip_schema != schema {
        return Err(No(
            "schema_differs",
            json!({"tip": tip_schema, "installed": schema}),
        ));
    }
    // 6. Accepted; the timer's limits; 7. room and the build lock.
    let (head, review, reviews) = accepted(s, c, &from, &tip)?;
    if c.r.get("by").is_none() {
        limits(s, c, &target, &tip)?;
    }
    let free = run(s, "df", args(&["-BM", "--output=avail", &c.state]), 10_000);
    let free = free.and_then(|o| {
        o.lines()
            .last()?
            .trim()
            .trim_end_matches('M')
            .parse::<u64>()
            .ok()
    });
    if free.is_none_or(|f| f < FREE_MIB) {
        return Err(No("disk_low", Value::Null));
    }
    let build_lock = c.r["build_lock"].as_str().unwrap_or_default();
    run(s, "flock", args(&["-n", build_lock, "true"]), 10_000)
        .ok_or(No("build_busy", Value::Null))?;
    // 8. The attempt, applied before any installer.
    c.rec["index"] = json!({"before": index(s, c), "after": null});
    let (same, _) = search(
        s,
        c,
        "install",
        &[("target", target.clone()), ("commit", json!(tip))],
    )?;
    let attempt = same.len() + 1;
    let from_sha256 = s.read(&format!("{}/ap", c.dir)).map(|b| digest(&b));
    let Ok(from_sha256) = from_sha256 else {
        return Err(differs(s, c, &target));
    };
    let mut p = json!({"target": target, "kind": "forward", "repository": c.r["repository"], "commit": tip,
        "attempt": attempt, "from": from, "from_sha256": from_sha256, "by": c.by, "reason": c.r["reason"],
        "head": head, "review": review, "reviews": reviews});
    let past = strs(&c.r["past"]);
    if !past.is_empty() {
        p["past"] = json!(past.join(" "));
    }
    start(s, c, format!("ap.{tip}.{attempt}"), p, attempt)?;
    // 9. The installers, then 10. the reads decide the outcome: is the pair at the tip, is it at
    // `from`; each rollback on its own question.
    let mut ran = vec![installer(s, c, "ap", &["install", &tip])];
    if ran[0].exit == Some(0) {
        ran.push(installer(s, c, "content", &["install", &tip]));
    }
    let file = version(s, c);
    let h = (
        kept_copy(s, c, &from_sha256),
        ran.get(1).and_then(|r| r.tarball.clone()),
    );
    let h = (h.0, h.1.filter(|t| s.exists(t).unwrap_or(false)));
    let (outcome, to) = if pair_at(s, c, &file, &tip) {
        ("installed", tip.as_str())
    } else if pair_at(s, c, &file, &from) {
        let busy = ran[0].code == "install_busy";
        (
            if busy {
                "not_started"
            } else {
                "failed_restored"
            },
            from.as_str(),
        )
    } else {
        let (outcome, file) = restore(s, c, file, &from, &h, &mut ran);
        let at = outcome == "failed" && pair_at(s, c, &file, &tip);
        (
            outcome,
            if at {
                tip.as_str()
            } else {
                left_at(outcome, &from)
            },
        )
    };
    // 11. The close, after the read; 12. the index again.
    let p = json!({"install": c.rec["install"], "outcome": outcome, "to": to, "code": first_code(&ran[..ran.len().min(2)])});
    close(s, c, &format!("ap.{tip}.{attempt}"), p, h);
    c.rec["index"]["after"] = json!(index(s, c));
    Ok(())
}
/// `start_install` (absent members dropped); only an answer `applied` that names the created
/// attempt goes on.
fn start(
    s: &mut impl System,
    c: &mut Call,
    request: String,
    mut p: Value,
    attempt: usize,
) -> Step<()> {
    drop_nulls(&mut p);
    let v = apply(s, c, "start_install", request, p)?;
    let edit = &v["result"]["edits"][0];
    let created = edit["change"] == "initial"
        && complete_id(&edit["id"])
        && edit["id"]["object_type"] == "install";
    if !created {
        return Err(store_unreadable(Value::Null));
    }
    (c.rec["install"], c.rec["attempt"]) = (edit["id"].clone(), json!(attempt));
    Ok(())
}
/// The timer's limits, from the attempts of the target newest first (README "Delivery").
/// Every attempt of the target, newest first, as the view `installs` orders them.
fn attempts(s: &mut impl System, c: &Call, target: &Value) -> Step<Vec<Value>> {
    let (mut all, _) = search(s, c, "install", &[("target", target.clone())])?;
    all.sort_by_key(|a| std::cmp::Reverse((prop(a, "started_at"), prop(a, "closed_at"))));
    Ok(all)
}
/// `pair_differs`; for the timer it names the newest attempt that did not end `not_started` (exit 3).
fn differs(s: &mut impl System, c: &Call, target: &Value) -> No {
    let timer = c.r.get("by").is_none();
    let all = if timer {
        attempts(s, c, target).unwrap_or_default()
    } else {
        vec![]
    };
    let ended = |a: &&Value| !matches!(prop(a, "outcome").as_str(), "" | "not_started");
    let newest = all.iter().find(ended).map(|a| json!({"install": a["id"]}));
    No("pair_differs", newest.unwrap_or(Value::Null))
}
fn limits(s: &mut impl System, c: &Call, target: &Value, tip: &str) -> Step<()> {
    let all = attempts(s, c, target)?;
    let is = |a: &Value, k: &str, v: &[&str]| v.contains(&prop(a, k).as_str());
    let no = |code, a: &Value| Err(No(code, json!({"install": a["id"]})));
    let ended = ["installed", "failed_restored", "failed"];
    if let Some(a) = all.iter().find(|a| is(a, "outcome", &ended)) {
        if is(a, "outcome", &ended[1..]) {
            return no("suppressed", a);
        }
        if is(a, "kind", &["rollback"]) {
            return no("held_by_rollback", a);
        }
    }
    let on_tip: Vec<&Value> = all
        .iter()
        .filter(|a| is(a, "kind", &["forward"]) && is(a, "commit", &[tip]))
        .collect();
    if let Some(a) = on_tip.iter().find(|a| is(a, "outcome", &ended)) {
        return no("attempt_exists", a);
    }
    let not_started: Vec<_> = on_tip
        .iter()
        .filter(|a| is(a, "outcome", &["not_started"]))
        .collect();
    match not_started.first() {
        Some(a) if not_started.len() >= 3 => no("attempts_exhausted", a),
        _ => Ok(()),
    }
}
/// A Lead's rollback of the newest install (README "Delivery"): the content first, the file only
/// after the content step exited 0.
fn rollback(s: &mut impl System, c: &mut Call, target: &Value) -> Step<()> {
    let file = version(s, c);
    let here = file.filter(|v| tree_at(s, c, v));
    let here = here.ok_or(No("pair_differs", Value::Null))?;
    c.rec["from"] = json!(here);
    let nothing = || No("nothing_to_roll_back", Value::Null);
    let kind = |k: &str| ("kind", json!(k));
    let (found, _) = search(
        s,
        c,
        "install",
        &[
            ("target", target.clone()),
            ("commit", json!(here)),
            kind("forward"),
        ],
    )?;
    let a = found.iter().filter(|a| prop(a, "outcome") == "installed");
    let a = a
        .max_by_key(|a| prop(a, "started_at"))
        .ok_or_else(nothing)?;
    let back = prop(a, "from");
    let bin = kept_copy(s, c, &prop(a, "from_sha256")).ok_or_else(nothing)?;
    let tar = field(a, "rollback_content").as_str().map(str::to_owned);
    let tar = tar.filter(|t| s.exists(t).unwrap_or(false));
    if tar.is_none() {
        let sums = |s: &mut _, x: &str| git(s, c, &["show", &format!("{x}:content/SHA256SUMS")]);
        let (one, two) = (sums(s, &back), sums(s, &here));
        if one.is_none() || one != two {
            return Err(nothing());
        }
    }
    c.rec["index"] = json!({"before": index(s, c), "after": null});
    let (same, _) = search(
        s,
        c,
        "install",
        &[
            ("target", target.clone()),
            ("commit", json!(back)),
            kind("rollback"),
        ],
    )?;
    let attempt = same.len() + 1;
    let from_sha256 = s.read(&format!("{}/ap", c.dir)).map(|b| digest(&b));
    let from_sha256 = from_sha256.map_err(|_| No("pair_differs", Value::Null))?;
    let p = json!({"target": target, "kind": "rollback", "repository": c.r["repository"], "commit": back,
        "attempt": attempt, "from": here, "from_sha256": from_sha256, "rolls_back": a["id"], "by": c.by,
        "reason": c.r["reason"]});
    let request = format!("ap.{back}.rb{attempt}");
    start(s, c, request.clone(), p, attempt)?;
    let mut ran = vec![];
    if let Some(t) = &tar {
        ran.push(installer(s, c, "content-rollback", &["rollback", t]));
    }
    let content = ran.first().map_or(Some(0), |r| r.exit);
    let outcome = if content == Some(0) {
        ran.push(installer(s, c, "ap-rollback", &["rollback", &bin]));
        let file = version(s, c);
        if pair_at(s, c, &file, &back) {
            "installed"
        } else if pair_at(s, c, &file, &here) {
            "failed_restored"
        } else {
            "failed"
        }
    } else if let file = version(s, c)
        && pair_at(s, c, &file, &here)
    {
        if content == Some(2) {
            "not_started"
        } else {
            "failed_restored"
        }
    } else {
        "failed"
    };
    let to = if outcome == "installed" {
        back.as_str()
    } else {
        left_at(outcome, &here)
    };
    let p = json!({"install": c.rec["install"], "outcome": outcome, "to": to, "code": first_code(&ran)});
    let h = (kept_copy(s, c, &from_sha256), None);
    close(s, c, &request, p, h);
    c.rec["index"]["after"] = json!(index(s, c));
    Ok(())
}
fn record() -> Value {
    json!({"at": stamp(SystemTime::now()), "by": null, "repository": null, "target": TARGET, "tip": null,
        "from": null, "end": "refused", "outcome": null, "code": null, "detail": null, "install": null,
        "attempt": null, "installers": [], "to": null, "rollback_binary": null, "rollback_content": null,
        "index": null, "closed": null})
}
fn invalid(field: &str) -> (Value, u8) {
    let mut rec = record();
    (rec["code"], rec["detail"]) = (json!("invalid_input"), json!({ "field": field }));
    (rec, 2)
}
/// One call: the record and the exit code. The record is in `<state>/last.json` for every end but
/// `invalid_input`.
pub fn execute(s: &mut impl System, r: &Value) -> (Value, u8) {
    if let Err(field) = check(r) {
        return invalid(&field);
    }
    let env = s.environment();
    let text = |k: &str| r[k].as_str().unwrap_or_default().to_owned();
    let name = text("repository")
        .split_once('/')
        .map(|(_, n)| n.to_owned())
        .unwrap_or_default();
    let state = text("state").trim_end_matches('/').to_owned();
    let mut c = Call {
        r,
        clone: format!("{state}/{name}"),
        state,
        dir: var(&env, "AP_INSTALL_DIR", "/usr/local/bin"),
        backup: var(&env, "AP_BACKUP_DIR", "/var/lib/agent-platform/install"),
        dst: var(&env, "AP_CONTENT_DST", "/root/.agents"),
        bk: var(
            &env,
            "AP_CONTENT_BACKUP_DIR",
            "/var/backups/agent-platform-content",
        ),
        rec: record(),
        by: r.get("by").cloned().unwrap_or_else(|| {
            let id = env
                .iter()
                .find(|(k, v)| k == "INVOCATION_ID" && !v.is_empty());
            json!(id.map_or("timer".into(), |(_, v)| format!("timer:{v}")))
        }),
        lock: None,
        env,
    };
    (c.rec["by"], c.rec["repository"]) = (c.by.clone(), r["repository"].clone());
    let made = std::fs::create_dir_all(&c.state).is_ok();
    c.lock = take(&var(&c.env, "AP_DELIVER_LOCK", "/run/lock/ap-deliver.lock"));
    let result = match c.lock {
        Some(_) => deliver(s, &mut c),
        None => Err(No("busy", Value::Null)),
    };
    if let Err(No(code, detail)) = result {
        (c.rec["end"], c.rec["code"], c.rec["detail"]) = (json!("refused"), json!(code), detail);
    }
    let code = c.rec["code"].as_str().unwrap_or_default();
    let exit = match (c.rec["end"].as_str(), c.rec["outcome"] == "failed") {
        (_, true) => 4,
        (Some("installed" | "current"), _) => 0,
        (Some("refused"), _) if ["suppressed", "attempts_exhausted"].contains(&code) => 3,
        (Some("refused"), _) if code == "pair_differs" && r.get("by").is_none() => 3,
        (Some("refused"), _) => 2,
        (Some("not_started" | "failed_restored"), _) => 3,
        _ => 5,
    };
    if made {
        let _ = write(
            &format!("{}/last.json", c.state),
            format!("{}\n", c.rec).as_bytes(),
        );
    }
    drop(c.lock);
    (c.rec, exit)
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let mut bytes = vec![];
    let read = std::io::stdin()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .is_ok()
        && bytes.len() as u64 <= LIMIT;
    let (rec, exit) = if !argv.is_empty() {
        invalid("args")
    } else if !read {
        invalid("stdin")
    } else {
        execute(
            &mut Host,
            &serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    };
    let _ = writeln!(std::io::stdout(), "{rec}");
    ExitCode::from(exit)
}
