//! `ap check` (`specs/ap/check.md`) as a command, against a fake PC: a fake `tailscale` on the
//! PATH whose `ping` answers unless the file `down` exists and whose `ssh` runs the PC's command
//! here, as the PC's user would, with only a PC's own environment (`HOME` and `PATH` of the fake
//! PC); a stand-in `bwrap` on the fake PC's PATH that only changes directory and sets the variables
//! (one test runs the real `bwrap` instead). Every path is under a scratch directory: the policy
//! file, the record file, the build lock and the PC's work directory.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

const TAILSCALE: &str = r#"#!/bin/sh
# The fake PC. ping: answers unless $FAKE_PC/down exists. ssh: the probe reads $FAKE_PC/loadavg and
# 4 CPUs; the run keeps its command line and payload, and with $FAKE_PC/lose ends as a lost
# connection before the command, with $FAKE_PC/lose-after after it (its status line dropped).
pc=$FAKE_PC
case $1 in
ping)
  echo ping >> "$pc/calls"
  if [ -e "$pc/down" ]; then exit 1; fi
  exit 0 ;;
ssh)
  shift; dest=$1; shift; cmd=$*
  echo "ssh $dest" >> "$pc/calls"
  case $cmd in
  *ap-check-probe*)
    [ ! -e "$pc/probe-fails" ] || exit 255
    cmd=$(printf '%s' "$cmd" | sed "s#/proc/loadavg#$pc/loadavg#; s#(nproc)#(echo 4)#")
    exec env -i HOME="$pc/home" PATH="$pc/bin:/usr/bin:/bin" sh -c "$cmd" < /dev/null ;;
  esac
  printf '%s' "$cmd" > "$pc/remote"
  cat > "$pc/payload.tar"
  [ ! -e "$pc/lose" ] || exit 255
  if [ -e "$pc/lose-after" ]; then
    { env -i HOME="$pc/home" PATH="$pc/bin:/usr/bin:/bin" sh -c "$cmd" < "$pc/payload.tar" 2>&1 1>&3 | grep -av 'ap-check' >&2; } 3>&1
    exit 255
  fi
  exec env -i HOME="$pc/home" PATH="$pc/bin:/usr/bin:/bin" sh -c "$cmd" < "$pc/payload.tar" ;;
esac
exit 2
"#;

const BWRAP: &str = r#"#!/bin/sh
# A stand-in for bwrap: changes directory and sets the variables it is given, then runs the command.
while [ $# -gt 0 ]; do
  case $1 in
  --) shift; break ;;
  --chdir) cd "$2" || exit 1; shift 2 ;;
  --setenv) export "$2=$3"; shift 3 ;;
  --bind | --ro-bind) shift 3 ;;
  --dev | --proc | --uid | --gid) shift 2 ;;
  *) shift ;;
  esac
done
exec "$@"
"#;

const CHECK: &str = r#"#!/bin/sh
echo "check ran in $PWD"
if [ -n "${HOST_CARGO_LOCK:-}" ] && [ -e "$HOST_CARGO_LOCK" ]; then
  if flock -n "$HOST_CARGO_LOCK" true; then echo "lock free"; else echo "lock held"; fi
fi
echo "check err" >&2
exit 3
"#;

/// What the worktree holds as Git lists it: the commit, the index entries, the untracked files and
/// each listed path's kind, mode and bytes.
const MANIFEST: &str = r#"export LC_ALL=C
git rev-parse HEAD
git ls-files -s
echo --others
git ls-files --others --exclude-standard
echo --files
{ git ls-files -z --cached; git ls-files -z --others --exclude-standard; } | sort -zu |
while IFS= read -r -d '' f; do
  if [ -L "$f" ]; then echo "link $f $(readlink "$f")"
  elif [ -f "$f" ]; then echo "file $f $(stat -c %a "$f") $(sha256sum < "$f" | cut -d' ' -f1)"
  elif [ -e "$f" ]; then echo "other $f"
  else echo "absent $f"; fi
done"#;

/// Every file and link of the tree but `.git`, sorted.
const FOUND: &str = "export LC_ALL=C; find . -path ./.git -prune -o \\( -type f -o -type l \\) -print | sed 's#^\\./##' | sort";

struct World {
    root: PathBuf,
    repo: PathBuf,
}

fn write(path: &Path, text: &str, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn sh(dir: &Path, script: &str) -> String {
    let out = Command::new("bash")
        .args(["-c", script])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("../gitconfig"))
        .env("XDG_CONFIG_HOME", dir.join("../xdg"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{script}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}
fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_owned)
        .collect()
}

impl World {
    fn new() -> Self {
        Self::within(None, true)
    }
    /// A scratch root (under `parent` when given), a worktree whose origin is on GitHub, the fake
    /// PC with a policy that names it, and the stand-in `bwrap` unless `fake_bwrap` is false.
    fn within(parent: Option<&Path>, fake_bwrap: bool) -> Self {
        let mut mktemp = Command::new("mktemp");
        mktemp.arg("-d");
        if let Some(p) = parent {
            mktemp.args(["-p", p.to_str().unwrap()]);
        }
        let out = mktemp.output().unwrap();
        let root = fs::canonicalize(String::from_utf8(out.stdout).unwrap().trim()).unwrap();
        let w = World {
            repo: root.join("repo"),
            root,
        };
        write(&w.p("bin/tailscale"), TAILSCALE, 0o755);
        if fake_bwrap {
            write(&w.p("pc/bin/bwrap"), BWRAP, 0o755);
        }
        fs::create_dir_all(w.p("pc/bin")).unwrap();
        write(&w.p("pc/loadavg"), "0.10 0.10 0.10 1/100 1\n", 0o644);
        // The PC user's own global excludes hide every .txt: they must not apply to the tree.
        write(&w.p("pc/home/ignore"), "*.txt\n", 0o644);
        let pc_gitconfig = format!("[core]\n\texcludesFile = {}\n", w.s("pc/home/ignore"));
        write(&w.p("pc/home/.gitconfig"), &pc_gitconfig, 0o644);
        write(&w.p("gitconfig"), "", 0o644);
        write(&w.p("xdg/git/ignore"), "glob-ignored*\n", 0o644);
        w.policy(&format!(
            r#"{{"hosts": {{"pc": {{"ssh": "tester@fake-pc", "work": "{}", "busy_load": 0.75}}}}}}"#,
            w.s("pc/work")
        ));
        fs::create_dir_all(&w.repo).unwrap();
        write(&w.repo.join("check"), CHECK, 0o755);
        write(&w.repo.join("a.txt"), "a\n", 0o644);
        write(&w.repo.join("run.sh"), "#!/bin/sh\necho run\n", 0o755);
        write(&w.repo.join("sub/b.txt"), "b\n", 0o644);
        write(&w.repo.join(".gitignore"), "ignored*\n", 0o644);
        std::os::unix::fs::symlink("a.txt", w.repo.join("link")).unwrap();
        sh(
            &w.repo,
            "git init -q -b main . && git remote add origin https://github.com/example/repo.git \
             && git add -A && git -c user.name=t -c user.email=t@e commit -q -m one",
        );
        w
    }
    fn p(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
    fn s(&self, rel: &str) -> String {
        self.p(rel).to_str().unwrap().to_owned()
    }
    fn dir(&self) -> String {
        self.repo.to_str().unwrap().to_owned()
    }
    fn policy(&self, text: &str) {
        write(&self.p("policy.json"), text, 0o644);
    }
    fn no_policy(&self) {
        let _ = fs::remove_file(self.p("policy.json"));
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
        let path = format!("{}:{}", self.s("bin"), std::env::var("PATH").unwrap());
        c.arg("check")
            .args(args)
            .env("PATH", path)
            .env("FAKE_PC", self.p("pc"))
            .env("AP_POLICY_FILE", self.p("policy.json"))
            .env("AP_CHECKS_LOG", self.p("checks.jsonl"))
            .env("HOST_CARGO_LOCK", self.p("build.lock"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.p("gitconfig"))
            .env("XDG_CONFIG_HOME", self.p("xdg"))
            .stdin(Stdio::null());
        c
    }
    fn ap(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn records(&self) -> Vec<Value> {
        fs::read_to_string(self.p("checks.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    /// The key of the worktree on the PC: the first 16 hex of the SHA-256 of its path.
    fn key(&self, repo: &Path) -> String {
        format!("{:x}", Sha256::digest(repo.to_str().unwrap().as_bytes()))[..16].to_owned()
    }
    fn tree(&self) -> PathBuf {
        self.p(&format!("pc/work/trees/{}", self.key(&self.repo)))
    }
    fn payload_members(&self) -> Vec<String> {
        let mut m = lines(
            &Command::new("tar")
                .args(["-tf", &self.s("pc/payload.tar")])
                .output()
                .unwrap()
                .stdout,
        );
        m.sort();
        m
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = Command::new("chmod")
            .args(["-R", "u+rwX"])
            .arg(&self.root)
            .status();
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn error(out: &Output) -> Value {
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(out.stdout.is_empty());
    let text = String::from_utf8(out.stderr.clone()).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    serde_json::from_str::<Value>(&text).unwrap()["error"].clone()
}

// spec: specs/ap/check.md#R1
#[test]
fn r1_a_malformed_call_is_refused_before_any_effect() {
    let w = World::new();
    let d = w.dir();
    for (args, code, field) in [
        (vec![], "usage", "args"),
        (vec!["--timeout", "5", &d], "usage", "args"),
        (vec![&d, "--timeout"], "usage", "args"),
        (vec![&d, "--"], "usage", "args"),
        (vec![&d, "extra"], "usage", "args"),
        (
            vec![&d, "--timeout", "5", "--timeout", "6"],
            "usage",
            "args",
        ),
        (vec!["repo"], "invalid_input", "dir"),
        (vec![&d, "--timeout", "0"], "invalid_input", "timeout"),
        (vec![&d, "--timeout", "14401"], "invalid_input", "timeout"),
        (vec![&d, "--timeout", "1e3"], "invalid_input", "timeout"),
        (vec![&d, "--", ""], "invalid_input", "command"),
    ] {
        let e = error(&w.ap(&args));
        assert_eq!(
            (e["code"].as_str(), e["field"].as_str()),
            (Some(code), Some(field)),
            "{args:?}"
        );
    }
    // Neither the record nor the PC is touched.
    assert!(!w.p("checks.jsonl").exists());
    assert!(!w.p("pc/calls").exists());
    // The bounds themselves are taken.
    for t in ["1", "14400"] {
        assert_eq!(
            w.ap(&[&d, "--timeout", t, "--", "true"]).status.code(),
            Some(0)
        );
    }
}

// spec: specs/ap/check.md#R2
#[test]
fn r2_a_directory_that_is_not_a_worktree_on_github_is_refused() {
    let w = World::new();
    let sub = w.s("repo/sub");
    let plain = w.s("pc");
    let missing = w.s("missing");
    for dir in [&sub, &plain, &missing, &w.s("repo/a.txt")] {
        let e = error(&w.ap(&[dir]));
        assert_eq!(e["code"], "not_a_worktree", "{dir}");
        assert_eq!(e["field"], "dir");
    }
    sh(
        &w.repo,
        "git remote set-url origin https://gitlab.com/example/repo.git",
    );
    assert_eq!(error(&w.ap(&[&w.dir()]))["code"], "origin_not_github");
    sh(&w.repo, "git remote remove origin");
    assert_eq!(error(&w.ap(&[&w.dir()]))["code"], "origin_not_github");
    assert!(!w.p("checks.jsonl").exists());
    assert!(!w.p("pc/calls").exists());
}

// spec: specs/ap/check.md#R3
#[test]
fn r3_an_invalid_policy_is_refused_and_an_absent_one_runs_here() {
    let w = World::new();
    let d = w.dir();
    for (text, member) in [
        ("{", "file "),
        ("[]", "file "),
        (
            r#"{"hosts": {"pc": {"ssh": "nobody", "work": "/w", "busy_load": 0.5}}}"#,
            "hosts.pc.ssh ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "w", "busy_load": 0.5}}}"#,
            "hosts.pc.work ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 2}}}"#,
            "hosts.pc.busy_load ",
        ),
        (r#"{"hosts": {"vps": {}}}"#, "hosts "),
    ] {
        w.policy(text);
        let e = error(&w.ap(&[&d]));
        assert_eq!(e["code"], "policy_invalid", "{text}");
        assert_eq!(e["field"], "policy");
        assert!(e["detail"].as_str().unwrap().starts_with(member), "{e}");
    }
    assert!(!w.p("checks.jsonl").exists());
    // Without the file, and with a file without hosts: here, saying so.
    w.no_policy();
    let out = w.ap(&[&d]);
    assert_eq!(
        lines(&out.stderr)[0],
        "check: on this host (no_pc_in_policy)"
    );
    w.policy(r#"{"models": {"allowed": ["x"], "never": []}}"#);
    let out = w.ap(&[&d]);
    assert_eq!(
        lines(&out.stderr)[0],
        "check: on this host (no_pc_in_policy)"
    );
    assert!(!w.p("pc/calls").exists());
}

// spec: specs/ap/check.md#R4
// spec: specs/ap/check.md#R14
#[test]
fn r4_the_pc_runs_the_call_when_it_answers_and_is_not_busy() {
    let w = World::new();
    let d = w.dir();
    let tree = w.tree();
    let tree = tree.to_str().unwrap();
    let where_ = ["--", "sh", "-c", "echo \"in $PWD\""];
    let run = |reason: &str, place: &str| {
        let out = w.ap(&[&[d.as_str()][..], &where_].concat());
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(lines(&out.stderr)[0], reason);
        assert_eq!(lines(&out.stdout), [format!("in {place}")]);
        let r = w.records().pop().unwrap();
        (r["host"].clone(), r["reason"].clone())
    };
    // 3.00 is 0.75 of 4 CPUs: busy; just below it is not.
    write(&w.p("pc/loadavg"), "2.99 0.10 0.10 1/100 1\n", 0o644);
    assert_eq!(
        run("check: on pc (4 CPUs, load 2.99)", tree),
        ("pc".into(), Value::Null)
    );
    write(&w.p("pc/loadavg"), "3.00 0.10 0.10 1/100 1\n", 0o644);
    assert_eq!(
        run("check: on this host (pc_busy)", &d),
        ("local".into(), "pc_busy".into())
    );
    write(&w.p("pc/probe-fails"), "", 0o644);
    assert_eq!(
        run("check: on this host (pc_unreachable)", &d),
        ("local".into(), "pc_unreachable".into())
    );
    write(&w.p("pc/down"), "", 0o644);
    assert_eq!(
        run("check: on this host (pc_unreachable)", &d),
        ("local".into(), "pc_unreachable".into())
    );
    // The default command is ./check; its exit status is the call's.
    let _ = fs::remove_file(w.p("pc/down"));
    let _ = fs::remove_file(w.p("pc/probe-fails"));
    write(&w.p("pc/loadavg"), "0.20 0.10 0.10 1/100 1\n", 0o644);
    let out = w.ap(&[&d]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(lines(&out.stdout), [format!("check ran in {tree}")]);
    assert_eq!(
        lines(&out.stderr),
        ["check: on pc (4 CPUs, load 0.20)", "check err"]
    );
    w.no_policy();
    let out = w.ap(&[&d]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(lines(&out.stdout), [format!("check ran in {d}")]);
    assert_eq!(
        lines(&out.stderr),
        ["check: on this host (no_pc_in_policy)", "check err"]
    );
    // A command killed by a signal: 128 and the signal.
    let out = w.ap(&[&d, "--", "sh", "-c", "kill -TERM $$"]);
    assert_eq!(out.status.code(), Some(143));
    // A command that cannot be started here: 127.
    let out = w.ap(&[&d, "--", "./no-such-program"]);
    assert_eq!(out.status.code(), Some(127));
}

// spec: specs/ap/check.md#R5
#[test]
fn r5_here_a_command_other_than_check_takes_the_build_lock() {
    let w = World::new();
    w.no_policy();
    let d = w.dir();
    let probe = "if flock -n \"$HOST_CARGO_LOCK\" true; then echo free; else echo held; fi";
    // Without the lock file nothing is locked and nothing is made.
    let out = w.ap(&[&d, "--", "sh", "-c", "echo ran"]);
    assert_eq!(lines(&out.stdout), ["ran"]);
    assert!(!w.p("build.lock").exists());
    write(&w.p("build.lock"), "", 0o644);
    let out = w.ap(&[&d, "--", "sh", "-c", probe]);
    assert_eq!(lines(&out.stdout), ["held"]);
    // ./check takes the lock by itself, with or without arguments: it is not held for it.
    let out = w.ap(&[&d]);
    assert_eq!(
        lines(&out.stdout),
        [format!("check ran in {d}"), "lock free".into()]
    );
    let out = w.ap(&[&d, "--", "./check", "content"]);
    assert_eq!(
        lines(&out.stdout),
        [format!("check ran in {d}"), "lock free".into()]
    );
}

// spec: specs/ap/check.md#R6
#[test]
fn r6_the_pc_tree_is_the_worktree_as_git_lists_it() {
    let w = World::new();
    let d = w.dir();
    let state = |w: &World| {
        sh(
            &w.repo,
            "echo changed >> a.txt && echo s > staged.txt && git add staged.txt \
             && echo g > ghost.txt && git add ghost.txt && rm ghost.txt && rm sub/b.txt \
             && echo n > new.txt && ln -sfn new.txt newlink && printf x > tool.sh && chmod 700 tool.sh \
             && echo i > ignored-x && echo i > info-ignored-x && echo i > glob-ignored-x \
             && mkdir -p .git/info && echo 'info-ignored*' > .git/info/exclude",
        );
    };
    state(&w);
    let manifest = |w: &World| {
        let local = sh(&w.repo, MANIFEST);
        let out = w.ap(&[&d, "--", "bash", "-c", MANIFEST]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(lines(&out.stderr)[0].starts_with("check: on pc"), "{out:?}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), local);
        // Nothing else is in the tree: the listed files that exist and no other.
        let listed: Vec<String> = local
            .lines()
            .filter(|l| l.starts_with("file ") || l.starts_with("link "))
            .map(|l| l.split(' ').nth(1).unwrap().to_owned())
            .collect();
        assert_eq!(sh(&w.tree(), FOUND).lines().collect::<Vec<_>>(), listed);
        local
    };
    let first = manifest(&w);
    for want in [
        "absent ghost.txt",
        "absent sub/b.txt",
        "file tool.sh 700 ",
        "link newlink new.txt",
        "link link a.txt",
        "file run.sh 755 ",
        "file staged.txt 644 ",
    ] {
        assert!(
            first.lines().any(|l| l.starts_with(want)),
            "{want}:\n{first}"
        );
    }
    assert!(!first.contains("ignored-x"), "{first}");
    assert!(
        first.contains("--others\nnew.txt\nnewlink\ntool.sh\n"),
        "{first}"
    );
    assert!(w.payload_members().contains(&"head.bundle".to_owned()));
    // A new commit reaches the tree as a bundle of what it lacks; the same commit needs none.
    sh(
        &w.repo,
        "git add -A && git -c user.name=t -c user.email=t@e commit -q -m two && echo again >> run.sh && git rm -q --cached a.txt",
    );
    manifest(&w);
    let pc_head = sh(&w.tree(), "git rev-parse HEAD");
    assert_eq!(pc_head, sh(&w.repo, "git rev-parse HEAD"));
    assert!(w.payload_members().contains(&"head.bundle".to_owned()));
    manifest(&w);
    assert!(!w.payload_members().contains(&"head.bundle".to_owned()));
}

// spec: specs/ap/check.md#R7
#[test]
fn r7_only_the_payload_and_the_script_reach_the_pc() {
    let w = World::new();
    let d = w.dir();
    sh(&w.repo, "echo n > new.txt && echo i > ignored-secret");
    let secret = "s3cr3t-value-of-this-host";
    let out = w
        .command(&[&d, "--", "sh", "-c", "echo ${SECRET_PROBE:-absent}"])
        .env("SECRET_PROBE", secret)
        .output()
        .unwrap();
    assert_eq!(lines(&out.stdout), ["absent"]);
    assert_eq!(
        w.payload_members(),
        [
            "exclude",
            "global-excludes",
            "head",
            "head.bundle",
            "index-info",
            "tree.tar"
        ]
    );
    // The tree's tar holds exactly the listed files.
    fs::create_dir(w.p("unpacked")).unwrap();
    sh(&w.root, "tar -xf pc/payload.tar -C unpacked");
    let mut members = lines(
        &Command::new("tar")
            .args(["-tf", &w.s("unpacked/tree.tar")])
            .output()
            .unwrap()
            .stdout,
    );
    members.sort();
    assert_eq!(
        members,
        [
            ".gitignore",
            "a.txt",
            "check",
            "link",
            "new.txt",
            "run.sh",
            "sub/b.txt"
        ]
    );
    // The command line is the fixed script and its arguments: nothing of this host's environment.
    let remote = fs::read_to_string(w.p("pc/remote")).unwrap();
    assert!(
        remote.starts_with("bash -c '# The PC'\\''s side of `ap check`"),
        "{remote}"
    );
    assert!(!remote.contains(secret));
    let payload = fs::read(w.p("pc/payload.tar")).unwrap();
    assert!(
        !payload
            .windows(secret.len())
            .any(|x| x == secret.as_bytes())
    );
    assert!(
        remote.ends_with(" 'sh' '-c' 'echo ${SECRET_PROBE:-absent}'"),
        "{remote}"
    );
}

/// Whether the real `bwrap` can make a user namespace here (not on every CI runner).
fn bwrap_works() -> bool {
    Command::new("bwrap")
        .args([
            "--unshare-user",
            "--uid",
            "0",
            "--gid",
            "0",
            "--unshare-pid",
            "--die-with-parent",
            "--new-session",
        ])
        .args([
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--",
            "true",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// spec: specs/ap/check.md#R8
#[test]
fn r8_on_the_pc_the_command_is_uid_0_with_a_read_only_root_but_its_directories() {
    if !bwrap_works() {
        eprintln!("skipped: bwrap cannot make a user namespace on this host");
        return;
    }
    let w = World::within(Some(Path::new(env!("CARGO_TARGET_TMPDIR"))), false);
    let script = "id -u; for p in \"$PWD/x\" \"$CARGO_TARGET_DIR/x\" \"$CARGO_HOME/x\" /tmp/x \"$HOME/x\" /etc/ap-check-x \"$CARGO_TARGET_DIR/../x\"; do \
                  if touch \"$p\" 2>/dev/null; then echo \"rw $p\"; else echo \"ro $p\"; fi; done; echo \"tmp $TMPDIR\"";
    let out = w.ap(&[&w.dir(), "--", "sh", "-c", script]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let key = w.key(&w.repo);
    let work = w.s("pc/work");
    assert_eq!(
        lines(&out.stdout),
        [
            "0".to_owned(),
            format!("rw {work}/trees/{key}/x"),
            format!("rw {work}/targets/{key}/x"),
            format!("rw {work}/cargo-home/x"),
            "rw /tmp/x".into(),
            format!("ro {}/x", w.s("pc/home")),
            "ro /etc/ap-check-x".into(),
            format!("ro {work}/targets/{key}/../x"),
            "tmp /tmp".into(),
        ]
    );
    // The temporary directory goes with the call.
    assert_eq!(fs::read_dir(w.p("pc/work/tmp")).unwrap().count(), 0);
}

// spec: specs/ap/check.md#R9
#[test]
fn r9_the_target_persists_per_worktree_and_one_worktree_waits_for_itself() {
    let w = World::new();
    let d = w.dir();
    let mark = [
        "--",
        "sh",
        "-c",
        "cat \"$CARGO_TARGET_DIR/mark\" 2>/dev/null || echo none; echo kept > \"$CARGO_TARGET_DIR/mark\"",
    ];
    let other = w.p("other");
    sh(
        &w.root,
        "git clone -q repo other && git -C other remote set-url origin https://github.com/example/other.git",
    );
    let o = other.to_str().unwrap().to_owned();
    assert_eq!(
        lines(&w.ap(&[&[d.as_str()][..], &mark].concat()).stdout),
        ["none"]
    );
    assert_eq!(
        lines(&w.ap(&[&[d.as_str()][..], &mark].concat()).stdout),
        ["kept"]
    );
    let out = w.ap(&[&[o.as_str()][..], &mark].concat());
    assert_eq!(lines(&out.stdout), ["none"], "{out:?}");
    // The lock of one worktree held: its call waits to its timeout; another's runs meanwhile.
    fs::create_dir_all(w.p("pc/work/locks")).unwrap();
    let held = fs::File::create(w.p(&format!("pc/work/locks/{}", w.key(&w.repo)))).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let t = Instant::now();
    let waiting = w
        .command(&[&d, "--timeout", "4", "--", "echo", "ran"])
        .output()
        .unwrap();
    assert_eq!(waiting.status.code(), Some(124), "{waiting:?}");
    assert!(waiting.stdout.is_empty());
    assert!(t.elapsed() < Duration::from_secs(20));
    let out = w.ap(&[&o, "--", "echo", "ran"]);
    assert_eq!(lines(&out.stdout), ["ran"]);
    drop(held);
}

fn running(token: &str) -> bool {
    Command::new("pgrep")
        .args(["-f", token])
        .stdout(Stdio::null())
        .status()
        .unwrap()
        .success()
}

// spec: specs/ap/check.md#R10
#[test]
fn r10_at_the_timeout_the_status_is_124_and_no_process_is_left() {
    let w = World::new();
    let d = w.dir();
    for (place, token) in [
        ("pc", "ap-check-timeout-pc-a1"),
        ("here", "ap-check-timeout-here-b2"),
    ] {
        if place == "here" {
            w.no_policy();
        }
        let t = Instant::now();
        let out = w.ap(&[&d, "--timeout", "2", "--", "sh", "-c", "sleep 60; :", token]);
        assert_eq!(out.status.code(), Some(124), "{place}: {out:?}");
        assert!(
            t.elapsed() < Duration::from_secs(15),
            "{place}: {:?}",
            t.elapsed()
        );
        assert!(!running(token), "{place}: a process of the call is left");
        let r = w.records().pop().unwrap();
        assert_eq!(r["exit"], 124);
    }
}

// spec: specs/ap/check.md#R11
#[test]
fn r11_a_lost_call_runs_here_once_and_the_commands_255_is_its_own() {
    let w = World::new();
    let d = w.dir();
    let tree = w.tree();
    let where_ = ["--", "sh", "-c", "echo \"in $PWD\""];
    let args = [&[d.as_str()][..], &where_].concat();
    write(&w.p("pc/lose"), "", 0o644);
    let out = w.ap(&args);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        lines(&out.stderr),
        [
            "check: on pc (4 CPUs, load 0.10)",
            "check: on this host (pc_lost)"
        ]
    );
    assert_eq!(lines(&out.stdout), [format!("in {d}")]);
    let r = w.records().pop().unwrap();
    assert_eq!(
        (r["host"].as_str(), r["reason"].as_str()),
        (Some("local"), Some("pc_lost"))
    );
    // The command ran there, but its status never came back: once more here.
    fs::remove_file(w.p("pc/lose")).unwrap();
    write(&w.p("pc/lose-after"), "", 0o644);
    let out = w.ap(&args);
    assert_eq!(
        lines(&out.stdout),
        [format!("in {}", tree.to_str().unwrap()), format!("in {d}")]
    );
    assert_eq!(
        lines(&out.stderr).last().unwrap(),
        "check: on this host (pc_lost)"
    );
    // The command's own 255 comes back as its status.
    fs::remove_file(w.p("pc/lose-after")).unwrap();
    let out = w.ap(&[&d, "--", "sh", "-c", "exit 255"]);
    assert_eq!(out.status.code(), Some(255));
    assert_eq!(lines(&out.stderr), ["check: on pc (4 CPUs, load 0.10)"]);
    let r = w.records().pop().unwrap();
    assert_eq!(
        (r["host"].as_str(), r["exit"].as_i64()),
        (Some("pc"), Some(255))
    );
}

// spec: specs/ap/check.md#R12
#[test]
fn r12_a_tree_unused_for_seven_days_is_removed_under_its_lock() {
    let w = World::new();
    let work = w.p("pc/work");
    let old = |rel: &str| {
        let p = work.join(rel);
        fs::create_dir_all(&p).unwrap();
        write(&p.join("f"), "x", 0o444);
        sh(
            &w.root,
            &format!("touch -d '8 days ago' '{}'", p.to_str().unwrap()),
        );
    };
    let fresh = |rel: &str| write(&work.join(rel).join("f"), "x", 0o644);
    let (a, b, c, l) = (
        "aaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbb",
        "cccccccccccccccc",
        "dddddddddddddddd",
    );
    old(&format!("trees/{a}"));
    old(&format!("targets/{a}"));
    write(&work.join(format!("used/{a}")), "", 0o644);
    sh(&w.root, &format!("touch -d '8 days ago' pc/work/used/{a}"));
    fresh(&format!("trees/{b}"));
    write(&work.join(format!("used/{b}")), "", 0o644);
    old(&format!("trees/{c}"));
    old("trees/not-a-key");
    old(&format!("trees/{l}"));
    write(&w.p("pc/outside/f"), "x", 0o644);
    sh(&w.root, "touch -d '8 days ago' pc/outside pc/outside/f");
    fs::create_dir_all(work.join("locks")).unwrap();
    let held = fs::File::create(work.join(format!("locks/{l}"))).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let out = w.ap(&[&w.dir(), "--", "true"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    for (rel, kept) in [
        (format!("trees/{a}"), false),
        (format!("targets/{a}"), false),
        (format!("used/{a}"), false),
        (format!("trees/{b}"), true),
        (format!("trees/{c}"), false),
        ("trees/not-a-key".into(), true),
        (format!("trees/{l}"), true),
    ] {
        assert_eq!(work.join(&rel).exists(), kept, "{rel}");
    }
    assert!(w.p("pc/outside/f").exists());
    assert!(w.tree().join("check").exists());
}

// spec: specs/ap/check.md#R13
#[test]
fn r13_output_streams_back_each_to_its_own() {
    let w = World::new();
    let d = w.dir();
    for place in ["pc", "here"] {
        if place == "here" {
            w.no_policy();
        }
        let out = w.ap(&[&d, "--", "sh", "-c", "echo out; echo err >&2; echo out2"]);
        assert_eq!(lines(&out.stdout), ["out", "out2"], "{place}");
        assert_eq!(&lines(&out.stderr)[1..], ["err"], "{place}");
        // The first line arrives while the command still runs.
        let mut child = w
            .command(&[&d, "--", "sh", "-c", "echo first; sleep 3; echo second"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        let at_first = Instant::now();
        let mut second = String::new();
        reader.read_line(&mut second).unwrap();
        assert_eq!((first.as_str(), second.as_str()), ("first\n", "second\n"));
        assert!(
            at_first.elapsed() > Duration::from_secs(2),
            "{place}: the first line came at the end"
        );
        assert!(child.wait().unwrap().success());
    }
}

// spec: specs/ap/check.md#R15
#[test]
fn r15_each_call_appends_one_record_line() {
    let w = World::new();
    let d = w.dir();
    let head = sh(&w.repo, "git rev-parse HEAD").trim().to_owned();
    w.ap(&[&d, "--", "sh", "-c", "exit 7"]);
    w.no_policy();
    w.ap(&[&d]);
    let r = w.records();
    assert_eq!(r.len(), 2);
    let keys = |v: &Value| {
        let mut k: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let want = [
        "command",
        "dir",
        "end",
        "exit",
        "head",
        "host",
        "reason",
        "repository",
        "seconds",
        "start",
    ];
    for v in &r {
        assert_eq!(keys(v), want);
        assert_eq!(v["dir"], d.as_str());
        assert_eq!(v["repository"], "example/repo");
        assert_eq!(v["head"], head.as_str());
        assert!(v["seconds"].as_f64().unwrap() >= 0.0);
        for t in ["start", "end"] {
            let s = v[t].as_str().unwrap();
            assert!(
                s.len() == 20 && s.ends_with('Z') && s.as_bytes()[10] == b'T',
                "{s}"
            );
        }
    }
    assert_eq!(
        (
            r[0]["host"].as_str(),
            &r[0]["reason"],
            r[0]["exit"].as_i64()
        ),
        (Some("pc"), &Value::Null, Some(7))
    );
    assert_eq!(r[0]["command"], serde_json::json!(["sh", "-c", "exit 7"]));
    assert_eq!(
        (
            r[1]["host"].as_str(),
            r[1]["reason"].as_str(),
            r[1]["exit"].as_i64()
        ),
        (Some("local"), Some("no_pc_in_policy"), Some(3))
    );
    assert_eq!(r[1]["command"], serde_json::json!(["./check"]));
    // A record that cannot be written is said, and the status stays the command's.
    let out = w
        .command(&[&d, "--", "sh", "-c", "exit 5"])
        .env("AP_CHECKS_LOG", w.p("missing/checks.jsonl"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(
        lines(&out.stderr)
            .last()
            .unwrap()
            .starts_with("check: the record could not be appended")
    );
}
