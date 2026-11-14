//! The case call of `ap unit` (README "One case"): a past unit again, in a copy of its base, judged
//! by its accepted tests. The record, the claim, the stage and the bounded evaluator are the unit
//! call's code.
use super::{
    End, RUN_FILES, Stop, Unit, accept, claimed, create, executable, now, path, regular, seconds,
    stage, stage_request, stage_run, stop, unchanged,
};
use crate::work::{Failure, LIMIT, System, args, digest, error, fail, require};
use serde_json::{Value, json};
use std::path::{Component, Path};

const MEMBERS: &str = "case clone worktree page out acceptance implementer at acceptance_seconds";
const CASE: &str = "id repository pull_request base head tests commands page_sha256";
const FILES: &str = ".unit.json .implementer.md .implementer.prompt.md .accept.log .candidate.diff";
/// A new repository at `$2` with one commit: the tree of `$3` with the paths `$5…` as they are at
/// `$4`. Only the objects of that tree go from the clone `$1` to the copy, which names no remote.
const COPY: &str = r#"set -euo pipefail
clone=$1 wt=$2 at=$3 head=$4; shift 4
g() { git -C "$wt" -c core.hooksPath=/dev/null -c user.name=ap-case -c user.email=ap-case@localhost "$@"; }
tree=$(git -C "$clone" rev-parse --verify -q "$at^{tree}")
git init -q "$wt"
{ git -C "$clone" rev-list --objects "$tree"; for p; do git -C "$clone" rev-parse --verify -q "$head:$p"; done; } |
  git -C "$clone" pack-objects -q --stdout | g unpack-objects -q
g read-tree "$tree"
for p; do
  entry=$(git -C "$clone" ls-tree "$head" -- "$p"); mode=${entry%% *}; blob=${entry#* blob }
  g update-index --add --cacheinfo "$mode,${blob%%$'\t'*},$p"
done
g checkout-index -a -f
commit=$(g commit-tree -m "case at $at" "$(g write-tree)")
g update-ref HEAD "$commit"
echo "$commit""#;
/// The candidate's difference from the copy's commit: new files too, ignored files not.
const DIFF: &str = r#"set -euo pipefail -o noclobber
cd "$1"; git add -A; git diff --cached --binary "$2" > "$3""#;

fn hex(v: &Value, n: usize) -> bool {
    let ok = |s: &str| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    v.as_str().is_some_and(ok)
}
/// A nonempty array whose every item is a nonempty string that `each` admits.
fn strings(v: &Value, each: fn(&str) -> bool) -> bool {
    let item = |w: &Value| {
        w.as_str()
            .is_some_and(|w| !w.is_empty() && !w.contains('\0') && each(w))
    };
    v.as_array()
        .is_some_and(|a| !a.is_empty() && a.iter().all(item))
}
fn relative(p: &str) -> bool {
    let normal = |c: Component| matches!(c, Component::Normal(_));
    Path::new(p).components().all(normal)
}
fn command(v: &Value) -> bool {
    strings(v, |_| true)
}
fn texts(v: &Value) -> impl Iterator<Item = &str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str)
}
/// The case file: exactly its members, each well formed, named after its id.
fn read_case(file: &str) -> Result<(Value, String), Failure> {
    let bytes = regular(file, "case_unreadable", "case")?;
    let c: Value = serde_json::from_slice(&bytes).map_err(|_| error("case_unreadable", "case"))?;
    let o = c
        .as_object()
        .ok_or_else(|| error("case_unreadable", "case"))?;
    let id = c["id"].as_str().unwrap_or_default();
    let id_char =
        |(i, b): (usize, u8)| b.is_ascii_lowercase() || b.is_ascii_digit() || (i > 0 && b == b'-');
    let id_ok = (1..=64).contains(&id.len()) && id.bytes().enumerate().all(id_char);
    let named = Path::new(file).file_name().and_then(|n| n.to_str()) == Some(&format!("{id}.json"));
    let repo = c["repository"].as_str().and_then(|r| r.split_once('/'));
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    let ok = o.len() == 8
        && CASE.split(' ').all(|k| o.contains_key(k))
        && id_ok
        && named
        && repo.is_some_and(|(a, b)| part(a) && part(b))
        && c["pull_request"].as_u64().is_some_and(|n| n > 0)
        && hex(&c["base"], 40)
        && hex(&c["head"], 40)
        && hex(&c["page_sha256"], 64)
        && strings(&c["tests"], relative)
        && c["commands"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(command));
    require!(ok => "case_unreadable", "case");
    Ok((c, digest(&bytes)))
}
/// The block of facts appended to the request's prompt.
fn facts(r: &Value, c: &Value) -> String {
    let tests: String = texts(&c["tests"]).map(|t| format!("- {t}\n")).collect();
    let commands = c["commands"].as_array().into_iter().flatten();
    let commands: String = commands.map(|v| format!("{v}\n")).collect();
    let (page, wt) = (&r["page"], &r["worktree"]);
    format!(
        "\n## Facts of this case (ap unit)\n\nPage: {}\nCopy: {}\nTest files (restored before your work is judged):\n{tests}Commands, in order, each an argument array run in the copy:\n{commands}The copy has no remote.\n",
        page.as_str().unwrap_or_default(),
        wt.as_str().unwrap_or_default()
    )
}
/// The checks before any effect; the answer is the case and its digest.
fn request(s: &mut impl System, r: &Value) -> Result<(Value, String), Failure> {
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.split(' ').any(|m| m == *k)) {
        return Err(error("invalid_input", k));
    }
    let p = |k: &str| path(r, k, k);
    let (file, clone, worktree) = (p("case")?, p("clone")?, p("worktree")?);
    let (page, out, acceptance) = (p("page")?, p("out")?, p("acceptance")?);
    let staged = r.get("implementer").is_some();
    let at = r.get("at").map_or(Some("base"), Value::as_str);
    // No stage ever works in a copy that holds the head.
    require!(at == Some("base") || (at == Some("head") && !staged) => "invalid_input", "at");
    seconds(r)?;
    if staged {
        stage(r, "implementer")?;
        require!(r["implementer"].get("resume").is_none() => "invalid_input", "implementer.resume");
    }
    let (c, sha) = read_case(file)?;
    let root = s.work_root();
    for (k, p) in [("page", page), ("out", out)] {
        require!(Path::new(p).starts_with(&root) => "outside_work_root", k);
    }
    require!(!Path::new(acceptance).starts_with(worktree) => "inside_worktree", "acceptance");
    let bytes = regular(page, "page_unreadable", "page")?;
    require!(c["page_sha256"] == digest(&bytes).as_str() => "case_page_differs", "page");
    if let Some(lead) = r["implementer"]["prompt"].as_str() {
        let field = |f: Failure| fail(f.code, &format!("implementer.{}", f.field), f.detail);
        crate::run::admit_stage(s, &stage_request(r, "implementer", lead)).map_err(field)?;
        let size = std::fs::metadata(lead).map_or(LIMIT, |m| m.len());
        require!(size + (facts(r, &c).len() as u64) < LIMIT => "prompt_unreadable", "implementer.prompt");
    }
    executable(acceptance)?;
    let has = |s: &mut _, what: &str| crate::review::git(s, clone, &["cat-file", "-t", what]);
    let (base, head) = (
        c["base"].as_str().unwrap_or_default(),
        c["head"].as_str().unwrap_or_default(),
    );
    for (k, commit) in [("case.base", base), ("case.head", head)] {
        require!(has(s, &format!("{commit}^{{commit}}")).is_ok() => "case_commit_missing", k);
    }
    for t in texts(&c["tests"]) {
        let kind = has(s, &format!("{head}:{t}"));
        require!(kind.is_ok_and(|k| k.trim() == "blob") => "case_commit_missing", "case.tests");
    }
    for suffix in FILES.split(' ') {
        for extra in RUN_FILES {
            let file = format!("{out}{suffix}{extra}");
            require!(Path::new(&file).symlink_metadata().is_err() => "out_exists", "out");
        }
    }
    require!(Path::new(worktree).symlink_metadata().is_err() => "worktree_exists", "worktree");
    Ok((c, sha))
}

fn steps(s: &mut impl System, r: &Value, c: &Value, u: &mut Unit) -> End {
    let f = |k: &str| r[k].as_str().unwrap_or_default();
    let (clone, wt, out, acceptance) = (f("clone"), f("worktree"), f("out"), f("acceptance"));
    let at = r["at"].as_str().unwrap_or("base");
    let mut commit = String::new();
    u.step("worktree", |_| {
        let (from, head) = (
            c[at].as_str().unwrap_or_default(),
            c["head"].as_str().unwrap_or_default(),
        );
        let mut words = vec!["-c", COPY, "bash", clone, wt, from, head];
        words.extend(texts(&c["tests"]));
        let reply = s.call("bash", &args(&words), &Value::Null, 1_800_000);
        let copied = reply.ok().filter(|r| r.success).map(|r| r.stdout);
        let copied = copied.map(|b| String::from_utf8_lossy(&b).trim().to_owned());
        commit = copied
            .filter(|c| c.len() == 40)
            .ok_or_else(|| stop("copy_failed", Value::Null))?;
        Ok(json!({"commit": commit}))
    })?;
    let mut failed = None;
    if let Some(lead) = r["implementer"]["prompt"].as_str() {
        u.begin("implementer");
        unchanged(&u.rec["prompt"], lead)?;
        let mut bytes = regular(lead, "digest_differs", "implementer.prompt")?;
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend(facts(r, c).as_bytes());
        let prompt = format!("{out}.implementer.prompt.md");
        create(&prompt, &bytes)?;
        u.rec["files"]["report"] = json!(format!("{out}.implementer.md"));
        let req = stage_request(r, "implementer", &prompt);
        match stage_run(s, u, "implementer", req) {
            Ok(detail) => u.end("passed", detail),
            Err(Stop { cause, .. }) if cause == "timeout" => {
                failed = Some(json!({"stage": "timeout"}));
                u.end("failed", failed.clone().unwrap_or_default());
            }
            Err(stopped) => return Err(stopped),
        }
    }
    if failed.is_none() {
        u.begin("acceptance");
        unchanged(&u.rec["acceptance"], acceptance)?;
        let log = format!("{out}.accept.log");
        u.rec["files"]["acceptance_output"] = json!(log);
        let file = r["case"].as_str().unwrap_or_default();
        let exit = accept(s, wt, acceptance, &log, seconds(r)?, &[file, clone]);
        let detail = json!({"exit": exit});
        if exit == Some(0) {
            u.end("passed", detail);
        } else if exit == Some(1) || exit == Some(124) {
            u.end("failed", detail.clone());
            failed = Some(detail);
        } else {
            return Err(stop("acceptance_failed", detail));
        }
    }
    u.begin("cleanup");
    let diff = format!("{out}.candidate.diff");
    let a = args(&["-c", DIFF, "bash", wt, &commit, &diff]);
    let written = s
        .call("bash", &a, &Value::Null, 600_000)
        .is_ok_and(|r| r.success);
    u.rec["files"]["candidate_diff"] = json!(diff);
    let removed = std::fs::remove_dir_all(wt).is_ok();
    let detail = json!({"diff": written, "copy": if removed { "removed" } else { "failed" }});
    let result = if written && removed {
        "passed"
    } else {
        "failed"
    };
    u.end(result, detail);
    u.rec["detail"] = failed.clone().unwrap_or_default();
    Ok(if failed.is_some() { "failed" } else { "passed" })
}

/// One case call. A refusal (exit 2) comes before any effect and writes no file.
pub(super) fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let (c, sha) = request(s, r)?;
    let file =
        |p: &str| regular(p, "io_error", "file").map(|b| json!({"path": p, "sha256": digest(&b)}));
    let lead = r["implementer"]["prompt"].as_str();
    // The bound in effect: the stage's own, or `ap run`'s default of the implementer (its first role).
    let bound = r["implementer"]["timeout_seconds"].as_u64();
    let stage_seconds = lead.map(|_| bound.unwrap_or(crate::run::ROLES[0].1));
    let rec = json!({"mode": "case", "repository": c["repository"], "clone": r["clone"], "worktree": r["worktree"],
        "branch": null, "task_key": null, "base": c["base"], "page": file(r["page"].as_str().unwrap_or_default())?,
        "write_set": null, "acceptance": file(r["acceptance"].as_str().unwrap_or_default())?,
        "verifier_prompt": null, "verdict_file": null, "merge": null, "pull_request": null, "head": null,
        "review": null, "merge_commit": null, "stages": {"implementer": null, "verifier": null},
        "files": {"gate": null, "acceptance_output": null, "report": null, "candidate_diff": null},
        "case": {"path": r["case"], "sha256": sha, "id": c["id"]}, "at": r["at"].as_str().unwrap_or("base"),
        "prompt": lead.map(file).transpose()?, "stage_seconds": stage_seconds, "state": "running",
        "step": null, "stopped_at": null, "cause": null, "detail": null, "steps": [], "start": now(), "end": null});
    claimed(r, rec, |u| steps(s, r, &c, u))
}
