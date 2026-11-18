//! `log`: the action log read, and the refusal records of `apply`.
//!
//! Applied rows come from the store's `action_log` table; refused rows from
//! `refusals.jsonl` beside the store, which `crate::action::apply_action`
//! appends to (`specs/ont/action.md` "Refusal records" and `specs/ont/log.md` "Reading the action log").
use crate::principal::GrantView;
use crate::refusal::{io, open_store, st};
use crate::{Answer, Context, Failure, Refusal};
use ont_core::schema::SchemaSet;
use ont_store::schema_mode::{ActionLogEntry, SchemaRead};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// Rows of the table, or lines of the file, examined per call.
pub const SCAN_LIMIT: usize = 10_000;
pub const DEFAULT_LIMIT: u64 = 100;
pub const MAX_LIMIT: u64 = 1_000;
/// A refusal line longer than this is skipped without being held.
const MAX_LINE: u64 = 1_048_576;
const CHUNK: u64 = 65_536;
const PAGE: usize = 256;
const CONTINUATION_PREFIX: &str = "l1:";

/// The file of refusal records beside the store.
pub fn refusals_path(store: &Path) -> PathBuf {
    match store.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join("refusals.jsonl"),
        _ => PathBuf::from("refusals.jsonl"),
    }
}

/// Append one line to `refusals.jsonl` under an exclusive lock on
/// `refusals.jsonl.lock`. Returns whether it was written; a failure is
/// silent, so the refusal's answer never changes.
pub(crate) fn append_refusal(store: &Path, line: &str) -> bool {
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let path = refusals_path(store);
    let flags = (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o600)
        .custom_flags(flags)
        .open(path.with_file_name("refusals.jsonl.lock"));
    let Ok(lock) = lock else { return false };
    if rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).is_err() {
        return false;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .mode(0o600)
        .custom_flags(flags)
        .open(&path);
    let Ok(mut file) = file else { return false };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    let caller_uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(u32::MAX);
    if meta.permissions().mode() & 0o777 != 0o600 || meta.uid() != caller_uid {
        return false;
    }
    let mut bytes = Vec::with_capacity(line.len() + 2);
    if meta.len() > 0 {
        let mut last = [0u8];
        if file.read_exact_at(&mut last, meta.len() - 1).is_err() {
            return false;
        }
        if last[0] != b'\n' {
            bytes.push(b'\n');
        }
    }
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    file.write_all(&bytes).is_ok()
}

/// The request of `ont log` and `ont_log`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRequest {
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub action_type: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub actor: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub parameters: Option<bool>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub continuation: Option<String>,
}
impl LogRequest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Refusal> {
        crate::cli::parse_request(bytes)
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` as seconds; `None` when not of that form or not a
/// real time.
fn parse_utc(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    let form = b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !form {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let secs = n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?;
    // Days from 1970-01-01 of the proleptic Gregorian date (civil algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = u64::try_from(days * 86_400 + secs).ok()?;
    // The round trip rejects a day, hour or minute out of range.
    (crate::action::utc_string(total) == s).then_some(total)
}

struct Filter {
    refused: bool,
    action_type: Option<String>,
    since: Option<u64>,
    until: Option<u64>,
    actor: BTreeMap<String, String>,
    parameters: bool,
}
impl Filter {
    fn from(req: &LogRequest) -> Result<Self, Failure> {
        let refused = match req.outcome.as_deref() {
            None | Some("applied") => false,
            Some("refused") => true,
            Some(_) => {
                return Err(
                    Refusal::input("invalid_request", "/outcome", "applied or refused").into(),
                );
            }
        };
        let time = |field: &str, value: &Option<String>| -> Result<Option<u64>, Failure> {
            match value {
                None => Ok(None),
                Some(s) => parse_utc(s).map(Some).ok_or_else(|| {
                    Refusal::input(
                        "invalid_request",
                        field,
                        "UTC time of the form 2026-11-17T00:00:00Z",
                    )
                    .into()
                }),
            }
        };
        Ok(Self {
            refused,
            action_type: req.action_type.clone(),
            since: time("/since", &req.since)?,
            until: time("/until", &req.until)?,
            actor: req.actor.clone().unwrap_or_default(),
            parameters: req.parameters.unwrap_or(false),
        })
    }
    fn matches(&self, action_type: &str, submitted_at: u64, actor: &Value) -> bool {
        self.action_type.as_deref().is_none_or(|t| t == action_type)
            && self.since.is_none_or(|s| submitted_at >= s)
            && self.until.is_none_or(|u| submitted_at < u)
            && self
                .actor
                .iter()
                .all(|(k, v)| actor.get(k).and_then(Value::as_str) == Some(v.as_str()))
    }
}

/// Whether the installed schema defines the action type in its domain.
fn defines(set: &SchemaSet, domain: &str, action_type: &str) -> bool {
    set.schema(domain)
        .is_some_and(|s| s.action_types.contains_key(action_type))
}

/// The admission of one row (`specs/ont/log.md` "Reading the action log").
fn admits(
    set: &SchemaSet,
    view: &GrantView,
    domain: &str,
    action_type: &str,
    edits: &[Value],
) -> bool {
    if defines(set, domain, action_type)
        && !view
            .entries
            .iter()
            .any(|e| e.domain == domain && e.actions.contains(action_type))
    {
        return false;
    }
    edits.iter().all(|edit| {
        let Some(ty) = edit["id"]["object_type"].as_str() else {
            return false;
        };
        set.schema_of(ty).is_some_and(|schema| {
            view.entries
                .iter()
                .any(|e| e.domain == schema.domain && e.read.contains(ty))
        })
    })
}

fn encode_continuation(refused: bool, position: u64) -> String {
    format!(
        "{CONTINUATION_PREFIX}{}{position}",
        if refused { 'r' } else { 'a' }
    )
}
fn decode_continuation(token: &str, refused: bool) -> Result<u64, Failure> {
    let digits = token
        .strip_prefix(CONTINUATION_PREFIX)
        .and_then(|t| t.strip_prefix(if refused { 'r' } else { 'a' }));
    digits
        .filter(|d| !d.is_empty() && d.len() <= 19 && d.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|d| d.parse::<u64>().ok())
        .filter(|p| refused || i64::try_from(*p).is_ok())
        .ok_or_else(|| {
            Refusal::input(
                "invalid_continuation",
                "/continuation",
                "the continuation of a previous log answer with the same outcome",
            )
            .into()
        })
}

/// What one call collected.
#[derive(Default)]
struct Page {
    rows: Vec<Value>,
    continuation: Option<String>,
    withheld: u64,
    skipped: u64,
    scan_limited: bool,
}

fn answer(set: &SchemaSet, revision: u64, page: Page) -> Answer {
    Answer::Enveloped {
        pins: set.pins().clone(),
        revision,
        result: json!({
            "rows":page.rows,
            "continuation":page.continuation,
            "withheld":page.withheld,
            "skipped":page.skipped,
            "scan_limited":page.scan_limited
        }),
    }
}

/// `ont log`: applied actions from the table or recorded refusals from
/// `refusals.jsonl`, newest first, admitted row by row.
pub fn log(ctx: &Context, req: &LogRequest) -> Result<Answer, Failure> {
    read_log(ctx, req, SCAN_LIMIT)
}

pub(crate) fn read_log(
    ctx: &Context,
    req: &LogRequest,
    scan_limit: usize,
) -> Result<Answer, Failure> {
    let limit = match req.limit {
        None => DEFAULT_LIMIT,
        Some(0) => {
            return Err(
                Refusal::input("invalid_request", "/limit", "integer from 1 to 1000").into(),
            );
        }
        Some(n) if n > MAX_LIMIT => {
            return Err(
                Refusal::input("limit_too_large", "/limit", "integer from 1 to 1000").into(),
            );
        }
        Some(n) => n,
    } as usize;
    let filter = Filter::from(req)?;
    let position = req
        .continuation
        .as_deref()
        .map(|t| decode_continuation(t, filter.refused))
        .transpose()?;
    let mut store = open_store(&ctx.store, false)?;
    if filter.refused {
        let set = st!(store.schema_set()).clone();
        let principal = crate::principal::Principal::bind(&set)?;
        let page = read_refused(
            ctx,
            &filter,
            &set,
            principal.view(),
            position,
            limit,
            scan_limit,
        )?;
        return Ok(answer(&set, principal.revision()?, page));
    }
    st!(store.read_schema(|tx| {
        let set = tx.schema_set();
        let principal = crate::principal::Principal::bind(set)?;
        let page = read_applied(
            tx,
            &filter,
            set,
            principal.view(),
            position,
            limit,
            scan_limit,
        )?;
        Ok(answer(set, principal.revision()?, page))
    }))
}

fn applied_row(e: &ActionLogEntry, parameters: bool) -> Value {
    let mut row = json!({
        "action_type":e.action_type,"domain":e.domain,"actor":e.actor,
        "submitted_at":crate::action::utc_string(e.submitted_at),"outcome":e.outcome,
        "action_id":e.action_id,"edits":e.edits
    });
    if parameters {
        row["parameters"] = e.parameters.clone();
    }
    row
}

fn read_applied(
    tx: &SchemaRead<'_>,
    filter: &Filter,
    set: &SchemaSet,
    view: &GrantView,
    position: Option<u64>,
    limit: usize,
    scan_limit: usize,
) -> Result<Page, Failure> {
    let mut page = Page::default();
    let mut cursor = position.map(|p| p as i64);
    let mut buffer: VecDeque<(i64, ActionLogEntry)> = VecDeque::new();
    let mut examined = 0usize;
    loop {
        if page.rows.len() == limit || examined == scan_limit {
            if buffer.is_empty() {
                buffer.extend(st!(tx.action_log_before(cursor, 1)));
            }
            if let (false, Some(seq)) = (buffer.is_empty(), cursor) {
                page.continuation = Some(encode_continuation(false, seq as u64));
                page.scan_limited = page.rows.len() < limit;
            }
            return Ok(page);
        }
        if buffer.is_empty() {
            let want = PAGE.min(scan_limit - examined);
            buffer.extend(st!(tx.action_log_before(cursor, want)));
        }
        let Some((seq, entry)) = buffer.pop_front() else {
            return Ok(page);
        };
        examined += 1;
        cursor = Some(seq);
        if !filter.matches(&entry.action_type, entry.submitted_at, &entry.actor) {
            continue;
        }
        let edits = entry.edits.as_array().map(Vec::as_slice);
        if !edits.is_some_and(|edits| admits(set, view, &entry.domain, &entry.action_type, edits)) {
            page.withheld += 1;
            continue;
        }
        page.rows.push(applied_row(&entry, filter.parameters));
    }
}

/// Reads `refusals.jsonl` backwards, one complete line at a time.
struct Backward {
    file: std::fs::File,
    buf: Vec<u8>,
    start: u64,
}
impl Backward {
    /// The position of the last newline before `before`.
    fn rfind_newline(&mut self, mut before: u64) -> std::io::Result<Option<u64>> {
        loop {
            if before == 0 {
                return Ok(None);
            }
            if !(self.start < before && before <= self.start + self.buf.len() as u64) {
                let s = before.saturating_sub(CHUNK);
                self.buf.resize((before - s) as usize, 0);
                self.file.read_exact_at(&mut self.buf, s)?;
                self.start = s;
            }
            let rel = (before - self.start) as usize;
            if let Some(i) = self.buf[..rel].iter().rposition(|b| *b == b'\n') {
                return Ok(Some(self.start + i as u64));
            }
            before = self.start;
        }
    }
}

fn refused_row(line: &[u8], filter: &Filter) -> Option<(String, String, u64, Value, Value)> {
    let Ok(Value::Object(m)) = serde_json::from_slice::<Value>(line) else {
        return None;
    };
    let text = |k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
    let (action_type, domain, code) = (text("action_type")?, text("domain")?, text("code")?);
    let submitted_at = m.get("submitted_at").and_then(Value::as_u64)?;
    let actor = m.get("actor").filter(|a| a.is_object())?.clone();
    if m.get("outcome").and_then(Value::as_str) != Some("refused") {
        return None;
    }
    let mut row = Map::new();
    row.insert("action_type".into(), json!(action_type));
    row.insert("domain".into(), json!(domain));
    row.insert("actor".into(), actor.clone());
    row.insert(
        "submitted_at".into(),
        json!(crate::action::utc_string(submitted_at)),
    );
    row.insert("outcome".into(), json!("refused"));
    row.insert("code".into(), json!(code));
    for key in ["failed", "request_id"] {
        if let Some(v) = m.get(key) {
            row.insert(key.into(), v.clone());
        }
    }
    if filter.parameters {
        row.insert(
            "parameters".into(),
            m.get("parameters").cloned().unwrap_or(Value::Null),
        );
    }
    Some((action_type, domain, submitted_at, actor, Value::Object(row)))
}

fn read_refused(
    ctx: &Context,
    filter: &Filter,
    set: &SchemaSet,
    view: &GrantView,
    position: Option<u64>,
    limit: usize,
    scan_limit: usize,
) -> Result<Page, Failure> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut page = Page::default();
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32)
        .open(refusals_path(&ctx.store));
    let file = match opened {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(page),
        Err(_) => {
            return Err(Failure::Internal(crate::internal!(
                "refusals_log_unreadable"
            )));
        }
    };
    let len = io!(file.metadata(), "refusals_log_unreadable").len();
    let mut reader = Backward {
        file,
        buf: Vec::new(),
        start: 0,
    };
    let end = position.map_or(len, |p| p.min(len));
    // The newline that ends the next line to read; an incomplete last line
    // (no newline yet) is not read.
    let mut line_end = io!(reader.rfind_newline(end), "refusals_log_unreadable");
    let mut examined = 0usize;
    loop {
        let Some(nl) = line_end else {
            return Ok(page);
        };
        if page.rows.len() == limit || examined == scan_limit {
            // The next call reads the lines that end at or before `nl`.
            page.continuation = Some(encode_continuation(true, nl + 1));
            page.scan_limited = page.rows.len() < limit;
            return Ok(page);
        }
        let start = io!(reader.rfind_newline(nl), "refusals_log_unreadable").map_or(0, |p| p + 1);
        examined += 1;
        line_end = start.checked_sub(1);
        if nl - start > MAX_LINE {
            page.skipped += 1;
            continue;
        }
        let mut line = vec![0u8; (nl - start) as usize];
        io!(
            reader.file.read_exact_at(&mut line, start),
            "refusals_log_unreadable"
        );
        let Some((action_type, domain, submitted_at, actor, row)) = refused_row(&line, filter)
        else {
            page.skipped += 1;
            continue;
        };
        if !filter.matches(&action_type, submitted_at, &actor) {
            continue;
        }
        if !admits(set, view, &domain, &action_type, &[]) {
            page.withheld += 1;
            continue;
        }
        let mut row = row;
        // A refused row of a type the schema no longer defines has no edits
        // to judge its parameters by: they are never returned.
        if !defines(set, &domain, &action_type)
            && let Some(row) = row.as_object_mut()
        {
            row.remove("parameters");
            row.insert("parameters_withheld".into(), json!(true));
        }
        page.rows.push(row);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn utc_times_parse_only_in_their_one_form() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2026-11-17T00:00:00Z"), Some(1_794_873_600));
        assert_eq!(parse_utc("2024-02-29T23:59:59Z"), Some(1_709_251_199));
        for bad in [
            "2026-02-30T00:00:00Z",
            "2026-11-17T24:00:00Z",
            "2026-11-17 00:00:00Z",
            "2026-11-17T00:00:00",
            "1969-12-31T23:59:59Z",
            "+026-11-17T00:00:00Z",
        ] {
            assert_eq!(parse_utc(bad), None, "{bad}");
        }
        assert_eq!(
            crate::action::utc_string(1_794_873_600),
            "2026-11-17T00:00:00Z"
        );
    }

    // The table scan stops at its bound (`specs/ont/log.md` "Reading the action log").
    #[test]
    fn the_table_scan_stops_at_its_bound() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ont-log-unit-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("store.sqlite");
        let schema = dir.join("schema.json");
        std::fs::write(
            &schema,
            include_bytes!("../tests/fixtures/action-work.json"),
        )
        .unwrap();
        let s = store.display().to_string();
        let run = |args: &[&str], stdin: &str| {
            let mut all = vec!["--store", &s, "--now", "1000"];
            all.extend_from_slice(args);
            let all: Vec<String> = all.iter().map(|a| a.to_string()).collect();
            crate::run(&all, stdin.as_bytes(), 0, 0)
        };
        assert_eq!(
            run(&["schema", "install", schema.to_str().unwrap()], "").code,
            0
        );
        for n in 0..5 {
            let request = format!(
                r#"{{"action_type":"create_tag","parameters":{{"label":"t{n}"}},"request_id":"t{n}"}}"#
            );
            assert_eq!(run(&["action", "apply"], &request).code, 0);
        }
        let ctx = Context::new(store, 1000);
        let result = |req: &LogRequest| match read_log(&ctx, req, 2) {
            Ok(Answer::Enveloped { result, .. }) => result,
            _ => panic!("log failed"),
        };
        let mut req = LogRequest {
            action_type: Some("none".into()),
            ..LogRequest::default()
        };
        let mut calls = Vec::new();
        loop {
            let r = result(&req);
            assert_eq!(r["rows"], json!([]));
            calls.push((r["scan_limited"].clone(), r["continuation"].clone()));
            match r["continuation"].as_str() {
                Some(c) => req.continuation = Some(c.to_string()),
                None => break,
            }
        }
        assert_eq!(
            calls,
            [
                (json!(true), json!("l1:a4")),
                (json!(true), json!("l1:a2")),
                (json!(false), Value::Null)
            ]
        );
        let r = result(&LogRequest {
            limit: Some(2),
            ..LogRequest::default()
        });
        assert_eq!(r["rows"].as_array().map(Vec::len), Some(2));
        assert_eq!(r["scan_limited"], false);
        assert_eq!(r["continuation"], "l1:a4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn continuations_name_their_outcome() {
        assert_eq!(
            decode_continuation(&encode_continuation(true, 42), true).ok(),
            Some(42)
        );
        assert_eq!(
            decode_continuation(&encode_continuation(false, 7), false).ok(),
            Some(7)
        );
        for (token, refused) in [
            ("l1:a7", true),
            ("l1:r7", false),
            ("l1:a", false),
            ("l1:a-1", false),
            ("l1:a9223372036854775808", false),
            ("c1:61", false),
        ] {
            assert!(decode_continuation(token, refused).is_err(), "{token}");
        }
    }
}
