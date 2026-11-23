//! `ap row`: one row of the user or of the Lead recorded and triaged in one call (README "One row").
use crate::host::{Failure, Host, LIMIT, System, apply_answer, codes, error, fail, id, ont};
use crate::host::{refused, require};
use crate::review::{field, objects};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Read, io::Write, process::ExitCode};
/// A refusal of the call: `after` is `record` once the record was applied.
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<&'static str>,
}
const COMMON: &str = "task request_id agent session topic text by";
/// The objects of `kind` of the task, as `ap backlog` searches them.
fn of(s: &mut impl System, ctx: &Value, kind: &str, task: &Value) -> Result<Vec<Value>, Failure> {
    Ok(objects(s, ctx, kind, vec![json!({"eq": ["task", task]})])?.0)
}
fn bad(f: &str) -> Failure {
    error("invalid_input", f)
}
fn req<'a>(v: &'a Value, k: &str, at: &str) -> Result<&'a str, Failure> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(&format!("{at}{k}")))
}
fn only(v: &Value, allowed: &str, at: &str) -> Result<(), Failure> {
    let obj = v.as_object().ok_or_else(|| bad(at.trim_end_matches('.')))?;
    let other = obj.keys().find(|k| !allowed.split(' ').any(|a| a == *k));
    other.map_or(Ok(()), |k| Err(bad(&format!("{at}{k}"))))
}
fn size(f: &str, v: &str, max: usize, code: &'static str) -> Result<(), Failure> {
    let detail = json!({"bytes": v.len(), "limit": max});
    (v.len() <= max)
        .then_some(())
        .ok_or_else(|| fail(code, f, detail))
}
/// The words: exactly one of `words`, `words_file`, `words_from`, never changed.
fn words(s: &impl System, r: &Value) -> Result<String, Failure> {
    let has = |k: &&str| r.get(*k).is_some();
    let given: Vec<_> = ["words", "words_file", "words_from"]
        .into_iter()
        .filter(has)
        .collect();
    require!(given.len() == 1 => "invalid_input", given.last().unwrap_or(&"words"));
    let file = |p: &str, f: &str| {
        s.read(p)
            .ok()
            .filter(|_| p.starts_with('/'))
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| bad(f))
    };
    let words = match given[0] {
        "words" => req(r, "words", "")?.to_owned(),
        "words_file" => file(req(r, "words_file", "")?, "words_file")?,
        _ => {
            only(&r["words_from"], "record at", "words_from.")?;
            let record = req(&r["words_from"], "record", "words_from.")?;
            let at = req(&r["words_from"], "at", "words_from.")?;
            require!(at.len() == 19 && at.as_bytes()[10] == b'T' && at.bytes().filter(u8::is_ascii_digit).count() == 14 => "invalid_input", "words_from.at");
            from_record(&file(record, "words_from.record")?, at)?
        }
    };
    require!(!words.is_empty() => "invalid_input", given[0]);
    Ok(words)
}
/// The one line of a session record at `at` that is a `user` line with a string `message.content`
/// or a `queue-operation` line (`enqueue`) with a string `content`: its string.
fn from_record(text: &str, at: &str) -> Result<String, Failure> {
    let mut found: Vec<String> = text
        .lines()
        .filter_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            let said = match v["type"].as_str()? {
                "user" => &v["message"]["content"],
                "queue-operation" if v["operation"] == "enqueue" => &v["content"],
                _ => return None,
            };
            v["timestamp"]
                .as_str()?
                .starts_with(at)
                .then(|| said.as_str().map(str::to_owned))?
        })
        .collect();
    let code = if found.is_empty() {
        "words_not_found"
    } else {
        "words_ambiguous"
    };
    (found.len() == 1)
        .then(|| found.remove(0))
        .ok_or_else(|| fail(code, "words_from", json!(found.len())))
}
/// `ont action validate`, then `apply`. `validate` cannot see the store's record of applied requests:
/// of a repeated one it finds the row current or triaged, and `apply` answers `already_applied`.
fn put(s: &mut impl System, ctx: &Value, body: &Value, f: &str) -> Result<Value, Failure> {
    let mut v = Value::Null;
    for op in ["validate", "apply"] {
        let reply = ont(s, ctx, &["action", op, "--client", "lead"], body)?;
        v = apply_answer(reply, "store_answer_unreadable")?;
        let found = codes(&v);
        let list = found["codes"].to_string();
        let again = [r#"["decision_already_current"]"#, r#"["already_triaged"]"#];
        let invalid = refused(&v) || v["result"]["valid"] == false;
        if invalid && !(op == "validate" && again.contains(&list.as_str())) {
            return Err(fail("store_refused", f, found));
        }
        require!(invalid || v["status"] == "ok" => "store_answer_unreadable", f);
    }
    Ok(v)
}
/// The parameters of `triage_decision` but the row.
fn triage_of(s: &mut impl System, ctx: &Value, task: &Value, t: &Value) -> Result<Value, Failure> {
    only(t, "word item objective reason", "triage.")?;
    let word = req(t, "word", "triage.")?;
    require!(["item", "intent", "not_adopted", "none"].contains(&word) => "invalid_input", "triage.word");
    let mut out = json!({"triage": word});
    if t.get("reason").is_some() || word.starts_with("no") {
        let why = req(t, "reason", "triage.")?;
        size("triage.reason", why, 600, "reason_too_long")?;
        out["reason"] = json!(why);
    }
    let refs = [
        ("item", "item", "key", "item_unknown"),
        ("objective", "intent", "name", "objective_unknown"),
    ];
    require!(refs.iter().all(|r| (word == r.1) == t.get(r.0).is_some()) => "invalid_input", "triage");
    for (k, _, name, unknown) in refs.into_iter().filter(|r| r.1 == word) {
        let key = req(t, k, "triage.")?;
        let found = of(s, ctx, k, task)?
            .into_iter()
            .find(|o| field(o, name) == key);
        out[k] = found.ok_or_else(|| error(unknown, "triage"))?["id"].clone();
    }
    Ok(out)
}
fn run(
    s: &mut impl System,
    store: Option<&str>,
    r: &Value,
    after: &mut Option<&'static str>,
) -> Result<Value, Failure> {
    let user = r["by"] == "user";
    require!(user || r["by"] == "lead" => "invalid_input", "by");
    let own = ["reason undo", "words words_file words_from form triage"][user as usize];
    let all = format!("{COMMON} {own}");
    only(r, &all, "")?;
    for k in [all.as_str(), COMMON][user as usize].split(' ') {
        req(r, k, "")?;
    }
    let g = |k: &str| r[k].as_str().unwrap_or_default();
    let (topic, text) = (g("topic"), g("text"));
    let form = r
        .get("form")
        .map_or(Ok("decision"), |_| req(r, "form", ""))?;
    let (ctx, task) = (json!({ "store": store }), id("task", g("task")));
    let mut triage = Value::Null;
    if user {
        triage = triage_of(s, &ctx, &task, &r["triage"])?;
    }
    let spoken = if user { words(s, r)? } else { String::new() };
    size("words", &spoken, 1000, "words_too_long")?;
    size("text", text, 1000, "text_too_long")?;
    size("reason", g("reason"), 600, "reason_too_long")?;
    size("undo", g("undo"), 300, "undo_too_long")?;
    let mut p = json!({"task": task, "topic": topic, "text": text, "decided_by": g("by")});
    let member = ["text", "user_words"][user as usize];
    if user {
        (p["user_words"], p["user_form"]) = (json!(spoken), json!(form));
    } else {
        (p["reason"], p["undo"]) = (json!(g("reason")), json!(g("undo")));
    }
    let body = |action: &str, suffix: &str, p: &Value| json!({"action_type": action, "request_id": format!("{}-{suffix}", g("request_id")), "agent": g("agent"), "session": g("session"), "role": "lead", "parameters": p});
    let applied = put(s, &ctx, &body("record_decision", "record", &p), "record")?;
    *after = Some("record");
    let rows = of(s, &ctx, "decision", &task)?;
    let row = rows
        .iter()
        .find(|d| field(d, "status") == "current" && field(d, "topic") == topic);
    let row = row.ok_or_else(|| error("store_answer_unreadable", "decision"))?;
    require!(field(row, member) == &p[member] => "readback_differs", member);
    if user {
        triage["decision"] = row["id"].clone();
        put(
            s,
            &ctx,
            &body("triage_decision", "triage", &triage),
            "triage",
        )?;
    }
    let bytes = p[member].as_str().unwrap_or_default().len();
    Ok(
        json!({"topic": topic, "decision": row["id"], "record": applied["result"]["outcome"], "triage": triage["triage"], "words_bytes": bytes}),
    )
}
fn refusal(f: Failure, after: Option<&'static str>) -> Refusal {
    let first = f.detail["codes"][0]
        .as_str()
        .filter(|_| f.code == "store_refused");
    Refusal {
        code: first.unwrap_or(f.code).into(),
        field: f.field,
        detail: f.detail,
        after,
    }
}
/// The call: the answer, or the refusal with how far the call went.
pub fn execute(s: &mut impl System, store: Option<&str>, r: &Value) -> Result<Value, Refusal> {
    let mut after = None;
    run(s, store, r, &mut after).map_err(|f| refusal(f, after))
}
/// The store option and the request of the command line and stdin.
fn input(argv: &[OsString]) -> Result<(Option<String>, Value), Failure> {
    let args: Vec<_> = argv.iter().map(|a| a.to_str()).collect();
    let store = match args.as_slice() {
        [] => None,
        [Some("--store"), Some(p)] if p.starts_with('/') => Some((*p).to_owned()),
        _ => return Err(error("usage", "args")),
    };
    let mut bytes = vec![];
    let read = std::io::stdin().take(LIMIT + 1).read_to_end(&mut bytes);
    require!(read.is_ok() && bytes.len() as u64 <= LIMIT => "input_too_large", "stdin");
    let r = serde_json::from_slice(&bytes).map_err(|_| bad("stdin"))?;
    Ok((store, r))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let called = input(argv).map_err(|f| refusal(f, None));
    let result = called.and_then(|(store, r)| execute(&mut Host, store.as_deref(), &r));
    let failed = |e: Refusal| json!({"error": {"code": e.code, "field": e.field, "detail": e.detail, "after": e.after}});
    let (written, status) = match result {
        Ok(v) => (writeln!(std::io::stdout(), "{v}"), 0),
        Err(e) => (writeln!(std::io::stderr(), "{}", failed(e)), 2),
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
