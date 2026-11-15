//! `ap backlog`: the board of a task, read only (README "The board"). Its reading of the store and
//! the refusal of a unit call for an item are also `ap unit`'s (README "One unit", `item`).
use crate::host::{Failure, Host, System, digest, error, fail, id, require, text};
use crate::review::{self, field, objects};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Write, process::ExitCode};

/// Room for the item's facts in the verifier's prompt: the schema's bounds of the six values and labels.
pub(crate) const ROOM: u64 = 60 + 200 + 400 + 300 + 60 + 600 + 200;

/// What a unit call and `ap backlog` read of one task: the objects of the store's searches.
pub(crate) struct Board {
    board: Option<Value>,
    items: Vec<Value>,
    objectives: Vec<Value>,
    decisions: Vec<Value>,
}
/// The objects of `kind` of the task: `task` is a property every object of the four types has.
fn of(s: &mut impl System, r: &Value, kind: &str, task: &Value) -> Result<Vec<Value>, Failure> {
    Ok(objects(s, r, kind, vec![json!({"eq": ["task", task]})])?.0)
}
fn find<'a>(list: &'a [Value], id: &Value) -> Option<&'a Value> {
    list.iter().find(|o| o["id"] == *id)
}
/// The board of the task `key`; with `all`, or when the task has a board, also its items,
/// objectives and decisions.
fn read(s: &mut impl System, r: &Value, key: &str, all: bool) -> Result<Board, Failure> {
    let task = id("task", key);
    let board = of(s, r, "board", &task)?.into_iter().next();
    let mut b = Board {
        board,
        items: vec![],
        objectives: vec![],
        decisions: vec![],
    };
    if all || b.board.is_some() {
        b.items = of(s, r, "item", &task)?;
        b.objectives = of(s, r, "objective", &task)?;
        b.decisions = of(s, r, "decision", &task)?;
    }
    Ok(b)
}
impl Board {
    fn item(&self, key: &str) -> Option<&Value> {
        self.items.iter().find(|i| field(i, "key") == key)
    }
    fn objective(&self, item: &Value) -> Option<&Value> {
        find(&self.objectives, field(item, "objective"))
    }
    /// The code a unit call for the item `key` from `at` (repository, branch) is refused with before
    /// any effect; None: it goes on (in work for `at`) or starts (queued: the store decides).
    fn refusal(&self, key: &str, at: Option<(&str, &str)>) -> Option<&'static str> {
        let Some(item) = self.item(key) else {
            return Some("item_unknown");
        };
        let state = field(item, "state");
        if state == "done" || state == "dropped" {
            return Some(if state == "done" {
                "item_done"
            } else {
                "item_dropped"
            });
        }
        let objective = self.objective(item);
        if objective.is_none_or(|o| field(o, "status") == "retired") {
            return Some("objective_not_current");
        }
        let source = objective.and_then(|o| find(&self.decisions, field(o, "source")));
        if source.is_none_or(|d| field(d, "status") != "current") {
            return Some("objective_source_superseded");
        }
        let own = at.is_some_and(|(repository, branch)| {
            field(item, "repository") == repository && field(item, "branch") == branch
        });
        (state == "in_work" && !own).then_some("item_of_another_unit")
    }
}
/// The first code of a store's refusal (`store_refused`), or the failure's own code.
fn code(f: &Failure) -> String {
    f.detail["codes"][0].as_str().unwrap_or(f.code).to_owned()
}
/// A unit call's item: the record's `item` and the block of facts for the verifier.
pub(crate) struct Item {
    pub(crate) record: Value,
    pub(crate) facts: String,
}
/// The item of a unit call, resolved before any other effect (README "One unit", `item`); a queued
/// item is started for the request's repository and branch. None: the task has no board.
pub(crate) fn resolve(s: &mut impl System, r: &Value) -> Result<Option<Item>, Failure> {
    let wanted = r["item"].as_str();
    let b = read(s, r, text(r, "task_key")?, false)?;
    let Some(board) = &b.board else {
        require!(wanted.is_none() => "item_not_ordered", "item");
        return Ok(None);
    };
    let wanted = wanted.ok_or_else(|| error("item_required", "item"))?;
    let at = (text(r, "repository")?, text(r, "branch")?);
    if let Some(code) = b.refusal(wanted, Some(at)) {
        return Err(error(code, "item"));
    }
    let item = b
        .item(wanted)
        .ok_or_else(|| error("item_unknown", "item"))?;
    let objective = b.objective(item);
    let objective = objective.ok_or_else(|| error("objective_not_current", "item"))?;
    if field(item, "state") == "queued" {
        // A start after a release is a new request: the item's and the board's revisions are in its id.
        let (i, o) = (&item["head"]["revision"], &board["head"]["revision"]);
        let input = format!("{}\n{}\n{}\n{i}\n{o}", item["id"]["native_key"], at.0, at.1);
        let request = format!("ap-start-{}", &digest(input.as_bytes())[..40]);
        let parameters = json!({"item": item["id"], "repository": at.0, "branch": at.1});
        review::apply(s, r, "start_item", request, parameters).map_err(|f| {
            let codes = f.detail["codes"].as_array();
            match codes.is_some_and(|c| c.contains(&json!("item_not_ordered"))) {
                true => error("item_not_ordered", "item"),
                false => fail("item_unwritten", "item", json!({"code": code(&f)})),
            }
        })?;
    }
    let f = |o: &Value, k: &str| field(o, k).as_str().unwrap_or_default().to_owned();
    let facts = format!(
        "\nItem of the board: {wanted}\nTitle: {}\nStatement: {}\nFirst use: {}\nObjective: {}\nObjective statement: {}\n",
        f(item, "title"),
        f(item, "statement"),
        f(item, "first_use"),
        f(objective, "name"),
        f(objective, "statement")
    );
    let record = json!({"key": wanted, "id": item["id"],
        "objective": {"name": field(objective, "name"), "id": objective["id"]}});
    Ok(Some(Item { record, facts }))
}
/// The state of the item of the record's `item`, read again (immediately before the merge).
pub(crate) fn state(s: &mut impl System, r: &Value, item: &Value) -> Result<String, Failure> {
    let task = id("task", text(r, "task_key")?);
    let all = vec![
        json!({"eq": ["task", task]}),
        json!({"eq": ["key", item["key"]]}),
    ];
    let (found, _) = objects(s, r, "item", all)?;
    let state = find(&found, &item["id"]).and_then(|i| field(i, "state").as_str());
    let state = state.ok_or_else(|| error("store_answer_unreadable", "item"))?;
    Ok(state.to_owned())
}
/// After the merge: `close_item` with the accepting review; the record's `item` gets `closed`
/// and, when the store did not apply it, the store's `code`.
pub(crate) fn close(s: &mut impl System, r: &Value, item: &mut Value, review: &Value) {
    let input = format!("{}\n{}", item["id"]["native_key"], review["native_key"]);
    let request = format!("ap-close-{}", &digest(input.as_bytes())[..40]);
    let parameters = json!({"item": item["id"], "review": review});
    match review::apply(s, r, "close_item", request, parameters) {
        Ok(_) => item["closed"] = json!(true),
        Err(f) => (item["closed"], item["code"]) = (json!(false), json!(code(&f))),
    }
}

/// `ap backlog <task key> [--text] [--store <path>]`: the board as one JSON object, or as lines.
pub fn execute(s: &mut impl System, argv: &[String]) -> Result<String, Failure> {
    let (mut key, mut lines, mut r) = (None, false, json!({}));
    let mut words = argv.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--text" if !lines => lines = true,
            "--store" if r.get("store").is_none() => {
                r["store"] = json!(words.next().ok_or_else(|| error("usage", "args"))?)
            }
            w if key.is_none() && !w.starts_with('-') => key = Some(w),
            _ => return Err(error("usage", "args")),
        }
    }
    let key = key.ok_or_else(|| error("usage", "args"))?;
    let shape =
        |(i, c): (usize, u8)| c.is_ascii_lowercase() || c.is_ascii_digit() || (i > 0 && c == b'-');
    require!(key.len() <= 64 && key.bytes().enumerate().all(shape) => "usage", "args");
    require!(r.get("store").is_none_or(|p| p.as_str().is_some_and(|p| p.starts_with('/'))) => "usage", "args");
    let b = read(s, &r, key, true)?;
    let v = board(s, &r, &b);
    Ok(match lines {
        true => text_of(&v),
        false => format!("{v}\n"),
    })
}
/// The JSON form of the board (README "The board").
fn board(s: &mut impl System, r: &Value, b: &Board) -> Value {
    let f = |o: &Value, k: &str| field(o, k).clone();
    let mut objectives: Vec<&Value> = b.objectives.iter().collect();
    objectives.sort_by_key(|o| field(o, "rank").as_i64());
    let objectives: Vec<_> = objectives.iter().map(|o| {
        let check = (!field(o, "last_checked_at").is_null()).then(|| {
            json!({"exit": f(o, "last_exit"), "line": f(o, "last_line"), "at": f(o, "last_checked_at")})
        });
        json!({"name": f(o, "name"), "rank": f(o, "rank"), "status": f(o, "status"),
            "statement": f(o, "statement"), "last_check": check})
    }).collect();
    let order = b.board.as_ref().map(|o| f(o, "order")).unwrap_or_default();
    let order = order.as_array().cloned().unwrap_or_default();
    let mut items: Vec<&Value> = b.items.iter().collect();
    items.sort_by_key(|i| (f(i, "added_at").to_string(), f(i, "key").to_string()));
    let unlisted = |state: &str| -> Vec<&Value> {
        let mine = |i: &&&Value| field(i, "state") == state && !order.contains(&i["id"]);
        items.iter().filter(mine).copied().collect()
    };
    let unplaced: Vec<_> = unlisted("queued")
        .into_iter()
        .map(|i| json!({"key": f(i, "key"), "title": f(i, "title"), "added_at": f(i, "added_at")}))
        .collect();
    let mut rows: Vec<&Value> = b
        .decisions
        .iter()
        .filter(|d| {
            field(d, "status") == "current"
                && field(d, "decided_by") == "user"
                && field(d, "triage").is_null()
        })
        .collect();
    rows.sort_by_key(|d| f(d, "decided_at").to_string());
    let topics: Vec<_> = rows.iter().take(10).map(|d| f(d, "topic")).collect();
    let untriaged = json!({"count": rows.len(), "topics": topics});
    let Some(set) = &b.board else {
        return json!({"board": null, "objectives": objectives, "unplaced": unplaced, "untriaged": untriaged});
    };
    let (mut listed, mut bad) = (vec![], vec![]);
    for element in &order {
        let Some(i) = find(&b.items, element) else {
            // Not an item of this task: of another task when the store can read it, else missing.
            let other = review::fields(s, r, element, "store_answer_unreadable", "order").is_ok();
            let problem = if other { "another_task" } else { "missing" };
            bad.push(json!({"id": element, "key": null, "problem": problem}));
            continue;
        };
        let (k, state) = (
            field(i, "key").as_str().unwrap_or_default(),
            field(i, "state"),
        );
        if state == "done" || state == "dropped" {
            bad.push(json!({"id": element, "key": k, "problem": state}));
        }
        let name = b.objective(i).map(|o| f(o, "name"));
        let mut e = json!({"key": k, "title": f(i, "title"), "state": state, "objective": name,
            "refusal": b.refusal(k, None)});
        if state == "in_work" {
            (e["repository"], e["branch"]) = (f(i, "repository"), f(i, "branch"));
        }
        listed.push(e);
    }
    let in_work: Vec<_> = unlisted("in_work")
        .into_iter()
        .map(|i| json!({"key": f(i, "key"), "title": f(i, "title"), "repository": f(i, "repository"), "branch": f(i, "branch")}))
        .collect();
    json!({"board": {"set_at": f(set, "set_at"), "reason": f(set, "reason")}, "objectives": objectives,
        "order": listed, "bad": bad, "unplaced": unplaced, "in_work_unlisted": in_work, "untriaged": untriaged})
}
/// The plain form: one line per objective, item and element, at most 80 lines.
fn text_of(v: &Value) -> String {
    let t = |v: &Value| match v {
        Value::String(s) => s.replace(['\n', '\r'], " "),
        Value::Null => "-".to_owned(),
        other => other.to_string(),
    };
    let list = |k: &str| v[k].as_array().cloned().unwrap_or_default();
    let mut out = vec![match v["board"].is_null() {
        true => "board: none (a unit call of this task names no item)".to_owned(),
        false => format!(
            "board set {}: {}",
            t(&v["board"]["set_at"]),
            t(&v["board"]["reason"])
        ),
    }];
    for o in list("objectives") {
        let check = match o["last_check"].is_null() {
            true => "no check recorded".to_owned(),
            false => format!(
                "last check exit {} at {}",
                t(&o["last_check"]["exit"]),
                t(&o["last_check"]["at"])
            ),
        };
        let (rank, name, status) = (t(&o["rank"]), t(&o["name"]), t(&o["status"]));
        out.push(format!(
            "objective {rank} {name} [{status}]: {}; {check}",
            t(&o["statement"])
        ));
    }
    for (n, i) in list("order").iter().enumerate() {
        let place = match i["state"] == "in_work" {
            true => format!(" in {} {}", t(&i["repository"]), t(&i["branch"])),
            false => String::new(),
        };
        let refusal = match i["refusal"].is_null() {
            true => String::new(),
            false => format!("; a unit call is refused: {}", t(&i["refusal"])),
        };
        let (key, state, title) = (t(&i["key"]), t(&i["state"]), t(&i["title"]));
        out.push(format!(
            "{}. {key} [{state}] {title} (objective {}){place}{refusal}",
            n + 1,
            t(&i["objective"])
        ));
    }
    for e in list("bad") {
        out.push(format!(
            "bad element {}: {}",
            t(&e["id"]["native_key"]),
            t(&e["problem"])
        ));
    }
    for i in list("unplaced") {
        out.push(format!(
            "unplaced {} since {}: {}",
            t(&i["key"]),
            t(&i["added_at"]),
            t(&i["title"])
        ));
    }
    for i in list("in_work_unlisted") {
        let (key, title) = (t(&i["key"]), t(&i["title"]));
        out.push(format!(
            "in work, not ordered: {key} {title} in {} {}",
            t(&i["repository"]),
            t(&i["branch"])
        ));
    }
    let u = &v["untriaged"];
    let topics: Vec<_> = u["topics"]
        .as_array()
        .into_iter()
        .flatten()
        .map(t)
        .collect();
    out.push(format!(
        "untriaged rows of the user: {} ({})",
        t(&u["count"]),
        topics.join(", ")
    ));
    if out.len() > 80 {
        let more = out.len() - 79;
        out.truncate(79);
        out.push(format!(
            "... {more} more lines: ap backlog without --text prints them all"
        ));
    }
    out.join("\n") + "\n"
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let words: Option<Vec<String>> = argv.iter().map(|a| a.to_str().map(str::to_owned)).collect();
    let result = words.ok_or_else(|| error("usage", "args"));
    match result.and_then(|words| execute(&mut Host, &words)) {
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
