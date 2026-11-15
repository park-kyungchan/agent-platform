//! `ap --help` lists the verbs the command dispatches, each with its request form and the heading of
//! the README section that documents it.
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
    assert!(dispatched.contains("gate") && dispatched.contains("pane"));
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
