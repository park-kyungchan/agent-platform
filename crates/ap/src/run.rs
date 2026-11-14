//! One stage of a unit: one headless Claude Code session in a transient systemd scope under
//! `agents.slice`, ended at its first `result` event, its exit or its time bound, after which the
//! whole scope is stopped. One record `<out>.run.json` per run; no other ledger, no store write.
use crate::work::{
    Failure, LIMIT, System, args, digest, error, fail, new_session_id, require, text,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Child, ExitCode},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The Lead's own account: never an implementer's or a fixer's; a reviewer's only for integration.
const LEAD_ACCOUNT: &str = "/root/.claude";
const SLICE: &str = "agents.slice";
/// The default time bound of each role, in seconds.
const ROLES: [(&str, u64); 3] = [
    ("implementer", 5_400),
    ("fixer", 3_600),
    ("reviewer", 3_300),
];
const MODES: [&str; 6] = [
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "manual",
    "dontAsk",
    "plan",
];
/// The fixed short list of variables taken from this process; nothing else is inherited.
const KEEP: [&str; 10] = [
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "TERM", "TMPDIR", "TZ",
];
const MEMBERS: [&str; 17] = [
    "role",
    "runtime",
    "account",
    "next_accounts",
    "model",
    "effort",
    "permission_mode",
    "cwd",
    "prompt",
    "out",
    "timeout_seconds",
    "expect",
    "resume",
    "continue_dirty",
    "integration",
    "mcp_config",
    "env",
];
/// The unit's own bound runs this long past the request's: `ap run` stops the unit first, and the
/// bound still ends it when `ap run` is gone.
const GRACE: u64 = 10;
const MAX_TIMEOUT: u64 = 86_400;
/// Switches off every hook (account, host, project, plugin) of the session.
const HOOKS_OFF: &str = r#"{"disableAllHooks":true}"#;

struct Request {
    role: String,
    accounts: Vec<String>,
    model: String,
    effort: String,
    mode: String,
    cwd: String,
    prompt: String,
    prompt_sha256: String,
    out: String,
    timeout: u64,
    expect_file: Option<String>,
    expect_line: Option<String>,
    mcp: Vec<String>,
    env: Vec<(String, String)>,
    /// The session and account of the record to continue.
    resume: Option<(String, String)>,
}

fn path(r: &Value, key: &str) -> Result<String, Failure> {
    let p = text(r, key)?;
    require!(Path::new(p).is_absolute() && !p.contains('\0') => "invalid_input", key);
    Ok(p.to_owned())
}
fn flag(r: &Value, key: &str) -> Result<bool, Failure> {
    match r.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(error("invalid_input", key)),
    }
}
fn word(r: &Value, key: &str, max: usize) -> Result<String, Failure> {
    let value = text(r, key)?;
    require!(value.len() <= max && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-[]".contains(&c)) => "invalid_input", key);
    Ok(value.to_owned())
}
fn paths(r: &Value, key: &str, max: usize) -> Result<Vec<String>, Failure> {
    let Some(v) = r.get(key) else {
        return Ok(vec![]);
    };
    let list = v.as_array().ok_or_else(|| error("invalid_input", key))?;
    require!(list.len() <= max => "invalid_input", key);
    list.iter()
        .map(|p| {
            p.as_str()
                .filter(|p| Path::new(p).is_absolute() && !p.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| error("invalid_input", key))
        })
        .collect()
}
fn is_lead(p: &Path) -> bool {
    p.components().eq(Path::new(LEAD_ACCOUNT).components())
}
/// The account rule: `/root/.claude`, by path components and after resolving, is refused for an
/// implementer and a fixer, and admitted for a reviewer only with `integration: true`.
fn admit(
    s: &impl System,
    role: &str,
    integration: bool,
    account: &str,
    field: &str,
) -> Result<(), Failure> {
    let allowed = role == "reviewer" && integration;
    require!(allowed || !is_lead(Path::new(account)) => "account_forbidden", field);
    let resolved = s
        .directory(account)
        .map_err(|_| error("account_missing", field))?;
    require!(allowed || !is_lead(&resolved) => "account_forbidden", field);
    Ok(())
}
fn environment(r: &Value) -> Result<Vec<(String, String)>, Failure> {
    let Some(v) = r.get("env") else {
        return Ok(vec![]);
    };
    let map = v.as_object().ok_or_else(|| error("invalid_input", "env"))?;
    require!(map.len() <= 32 => "invalid_input", "env");
    map.iter()
        .map(|(k, v)| {
            let name_ok = !k.is_empty()
                && k.len() <= 64
                && k.bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                && !k.as_bytes()[0].is_ascii_digit();
            require!(name_ok => "invalid_input", "env");
            require!(!k.starts_with("HERDR_") && !k.starts_with("CLAUDE_") => "env_forbidden", "env");
            let v = v
                .as_str()
                .filter(|v| !v.contains('\0') && v.len() <= 4096)
                .ok_or_else(|| error("invalid_input", "env"))?;
            Ok((k.clone(), v.to_owned()))
        })
        .collect()
}
fn record_path(out: &str) -> String {
    format!("{out}.run.json")
}
fn request(s: &mut impl System, r: &Value) -> Result<Request, Failure> {
    let object = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = object.keys().find(|k| !MEMBERS.contains(&k.as_str())) {
        return Err(error("invalid_input", k));
    }
    match r.get("runtime") {
        None => (),
        Some(v) if v == "claude" => (),
        Some(v) if v == "codex" => return Err(error("runtime_not_built", "runtime")),
        Some(_) => return Err(error("invalid_input", "runtime")),
    }
    let role = text(r, "role")?;
    let default = ROLES
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, t)| *t)
        .ok_or_else(|| error("invalid_input", "role"))?;
    let integration = flag(r, "integration")?;
    require!(!integration || role == "reviewer" => "invalid_input", "integration");
    let (resume, continue_dirty) = (flag(r, "resume")?, flag(r, "continue_dirty")?);
    let (model, effort) = (word(r, "model", 40)?, word(r, "effort", 16)?);
    let mode = text(r, "permission_mode")?;
    require!(MODES.contains(&mode) => "invalid_input", "permission_mode");
    let timeout = match r.get("timeout_seconds") {
        None => default,
        Some(v) => v
            .as_u64()
            .filter(|t| (1..=MAX_TIMEOUT).contains(t))
            .ok_or_else(|| error("invalid_input", "timeout_seconds"))?,
    };
    let (account, cwd, prompt, out) = (
        path(r, "account")?,
        path(r, "cwd")?,
        path(r, "prompt")?,
        path(r, "out")?,
    );
    let mut accounts = vec![account];
    accounts.extend(paths(r, "next_accounts", 4)?);
    let mcp = paths(r, "mcp_config", 8)?;
    let env = environment(r)?;
    let (mut expect_file, mut expect_line) = (None, None);
    if let Some(e) = r.get("expect") {
        let o = e
            .as_object()
            .filter(|o| !o.is_empty() && o.keys().all(|k| k == "file" || k == "first_line"))
            .ok_or_else(|| error("invalid_input", "expect"))?;
        if o.contains_key("file") {
            expect_file = Some(path(e, "file").map_err(|_| error("invalid_input", "expect"))?);
        }
        if o.contains_key("first_line") {
            let line = text(e, "first_line").map_err(|_| error("invalid_input", "expect"))?;
            require!(line.len() <= 200 && !line.contains('\n') => "invalid_input", "expect");
            expect_line = Some(line.to_owned());
        }
    }
    for (i, a) in accounts.iter().enumerate() {
        admit(
            s,
            role,
            integration,
            a,
            if i == 0 { "account" } else { "next_accounts" },
        )?;
    }
    s.directory(&cwd).map_err(|_| error("cwd_missing", "cwd"))?;
    let bytes = s
        .read(&prompt)
        .map_err(|_| error("prompt_unreadable", "prompt"))?;
    require!(!bytes.is_empty() && bytes.len() as u64 <= LIMIT => "prompt_unreadable", "prompt");
    let out_path = Path::new(&out);
    let name = out_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| error("invalid_input", "out"))?;
    let parent = out_path
        .parent()
        .and_then(|p| p.to_str())
        .ok_or_else(|| error("invalid_input", "out"))?;
    s.directory(parent)
        .map_err(|_| error("out_unwritable", "out"))?;
    require!(!out_path.is_dir() && out != prompt => "out_unwritable", "out");
    // A prompt that names the out file could make the session write the answer it is judged by.
    require!(!bytes.windows(name.len()).any(|w| w == name.as_bytes()) => "out_named_in_prompt", "prompt");
    let resume = if resume {
        let rec: Value = std::fs::read(record_path(&out))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or_else(|| error("resume_record_missing", "out"))?;
        let session = text(&rec, "session").map_err(|_| error("resume_record_missing", "out"))?;
        let account = text(&rec, "account").map_err(|_| error("resume_record_missing", "out"))?;
        require!(accounts.iter().any(|a| a == account) => "resume_account_differs", "account");
        if rec["state"] == "running" {
            let unit = text(&rec, "unit").map_err(|_| error("resume_record_missing", "out"))?;
            let active = s.call(
                "systemctl",
                &args(&["is-active", "--quiet", unit]),
                &Value::Null,
                10_000,
            );
            require!(!active.is_ok_and(|a| a.success) => "run_still_active", "out");
        }
        Some((session.to_owned(), account.to_owned()))
    } else {
        None
    };
    if resume.is_none() && role == "implementer" {
        let inside = s.call(
            "git",
            &args(&["-C", &cwd, "rev-parse", "--is-inside-work-tree"]),
            &Value::Null,
            10_000,
        );
        if inside.is_ok_and(|a| a.success && a.stdout.starts_with(b"true")) {
            let status = s.call(
                "git",
                &args(&["-C", &cwd, "status", "--porcelain"]),
                &Value::Null,
                10_000,
            )?;
            require!(status.success => "io_error", "cwd");
            require!(status.stdout.is_empty() || continue_dirty => "worktree_dirty", "cwd");
        }
    }
    require!(s.units() => "systemd_absent", "host");
    Ok(Request {
        role: role.to_owned(),
        accounts,
        model,
        effort,
        mode: mode.to_owned(),
        cwd,
        prompt,
        prompt_sha256: digest(&bytes),
        out,
        timeout,
        expect_file,
        expect_line,
        mcp,
        env,
        resume,
    })
}

/// UTC RFC 3339 to the second.
fn stamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to the civil date (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}
/// `*` matches any run of characters; every other character matches itself.
fn glob(pattern: &str, line: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), line.chars().collect());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if i < p.len() && p[i] == t[j] {
            i += 1;
            j += 1;
        } else if let Some(at) = star {
            i = at + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}
/// Writes through a temporary name and a rename: a reader never sees half a file.
fn write(path: &str, bytes: &[u8]) -> Result<(), Failure> {
    let tmp = format!("{path}.tmp");
    std::fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|_| error("io_error", "out"))
}
fn write_record(out: &str, record: &Value) -> Result<(), Failure> {
    let mut bytes = serde_json::to_vec_pretty(record).map_err(|_| error("io_error", "record"))?;
    bytes.push(b'\n');
    write(&record_path(out), &bytes)
}

/// What the stream of one run showed.
#[derive(Default)]
struct Seen {
    result: Option<Value>,
    /// An assistant message used a tool: the session did work.
    worked: bool,
    /// A rate-limit event said `rejected`.
    rejected: bool,
}
impl Seen {
    fn line(&mut self, line: &[u8]) {
        let Ok(e) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        match e["type"].as_str() {
            Some("result") if self.result.is_none() => self.result = Some(e),
            Some("assistant") => {
                let tools = e["message"]["content"].as_array().into_iter().flatten();
                self.worked |= tools.into_iter().any(|b| b["type"] == "tool_use");
            }
            Some("rate_limit_event") => {
                self.rejected |= e["rate_limit_info"]["status"] == "rejected";
            }
            _ => (),
        }
    }
}
/// Reads what the CLI appended to its output file since the last call; whole lines only.
fn follow(file: &str, offset: &mut u64, partial: &mut Vec<u8>, seen: &mut Seen) {
    let Ok(mut f) = std::fs::File::open(file) else {
        return;
    };
    let mut chunk = vec![];
    if f.seek(SeekFrom::Start(*offset)).is_err() || f.read_to_end(&mut chunk).is_err() {
        return;
    }
    *offset += chunk.len() as u64;
    partial.extend_from_slice(&chunk);
    while let Some(at) = partial.iter().position(|b| *b == b'\n') {
        let line: Vec<u8> = partial.drain(..=at).collect();
        seen.line(&line);
        if seen.result.is_some() {
            return;
        }
    }
}
enum End {
    Result,
    Exit,
    Timeout,
}
/// The processes in the unit other than the CLI at the moment of the stop; `None` when the
/// unit's control group cannot be read.
fn others(s: &mut impl System, unit: &str, cli: u32) -> Option<u64> {
    let reply = s
        .call(
            "systemctl",
            &args(&["show", "--property=ControlGroup", "--value", unit]),
            &Value::Null,
            10_000,
        )
        .ok()?;
    let group = String::from_utf8(reply.stdout).ok()?;
    let group = group.trim();
    if !reply.success {
        return None;
    }
    if group.is_empty() {
        // The unit is gone: everything in it has ended.
        return Some(0);
    }
    let procs = s
        .read(&format!("/sys/fs/cgroup{group}/cgroup.procs"))
        .ok()?;
    let text = String::from_utf8(procs).ok()?;
    Some(
        text.split_whitespace()
            .filter(|p| p.parse::<u32>().is_ok_and(|p| p != cli))
            .count() as u64,
    )
}
/// Stops the whole unit (every process in its control group) and reaps the CLI.
fn stop(s: &mut impl System, unit: &str, child: &mut Child) {
    let _ = s.call("systemctl", &args(&["stop", unit]), &Value::Null, 60_000);
    for _ in 0..200 {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        s.pause(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}
fn limit_text(text: &str) -> bool {
    let t = text.to_lowercase();
    t.len() < 400
        && (t.contains("usage limit")
            || (t.contains("hit your") && t.contains("limit"))
            || t.contains("resets "))
}
fn number(v: &Value) -> Value {
    v.as_u64().map_or(Value::Null, Value::from)
}
/// One launch on one account. Returns the final record and whether the session did work.
fn attempt(
    s: &mut impl System,
    q: &Request,
    account: &str,
    session: &str,
    resume: bool,
) -> Result<(Value, bool), Failure> {
    let unit = format!(
        "ap-run-{}.scope",
        new_session_id(s).map_err(|_| error("io_error", "unit"))?
    );
    let stream = format!("{}.stream.jsonl", q.out);
    let errors = format!("{}.err", q.out);
    let mut record = json!({
        "role": q.role, "runtime": "claude", "model": q.model, "effort": q.effort,
        "account": account, "session": session, "cwd": q.cwd, "unit": unit,
        "prompt": {"path": q.prompt, "sha256": q.prompt_sha256},
        "answer": null, "start": stamp(SystemTime::now()), "end": null, "state": "running",
        "turns": null,
        "tokens": {"input": null, "output": null, "cache_read": null, "cache_creation": null},
        "permission_denials": [], "others_at_stop": null, "unmet": null, "refusal": null,
    });
    write_record(&q.out, &record)?;
    let mut argv = args(&[
        "systemd-run",
        "--scope",
        "--quiet",
        "--collect",
        &format!("--unit={unit}"),
        &format!("--slice={SLICE}"),
        "-p",
        &format!("RuntimeMaxSec={}s", q.timeout + GRACE),
        "-p",
        "TimeoutStopSec=10s",
        "--",
        "claude",
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--model",
        &q.model,
        "--effort",
        &q.effort,
        "--permission-mode",
        &q.mode,
        "--permission-prompts",
        "none",
        "--settings",
        HOOKS_OFF,
        "--strict-mcp-config",
    ]);
    for m in &q.mcp {
        argv.extend(args(&["--mcp-config", m]));
    }
    argv.extend(args(&[
        if resume { "--resume" } else { "--session-id" },
        session,
    ]));
    let mut env: Vec<(String, String)> = s
        .environment()
        .into_iter()
        .filter(|(k, _)| KEEP.contains(&k.as_str()))
        .filter(|(k, _)| !q.env.iter().any(|(name, _)| name == k))
        .collect();
    env.extend(q.env.iter().cloned());
    env.push(("CLAUDE_CONFIG_DIR".into(), account.into()));
    let files = [Path::new(&q.prompt), Path::new(&stream), Path::new(&errors)];
    let started = s.now();
    let mut child = match s.launch(&argv, &env, &q.cwd, files) {
        Ok(child) => child,
        Err(_) => {
            record["state"] = json!("failed");
            record["unmet"] = json!("launch");
            record["end"] = json!(stamp(SystemTime::now()));
            write_record(&q.out, &record)?;
            return Ok((record, false));
        }
    };
    let bound = Duration::from_secs(q.timeout);
    let (mut offset, mut partial, mut seen) = (0, vec![], Seen::default());
    let end = loop {
        let exited = !matches!(child.try_wait(), Ok(None));
        follow(&stream, &mut offset, &mut partial, &mut seen);
        if seen.result.is_some() {
            break End::Result;
        }
        if exited {
            break if s.now().duration_since(started) >= bound {
                End::Timeout
            } else {
                End::Exit
            };
        }
        if s.now().duration_since(started) >= bound {
            break End::Timeout;
        }
        s.pause(Duration::from_millis(100));
    };
    record["others_at_stop"] = json!(others(s, &unit, child.id()));
    stop(s, &unit, &mut child);
    record["end"] = json!(stamp(SystemTime::now()));
    let state = match (end, &seen.result) {
        (End::Timeout, _) => "timeout",
        (End::Exit, _) | (End::Result, None) => {
            record["unmet"] = json!("result_event");
            "failed"
        }
        (End::Result, Some(e)) => {
            record["turns"] = number(&e["num_turns"]);
            let u = &e["usage"];
            record["tokens"] = json!({
                "input": number(&u["input_tokens"]), "output": number(&u["output_tokens"]),
                "cache_read": number(&u["cache_read_input_tokens"]),
                "cache_creation": number(&u["cache_creation_input_tokens"]),
            });
            record["permission_denials"] = json!(
                e["permission_denials"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| json!({"tool_name": d["tool_name"], "tool_use_id": d["tool_use_id"]}))
                    .collect::<Vec<_>>()
            );
            if let Some(id) = e["session_id"].as_str().filter(|id| !id.is_empty()) {
                record["session"] = json!(id);
            }
            let answer = e["result"].as_str().unwrap_or("");
            if e["is_error"] == true {
                let cut = seen.rejected || e["api_error_status"] == 429 || limit_text(answer);
                if cut { "cut" } else { "failed" }
            } else if answer.is_empty() {
                record["unmet"] = json!("answer");
                "no_result"
            } else {
                write(&q.out, answer.as_bytes())?;
                record["answer"] = json!({"path": q.out, "sha256": digest(answer.as_bytes())});
                let first = answer.lines().next().unwrap_or("");
                if q.expect_file.as_deref().is_some_and(|f| {
                    !std::fs::metadata(f).is_ok_and(|m| m.is_file() && m.len() > 0)
                }) {
                    record["unmet"] = json!("file");
                    "no_result"
                } else if q.expect_line.as_deref().is_some_and(|p| !glob(p, first)) {
                    record["unmet"] = json!("first_line");
                    "no_result"
                } else {
                    "ok"
                }
            }
        }
    };
    record["state"] = json!(state);
    write_record(&q.out, &record)?;
    Ok((record, seen.worked))
}
fn run(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let q = request(s, r)?;
    // An answer of an earlier run under this name never stays where this run's answer is expected.
    if std::fs::symlink_metadata(&q.out).is_ok() {
        std::fs::rename(&q.out, format!("{}.prev", q.out)).map_err(|_| error("io_error", "out"))?;
    }
    let accounts = match &q.resume {
        Some((_, account)) => vec![account.clone()],
        None => q.accounts.clone(),
    };
    let mut last = Value::Null;
    for (i, account) in accounts.iter().enumerate() {
        let session = match &q.resume {
            Some((session, _)) => session.clone(),
            None => new_session_id(s)?,
        };
        let (record, worked) = attempt(s, &q, account, &session, q.resume.is_some())?;
        // A fresh run cut before it did any work (no tool ran) may go to the next account.
        let next =
            record["state"] == "cut" && q.resume.is_none() && !worked && i + 1 < accounts.len();
        last = record;
        if !next {
            break;
        }
    }
    let status = match last["state"].as_str() {
        Some("ok") => 0,
        Some("cut") => 75,
        _ => 3,
    };
    Ok((last, status))
}
/// One `ap run` request. A refusal writes a record with state `refused` only where no record
/// exists, so a resumable record is never replaced by a refusal.
pub fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    run(s, r).inspect_err(|f| {
        let Some(out) = r["out"]
            .as_str()
            .filter(|o| Path::new(o).is_absolute() && !o.contains('\0'))
        else {
            return;
        };
        let parent_ok = Path::new(out).parent().is_some_and(Path::is_dir);
        if !parent_ok || std::fs::symlink_metadata(record_path(out)).is_ok() {
            return;
        }
        let now = stamp(SystemTime::now());
        let given = |k: &str| r.get(k).filter(|v| v.is_string()).cloned().unwrap_or(Value::Null);
        let _ = write_record(
            out,
            &json!({
                "role": given("role"), "runtime": r.get("runtime").cloned().unwrap_or(json!("claude")),
                "model": given("model"), "effort": given("effort"), "account": given("account"),
                "session": null, "cwd": given("cwd"), "unit": null,
                "prompt": {"path": given("prompt"), "sha256": null}, "answer": null,
                "start": now, "end": now, "state": "refused", "turns": null,
                "tokens": {"input": null, "output": null, "cache_read": null, "cache_creation": null},
                "permission_denials": [], "others_at_stop": null, "unmet": null,
                "refusal": {"code": f.code, "field": f.field},
            }),
        );
    })
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = (|| {
        require!(argv.is_empty() => "usage", "args");
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail("io_error", "stdin", Value::Null))?;
        require!(bytes.len() as u64 <= LIMIT => "input_too_large", "stdin");
        let r: Value = serde_json::from_slice(&bytes)
            .map_err(|_| fail("invalid_input", "stdin", Value::Null))?;
        execute(&mut crate::work::Host, &r)
    })();
    match result {
        Ok((v, status)) => {
            if writeln!(std::io::stdout(), "{v}").is_err() {
                return ExitCode::from(4);
            }
            ExitCode::from(status)
        }
        Err(e) => {
            let v = json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}});
            if writeln!(std::io::stderr(), "{v}").is_err() {
                return ExitCode::from(4);
            }
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{glob, limit_text, stamp};
    use std::time::{Duration, UNIX_EPOCH};
    #[test]
    fn stamps_are_utc_rfc3339() {
        assert_eq!(stamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_794_648_600);
        assert_eq!(stamp(t), "2026-11-14T09:30:00Z");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(stamp(leap), "2000-02-29T00:00:00Z");
    }
    #[test]
    fn first_line_patterns() {
        assert!(glob("Verdict: *", "Verdict: accept"));
        assert!(glob("Verdict: accept", "Verdict: accept"));
        assert!(!glob("Verdict: accept", "Verdict: accepted"));
        assert!(glob("*done", "all done"));
        assert!(glob("a*b*c", "aXbYc"));
        assert!(!glob("a*b*c", "aXbY"));
        assert!(!glob("Verdict: *", "verdict: accept"));
        assert!(glob("*", ""));
    }
    #[test]
    fn usage_limit_texts() {
        assert!(limit_text(
            "You have hit your 5-hour limit · resets 6:30pm (UTC)"
        ));
        assert!(!limit_text("API Error: 500 internal"));
    }
}
