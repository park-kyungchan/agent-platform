//! The status page (`specs/ap/status.md` R47 to R53): `ap status --page` against a fake of `ont` that answers the
//! reads of the verb, the action log and the functions `cost`, `capacity` and `api_spend`, with a clock the test
//! moves; and the systemd unit files and README section the page is written by. Every object is invented.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::{
    host::{Answer, Reply, System},
    status::{Call, execute, parse, text_of},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, ffi::OsString, time::Duration, time::Instant};

/// 2026-11-22T01:00:00Z: the week is 2026-11-16 to 2026-11-22, the month 2026-11.
const NOW: u64 = 1_795_309_200;
const STORE: &str = "/scratch/store";

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn reply(success: bool, v: Value) -> Answer {
    let bytes = serde_json::to_vec(&v).unwrap();
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
fn refused(code: &str) -> Answer {
    reply(false, json!({"status": "refused", "code": code}))
}

/// The fake store: objects, the action log, the answers of the functions and the clock.
struct P {
    objects: Vec<(String, String, Value)>,
    /// The applied rows of the action log, oldest first.
    log: Vec<Value>,
    /// The answers of `ont fn`, by `<name>` or `<name>:<intent>`: the answer, or the code of a refusal.
    functions: BTreeMap<String, Result<Value, &'static str>>,
    /// A verb the fake refuses with a code.
    refuse: BTreeMap<&'static str, &'static str>,
    /// Seconds the clock moves after a call, by `<verb>` or `fn:<name>[:<intent>]`.
    advance: BTreeMap<String, u64>,
    began: Instant,
    elapsed: Duration,
    calls: Vec<(Vec<String>, Value)>,
    stores: Vec<Option<String>>,
    reads: RefCell<Vec<String>>,
}
impl P {
    fn empty() -> Self {
        Self {
            objects: vec![],
            log: vec![],
            functions: BTreeMap::new(),
            refuse: BTreeMap::new(),
            advance: BTreeMap::new(),
            began: Instant::now(),
            elapsed: Duration::ZERO,
            calls: vec![],
            stores: vec![],
            reads: RefCell::new(vec![]),
        }
    }
    fn put(&mut self, kind: &str, key: &str, fields: Value) {
        self.objects.push((kind.into(), key.into(), fields));
    }
    fn ok(&self, result: Value) -> Answer {
        let now = NOW + self.calls.len() as u64 - 1;
        reply(
            true,
            json!({"status":"ok","now":now,"schema":{"work":{"version":19}},"result":result}),
        )
    }
    fn fields(o: &Value) -> Value {
        let mine = o.as_object().unwrap().iter();
        mine.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
            .collect()
    }
    fn search(&self, b: &Value) -> Answer {
        let kind = b["object_type"].as_str().unwrap();
        let holds = |o: &Value| {
            b["where"]["all"].as_array().unwrap().iter().all(|c| {
                match (c.get("eq"), c.get("in")) {
                    (Some(e), _) => o[e[0].as_str().unwrap()] == e[1],
                    (_, Some(i)) => i[1].as_array().unwrap().contains(&o[i[0].as_str().unwrap()]),
                    _ => panic!("filter {c}"),
                }
            })
        };
        let found: Vec<Value> = (self.objects.iter())
            .filter(|(k, _, o)| k == kind && holds(o))
            .map(|(k, key, o)| json!({"id": oid(k, key), "head": {"fields": Self::fields(o)}}))
            .collect();
        assert!(found.len() <= 15, "the fake answers one page");
        self.ok(json!({"basis": {"qualified": true}, "objects": found, "continuation": null}))
    }
    fn get(&self, b: &Value) -> Answer {
        let (kind, key) = (
            b["id"]["object_type"].as_str().unwrap(),
            b["id"]["native_key"].as_str().unwrap(),
        );
        let Some((_, _, o)) = (self.objects.iter()).find(|(k, n, _)| k == kind && n == key) else {
            return refused("unavailable");
        };
        let observation = json!({"revision": "r#1", "fields": Self::fields(o)});
        self.ok(
            json!({"objects": [{"id": b["id"], "head": "r#1", "observations": [observation]}]}),
        )
    }
    /// A view with no row: a merged unit of the fake has no review and no run.
    fn render(&self, b: &Value) -> Answer {
        let view = b["view"].as_str().unwrap();
        let md = format!(
            "<!-- ont:render {view} -->\n| Key | Kind |\n| --- | --- |\n<!-- /ont:render -->"
        );
        reply(
            true,
            json!({"markdown": md, "rows": 0, "schema": "work@19", "status": "ok", "truncated": false, "view": view}),
        )
    }
    fn log(&self, b: &Value) -> Answer {
        assert_eq!(b["outcome"], "applied");
        let action = b["action_type"].as_str().unwrap();
        let newest: Vec<&Value> = (self.log.iter().rev())
            .filter(|r| r["action_type"] == action)
            .collect();
        self.ok(json!({"rows": newest, "continuation": null, "scan_limited": false, "skipped": 0, "withheld": 0}))
    }
    /// `ont fn`: the engine's line, or its refusal; the clock moves as the test says.
    fn function(&mut self, name: &str, input: &Value) -> Answer {
        let mut key = name.to_owned();
        if let Some(i) = input["parameters"]["intent"].as_str() {
            key = format!("{name}:{i}");
        }
        if let Some(secs) = self.advance.get(&format!("fn:{key}")) {
            self.elapsed += Duration::from_secs(*secs);
        }
        match self.functions.get(&key) {
            Some(Ok(answer)) => reply(
                true,
                json!({"function": name, "domain": "work", "answer": answer, "elapsed_ms": 7}),
            ),
            Some(Err(code)) => refused(code),
            None => panic!("ont fn {key}: no answer set"),
        }
    }
    /// The verbs and functions asked, in order: `get`, `search`, `fn cost`, ...
    fn asked(&self) -> Vec<String> {
        let name = |c: &(Vec<String>, Value)| match c.0[0].as_str() {
            "fn" => format!("fn {}", c.0[1]),
            "log" => format!("log {}", c.1["action_type"].as_str().unwrap()),
            other => other.to_owned(),
        };
        self.calls.iter().map(name).collect()
    }
}
impl System for P {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        assert_eq!(program, "ont", "only the store is called");
        let mut args = args.to_vec();
        let store = args.iter().position(|a| a == "--store").map(|i| {
            let s = args[i + 1].clone();
            args.truncate(i);
            s
        });
        self.stores.push(store);
        self.calls.push((args.clone(), input.clone()));
        if args[0] == "fn" {
            assert_eq!(ms, 630_000, "one function is bound to 630 seconds");
            return self.function(&args[1], input);
        }
        if let Some(code) = self.refuse.get(args[0].as_str()) {
            return refused(code);
        }
        let answer = match args[0].as_str() {
            "get" => self.get(input),
            "search" => self.search(input),
            "render" => self.render(input),
            "log" => self.log(input),
            other => panic!("ont {other}: the page reads only"),
        };
        if let Some(secs) = self.advance.get(args[0].as_str()) {
            self.elapsed += Duration::from_secs(*secs);
        }
        answer
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_owned());
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn now(&self) -> Instant {
        self.began + self.elapsed
    }
    fn pause(&mut self, _: Duration) {}
}

fn intent(p: &mut P, key: &str, status: &str) {
    p.put(
        "intent",
        key,
        json!({"outcome": format!("Outcome of {key}."), "first_user": "An invented reader.", "status": status,
        "origin": oid("request", "q-start"), "amended_at": "2026-11-17T09:00:00Z"}),
    );
}
/// A merged unit of `intent`, started and merged at the times given.
fn merged(p: &mut P, key: &str, intent: &str, (from, to): (&str, &str)) {
    p.put(
        "unit",
        key,
        json!({"lane": "objective", "class": "plain", "objective": "An invented state.", "state": "merged",
        "intent": oid("intent", intent), "added_at": "2026-11-01T09:00:00Z", "amended_at": from}),
    );
    for (action, at) in [("start_unit", from), ("merge_unit", to)] {
        p.log.push(json!({"action_type": action, "outcome": "applied", "submitted_at": at,
            "parameters": {"unit": oid("unit", key)}, "actor": {"agent": "ap"}}));
    }
}
fn cost(groups: Value, total: f64, per: Option<f64>) -> Value {
    json!({"since": "2026-11-16", "until": "2026-11-22", "by": "role", "groups": groups, "total_usd": total,
        "merged": 3, "usd_a_pr": per, "lead_share": null, "unmeasured": 2})
}
fn capacity() -> Value {
    json!({"at": "2026-11-22T01:00:00Z",
        "hosts": [
            {"host": "local", "node": "vps", "machine": "vps", "answers": true, "stale": false, "free": true, "cpus": 8,
             "load_x100": 5, "available_mb": 4096, "seen_at": "2026-11-22T00:59:00Z", "observed_at": "2026-11-22T00:59:00Z"},
            {"host": "pc", "node": "a\nb", "machine": "pc", "answers": null, "stale": false, "free": null, "cpus": null,
             "load_x100": 1234, "available_mb": null, "seen_at": null, "observed_at": null},
            {"host": "idle", "node": null, "machine": "pc", "answers": true, "stale": true, "free": false, "cpus": 2,
             "load_x100": null, "available_mb": 10, "seen_at": "2026-11-22T00:00:00Z", "observed_at": null}],
        "machines": [
            {"machine": "vps", "hosts": ["local"], "up": true, "cpus": 8, "load_x100": 100, "available_mb": 4096, "free": true},
            {"machine": "pc", "hosts": ["pc"], "up": false, "cpus": null, "load_x100": null, "available_mb": null, "free": false}],
        "accounts": [
            {"account": "a1", "runtime": "claude", "allowance": "5h window 40 percent", "source": "run", "at": "2026-11-22T00:30:00Z"},
            {"account": "a2", "runtime": "codex", "allowance": {"used": 1, "of": [2, 3]}, "source": "stated", "at": "2026-11-21T00:00:00Z"},
            {"account": "a3", "runtime": "claude", "allowance": null, "source": null, "at": null}],
        "can_start": ["vps", "pc"]})
}
fn api() -> Value {
    json!({"month": "2026-11", "jobs": [
        {"job": "soak", "runs": 1, "running": 0, "settled_usd_micros": -5_000, "reserved_usd_micros": 4_999, "counted_usd_micros": 0},
        {"job": "verify", "runs": 2, "running": 1, "settled_usd_micros": 12_345_000, "reserved_usd_micros": 5_000, "counted_usd_micros": 12_350_000}],
        "settled_usd_micros": 12_340_000, "reserved_usd_micros": 9_999, "counted_usd_micros": 12_350_000})
}
/// Two open intents (`alpha`, `beta`), a closed one (`gamma`) and merged units in and out of the week.
fn fixture() -> P {
    let mut p = P::empty();
    intent(&mut p, "beta", "draft");
    intent(&mut p, "alpha", "confirmed");
    intent(&mut p, "gamma", "met");
    merged(&mut p, "u-edge", "alpha", ("2026-11-16T00:00:00Z", "2026-11-16T00:20:00Z"));
    merged(&mut p, "u-in", "alpha", ("2026-11-20T09:00:00Z", "2026-11-20T10:30:00Z"));
    // One second before the week.
    merged(&mut p, "u-out", "alpha", ("2026-11-15T23:29:59Z", "2026-11-15T23:59:59Z"));
    merged(&mut p, "u-b", "beta", ("2026-11-21T08:00:00Z", "2026-11-21T08:10:00Z"));
    merged(&mut p, "u-g", "gamma", ("2026-11-22T00:00:00Z", "2026-11-22T00:50:00Z"));
    let groups = json!([{"key": "implementer", "usd": 1.5, "messages": 3, "runs": 2}, {"key": null, "usd": 0.25, "messages": 1, "runs": 1}]);
    p.functions.insert("cost".into(), Ok(cost(groups.clone(), 9.0, Some(3.0))));
    p.functions.insert("cost:alpha".into(), Ok(cost(groups, 1.75, Some(0.88))));
    p.functions.insert("cost:beta".into(), Ok(cost(json!([]), 0.0, None)));
    p.functions.insert("capacity".into(), Ok(capacity()));
    p.functions.insert("api_spend".into(), Ok(api()));
    p
}
fn page(p: &mut P) -> Vec<String> {
    p.calls.clear();
    p.stores.clear();
    let v = match execute(p, Some(STORE), &Call::Page) {
        Ok(v) => v,
        Err(e) => panic!("{} {} {}", e.code, e.field, e.detail),
    };
    text_of(&Call::Page, &v).lines().map(str::to_owned).collect()
}
fn lines_of(call: &Call, p: &mut P) -> Vec<String> {
    p.calls.clear();
    let v = execute(p, Some(STORE), call).unwrap_or_else(|e| panic!("{} {}", e.code, e.field));
    text_of(call, &v).lines().map(str::to_owned).collect()
}
fn at(lines: &[String], line: &str) -> usize {
    (lines.iter().position(|l| l == line)).unwrap_or_else(|| panic!("no line {line:?} in {lines:#?}"))
}
fn args(a: &[&str]) -> Vec<OsString> {
    a.iter().map(OsString::from).collect()
}
fn ap(a: &[&str]) -> (Option<i32>, String, Value) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(a)
        .env("PATH", "/nonexistent")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let err = serde_json::from_slice(&out.stderr).unwrap_or(Value::Null);
    (out.status.code(), String::from_utf8(out.stdout).unwrap(), err)
}

// spec: specs/ap/status.md#R47
#[test]
fn the_status_page_unit_files_and_readme_section() {
    let service = include_str!("../../../deploy/ap-status-page.service");
    let lines: Vec<&str> = service.lines().filter(|l| !l.starts_with('#')).collect();
    for want in [
        "Type=oneshot",
        "Nice=10",
        "Environment=HOME=/root",
        "TimeoutStartSec=1290",
        "StateDirectory=agent-platform/status-page",
        "StateDirectoryMode=0755",
    ] {
        assert!(lines.contains(&want), "missing in service: {want}");
    }
    assert!(
        !service.contains("Restart="),
        "service must not set Restart="
    );

    let exec = lines
        .iter()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .expect("missing ExecStart= in service");
    assert!(exec.starts_with("/bin/sh -c "), "{exec}");
    assert!(exec.contains("/usr/local/bin/ap status --page"), "{exec}");
    assert!(!exec.contains("status --text"), "{exec}");
    assert!(
        exec.contains("/var/lib/agent-platform/status-page/status.txt.new"),
        "{exec}"
    );
    assert!(
        exec.contains("/var/lib/agent-platform/status-page/status.txt"),
        "{exec}"
    );
    for forbidden in ["${", "$$", "%"] {
        assert!(
            !exec.contains(forbidden),
            "ExecStart contains {forbidden}: {exec}"
        );
    }

    let timer = include_str!("../../../deploy/ap-status-page.timer");
    let timer_lines: Vec<&str> = timer.lines().filter(|l| !l.starts_with('#')).collect();
    for want in [
        "[Timer]",
        "OnCalendar=*:0/15",
        "AccuracySec=10s",
        "Persistent=false",
        "WantedBy=timers.target",
    ] {
        assert!(timer_lines.contains(&want), "missing in timer: {want}");
    }

    let readme = include_str!("../../../deploy/README.md");
    let (_, section) = readme
        .split_once("## The status page")
        .expect("missing '## The status page' section in deploy/README.md");
    let section = section
        .split_once("\n## ")
        .map_or(section, |(head, _)| head);
    assert!(section.contains("ap-status-page.service"));
    assert!(section.contains("ap-status-page.timer"));
    assert!(section.contains("tailscale funnel"));
    let save_pos = section
        .find("serve status --json")
        .expect("missing 'serve status --json' in status page section");
    let bg_pos = section
        .find("--bg")
        .expect("missing '--bg' in status page section");
    assert!(
        save_pos < bg_pos,
        "'serve status --json' must appear before '--bg'"
    );
}

// spec: specs/ap/status.md#R47
#[test]
fn r47_the_page_form_takes_only_a_store_else_usage_before_any_call() {
    for a in [
        vec!["--page", "--text"],
        vec!["--page", "vps"],
        vec!["--page", "--requests"],
        vec!["--page", "--term", "unit"],
        vec!["--page", "--wait", "vps"],
        vec!["--page", "--timeout", "30"],
        vec!["--page", "--indicators"],
        vec!["--page", "--brief", "vps"],
        vec!["--page", "--page"],
        vec!["--page", "--store", "relative"],
        vec!["--page", "--store"],
        vec!["--page", "-x"],
        vec!["--text", "--page"],
        vec!["--indicators", "--page"],
        vec!["--brief", "vps", "--page"],
        vec!["vps", "--page"],
    ] {
        let refused = parse(&args(&a)).err().unwrap_or_else(|| panic!("{a:?} was taken"));
        assert_eq!((refused.code, refused.field.as_str()), ("usage", "args"), "{a:?}");
        // The built binary refuses it before it asks for a program: `PATH` names none.
        let (status, out, err) = ap(&[&["status"][..], &a[..]].concat());
        assert_eq!(status, Some(2), "{a:?}");
        assert_eq!((out.as_str(), err["error"]["code"].as_str()), ("", Some("usage")), "{a:?}");
        assert_eq!(err["error"]["field"], "args", "{a:?}");
    }
    for a in [vec!["--page"], vec!["--page", "--store", "/s"], vec!["--store", "/s", "--page"]] {
        let taken = parse(&args(&a)).unwrap_or_else(|e| panic!("{a:?} refused {}", e.code));
        assert!(matches!(taken.call, Call::Page) && !taken.text, "{a:?}");
    }
    // The answer is plain lines, not JSON.
    let lines = page(&mut fixture());
    assert!(lines[0].starts_with("status 2026-11-22T01:00:00Z schema work@19"), "{}", lines[0]);
    assert!(serde_json::from_str::<Value>(&lines.join("\n")).is_err());
    // The help names the form, in the verb list and in the verb's own help.
    let (status, all, _) = ap(&["--help"]);
    assert_eq!(status, Some(0));
    assert!(all.contains("ap status --page [--store <path>]"), "{all}");
    let (status, help, _) = ap(&["status", "--help"]);
    assert_eq!(status, Some(0));
    assert!(help.contains("       ap status --page [--store <path>]\n"), "{help}");
    let (indicators, page_at) = (help.find("ap status --indicators"), help.find("ap status --page"));
    assert!(indicators < page_at, "the page's line follows the indicators' line");
}

// spec: specs/ap/status.md#R48
#[test]
fn r48_the_calls_are_the_reads_then_capacity_then_spend_then_briefs_rows_and_costs() {
    let mut p = fixture();
    page(&mut p);
    let asked = p.asked();
    // Only reads, the three functions and no other.
    for a in &asked {
        let known = ["get", "search", "render", "log ", "fn capacity", "fn api_spend", "fn cost"];
        assert!(known.iter().any(|k| a.starts_with(k)), "{a}");
    }
    let first = |what: &str| asked.iter().position(|a| a == what).unwrap_or_else(|| panic!("{what} in {asked:?}"));
    let capacity = first("fn capacity");
    let spend = first("fn api_spend");
    let gets: Vec<usize> = (0..asked.len()).filter(|i| asked[*i] == "get").collect();
    let logs: Vec<usize> = (0..asked.len()).filter(|i| asked[*i].starts_with("log ")).collect();
    let costs: Vec<usize> = (0..asked.len()).filter(|i| asked[*i] == "fn cost").collect();
    assert!(gets.len() >= 2 && logs.len() == 3 && costs.len() == 3, "{asked:?}");
    // The reads of R4 and R17 (searches) first; then `capacity`, `api_spend`, the briefs, the rows, the costs.
    assert!(asked[..capacity].iter().all(|a| a == "search"), "{asked:?}");
    assert!(capacity + 1 == spend, "{asked:?}");
    assert!(spend < gets[0] && gets[0] < logs[0], "{asked:?}");
    assert!(logs[2] < costs[0], "capacity and api_spend before the first log and before any cost");
    // The briefs come by key before the rows; the costs by key after.
    assert!(gets.iter().all(|g| *g < logs[0]));
    let intents: Vec<&str> = (gets.iter())
        .filter_map(|g| p.calls[*g].1["id"]["native_key"].as_str())
        .collect();
    assert_eq!(intents, ["alpha", "beta"], "one brief of each open intent, by key");
    let by_intent: Vec<Option<&str>> = costs.iter().map(|c| p.calls[*c].1["parameters"]["intent"].as_str()).collect();
    assert_eq!(by_intent, [None, Some("alpha"), Some("beta")]);
    // The parameters.
    let window = |extra: Option<&str>| {
        let mut w = json!({"since": "2026-11-16", "until": "2026-11-22", "by": "role"});
        if let Some(i) = extra {
            w["intent"] = json!(i);
        }
        json!({ "parameters": w })
    };
    assert_eq!(p.calls[costs[0]].1, window(None));
    assert_eq!(p.calls[costs[1]].1, window(Some("alpha")));
    assert_eq!(p.calls[costs[2]].1, window(Some("beta")));
    assert_eq!(p.calls[capacity].1, json!({"parameters": {}}));
    assert_eq!(p.calls[spend].1, json!({"parameters": {"month": "2026-11"}}));
    // The rows are read once, whatever the number of intents: one search of the merged units, one log of each action.
    let merged_searches = p.calls.iter()
        .filter(|c| c.0[0] == "search" && c.1["object_type"] == "unit" && c.1["where"]["all"] == json!([{"eq": ["state", "merged"]}]))
        .count();
    assert_eq!(merged_searches, 1);
    let mut actions: Vec<&str> = logs.iter().map(|l| p.calls[*l].1["action_type"].as_str().unwrap()).collect();
    actions.sort_unstable();
    assert_eq!(actions, ["merge_unit", "start_unit", "stop_unit"]);
    // Every call named the store, and only the files of R11 were read.
    assert!(p.stores.iter().all(|s| s.as_deref() == Some(STORE)));
    let reads = p.reads.borrow();
    assert!(reads.iter().all(|r| r == "/etc/agent-platform/deliver.json" || r == "/var/lib/agent-platform/deliver/last.json"), "{reads:?}");
}

// spec: specs/ap/status.md#R48
#[test]
fn r48_no_step_after_the_reads_begins_past_600_seconds_by_the_ports_clock() {
    // The first intent's cost ends past the bound: the next intent's cost is never asked.
    let mut p = fixture();
    p.advance.insert("fn:cost:alpha".into(), 601);
    let lines = page(&mut p);
    assert_eq!(p.asked().iter().filter(|a| *a == "fn cost").count(), 3 - 1, "{:?}", p.asked());
    assert!(lines.contains(&"cost alpha total 1.75 USD, merged 3, a pr 0.88, lead share -".to_owned()), "{lines:#?}");
    assert!(lines.contains(&"cost beta unreadable page_bound".to_owned()), "{lines:#?}");
    // Every earlier section is printed.
    for l in ["flow alpha 3 merged, 2 in the week, median minutes 30 rounds 0 repairs 0 first accepted -",
        "cost all total 9.0 USD, merged 3, a pr 3.0, lead share - unmeasured 2", "HOSTS", "ACCOUNTS",
        "api 2026-11 total settled 12.34 reserved 0.01 counted 12.35 USD"] {
        at(&lines, l);
    }
    assert_eq!(lines.iter().filter(|l| l.contains("page_bound")).count(), 1, "{lines:#?}");

    // The rows end past the bound: no cost is asked, the flows and the spend and capacity before them are printed.
    let mut p = fixture();
    p.advance.insert("log".into(), 600);
    let lines = page(&mut p);
    assert!(!p.asked().iter().any(|a| a == "fn cost"), "{:?}", p.asked());
    for l in ["cost alpha unreadable page_bound", "cost beta unreadable page_bound", "cost all unreadable page_bound"] {
        at(&lines, l);
    }
    assert!(lines.iter().any(|l| l.starts_with("flow alpha 3 merged")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.starts_with("api 2026-11 verify")), "{lines:#?}");
    at(&lines, "HOSTS");

    // 599 seconds is within the bound.
    let mut p = fixture();
    p.advance.insert("fn:cost:alpha".into(), 599);
    let lines = page(&mut p);
    assert!(!lines.iter().any(|l| l.contains("page_bound")), "{lines:#?}");

    // A bound that passed before `capacity` cuts every step after the reads.
    let mut p = fixture();
    p.advance.insert("search".into(), 601);
    let lines = page(&mut p);
    assert!(p.asked().iter().all(|a| a == "search"), "{:?}", p.asked());
    for l in ["capacity unreadable page_bound", "api 2026-11 unreadable page_bound", "brief alpha unreadable page_bound",
        "flow all unreadable page_bound", "cost beta unreadable page_bound"] {
        at(&lines, l);
    }
}

// spec: specs/ap/status.md#R49
#[test]
fn r49_the_order_of_the_lines_and_the_week() {
    let mut p = fixture();
    let lines = page(&mut p);
    // The lines `ap status --text` prints for the same answers come first.
    let text = lines_of(&Call::Overview, &mut p);
    assert_eq!(lines[..text.len()], text[..], "the lines of ap status --text come first");
    assert_eq!(lines[text.len()], "WEEK 2026-11-16 2026-11-22");
    let order = ["WEEK 2026-11-16 2026-11-22", "INTENT alpha", "INTENT beta", "ALL", "HOSTS", "ACCOUNTS"];
    let at_each: Vec<usize> = order.iter().map(|l| at(&lines, l)).collect();
    assert!(at_each.windows(2).all(|w| w[0] < w[1]), "{at_each:?}");
    // No line for the closed intent.
    assert!(!lines.iter().any(|l| l.contains("gamma") && !l.contains("INTENTS")), "{lines:#?}");
    assert!(!lines.contains(&"INTENT gamma".to_owned()));
    // Within an intent: its brief, its flow, its cost; then all: flow, cost, api; then the capacity.
    let (alpha, beta, all) = (at(&lines, "INTENT alpha"), at(&lines, "INTENT beta"), at(&lines, "ALL"));
    let kinds = |from: usize, to: usize| -> Vec<&str> {
        let mut k: Vec<&str> = (lines[from + 1..to].iter())
            .map(|l| l.split(' ').next().unwrap())
            .collect();
        k.dedup();
        k
    };
    let alpha_kinds = kinds(alpha, beta);
    assert_eq!(alpha_kinds.first(), Some(&"status"), "{alpha_kinds:?}");
    assert_eq!(alpha_kinds[alpha_kinds.len() - 2..], ["flow", "cost"], "{alpha_kinds:?}");
    assert_eq!(kinds(all, at(&lines, "HOSTS")), ["flow", "cost", "api"]);
}

// spec: specs/ap/status.md#R50
#[test]
fn r50_an_intent_has_its_brief_flow_and_cost() {
    let mut p = fixture();
    let lines = page(&mut p);
    let brief = lines_of(&Call::Brief("alpha".into()), &mut p);
    let from = at(&lines, "INTENT alpha");
    assert_eq!(lines[from + 1..from + 1 + brief.len()], brief[..], "the lines of ap status --brief alpha");
    let after = &lines[from + 1 + brief.len()..];
    // Three merged units, two in the week (one merged on the first day, one second before it is out).
    assert_eq!(after[0], "flow alpha 3 merged, 2 in the week, median minutes 30 rounds 0 repairs 0 first accepted -");
    assert_eq!(after[1], "cost alpha implementer 1.5 USD 2 runs");
    assert_eq!(after[2], "cost alpha - 0.25 USD 1 runs");
    assert_eq!(after[3], "cost alpha total 1.75 USD, merged 3, a pr 0.88, lead share -");
    assert_eq!(after[4], "INTENT beta");
    let beta = at(&lines, "INTENT beta");
    let brief = lines_of(&Call::Brief("beta".into()), &mut p);
    assert_eq!(lines[beta + 1..beta + 1 + brief.len()], brief[..]);
    let tail = &lines[beta + 1 + brief.len()..];
    assert_eq!(tail[0], "flow beta 1 merged, 1 in the week, median minutes 10 rounds 0 repairs 0 first accepted -");
    // No group: the total alone; a null `usd_a_pr` is `-`.
    assert_eq!(tail[1], "cost beta total 0.0 USD, merged 3, a pr -, lead share -");
}

// spec: specs/ap/status.md#R50
#[test]
fn r50_a_flow_line_ends_as_the_summary_line_of_the_indicators_and_marks_a_cut_log() {
    let mut p = fixture();
    let lines = page(&mut p);
    for (label, call) in [("alpha", Call::Indicators(Some("alpha".into()))), ("beta", Call::Indicators(Some("beta".into()))),
        ("all", Call::Indicators(None))] {
        let indicators = lines_of(&call, &mut p);
        let summary = indicators.last().unwrap();
        let (count, tail) = summary.strip_prefix("summary ").unwrap().split_once(" units ").unwrap();
        let flow = lines.iter().find(|l| l.starts_with(&format!("flow {label} "))).unwrap();
        assert!(flow.starts_with(&format!("flow {label} {count} merged, ")), "{flow}");
        assert!(flow.ends_with(tail), "{flow} / {summary}");
    }
    // A merged unit the log has no row of: its rows' `log_cut` ends the flow line.
    let mut p = fixture();
    p.put("unit", "u-cut", json!({"lane": "objective", "class": "plain", "state": "merged", "intent": oid("intent", "beta")}));
    let lines = page(&mut p);
    let beta = lines.iter().find(|l| l.starts_with("flow beta ")).unwrap();
    assert!(beta.starts_with("flow beta 2 merged, 1 in the week, ") && beta.ends_with(" log_cut"), "{beta}");
    let alpha = lines.iter().find(|l| l.starts_with("flow alpha ")).unwrap();
    assert!(!alpha.contains("log_cut"), "{alpha}");
    let all = lines.iter().find(|l| l.starts_with("flow all ")).unwrap();
    assert!(all.ends_with(" log_cut"), "{all}");
}

// spec: specs/ap/status.md#R51
#[test]
fn r51_all_has_its_flow_cost_and_the_months_spend_in_cents() {
    let mut p = fixture();
    let lines = page(&mut p);
    let all = at(&lines, "ALL");
    let want = [
        "ALL",
        "flow all 5 merged, 4 in the week, median minutes 30 rounds 0 repairs 0 first accepted -",
        "cost all implementer 1.5 USD 2 runs",
        "cost all - 0.25 USD 1 runs",
        "cost all total 9.0 USD, merged 3, a pr 3.0, lead share - unmeasured 2",
        "api 2026-11 soak settled -0.01 reserved 0.00 counted 0.00 USD, 1 runs, 0 running",
        "api 2026-11 verify settled 12.35 reserved 0.01 counted 12.35 USD, 2 runs, 1 running",
        "api 2026-11 total settled 12.34 reserved 0.01 counted 12.35 USD",
        "HOSTS",
    ];
    assert_eq!(lines[all..all + want.len()], want);
    // The rounding: half away from zero, in integers.
    for (micros, usd) in [(12_345_000_i64, "12.35"), (5_000, "0.01"), (-5_000, "-0.01"), (4_999, "0.00"), (-4_999, "0.00"), (0, "0.00"),
        (999_995_000, "1000.00"), (1_000_000_000_000, "1000000.00"), (-12_345_000, "-12.35")] {
        let mut p = fixture();
        let mut a = api();
        a["settled_usd_micros"] = json!(micros);
        p.functions.insert("api_spend".into(), Ok(a));
        let lines = page(&mut p);
        let total = lines.iter().find_map(|l| l.strip_prefix("api 2026-11 total settled ")).unwrap();
        assert!(total.starts_with(&format!("{usd} reserved ")), "{micros}: {total}");
    }
}

// spec: specs/ap/status.md#R52
#[test]
fn r52_the_capacity_of_the_hosts_machines_and_accounts() {
    let mut p = fixture();
    let lines = page(&mut p);
    let from = at(&lines, "HOSTS");
    let want = [
        "HOSTS",
        "host local machine vps node vps answers true stale false free true cpus 8 load 0.05 available 4096 MB seen 2026-11-22T00:59:00Z",
        // A null is `-`, a line break a space.
        "host pc machine pc node a b answers - stale false free - cpus - load 12.34 available - MB seen -",
        "host idle machine pc node - answers true stale true free false cpus 2 load - available 10 MB seen 2026-11-22T00:00:00Z",
        "machine vps up true free true cpus 8 load 1.00 available 4096 MB",
        "machine pc up false free false cpus - load - available - MB",
        "can start vps,pc",
        "ACCOUNTS",
        "account a1 claude allowance 5h window 40 percent source run at 2026-11-22T00:30:00Z",
        // An allowance that is not a string is compact JSON.
        "account a2 codex allowance {\"of\":[2,3],\"used\":1} source stated at 2026-11-21T00:00:00Z",
        "account a3 claude allowance - source - at -",
    ];
    assert_eq!(lines[from..], want);
    // `can_start` empty is `none`; capacity asked with no parameter.
    let mut p = fixture();
    let mut c = capacity();
    c["can_start"] = json!([]);
    c["accounts"] = json!([]);
    p.functions.insert("capacity".into(), Ok(c));
    let lines = page(&mut p);
    assert_eq!(lines[lines.len() - 2..], ["can start none", "ACCOUNTS"]);
    assert_eq!(p.calls[p.asked().iter().position(|a| a == "fn capacity").unwrap()].1, json!({"parameters": {}}));
}

// spec: specs/ap/status.md#R53
#[test]
fn r53_a_section_that_cannot_be_read_is_one_line_and_the_page_goes_on() {
    let mut p = fixture();
    for k in ["cost", "cost:alpha", "cost:beta"] {
        p.functions.insert(k.into(), Err("function_timeout"));
    }
    p.functions.insert("capacity".into(), Err("unknown_function"));
    let lines = page(&mut p);
    for l in ["cost alpha unreadable function_timeout", "cost beta unreadable function_timeout", "cost all unreadable function_timeout",
        "capacity unreadable unknown_function"] {
        at(&lines, l);
    }
    // Every other section is printed; no function was asked again.
    for l in ["INTENT alpha", "INTENT beta", "ALL", "flow all 5 merged, 4 in the week, median minutes 30 rounds 0 repairs 0 first accepted -",
        "api 2026-11 total settled 12.34 reserved 0.01 counted 12.35 USD"] {
        at(&lines, l);
    }
    assert_eq!(p.asked().iter().filter(|a| a.starts_with("fn ")).count(), 5, "{:?}", p.asked());
    assert!(!lines.contains(&"HOSTS".to_owned()) && !lines.iter().any(|l| l.starts_with("cost alpha ") && !l.contains("unreadable")));
    assert_eq!(lines.last().unwrap(), "capacity unreadable unknown_function");

    // An answer without a member a section names: `store_answer_unreadable`, for that section alone.
    let mut p = fixture();
    let mut c = capacity();
    c.as_object_mut().unwrap().remove("hosts");
    p.functions.insert("capacity".into(), Ok(c));
    p.functions.insert("api_spend".into(), Ok(json!({"month": "2026-11", "jobs": []})));
    let mut a = cost(json!([{"key": "x", "usd": 1, "messages": 1}]), 1.0, None);
    a.as_object_mut().unwrap().remove("unmeasured");
    p.functions.insert("cost".into(), Ok(a));
    let mut b = cost(json!([]), 1.0, None);
    b.as_object_mut().unwrap().remove("merged");
    p.functions.insert("cost:beta".into(), Ok(b));
    let lines = page(&mut p);
    for l in ["capacity unreadable store_answer_unreadable", "api 2026-11 unreadable store_answer_unreadable",
        "cost all unreadable store_answer_unreadable", "cost beta unreadable store_answer_unreadable",
        "cost alpha total 1.75 USD, merged 3, a pr 0.88, lead share -"] {
        at(&lines, l);
    }

    // A refusal of the store in the rows: every flow line says so, the rest stays.
    let mut p = fixture();
    p.refuse.insert("log", "store_unavailable");
    let lines = page(&mut p);
    for l in ["flow alpha unreadable store_unavailable", "flow beta unreadable store_unavailable", "flow all unreadable store_unavailable"] {
        at(&lines, l);
    }
    at(&lines, "cost alpha total 1.75 USD, merged 3, a pr 0.88, lead share -");
    at(&lines, "HOSTS");
    // A unit's view the store refuses is a refusal of the rows.
    let mut p = fixture();
    p.refuse.insert("render", "store_unavailable");
    let lines = page(&mut p);
    at(&lines, "flow alpha unreadable store_unavailable");
    at(&lines, "cost beta total 0.0 USD, merged 3, a pr -, lead share -");
    // A brief that cannot be read: its one line, and its intent's flow and cost stay.
    let mut p = fixture();
    p.refuse.insert("get", "store_unavailable");
    let lines = page(&mut p);
    at(&lines, "brief alpha unreadable store_unavailable");
    at(&lines, "brief beta unreadable store_unavailable");
    at(&lines, "flow alpha 3 merged, 2 in the week, median minutes 30 rounds 0 repairs 0 first accepted -");
    at(&lines, "cost alpha total 1.75 USD, merged 3, a pr 0.88, lead share -");

    // The form's refusal: a failure of the reads of R4 is the failure, with no line.
    let mut p = fixture();
    p.refuse.insert("search", "store_unavailable");
    let refused = execute(&mut p, Some(STORE), &Call::Page).err().unwrap();
    assert_eq!(ap::store::code(&refused), "store_unavailable");
    assert!(p.asked().iter().all(|a| a == "search"), "{:?}", p.asked());
    let (status, out, err) = ap(&["status", "--page"]);
    assert_eq!((status, out.as_str()), (Some(2), ""));
    assert!(err["error"]["code"].is_string(), "{err}");
}
