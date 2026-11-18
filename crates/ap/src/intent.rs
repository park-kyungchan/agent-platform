//! `ap intent`: the intent of an objective drafted with its success criteria, amended, led, confirmed,
//! checked and closed, and shown (README "One intent", `specs/ap/intent.md`).
use crate::host::{Failure, Host, System, error, fail, id};
use crate::store::{self, Actor, Applied, Lead, at, bad, only, shaped, sized};
use serde_json::{Map, Value, json};
use std::{ffi::OsString, io::Write, process::ExitCode};

/// A refusal of the call: `after` the last step this call applied, `intent` the intent it names or
/// drafted, `criteria` the keys of the criteria it added.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<String>,
    pub intent: Option<String>,
    pub criteria: Vec<String>,
}
#[derive(Default)]
struct Done {
    after: Option<String>,
    intent: Option<String>,
    criteria: Vec<String>,
}
type Plain<S> = fn(&mut S, Option<&str>, &Actor<'_>, String, Value) -> Result<Applied, Failure>;
type Role<S> =
    fn(&mut S, Option<&str>, &Actor<'_>, String, Value, &Lead) -> Result<Applied, Failure>;
/// An action without the role, or one of the two that carry it (`assume_intent`, `confirm_intent_delegated`).
enum Act<S> {
    Plain(Plain<S>),
    Role(Role<S>),
}
/// One action of a writing `op`: the action, its request id suffix, its parameters, and the object
/// read back (`kind`, and its key, or none when the action creates it).
struct Plan<S> {
    act: Act<S>,
    suffix: &'static str,
    p: Value,
    kind: &'static str,
    key: Option<String>,
}
impl<S> Plan<S> {
    /// An action on the intent `key`, read back as the intent.
    fn on(act: Act<S>, suffix: &'static str, p: Value, key: &str) -> Self {
        let (kind, key) = ("intent", Some(key.to_owned()));
        Self {
            act,
            suffix,
            p,
            kind,
            key,
        }
    }
}
/// The texts of `draft` and `amend`, with the schema's bounds; the first two are required by `draft`.
const SUBSTANCE: [(&str, usize); 6] = [
    ("outcome", 1200),
    ("first_user", 600),
    ("why", 800),
    ("not_included", 800),
    ("constraints", 800),
    ("directory", 300),
];
/// A string of at most `max` bytes (an empty one is the store's to refuse), or none when not required.
fn text<'a>(
    v: Option<&'a Value>,
    f: &str,
    max: usize,
    required: bool,
) -> Result<Option<&'a str>, Failure> {
    match v {
        None if !required => Ok(None),
        Some(Value::String(t)) if t.len() <= max => Ok(Some(t)),
        Some(Value::String(t)) => Err(fail(
            "invalid_input",
            f,
            json!({"bytes": t.len(), "limit": max}),
        )),
        _ => Err(bad(f)),
    }
}
fn need<'a>(r: &'a Value, k: &str, max: usize) -> Result<&'a str, Failure> {
    Ok(text(r.get(k), k, max, true)?.unwrap_or_default())
}
/// A native key of 1 to 512 bytes.
fn key<'a>(r: &'a Value, k: &str) -> Result<&'a str, Failure> {
    sized(&r[k], k, 512)
}
/// The key of a new intent: 1 to 60 bytes of lower-case ASCII letters, digits and `-`, a letter first.
fn new_key(r: &Value) -> Result<&str, Failure> {
    let ok = |t: &&str| {
        (1..=60).contains(&t.len())
            && t.starts_with(|c: char| c.is_ascii_lowercase())
            && t.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    };
    r["intent"].as_str().filter(ok).ok_or_else(|| bad("intent"))
}
/// At most five questions, each with a nonempty `question` and `default`; `answer` empty when absent.
fn questions(q: &Value) -> Result<Value, Failure> {
    let list = q.as_array().ok_or_else(|| bad("questions"))?;
    if list.len() > 5 {
        return Err(fail("too_many_questions", "questions", json!(list.len())));
    }
    let one = |(i, q): (usize, &Value)| {
        let at = format!("questions.{i}.");
        only(q, "question default answer", &at)?;
        let filled = |k: &str| {
            q[k].as_str()
                .filter(|t| !t.is_empty())
                .ok_or_else(|| bad(&format!("{at}{k}")))
        };
        let answer = text(q.get("answer"), &format!("{at}answer"), usize::MAX, false)?;
        Ok(
            json!({"question": filled("question")?, "default": filled("default")?, "answer": answer.unwrap_or("")}),
        )
    };
    list.iter()
        .enumerate()
        .map(one)
        .collect::<Result<Vec<_>, Failure>>()
        .map(Value::from)
}
/// The substance of `draft` (`required`) or `amend` into the parameters `p`.
fn substance(r: &Value, p: &mut Map<String, Value>, required: bool) -> Result<(), Failure> {
    for (i, (k, max)) in SUBSTANCE.into_iter().enumerate() {
        if let Some(t) = text(r.get(k), k, max, required && i < 2)? {
            p.insert(k.into(), json!(t));
        }
    }
    if let Some(q) = r.get("questions") {
        p.insert("questions".into(), questions(q)?);
    }
    Ok(())
}
/// One criterion of intent `key`: `statement` 1 to 600 bytes, `check` 1 to 800, optional `baseline` to 300.
fn criterion(c: &Value, at: &str, key: &str) -> Result<Value, Failure> {
    let statement = sized(&c["statement"], &format!("{at}statement"), 600)?;
    let check = sized(&c["check"], &format!("{at}check"), 800)?;
    let mut p = json!({"intent": id("intent", key), "statement": statement, "check": check});
    if let Some(b) = text(c.get("baseline"), &format!("{at}baseline"), 300, false)? {
        p["baseline"] = json!(b);
    }
    Ok(p)
}
/// The criteria of `draft`: 1 to 12, each checked as `criterion` before anything is applied.
fn criteria(r: &Value, key: &str) -> Result<Vec<Value>, Failure> {
    let Some(list) = r.get("criteria") else {
        return Ok(vec![]);
    };
    let list = list.as_array().filter(|l| (1..=12).contains(&l.len()));
    let list = list.ok_or_else(|| bad("criteria"))?;
    let one = |(i, c): (usize, &Value)| {
        let at = format!("criteria.{i}.");
        only(c, "statement check baseline", &at)?;
        criterion(c, &at, key)
    };
    list.iter().enumerate().map(one).collect()
}
/// The action of a writing `op`, from the request alone: nothing is read or applied here.
fn plan<S: System>(op: &str, r: &Value) -> Result<Plan<S>, Failure> {
    let on = |k: &str| json!({"intent": id("intent", k)});
    let reason = |max| need(r, "reason", max);
    let k = || key(r, "intent");
    Ok(match op {
        "draft" | "amend" => {
            let k = if op == "draft" { new_key(r)? } else { k()? };
            let mut p = Map::from_iter([("intent".to_owned(), id("intent", k))]);
            substance(r, &mut p, op == "draft")?;
            if op == "amend" {
                return Ok(Plan::on(
                    Act::Plain(store::amend_intent),
                    "amend",
                    p.into(),
                    k,
                ));
            }
            p.insert("origin".into(), id("request", key(r, "origin")?));
            Plan::on(Act::Plain(store::draft_intent), "draft", p.into(), k)
        }
        "note" => {
            let mut p = on(k()?);
            p["notes"] = json!(need(r, "notes", 4000)?);
            Plan::on(Act::Plain(store::note_intent), "note", p, k()?)
        }
        "lead" => {
            let mut p = on(k()?);
            p["session"] = json!(shaped(r, "lead_session", 64, "._:-")?);
            p["by"] = id("request", key(r, "by")?);
            Plan::on(Act::Plain(store::name_lead), "lead", p, k()?)
        }
        "assume" | "close" | "retire" => {
            let mut p = on(k()?);
            p["reason"] = json!(reason(600)?);
            let (act, suffix) = match op {
                "assume" => (Act::Role(store::assume_intent as Role<S>), "assume"),
                "close" => (Act::Plain(store::close_intent as Plain<S>), "close"),
                _ => (Act::Plain(store::retire_intent as Plain<S>), "retire"),
            };
            Plan::on(act, suffix, p, k()?)
        }
        "confirm" => {
            let mut p = on(k()?);
            let act = match (r.get("by"), r.get("delegation"), r.get("reason")) {
                (Some(_), None, None) => {
                    p["by"] = id("request", key(r, "by")?);
                    Act::Plain(store::confirm_intent as Plain<S>)
                }
                (Some(_), None, Some(_)) => return Err(bad("reason")),
                (None, Some(_), _) => {
                    p["delegation"] = id("request", key(r, "delegation")?);
                    p["reason"] = json!(reason(600)?);
                    Act::Role(store::confirm_intent_delegated as Role<S>)
                }
                (Some(_), Some(_), _) => return Err(bad("delegation")),
                (None, None, _) => return Err(bad("by")),
            };
            Plan::on(act, "confirm", p, k()?)
        }
        "criterion" => {
            let (act, suffix, p, key): (Plain<S>, _, _, _) = match r["do"].as_str() {
                Some("add") => (
                    store::add_criterion,
                    "criterion",
                    criterion(r, "", k()?)?,
                    None,
                ),
                Some("check") => {
                    let state = r["state"].as_str().filter(|s| ["pass", "fail"].contains(s));
                    let p = json!({"criterion": id("criterion", key(r, "criterion")?),
                        "state": state.ok_or_else(|| bad("state"))?, "evidence": need(r, "evidence", 1000)?});
                    (
                        store::check_criterion,
                        "check",
                        p,
                        Some(key(r, "criterion")?),
                    )
                }
                _ => {
                    let p = json!({"criterion": id("criterion", key(r, "criterion")?),
                        "by": id("request", key(r, "by")?), "reason": reason(1000)?});
                    (
                        store::waive_criterion,
                        "waive",
                        p,
                        Some(key(r, "criterion")?),
                    )
                }
            };
            let (act, key) = (Act::Plain(act), key.map(str::to_owned));
            Plan {
                act,
                suffix,
                p,
                kind: "criterion",
                key,
            }
        }
        _ => {
            let k = key(r, "request")?;
            let p = json!({"request": id("request", k), "stopped_by": id("request", key(r, "stopped_by")?)});
            let (act, key) = (Act::Plain(store::end_delegation), Some(k.to_owned()));
            Plan {
                act,
                suffix: "end",
                p,
                kind: "request",
                key,
            }
        }
    })
}
/// The members each writing `op` admits besides `op`, `request_id`, `agent` and `session`.
fn members(op: &str, r: &Value) -> Result<&'static str, Failure> {
    Ok(match op {
        "draft" => {
            "intent outcome first_user origin why not_included constraints directory questions criteria"
        }
        "amend" => "intent outcome first_user why not_included constraints directory questions",
        "note" => "intent notes",
        "lead" => "intent lead_session by",
        "assume" | "close" | "retire" => "intent reason",
        "confirm" => "intent by delegation reason",
        "criterion" => match r["do"].as_str() {
            Some("add") => "do intent statement check baseline",
            Some("check") => "do criterion state evidence",
            Some("waive") => "do criterion by reason",
            _ => return Err(bad("do")),
        },
        "end_delegation" => "request stopped_by",
        _ => return Err(bad("op")),
    })
}
/// The role `lead` for `session` on intent `key`: only when the intent, read once, names that session
/// as its Lead; the revision read is the apply's `expected`.
fn lead(
    s: &mut impl System,
    store: Option<&str>,
    session: &str,
    key: &str,
) -> Result<Lead, Failure> {
    let read = at(store::head(s, store, "intent", key), "intent")?;
    let (revision, intent) = read.ok_or_else(|| error("unknown_intent", "intent"))?;
    if intent["lead_session"].as_str() != Some(session) {
        return Err(fail(
            "not_the_lead",
            "session",
            intent["lead_session"].clone(),
        ));
    }
    Ok(Lead {
        intent: id("intent", key),
        revision,
    })
}
/// The engine gives the role of `ONT_ROLE` to a request that carries none.
fn lead_in_environment(s: &impl System) -> bool {
    s.environment()
        .iter()
        .any(|(k, v)| k == "ONT_ROLE" && v == "lead")
}
/// One apply of a plan with the request id `<request_id>-<name>`, then its object read back: its key,
/// the outcome and the fields read.
#[allow(clippy::too_many_arguments)]
fn step<S: System>(
    s: &mut S,
    store: Option<&str>,
    actor: &Actor,
    (rid, name): (&str, String),
    plan: &Plan<S>,
    lead: Option<&Lead>,
    done: &mut Done,
) -> Result<(String, String, Value), Failure> {
    let (id_, p) = (format!("{rid}-{name}"), plan.p.clone());
    let applied = match (&plan.act, lead) {
        (Act::Plain(f), _) => f(s, store, actor, id_, p),
        (Act::Role(f), Some(lead)) => f(s, store, actor, id_, p, lead),
        (Act::Role(_), None) => return Err(error("not_the_lead", "session")),
    };
    let applied = at(applied, &name)?;
    let made = applied
        .made(plan.kind)
        .and_then(|i| i["native_key"].as_str());
    let key = plan.key.clone().or(made.map(str::to_owned));
    done.after = Some(name);
    let key = key.ok_or_else(|| error("store_answer_unreadable", plan.kind))?;
    let back = at(store::get(s, store, plan.kind, &key), plan.kind)?;
    let back = back.ok_or_else(|| error("store_answer_unreadable", plan.kind))?;
    Ok((key, applied.outcome, back))
}
/// A writing `op`: its plan, the role when it needs it, the apply, the read back and the answer.
fn write<S: System>(
    s: &mut S,
    store: Option<&str>,
    r: &Value,
    op: &str,
    done: &mut Done,
) -> Result<Value, Failure> {
    only(
        r,
        &format!("op request_id agent session {}", members(op, r)?),
        "",
    )?;
    done.intent = r["intent"].as_str().map(str::to_owned);
    let rid = shaped(r, "request_id", 44, "._-")?;
    let actor = Actor {
        agent: shaped(r, "agent", 64, "._:-")?,
        session: shaped(r, "session", 64, "._:-")?,
    };
    let plan: Plan<S> = plan(op, r)?;
    let added = match op {
        "draft" => criteria(r, plan.key.as_deref().unwrap_or_default())?,
        _ => vec![],
    };
    let lead = match (&plan.act, &plan.key) {
        (Act::Role(_), Some(k)) => Some(lead(s, store, actor.session, k)?),
        _ if lead_in_environment(s) => return Err(error("role_in_environment", "ONT_ROLE")),
        _ => None,
    };
    let name = plan.suffix.to_owned();
    let applied = step(s, store, &actor, (rid, name), &plan, lead.as_ref(), done);
    let (key, outcome, back) = match applied {
        Err(f)
            if op == "draft" && done.after.is_none() && store::code(&f) == "revision_conflict" =>
        {
            return Err(fail("intent_exists", "intent", f.detail));
        }
        other => other?,
    };
    if plan.kind == "criterion" {
        let intent = back["intent"]["native_key"].clone();
        return Ok(
            json!({"criterion": key, "intent": intent, "outcome": outcome, "state": back["state"]}),
        );
    }
    for (n, p) in added.into_iter().enumerate() {
        let (act, kind) = (Act::Plain(store::add_criterion), "criterion");
        let one = Plan::<S> {
            act,
            suffix: "criterion",
            p,
            kind,
            key: None,
        };
        let name = format!("criterion-{}", n + 1);
        let (criterion, ..) = step(s, store, &actor, (rid, name), &one, None, done)?;
        done.criteria.push(criterion);
    }
    let mut answer = json!({plan.kind: key, "outcome": outcome, "status": back["status"]});
    if op == "draft" {
        answer["criteria"] = json!(done.criteria);
    }
    Ok(answer)
}
/// `show`: the intent as the store holds it, with its criteria and the requests routed to it.
fn show(s: &mut impl System, store: Option<&str>, r: &Value) -> Result<Value, Failure> {
    only(r, "op intent", "")?;
    let key = key(r, "intent")?;
    let intent = at(store::get(s, store, "intent", key), "intent")?;
    let v = intent.ok_or_else(|| error("unknown_intent", "intent"))?;
    store::intent_shown(s, store, key, v)
}
/// The lines of `show --text`.
pub fn text_of(v: &Value) -> String {
    let g = |k: &str| v[k].as_str().unwrap_or_default();
    let mut out = format!("{} {}", g("intent"), g("status"));
    if v["confirmed"] == "by delegation" {
        out.push_str(" (confirmed by delegation)");
    } else if v["status"] == "assumed" {
        out.push_str(&format!(" (assumed: {})", g("assumed_reason")));
    }
    out.push_str(&format!(
        "\noutcome: {}\nfirst user: {}\n",
        g("outcome"),
        g("first_user")
    ));
    if v["not_included"].is_string() {
        out.push_str(&format!("not included: {}\n", g("not_included")));
    }
    for q in v["questions"].as_array().into_iter().flatten() {
        let q = |k: &str| q[k].as_str().unwrap_or_default().to_owned();
        out.push_str(&format!(
            "question: {} (default: {}; answer: {})\n",
            q("question"),
            q("default"),
            q("answer")
        ));
    }
    for c in v["criteria"].as_array().into_iter().flatten() {
        let c = |k: &str| c[k].as_str().unwrap_or_default().to_owned();
        out.push_str(&format!(
            "[{}] {} — {}\n",
            c("state"),
            c("statement"),
            c("check")
        ));
    }
    if v["notes"].is_string() {
        out.push_str(&format!("notes: {}\n", g("notes")));
    }
    out
}
fn run<S: System>(
    s: &mut S,
    store: Option<&str>,
    r: &Value,
    done: &mut Done,
) -> Result<Value, Failure> {
    match r["op"].as_str() {
        _ if !r.is_object() => Err(bad("stdin")),
        Some("show") => show(s, store, r),
        Some(op) => write(s, store, r, op, done),
        None => Err(bad("op")),
    }
}
fn refusal(f: Failure, done: Done) -> Box<Refusal> {
    Box::new(Refusal {
        code: store::code(&f),
        field: f.field,
        detail: f.detail,
        after: done.after,
        intent: done.intent,
        criteria: done.criteria,
    })
}
/// The call: the answer, or the refusal with how far the call went.
pub fn execute(s: &mut impl System, store: Option<&str>, r: &Value) -> Result<Value, Box<Refusal>> {
    let mut done = Done::default();
    run(s, store, r, &mut done).map_err(|f| refusal(f, done))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let called = store::input(argv, "show").map_err(|f| refusal(f, Done::default()));
    let result = called.and_then(|(store, text, r)| {
        let v = execute(&mut Host, store.as_deref(), &r)?;
        Ok(if text { text_of(&v) } else { format!("{v}\n") })
    });
    let (written, status) = match result {
        Ok(out) => (std::io::stdout().write_all(out.as_bytes()), 0),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail, "after": e.after,
                "intent": e.intent, "criteria": e.criteria}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
