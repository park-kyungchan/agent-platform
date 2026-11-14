//! One stage of a unit: one headless Claude Code session in a transient systemd scope under
//! `agents.slice`, ended at its time bound, its exit or (a reviewer only) its first `result` event,
//! after which the whole scope is stopped. One record `<out>.run.json` per run; no other ledger, no store write.
use crate::work::{
    Failure, LIMIT, System, args, digest, error, fail, new_session_id, require, text,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{FileExt, MetadataExt},
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
/// Only the account's settings load: a worktree's `.claude/settings*.json`, which an earlier
/// stage may have written, cannot steer this one (shown live: its `env` reached the shell).
const SOURCES: &str = "user";
/// The exit of a run whose stage may have run but whose answer or record could not be written.
const WRITE_FAILED: u8 = 5;
/// The states `systemctl is-active` answers while something of a unit may live.
const ALIVE: [&str; 5] = [
    "active",
    "activating",
    "deactivating",
    "reloading",
    "refreshing",
];

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
    /// The session, account and record path of the earlier run this one continues.
    resume: Option<(String, String, String)>,
    /// The expected file as it was before the session's first launch.
    expect_before: Value,
    /// The environment of the CLI (and of `git`), without `CLAUDE_CONFIG_DIR`.
    base_env: Vec<(String, String)>,
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
            // The account named in the record must be the account that answers.
            require!(!k.starts_with("HERDR_") && !k.starts_with("CLAUDE_")
                && !k.starts_with("ANTHROPIC_") && k != "HOME" => "env_forbidden", "env");
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
/// What `systemctl is-active` answered for `unit`: false unless it answered a state in which
/// nothing of the unit lives (a call that fails, times out or prints nothing is no answer).
fn ended(s: &mut impl System, unit: &str) -> bool {
    let reply = s.call(
        "systemctl",
        &args(&["is-active", unit]),
        &Value::Null,
        10_000,
    );
    let state = reply.map(|r| String::from_utf8_lossy(&r.stdout).trim().to_owned());
    state.is_ok_and(|st| !st.is_empty() && !ALIVE.contains(&st.as_str()))
}
/// The fixed short list of this process's variables, then the request's `env`.
fn base_env(s: &impl System, extra: &[(String, String)]) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = s
        .environment()
        .into_iter()
        .filter(|(k, _)| KEEP.contains(&k.as_str()))
        .filter(|(k, _)| !extra.iter().any(|(name, _)| name == k))
        .collect();
    env.extend(extra.iter().cloned());
    env
}
/// `git -C <cwd> …` in the CLI's clean environment (no `GIT_DIR` of the caller), messages in C.
fn git(
    s: &mut impl System,
    env: &[(String, String)],
    cwd: &str,
    command: &[&str],
) -> Result<crate::work::Reply, Failure> {
    let mut a = vec!["-i".to_owned()];
    a.extend(
        env.iter()
            .filter(|(k, _)| k != "LC_ALL")
            .map(|(k, v)| format!("{k}={v}")),
    );
    a.extend(args(&["LC_ALL=C", "git", "-C", cwd]));
    a.extend(args(command));
    s.call("env", &a, &Value::Null, 10_000)
}
/// The expected file as it is now: its digest and modification time, or nulls when absent.
fn snapshot(path: &str) -> Value {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => json!({
            "path": path,
            "sha256": std::fs::read(path).ok().map(|b| digest(&b)),
            "modified": m.mtime().saturating_mul(1_000_000_000).saturating_add(m.mtime_nsec()),
        }),
        _ => json!({"path": path, "sha256": null, "modified": null}),
    }
}
/// Met only by a nonempty file this session wrote: one that lay there unchanged since before the
/// session's first launch does not meet it.
fn written(path: &str, before: &Value) -> bool {
    let now = snapshot(path);
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
        && (now["sha256"] != before["sha256"] || now["modified"] != before["modified"])
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
    let continue_dirty = flag(r, "continue_dirty")?;
    let resume = match r.get("resume") {
        None => None,
        Some(v) => {
            let keys = v.as_object().map(|o| o.keys().all(|k| k == "record"));
            require!(keys == Some(true) => "invalid_input", "resume");
            Some(path(v, "record").map_err(|_| error("invalid_input", "resume"))?)
        }
    };
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
    // Every run has its own files: nothing of an earlier run is run over.
    let names = [
        &out,
        &record_path(&out),
        &format!("{out}.stream.jsonl"),
        &format!("{out}.err"),
    ];
    require!(names.iter().all(|n| std::fs::symlink_metadata(n).is_err()) => "out_exists", "out");
    let earlier = match &resume {
        None => None,
        Some(file) => {
            let rec: Value = std::fs::read(file)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .ok_or_else(|| error("resume_record_missing", "resume"))?;
            let missing = |_| error("resume_record_missing", "resume");
            let (session, account) = (
                text(&rec, "session").map_err(missing)?,
                text(&rec, "account").map_err(missing)?,
            );
            require!(accounts.iter().any(|a| a == account) => "resume_account_differs", "account");
            require!(rec["cwd"] == cwd.as_str() => "resume_cwd_differs", "cwd");
            let fixes = role == "fixer" && (rec["role"] == "implementer" || rec["role"] == "fixer");
            require!(rec["role"] == role || fixes => "resume_role_differs", "role");
            // Whatever the record says (`failed` with `unmet: stop` too): nothing of its unit may live.
            if let Some(unit) = rec["unit"].as_str() {
                require!(ended(s, unit) => "run_still_active", "resume");
            }
            Some((
                rec.clone(),
                (session.to_owned(), account.to_owned(), file.clone()),
            ))
        }
    };
    if earlier.is_none() && role == "implementer" {
        // Fails closed: only "not a repository" lets an implementer start without the check. Git
        // gets the fixed short list only: the request's `env` (a `GIT_DIR`) cannot skip it.
        let clean = base_env(s, &[]);
        let inside = git(s, &clean, &cwd, &["rev-parse", "--is-inside-work-tree"])
            .map_err(|_| error("worktree_unreadable", "cwd"))?;
        if inside.success && inside.stdout == b"true\n" {
            let status = git(s, &clean, &cwd, &["status", "--porcelain"])
                .map_err(|_| error("worktree_unreadable", "cwd"))?;
            require!(status.success => "worktree_unreadable", "cwd");
            require!(status.stdout.is_empty() || continue_dirty => "worktree_dirty", "cwd");
        } else {
            let not_a_repository = !inside.success
                && String::from_utf8_lossy(&inside.bytes).contains("not a git repository");
            require!(not_a_repository => "worktree_unreadable", "cwd");
        }
    }
    // The reference for `expect.file` is the file before THIS run's launch; only a run that
    // continues the same unfinished stage (same role, earlier state not `ok`) takes the earlier one.
    let expect_before = match (&expect_file, &earlier) {
        (None, _) => Value::Null,
        (Some(f), Some((rec, _)))
            if rec["role"] == role
                && rec["state"] != "ok"
                && rec["expect_file_before"]["path"] == f.as_str() =>
        {
            rec["expect_file_before"].clone()
        }
        (Some(f), _) => snapshot(f),
    };
    let base_env = base_env(s, &env);
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
        resume: earlier.map(|(_, r)| r),
        expect_before,
        base_env,
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
/// The temporary name is unique per write and created exclusively, so it is never a file of
/// another run (whose `out` may be any name).
fn write(path: &str, bytes: &[u8]) -> Result<(), Failure> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for n in 0..8 {
        let tmp = format!("{path}.{}-{nanos}-{n}.ap-tmp", std::process::id());
        let created = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp);
        match created {
            Ok(mut f) => {
                let done = f.write_all(bytes).and_then(|()| f.sync_all());
                let done = done.and_then(|()| std::fs::rename(&tmp, path));
                if done.is_err() {
                    let _ = std::fs::remove_file(&tmp);
                }
                return done.map_err(|_| error("io_error", "out"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => break,
        }
    }
    Err(error("io_error", "out"))
}
fn write_record(out: &str, record: &Value) -> Result<(), Failure> {
    let mut bytes = serde_json::to_vec_pretty(record).map_err(|_| error("io_error", "record"))?;
    bytes.push(b'\n');
    write(&record_path(out), &bytes)
}
/// After an effect: the record through the rename, else written in place. False when neither
/// write succeeded.
fn persist(out: &str, record: &Value) -> bool {
    write_record(out, record).is_ok() || {
        let mut bytes = serde_json::to_vec_pretty(record).unwrap_or_default();
        bytes.push(b'\n');
        File::create(record_path(out)).is_ok_and(|mut f| f.write_all(&bytes).is_ok())
    }
}
/// What the stream of one run showed.
#[derive(Default)]
struct Seen {
    /// Keep only the first `result` (a reviewer's stage ends there).
    first_only: bool,
    /// Every `result` event in order; a session that left a shell can emit more than one.
    results: Vec<Value>,
    /// An assistant message used a tool: the session did work.
    worked: bool,
    /// A rate-limit event said `rejected`.
    rejected: bool,
}
impl Seen {
    fn done(&self) -> bool {
        self.first_only && !self.results.is_empty()
    }
    fn line(&mut self, line: &[u8]) {
        if self.done() {
            return;
        }
        let Ok(e) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        match e["type"].as_str() {
            Some("result") => self.results.push(e),
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
/// Reads what the CLI appended to its output since the last call, through the handle opened
/// before the launch (`read_at` leaves the CLI's file offset alone); whole lines only.
fn follow(file: &File, offset: &mut u64, partial: &mut Vec<u8>, seen: &mut Seen) {
    let mut chunk = vec![0; 1 << 16];
    // At most 16 MiB per call, so the caller's clock is read again.
    for _ in 0..256 {
        let n = match file.read_at(&mut chunk, *offset) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        *offset += n as u64;
        partial.extend_from_slice(&chunk[..n]);
        while let Some(at) = partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = partial.drain(..=at).collect();
            seen.line(&line);
        }
    }
}
/// The members of the record that come from the `result` events, and the state the LAST
/// result gives: turns and tokens are the session's totals; the answer and `expect` are the last.
fn judge(
    q: &Request,
    record: &mut Value,
    seen: &Seen,
    e: &Value,
    write_failed: &mut bool,
) -> &'static str {
    let sum = |pick: &dyn Fn(&Value) -> &Value| -> Value {
        seen.results
            .iter()
            .map(|r| pick(r).as_u64())
            .sum::<Option<u64>>()
            .map_or(Value::Null, Value::from)
    };
    record["turns"] = sum(&|r| &r["num_turns"]);
    record["tokens"] = json!({
        "input": sum(&|r| &r["usage"]["input_tokens"]),
        "output": sum(&|r| &r["usage"]["output_tokens"]),
        "cache_read": sum(&|r| &r["usage"]["cache_read_input_tokens"]),
        "cache_creation": sum(&|r| &r["usage"]["cache_creation_input_tokens"]),
    });
    let mut denials: Vec<Value> = vec![];
    for d in seen
        .results
        .iter()
        .flat_map(|r| r["permission_denials"].as_array().into_iter().flatten())
    {
        let d = json!({"tool_name": d["tool_name"], "tool_use_id": d["tool_use_id"]});
        if !denials.contains(&d) {
            denials.push(d);
        }
    }
    record["permission_denials"] = json!(denials);
    if let Some(id) = e["session_id"].as_str().filter(|id| !id.is_empty()) {
        record["session"] = json!(id);
    }
    let answer = e["result"].as_str().unwrap_or("");
    // A finished answer is never read as a cut: without `is_error`, the usage-limit text is a cut
    // only when the stream also holds a rejected rate-limit event.
    let cut = (limit_text(answer) && seen.rejected)
        || (e["is_error"] == true
            && (seen.rejected || e["api_error_status"] == 429 || limit_text(answer)));
    if cut {
        "cut"
    } else if e["is_error"] == true {
        "failed"
    } else if answer.is_empty() {
        record["unmet"] = json!("answer");
        "no_result"
    } else if write(&q.out, answer.as_bytes()).is_err() {
        *write_failed = true;
        record["unmet"] = json!("write");
        "failed"
    } else {
        record["answer"] = json!({"path": q.out, "sha256": digest(answer.as_bytes())});
        let first = answer.lines().next().unwrap_or("");
        if q.expect_file
            .as_deref()
            .is_some_and(|f| !written(f, &q.expect_before))
        {
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
enum End {
    Result,
    Exit,
    Timeout,
}
/// Stops the whole unit (every process in its control group), reaps the CLI and confirms the
/// unit is no longer active. False when that cannot be confirmed.
fn stop(s: &mut impl System, unit: &str, child: &mut Child) -> bool {
    let _ = s.call("systemctl", &args(&["stop", unit]), &Value::Null, 60_000);
    let mut reaped = false;
    for _ in 0..200 {
        if !matches!(child.try_wait(), Ok(None)) {
            reaped = true;
            break;
        }
        s.pause(Duration::from_millis(100));
    }
    if !reaped {
        let _ = child.kill();
        let _ = child.wait();
    }
    ended(s, unit)
}
fn limit_text(text: &str) -> bool {
    let t = text.to_lowercase();
    t.len() < 400
        && (t.contains("usage limit")
            || (t.contains("hit your") && t.contains("limit"))
            || t.contains("resets "))
}
/// One launch on one account. Returns the final record, whether the session did work, and
/// whether every write after the first effect succeeded.
fn attempt(
    s: &mut impl System,
    q: &Request,
    account: &str,
    (output, errs): &(File, File),
) -> (Value, bool, bool) {
    let resume = q.resume.as_ref().map(|(session, _, _)| session.as_str());
    let mut record = json!({
        "role": q.role, "runtime": "claude", "model": q.model, "effort": q.effort,
        "account": account, "session": resume, "cwd": q.cwd, "unit": null,
        "prompt": {"path": q.prompt, "sha256": q.prompt_sha256},
        "answer": null, "start": stamp(SystemTime::now()), "end": null, "state": "running",
        "turns": null,
        "tokens": {"input": null, "output": null, "cache_read": null, "cache_creation": null},
        "permission_denials": [], "results": 0, "expect_met": null,
        "expect_file_before": q.expect_before, "unmet": null,
        "resumes": q.resume.as_ref().map(|(_, _, file)| file),
    });
    let not_launched = |mut record: Value, unmet: &str| {
        record["state"] = json!("failed");
        record["unmet"] = json!(unmet);
        record["end"] = json!(stamp(SystemTime::now()));
        let saved = persist(&q.out, &record);
        (record, false, saved)
    };
    let (Ok(session), Ok(unit)) = (
        resume.map_or_else(|| new_session_id(s), |r| Ok(r.to_owned())),
        new_session_id(s),
    ) else {
        return not_launched(record, "launch");
    };
    let unit = format!("ap-run-{unit}.scope");
    record["session"] = json!(session);
    record["unit"] = json!(unit);
    // The run's own stream and error files, emptied for a next account's attempt.
    let empty = |mut f: &File| {
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.try_clone()
    };
    let (Ok(prompt), Ok(stdout), Ok(stderr)) = (File::open(&q.prompt), empty(output), empty(errs))
    else {
        return not_launched(record, "launch");
    };
    let reader = output;
    // The record names the session BEFORE the launch.
    if !persist(&q.out, &record) {
        return not_launched(record, "write");
    }
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
        "--setting-sources",
        SOURCES,
        "--settings",
        HOOKS_OFF,
        "--strict-mcp-config",
    ]);
    for m in &q.mcp {
        argv.extend(args(&["--mcp-config", m]));
    }
    argv.extend(args(&[
        if resume.is_some() {
            "--resume"
        } else {
            "--session-id"
        },
        &session,
    ]));
    let mut env = q.base_env.clone();
    env.push(("CLAUDE_CONFIG_DIR".into(), account.into()));
    let started = s.now();
    let mut child = match s.launch(&argv, &env, &q.cwd, [prompt, stdout, stderr]) {
        Ok(child) => child,
        Err(_) => return not_launched(record, "launch"),
    };
    let bound = Duration::from_secs(q.timeout);
    // A reviewer's stage ends at its first result; an implementer's or a fixer's only at the
    // CLI's exit or the bound, because a result is not final while a shell it started lives.
    let mut seen = Seen {
        first_only: q.role == "reviewer",
        ..Seen::default()
    };
    let (mut offset, mut partial) = (0, vec![]);
    let end = loop {
        let exited = !matches!(child.try_wait(), Ok(None));
        follow(reader, &mut offset, &mut partial, &mut seen);
        if seen.done() {
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
    let stopped = stop(s, &unit, &mut child);
    // Once more after the stop: what came in the last moment, and a last line without newline.
    follow(reader, &mut offset, &mut partial, &mut seen);
    if !partial.is_empty() {
        seen.line(&partial);
    }
    // A reviewer whose first result was written before the stop has ended at it.
    let timed_out = matches!(end, End::Timeout) && !seen.done();
    record["end"] = json!(stamp(SystemTime::now()));
    record["results"] = json!(seen.results.len());
    let mut write_failed = false;
    let state = match seen.results.last() {
        None if timed_out => "timeout",
        None => {
            record["unmet"] = json!("result_event");
            "failed"
        }
        Some(e) => {
            let judged = judge(q, &mut record, &seen, e, &mut write_failed);
            record["expect_met"] = json!(judged == "ok");
            // A session that never exits keeps its last answer, but its state is the bound's.
            if timed_out { "timeout" } else { judged }
        }
    };
    record["state"] = json!(state);
    if !stopped {
        // Something of the stage may still live: never `ok`.
        record["state"] = json!("failed");
        record["unmet"] = json!("stop");
    }
    let saved = persist(&q.out, &record);
    (record, seen.worked, saved && !write_failed)
}
/// One `ap run` request. A refusal (exit 2) comes before any effect and writes no file.
pub fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let q = request(s, r)?;
    // The claim: the record is created exclusively, so of two requests on one `out` started at
    // the same instant only one passes. Every effect comes after it.
    let create = |p: &str| {
        std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(p)
    };
    create(&record_path(&q.out)).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => error("out_exists", "out"),
        _ => error("out_unwritable", "out"),
    })?;
    let errors = format!("{}.err", q.out);
    let files = (create(&format!("{}.stream.jsonl", q.out)), create(&errors));
    let accounts = match &q.resume {
        Some((_, account, _)) => vec![account.clone()],
        None => q.accounts.clone(),
    };
    let (mut last, mut clean) = (Value::Null, true);
    if let (Ok(output), Ok(errs)) = files {
        let files = (output, errs);
        for (i, account) in accounts.iter().enumerate() {
            let (record, worked, saved) = attempt(s, &q, account, &files);
            clean &= saved;
            // A fresh run cut before it did any work (no tool ran) may go to the next account.
            let next =
                record["state"] == "cut" && q.resume.is_none() && !worked && i + 1 < accounts.len();
            last = record;
            if !next || !saved {
                break;
            }
        }
    } else {
        last = json!({"state": "failed", "unmet": "launch", "account": accounts[0]});
        let _ = persist(&q.out, &last);
        clean = false;
    }
    if std::fs::metadata(&errors).is_ok_and(|m| m.len() == 0) {
        let _ = std::fs::remove_file(&errors);
    }
    let status = match last["state"].as_str() {
        _ if !clean => WRITE_FAILED,
        Some("ok") => 0,
        Some("cut") => 75,
        _ => 3,
    };
    Ok((last, status))
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
