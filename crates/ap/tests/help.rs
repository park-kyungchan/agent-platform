//! `ap --help` lists the verbs the command dispatches, each with its request form and the heading of
//! the README section that documents it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use std::{collections::BTreeSet, process::Command};

fn ap(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}
/// The words that follow `needle` in `source`, each between double quotes, as the content check
/// (`ci/check-content.py`) reads the dispatch: `arg == "gate"`, `args[0] == "version"`.
fn quoted_after(source: &str, needle: &str, out: &mut BTreeSet<String>) {
    for part in source.split(needle).skip(1) {
        let word: String = part
            .strip_prefix('"')
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if word.starts_with(|c: char| c.is_ascii_lowercase()) {
            out.insert(word);
        }
    }
}
// spec: specs/ap/verbs.md#R1
// spec: specs/ap/verbs.md#R2
#[test]
fn r2_help_lists_every_verb() {
    let help = ap(&["--help"]);
    assert!(help.status.success());
    assert_eq!(help.stdout, ap(&["-h"]).stdout, "-h differs from --help");
    let help = String::from_utf8(help.stdout).unwrap();
    // One line per verb: `<verb>: <request form> (README "<heading>")`.
    let mut listed = vec![];
    for line in help.lines().filter(|l| l.contains(" (README \"")) {
        let (verb, rest) = line.trim().split_once(": ").unwrap();
        let (form, heading) = rest.rsplit_once(" (README \"").unwrap();
        assert!(!form.is_empty(), "{line}");
        listed.push((
            verb.to_owned(),
            heading.strip_suffix("\")").unwrap().to_owned(),
        ));
    }
    assert!(!listed.is_empty(), "no verb listed:\n{help}");
    let names: BTreeSet<_> = listed.iter().map(|(v, _)| v.clone()).collect();
    assert_eq!(names.len(), listed.len(), "a verb is listed twice");

    // Every listed verb answers its own help.
    for (verb, _) in &listed {
        let answer = ap(&[verb, "--help"]);
        assert_eq!(answer.status.code(), Some(0), "ap {verb} --help");
        assert!(!answer.stdout.is_empty(), "ap {verb} --help prints nothing");
    }

    // Every verb the dispatch knows is listed.
    let mut dispatched = BTreeSet::new();
    let source = include_str!("../src/main.rs");
    quoted_after(source, "arg == ", &mut dispatched);
    quoted_after(source, "args[0] == ", &mut dispatched);
    quoted_after(source, "args[0] != ", &mut dispatched);
    assert!(dispatched.contains("gate") && dispatched.contains("version"));
    let missing: Vec<_> = dispatched.difference(&names).collect();
    assert!(missing.is_empty(), "dispatched and not listed: {missing:?}");

    // Every heading the help names is a `##` heading of the README.
    let readme = include_str!("../README.md");
    let headings: BTreeSet<_> = readme
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .collect();
    for (verb, heading) in &listed {
        assert!(
            headings.contains(heading.as_str()),
            "{verb}: no `## {heading}` in the README"
        );
    }
}

// spec: specs/ap/status.md#R23
#[test]
fn r23_the_help_of_status_names_the_wait_form() {
    let help = String::from_utf8(ap(&["--help"]).stdout).unwrap();
    assert!(help.contains("ap status --wait <intent> [--timeout <seconds>] [--store <path>]"));
    let status = String::from_utf8(ap(&["status", "--help"]).stdout).unwrap();
    assert!(status.contains("ap status --wait <intent> [--timeout <seconds>] [--store <path>]"));
}

// spec: specs/ap/status.md#R34
#[test]
fn r34_the_help_of_status_names_the_indicators_form() {
    let form = "ap status --indicators [<intent>] [--text] [--store <path>]";
    let help = String::from_utf8(ap(&["--help"]).stdout).unwrap();
    assert!(help.contains(form));
    let status = String::from_utf8(ap(&["status", "--help"]).stdout).unwrap();
    assert!(status.contains(form));
}

// spec: specs/ap/status.md#R43
#[test]
fn r43_the_help_of_status_names_the_brief_and_requests_forms() {
    let form = "ap status --brief [<intent>] [--directory <absolute path>] [--store <path>]";
    let help = String::from_utf8(ap(&["--help"]).stdout).unwrap();
    assert!(help.contains(form));
    let status = String::from_utf8(ap(&["status", "--help"]).stdout).unwrap();
    assert!(status.contains(form));
    assert!(status.contains("<key> [--requests]"));
}

// spec: specs/ap/verbs.md#R4
#[test]
fn verbs_r4_an_argument_list_that_reaches_no_verb_is_refused_usage() {
    let refusal = b"{\"error\":{\"code\":\"usage\",\"field\":\"args\"}}\n";
    for args in [
        &[][..],
        &["install"][..],
        &["doctor"][..],
        &["render"][..],
        &["no-such-verb"][..],
        &["--help", "run"][..],
        &["-h", "x"][..],
        &["--version", "x"][..],
        &["version", "x"][..],
    ] {
        let answer = ap(args);
        assert_eq!(answer.status.code(), Some(2), "ap {args:?}");
        assert!(answer.stdout.is_empty(), "ap {args:?}");
        assert_eq!(answer.stderr, refusal, "ap {args:?}");
    }
}

// spec: specs/ap/verbs.md#R1
// spec: specs/ap/verbs.md#R2
#[test]
fn verbs_r1_r2_the_list_begins_with_the_usage_and_a_verb_without_a_usage_prints_its_line() {
    let help = String::from_utf8(ap(&["-h"]).stdout).unwrap();
    let mut lines = help.lines();
    assert_eq!(lines.next(), Some("Usage: ap <verb> [< request.json]"));
    assert_eq!(
        lines.next(),
        Some("Verbs; the section is of crates/ap/README.md:")
    );
    let line = help.lines().find(|l| l.starts_with("  version: ")).unwrap();
    let answer = ap(&["version", "--help"]);
    assert_eq!(answer.status.code(), Some(0), "ap version --help");
    assert_eq!(answer.stdout, format!("{line}\n").into_bytes());
}

// spec: specs/ap/verbs.md#R5
#[test]
fn verbs_r5_an_answer_that_cannot_be_written_exits_4() {
    let Ok(full) = std::fs::OpenOptions::new().write(true).open("/dev/full") else {
        eprintln!("skipped: no /dev/full");
        return;
    };
    for args in [
        &["--version"][..],
        &["--help"][..],
        &["version", "--help"][..],
    ] {
        let answer = Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(full.try_clone().unwrap())
            .output()
            .unwrap();
        assert_eq!(answer.status.code(), Some(4), "ap {args:?}");
        assert_eq!(
            answer.stderr, b"{\"error\":{\"code\":\"io_error\",\"field\":\"stdout\"}}\n",
            "ap {args:?}"
        );
    }
}

// spec: specs/ap/unit.md#R50
#[test]
fn r50_the_help_names_defer_undefer_and_used_in_the_line_of_unit() {
    let all = String::from_utf8(ap(&["--help"]).stdout).unwrap();
    let line = all
        .lines()
        .find(|l| l.trim_start().starts_with("unit: "))
        .unwrap();
    assert!(
        line.contains("unit defer|undefer|used [--store <path>] < request.json"),
        "{line}"
    );
    let unit = String::from_utf8(ap(&["unit", "--help"]).stdout).unwrap();
    assert!(
        unit.contains("ap unit defer|undefer|used [--store <path>] < request.json"),
        "{unit}"
    );
}
