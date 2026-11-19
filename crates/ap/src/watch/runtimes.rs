//! The detector `runtimes` of `ap watch` (`specs/ap/watch.md` R20, R26): checks `--version` and
//! `--help` flags of `claude`, `muse` and `agy` (never `codex`) without starting a session.
use super::{RUNTIMES, Read, Reading, RuntimeSpec, Signal};
use crate::host::{System, digest};
use serde_json::{Map, Value, json};

const MS: u64 = 10_000;

/// Whether `line` (after leading spaces) holds `flag` alone or after `-x, ` / `--y, `, followed by
/// a character that is not alphanumeric or `-`, or by the end of the line.
pub fn flag_on_line(line: &str, flag: &str) -> bool {
    let mut rest = line.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix(flag) {
            let boundary = after
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '-');
            if boundary {
                return true;
            }
        }
        let Some((prefix, after)) = rest.split_once(", ") else {
            return false;
        };
        if !prefix.starts_with('-') || prefix.chars().any(char::is_whitespace) {
            return false;
        }
        rest = after;
    }
}

fn flag_in_help(help: &str, flag: &str) -> bool {
    help.lines().any(|line| flag_on_line(line, flag))
}

fn run_cli(s: &mut impl System, name: &str, args: &[&str]) -> Option<String> {
    let words: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    match s.call(name, &words, &Value::Null, MS) {
        // `bytes`: standard output, or standard error when standard output is empty (a CLI may
        // print its `--help` on standard error with exit 0).
        Ok(r) if r.success => Some(String::from_utf8_lossy(&r.bytes).into_owned()),
        _ => None,
    }
}

fn check_runtime(
    s: &mut impl System,
    spec: &RuntimeSpec,
    kept: Option<&Value>,
    out: &mut Vec<Signal>,
) -> Option<Value> {
    let name = spec.name;
    let Some(ver_out) = run_cli(s, name, &["--version"]) else {
        out.push(Signal::new(
            format!("cli {name} absent"),
            "act",
            &format!("runtime CLI {name} is absent or failed --version"),
            format!("{name} --version failed or not on PATH"),
        ));
        return kept.cloned();
    };
    let version = ver_out.lines().next().unwrap_or("").trim().to_owned();
    let flags_digest = digest(spec.flags.join("\n").as_bytes());
    let unchanged = kept.is_some_and(|k| {
        k["version"].as_str() == Some(version.as_str())
            && k["flags"].as_str() == Some(flags_digest.as_str())
    });
    if unchanged {
        return kept.cloned();
    }
    let mut help_texts = String::new();
    let Some(main_help) = run_cli(s, name, &["--help"]) else {
        out.push(Signal::new(
            format!("cli {name} help"),
            "act",
            &format!("runtime CLI {name} failed --help"),
            format!("{name} --help failed or timed out"),
        ));
        return kept.cloned();
    };
    help_texts.push_str(&main_help);
    for sub in spec.subcommands {
        let Some(sub_help) = run_cli(s, name, &[sub, "--help"]) else {
            out.push(Signal::new(
                format!("cli {name} help"),
                "act",
                &format!("runtime CLI {name} {sub} failed --help"),
                format!("{name} {sub} --help failed or timed out"),
            ));
            return kept.cloned();
        };
        help_texts.push('\n');
        help_texts.push_str(&sub_help);
    }
    let mut all_found = true;
    for flag in spec.flags {
        if !flag_in_help(&help_texts, flag) {
            all_found = false;
            out.push(Signal::new(
                format!("cli {name} flag {flag}"),
                "act",
                &format!("runtime CLI {name} help lacks {flag}"),
                format!("{name} ({version}) --help lacks {flag}"),
            ));
        }
    }
    if all_found {
        Some(json!({"version": version, "flags": flags_digest}))
    } else {
        kept.cloned()
    }
}

/// `runtimes` (R20, R26): `--version` and `--help` flags of `claude`, `muse` and `agy`.
pub(crate) fn detect(s: &mut impl System, prior: Option<&Value>) -> Reading {
    let mut r = Read::default();
    let mut next = Map::new();
    for spec in &RUNTIMES {
        let kept = prior
            .and_then(|p| p.get(spec.name))
            .filter(|v| v.is_object());
        if let Some(entry) = check_runtime(s, spec, kept, &mut r.signals) {
            next.insert(spec.name.to_owned(), entry);
        }
    }
    r.state = Some(Value::Object(next));
    Ok(r)
}
