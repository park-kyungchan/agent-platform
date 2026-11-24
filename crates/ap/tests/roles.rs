//! The roles of the work schema's grants, the role each verb and service claims, and the roles file
//! (`specs/ap/roles.md`): the schema's grants against the table of R2, `deploy/roles.json` against the grants, the
//! services' lines, and the role every apply of `ap` claims, read from the bodies a fake `ont` records.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const SCHEMA: &str = include_str!("../../../ontology/work.schema.json");
const STAGES: [&str; 5] = ["implementer", "fixer", "verifier", "reviewer", "writer"];
const SERVICES: [&str; 5] = ["dispatch", "deliver", "watch", "measure", "api"];

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}
fn schema() -> Value {
    serde_json::from_str(SCHEMA).unwrap()
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}
fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

// spec: specs/ap/roles.md#R1
// spec: specs/ap/roles.md#R2
#[test]
fn grants_hold_the_roles_of_the_table() {
    let s = schema();
    assert_eq!(s["version"], 25);
    let grants = s["grants"].as_array().unwrap();
    assert_eq!(grants.len(), 13);
    // The grant without a role: the reads and the derivations of the one grant of version 21, the function route.
    let base = &grants[0];
    assert!(base.get("role").is_none());
    assert_eq!(base["principal"], "agent");
    assert_eq!(base["scope"], json!({}));
    assert_eq!(base["actions"], json!([]));
    assert_eq!(base["functions"], json!(["route"]));
    let read = base["read"].clone();
    assert!(!strings(&read).is_empty() && !strings(&base["derive"]).is_empty());
    let table: Vec<&str> = [
        "lead", "dispatch", "deliver", "watch", "route", "measure", "api",
    ]
    .into_iter()
    .chain(STAGES)
    .collect();
    let roles: Vec<&str> = grants[1..]
        .iter()
        .map(|g| g["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, table);
    let held: BTreeMap<&str, &Value> = roles.iter().copied().zip(&grants[1..]).collect();
    for (role, g) in &held {
        assert_eq!(
            (g["principal"].as_str(), &g["scope"]),
            (Some("agent"), &json!({}))
        );
        assert_eq!(g["derive"], json!([]), "{role}");
        let actions = strings(&g["actions"]);
        assert_eq!(actions, sorted(actions.clone()), "{role}");
        let functions = strings(&g["functions"]);
        assert_eq!(functions, sorted(functions.clone()), "{role}");
        // The reads of a grant that applies an action: those of the grant without a role; else none.
        let want = if actions.is_empty() {
            json!([])
        } else {
            read.clone()
        };
        assert_eq!(g["read"], want, "{role}");
    }
    // `lead` holds every action type and every function; the gates are in its grant alone.
    let types = sorted(
        s["action_types"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect(),
    );
    let functions = sorted(
        s["functions"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect(),
    );
    assert_eq!(strings(&held["lead"]["actions"]), types);
    assert_eq!(strings(&held["lead"]["functions"]), functions);
    for gate in ["approve_unit", "record_principle"] {
        let owners: Vec<&str> = held
            .iter()
            .filter(|(_, g)| strings(&g["actions"]).iter().any(|a| a == gate))
            .map(|(r, _)| *r)
            .collect();
        assert_eq!(owners, ["lead"], "{gate}");
    }
    // A stage role applies nothing and calls `clause` (and `route` through the grant without a role).
    for role in STAGES {
        assert_eq!(held[role]["actions"], json!([]), "{role}");
        assert_eq!(held[role]["functions"], json!(["clause"]), "{role}");
    }
    // The services hold what their programs apply and call.
    assert_eq!(
        held["deliver"]["actions"],
        json!(["close_install", "start_install"])
    );
    assert_eq!(
        held["watch"]["actions"],
        json!([
            "close_finding",
            "open_finding",
            "reopen_finding",
            "see_finding_again"
        ])
    );
    assert_eq!(held["route"]["actions"], json!([]));
    assert!(held["route"].get("functions").is_none());
    // `ap unit run` reads `capacity` as `dispatch` for the placement of a look (`unit-run.md` R45).
    assert_eq!(held["dispatch"]["functions"], json!(["capacity"]));
    for role in ["dispatch", "api"] {
        let a = strings(&held[role]["actions"]);
        assert!(
            a.contains(&"start_run".into()) && a.contains(&"close_run".into()),
            "{role}"
        );
    }
    for a in [
        "start_unit",
        "merge_unit",
        "amend_unit",
        "start_unit_review",
        "carry_review",
    ] {
        assert!(
            strings(&held["dispatch"]["actions"]).contains(&a.to_owned()),
            "{a}"
        );
    }
    assert_eq!(
        strings(&held["measure"]["functions"]),
        ["api_spend", "capacity", "cost"]
    );
    // Every action of a service role is an action type of the schema.
    for role in SERVICES {
        for a in strings(&held[role]["actions"]) {
            assert!(types.contains(&a), "{role} {a}");
        }
    }
}

// spec: specs/ap/roles.md#R5
#[test]
fn roles_file_lists_every_role() {
    let file: Value =
        serde_json::from_slice(&fs::read(repo("deploy/roles.json")).unwrap()).unwrap();
    let principals = file["principals"].as_object().unwrap();
    assert_eq!(file.as_object().unwrap().len(), 1);
    let mut keys: Vec<&str> = principals.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["ts:park-kyungchan@github", "uid:0"]);
    let listed = strings(&principals["uid:0"]);
    assert_eq!(listed, sorted(listed.clone()));
    let granted: BTreeSet<String> = schema()["grants"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|g| g["role"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(listed.iter().cloned().collect::<BTreeSet<_>>(), granted);
    assert_eq!(listed.len(), granted.len());
    let ts_roles = strings(&principals["ts:park-kyungchan@github"]);
    assert_eq!(ts_roles, ["lead"]);
    for role in &ts_roles {
        assert!(granted.contains(role), "{role}");
    }
}

// spec: specs/ap/roles.md#R4
#[test]
fn every_service_names_its_role() {
    let table: BTreeMap<&str, &str> = BTreeMap::from([
        ("ap-work", "dispatch"),
        ("ap-deliver", "deliver"),
        ("ap-watch", "watch"),
        ("ap-route", "route"),
        ("ap-clause", "route"),
        ("ap-resource", "measure"),
        ("ap-quota", "measure"),
        ("ap-status-page", "measure"),
        ("ap-measure", "measure"),
        ("ap-api-nightly", "api"),
        ("ap-api-weekly", "api"),
    ]);
    let mut seen = BTreeSet::new();
    for entry in fs::read_dir(repo("deploy")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let Some(service) = name.strip_suffix(".service") else {
            continue;
        };
        let text = fs::read_to_string(repo(&format!("deploy/{name}"))).unwrap();
        let lines: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("Environment=ONT_ROLE="))
            .collect();
        if matches!(service, "ont-backup" | "ont-serve") {
            assert!(lines.is_empty(), "{service}");
            continue;
        }
        let role = table
            .get(service)
            .unwrap_or_else(|| panic!("{service} has no role in the table"));
        assert_eq!(lines, [format!("Environment=ONT_ROLE={role}")], "{service}");
        seen.insert(service.to_owned());
    }
    assert_eq!(seen.len(), table.len());
}

/// A fake `ont` that notes each `action apply` and `action validate` request and answers what a store answers to a
/// first apply; any other call is an empty, qualified read.
const ONT: &str = r#"#!/bin/sh
d=$(dirname "$0")
body=$(cat)
case "$*" in
  *"action apply"*|*"action validate"*)
    printf '%s\n' "$body" >> "$d/applies"
    echo '{"status":"ok","now":1795000000,"schema":{"work":{"version":22}},"result":{"outcome":"applied","valid":true,"edits":[{"change":"initial","id":{"object_type":"request","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"k"}}]}}' ;;
  *)
    printf '%s\n' "$*" >> "$d/others"
    echo '{"status":"ok","now":1795000000,"schema":{"work":{"version":22}},"result":{"objects":[],"basis":{"qualified":true},"continuation":null}}' ;;
esac
"#;

struct Bed {
    dir: PathBuf,
}
impl Drop for Bed {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl Bed {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ap-roles-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin/ont"), ONT).unwrap();
        let mut p = fs::metadata(dir.join("bin/ont")).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut p, 0o755);
        fs::set_permissions(dir.join("bin/ont"), p).unwrap();
        Self { dir }
    }
    /// One call of the built `ap`; the bodies the fake engine recorded for it.
    fn run(&self, args: &[&str], stdin: &Value) -> Vec<Value> {
        let _ = fs::remove_file(self.dir.join("bin/applies"));
        let _ = fs::remove_file(self.dir.join("bin/others"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(args)
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.dir.join("bin").display()),
            )
            .env("HOME", &self.dir)
            .env("AP_POLICY_FILE", self.dir.join("no-policy.json"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        std::io::Write::write_all(child.stdin.as_mut().unwrap(), stdin.to_string().as_bytes())
            .unwrap();
        drop(child.stdin.take());
        child.wait().unwrap();
        fs::read_to_string(self.dir.join("bin/applies"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}
fn roles_of(bodies: &[Value]) -> BTreeSet<String> {
    bodies
        .iter()
        .map(|b| b["role"].as_str().unwrap_or("<none>").to_owned())
        .collect()
}

/// The role literals of one source file of `ap`: each `role: "x"`, `claim: "x"` and `"role": "x"` in it.
fn claims(file: &str) -> Vec<String> {
    let text = fs::read_to_string(repo(&format!("crates/ap/src/{file}"))).unwrap();
    let mut found = vec![];
    for key in ["role: \"", "claim: \"", "\"role\": \"", "\"role\":\""] {
        for (at, _) in text.match_indices(key) {
            let rest = &text[at + key.len()..];
            found.push((at, rest[..rest.find('"').unwrap()].to_owned()));
        }
    }
    found.sort();
    found.into_iter().map(|(_, r)| r).collect()
}

// spec: specs/ap/roles.md#R3
#[test]
fn every_apply_of_ap_claims_its_role() {
    // The verbs that apply with a plain request, driven against the fake engine: the body of each apply holds the
    // role of its verb.
    let bed = Bed::new("verbs");
    let request = bed.run(
        &["request"],
        &json!({"op": "record", "request_id": "m1", "agent": "lead", "session": "s1", "topic": "walk/one",
            "kind": "order", "reading": "the reading", "words": "do the thing", "said_at": "2026-11-16T03:04:05Z"}),
    );
    assert_eq!(
        roles_of(&request),
        BTreeSet::from(["lead".to_owned()]),
        "{request:?}"
    );
    let draft = bed.run(
        &["intent"],
        &json!({"op": "draft", "request_id": "d1", "agent": "lead", "session": "s1", "intent": "widget-report",
            "outcome": "Every widget is listed.", "first_user": "An invented reader.", "origin": "q-start",
            "criteria": [{"statement": "All listed.", "check": "count rows"}]}),
    );
    assert_eq!(
        roles_of(&draft),
        BTreeSet::from(["lead".to_owned()]),
        "{draft:?}"
    );
    let dir = bed.dir.join("resource");
    fs::create_dir_all(&dir).unwrap();
    let store = bed.dir.join("store.sqlite");
    let pass = bed.run(
        &[
            "resource",
            "--directory",
            dir.to_str().unwrap(),
            "--store",
            store.to_str().unwrap(),
        ],
        &Value::Null,
    );
    assert!(!pass.is_empty());
    assert_eq!(
        roles_of(&pass),
        BTreeSet::from(["measure".to_owned()]),
        "{pass:?}"
    );
    // `ap route` and `ap clause` apply no action: their only call of the engine is an index.
    for verb in ["route", "clause"] {
        let applied = bed.run(
            &[
                verb,
                "index",
                "--directory",
                dir.to_str().unwrap(),
                "--store",
                store.to_str().unwrap(),
            ],
            &Value::Null,
        );
        assert!(applied.is_empty(), "{verb}: {applied:?}");
    }
    // The bodies of the other verbs are asserted where their fakes record them: `ap watch` (`watch.rs`), `ap row`
    // (`row.rs`), `ap backlog` and `ap review` (`unit.rs`, `review.rs`), `ap measure` (`measure.rs`), `ap work`
    // (`work.rs`), `ap unit` (`unit_object.rs`, `unit_review.rs`, `unit_run.rs`, `unit_write.rs`), `ap run` (`run.rs`),
    // `ap api` (`api.rs`) and `ap deliver` (`deliver.rs`). Every other verb of the table, by the role its source names where it builds the request: the verbs of the Lead,
    // of the drivers (`dispatch`), of delivery, the watch and the pass of the resources and measures.
    let table: [(&str, &[&str]); 12] = [
        ("request.rs", &["lead", "lead"]),
        ("intent.rs", &["lead"]),
        ("review.rs", &["lead"]),
        ("row.rs", &["lead"]),
        ("watch.rs", &["watch"]),
        ("work.rs", &["dispatch", "dispatch"]),
        ("deliver.rs", &["deliver"]),
        ("resource.rs", &["measure"]),
        ("measure/kpis.rs", &["measure"]),
        ("measure/sessions.rs", &["measure"]),
        ("unit/object.rs", &["lead"]),
        (
            "unit/drive.rs",
            &["dispatch", "dispatch", "dispatch", "lead"],
        ),
    ];
    for (file, want) in table {
        assert_eq!(claims(file), want, "{file}");
    }
    // `ap unit write` amends and looks as `dispatch`; the stage's runs are claimed once, in the store module.
    assert!(claims("unit/page_write.rs").contains(&"dispatch".to_owned()));
    let write = fs::read_to_string(repo("crates/ap/src/unit/page_write.rs")).unwrap();
    assert!(write.contains("execute_as(s, work.store, &r, \"dispatch\")"));
    let store = fs::read_to_string(repo("crates/ap/src/store.rs")).unwrap();
    for (action, claim) in [
        ("start_run", "dispatch"),
        ("close_run", "dispatch"),
        ("start_run", "api"),
        ("close_run", "api"),
    ] {
        assert!(
            store.contains(&format!("(\"{action}\", \"{claim}\"")),
            "{action} {claim}"
        );
    }
    // `ap api batch` claims `api` and keeps `batch` as the run's parameter; `ap work` names `dispatch` for its runs.
    let api = fs::read_to_string(repo("crates/ap/src/api.rs")).unwrap();
    assert!(api.contains("store::start_run_api(") && api.contains("store::close_run_api("));
    assert!(!api.contains("store::start_run(") && !api.contains("store::close_run("));
    assert!(api.contains("\"role\": \"batch\""));
    let work = fs::read_to_string(repo("crates/ap/src/work.rs")).unwrap();
    assert!(work.contains("\"ONT_ROLE=dispatch\""));
}
