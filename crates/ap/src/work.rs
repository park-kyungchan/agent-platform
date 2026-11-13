//! One worker run; native programs own lifecycle and the store owns admission.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    process::{Command, ExitCode, Stdio},
    time::{Duration, Instant},
};

const LIMIT: u64 = 1_048_576;
const ROOT: &str = "/home/palantirkc/work/";
#[derive(Debug)]
pub struct Failure {
    pub code: &'static str,
    pub field: String,
    pub detail: Value,
}
fn fail(code: &'static str, field: &str, detail: Value) -> Failure {
    Failure {
        code,
        field: field.into(),
        detail,
    }
}
fn error(code: &'static str, field: &str) -> Failure {
    fail(code, field, Value::Null)
}
pub struct Reply {
    pub success: bool,
    pub bytes: Vec<u8>,
    pub stdout: Vec<u8>,
}
pub type Answer = Result<Reply, Failure>;
/// All subprocesses cross this boundary; tests supply literal replies and record calls.
pub trait System {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer;
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if !std::fs::metadata(path)?.is_file() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        std::fs::read(path)
    }
    fn exists(&self, path: &str) -> std::io::Result<bool> {
        Path::new(path).try_exists()
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).into()).collect()
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Failure> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error("invalid_input", key))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn id(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn file_id(kind: &str, path: &str) -> Result<Value, Failure> {
    Ok(id(
        kind,
        path.strip_prefix(ROOT)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| error("invalid_input", kind))?,
    ))
}
fn parse(reply: Reply, code: &'static str) -> Result<Value, Failure> {
    serde_json::from_slice(&reply.bytes).map_err(|_| error(code, "answer"))
}
fn acknowledged(reply: Answer) -> bool {
    reply.is_ok_and(|r| {
        r.success
            && serde_json::from_slice::<Value>(&r.stdout)
                .is_ok_and(|v| v.is_object() && v["result"].is_object() && v.get("error").is_none())
    })
}
fn codes(v: &Value) -> Value {
    let mut found: Vec<String> = v["result"]["criteria"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["passed"] == false)
        .filter_map(|c| c["code"].as_str().map(str::to_owned))
        .collect();
    found.extend(
        v["failed"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    if let Some(code) = v["code"].as_str() {
        found.push(code.to_owned());
    }
    json!({"codes":found})
}
fn ont(s: &mut impl System, r: &Value, command: &[&str], body: &Value) -> Result<Reply, Failure> {
    let mut a = args(command);
    if let Some(store) = r["store"].as_str() {
        a.extend(args(&["--store", store]));
    }
    s.call("ont", &a, body, 30_000)
}
macro_rules! herdr {
    ($s:expr, $timeout:expr; $($arg:expr),+ $(,)?) => {
        $s.call("herdr", &args(&[$($arg),+]), &Value::Null, $timeout)
    };
}
macro_rules! require {
    ($condition:expr => $code:expr, $field:expr) => {
        if !$condition {
            return Err(fail($code, $field, Value::Null));
        }
    };
}
fn agents(s: &mut impl System, code: &'static str, ms: u64) -> Result<Vec<Value>, Failure> {
    let reply = herdr!(s, ms; "agent", "list").map_err(|_| error(code, "agents"))?;
    if !reply.success {
        return Err(error(code, "agents"));
    }
    let v = parse(reply, code)?;
    require!(v.get("error").is_none() => code, "agents");
    v["result"]["agents"]
        .as_array()
        .cloned()
        .ok_or_else(|| error(code, "agents"))
}
fn search(s: &mut impl System, r: &Value, kind: &str, filter: Value) -> Result<Value, Failure> {
    let mut q = json!({"object_type":kind,"where":filter,"limit":200});
    let mut found = Value::Null;
    let mut pages = 0;
    loop {
        require!(pages < 20 => "store_search_unbounded", kind);
        pages += 1;
        let v = parse(ont(s, r, &["search"], &q)?, "store_answer_unreadable")?;
        if v["status"] != "ok" {
            return Err(fail("store_search_refused", kind, codes(&v)));
        }
        require!(v["result"]["basis"]["qualified"] == true => "store_search_unqualified", kind);
        for o in v["result"]["objects"].as_array().into_iter().flatten() {
            let ambiguous = if kind == "thread" {
                "thread_ambiguous"
            } else {
                "plan_ambiguous"
            };
            require!(found.is_null() => ambiguous, kind);
            found = o["id"].clone();
        }
        q["continuation"] = v["result"]["continuation"].clone();
        if q["continuation"].is_null() {
            break;
        }
    }
    let missing = if kind == "thread" {
        "thread_not_found"
    } else {
        "plan_not_found"
    };
    require!(!found.is_null() => missing, kind);
    Ok(found)
}
fn write_binding(pane: &str, tab: &str) -> Option<String> {
    let binding = format!("ap-work/1:{}", json!([pane, tab]));
    (!pane.is_empty() && !tab.is_empty() && binding.len() <= 200).then_some(binding)
}
fn read_binding(value: &Value) -> Option<(String, String)> {
    let encoded = value.as_str()?;
    let [pane, tab]: [String; 2] =
        serde_json::from_str(encoded.strip_prefix("ap-work/1:")?).ok()?;
    (write_binding(&pane, &tab)?.as_str() == encoded).then_some((pane, tab))
}
fn apply_answer(reply: Reply, code: &'static str) -> Result<Value, Failure> {
    let success = reply.success;
    let v = parse(reply, code)?;
    require!(success || refused(&v) => code, "apply");
    Ok(v)
}
fn applied_run<'a>(v: &'a Value, change: &str) -> Option<&'a Value> {
    let edits = v["result"]["edits"].as_array()?;
    let [edit] = edits.as_slice() else {
        return None;
    };
    (v["status"] == "ok"
        && v.get("error").is_none()
        && edit["change"] == change
        && complete_id(&edit["id"])
        && edit["id"]["object_type"] == "worker_run")
        .then_some(&edit["id"])
}
fn run_fields(s: &mut impl System, r: &Value, code: &'static str) -> Result<Value, Failure> {
    let reply = ont(s, r, &["get"], &json!({"id":r["run"]})).map_err(|_| error(code, "run"))?;
    require!(reply.success => code, "run");
    let v = parse(reply, code)?;
    require!(v["status"] == "ok" && v.get("code").is_none() && v.get("error").is_none() => code, "run");
    let objects = v["result"]["objects"]
        .as_array()
        .ok_or_else(|| error(code, "run"))?;
    require!(objects.len() == 1 && objects[0]["id"] == r["run"] => code, "run");
    let object = &objects[0];
    let head = text(object, "head").map_err(|_| error(code, "run"))?;
    let heads: Vec<_> = object["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["revision"] == head)
        .collect();
    require!(heads.len() == 1 && heads[0]["change"] != "retraction" && heads[0]["fields"].is_object() => code, "run");
    Ok(heads[0]["fields"].clone())
}
fn start(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let name = r["name"]
        .as_str()
        .ok_or_else(|| error("invalid_input", "name"))?;
    if name.is_empty()
        || name.len() > 32
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || !name.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(error("name_invalid", "name"));
    }
    let packet = text(r, "packet")?;
    let bytes = s
        .read(packet)
        .map_err(|_| error("packet_missing", "packet"))?;
    require!(!bytes.is_empty() => "packet_missing", "packet");
    require!(!s.exists(text(r,"report")?).map_err(|_| error("io_error","report"))? => "report_exists", "report");
    let mut worktree_head = Value::Null;
    let mut write_set = None;
    if let Some(w) = r["worktree"].as_str() {
        let dirty = s.call(
            "git",
            &args(&["-C", w, "status", "--porcelain"]),
            &Value::Null,
            5_000,
        )?;
        require!(dirty.success => "io_error", "worktree");
        require!(dirty.stdout.is_empty() || r["continue_dirty"] == true => "worktree_dirty", "worktree");
        let has = |prefix: &[u8]| bytes.split(|b| *b == b'\n').any(|l| l.starts_with(prefix));
        require!(has(b"Callers searched:") && has(b"Broken tests:") => "packet_lacks_callers_or_broken_tests_line", "packet");
        require!(has(b"Controls:") => "packet_lacks_controls_line", "packet");
        let mut lines = bytes
            .split(|b| *b == b'\n')
            .filter_map(|l| l.strip_prefix(b"Write set:"));
        let line = lines
            .next()
            .ok_or_else(|| error("packet_lacks_write_set_line", "packet"))?;
        require!(lines.next().is_none() => "write_set_invalid", "packet");
        let line = std::str::from_utf8(line).map_err(|_| error("write_set_invalid", "packet"))?;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let members: Vec<&str> = line
            .split(',')
            .map(|member| member.trim_matches([' ', '\t']))
            .collect();
        require!(members.len() <= 64 && members.iter().enumerate().all(|(index, member)| {
            let path = member.strip_suffix("/**").unwrap_or(member);
            !path.is_empty()
                && path.len() <= 297
                && path.bytes().all(|c| c.is_ascii_alphanumeric() || b"._/+@-".contains(&c))
                && path.as_bytes().first().is_some_and(|c| c.is_ascii_alphanumeric() || b"._".contains(c))
                && !members[..index].contains(member)
                && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
        }) => "write_set_invalid", "packet");
        write_set = Some(json!(members));
        let head = s
            .call(
                "git",
                &args(&["-C", w, "rev-parse", "HEAD"]),
                &Value::Null,
                5_000,
            )
            .map_err(|_| error("io_error", "worktree"))?;
        require!(head.success => "io_error", "worktree");
        worktree_head = json!(
            std::str::from_utf8(&head.stdout)
                .map_err(|_| error("io_error", "worktree"))?
                .trim_end()
        );
    }
    let task_key = Path::new(text(r, "task_dir")?)
        .file_name()
        .and_then(|s| s.to_str());
    let task = id(
        "task",
        task_key.ok_or_else(|| error("invalid_input", "task_dir"))?,
    );
    let thread = search(
        s,
        r,
        "thread",
        json!({"all":[{"eq":["task",task]},{"eq":["key",text(r,"thread")?]}]}),
    )?;
    let plan = search(s, r, "plan", json!({"eq":["thread",thread]}))?;
    let mut body = json!({"action_type":"start_worker_run","parameters":{"task":task,"name":name,"packet":file_id("packet",packet)?,"runtime":"codex","thread":thread,"plan":plan,"tier":text(r,"tier")?,"layer":text(r,"layer")?}});
    if let Some(write_set) = write_set {
        body["parameters"]["write_set"] = write_set;
    }
    let v = parse(
        ont(s, r, &["action", "validate"], &body)?,
        "store_answer_unreadable",
    )?;
    if v["status"] != "ok" || v["result"]["valid"] != true {
        return Err(fail("dispatch_stopped_by_store", "validate", codes(&v)));
    }
    require!(!(agents(s, "herdr_answer_unreadable", 5_000)?.iter().any(|a| a["name"] == name)) => "agent_exists", "name");
    let cwd = text(r, "cwd")?;
    let created = herdr!(s, 5_000; "tab", "create", "--workspace", text(r,"workspace")?, "--cwd", cwd, "--label", name, "--no-focus").and_then(|a| {
        require!(a.success => "tab_create_failed", "tab");
        parse(a, "tab_create_failed")
    }).map_err(|_| error("tab_create_failed","tab"))?;
    require!(created.get("error").is_none() => "tab_create_failed", "tab");
    let tab = created["result"]["tab"]["tab_id"]
        .as_str()
        .ok_or_else(|| error("tab_create_failed", "tab"))?;
    let pane = created["result"]["root_pane"]["pane_id"]
        .as_str()
        .ok_or_else(|| error("tab_create_failed", "pane"))?;
    let binding = write_binding(pane, tab).ok_or_else(|| error("tab_create_failed", "binding"))?;
    let context = json!({"tab":tab,"pane":pane});
    let launch = herdr!(s, 90_000; "agent", "start", name, "--kind", "codex", "--pane", pane, "--timeout", "90000", "--", "--no-daemon", "-C", cwd, "-m", text(r,"model")?, "-c", &format!("model_reasoning_effort={}",text(r,"effort")?), "--dangerously-bypass-approvals-and-sandbox");
    if !acknowledged(launch) {
        return Err(fail("agent_start_failed", "agent", context));
    }
    let deadline = s.now() + Duration::from_secs(60);
    let session = loop {
        let remaining = deadline.saturating_duration_since(s.now()).as_millis() as u64;
        if remaining == 0 {
            return Err(fail("agent_not_ready", "agent", context));
        }
        let list = agents(s, "agent_not_ready", remaining.min(5_000))
            .map_err(|_| fail("agent_not_ready", "agent", context.clone()))?;
        if s.now() >= deadline {
            return Err(fail("agent_not_ready", "agent", context));
        }
        if list
            .iter()
            .any(|a| a["name"] == name && a["pane_id"] != pane)
        {
            return Err(fail("agent_identity_conflict", "session", context));
        }
        if let Some(a) = list.iter().find(|a| {
            a["name"] == name
                && a["pane_id"] == pane
                && a["tab_id"] == tab
                && a["agent"] == "codex"
                && a["interactive_ready"] == true
        }) {
            break a["agent_session"]["value"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
        }
        s.pause(Duration::from_millis(250).min(deadline.saturating_duration_since(s.now())));
    };
    let request = format!("start-{name}-{}", &digest(&bytes)[..12]);
    body["parameters"]["session"] = json!(binding);
    body["request_id"] = json!(request);
    let applied = ont(s, r, &["action", "apply", "--client", "lead"], &body)
        .and_then(|a| apply_answer(a, "store_start_uncertain"));
    let applied = match applied {
        Ok(v) if refused(&v) => {
            let closed = acknowledged(herdr!(s, 5_000; "tab", "close", tab));
            let mut detail = codes(&v);
            detail["tab_closed"] = json!(closed);
            detail["tab"] = json!(tab);
            return Err(fail("store_start_refused_after_gate", "apply", detail));
        }
        Ok(v) => v,
        Err(_) => Value::Null,
    };
    let run =
        applied_run(&applied, "initial").filter(|_| applied["result"]["outcome"] == "applied");
    let Some(run) = run else {
        return Err(fail(
            "store_start_uncertain",
            "apply",
            json!({"request_id":request,"tab":tab,"pane":pane}),
        ));
    };
    let prompt = herdr!(s, 30_000; "agent", "prompt", pane, &format!("Read the file {packet} and do exactly what it says. Never open a question dialog and never wait for an answer: a point that only the Lead can decide goes into your report, and you continue with every step that does not depend on it."), "--wait", "--until", "working", "--until", "blocked", "--timeout", "30000");
    Ok(
        json!({"status":"started","run":run,"binding":binding,"session":session,"pane":pane,"tab":tab,"prompt_target":pane,"worktree_head":worktree_head,"prompt":if acknowledged(prompt) {"sent"} else {"uncertain"}}),
    )
}
fn close(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let fields = run_fields(s, r, "store_run_unreadable")?;
    require!(fields["status"]["value"] == "running" => "worker_run_not_running", "run");
    let (pane, tab) = read_binding(&fields["session"]["value"])
        .ok_or_else(|| error("run_binding_missing", "run"))?;
    let name = text(&fields["name"], "value").map_err(|_| error("store_run_unreadable", "name"))?;
    require!(r["name"] == name && r["tab"] == tab => "tab_not_owned", "run");
    let reply = herdr!(s, 5_000; "tab", "get", &tab)
        .map_err(|_| error("herdr_answer_unreadable", "tab"))?;
    require!(reply.success => "herdr_answer_unreadable", "tab");
    let v = parse(reply, "herdr_answer_unreadable")?;
    require!(v.get("error").is_none() => "herdr_answer_unreadable", "tab");
    require!(v["result"]["tab"]["tab_id"] == tab && v["result"]["tab"]["label"] == name && v["result"]["tab"]["pane_count"] == 1 => "tab_not_owned", "tab");
    let reply = herdr!(s, 5_000; "pane", "get", &pane)
        .map_err(|_| error("herdr_answer_unreadable", "pane"))?;
    require!(reply.success => "herdr_answer_unreadable", "pane");
    let v = parse(reply, "herdr_answer_unreadable")?;
    require!(v.get("error").is_none() => "herdr_answer_unreadable", "pane");
    require!(v["result"]["pane"]["pane_id"] == pane && v["result"]["pane"]["tab_id"] == tab => "tab_not_owned", "pane");
    let list = agents(s, "agent_status_unreadable", 5_000)?;
    if r["agent_gone"] == true {
        require!(!list.iter().any(|a| a["name"] == name || a["tab_id"] == tab || a["pane_id"] == pane) => "tab_not_owned", "tab");
    }
    let session = match list.iter().find(|a| a["name"] == name) {
        Some(a) => {
            require!(a["tab_id"] == tab && a["pane_id"] == pane => "tab_not_owned", "tab");
            match a["agent_status"].as_str() {
                Some("idle" | "done" | "blocked") => (),
                Some("working") => return Err(error("agent_still_working", "name")),
                _ => return Err(error("agent_status_unknown", "name")),
            }
            let session = a["agent_session"]["value"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| error("session_not_reported", "session"))?;
            if !r["session"].is_null() {
                require!(text(r, "session")? == session => "session_mismatch", "session");
            }
            session
        }
        None if r["agent_gone"] == true => "unavailable (agent_gone)",
        None => return Err(error("agent_not_listed", "name")),
    };
    let result = format!("{}; session={session}", text(r, "result")?);
    require!(result.len() <= 600 => "invalid_input", "result");
    let mut body = json!({"action_type":"close_worker_run","request_id":format!("close-{}", &digest(text(&r["run"],"native_key")?.as_bytes())[..32]),"parameters":{"worker_run":r["run"],"result":result}});
    if let Some(report) = r["report"].as_str() {
        body["parameters"]["report"] = file_id("report", report)?;
    }
    let v = ont(s, r, &["action", "apply", "--client", "lead"], &body)
        .and_then(|a| apply_answer(a, "store_close_uncertain"))
        .map_err(|_| fail("store_close_uncertain", "apply", Value::Null))?;
    if refused(&v) {
        return Err(fail("store_close_refused", "apply", codes(&v)));
    }
    require!(applied_run(&v, "update") == Some(&r["run"]) => "store_close_uncertain", "apply");
    let closed_fields = run_fields(s, r, "store_close_uncertain")?;
    require!(closed_fields["status"]["value"] == "closed" && closed_fields["result"]["value"] == result
        && closed_fields["name"] == fields["name"] && closed_fields["session"] == fields["session"] => "store_close_uncertain", "run");
    let closed = acknowledged(herdr!(s, 5_000; "tab","close",&tab));
    Ok(json!({"status":"closed","outcome":v["result"]["outcome"],"tab_closed":closed}))
}
fn wait(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let name = text(r, "name")?;
    if let Some(pane) = r.get("pane") {
        require!(pane.is_string() => "invalid_input", "pane");
    }
    let target = r["pane"].as_str().unwrap_or(name);
    let ms = r["timeout_seconds"]
        .as_u64()
        .and_then(|n| n.checked_mul(1000))
        .ok_or_else(|| fail("invalid_input", "timeout_seconds", Value::Null))?;
    let reply = herdr!(s, ms; "agent","wait",target,"--until","idle","--until","done","--until","blocked","--timeout",&ms.to_string());
    let status = match reply.and_then(|a| parse(a, "herdr_answer_unreadable")) {
        Ok(v) => v
            .pointer("/result/agent/agent_status")
            .or_else(|| v.pointer("/result/event/data/agent_status"))
            .cloned()
            .unwrap_or(Value::Null),
        Err(_) => Value::Null,
    };
    match s.read(text(r, "report")?) {
        Ok(bytes) if bytes.is_empty() => {
            Ok(json!({"status":"report_missing","agent_status":status}))
        }
        Ok(bytes) => Ok(
            json!({"status":"report_present","sha256":digest(&bytes),"bytes":bytes.len(),"agent_status":status}),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({"status":"report_missing","agent_status":status}))
        }
        Err(_) => Err(fail("io_error", "report", Value::Null)),
    }
}
/// Execute one validated JSON request; output and status are kept separate from I/O.
pub fn execute(s: &mut impl System, verb: &str, r: &Value) -> Result<(Value, u8), Failure> {
    if !r.is_object() {
        return Err(fail("invalid_input", "stdin", Value::Null));
    }
    for key in ["task_dir", "packet", "report", "cwd", "worktree", "store"] {
        if let Some(v) = r.get(key)
            && !v.as_str().is_some_and(|p| Path::new(p).is_absolute())
        {
            return Err(fail("invalid_input", key, Value::Null));
        }
    }
    for key in ["continue_dirty", "agent_gone"] {
        if let Some(v) = r.get(key) {
            require!(v.is_boolean() => "invalid_input", key);
        }
    }
    if verb == "close" {
        require!(complete_id(&r["run"]) && r["run"]["object_type"] == "worker_run" => "invalid_input", "run");
    }
    let required = match verb {
        "start" => "packet report task_dir thread tier layer cwd workspace model effort",
        "wait" => "name report",
        "close" => "name tab result",
        _ => return Err(fail("usage", "args", Value::Null)),
    };
    for key in required.split_whitespace() {
        text(r, key)?;
    }
    let value = match verb {
        "start" => start(s, r)?,
        "wait" => wait(s, r)?,
        "close" => close(s, r)?,
        _ => return Err(fail("usage", "args", Value::Null)),
    };
    let status = if value["status"] == "report_missing" || value["tab_closed"] == false {
        3
    } else {
        0
    };
    Ok((value, status))
}
fn complete_id(v: &Value) -> bool {
    [
        "/object_type",
        "/native_key",
        "/namespace/instance",
        "/namespace/account",
    ]
    .iter()
    .all(|p| {
        v.pointer(p)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    })
}
fn refused(v: &Value) -> bool {
    v.is_object() && (v["status"].as_str().is_some_and(|s| s != "ok") || v["code"].is_string())
}
pub struct Host;
impl System for Host {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        let error = || fail("process_failed", program, Value::Null);
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| error())?;
        let mut stdin = child.stdin.take().ok_or_else(error)?;
        let body = if input.is_null() {
            vec![]
        } else {
            serde_json::to_vec(input).map_err(|_| error())?
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(stdin.write_all(&body));
        });
        let (out_tx, out_rx) = std::sync::mpsc::channel();
        let stdout = child.stdout.take().ok_or_else(error)?;
        let stderr = child.stderr.take().ok_or_else(error)?;
        for (which, pipe) in [
            (true, Box::new(stdout) as Box<dyn Read + Send>),
            (false, Box::new(stderr) as Box<dyn Read + Send>),
        ] {
            let tx = out_tx.clone();
            std::thread::spawn(move || {
                let mut bytes = vec![];
                let result = pipe
                    .take(LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .map(|_| (which, bytes));
                let _ = tx.send(result);
            });
        }
        let deadline = Instant::now() + Duration::from_millis(ms);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    child.kill().map_err(|_| error())?;
                    child.wait().map_err(|_| error())?;
                    return Err(error());
                }
            }
        };
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| error())?
            .map_err(|_| error())?;
        let mut out = vec![];
        let mut err = vec![];
        for _ in 0..2 {
            let (which, bytes) = out_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| error())?
                .map_err(|_| error())?;
            if bytes.len() as u64 > LIMIT {
                return Err(error());
            }
            if which {
                out = bytes;
            } else {
                err = bytes;
            }
        }
        Ok(Reply {
            success: status.success(),
            bytes: if out.is_empty() { err } else { out.clone() },
            stdout: out,
        })
    }
}
pub fn cli(args: &[OsString]) -> ExitCode {
    let result = (|| {
        if args.len() != 1 {
            return Err(fail("usage", "args", Value::Null));
        }
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail("io_error", "stdin", Value::Null))?;
        if bytes.len() as u64 > LIMIT {
            return Err(fail("input_too_large", "stdin", Value::Null));
        }
        let r = serde_json::from_slice(&bytes)
            .map_err(|_| fail("invalid_input", "stdin", Value::Null))?;
        execute(&mut Host, args[0].to_str().unwrap_or(""), &r)
    })();
    let (value, status, error) = match result {
        Ok((v, status)) => (v, status, false),
        Err(e) => (
            json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}}),
            3,
            true,
        ),
    };
    let mut stream: Box<dyn Write> = if error {
        Box::new(std::io::stderr())
    } else {
        Box::new(std::io::stdout())
    };
    if writeln!(stream, "{value}").is_err() {
        return ExitCode::from(4);
    }
    ExitCode::from(status)
}
