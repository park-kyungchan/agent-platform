//! `ap state`: what the records of a task directory and Git say of its unit calls and stages, and
//! the form of its `STATE.md`, read only (`specs/ap/state.md`). Files are read with `std::fs`;
//! `git`, `systemctl` and `ps` cross the `System` port.
use crate::host::{Failure, System, args, error};
use crate::run::stamp;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::ExitCode,
    time::{Duration, SystemTime},
};

/// The most lines `STATE.md` holds (`ontology/README.md`, "Version 21").
const STATE_LINES: usize = 30;
/// The lines the plain form prints in all, the form line included.
const TEXT_LINES: usize = 60;
/// What a block rendered by `ont render` begins with.
const BLOCK_OPENING: &[u8] = b"<!-- ont:render ";

/// A record that parsed: its path relative to the task directory and its object.
struct Rec {
    path: String,
    v: Value,
}
impl Rec {
    fn get(&self, k: &str) -> Value {
        self.v.get(k).cloned().unwrap_or(Value::Null)
    }
    fn str(&self, k: &str) -> &str {
        self.v[k].as_str().unwrap_or("")
    }
    /// The record's own prefix: the path less `.unit.json`, then `.`.
    fn owns(&self, stage: &Rec) -> bool {
        let own = self.path.strip_suffix(".unit.json").unwrap_or(&self.path);
        stage
            .path
            .strip_prefix(own)
            .is_some_and(|r| r.starts_with('.'))
    }
}
/// The calls of one pull request (`repository`, number), or of one call without.
type Chain<'a> = (Option<(String, u64)>, Vec<&'a Rec>);
/// The records of one kind, by `start` (then path), and the paths that did not parse.
fn records(runs: &Path, suffix: &str, bad: &mut Vec<String>) -> Vec<Rec> {
    let mut names: Vec<String> = std::fs::read_dir(runs)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(suffix))
        .collect();
    names.sort();
    let mut out = vec![];
    for name in names {
        let path = format!("runs/{name}");
        let v = std::fs::read(runs.join(&name))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        match v {
            Some(v) if v.is_object() => out.push(Rec { path, v }),
            _ => bad.push(path),
        }
    }
    out.sort_by(|a, b| (a.str("start"), &a.path).cmp(&(b.str("start"), &b.path)));
    out
}
/// The words of `systemctl is-active` while something of a unit may live.
const ALIVE: [&str; 5] = [
    "active",
    "activating",
    "deactivating",
    "reloading",
    "refreshing",
];
/// Whether the unit may live by `systemctl is-active`: the word on stdout decides and the exit is
/// not read (a unit that does not exist answers `inactive` with exit 4); None when there is no word.
fn active(s: &mut impl System, unit: &Value) -> Option<bool> {
    let unit = unit
        .as_str()
        .filter(|u| !u.is_empty() && !u.starts_with('-'))?;
    let reply = s.call(
        "systemctl",
        &args(&["is-active", unit]),
        &Value::Null,
        10_000,
    );
    let word = String::from_utf8_lossy(&reply.ok()?.stdout)
        .trim()
        .to_owned();
    (!word.is_empty()).then(|| ALIVE.contains(&word.as_str()))
}
/// Whether a process of `ap unit` lives; None when the process table was not read (a failed call,
/// a non-zero exit or no output: `ps` lists at least itself).
fn unit_lives(s: &mut impl System) -> Option<bool> {
    let table = s
        .call("ps", &args(&["-eo", "args="]), &Value::Null, 10_000)
        .ok()
        .filter(|t| t.success && !t.stdout.trim_ascii().is_empty())?;
    let text = String::from_utf8_lossy(&table.stdout);
    Some(text.lines().any(|line| {
        let mut words = line.split_whitespace();
        let program = words.next().and_then(|p| p.rsplit('/').next());
        program == Some("ap") && words.next() == Some("unit")
    }))
}
fn git(s: &mut impl System, dir: &str, rest: &[&str]) -> Option<String> {
    let mut a = args(&[
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "-C",
        dir,
    ]);
    a.extend(args(rest));
    let reply = s.call("git", &a, &Value::Null, 30_000).ok()?;
    reply
        .success
        .then(|| String::from_utf8_lossy(&reply.stdout).into_owned())
}
/// The form of `<dir>/STATE.md` (R13): None when it is absent or not a regular file, else its line
/// count and the number of its lines that open a rendered block. Not read through a symbolic link.
fn state_md(root: &Path) -> Option<(usize, usize)> {
    let path = root.join("STATE.md");
    if !std::fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let file = options.open(&path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let (mut lines, mut blocks) = (0, 0);
    let mut reader = BufReader::new(file);
    let mut line = vec![];
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                lines += 1;
                blocks += usize::from(line.starts_with(BLOCK_OPENING));
            }
            Err(_) => return None,
        }
    }
    Some((lines, blocks))
}
fn modified(path: &Path) -> Value {
    let time = std::fs::metadata(path).and_then(|m| m.modified());
    time.map_or(Value::Null, |t: SystemTime| json!(stamp(t)))
}

/// `ap state <task directory> [--text]`: the state as one JSON object, or as lines.
pub fn execute(s: &mut impl System, argv: &[String]) -> Result<String, Failure> {
    let (mut dir, mut lines) = (None, false);
    for word in argv {
        match word.as_str() {
            "--text" if !lines => lines = true,
            w if dir.is_none() && !w.starts_with('-') => dir = Some(w),
            _ => return Err(error("usage", "args")),
        }
    }
    let dir = dir.ok_or_else(|| error("usage", "args"))?;
    let root = s.directory(dir).map_err(|_| error("usage", "args"))?;
    let runs = root.join("runs");
    // A directory without `runs/` has no record to read and nothing to ask `systemctl` or `ps`.
    let run_tree = runs.is_dir();
    let root_text = root.to_string_lossy().into_owned();
    let head = git(s, &root_text, &["rev-parse", "HEAD"]).map(|h| h.trim().to_owned());
    let uncommitted = git(s, &root_text, &["status", "--porcelain"]).map(|o| o.lines().count());

    let mut unreadable = vec![];
    let (calls, stage_records) = if run_tree {
        let calls = records(&runs, ".unit.json", &mut unreadable);
        (calls, records(&runs, ".run.json", &mut unreadable))
    } else {
        (vec![], vec![])
    };
    unreadable.sort();

    // The state of each stage record: live or not, for the ones that say they run. `ap run` writes
    // the record before it starts the unit: a record under 10 seconds old is never stale.
    let now = SystemTime::now();
    let young = stamp(now - Duration::from_secs(10));
    let mut stale: BTreeMap<&str, Option<bool>> = BTreeMap::new();
    for st in stage_records.iter().filter(|r| r.v["end"].is_null()) {
        let gone = active(s, &st.v["unit"]).map(|a| !a);
        stale.insert(
            &st.path,
            gone.filter(|g| !(*g && st.str("start") > &*young)),
        );
    }
    let table = calls
        .iter()
        .any(|c| c.str("state") == "running")
        .then(|| unit_lives(s));
    let call_stale = |c: &Rec| -> Value {
        if c.str("state") != "running" {
            return Value::Null;
        }
        let mut owned = stage_records.iter().filter(|st| c.owns(st));
        if owned.any(|st| stale.get(st.path.as_str()) == Some(&Some(false))) {
            return json!(false);
        }
        match table {
            Some(Some(false)) => json!(true),
            _ => Value::Null,
        }
    };

    let mut chains: Vec<Chain> = vec![];
    for c in &calls {
        let key = c.v["pull_request"]
            .as_u64()
            .map(|n| (c.str("repository").to_owned(), n));
        match chains.iter_mut().find(|(k, _)| key.is_some() && *k == key) {
            Some((_, list)) => list.push(c),
            None => chains.push((key, vec![c])),
        }
    }
    chains.sort_by(|a, b| {
        b.1.last()
            .map(|c| c.str("start"))
            .cmp(&a.1.last().map(|c| c.str("start")))
    });
    let units: Vec<Value> = chains
        .iter()
        .map(|(_, list)| {
            let item = list.iter().find_map(|c| c.v["item"]["key"].as_str());
            let calls: Vec<Value> = list
                .iter()
                .map(|c| {
                    json!({"record": c.path, "state": c.get("state"), "stopped_at": c.get("stopped_at"),
                        "cause": c.get("cause"), "head": c.get("head"), "merge_commit": c.get("merge_commit"),
                        "start": c.get("start"), "end": c.get("end"), "stale": call_stale(c)})
                })
                .collect();
            json!({"item": item, "repository": list[0].get("repository"),
                "pull_request": list[0].get("pull_request"), "calls": calls})
        })
        .collect();

    let mut listed: Vec<&Rec> = stage_records
        .iter()
        .filter(|r| r.v["end"].is_null())
        .collect();
    let unowned = stage_records
        .iter()
        .rev()
        .filter(|r| !r.v["end"].is_null() && !calls.iter().any(|c| c.owns(r)));
    listed.extend(unowned.take(20));
    listed.sort_by(|a, b| (b.str("start"), &b.path).cmp(&(a.str("start"), &a.path)));
    let stages: Vec<Value> = listed
        .iter()
        .map(|r| {
            let mut v = json!({"record": r.path, "role": r.get("role"), "runtime": r.get("runtime"),
                "model": r.get("model"), "account": r.get("account"), "state": r.get("state"),
                "start": r.get("start"), "end": r.get("end"),
                "stale": stale.get(r.path.as_str()).copied().flatten()});
            if r.v.get("served_by").is_some() {
                v["served_by"] = r.get("served_by");
            }
            v
        })
        .collect();

    let mut streams = vec![];
    let mut names: Vec<String> = std::fs::read_dir(&runs)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in &names {
        let Some(stem) = name.strip_suffix(".stream.jsonl") else {
            continue;
        };
        if !names.contains(&format!("{stem}.run.json")) {
            streams.push(
                json!({"path": format!("runs/{name}"), "modified": modified(&runs.join(name))}),
            );
        }
    }

    // The newest record with a present, non-null `limit` of each account, by `end`, else `start`.
    let mut newest: BTreeMap<&str, (&str, &Rec)> = BTreeMap::new();
    for r in stage_records.iter().filter(|r| !r.get("limit").is_null()) {
        let Some(account) = r.v["account"].as_str() else {
            continue;
        };
        let at = if r.v["end"].is_string() {
            r.str("end")
        } else {
            r.str("start")
        };
        if newest
            .get(account)
            .is_none_or(|(t, p)| (at, &r.path) >= (*t, &p.path))
        {
            newest.insert(account, (at, r));
        }
    }
    let limits: Vec<Value> = newest
        .iter()
        .map(|(account, (_, r))| json!({"account": account, "record": r.path, "limit": r.get("limit")}))
        .collect();

    let form = json!({"state_md": state_md(&root).map(|(lines, blocks)|
            json!({"lines": lines, "limit": STATE_LINES, "blocks": blocks})), "run_tree": run_tree});
    let v = json!({"at": stamp(now),
        "task": {"dir": root_text, "head": head, "uncommitted": uncommitted},
        "units": units, "stages": stages, "streams_without_record": streams,
        "limits": limits, "unreadable": unreadable, "form": form});
    Ok(match lines {
        true => text_of(&v),
        false => format!("{v}\n"),
    })
}
/// The plain form: the form line, then the lines of the base, at most 60 lines in all (R14).
fn text_of(v: &Value) -> String {
    let form = &v["form"];
    let tree = if form["run_tree"] == true {
        "present"
    } else {
        "absent"
    };
    let head = match form["state_md"].as_object() {
        None => format!("form: STATE.md absent, runs/ {tree}"),
        Some(m) => {
            let lines = m["lines"].as_u64().unwrap_or(0);
            let over = if lines > STATE_LINES as u64 {
                " over"
            } else {
                ""
            };
            format!(
                "form: STATE.md {lines} of {STATE_LINES} lines, {} rendered blocks, runs/ {tree}{over}",
                m["blocks"].as_u64().unwrap_or(0)
            )
        }
    };
    let rest = base_lines(v);
    head + "\n" + &rest.iter().map(|l| format!("{l}\n")).collect::<String>()
}
/// The lines of the base's plain form, cut so that with the form line they are at most 60.
fn base_lines(v: &Value) -> Vec<String> {
    let t = |v: &Value| match v {
        Value::String(s) => s.replace(['\n', '\r'], " "),
        Value::Null => "-".to_owned(),
        other => other.to_string(),
    };
    let short = |v: &Value| t(v).chars().take(12).collect::<String>();
    let word = |v: &Value| if v["stale"] == true { " stale" } else { "" };
    let mut out = vec![];
    for r in v["stages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["end"].is_null())
    {
        out.push(format!(
            "runs: stage {} {} {} {} account {} since {}{}",
            t(&r["record"]),
            t(&r["role"]),
            t(&r["runtime"]),
            t(&r["model"]),
            t(&r["account"]),
            t(&r["start"]),
            word(r)
        ));
    }
    let calls = v["units"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| c["calls"].as_array().into_iter().flatten());
    let mut running: Vec<&Value> = calls.filter(|c| c["state"] == "running").collect();
    running.sort_by(|a, b| b["start"].as_str().cmp(&a["start"].as_str()));
    for c in running {
        out.push(format!(
            "runs: unit call {} running since {}{}",
            t(&c["record"]),
            t(&c["start"]),
            word(c)
        ));
    }
    for chain in v["units"].as_array().into_iter().flatten().take(10) {
        let last = chain["calls"]
            .as_array()
            .and_then(|c| c.last())
            .cloned()
            .unwrap_or(Value::Null);
        let at = if last["merge_commit"].is_null() {
            &last["head"]
        } else {
            &last["merge_commit"]
        };
        out.push(format!(
            "chain {} pr {} {} {} end {}",
            t(&chain["item"]),
            t(&chain["pull_request"]),
            t(&last["state"]),
            short(at),
            t(&last["end"])
        ));
    }
    for l in v["limits"].as_array().into_iter().flatten() {
        let i = &l["limit"];
        out.push(format!(
            "limit {} {} {} resets_at {} ({})",
            t(&l["account"]),
            t(&i["status"]),
            t(&i["kind"]),
            t(&i["resets_at"]),
            t(&l["record"])
        ));
    }
    if out.len() > TEXT_LINES - 1 {
        let more = out.len() - (TEXT_LINES - 2);
        out.truncate(TEXT_LINES - 2);
        out.push(format!(
            "... {more} more lines: ap state without --text prints them all"
        ));
    }
    out
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let words: Option<Vec<String>> = argv.iter().map(|a| a.to_str().map(str::to_owned)).collect();
    let result = words.ok_or_else(|| error("usage", "args"));
    match result.and_then(|words| execute(&mut crate::host::Host, &words)) {
        Ok(out) => match std::io::stdout().lock().write_all(out.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(4),
        },
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}});
            match writeln!(std::io::stderr(), "{v}") {
                Ok(()) => ExitCode::from(2),
                Err(_) => ExitCode::from(4),
            }
        }
    }
}
