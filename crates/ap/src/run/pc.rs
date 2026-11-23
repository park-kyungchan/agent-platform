//! A look on the PC (`specs/ap/run.md` R57 to R63): the placement by one ping and one probe, the
//! launch through `tailscale ssh` of the fixed script `check/stage.sh` with one payload, and the
//! fetch of the proxy's lines. Everything that reaches the PC is that script, its arguments, the
//! probe's and the fetch's texts and the payload; every `tailscale` child gets `PATH`, `HOME`, `USER`
//! and `LOGNAME` alone.
use super::{Request, egress::pc as proxy, record_path};
use crate::check::{self, Ctx, Pc, Stage, Stages, quote};
use crate::host::{System, digest, new_session_id};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::UNIX_EPOCH,
};

/// The PC's side of a look, run as `bash -c` (R59, R60).
const SCRIPT: &str = include_str!("../check/stage.sh");
/// The probe (R58): the stage account's login environment, then the CPU count, the one-minute load and
/// `MemAvailable` in kB; whether `PATH` finds `claude` and `bwrap`; what the proxy's `check` answers
/// (its text is the second argument, sent within the command: nothing is installed); the Claude
/// configuration directory; the end of the access token, which the third argument (a `python3 -I`
/// program) reads out of `<directory>/.credentials.json` and prints as one number or `-`; and the
/// `HEAD` file of the key's repository, read as text. It runs no Windows program and writes nothing
/// but what `check` makes and removes.
const PROBE: &str = r#"if [ -r "$HOME/.profile" ]; then . "$HOME/.profile" >/dev/null 2>&1 </dev/null; fi
cfg=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
found() { if command -v "$1" >/dev/null 2>&1; then echo yes; else echo no; fi; }
printf '%s %s %s\n' "$(nproc)" "$(cut -d ' ' -f 1 /proc/loadavg)" "$(sed -n 's/^MemAvailable: *\([0-9][0-9]*\) kB$/\1/p' /proc/meminfo 2>/dev/null)"
printf 'programs %s %s\n' "$(found claude)" "$(found bwrap)"
printf 'egress %s\n' "$(timeout 30 python3 -I -c "$2" check </dev/null 2>/dev/null | head -n 1)"
printf 'config %s\n' "$cfg"
printf 'expires %s\n' "$(timeout 10 python3 -I -c "$3" "$cfg/.credentials.json" </dev/null 2>/dev/null | head -n 1)"
printf 'head '
[ -L "$1/HEAD" ] || head -c 80 -- "$1/HEAD" 2>/dev/null
printf '\n'
exit 0"#;
/// The program that reads the access token's end: only the number `claudeAiOauth.expiresAt`, or `-`.
const EXPIRES: &str = r#"import json, math, sys
try:
    v = json.load(open(sys.argv[1]))["claudeAiOauth"]["expiresAt"]
    ok = isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)
    print(int(v) if ok else "-")
except Exception:
    print("-")
"#;
/// The fetch (R60): waits for the key's lock, which the PC's side holds to its end, and once it holds
/// it prints the lines file of the launch and removes it; without the lock in time it prints nothing,
/// leaves the file and fails.
const FETCH: &str = r#"work=$1 key=$2 nonce=$3
case $key in ????????????????) ;; *) exit 2 ;; esac
case $key in *[!0-9a-f]*) exit 2 ;; esac
case $nonce in '' | *[!0-9a-f-]*) exit 2 ;; esac
case $work in /*) ;; *) exit 2 ;; esac
mkdir -p -- "$work/locks" || exit 1
exec 9>"$work/locks/$key" || exit 1
flock -w 25 9 || exit 1
f=$work/egress/$nonce.lines
if [ -f "$f" ] && [ ! -L "$f" ]; then cat -- "$f" && rm -f -- "$f" || exit 1; fi
exit 0"#;
/// The placeholder the PC's side replaces with its proxy socket's path (R59).
const SOCKET: &str = "/ap-egress.sock";

/// Where a look runs: on the PC, with what the probe found, or here, for this reason (R58).
pub(super) enum Place {
    Pc(Box<Placed>),
    Here(&'static str),
}
/// The stage account of a look placed on the PC.
pub(super) struct Placed {
    stages: Stages,
    /// `pc-<the user of stages.ssh>`: the `account` of the launch (R63).
    pub(super) account: String,
    /// `<stages.ssh>:<the Claude configuration directory>` (R63).
    pub(super) config: String,
    /// The `HEAD` of the key's repository on the PC, if it has one.
    head: Option<String>,
    /// The key of the look's `cwd` (`check.md` Interface).
    key: String,
}

/// The words of a `tailscale` call as the port runs them (R58): `/usr/bin/env -i` with the four
/// variables of this process that a `tailscale` child may have, then `tailscale` and its words.
fn through_env(s: &impl System, words: &[&str]) -> (String, Vec<String>) {
    let mut args = vec!["-i".to_owned()];
    args.extend(environment(s).into_iter().map(|(k, v)| format!("{k}={v}")));
    args.push("tailscale".to_owned());
    args.extend(words.iter().map(|w| (*w).to_owned()));
    ("/usr/bin/env".to_owned(), args)
}
/// `PATH`, `HOME`, `USER` and `LOGNAME` of the port's environment, those that are set (the last of a
/// repeated name wins).
pub(super) fn environment(s: &impl System) -> Vec<(String, String)> {
    let all = s.environment();
    check::TAILSCALE_ENV
        .iter()
        .filter_map(|k| {
            all.iter()
                .rev()
                .find(|(n, _)| n == k)
                .map(|(n, v)| (n.clone(), v.clone()))
        })
        .collect()
}
/// One `tailscale` call through the port: its standard output when it succeeded.
fn call(s: &mut impl System, words: &[&str], ms: u64) -> Option<Vec<u8>> {
    let (program, args) = through_env(s, words);
    let reply = s.call(&program, &args, &Value::Null, ms).ok()?;
    reply.success.then_some(reply.stdout)
}
/// The key of a worktree (`check.md` Interface): the first 16 hexadecimal digits of the SHA-256 of its
/// resolved path.
fn key_of(cwd: &str) -> String {
    let real = fs::canonicalize(cwd)
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_else(|| cwd.to_owned());
    digest(real.as_bytes())[..16].to_owned()
}
fn host_of(ssh: &str) -> &str {
    ssh.split_once('@').map_or("", |(_, h)| h)
}

/// What the probe printed (R58).
struct Found {
    probe: check::Probe,
    programs: bool,
    ready: bool,
    config: String,
    expires: Option<u64>,
    head: Option<String>,
}
fn parse(text: &str) -> Option<Found> {
    let mut lines = text.lines();
    let probe = check::probe_line(lines.next()?)?;
    let (mut programs, mut ready, mut config, mut expires, mut head) =
        (None, None, None, None, None);
    for line in lines {
        if let Some(rest) = line.strip_prefix("programs ") {
            programs = Some(rest == "yes yes");
        } else if let Some(rest) = line.strip_prefix("egress ") {
            ready = Some(rest.starts_with("ap-egress ready "));
        } else if let Some(rest) = line.strip_prefix("config ") {
            config = Some(rest.to_owned());
        } else if let Some(rest) = line.strip_prefix("expires ") {
            expires = Some(rest.parse::<u64>().ok());
        } else if let Some(rest) = line.strip_prefix("head ") {
            head = Some(Some(rest.to_owned()).filter(|h| check::is_hex(h)));
        }
    }
    let config = config.filter(|c| c.starts_with('/') && !c.contains('\0'))?;
    Some(Found {
        probe,
        programs: programs?,
        ready: ready?,
        config,
        expires: expires?,
        head: head?,
    })
}
/// The placement of a look (R58): once per call, after the claim of `out`.
pub(super) fn place(s: &mut impl System, q: &Request) -> Place {
    let Some(
        pc @ Pc {
            stages: Some(stages),
            ..
        },
    ) = &q.pc
    else {
        return Place::Here("no_pc_in_policy");
    };
    let ping = [
        "ping",
        "--c",
        "1",
        "--timeout",
        "10s",
        "--until-direct=false",
        host_of(&stages.ssh),
    ];
    if call(s, &ping, 12_000).is_none() {
        return Place::Here("pc_unreachable");
    }
    let key = key_of(&q.cwd);
    let repo = format!("{}/repos/{key}", stages.work);
    let text = format!(
        "bash -c {} ap-stage-probe {} {} {}",
        quote(PROBE),
        quote(&repo),
        quote(proxy::PROGRAM),
        quote(EXPIRES)
    );
    let Some(out) = call(s, &["ssh", &stages.ssh, &text], 30_000) else {
        return Place::Here("pc_unreachable");
    };
    let Some(found) = parse(&String::from_utf8_lossy(&out)) else {
        return Place::Here("pc_unreachable");
    };
    if !found.programs || !found.ready {
        return Place::Here("pc_not_ready");
    }
    let now = s
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let bound = now + u128::from(q.timeout) * 1000;
    if !found.expires.is_some_and(|e| u128::from(e) > bound) {
        return Place::Here("pc_signed_out");
    }
    if let Some(reason) = check::placement(pc, &found.probe) {
        return Place::Here(reason);
    }
    let user = stages.ssh.split_once('@').map_or("", |(u, _)| u);
    Place::Pc(Box::new(Placed {
        account: format!("pc-{user}"),
        config: format!("{}:{}", stages.ssh, found.config),
        head: found.head,
        key,
        stages: stages.clone(),
    }))
}

/// What a launch on the PC is given besides the CLI's words: the payload on its standard input (a
/// file of this host, removed with the guard) and the words of the call.
pub(super) struct Staged {
    pub(super) input: File,
    pub(super) nonce: String,
    pub(super) words: Vec<String>,
    _stage: Stage,
}
/// A tar of `members` of `dir` into `file`.
fn tar(ctx: &Ctx, dir: &Path, file: &Path, members: &[&str]) -> bool {
    let mut c = ctx.command("tar");
    c.arg("-C")
        .arg(dir)
        .arg("-cf")
        .arg(file)
        .args(members)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    check::status_of(ctx, &mut c)
}
/// The bytes the CLI reads on its standard input: the prompt, with a `lead` before it, or an ask's
/// sentence alone (`input` of `run.rs`, as bytes).
fn cli_input(q: &Request, lead: Option<&str>, ask: bool) -> std::io::Result<Vec<u8>> {
    let mut bytes = lead.map(|l| l.as_bytes().to_vec()).unwrap_or_default();
    if !ask {
        bytes.extend(fs::read(&q.prompt)?);
    }
    Ok(bytes)
}
/// The launch on the PC (R59): the payload, a fresh nonce, and the words after `systemd-run … --` up
/// to the CLI's. `cli` are the CLI's words as R11 and R12 build them, `resume` whether it resumes.
pub(super) fn stage(
    s: &mut impl System,
    q: &Request,
    p: &Placed,
    (lead, ask): (Option<&str>, bool),
    session: &str,
    cli: &[String],
) -> Result<Staged, &'static str> {
    let nonce = new_session_id(s).map_err(|_| "nonce")?;
    let ctx = Ctx::own();
    let limits = q.limits.as_ref().ok_or("limits")?;
    let dir = ctx.temp_dir().join(format!("ap-stage.{nonce}"));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(|_| "stage")?;
    let guard = Stage(dir.clone());
    let tree = check::worktree(&ctx, &q.cwd).map_err(|_| "worktree")?;
    let first = check::payload(&ctx, &tree, p.head.as_deref(), &dir).map_err(|_| "payload")?;
    // What `check.md` R7 does not carry: the commit of origin/main and the history the bundle lacks,
    // each read-only file, and the proxy's text.
    let x = dir.join("x");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&x)
        .map_err(|_| "stage")?;
    let git = |words: &[&str]| check::git(&ctx, &tree.dir, words);
    let mut members = vec!["egress.py"];
    fs::write(x.join("egress.py"), proxy::PROGRAM).map_err(|_| "stage")?;
    let main = git(&[
        "rev-parse",
        "-q",
        "--verify",
        "refs/remotes/origin/main^{commit}",
    ])
    .and_then(check::line)
    .filter(|m| check::is_hex(m));
    if let Some(main) = &main {
        fs::write(x.join("main"), main).map_err(|_| "stage")?;
        members.push("main");
        let known = p
            .head
            .as_deref()
            .filter(|h| git(&["cat-file", "-e", &format!("{h}^{{commit}}")]).is_some())
            .map(|h| format!("^{h}"));
        let file = x.join("main.bundle");
        let file_text = file.to_str().ok_or("stage")?;
        let mut words = vec![
            "bundle",
            "create",
            "-q",
            file_text,
            "refs/remotes/origin/main",
            "^HEAD",
        ];
        words.extend(known.as_deref());
        // Empty when origin/main is already in HEAD's history: then the bundle of HEAD holds it.
        if git(&words).is_some() {
            members.push("main.bundle");
        }
    }
    if !limits.read_only.is_empty() {
        fs::create_dir(x.join("ro")).map_err(|_| "stage")?;
        let mut names = vec![];
        for (i, path) in limits.read_only.iter().enumerate() {
            fs::copy(path, x.join("ro").join(i.to_string())).map_err(|_| "read_only")?;
            names.extend_from_slice(path.as_bytes());
            names.push(0);
        }
        fs::write(x.join("ro-paths"), names).map_err(|_| "stage")?;
        members.extend(["ro", "ro-paths"]);
    }
    let second = dir.join("extra.tar");
    if !tar(&ctx, &x, &second, &members) {
        return Err("tar");
    }
    // One payload: `tar <bytes>`, the worktree's tar, `extra <bytes>`, the extra tar, the CLI's input.
    let frame = dir.join("frame");
    let mut out = File::create(&frame).map_err(|_| "stage")?;
    for (name, file) in [("tar", &first), ("extra", &second)] {
        let mut part = File::open(file).map_err(|_| "stage")?;
        let size = part.metadata().map_err(|_| "stage")?.len();
        writeln!(out, "{name} {size}").map_err(|_| "stage")?;
        std::io::copy(&mut part, &mut out).map_err(|_| "stage")?;
    }
    out.write_all(&cli_input(q, lead, ask).map_err(|_| "stage")?)
        .map_err(|_| "stage")?;
    drop(out);
    let input = File::open(&frame).map_err(|_| "stage")?;
    // The words of the PC's shell: each one quoted, since `tailscale ssh` joins them with spaces.
    let outside = proxy::outside(&q.actor_role, Path::new(SOCKET), &nonce);
    let mut words = vec![
        "bash".to_owned(),
        "-c".to_owned(),
        SCRIPT.to_owned(),
        "ap-stage".to_owned(),
        p.stages.work.clone(),
        p.key.clone(),
        nonce.clone(),
        q.timeout.to_string(),
        q.cwd.clone(),
        limits.scratch.clone(),
        session.to_owned(),
    ];
    words.extend(outside);
    words.push(q.pc_env.len().to_string());
    words.extend(q.pc_env.iter().map(|(k, v)| format!("{k}={v}")));
    words.extend(cli.iter().cloned());
    Ok(Staged {
        input,
        nonce,
        words,
        _stage: guard,
    })
}
/// The words of a launch on the PC after `systemd-run … --` (R59).
pub(super) fn launch_words(p: &Placed, staged: &Staged) -> Vec<String> {
    let remote: Vec<String> = staged.words.iter().map(|w| quote(w)).collect();
    vec![
        "tailscale".to_owned(),
        "ssh".to_owned(),
        p.stages.ssh.clone(),
        remote.join(" "),
    ]
}
/// The fetch of the proxy's lines (R60), after a launch on the PC has ended: the lines it printed
/// that carry the launch's nonce are appended to the egress log. The count, or None where the fetch
/// failed (no answer, or no lock in time), which appends nothing.
pub(super) fn fetch(s: &mut impl System, q: &Request, p: &Placed, nonce: &str) -> Option<usize> {
    let text = format!(
        "bash -c {} ap-stage-fetch {} {} {}",
        quote(FETCH),
        quote(&p.stages.work),
        quote(&p.key),
        quote(nonce)
    );
    let out = call(s, &["ssh", &p.stages.ssh, &text], 30_000)?;
    let dir: PathBuf = Ctx::own().temp_dir().join(format!("ap-fetch.{nonce}"));
    fs::DirBuilder::new().mode(0o700).create(&dir).ok()?;
    let guard = Stage(dir.clone());
    let file = dir.join("lines");
    fs::write(&file, out).ok()?;
    let taken = proxy::take(s, &file, nonce, &record_path(&q.out)).ok();
    drop(guard);
    taken
}
/// The record's `limits` of a launch on the PC (R63): the request's, without `writable`, `bwrap` and
/// `hidden` (this host's view, which the look does not have).
pub(super) fn limits_member(q: &Request) -> Value {
    let mut limits = json!({});
    if let Some(l) = &q.limits {
        limits["scratch"] = json!(l.scratch);
        for (k, v) in &l.given {
            limits[k] = v.clone();
        }
    }
    limits
}
