//! `ap unit <page>`: the request and the unit's two prompts composed from a page and the task's
//! `units.json` (README "A unit from its page"). Nothing here writes a file: the caller creates them.
use super::{absolute, regular, task_key_ok};
use crate::host::{Failure, digest, error, fail};
use crate::page;
use serde_json::{Value, json};
use std::path::Path;

const UNITS: &str = "task_key clones accounts timeouts compact_window prompts judge_threads";

/// What a call from a page holds in memory: the page's bytes as read, the composed request and the
/// three files (request, prompt, verify prompt: path and bytes), and the record's `composed`.
#[derive(Default)]
pub(super) struct Composed {
    pub page: Option<Vec<u8>>,
    pub request: Value,
    pub files: Vec<(String, Vec<u8>)>,
    pub record: Value,
}
impl Composed {
    /// The round's come-back: the first call's `composed.request` only, no file of its own.
    pub fn back(request: &Value) -> Self {
        let record = json!({ "request": request });
        Self {
            record,
            ..Self::default()
        }
    }
    /// The object `--compose-only` prints.
    pub fn printed(&self) -> Value {
        let text = |i: usize| String::from_utf8_lossy(&self.files[i].1).into_owned();
        json!({"request": self.request, "prompt": text(1), "verify_prompt": text(2)})
    }
}

fn bad(member: &str) -> Failure {
    error("invalid_input", &format!("units.{member}"))
}
/// Every member of `units.json`, by the rule of README "A unit from its page" point 2.
fn check(units: &Value) -> Result<(), Failure> {
    let o = units
        .as_object()
        .ok_or_else(|| error("invalid_input", "units"))?;
    if let Some(k) = o.keys().find(|k| !UNITS.split(' ').any(|m| m == *k)) {
        return Err(bad(k));
    }
    let path = |v: &Value| v.as_str().is_some_and(absolute);
    let exact = |v: &Value, keys: &[&str], ok: &dyn Fn(&Value) -> bool| {
        let o = v.as_object().filter(|o| o.len() == keys.len());
        o.is_some_and(|o| keys.iter().all(|k| o.get(*k).is_some_and(ok)))
    };
    let each = |v: &Value, ok: &dyn Fn(&str, &Value) -> bool| {
        v.as_object()
            .is_some_and(|o| o.iter().all(|(k, v)| ok(k, v)))
    };
    let seconds = |v: &Value| v.as_u64().is_some_and(|n| (1..=86_400).contains(&n));
    // A key is a runtime, or `<runtime>/<role or model>`: `claude/verifier`, `claude/opus`.
    let accounts = |key: &str, v: &Value| {
        let (runtime, rest) = key.split_once('/').unwrap_or((key, ""));
        let name = |r: &str| {
            !r.is_empty()
                && r.len() <= 64
                && r.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-._".contains(&c))
        };
        let list = v.as_array().filter(|l| !l.is_empty() && l.iter().all(path));
        let one = !matches!(runtime, "codex" | "muse") || list.is_some_and(|l| l.len() == 1);
        ["claude", "codex", "muse", "agy"].contains(&runtime)
            && (!key.contains('/') || name(rest))
            && list.is_some()
            && one
    };
    for k in UNITS.split(' ') {
        let ok = o.get(k).is_some_and(|v| match k {
            "task_key" => v.as_str().is_some_and(task_key_ok),
            "clones" => each(v, &|_, p| path(p)),
            "accounts" => each(v, &accounts),
            "timeouts" => exact(v, &["implementer", "verifier", "fixer"], &seconds),
            "compact_window" => each(v, &|_, w| w.as_u64().is_some_and(|w| w > 0)),
            "prompts" => exact(v, &["implementer", "verifier", "fix", "second_look"], &path),
            _ => v.is_boolean(),
        });
        if !ok {
            return Err(bad(k));
        }
    }
    Ok(())
}

/// The lines of the section under `heading` (a line that is exactly it; the section ends before the
/// next line beginning `## ` or the block's opening line, or at the end), with their line ends.
fn section(text: &str, heading: &str) -> Result<String, Failure> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let line = |i: usize| lines[i].strip_suffix('\n').unwrap_or(lines[i]);
    let found: Vec<_> = (0..lines.len()).filter(|i| line(*i) == heading).collect();
    let detail = json!({ "heading": heading });
    let at = match found[..] {
        [at] => at + 1,
        [] => return Err(fail("page_section_missing", "page", detail)),
        _ => return Err(fail("page_section_twice", "page", detail)),
    };
    let ends = |i: &usize| lines[*i].starts_with("## ") || line(*i) == "```unit";
    let end = (at..lines.len()).find(ends).unwrap_or(lines.len());
    Ok(lines[at..end].concat())
}
/// `{page}`, `{unit}` and `{repository}` replaced in one pass; any other text stays.
fn fill(text: &str, vars: [(&str, &str); 3]) -> String {
    let (mut out, mut rest) = (String::new(), text);
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let var = vars.iter().find(|(k, _)| rest.starts_with(k));
        let (word, skip) = var.map_or(("{", 1), |(k, v)| (*v, k.len()));
        out.push_str(word);
        rest = &rest[skip..];
    }
    out + rest
}

/// The composition of the page at `page` (an absolute path); the refusals come before any effect.
pub(super) fn compose(page: &str) -> Result<Composed, Failure> {
    let bytes = regular(page, "page_unreadable", "page")?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let block = page::block(&text)?.ok_or_else(|| page::invalid("no_block", None))?;
    let m = &block.members;
    for k in "repository branch item merge implementer verifier".split(' ') {
        if !m.contains_key(k) {
            return Err(page::shape(k));
        }
    }
    let (notes, questions) = (
        section(&text, "## Unit notes")?,
        section(&text, "## Verifier questions")?,
    );
    let dir = Path::new(page)
        .parent()
        .and_then(Path::to_str)
        .unwrap_or("/");
    let file = regular(&format!("{dir}/units.json"), "units_unreadable", "page")?;
    let units: Value =
        serde_json::from_slice(&file).map_err(|_| error("invalid_input", "units"))?;
    check(&units)?;
    let (unit, repo) = (
        m["unit"].as_str().unwrap_or_default(),
        m["repository"].as_str().unwrap_or_default(),
    );
    let clone = units["clones"][repo]
        .as_str()
        .ok_or_else(|| bad("clones"))?;
    let out = format!("{dir}/runs/U-{unit}");
    let name = repo.split('/').nth(1).unwrap_or_default();
    let standing = |which: &str, section: &str| -> Result<Vec<u8>, Failure> {
        let field = format!("units.prompts.{which}");
        let read = regular(
            units["prompts"][which].as_str().unwrap_or_default(),
            "prompt_unreadable",
            &field,
        )?;
        let read = String::from_utf8(read).map_err(|_| error("prompt_unreadable", &field))?;
        let mut t = fill(
            &read,
            [("{page}", page), ("{unit}", unit), ("{repository}", repo)],
        );
        t.push_str(if t.ends_with('\n') { "\n" } else { "\n\n" });
        Ok((t + section).into_bytes())
    };
    let (prompt, verify) = (
        standing("implementer", &notes)?,
        standing("verifier", &questions)?,
    );
    let stage = |which: &str, file: &str| -> Result<Value, Failure> {
        let b = &m[which];
        let runtime = b["runtime"].as_str().unwrap_or("claude");
        let model = b["model"].as_str().unwrap_or_default();
        // The accounts of the stage's role, else of its model, else of its runtime.
        let list = [
            format!("{runtime}/{which}"),
            format!("{runtime}/{model}"),
            runtime.to_owned(),
        ]
        .iter()
        .find_map(|k| units["accounts"][k.as_str()].as_array())
        .ok_or_else(|| bad("accounts"))?;
        let mut s = json!({"model": b["model"], "effort": b["effort"], "runtime": runtime,
            "account": list[0], "prompt": format!("{out}{file}"),
            "timeout_seconds": b.get("timeout_seconds").unwrap_or(&units["timeouts"][which])});
        if let Some(mode) = b.get("permission_mode") {
            s["permission_mode"] = mode.clone();
        }
        if list.len() > 1 {
            s["next_accounts"] = json!(&list[1..]);
        }
        let window = &units["compact_window"][b["model"].as_str().unwrap_or_default()];
        if runtime == "claude" && !window.is_null() {
            s["compact_window"] = window.clone();
        }
        Ok(s)
    };
    let request = json!({"repository": m["repository"], "branch": m["branch"], "item": m["item"],
        "merge": m["merge"], "clone": clone, "task_key": units["task_key"],
        "judge_threads": units["judge_threads"], "page": page, "out": out,
        "worktree": format!("{dir}/wt/{name}-{}", unit.to_lowercase()),
        "implementer": stage("implementer", ".prompt.md")?,
        "verifier": stage("verifier", ".verify.prompt.md")?,
        "repair": {"prompt": units["prompts"]["fix"], "verifier_prompt": units["prompts"]["second_look"],
            "timeout_seconds": units["timeouts"]["fixer"]}});
    let mut json = serde_json::to_vec_pretty(&request).unwrap_or_default();
    json.push(b'\n');
    let files: Vec<_> = [
        (".request.json", json),
        (".prompt.md", prompt),
        (".verify.prompt.md", verify),
    ]
    .into_iter()
    .map(|(suffix, bytes)| (format!("{out}{suffix}"), bytes))
    .collect();
    let entry = |i: usize| json!({"path": files[i].0, "sha256": digest(&files[i].1)});
    let record = json!({"request": entry(0), "prompt": entry(1), "verify_prompt": entry(2)});
    Ok(Composed {
        page: Some(bytes),
        request,
        files,
        record,
    })
}
