//! One stage of a unit: one headless Claude Code (or Codex, Muse Code or Antigravity CLI) session in a transient systemd
//! scope under `agents.slice`, ended at its time bound, its exit or (a reviewer or a writer only)
//! its first `result` event, after which the whole scope is stopped. One record `<out>.run.json` per run; no
//! other ledger, no store write.
use crate::host::{
    Failure, LIMIT, System, args, digest, error, fail, new_session_id, require, text,
};
use crate::store;
use serde_json::{Value, json};
mod egress;
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt},
    },
    path::{Component, Path, PathBuf},
    process::{Child, ExitCode},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SLICE: &str = "agents.slice";
/// The default time bound of each role, in seconds.
pub(crate) const ROLES: [(&str, u64); 4] = [
    ("implementer", 5_400),
    ("fixer", 3_600),
    ("reviewer", 3_300),
    ("writer", 5_400),
];
/// A stage that ends at its first `result` and starts over on a next account without the
/// continuation: a reviewer, and a page's writer, which is as a reviewer there (R15, R20, R21, R23,
/// R53).
fn first_result(role: &str) -> bool {
    role == "reviewer" || role == "writer"
}
pub(crate) const MODES: [&str; 6] = [
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "manual",
    "dontAsk",
    "plan",
];
/// The fixed short list of variables taken from this process; nothing else is inherited.
pub(crate) const KEEP: [&str; 10] = [
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "TERM", "TMPDIR", "TZ",
];
const MEMBERS: [&str; 28] = [
    "role",
    "runtime",
    "account",
    "next_accounts",
    "model",
    "effort",
    "permission_mode",
    "cwd",
    "prompt",
    "out",
    "timeout_seconds",
    "expect",
    "resume",
    "continue_dirty",
    "integration",
    "mcp_config",
    "env",
    "bind",
    "skills",
    "memory",
    "answer_schema",
    "compact_window",
    "unit",
    "intent",
    "store",
    "git_author",
    "ont_role",
    "limits",
];
/// The largest file of `answer_schema`, in bytes.
const SCHEMA_MAX: u64 = 65_536;
/// The longest prompt that is one argument of a program, in bytes: Linux takes 131072 with the NUL.
const ARGUMENT_MAX: usize = 131_071;
/// The program that holds a stage with `limits` to its home and its scratch directory
/// (`specs/ap/run-limits.md`).
pub(crate) const BWRAP: &str = "/usr/bin/bwrap";
/// The levels an `agy` slug ends with: the `effort` of its stage.
const LEVELS: [&str; 4] = ["low", "medium", "high", "xhigh"];
/// The unit's own bound runs this long past the request's: `ap run` stops the unit first, and the
/// bound still ends it when `ap run` is gone.
const GRACE: u64 = 10;
const MAX_TIMEOUT: u64 = 86_400;
/// The Bash tool's default and maximum time limit, in milliseconds: a check of 40 minutes runs in
/// the foreground of one call.
const BASH_MS: &str = "2700000";
/// Only the account's settings load: a worktree's `.claude/settings*.json`, which an earlier
/// stage may have written, cannot steer this one (shown live: its `env` reached the shell).
const SOURCES: &str = "user";
/// What a launch after a launch that worked and was cut reads before the prompt: one line, one
/// empty line.
const CONTINUATION: &str = "A session of this stage on another account was cut by a usage limit after it had begun to work. This directory may hold its work, committed or not. First read the state it left: in a Git worktree `git status`, `git log --oneline origin/main..HEAD` and `git diff HEAD`. Keep what serves the task, do not start over, and go on until the task below is done.\n\n";
/// What a launch that follows a cut launch of a resumed call reads before the prompt: one line, one
/// empty line. `{P}` and `{A}` are the resumed record's prompt and answer paths; the last three
/// sentences are for an implementer or a fixer only.
const MOVED: &str = "This stage was to resume an earlier session, and a usage limit cut that session on its account. You are a new session and hold nothing of what it read or wrote: before anything else read completely the prompt it was given, the file {P}, and the answer it gave, the file {A}. The task below was written for that session: where it speaks of what you did, wrote or were told, it means that session, its answer and its prompt.";
const MOVED_WORK: &str = " This directory holds its work, committed or not. Read the state it left: in a Git worktree `git status`, `git log --oneline origin/main..HEAD` and `git diff HEAD`. Keep what serves the task, do not start over, and go on until the task below is done.";
/// What the session of a Claude stage reads when it is asked again (R20): its answer's first line
/// did not meet `expect.first_line`. One sentence, one newline; no prompt file follows.
const ASK: &str = "Your last answer did not begin with the line your task asks it to begin with: give the whole answer again with that line as its first line and nothing before it, and post, push, send or change nothing a second time.\n";
/// What the session reads when its answer's first line met `expect.first_line` and its second line
/// missed `expect.second_line` (R20). One sentence, one newline.
const ASK_SECOND: &str = "Your last answer did not have as its second line the line your task asks for: give the whole answer again with the same first line, that line as its second line and nothing before them, and post, push, send or change nothing a second time.\n";
/// The exit of a run whose stage may have run but whose answer or record could not be written.
const WRITE_FAILED: u8 = 5;
/// The states `systemctl is-active` answers while something of a unit may live.
const ALIVE: [&str; 5] = [
    "active",
    "activating",
    "deactivating",
    "reloading",
    "refreshing",
];

/// The CLI a stage runs on. The only value of the request's `runtime` after it is read: every
/// place that depends on it matches on it with one arm per variant.
#[derive(Clone, Copy, Default, PartialEq)]
enum Runtime {
    #[default]
    Claude,
    Codex,
    Muse,
    Agy,
}
impl Runtime {
    fn of(word: Option<&Value>) -> Option<Self> {
        match word.map(Value::as_str) {
            None | Some(Some("claude")) => Some(Self::Claude),
            Some(Some("codex")) => Some(Self::Codex),
            Some(Some("muse")) => Some(Self::Muse),
            Some(Some("agy")) => Some(Self::Agy),
            Some(_) => None,
        }
    }
    /// The word of the record, made here only.
    fn word(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Muse => "muse",
            Self::Agy => "agy",
        }
    }
    /// The members of the request this runtime does not build (each refuses by its name).
    fn unbuilt(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &[],
            Self::Codex => &["skills", "memory", "integration", "compact_window"],
            Self::Muse | Self::Agy => &[
                "skills",
                "memory",
                "integration",
                "compact_window",
                "answer_schema",
            ],
        }
    }
    /// The roles this runtime builds: Codex has no reviewer, and an `agy` stage no other role (its
    /// `HOME` is the account, so a writing stage's `git`, `gh` and `cargo` lose their files); a
    /// page's writer runs on Claude alone (R53).
    fn role_ok(self, role: &str) -> bool {
        match self {
            Self::Claude => true,
            Self::Muse => role != "writer",
            Self::Codex => role != "reviewer" && role != "writer",
            Self::Agy => role == "reviewer",
        }
    }
    fn typed_answer(self) -> bool {
        match self {
            Self::Claude => true,
            Self::Codex | Self::Muse | Self::Agy => false,
        }
    }
    /// No approval and no sandbox of the CLI's own, one account and no MCP file.
    fn bare(self) -> bool {
        match self {
            Self::Claude => false,
            Self::Codex | Self::Muse | Self::Agy => true,
        }
    }
    /// The names `env` may not hold, so that the account named in the record is the one that answers.
    fn reserved(self, name: &str) -> bool {
        match self {
            Self::Claude => false,
            Self::Codex => name.starts_with("CODEX_") || name.starts_with("OPENAI_"),
            Self::Muse => name.starts_with("MUSE_") || name == "XDG_CONFIG_HOME",
            Self::Agy => name.starts_with("AGY_") || name.starts_with("GEMINI_"),
        }
    }
    /// A value that reaches the CLI as an argument of its own must not read as a flag there.
    fn argument_ok(self, value: &str) -> bool {
        match self {
            Self::Claude | Self::Codex => true,
            Self::Muse | Self::Agy => !value.starts_with('-'),
        }
    }
    /// An `agy` slug carries the level of its stage, which the CLI is not given apart from it.
    fn effort_ok(self, model: &str, effort: &str) -> bool {
        match self {
            Self::Claude | Self::Codex | Self::Muse => true,
            Self::Agy => LEVELS.contains(&effort) && model.rsplit('-').next() == Some(effort),
        }
    }
    /// The prompt reaches an `agy` stage as one argument of the program.
    fn prompt_argument(self) -> bool {
        match self {
            Self::Claude | Self::Codex | Self::Muse => false,
            Self::Agy => true,
        }
    }
    /// A Muse account is a directory named `muse`; an `agy` account is the template of the stage's
    /// homes: it holds the login token and no `.gemini/config`, where the CLI reads what is
    /// registered for it.
    fn account_named(self, account: &str) -> bool {
        let (home, token) = (
            Path::new(account).join(".gemini"),
            "antigravity-cli/antigravity-oauth-token",
        );
        match self {
            Self::Claude | Self::Codex => true,
            Self::Muse => Path::new(account).file_name().is_some_and(|n| n == "muse"),
            Self::Agy => {
                let config = std::fs::symlink_metadata(home.join("config"));
                home.join(token).metadata().is_ok_and(|m| m.is_file())
                    && config.is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            }
        }
    }
}

struct Request {
    role: String,
    accounts: Vec<String>,
    model: String,
    effort: String,
    mode: String,
    cwd: String,
    prompt: String,
    prompt_sha256: String,
    out: String,
    timeout: u64,
    expect_file: Option<String>,
    expect_line: Option<String>,
    /// `expect.second_line`: given only with `expect_line` (R2).
    expect_second: Option<String>,
    /// The file of `answer_schema`: its path, text and digest as read.
    schema: Option<(String, String, String)>,
    mcp: Vec<String>,
    /// The session, account and record path of the earlier run this one continues.
    resume: Option<(String, String, String)>,
    /// The line a launch after a cut one of this resumed call reads first; None when the resumed
    /// record's paths do not allow it (then a cut ends the call).
    moved: Option<String>,
    /// The expected file as it was before the session's first launch.
    expect_before: Value,
    /// The environment of the CLI (and of `git`), without `CLAUDE_CONFIG_DIR`.
    base_env: Vec<(String, String)>,
    /// The systemd unit whose end stops this stage's unit.
    bind: Option<String>,
    /// The account's skills and slash commands, and its auto-memory, load only when asked.
    skills: bool,
    memory: bool,
    runtime: Runtime,
    /// A Codex stage on a resume: the thread's tokens before this run.
    spent: Option<[u64; 4]>,
    /// The compaction window the stated `CLAUDE_CODE_AUTO_COMPACT_WINDOW` gets (a Claude stage).
    compact_window: Option<u64>,
    /// An `agy` stage: the prompt's text, the one argument that carries it.
    argument: String,
    /// An `agy` stage on a resume: the home of the record, where the conversation lives.
    home: Option<String>,
    /// The policy file as read: its path and digest (`specs/ap/policy.md`), None without one.
    policy: Option<(String, String)>,
    /// `unit` or `intent` and its key (`specs/ap/run-object.md`); the store its calls name.
    target: Option<(&'static str, String)>,
    store: Option<String>,
    /// The stage's role as the actor's claim: `verifier` for a reviewer that says so.
    actor_role: String,
    /// The scratch directory of `limits`, as written, and the home its hidden paths are under
    /// (`specs/ap/run-limits.md`).
    limits: Option<Limits>,
}
/// What a request with `limits` holds: its scratch directory, the paths it writes, binds over or
/// reads only (R1, R5), whether `~/.config/gh` is shown (R10), and the home of the passwd entry of
/// the user `ap run` runs as, where the hidden list's `~` points (R10).
struct Limits {
    scratch: String,
    home: String,
    write: Vec<String>,
    over: Vec<(String, String)>,
    read_only: Vec<String>,
    show: bool,
    /// `over`, `read_only` and `show` as the request gave them, for the record (R7).
    given: serde_json::Map<String, Value>,
}
/// The one path `show` may name (R1, R10).
const SHOWN: &str = "~/.config/gh";
impl Limits {
    /// The paths of the hidden list that exist now and are equal to, inside or hold the target of
    /// a bind of the launch (`targets`), but the one `show` names (R10).
    fn hidden(&self, work_root: &Path, targets: &[String]) -> Vec<(String, bool)> {
        let shown = Path::new(&self.home).join(&SHOWN[2..]);
        let mut hidden = hidden_now(&self.home, work_root);
        hidden.retain(|(p, _)| !self.show || Path::new(p) != shown);
        hidden.retain(|(p, _)| targets.iter().any(|t| related(p, t)));
        hidden
    }
    /// The targets of the binds the request names, beside the scratch directory: the `write`
    /// paths, the `over` targets and the `read_only` paths (R5, R10).
    fn targets(&self) -> Vec<String> {
        let targets = self.over.iter().map(|(_, t)| t.clone());
        [self.scratch.clone()]
            .into_iter()
            .chain(self.write.iter().cloned())
            .chain(targets)
            .chain(self.read_only.iter().cloned())
            .collect()
    }
    /// The `bwrap` words of the paths the request names, after the scratch directory: `write`,
    /// then `over`, then `read_only`, each in its order (R5).
    fn named(&self) -> Vec<String> {
        let mut words = vec![];
        for p in &self.write {
            words.extend(args(&["--bind", p, p]));
        }
        for (source, target) in &self.over {
            words.extend(args(&["--bind", source, target]));
        }
        for p in &self.read_only {
            words.extend(args(&["--ro-bind", p, p]));
        }
        words
    }
    /// What the stage can write beside its home: the scratch directory, the `write` paths and the
    /// `over` targets (R7).
    fn writable(&self) -> Vec<&str> {
        let targets = self.over.iter().map(|(_, t)| t.as_str());
        let write = self.write.iter().map(String::as_str);
        [self.scratch.as_str()]
            .into_iter()
            .chain(write)
            .chain(targets)
            .collect()
    }
    /// The paths R11 reads, in its order, each with whether a regular file may stand there.
    fn named_paths(&self) -> impl Iterator<Item = (&str, bool)> {
        let write = self.write.iter().map(|p| (p.as_str(), false));
        let over = self
            .over
            .iter()
            .flat_map(|(a, b)| [(a.as_str(), false), (b.as_str(), false)]);
        let read_only = self.read_only.iter().map(|p| (p.as_str(), true));
        write.chain(over).chain(read_only)
    }
}
/// The hidden list's paths under the home, in the list's order (R10).
const HIDDEN_HOME: [&str; 4] = [".config/gh", ".git-credentials", ".netrc", ".ssh"];
/// The hidden list's host paths, after those under the home (R10).
const HIDDEN_HOST: [&str; 8] = [
    "/etc/agent-platform/credentials",
    "/etc/agent-platform/ci",
    "/run/user",
    "/run/docker.sock",
    "/run/containerd/containerd.sock",
    "/run/tailscale",
    "/run/systemd/private",
    "/run/dbus/system_bus_socket",
];
/// The hidden list's last entry is every name under the work root that begins with this (`edu-*`).
const HIDDEN_PREFIX: &[u8] = b"edu-";
/// The fixed paths of the hidden list, `~` replaced by `home`, in the list's order (R10).
fn hidden_fixed(home: &str) -> Vec<String> {
    let at = |rel: &str| Path::new(home).join(rel).to_string_lossy().into_owned();
    (HIDDEN_HOME.iter().map(|r| at(r)))
        .chain(HIDDEN_HOST.iter().map(|p| (*p).to_owned()))
        .collect()
}
/// Whether `p` is equal to or inside a path of the hidden list, by its components as written:
/// a fixed path, or a name under the work root that the pattern `edu-*` matches (R1, R10).
fn in_hidden(p: &str, home: &str, work_root: &Path) -> bool {
    let p = Path::new(p);
    let pattern = p
        .strip_prefix(work_root)
        .ok()
        .and_then(|r| r.components().next());
    let pattern =
        matches!(pattern, Some(Component::Normal(n)) if n.as_bytes().starts_with(HIDDEN_PREFIX));
    pattern || hidden_fixed(home).iter().any(|h| p.starts_with(h))
}
/// The paths of the hidden list that exist now, each with whether it is a directory (a link
/// followed): the fixed paths in the list's order, then the names under the work root that begin
/// with `edu-`, by name (R10).
fn hidden_now(home: &str, work_root: &Path) -> Vec<(String, bool)> {
    let mut edu: Vec<PathBuf> = std::fs::read_dir(work_root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().as_bytes().starts_with(HIDDEN_PREFIX))
        .map(|e| e.path())
        .collect();
    edu.sort();
    let edu = edu.iter().map(|p| p.to_string_lossy().into_owned());
    hidden_fixed(home)
        .into_iter()
        .chain(edu)
        .filter_map(|p| {
            let dir = std::fs::metadata(&p).ok()?.is_dir();
            Some((p, dir))
        })
        .collect()
}
/// The `bwrap` words that hide the paths of the hidden list that exist now, all of them (nothing
/// shown): the view of an acceptance check (`specs/ap/run-limits.md` R12). None without a passwd
/// entry for this user (R10).
pub(crate) fn hidden_words(s: &impl System) -> Option<Vec<String>> {
    let home = s.home()?.to_string_lossy().into_owned();
    Some(hiding(&hidden_now(&home, &s.work_root())))
}
/// Whether one path is equal to or inside the other, by components as written (R10).
fn related(a: &str, b: &str) -> bool {
    Path::new(a).starts_with(b) || Path::new(b).starts_with(a)
}
/// The top directories beside `/usr` that R16 (b) places, each as a link or a directory.
const VIEW_TOP: [&str; 4] = ["/bin", "/lib", "/lib64", "/sbin"];
/// The names of `/etc` a stage sees, in this order (R16 (c)); no other name of `/etc`.
const VIEW_ETC: &[&str] = &[
    "passwd",
    "group",
    "nsswitch.conf",
    "hosts",
    "host.conf",
    "gai.conf",
    "resolv.conf",
    "services",
    "protocols",
    "networks",
    "ssl/certs",
    "ssl/openssl.cnf",
    "ca-certificates",
    "ca-certificates.conf",
    "localtime",
    "timezone",
    "ld.so.cache",
    "ld.so.conf",
    "ld.so.conf.d",
    "alternatives",
    "os-release",
    "lsb-release",
    "bash.bashrc",
    "profile",
    "inputrc",
    "shells",
    "gitconfig",
    "mime.types",
    "magic",
    "magic.mime",
    "python3",
    "terminfo",
    "locale.alias",
];
/// The paths under the home a stage sees: the toolchain, the instructions and Git's own
/// configuration (R16 (d)); nothing else of the home.
const VIEW_HOME: [&str; 5] = [
    ".rustup",
    ".cargo/bin",
    ".agents",
    ".gitconfig",
    ".config/git",
];
/// The host's build lock (`specs/ap/check.md` R5's default), which the stage's `ap check` takes
/// (R16 (f)).
const BUILD_LOCK: &str = "/var/lib/agent-platform/build.lock";
/// The policy file without `AP_POLICY_FILE` (`specs/ap/policy.md` R1; R16 (i)).
const POLICY_DEFAULT: &str = "/etc/agent-platform/policy.json";
/// The store's directory (`specs/ap/deliver.md` R37): no policy file inside it is shown (R16 (i)).
const STORE_DIR: &str = "/var/lib/ontology-platform";
/// The word that starts the CLI of a stage with `limits` (R16 (e)).
const CLI_WORD: &str = "claude";
/// What R16 places in place of `--ro-bind / /`: its words, the source of each read-only bind in
/// the words' order (the record's `view`, R7) and the target of each (R10).
#[derive(Default)]
struct View {
    words: Vec<String>,
    sources: Vec<String>,
    targets: Vec<String>,
}
impl View {
    fn bind(&mut self, source: &str, target: &str) {
        self.words.extend(args(&["--ro-bind", source, target]));
        self.sources.push(source.to_owned());
        self.targets.push(target.to_owned());
    }
    /// `--ro-bind <p> <p>` when `p` exists now, a link followed.
    fn bind_present(&mut self, p: &str) {
        if std::fs::metadata(p).is_ok() {
            self.bind(p, p);
        }
    }
}
fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
/// The last value of `name` in an environment list.
fn variable<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
    env.iter()
        .rev()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}
/// The stage's root, built from nothing (R16): a `/tmp` of its own, `/usr` and its links, the
/// fixed names of `/etc`, the toolchain of the home, the CLI's program, the build lock, the
/// working directory with its Git directories, the door, the policy file, and the state of `gh`
/// when `show` names it.
fn view(q: &Request, l: &Limits, work_root: &Path, hooks: Option<&Path>) -> View {
    let mut v = View::default();
    v.words.extend(args(&["--tmpfs", "/tmp"]));
    v.bind_present("/usr");
    for p in VIEW_TOP {
        match top_words(p) {
            words if words.first().is_some_and(|w| w == "--ro-bind") => v.bind(p, p),
            words => v.words.extend(words),
        }
    }
    for name in VIEW_ETC {
        v.bind_present(&format!("/etc/{name}"));
    }
    let at_home = |rel: &str| lossy(&Path::new(&l.home).join(rel));
    for rel in VIEW_HOME {
        v.bind_present(&at_home(rel));
    }
    if let Some((found, file)) = program(variable(&q.base_env, "PATH"), &q.cwd) {
        v.bind(&file, &found);
    }
    v.bind_present(BUILD_LOCK);
    for p in git_dirs(&q.cwd) {
        v.bind(&p, &p);
    }
    v.bind_present(&crate::door::path_of(&l.scratch));
    if let Some(p) = policy_shown(&q.base_env, &l.home, work_root) {
        v.bind(&p, &p);
    }
    if l.show {
        v.bind_present(&at_home(&SHOWN[2..]));
    }
    // (k): the host's hook is the script `ap run` made, which asks the door.
    if let Some(hooks) = hooks {
        let script = Path::new(&l.scratch).join("hooks/pre-commit");
        v.bind(&lossy(&script), &lossy(&hooks.join("pre-commit")));
    }
    v
}
/// The words of one top directory beside `/usr` (`specs/ap/run-limits.md` R16 (b)): `--symlink
/// <its link text> <p>` when it is a link, else `--ro-bind <p> <p>` when it exists, else none.
pub fn top_words(p: &str) -> Vec<String> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.file_type().is_symlink() => std::fs::read_link(p)
            .map(|to| args(&["--symlink", &lossy(&to), p]))
            .unwrap_or_default(),
        _ if std::fs::metadata(p).is_ok() => args(&["--ro-bind", p, p]),
        _ => vec![],
    }
}
/// Where `execvp` finds the CLI's word on `path` (the default search path without one), and the
/// file it resolves to through every link: the path found and the file (R16 (e)).
fn program(path: Option<&str>, cwd: &str) -> Option<(String, String)> {
    use std::os::unix::fs::PermissionsExt;
    for dir in path.unwrap_or("/bin:/usr/bin").split(':') {
        let dir = if dir.is_empty() { cwd } else { dir };
        let found = Path::new(cwd).join(dir).join(CLI_WORD);
        let Ok(m) = std::fs::metadata(&found) else {
            continue;
        };
        if !m.is_file() || m.permissions().mode() & 0o111 == 0 {
            continue;
        }
        let file = std::fs::canonicalize(&found).ok()?;
        return Some((lossy(&found), lossy(&file)));
    }
    None
}
/// The working directory and its Git directories, each resolved through its links and named once:
/// the common directory of a worktree whose `.git` is a file, then each directory its object
/// store borrows from (`objects/info/alternates`) (R16 (g)).
fn git_dirs(cwd: &str) -> Vec<String> {
    let mut dirs: Vec<PathBuf> = vec![];
    let mut add = |p: PathBuf| {
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    };
    let Ok(top) = std::fs::canonicalize(cwd) else {
        return vec![];
    };
    add(top);
    let dot = Path::new(cwd).join(".git");
    let common = match std::fs::metadata(&dot) {
        Ok(m) if m.is_dir() => Some(dot),
        Ok(m) if m.is_file() => gitdir(&dot, cwd).and_then(|a| {
            let common = match std::fs::read_to_string(a.join("commondir")) {
                Ok(c) => a.join(c.strip_suffix('\n').unwrap_or(&c)),
                Err(_) => a,
            };
            let common = std::fs::canonicalize(common).ok()?;
            common.is_dir().then(|| {
                add(common.clone());
                common
            })
        }),
        _ => None,
    };
    if let Some(common) = common {
        let objects = common.join("objects");
        let text = std::fs::read_to_string(objects.join("info/alternates")).unwrap_or_default();
        for line in text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            if let Ok(p) = std::fs::canonicalize(objects.join(line))
                && p.is_dir()
            {
                add(p);
            }
        }
    }
    dirs.iter().map(|p| lossy(p)).collect()
}
/// The directory a `.git` file names in its one line `gitdir: <A>`, read against `cwd`.
fn gitdir(dot: &Path, cwd: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(dot).ok()?;
    let line = text.strip_suffix('\n').unwrap_or(&text);
    let a = line
        .strip_prefix("gitdir: ")
        .filter(|a| !a.contains('\n'))?;
    Some(Path::new(cwd).join(a))
}
/// The policy file the stage's own `ap check` reads (`AP_POLICY_FILE` of the launch's environment
/// when set and not empty, else the default), when it is a regular file and neither the path as
/// written nor the file it resolves to is a hidden path or inside one, inside the store's
/// directory, or under `/etc` but the default (R16 (i)).
fn policy_shown(env: &[(String, String)], home: &str, work_root: &Path) -> Option<String> {
    let p = variable(env, "AP_POLICY_FILE")
        .filter(|p| !p.is_empty())
        .unwrap_or(POLICY_DEFAULT);
    let path = Path::new(p);
    if !path.is_absolute() || !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let resolved = std::fs::canonicalize(path).ok()?;
    let barred = |x: &Path| {
        in_hidden(&lossy(x), home, work_root)
            || x.starts_with(STORE_DIR)
            || (x.starts_with("/etc") && x != Path::new(POLICY_DEFAULT))
    };
    (!barred(path) && !barred(&resolved)).then(|| p.to_owned())
}
/// The `bwrap` arguments that hide these paths: a directory becomes an empty one the stage cannot
/// write, any other path `/dev/null` (R10).
fn hiding(hidden: &[(String, bool)]) -> Vec<String> {
    let mut words = vec![];
    for (p, dir) in hidden {
        match dir {
            true => words.extend(args(&["--tmpfs", p, "--remount-ro", p])),
            false => words.extend(args(&["--ro-bind", "/dev/null", p])),
        }
    }
    words
}

fn path(r: &Value, key: &str) -> Result<String, Failure> {
    let p = text(r, key)?;
    require!(Path::new(p).is_absolute() && !p.contains('\0') => "invalid_input", key);
    Ok(p.to_owned())
}
fn flag(r: &Value, key: &str) -> Result<bool, Failure> {
    match r.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(error("invalid_input", key)),
    }
}
fn word(r: &Value, key: &str, max: usize) -> Result<String, Failure> {
    let value = text(r, key)?;
    require!(word_ok(value, max) => "invalid_input", key);
    Ok(value.to_owned())
}
/// A model or an effort as a request may name it: at most `max` bytes of letters, digits and `._-[]`.
pub(crate) fn word_ok(value: &str, max: usize) -> bool {
    value.len() <= max
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-[]".contains(&c))
}
fn paths(r: &Value, key: &str, max: usize) -> Result<Vec<String>, Failure> {
    let Some(v) = r.get(key) else {
        return Ok(vec![]);
    };
    let list = v.as_array().ok_or_else(|| error("invalid_input", key))?;
    require!(list.len() <= max => "invalid_input", key);
    list.iter()
        .map(|p| {
            p.as_str()
                .filter(|p| Path::new(p).is_absolute() && !p.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| error("invalid_input", key))
        })
        .collect()
}
/// The account rule: an existing directory, whichever it is: `/root/.claude` is an account like any other.
fn admit(s: &impl System, account: &str, field: &str) -> Result<(), Failure> {
    s.directory(account)
        .map(|_| ())
        .map_err(|_| error("account_missing", field))
}
fn environment(r: &Value, runtime: Runtime) -> Result<Vec<(String, String)>, Failure> {
    let Some(v) = r.get("env") else {
        return Ok(vec![]);
    };
    let map = v.as_object().ok_or_else(|| error("invalid_input", "env"))?;
    require!(map.len() <= 32 => "invalid_input", "env");
    map.iter()
        .map(|(k, v)| {
            let name_ok = !k.is_empty()
                && k.len() <= 64
                && k.bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                && !k.as_bytes()[0].is_ascii_digit();
            require!(name_ok => "invalid_input", "env");
            // The account named in the record must be the account that answers.
            require!(!k.starts_with("HERDR_") && !k.starts_with("CLAUDE_")
                && !k.starts_with("ANTHROPIC_") && k != "HOME" => "env_forbidden", "env");
            require!(!runtime.reserved(k) => "env_forbidden", "env");
            // `BASH_ENV` and `ENV` name a file a shell sources before the stage's own command.
            require!(!["ONT_ROLE", "ONT_AGENT", "GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL",
                "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL", "BASH_ENV", "ENV"]
                .contains(&k.as_str()) => "env_forbidden", "env");
            let v = v
                .as_str()
                .filter(|v| !v.contains('\0') && v.len() <= 4096)
                .ok_or_else(|| error("invalid_input", "env"))?;
            Ok((k.clone(), v.to_owned()))
        })
        .collect()
}
/// `unit` or `intent` with its key, at most one of the two (`specs/ap/run-object.md` R1).
fn target(r: &Value) -> Result<Option<(&'static str, String)>, Failure> {
    let key = |name: &'static str| match r.get(name) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .filter(|k| {
                (1..=64).contains(&k.len())
                    && k.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            })
            .map(|k| Some((name, k.to_owned())))
            .ok_or_else(|| error("invalid_input", name)),
    };
    match (key("unit")?, key("intent")?) {
        (Some(_), Some(_)) => Err(error("invalid_input", "intent")),
        (a, b) => Ok(a.or(b)),
    }
}
/// A path of the lists of `limits`: absolute, not `/` and without a `..` component (R1).
fn listed(p: &Value) -> Option<String> {
    let p = p.as_str().filter(|p| !p.contains('\0'))?;
    let at = Path::new(p);
    let root = at.components().all(|c| c == Component::RootDir);
    let up = at.components().any(|c| c == Component::ParentDir);
    (at.is_absolute() && !root && !up).then(|| p.to_owned())
}
/// A list of `limits` of 1 to `max` paths; empty when the member is absent (R1).
fn listed_paths(o: &serde_json::Map<String, Value>, key: &str, max: usize) -> Option<Vec<String>> {
    let Some(v) = o.get(key) else {
        return Some(vec![]);
    };
    let list = v.as_array().filter(|a| (1..=max).contains(&a.len()))?;
    list.iter().map(listed).collect()
}
/// The `limits` of a request: a reviewer, an implementer, a fixer or a writer (with `scratch` alone,
/// `specs/ap/run.md` R53) on the Claude CLI only; a
/// `scratch`, an absolute path without `..`, apart from the accounts, the cwd, the out, the prompt
/// and the paths it writes by its components as written; the optional `write`, `over`, `read_only`
/// and `show`; never with `expect.file` (`specs/ap/run-limits.md` R1).
fn scratch(
    s: &impl System,
    v: &Value,
    (role, runtime, expect_file): (&str, Runtime, bool),
    accounts: &[String],
    [cwd, out, prompt]: [&str; 3],
) -> Result<Limits, Failure> {
    let bad = || error("invalid_input", "limits");
    let o = v.as_object().ok_or_else(bad)?;
    let known = ["scratch", "write", "over", "read_only", "show"];
    require!(o.keys().all(|k| known.contains(&k.as_str())) => "invalid_input", "limits");
    let p = path(v, "scratch").map_err(|_| bad())?;
    let write = listed_paths(o, "write", 8).ok_or_else(bad)?;
    let read_only = listed_paths(o, "read_only", 8).ok_or_else(bad)?;
    let over = match o.get("over") {
        None => vec![],
        Some(v) => {
            let list = v.as_array().filter(|a| (1..=4).contains(&a.len()));
            let pair = |p: &Value| {
                let p = p.as_array().filter(|p| p.len() == 2)?;
                Some((listed(&p[0])?, listed(&p[1])?))
            };
            let pairs = list.and_then(|l| l.iter().map(pair).collect::<Option<Vec<_>>>());
            pairs.ok_or_else(bad)?
        }
    };
    let show = match o.get("show") {
        None => false,
        Some(v) => {
            require!(*v == json!([SHOWN]) => "invalid_input", "limits");
            true
        }
    };
    let at = Path::new(&p);
    let up = at.components().any(|c| c == Component::ParentDir);
    // What the stage writes beside its home and scratch directory: the `write` paths, the `over`
    // sources and targets.
    let writes: Vec<&str> = (write.iter().map(String::as_str))
        .chain(over.iter().flat_map(|(a, b)| [a.as_str(), b.as_str()]))
        .collect();
    let inside = (accounts.iter().map(String::as_str))
        .chain([cwd])
        .chain(writes.iter().copied())
        .any(|a| at.starts_with(a));
    let holds = [cwd, out, prompt]
        .iter()
        .any(|h| Path::new(h).starts_with(at));
    let held = [out, prompt]
        .iter()
        .any(|h| writes.iter().any(|w| Path::new(h).starts_with(w)));
    // The view hides every account of the request but the home of the launch (R5, R14): no path
    // the stage works in or names may lie in one.
    let in_account = ([cwd].into_iter())
        .chain(writes.iter().copied())
        .chain(read_only.iter().map(String::as_str))
        .any(|p| accounts.iter().any(|a| Path::new(p).starts_with(a)));
    let roles = ["reviewer", "implementer", "fixer", "writer"];
    // A writer's `limits` names no path but its scratch directory (`specs/ap/run.md` R53).
    let only_scratch = role != "writer" || o.len() == 1;
    let stage = roles.contains(&role) && runtime == Runtime::Claude && !expect_file && only_scratch;
    require!(stage && !up && !inside && !holds && !held && !in_account => "invalid_input", "limits");
    // The hidden list's `~`: without a passwd entry for this user there is no list to hide (R10).
    let home = s.home().map(|h| h.to_string_lossy().into_owned());
    let home = home.ok_or_else(|| error("limits_unavailable", "limits"))?;
    let root = s.work_root();
    let hidden = (accounts.iter().map(String::as_str))
        .chain([cwd, p.as_str()])
        .chain(writes.iter().copied())
        .any(|a| in_hidden(a, &home, &root));
    require!(!hidden => "invalid_input", "limits");
    let mut given = serde_json::Map::new();
    for k in ["over", "read_only", "show"] {
        if let Some(v) = o.get(k) {
            given.insert(k.to_owned(), v.clone());
        }
    }
    Ok(Limits {
        scratch: p,
        home,
        write,
        over,
        read_only,
        show,
        given,
    })
}
/// The script of a stage's `core.hooksPath` in its view (R4): it asks the stage's door.
const HOOK_SCRIPT: &str = "#!/bin/sh\nexec ap hook pre-commit\n";
/// The host's Git hooks of a call (R4): the port's directory, when the stage's role, as its door
/// knows it, is an implementer or a fixer and `<hooks>/pre-commit` is a regular file (a link
/// followed). Read once in the call.
fn host_hooks(s: &impl System, q: &Request) -> Option<PathBuf> {
    q.limits.as_ref()?;
    if !matches!(q.actor_role.as_str(), "implementer" | "fixer") {
        return None;
    }
    let hooks = s.git_hooks()?;
    std::fs::metadata(hooks.join("pre-commit"))
        .is_ok_and(|m| m.is_file())
        .then_some(hooks)
}
/// `hooks/` in the scratch directory (mode 0700) and in it `pre-commit` (mode 0755) of
/// `HOOK_SCRIPT` (R4).
fn make_hook(scratch: &str) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    let dir = Path::new(scratch).join("hooks");
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let file = dir.join("pre-commit");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(&file)?
        .write_all(HOOK_SCRIPT.as_bytes())?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
}
/// One `mkdir` of the scratch directory that fails when the path exists (mode 0700), then its
/// `tmp` and `target`, and with `hook` its `hooks/pre-commit`: whether this call made it, and
/// whether everything was made (R4).
fn make_scratch(scratch: &str, hook: bool) -> (bool, bool) {
    use std::os::unix::fs::DirBuilderExt;
    let mut dirs = std::fs::DirBuilder::new();
    dirs.mode(0o700);
    if dirs.create(scratch).is_err() {
        return (false, false);
    }
    let made = ["tmp", "target"].map(|d| dirs.create(Path::new(scratch).join(d)).is_ok());
    let hooked = !hook || make_hook(scratch).is_ok();
    (true, made == [true, true] && hooked)
}
/// Removes the scratch directory this call made: only a directory that is not a link, and
/// nothing through a link inside it (`remove_dir_all` does not follow one) (R8).
fn remove_scratch(scratch: &str) {
    if std::fs::symlink_metadata(scratch).is_ok_and(|m| m.is_dir()) {
        let _ = std::fs::remove_dir_all(scratch);
    }
}
/// What every stage's environment gets: its role and agent as the actor's claim (R3, R4) and, with
/// `git_author`, the author and the committer of its commits, the same two values (R5).
fn identity(
    runtime: Runtime,
    (role, model): (&str, &str),
    git_author: bool,
) -> Vec<(String, String)> {
    let clean = |text: String, kept: &str| -> String {
        let keep = |c: &char| c.is_ascii_alphanumeric() || kept.contains(*c);
        text.chars()
            .map(|c| if keep(&c) { c } else { '-' })
            .collect()
    };
    let agent: String = clean(format!("{}:{model}", runtime.word()), "._:-")
        .chars()
        .take(64)
        .collect();
    let mut env = vec![
        ("ONT_ROLE".into(), role.into()),
        ("ONT_AGENT".into(), agent),
    ];
    if git_author {
        let name = format!("{role} {model}");
        let email = format!("{}@agents.invalid", clean(format!("{role}.{model}"), ".-"));
        // The committer is the author: a commit carries no host name (`user@host`) of Git's default.
        for who in ["AUTHOR", "COMMITTER"] {
            env.push((format!("GIT_{who}_NAME"), name.clone()));
            env.push((format!("GIT_{who}_EMAIL"), email.clone()));
        }
    }
    env
}
/// The host's policy as read (`specs/ap/policy.md`).
pub(crate) struct Policy {
    path: String,
    sha256: String,
    allowed: Vec<String>,
    never: Vec<String>,
    accounts: Vec<(String, Vec<String>)>,
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// A model name, or with `star` a pattern of at most two `*`: what `word` takes for a model.
fn model_name(n: &str, star: bool) -> bool {
    let stars = n.matches('*').count();
    !n.is_empty()
        && n.len() <= 40
        && stars <= if star { 2 } else { 0 }
        && n.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-[]*".contains(&c))
}
fn names(
    v: Option<&Value>,
    member: &str,
    count: std::ops::RangeInclusive<usize>,
    ok: impl Fn(&str) -> bool,
) -> Result<Vec<String>, Failure> {
    let list = v.and_then(Value::as_array);
    let list = list.filter(|l| count.contains(&l.len()));
    let list = list.ok_or_else(|| invalid(member, "is not an array of the allowed length"))?;
    list.iter()
        .map(|e| e.as_str().filter(|e| ok(e)).map(str::to_owned))
        .collect::<Option<_>>()
        .ok_or_else(|| invalid(member, "holds an entry of the wrong form"))
}
/// The policy file the host names, read once at admission. None: no path or no such file.
pub(crate) fn policy(s: &impl System) -> Result<Option<Policy>, Failure> {
    let Some(file) = s.policy_file() else {
        return Ok(None);
    };
    // A path that is not text cannot be read through `read` or named in the record: no file there
    // is no policy, a file there is a policy that cannot be used.
    let Some(path) = file.to_str().map(str::to_owned) else {
        let missing =
            std::fs::metadata(&file).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        return if missing {
            Ok(None)
        } else {
            Err(invalid("file", "path is not valid UTF-8"))
        };
    };
    let bytes = match s.read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("file", "is unreadable")),
    };
    if bytes.len() as u64 > LIMIT {
        return Err(invalid("file", "is too large"));
    }
    let sha256 = digest(&bytes);
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("file", "is not JSON"))?;
    let object = value.as_object();
    let object = object.ok_or_else(|| invalid("file", "is not one JSON object"))?;
    let models = object.get("models").and_then(Value::as_object);
    let models =
        models.filter(|m| m.len() == 2 && m.contains_key("allowed") && m.contains_key("never"));
    let models =
        models.ok_or_else(|| invalid("models", "is not an object of allowed and never"))?;
    let allowed = names(models.get("allowed"), "models.allowed", 1..=64, |n| {
        model_name(n, false)
    })?;
    let never = names(models.get("never"), "models.never", 0..=16, |n| {
        model_name(n, true)
    })?;
    let map = object.get("accounts").and_then(Value::as_object);
    let map = map.ok_or_else(|| invalid("accounts", "is not an object"))?;
    let mut accounts = vec![];
    for (key, list) in map {
        let member = format!("accounts.{key}");
        let (runtime, rest) = key
            .split_once('/')
            .map_or((key.as_str(), None), |(r, x)| (r, Some(x)));
        let shaped = Runtime::of(Some(&json!(runtime))).is_some()
            && rest.is_none_or(|x| ROLES.iter().any(|(r, _)| *r == x) || model_name(x, false));
        if !shaped {
            return Err(invalid(
                &member,
                "is not a runtime, runtime/role or runtime/model",
            ));
        }
        let list = names(Some(list), &member, 1..=5, |p| {
            Path::new(p).is_absolute() && !p.contains('\0')
        })?;
        accounts.push((key.clone(), list));
    }
    Ok(Some(Policy {
        path,
        sha256,
        allowed,
        never,
        accounts,
    }))
}
impl Policy {
    /// The model rule and the account rule for every account the stage may start on.
    fn admit(
        &self,
        runtime: Runtime,
        role: &str,
        model: &str,
        accounts: &[String],
    ) -> Result<(), Failure> {
        let named = self.allowed.iter().any(|a| a == model);
        let barred = self.never.iter().any(|p| glob(p, model));
        require!(named && !barred => "model_not_allowed", "model");
        let word = runtime.word();
        let keys = [
            format!("{word}/{role}"),
            format!("{word}/{model}"),
            word.to_owned(),
        ];
        let list = keys
            .iter()
            .find_map(|k| self.accounts.iter().find(|(name, _)| name == k))
            .map(|(_, list)| list);
        let list = list.ok_or_else(|| error("account_not_allowed", "account"))?;
        let bare = |p: &str| {
            if p.len() > 1 {
                p.trim_end_matches('/').to_owned()
            } else {
                p.to_owned()
            }
        };
        for (i, a) in accounts.iter().enumerate() {
            let field = if i == 0 { "account" } else { "next_accounts" };
            require!(list.iter().any(|l| bare(l) == bare(a)) => "account_not_allowed", field);
        }
        Ok(())
    }
}
/// The record's `policy`: the file as read, or null.
fn policy_member(q: &Request) -> Value {
    let read = q.policy.as_ref();
    json!(read.map(|(path, sha256)| json!({"path": path, "sha256": sha256})))
}
fn record_path(out: &str) -> String {
    format!("{out}.run.json")
}
/// What `systemctl is-active` answered for `unit`: false unless it answered a state in which
/// nothing of the unit lives (a call that fails, times out or prints nothing is no answer).
fn ended(s: &mut impl System, unit: &str) -> bool {
    let reply = s.call(
        "systemctl",
        &args(&["is-active", unit]),
        &Value::Null,
        10_000,
    );
    let state = reply.map(|r| String::from_utf8_lossy(&r.stdout).trim().to_owned());
    state.is_ok_and(|st| !st.is_empty() && !ALIVE.contains(&st.as_str()))
}
/// The fixed short list of this process's variables, then the request's `env`.
fn base_env(s: &impl System, extra: &[(String, String)]) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = s
        .environment()
        .into_iter()
        .filter(|(k, _)| KEEP.contains(&k.as_str()))
        .filter(|(k, _)| !extra.iter().any(|(name, _)| name == k))
        .collect();
    env.extend(extra.iter().cloned());
    env
}
/// `git -C <cwd> …` in the CLI's clean environment (no `GIT_DIR` of the caller), messages in C.
fn git(
    s: &mut impl System,
    env: &[(String, String)],
    cwd: &str,
    command: &[&str],
) -> Result<crate::host::Reply, Failure> {
    let mut a = vec!["-i".to_owned()];
    a.extend(
        env.iter()
            .filter(|(k, _)| k != "LC_ALL")
            .map(|(k, v)| format!("{k}={v}")),
    );
    a.extend(args(&["LC_ALL=C", "git", "-C", cwd]));
    // No hook or fsmonitor program of the worktree runs (`specs/ap/unit-run.md` R39).
    a.extend(args(&crate::review::GIT_SAFE));
    a.extend(args(command));
    s.call("env", &a, &Value::Null, 10_000)
}
/// The expected file as it is now: its digest and modification time, or nulls when absent.
fn snapshot(path: &str) -> Value {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => json!({
            "path": path,
            "sha256": std::fs::read(path).ok().map(|b| digest(&b)),
            "modified": m.mtime().saturating_mul(1_000_000_000).saturating_add(m.mtime_nsec()),
        }),
        _ => json!({"path": path, "sha256": null, "modified": null}),
    }
}
/// Met only by a nonempty file this session wrote: one that lay there unchanged since before the
/// session's first launch does not meet it.
fn written(path: &str, before: &Value) -> bool {
    let now = snapshot(path);
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
        && (now["sha256"] != before["sha256"] || now["modified"] != before["modified"])
}
fn request(s: &mut impl System, r: &Value) -> Result<Request, Failure> {
    request_at(s, r, true)
}
/// Every check of a request but those that need its `cwd` to exist: for a caller that admits a
/// stage before the stage's worktree exists (`ap unit`).
pub(crate) fn admit_stage(s: &mut impl System, r: &Value) -> Result<(), Failure> {
    request_at(s, r, false).map(|_| ())
}
fn request_at(s: &mut impl System, r: &Value, at_cwd: bool) -> Result<Request, Failure> {
    let object = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = object.keys().find(|k| !MEMBERS.contains(&k.as_str())) {
        return Err(error("invalid_input", k));
    }
    let runtime = Runtime::of(r.get("runtime")).ok_or_else(|| error("invalid_input", "runtime"))?;
    let role = text(r, "role")?;
    // Not built for every runtime: some roles (Codex has no reviewer, `agy` only one) and these members.
    require!(runtime.role_ok(role) => "invalid_input", "role");
    if let Some(k) = (runtime.unbuilt().iter()).find(|k| r.get(**k).is_some()) {
        return Err(error("invalid_input", k));
    }
    let default = ROLES
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, t)| *t)
        .ok_or_else(|| error("invalid_input", "role"))?;
    let integration = flag(r, "integration")?;
    require!(!integration || role == "reviewer" => "invalid_input", "integration");
    let continue_dirty = flag(r, "continue_dirty")?;
    let resume = match r.get("resume") {
        None => None,
        Some(v) => {
            let keys = v.as_object().map(|o| o.keys().all(|k| k == "record"));
            require!(keys == Some(true) => "invalid_input", "resume");
            Some(path(v, "record").map_err(|_| error("invalid_input", "resume"))?)
        }
    };
    let (model, effort) = (word(r, "model", 40)?, word(r, "effort", 16)?);
    for (key, value) in [("model", &model), ("effort", &effort)] {
        require!(runtime.argument_ok(value) => "invalid_input", key);
    }
    require!(runtime.effort_ok(&model, &effort) => "invalid_input", "effort");
    let mode = text(r, "permission_mode")?;
    require!(MODES.contains(&mode) => "invalid_input", "permission_mode");
    // No approval and no sandbox of the CLI's own: only when the request says so.
    require!(!runtime.bare() || mode == "bypassPermissions" => "invalid_input", "permission_mode");
    let timeout = match r.get("timeout_seconds") {
        None => default,
        Some(v) => v
            .as_u64()
            .filter(|t| (1..=MAX_TIMEOUT).contains(t))
            .ok_or_else(|| error("invalid_input", "timeout_seconds"))?,
    };
    // An integer of the CLI's range; a fraction or an exponent is no integer here (as for timeout).
    let compact_window = match r.get("compact_window") {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .filter(|n| (100_000..=1_000_000).contains(n))
                .ok_or_else(|| error("invalid_input", "compact_window"))?,
        ),
    };
    let (account, cwd, prompt, out) = (
        path(r, "account")?,
        path(r, "cwd")?,
        path(r, "prompt")?,
        path(r, "out")?,
    );
    let mut accounts = vec![account];
    accounts.extend(paths(r, "next_accounts", 4)?);
    require!(!runtime.bare() || accounts.len() == 1 => "invalid_input", "next_accounts");
    let mcp = paths(r, "mcp_config", 8)?;
    require!(!runtime.bare() || mcp.is_empty() => "invalid_input", "mcp_config");
    let env = environment(r, runtime)?;
    let target = target(r)?;
    let store = match r.get("store") {
        None => None,
        Some(_) => {
            require!(target.is_some() => "invalid_input", "store");
            Some(path(r, "store")?)
        }
    };
    let git_author = flag(r, "git_author")?;
    let actor_role = match r.get("ont_role") {
        None => role,
        Some(v) => {
            require!(v == "verifier" && role == "reviewer" => "invalid_input", "ont_role");
            "verifier"
        }
    };
    let (mut expect_file, mut expect_line, mut expect_second) = (None, None, None);
    if let Some(e) = r.get("expect") {
        let members = ["file", "first_line", "second_line"];
        let o = e
            .as_object()
            .filter(|o| !o.is_empty() && o.keys().all(|k| members.contains(&k.as_str())))
            .ok_or_else(|| error("invalid_input", "expect"))?;
        if o.contains_key("file") {
            expect_file = Some(path(e, "file").map_err(|_| error("invalid_input", "expect"))?);
        }
        if o.contains_key("first_line") {
            let line = text(e, "first_line").map_err(|_| error("invalid_input", "expect"))?;
            require!(line.len() <= 200 && !line.contains('\n') => "invalid_input", "expect");
            expect_line = Some(line.to_owned());
        }
        if o.contains_key("second_line") {
            let field = "expect.second_line";
            require!(expect_line.is_some() => "invalid_input", field);
            let line = text(e, "second_line").map_err(|_| error("invalid_input", field))?;
            require!(line.len() <= 200 && !line.contains('\n') => "invalid_input", field);
            expect_second = Some(line.to_owned());
        }
    }
    let limits = match r.get("limits") {
        None => None,
        Some(v) => Some(scratch(
            s,
            v,
            (role, runtime, expect_file.is_some()),
            &accounts,
            [&cwd, &out, &prompt],
        )?),
    };
    // No writer stage without `limits` (R53).
    require!(role != "writer" || limits.is_some() => "invalid_input", "limits");
    // A typed answer is the Claude CLI's: its answer is the object, not a text with a first line.
    let schema = match r.get("answer_schema") {
        None => None,
        Some(_) => {
            require!(runtime.typed_answer() && expect_line.is_none() => "invalid_input", "answer_schema");
            let file = path(r, "answer_schema")?;
            let regular =
                std::fs::metadata(&file).is_ok_and(|m| m.is_file() && m.len() <= SCHEMA_MAX);
            let bytes = s
                .read(&file)
                .ok()
                .filter(|b| regular && b.len() as u64 <= SCHEMA_MAX);
            let text = bytes.and_then(|b| String::from_utf8(b).ok());
            let object = text.as_deref().map(serde_json::from_str::<Value>);
            require!(object.is_some_and(|v| v.is_ok_and(|v| v.is_object())) => "invalid_input", "answer_schema");
            let text = text.unwrap_or_default();
            let sha = digest(text.as_bytes());
            Some((file, text, sha))
        }
    };
    for (i, a) in accounts.iter().enumerate() {
        admit(s, a, if i == 0 { "account" } else { "next_accounts" })?;
    }
    require!(runtime.account_named(&accounts[0]) => "invalid_input", "account");
    if at_cwd {
        s.directory(&cwd).map_err(|_| error("cwd_missing", "cwd"))?;
    }
    let bytes = s
        .read(&prompt)
        .map_err(|_| error("prompt_unreadable", "prompt"))?;
    require!(!bytes.is_empty() && bytes.len() as u64 <= LIMIT => "prompt_unreadable", "prompt");
    // Whole and before any effect: an argument cannot hold a NUL byte or more than its limit.
    let argument = String::from_utf8(bytes.clone()).ok();
    let argument = argument.filter(|a| a.len() <= ARGUMENT_MAX && !a.contains('\0'));
    require!(!runtime.prompt_argument() || argument.is_some() => "invalid_input", "prompt");
    let out_path = Path::new(&out);
    let name = out_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| error("invalid_input", "out"))?;
    // No run's answer path can be another run's derived file.
    let derived = [".run.json", ".stream.jsonl", ".err", ".ap-tmp"];
    require!(!derived.iter().any(|d| name.ends_with(d)) => "invalid_input", "out");
    let parent = out_path
        .parent()
        .and_then(|p| p.to_str())
        .ok_or_else(|| error("invalid_input", "out"))?;
    s.directory(parent)
        .map_err(|_| error("out_unwritable", "out"))?;
    require!(!out_path.is_dir() && out != prompt => "out_unwritable", "out");
    // A prompt that names the out file could make the session write the answer it is judged by.
    require!(!bytes.windows(name.len()).any(|w| w == name.as_bytes()) => "out_named_in_prompt", "prompt");
    // Every run has its own files: nothing of an earlier run is run over.
    let names = [
        &out,
        &record_path(&out),
        &format!("{out}.stream.jsonl"),
        &format!("{out}.err"),
    ];
    require!(names.iter().all(|n| std::fs::symlink_metadata(n).is_err()) => "out_exists", "out");
    if let Some(Limits { scratch, .. }) = &limits {
        // The host's state for `limits`, before the policy step: nothing at the path, a parent
        // that is a directory with no link in its path (R2), the named paths (R11), and a `bwrap`
        // that answers (R3).
        require!(std::fs::symlink_metadata(scratch).is_err() => "scratch_exists", "limits");
        let parent = Path::new(scratch).parent().is_some_and(|p| {
            // Resolved through every link, the parent is the path as written, component by
            // component: a repeated `/` or a `.` is no difference, a link anywhere is.
            let resolved = std::fs::canonicalize(p);
            resolved.is_ok_and(|r| r.is_dir() && r.components().eq(p.components()))
        });
        require!(parent => "scratch_parent_missing", "limits");
    }
    if let Some(l) = &limits {
        // The paths the request names exist: a directory (a `read_only` path may be a regular
        // file), with no link in its path (R11).
        let real = |p: &str, file: bool| {
            let at = Path::new(p);
            let kind = std::fs::metadata(at).is_ok_and(|m| m.is_dir() || (file && m.is_file()));
            let resolved = std::fs::canonicalize(at);
            kind && resolved.is_ok_and(|r| r.components().eq(at.components()))
        };
        if let Some((p, _)) = l.named_paths().find(|(p, file)| !real(p, *file)) {
            return Err(fail("invalid_input", "limits", json!({ "path": p })));
        }
        let probe = s.call(BWRAP, &args(&["--version"]), &Value::Null, 10_000);
        require!(probe.is_ok_and(|r| r.success) => "limits_unavailable", "limits");
    }
    let mut home = None;
    let earlier = match &resume {
        None => None,
        Some(file) => {
            let rec: Value = std::fs::read(file)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .ok_or_else(|| error("resume_record_missing", "resume"))?;
            let missing = |_| error("resume_record_missing", "resume");
            require!(Runtime::of(rec.get("runtime")) == Some(runtime) => "resume_runtime_differs", "runtime");
            let (session, account) = (
                text(&rec, "session").map_err(missing)?,
                text(&rec, "account").map_err(missing)?,
            );
            // The Codex CLI takes the session as an argument of its own: never one it reads as a flag.
            require!(!session.starts_with('-') => "resume_record_missing", "resume");
            require!(accounts.iter().any(|a| a == account) => "resume_account_differs", "account");
            require!(rec["cwd"] == cwd.as_str() => "resume_cwd_differs", "cwd");
            let fixes = role == "fixer" && (rec["role"] == "implementer" || rec["role"] == "fixer");
            require!(rec["role"] == role || fixes => "resume_role_differs", "role");
            // Whatever the record says (`failed` with `unmet: stop` too): nothing of its unit may live.
            if let Some(unit) = rec["unit"].as_str() {
                require!(ended(s, unit) => "run_still_active", "resume");
            }
            // Under `limits` a session continues in its home, which a session that ran without
            // them has not; a session that ran in a home continues only there (`specs/ap/run-limits.md`
            // R14).
            if runtime == Runtime::Claude {
                let fits = match &limits {
                    Some(_) => session_home(account, session).is_some(),
                    None => rec.get("home").is_none(),
                };
                require!(fits => "invalid_input", "resume");
            }
            if runtime == Runtime::Agy {
                home = Some(
                    resumed_home(&rec, account).ok_or_else(|| error("invalid_input", "resume"))?,
                );
            }
            Some((
                rec.clone(),
                (session.to_owned(), account.to_owned(), file.clone()),
            ))
        }
    };
    if at_cwd && earlier.is_none() && role == "implementer" {
        // Fails closed: only "not a repository" lets an implementer start without the check. Git
        // gets the fixed short list only: the request's `env` (a `GIT_DIR`) cannot skip it.
        let clean = base_env(s, &[]);
        let inside = git(s, &clean, &cwd, &["rev-parse", "--is-inside-work-tree"])
            .map_err(|_| error("worktree_unreadable", "cwd"))?;
        if inside.success && inside.stdout == b"true\n" {
            let status = git(
                s,
                &clean,
                &cwd,
                &["status", "--porcelain", "--ignore-submodules=all"],
            )
            .map_err(|_| error("worktree_unreadable", "cwd"))?;
            require!(status.success => "worktree_unreadable", "cwd");
            require!(status.stdout.is_empty() || continue_dirty => "worktree_dirty", "cwd");
        } else {
            let not_a_repository = !inside.success
                && String::from_utf8_lossy(&inside.bytes).contains("not a git repository");
            require!(not_a_repository => "worktree_unreadable", "cwd");
        }
    }
    // The reference for `expect.file` is the file before THIS run's launch; only a run that
    // continues the same unfinished stage (same role, earlier state not `ok`) takes the earlier one.
    let expect_before = match (&expect_file, &earlier) {
        (None, _) => Value::Null,
        (Some(f), Some((rec, _)))
            if rec["role"] == role
                && rec["state"] != "ok"
                && rec["expect_file_before"]["path"] == f.as_str() =>
        {
            rec["expect_file_before"].clone()
        }
        (Some(f), _) => snapshot(f),
    };
    // A move names the resumed record's two paths: only strings that cannot steer the prompt, and
    // that do not name the out file (as `out_named_in_prompt` for the prompt itself).
    let moved = earlier.as_ref().and_then(|(rec, _)| {
        let fit = |k: &'static str| {
            let p = rec[k]["path"].as_str()?;
            (!p.contains(name) && !p.chars().any(char::is_control)).then_some(p)
        };
        let (p, a) = (fit("prompt")?, fit("answer")?);
        let work = if first_result(role) { "" } else { MOVED_WORK };
        // One pass: a path that holds the literal `{A}` stays as the record has it.
        let (head, rest) = MOVED.split_once("{P}")?;
        let (middle, tail) = rest.split_once("{A}")?;
        Some(format!("{head}{p}{middle}{a}{tail}{work}\n\n"))
    });
    let mut base_env = base_env(s, &env);
    base_env.extend(identity(runtime, (actor_role, &model), git_author));
    require!(s.units() => "systemd_absent", "host");
    let bind = match r.get("bind") {
        None => None,
        Some(v) => {
            let unit = v.as_str().filter(|u| {
                u.len() <= 200
                    && u.contains('.')
                    && !u.starts_with('-')
                    && u.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b":_.@-".contains(&c))
            });
            let unit = unit.ok_or_else(|| error("invalid_input", "bind"))?;
            // A unit that is not active now would stop the stage at its launch.
            let state = s.call(
                "systemctl",
                &args(&["is-active", unit]),
                &Value::Null,
                10_000,
            );
            require!(state.is_ok_and(|r| r.stdout == b"active\n") => "bind_inactive", "bind");
            Some(unit.to_owned())
        }
    };
    let (skills, memory) = (flag(r, "skills")?, flag(r, "memory")?);
    // The host's policy is the last check of admission: after every refusal `ap run` had before it,
    // the checks of the host's state included, and before any effect. A resumed call's record
    // account is one of `accounts` (`resume_account_differs`).
    let policy = policy(s)?;
    if let Some(p) = &policy {
        p.admit(runtime, role, &model, &accounts)?;
    }
    Ok(Request {
        role: role.to_owned(),
        accounts,
        model,
        effort,
        mode: mode.to_owned(),
        cwd,
        prompt,
        prompt_sha256: digest(&bytes),
        out,
        timeout,
        expect_file,
        expect_line,
        expect_second,
        schema,
        mcp,
        spent: match (runtime, &earlier) {
            (Runtime::Codex | Runtime::Agy, Some((_, (_, _, file)))) => spent(file),
            (Runtime::Codex | Runtime::Agy, None) | (Runtime::Claude | Runtime::Muse, _) => {
                Some([0; 4])
            }
        },
        resume: earlier.map(|(_, r)| r),
        moved,
        expect_before,
        base_env,
        bind,
        skills,
        memory,
        runtime,
        compact_window,
        argument: argument
            .filter(|_| runtime.prompt_argument())
            .unwrap_or_default(),
        home,
        policy: policy.map(|p| (p.path, p.sha256)),
        target,
        store,
        actor_role: actor_role.to_owned(),
        limits,
    })
}
/// The `home` of an `agy` record that a resume runs in: a directory under `<account>/homes` (links
/// and `..` resolved), the `homes` directory itself excluded. None otherwise.
fn resumed_home(record: &Value, account: &str) -> Option<String> {
    let named = record["home"].as_str()?;
    let home = std::fs::canonicalize(named).ok()?;
    let homes = std::fs::canonicalize(Path::new(account).join("homes")).ok()?;
    (home.is_dir() && home.starts_with(&homes) && home != homes).then(|| named.to_owned())
}
/// The home of a session of a stage with `limits`, `<account>/homes/<session>`, when it is an existing
/// directory below `<account>/homes` (links and `..` resolved), `homes` itself excluded
/// (`specs/ap/run-limits.md` R14). None otherwise.
fn session_home(account: &str, session: &str) -> Option<PathBuf> {
    let homes = Path::new(account).join("homes");
    let named = homes.join(session);
    let home = std::fs::canonicalize(&named).ok()?;
    let homes = std::fs::canonicalize(homes).ok()?;
    (home.is_dir() && home.starts_with(&homes) && home != homes).then_some(named)
}
/// A new session's home under `limits`: `<account>/homes` when it is absent, then the home by one
/// `mkdir` that fails when the name exists, each with mode 0700 (`specs/ap/run-limits.md` R14).
fn make_stage_home(home: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut dirs = std::fs::DirBuilder::new();
    dirs.mode(0o700);
    if let Some(homes) = home.parent() {
        match dirs.create(homes) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e),
            _ => (),
        }
    }
    dirs.create(home)
}
/// The two names of a home that the binds of R5 cover: each must be absent or a regular file that is
/// no link, read without following one (`specs/ap/run-limits.md` R15).
fn plain_names(home: &Path) -> bool {
    [CREDENTIALS, "CLAUDE.md"]
        .iter()
        .all(|name| match std::fs::symlink_metadata(home.join(name)) {
            Ok(m) => m.file_type().is_file(),
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        })
}
/// The `bwrap` words of the home of a launch on `account` (`specs/ap/run-limits.md` R5): each
/// account of the request hidden by an empty directory, the home bound, with a sign-in the derived
/// file over its `.credentials.json`, the account's `CLAUDE.md` when it is a regular file (a link
/// followed), and each account read-only.
fn home_words(accounts: &[String], account: &str, home: &Path, signed: bool) -> Vec<String> {
    let mut once: Vec<&str> = vec![];
    for a in accounts {
        if !once.contains(&a.as_str()) {
            once.push(a);
        }
    }
    let at = |name: &str| home.join(name).to_string_lossy().into_owned();
    let home_text = home.to_string_lossy();
    let mut words = vec![];
    for a in &once {
        words.extend(args(&["--tmpfs", a]));
    }
    words.extend(args(&["--bind", &home_text, &home_text]));
    if signed {
        let derived = sign_in_path(home);
        words.extend(args(&["--ro-bind", &derived, &at(CREDENTIALS)]));
    }
    let md = Path::new(account).join("CLAUDE.md");
    if std::fs::metadata(&md).is_ok_and(|m| m.is_file()) {
        words.extend(args(&[
            "--ro-bind",
            &md.to_string_lossy(),
            &at("CLAUDE.md"),
        ]));
    }
    for a in &once {
        words.extend(args(&["--remount-ro", a]));
    }
    words
}
/// The name of an account's sign-in, and of the derived file in a home.
const CREDENTIALS: &str = ".credentials.json";
/// The largest sign-in `ap run` derives from (`specs/ap/run-limits.md` R15).
const SIGN_IN_MAX: usize = 65_536;
/// A derived sign-in is padded with spaces to a multiple of this.
const SIGN_IN_BLOCK: usize = 4_096;
/// A reading of the sign-in while a launch runs is due this long after the last one.
const READING: Duration = Duration::from_secs(30);
/// At most one call of `claude auth status` for one account in this long.
const RENEWAL_GAP: Duration = Duration::from_secs(60);
/// An end this near (in milliseconds), or past, has the CLI outside `limits` renew the sign-in.
const NEAR_END_MS: f64 = 210_000.0;
/// The derived sign-in of a home: `<account>/homes/<session>.sign-in`.
fn sign_in_path(home: &Path) -> String {
    format!("{}.sign-in", home.to_string_lossy())
}
/// The derived text of a sign-in: one JSON object of at most 65,536 bytes whose `claudeAiOauth` is
/// an object with a nonempty string `accessToken`, without `refreshToken` and
/// `refreshTokenExpiresAt`, padded with spaces to the least multiple of 4,096 bytes that holds it.
/// None for any other content (`specs/ap/run-limits.md` R15).
fn derive(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > SIGN_IN_MAX {
        return None;
    }
    let mut v: Value = serde_json::from_slice(bytes).ok()?;
    let oauth = v
        .as_object_mut()?
        .get_mut("claudeAiOauth")?
        .as_object_mut()?;
    let token = oauth.get("accessToken").and_then(Value::as_str);
    if token.is_none_or(str::is_empty) {
        return None;
    }
    oauth.remove("refreshToken");
    oauth.remove("refreshTokenExpiresAt");
    let mut text = serde_json::to_vec(&v).ok()?;
    text.resize(text.len().div_ceil(SIGN_IN_BLOCK) * SIGN_IN_BLOCK, b' ');
    Some(text)
}
/// What one reading of an account's sign-in gives.
enum Reading {
    /// The account has no sign-in.
    Absent,
    /// The derived text.
    Derived(Vec<u8>),
    /// A sign-in of another form, or one that cannot be read.
    Unfit,
}
/// When `claude auth status` ran last for each account in this call of `ap run`.
#[derive(Default)]
struct Renewed(Vec<(String, Instant)>);
/// One reading of the account's sign-in through the port (`specs/ap/run-limits.md` R15): when its
/// `expiresAt` is near or past, and no call ran for the account in the last 60 seconds, first the
/// CLI outside `limits` is asked for its status, which renews it; its answer is not read.
fn reading(s: &mut impl System, account: &str, renewed: &mut Renewed) -> Reading {
    let path = Path::new(account).join(CREDENTIALS);
    let path = path.to_string_lossy().into_owned();
    let mut bytes = read_sign_in(s, &path);
    let near = |b: &[u8], wall: SystemTime| {
        let v: Option<Value> = (b.len() <= SIGN_IN_MAX)
            .then(|| serde_json::from_slice(b).ok())
            .flatten();
        let end = v.and_then(|v| v["claudeAiOauth"]["expiresAt"].as_f64());
        let now = wall.duration_since(UNIX_EPOCH).unwrap_or_default();
        end.is_some_and(|end| end - now.as_millis() as f64 <= NEAR_END_MS)
    };
    let due = matches!(&bytes, Ok(Some(b)) if near(b, s.wall()));
    let last = renewed
        .0
        .iter()
        .find(|(a, _)| a == account)
        .map(|(_, t)| *t);
    if due && last.is_none_or(|t| s.now().duration_since(t) >= RENEWAL_GAP) {
        renewed.0.retain(|(a, _)| a != account);
        renewed.0.push((account.to_owned(), s.now()));
        let mut words = vec!["-i".to_owned()];
        let kept = base_env(s, &[])
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"));
        words.extend(kept);
        words.push(format!("CLAUDE_CONFIG_DIR={account}"));
        words.extend(args(&["claude", "auth", "status"]));
        let _ = s.call("env", &words, &Value::Null, 30_000);
        bytes = read_sign_in(s, &path);
    }
    match bytes {
        Ok(None) => Reading::Absent,
        Ok(Some(b)) => derive(&b).map_or(Reading::Unfit, Reading::Derived),
        Err(()) => Reading::Unfit,
    }
}
/// The account's sign-in as it is now, through the port: None when there is none, Err when it
/// cannot be read.
fn read_sign_in(s: &impl System, path: &str) -> Result<Option<Vec<u8>>, ()> {
    match s.read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}
/// The derived sign-in of a running launch: its path, the open file, the text it holds and when it
/// was last read. Dropped, it is removed (`specs/ap/run-limits.md` R15).
struct SignIn {
    path: String,
    file: File,
    text: Vec<u8>,
    last: Instant,
}
impl SignIn {
    /// Creates the file anew, mode 0600, after removing a file a killed call left at its name.
    fn create(path: String, text: Vec<u8>, last: Instant) -> std::io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::remove_file(&path);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(OWNER_ONLY)
            .open(&path)?;
        let mut sign = Self {
            path,
            file,
            text: vec![],
            last,
        };
        sign.file.write_all(&text)?;
        sign.text = text;
        Ok(sign)
    }
    /// Rewrites the file in place when `text` differs: the same inode, one write of the whole text
    /// from the first byte, the length set only when it differs.
    fn rewrite(&mut self, text: Vec<u8>) {
        if text == self.text || self.file.write_all_at(&text, 0).is_err() {
            return;
        }
        if text.len() != self.text.len() {
            let _ = self.file.set_len(text.len() as u64);
        }
        self.text = text;
    }
}
impl Drop for SignIn {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
/// A fresh `agy` stage's home: a new directory (mode 0700) under `<account>/homes` that holds three
/// links and the directories that hold them: the account's token, `AGENTS.md` and `GEMINI.md`.
fn make_home(account: &str, home: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, symlink};
    let mut dirs = std::fs::DirBuilder::new();
    dirs.mode(0o700);
    if let Some(homes) = home.parent() {
        match dirs.create(homes) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e),
            _ => (),
        }
    }
    for dir in ["", ".gemini", ".gemini/antigravity-cli"] {
        dirs.create(home.join(dir))?;
    }
    let (from, to) = (Path::new(account).join(".gemini"), home.join(".gemini"));
    for link in [
        "antigravity-cli/antigravity-oauth-token",
        "AGENTS.md",
        "GEMINI.md",
    ] {
        symlink(from.join(link), to.join(link))?;
    }
    Ok(())
}
const TOKENS: [&str; 4] = ["input", "output", "cache_read", "cache_creation"];
/// The tokens of a Codex or `agy` thread up to the end of the run of `file`: that record's and those of
/// every record it resumes (the CLI reports the thread's usage, a record its own part). None when
/// the chain cannot be followed to its end: a record that cannot be read, a record met twice, or
/// more than 256 records.
fn spent(file: &str) -> Option<[u64; 4]> {
    let (mut sum, mut next, mut met) = ([0; 4], file.to_owned(), vec![]);
    while met.len() < 256 && !met.contains(&next) {
        let rec: Value = serde_json::from_slice(&std::fs::read(&next).ok()?).ok()?;
        for (s, k) in sum.iter_mut().zip(TOKENS) {
            *s += rec["tokens"][k].as_u64().unwrap_or(0);
        }
        met.push(next);
        match &rec["resumes"] {
            Value::Null => return Some(sum),
            v => next = v.as_str()?.to_owned(),
        }
    }
    None
}

/// UTC RFC 3339 to the second.
pub(crate) fn stamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to the civil date (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}
/// `*` matches any run of characters; every other character matches itself.
fn glob(pattern: &str, line: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), line.chars().collect());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if i < p.len() && p[i] == t[j] {
            i += 1;
            j += 1;
        } else if let Some(at) = star {
            i = at + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}
/// Writes through a temporary name and a rename: a reader never sees half a file.
/// The temporary name is unique per write and created exclusively, so it is never a file of
/// another run (whose `out` may be any name).
pub(crate) fn write(path: &str, bytes: &[u8]) -> Result<(), Failure> {
    write_with(path, bytes, None)
}
/// `write` with the mode every temporary name is created with (the default options' when none).
fn write_with(path: &str, bytes: &[u8], mode: Option<u32>) -> Result<(), Failure> {
    use std::os::unix::fs::OpenOptionsExt;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for n in 0..8 {
        let tmp = format!("{path}.{}-{nanos}-{n}.ap-tmp", std::process::id());
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        if let Some(mode) = mode {
            options.mode(mode);
        }
        match options.open(&tmp) {
            Ok(mut f) => {
                let done = f.write_all(bytes).and_then(|()| f.sync_all());
                let done = done.and_then(|()| std::fs::rename(&tmp, path));
                if done.is_err() {
                    let _ = std::fs::remove_file(&tmp);
                }
                return done.map_err(|_| error("io_error", "out"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => break,
        }
    }
    Err(error("io_error", "out"))
}
/// The mode of the record, the stream and the error file: readable by their owner alone, asked
/// for at the creation (`specs/ap/run.md` R11).
const OWNER_ONLY: u32 = 0o600;
fn write_record(out: &str, record: &Value) -> Result<(), Failure> {
    let mut bytes = serde_json::to_vec_pretty(record).map_err(|_| error("io_error", "record"))?;
    bytes.push(b'\n');
    write_with(&record_path(out), &bytes, Some(OWNER_ONLY))
}
/// After an effect: the record through the rename, else written in place. False when neither
/// write succeeded.
fn persist(out: &str, record: &Value) -> bool {
    use std::os::unix::fs::OpenOptionsExt;
    write_record(out, record).is_ok() || {
        let mut bytes = serde_json::to_vec_pretty(record).unwrap_or_default();
        bytes.push(b'\n');
        let place = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(OWNER_ONLY)
            .open(record_path(out));
        place.is_ok_and(|mut f| f.write_all(&bytes).is_ok())
    }
}
/// What the stream of one run showed.
#[derive(Default)]
struct Seen {
    /// Keep only the first `result` (a reviewer's stage ends there).
    first_only: bool,
    /// Every `result` event in order; a session that left a shell can emit more than one.
    results: Vec<Value>,
    /// An assistant message used a tool: the session did work.
    worked: bool,
    /// A rate-limit event said `rejected`.
    rejected: bool,
    /// The rate-limit event `limit` is made from: the last rejected one, else the last one.
    limit: Option<Value>,
    /// The CLI the lines come from, with a Codex or `agy` stream's thread tokens before this run (None:
    /// they are unknown, so this run's are too) and a Muse stream's requested model.
    runtime: Runtime,
    spent: Option<[u64; 4]>,
    model: String,
    /// A Codex stream's thread (an `agy` stream's conversation), the last agent message of the turn running, and the items
    /// completed since the last turn end.
    thread: Option<String>,
    message: Option<String>,
    items: u64,
    /// A Muse stream: the last `run_usage` and the first other model that served the stage.
    usage: Option<Value>,
    served: Option<String>,
}
impl Seen {
    fn done(&self) -> bool {
        self.first_only && !self.results.is_empty()
    }
    fn line(&mut self, line: &[u8]) {
        if self.done() {
            return;
        }
        let Ok(e) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        match self.runtime {
            Runtime::Claude => self.claude_line(e),
            Runtime::Codex => self.codex_line(&e, self.spent),
            Runtime::Muse => self.muse_line(&e["payload"]),
            Runtime::Agy => self.agy_line(&e),
        }
    }
    fn claude_line(&mut self, e: Value) {
        match e["type"].as_str() {
            Some("result") => self.results.push(e),
            Some("assistant") => {
                let tools = e["message"]["content"].as_array().into_iter().flatten();
                self.worked |= tools.into_iter().any(|b| b["type"] == "tool_use");
            }
            Some("rate_limit_event") => {
                let rejected = e["rate_limit_info"]["status"] == "rejected";
                if rejected || !self.rejected {
                    self.limit = Some(e);
                }
                self.rejected |= rejected;
            }
            _ => (),
        }
    }
    /// A Muse Code line, read into the shape of a `result`: each `run_terminal` is one, its `text`
    /// the answer when it completed, its `reason` the error's message otherwise. A tool task is
    /// work; a facet that names another serving model than the request's is noted; the last
    /// `run_usage` gives the tokens and the limit.
    fn muse_line(&mut self, p: &Value) {
        let text = |k: &str| p[k].as_str().unwrap_or("");
        match p["kind"].as_str() {
            Some("task_lifecycle") => {
                let event = &p["event"];
                let tool = event["task_kind"]
                    .as_str()
                    .is_some_and(|k| k.starts_with("tool."));
                self.worked |= tool;
                let facets = event["details"]["facets"].as_array().into_iter().flatten();
                let mut other = facets.filter_map(|f| f["detail"]["model"].as_str());
                let other = other.find(|m| *m != self.model);
                self.served = self.served.take().or(other.map(str::to_owned));
            }
            Some("run_usage") => self.usage = Some(p.clone()),
            Some("run_terminal") => {
                let done = p["terminal"] == "completed";
                self.results
                    .push(json!({"type": "result", "is_error": !done,
                    "result": if done { text("text") } else { text("reason") },
                    "permission_denials": []}));
            }
            _ => (),
        }
    }
    /// An `agy` line, read into the shape of a `result`: `init` names the conversation, a `tool`
    /// step is work, the `result` line is one (its `response` the answer when its `status` is
    /// `SUCCESS`, its `error` the message otherwise). The usage is the conversation's, less what the
    /// earlier records of it hold; `output_tokens` already includes the thinking tokens.
    fn agy_line(&mut self, e: &Value) {
        match e["event"].as_str() {
            Some("init") => {
                let id = e["conversation_id"].as_str().filter(|id| !id.is_empty());
                self.thread = self.thread.take().or(id.map(str::to_owned));
            }
            Some("step_update") => self.worked |= e["step_update"]["step_type"] == "tool",
            Some("result") => {
                let r = &e["result"];
                let n = |k: &str| r["usage"][k].as_u64().unwrap_or(0);
                let own = |v: u64, i: usize| self.spent.map(|s| v.saturating_sub(s[i]));
                let done = r["status"] == "SUCCESS";
                self.results
                    .push(json!({"type": "result", "is_error": !done,
                    "result": if done { &r["response"] } else { &r["error"] },
                    "num_turns": r["num_turns"], "permission_denials": [],
                    "usage": {"input_tokens": own(n("input_tokens"), 0),
                        "output_tokens": own(n("output_tokens"), 1),
                        "cache_read_input_tokens": own(n("cache_read_tokens"), 2),
                        "cache_creation_input_tokens": own(0, 3)}}));
            }
            _ => (),
        }
    }
    /// A Codex event, read into the shape of a `result`: a turn end is one, with the last agent
    /// message of its turn as the answer when it completed, the error's message when it failed.
    /// An `error` event alone is none: the CLI prints one for each reconnection attempt.
    fn codex_line(&mut self, e: &Value, spent: Option<[u64; 4]>) {
        let (item, kind) = (&e["item"], e["item"]["type"].as_str());
        match e["type"].as_str().unwrap_or("") {
            "thread.started" => self.thread = e["thread_id"].as_str().map(str::to_owned),
            "turn.started" => self.message = None,
            t @ ("item.started" | "item.completed") => {
                self.worked |= !matches!(kind, Some("agent_message" | "reasoning" | "error"));
                if t == "item.completed" {
                    self.items += 1;
                    if kind == Some("agent_message") {
                        self.message = Some(item["text"].as_str().unwrap_or("").to_owned());
                    }
                }
            }
            t @ ("turn.completed" | "turn.failed") => {
                let message = self.message.take();
                let mut r = json!({"type": "result", "session_id": self.thread,
                    "num_turns": std::mem::take(&mut self.items), "is_error": t == "turn.failed",
                    "result": e["error"]["message"], "permission_denials": []});
                if t == "turn.completed" {
                    // As evals/readout.py maps a Codex session; the thread's usage less what the
                    // runs before this one spent.
                    let u = |k: &str| e["usage"][k].as_u64();
                    let (read, write) = (u("cached_input_tokens"), u("cache_write_input_tokens"));
                    let (read, write) = (read.unwrap_or(0), write.unwrap_or(0));
                    let input = u("input_tokens").map(|i| i.saturating_sub(read + write));
                    let own = |v: Option<u64>, i: usize| {
                        spent.and_then(|s| v.map(|v| v.saturating_sub(s[i])))
                    };
                    r["result"] = json!(message.unwrap_or_default());
                    r["usage"] = json!({"input_tokens": own(input, 0),
                        "output_tokens": own(u("output_tokens"), 1),
                        "cache_read_input_tokens": own(Some(read), 2),
                        "cache_creation_input_tokens": own(Some(write), 3)});
                }
                self.results.push(r);
            }
            _ => (),
        }
    }
}
/// `status` and `resets_at` of a rate-limit event or of one of its windows, by the shape stated in
/// README "One stage": a member of another shape is null.
fn limit_part(v: &Value) -> Value {
    json!({"status": v["status"].as_str(), "resets_at": v["resetsAt"].as_u64()})
}
/// The record's `limit`: what the one rate-limit event of the launch held. `overageStatus` and
/// every member not named here are not read.
fn limit_of(e: Option<&Value>) -> Value {
    let Some(e) = e else {
        return Value::Null;
    };
    let info = &e["rate_limit_info"];
    let mut limit = limit_part(info);
    limit["kind"] = json!(info["rateLimitType"].as_str());
    let windows = info["unifiedWindows"].as_object().into_iter().flatten();
    // `unifiedWindows` is a sorted map: the first eight objects are the first by name.
    let windows = windows.filter(|(_, w)| w.is_object()).take(8);
    limit["windows"] = windows
        .map(|(name, w)| {
            let mut one = limit_part(w);
            one["utilization"] = if w["utilization"].is_number() {
                w["utilization"].clone()
            } else {
                Value::Null
            };
            (name.clone(), one)
        })
        .collect();
    limit
}
/// A Muse stage's `turns` and `tokens` from its last `run_usage` line: the sums over its `models`,
/// where `input` is each entry's `input_tokens` less its `cached_tokens` (never below 0) and a
/// member that is no number counts 0; null without such a line.
fn muse_tally(usage: Option<&Value>) -> (Value, Value) {
    let Some(u) = usage else {
        let none =
            json!({"input": null, "output": null, "cache_read": null, "cache_creation": null});
        return (Value::Null, none);
    };
    let (mut input, mut read, mut output) = (0u64, 0u64, 0u64);
    for m in u["models"].as_array().into_iter().flatten() {
        let n = |k: &str| m[k].as_u64().unwrap_or(0);
        input += n("input_tokens").saturating_sub(n("cached_tokens"));
        (read, output) = (read + n("cached_tokens"), output + n("output_tokens"));
    }
    let tokens = json!({"input": input, "output": output, "cache_read": read, "cache_creation": 0});
    (json!(u["calls"].as_u64()), tokens)
}
/// A Muse stage's `limit`: the object of the Claude shape from the `allowance` of its last
/// `run_usage` line, with one entry per window (`weekly`, `window`) that is an object.
fn muse_limit(usage: Option<&Value>) -> Value {
    let Some(a) = usage.map(|u| &u["allowance"]).filter(|a| a.is_object()) else {
        return Value::Null;
    };
    let windows: serde_json::Map<String, Value> = ["weekly", "window"]
        .into_iter()
        .filter(|k| a[*k].is_object())
        .map(|k| {
            let w = &a[k];
            let at = w["resets_at_ms"].as_u64().map(|ms| ms / 1000);
            let used = w["used_percent"].as_f64().map(|p| p / 100.0);
            (
                k.to_owned(),
                json!({"status": null, "resets_at": at, "utilization": used}),
            )
        })
        .collect();
    json!({"status": null, "resets_at": null, "kind": null, "windows": windows})
}
/// Reads what the CLI appended to its output since the last call, through the handle opened
/// before the launch (`read_at` leaves the CLI's file offset alone); whole lines only.
fn follow(file: &File, offset: &mut u64, partial: &mut Vec<u8>, seen: &mut Seen) {
    let mut chunk = vec![0; 1 << 16];
    // At most 16 MiB per call, so the caller's clock is read again.
    for _ in 0..256 {
        let n = match file.read_at(&mut chunk, *offset) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        *offset += n as u64;
        partial.extend_from_slice(&chunk[..n]);
        while let Some(at) = partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = partial.drain(..=at).collect();
            seen.line(&line);
        }
    }
}
/// The members of the record that come from the `result` events, and the state the LAST
/// result gives: turns and tokens are the session's totals; the answer and `expect` are the last.
fn judge(
    q: &Request,
    record: &mut Value,
    seen: &Seen,
    e: &Value,
    write_failed: &mut bool,
) -> &'static str {
    let sum = |pick: &dyn Fn(&Value) -> &Value| -> Value {
        seen.results
            .iter()
            .map(|r| pick(r).as_u64())
            .sum::<Option<u64>>()
            .map_or(Value::Null, Value::from)
    };
    record["turns"] = sum(&|r| &r["num_turns"]);
    record["tokens"] = json!({
        "input": sum(&|r| &r["usage"]["input_tokens"]),
        "output": sum(&|r| &r["usage"]["output_tokens"]),
        "cache_read": sum(&|r| &r["usage"]["cache_read_input_tokens"]),
        "cache_creation": sum(&|r| &r["usage"]["cache_creation_input_tokens"]),
    });
    let mut denials: Vec<Value> = vec![];
    for d in seen
        .results
        .iter()
        .flat_map(|r| r["permission_denials"].as_array().into_iter().flatten())
    {
        let d = json!({"tool_name": d["tool_name"], "tool_use_id": d["tool_use_id"]});
        if !denials.contains(&d) {
            denials.push(d);
        }
    }
    record["permission_denials"] = json!(denials);
    if let Some(id) = e["session_id"].as_str().filter(|id| !id.is_empty()) {
        record["session"] = json!(id);
    }
    // A typed answer is the object, as JSON text; a result without one is saved as its text.
    let typed = q.schema.is_some();
    let object = e["structured_output"].as_object().filter(|_| typed);
    let body = match object {
        Some(o) => [
            serde_json::to_vec_pretty(o).unwrap_or_default(),
            b"\n".to_vec(),
        ]
        .concat(),
        None => e["result"].as_str().unwrap_or("").as_bytes().to_vec(),
    };
    let missed = typed && object.is_none();
    if cut(seen, e) {
        "cut"
    } else if e["is_error"] == true {
        "failed"
    } else if body.is_empty() && !missed {
        record["unmet"] = json!("answer");
        "no_result"
    } else if !body.is_empty() && write(&q.out, &body).is_err() {
        *write_failed = true;
        record["unmet"] = json!("write");
        "failed"
    } else {
        if !body.is_empty() {
            record["answer"] = json!({"path": q.out, "sha256": digest(&body)});
        }
        if missed {
            record["unmet"] = json!("schema");
            "no_result"
        } else if q
            .expect_file
            .as_deref()
            .is_some_and(|f| !written(f, &q.expect_before))
        {
            record["unmet"] = json!("file");
            "no_result"
        } else if q.expect_line.as_deref().is_some_and(|p| {
            !glob(
                p,
                e["result"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or(""),
            )
        }) {
            record["unmet"] = json!("first_line");
            "no_result"
        } else if !second_met(q, e["result"].as_str().unwrap_or("")) {
            record["unmet"] = json!("second_line");
            "no_result"
        } else {
            "ok"
        }
    }
}
enum End {
    Result,
    Exit,
    Timeout,
}
/// Stops the whole unit (every process in its control group), reaps the CLI and confirms the
/// unit is no longer active. False when that cannot be confirmed.
fn stop(s: &mut impl System, unit: &str, child: &mut Child) -> bool {
    let _ = s.call("systemctl", &args(&["stop", unit]), &Value::Null, 60_000);
    let mut reaped = false;
    for _ in 0..200 {
        if !matches!(child.try_wait(), Ok(None)) {
            reaped = true;
            break;
        }
        s.pause(Duration::from_millis(100));
    }
    if !reaped {
        let _ = child.kill();
        let _ = child.wait();
    }
    ended(s, unit)
}
/// A finished answer is never read as a cut: without `is_error`, the usage-limit text is a cut
/// only when the stream also holds a rejected rate-limit event.
fn cut(seen: &Seen, e: &Value) -> bool {
    let answer = e["result"].as_str().unwrap_or("");
    (limit_text(answer) && seen.rejected)
        || (e["is_error"] == true
            && (seen.rejected || e["api_error_status"] == 429 || limit_text(answer)))
}
/// The result the stage is judged by: the last one whose answer meets `expect` (a later result
/// does not unmake it), else the last one.
fn chosen<'a>(q: &Request, seen: &'a Seen) -> Option<&'a Value> {
    let meets = |e: &&Value| {
        let answer = e["result"].as_str().unwrap_or("");
        let typed = q.schema.is_some();
        e["is_error"] != true
            && !cut(seen, e)
            && if typed {
                e["structured_output"].is_object()
            } else {
                !answer.is_empty()
                    && q.expect_line
                        .as_deref()
                        .is_none_or(|p| glob(p, answer.lines().next().unwrap_or("")))
                    && second_met(q, answer)
            }
    };
    seen.results
        .iter()
        .rev()
        .find(meets)
        .or(seen.results.last())
}
/// The answer's second line meets `expect.second_line`, or none is given (R16, R17).
fn second_met(q: &Request, answer: &str) -> bool {
    q.expect_second
        .as_deref()
        .is_none_or(|p| glob(p, answer.lines().nth(1).unwrap_or("")))
}
fn limit_text(text: &str) -> bool {
    let t = text.to_lowercase();
    t.len() < 400
        && (t.contains("usage limit")
            || (t.contains("hit your") && t.contains("limit"))
            || t.contains("resets "))
}
/// The Claude Code CLI after `systemd-run … --`. No hook of any source; no auto-memory unless
/// asked; the Bash tool runs a 40-minute check in the foreground and never moves a command to the
/// background.
fn claude_args(q: &Request, argv: &mut Vec<String>, resume: Option<&str>, session: &str) {
    let mut settings = json!({"disableAllHooks": true, "env": {
        "BASH_DEFAULT_TIMEOUT_MS": BASH_MS, "BASH_MAX_TIMEOUT_MS": BASH_MS,
        "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"}});
    if !q.memory {
        settings["autoMemoryEnabled"] = json!(false);
    }
    if let Some(window) = q.compact_window {
        settings["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"] = json!(window.to_string());
    }
    argv.extend(args(&[
        "--",
        "claude",
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--model",
        &q.model,
        "--effort",
        &q.effort,
        "--permission-mode",
        &q.mode,
        "--permission-prompts",
        "none",
        "--setting-sources",
        SOURCES,
        "--settings",
        &settings.to_string(),
        "--disallowedTools",
        "Monitor",
        "--strict-mcp-config",
    ]));
    for m in &q.mcp {
        argv.extend(args(&["--mcp-config", m]));
    }
    if !q.skills {
        argv.push("--disable-slash-commands".into());
    }
    if let Some((_, text, _)) = &q.schema {
        argv.extend(args(&["--json-schema", text]));
    }
    argv.extend(args(&[
        if resume.is_some() {
            "--resume"
        } else {
            "--session-id"
        },
        session,
    ]));
}
/// The Codex CLI after `systemd-run … --`, headless and without the home's configuration, hooks,
/// rules, apps, plugins, memories or further agents (README "One stage" names each flag).
fn codex_args(q: &Request, resume: Option<&str>) -> Vec<String> {
    let mut a = args(&["--", "codex", "--no-daemon", "exec"]);
    a.extend(resume.map(|_| "resume".to_owned()));
    a.extend(args(&["--json", "--ignore-user-config", "--ignore-rules"]));
    for feature in [
        "hooks",
        "apps",
        "plugins",
        "plugin_discovery_prefetch",
        "skill_mcp_dependency_install",
        "memories",
        "multi_agent",
    ] {
        a.extend(args(&["--disable", feature]));
    }
    let effort = format!("model_reasoning_effort=\"{}\"", q.effort);
    a.extend(args(&[
        "-c",
        "features.multi_agent_v2.max_concurrent_threads_per_session=1",
        "--skip-git-repo-check",
        "--dangerously-bypass-approvals-and-sandbox",
        "-m",
        &q.model,
        "-c",
        &effort,
    ]));
    a.extend(match resume {
        Some(session) => args(&[session]),
        None => args(&["-C", &q.cwd]),
    });
    a.push("-".into());
    a
}
/// The Muse Code CLI after `systemd-run … --`: one prompt from a file, headless, no approval and
/// no sandbox (`--yolo`), the contributor mode off (README "A stage on Muse Code").
fn muse_args(q: &Request, session: &str) -> Vec<String> {
    args(&[
        "--",
        "muse",
        "exec",
        "--json",
        "--model",
        &q.model,
        "--reasoning-effort",
        &q.effort,
        "--contributor-mode",
        "off",
        "--disable-reminders",
        "--session-id",
        session,
        "--workspace",
        &q.cwd,
        "--prompt-file",
        &q.prompt,
        "--yolo",
    ])
}
/// The Antigravity CLI after `systemd-run … --`: the prompt as one argument, headless, no approval
/// (README "A stage on the Antigravity CLI"); the stage's bound is the CLI's own.
fn agy_args(q: &Request, resume: Option<&str>) -> Vec<String> {
    let bound = format!("{}s", q.timeout);
    let mut a = args(&["--", "agy", "--print", &q.argument]);
    a.extend(args(&[
        "--output-format",
        "stream-json",
        "--model",
        &q.model,
    ]));
    a.extend(args(&[
        "--dangerously-skip-permissions",
        "--print-timeout",
        &bound,
    ]));
    if let Some(conversation) = resume {
        a.extend(args(&["--conversation", conversation]));
    }
    a
}
/// The session the Codex CLI named at the thread's start (or the `agy` conversation at its `init`) enters the record at once. False when
/// that write of the record failed, which counts as every failed write of it after the launch.
fn named(out: &str, record: &mut Value, seen: &Seen) -> bool {
    if let (true, Some(thread)) = (record["session"].is_null(), &seen.thread) {
        record["session"] = json!(thread);
        mirror(record, seen.worked);
        return persist(out, record);
    }
    true
}
/// The last entry of `attempts` takes what the record holds of the launch running.
fn mirror(record: &mut Value, worked: bool) {
    let mut entry = json!({"worked": worked});
    for k in ["account", "session", "start", "end", "state", "limit"] {
        entry[k] = record[k].clone();
    }
    if let Some(last) = record["attempts"].as_array_mut().and_then(|a| a.last_mut()) {
        for (k, v) in entry.as_object().into_iter().flatten() {
            last[k] = v.clone();
        }
    }
}
/// What the launch reads on stdin: the prompt file, or with a `lead` (the continuation, or the
/// line of a move) that text, an empty line and the file's bytes, through a pipe that a thread
/// fills (no file); an ask reads its sentence alone.
fn input(q: &Request, lead: Option<&str>, ask: bool) -> std::io::Result<File> {
    // A Muse stage reads its prompt from the file the argument names, an `agy` stage has its text
    // as an argument, and neither reads stdin.
    let file = match q.runtime {
        Runtime::Muse | Runtime::Agy => return File::open("/dev/null"),
        Runtime::Claude | Runtime::Codex => File::open(&q.prompt)?,
    };
    let Some(lead) = lead else {
        return Ok(file);
    };
    let mut bytes = lead.as_bytes().to_vec();
    if !ask {
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    }
    let (reader, mut writer) = std::io::pipe()?;
    // The write ends with the pipe: when the CLI is gone, the reader is closed and the write fails.
    std::thread::spawn(move || writer.write_all(&bytes));
    Ok(File::from(std::os::fd::OwnedFd::from(reader)))
}
/// What one launch of a call is given besides the request.
struct Launch<'a> {
    /// The entries of `attempts` of the launches before it.
    earlier: &'a [Value],
    /// The text its input holds first (with `ask`, its whole input).
    lead: Option<&'a str>,
    /// The session it resumes; without one it is a new session.
    resume: Option<&'a str>,
    /// It asks the session of the launch before it again (R20).
    ask: bool,
    /// The scratch directory, if the request has one, could be made.
    ready: bool,
    /// The proxy of a call with `limits` (`specs/ap/run-limits.md` R17), when it could be opened.
    proxy: Option<&'a egress::Proxy>,
    /// The host's Git hooks directory when R4 made the stage's `hooks/pre-commit` (R16 (k)).
    hooks: Option<&'a Path>,
    /// Its time bound.
    bound: Duration,
}
/// One launch on one account, after the launches `earlier` (their entries of `attempts`); with a
/// `lead` its input holds that text first; it resumes the session `resume` when one is given, else
/// it is a new session. Returns the final record, whether the session did work, and whether every
/// write after the first effect succeeded.
fn attempt(
    s: &mut impl System,
    q: &Request,
    account: &str,
    (output, errs): &(File, File),
    launch: &Launch,
    object: &Object,
    renewed: &mut Renewed,
) -> (Value, bool, bool) {
    let Launch {
        earlier,
        lead,
        resume,
        ask,
        ready,
        proxy,
        hooks,
        bound,
    } = *launch;
    // The continuation is the text of a fresh call; a resumed call's text is a move's; an ask is
    // neither.
    let continued = lead.is_some() && q.resume.is_none() && !ask;
    let mut record = json!({
        "role": q.role, "runtime": q.runtime.word(),
        "model": q.model, "effort": q.effort,
        "account": account, "session": resume, "cwd": q.cwd, "unit": null,
        "prompt": {"path": q.prompt, "sha256": q.prompt_sha256},
        "answer": null, "start": stamp(SystemTime::now()), "end": null, "state": "running",
        "turns": null,
        "tokens": {"input": null, "output": null, "cache_read": null, "cache_creation": null},
        "permission_denials": [], "results": 0, "expect_met": null,
        "expect_file_before": q.expect_before, "unmet": null,
        "resumes": q.resume.as_ref().map(|(_, _, file)| file), "limit": null,
        "compact_window": q.compact_window,
        "policy": policy_member(q),
        "run_object": object.member(),
    });
    let mut attempts = earlier.to_vec();
    attempts.push(json!({"continued": continued, "asked": ask}));
    record["attempts"] = json!(attempts);
    mirror(&mut record, false);
    if let Some((path, _, sha256)) = &q.schema {
        record["answer_schema"] = json!({"path": path, "sha256": sha256});
    }
    let not_launched = |mut record: Value, unmet: &str| {
        record["state"] = json!("failed");
        record["unmet"] = json!(unmet);
        record["end"] = json!(stamp(SystemTime::now()));
        // The text went to no launch.
        record["attempts"][earlier.len()]["continued"] = json!(false);
        mirror(&mut record, false);
        let saved = persist(&q.out, &record);
        (record, false, saved)
    };
    // The session `start_run` named is the first launch's.
    let before = object.session.as_deref().filter(|_| earlier.is_empty());
    let (Ok(session), Ok(unit)) = (
        resume
            .or(before)
            .map_or_else(|| new_session_id(s), |r| Ok(r.to_owned())),
        new_session_id(s),
    ) else {
        return not_launched(record, "launch");
    };
    let unit = format!("ap-run-{unit}.scope");
    // The Codex CLI names a fresh session itself, at the thread's start, and `agy` at its `init`.
    let names_itself = match q.runtime {
        Runtime::Codex | Runtime::Agy => resume.is_none(),
        Runtime::Claude | Runtime::Muse => false,
    };
    if !names_itself {
        record["session"] = json!(session);
    }
    // An `agy` stage's home: the record's on a resume, else a new one named by the fresh id.
    let home = (q.runtime == Runtime::Agy).then(|| match &q.home {
        Some(home) => PathBuf::from(home),
        None => Path::new(account).join("homes").join(&session),
    });
    if let Some(home) = &home {
        record["home"] = json!(home);
    }
    // A stage with `limits` runs in its home of the launch, `<account>/homes/<session>`: a new
    // session's, or the one of the session it continues (`specs/ap/run-limits.md` R14).
    let stage_home = q
        .limits
        .as_ref()
        .map(|_| Path::new(account).join("homes").join(&session));
    if let Some(home) = &stage_home {
        record["home"] = json!(home);
    }
    // The stage's view of the host (R16), and the hidden paths of this launch as they exist when
    // it is prepared: those a bind of the launch holds or lies in, but the one shown (R7, R10).
    let view = q
        .limits
        .as_ref()
        .map(|l| view(q, l, &s.work_root(), hooks))
        .unwrap_or_default();
    let hidden = match (&q.limits, &stage_home) {
        (Some(l), Some(home)) => {
            let mut targets = view.targets.clone();
            targets.push(account.to_owned());
            targets.extend(l.targets());
            targets.push(lossy(home));
            l.hidden(&s.work_root(), &targets)
        }
        _ => vec![],
    };
    if let (Some(l), Some(home)) = (&q.limits, &stage_home) {
        let paths: Vec<&String> = hidden.iter().map(|(p, _)| p).collect();
        let mut writable = vec![home.to_string_lossy().into_owned()];
        writable.extend(l.writable().into_iter().map(str::to_owned));
        let mut limits = json!({"scratch": l.scratch, "writable": writable,
            "bwrap": BWRAP, "hidden": paths, "view": view.sources});
        for (k, v) in &l.given {
            limits[k] = v.clone();
        }
        // The stage's proxy and the names its role reaches (R7, R17, R18).
        if let Some(proxy) = proxy {
            limits["egress"] = proxy.member();
        }
        record["limits"] = limits;
    }
    record["unit"] = json!(unit);
    mirror(&mut record, false);
    // The run's own stream and error files, emptied for a next account's attempt.
    let empty = |mut f: &File| {
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.try_clone()
    };
    let (Ok(prompt), Ok(stdout), Ok(stderr)) = (input(q, lead, ask), empty(output), empty(errs))
    else {
        return not_launched(record, "launch");
    };
    let reader = output;
    // The record names the session BEFORE the launch.
    if !persist(&q.out, &record) {
        return not_launched(record, "write");
    }
    if let (Some(home), None) = (&home, &q.home)
        && make_home(account, home).is_err()
    {
        return not_launched(record, "launch");
    }
    if !ready {
        return not_launched(record, "launch");
    }
    // The home: made for a new session; the two names its binds cover hold no link and no
    // directory; then the derived sign-in, when the account has one (R14, R15).
    let mut sign = None;
    if let Some(home) = &stage_home {
        if resume.is_none() && make_stage_home(home).is_err() {
            return not_launched(record, "launch");
        }
        if !plain_names(home) {
            return not_launched(record, "launch");
        }
        match reading(s, account, renewed) {
            Reading::Absent => (),
            Reading::Unfit => return not_launched(record, "launch"),
            Reading::Derived(text) => match SignIn::create(sign_in_path(home), text, s.now()) {
                Ok(made) => sign = Some(made),
                Err(_) => return not_launched(record, "launch"),
            },
        }
    }
    let mut argv = args(&[
        "systemd-run",
        "--scope",
        "--quiet",
        "--collect",
        &format!("--unit={unit}"),
        &format!("--slice={SLICE}"),
        "-p",
        &format!("RuntimeMaxSec={}s", q.timeout + GRACE),
        "-p",
        "TimeoutStopSec=10s",
    ]);
    if let Some(b) = &q.bind {
        // The bound unit's end, for any reason, stops this one (`BindsTo` with `After`).
        argv.extend(args(&[
            "-p",
            &format!("BindsTo={b}"),
            "-p",
            &format!("After={b}"),
        ]));
    }
    let mut env = q.base_env.clone();
    let cli = argv.len();
    match q.runtime {
        Runtime::Claude => {
            claude_args(q, &mut argv, resume, &session);
            let dir = stage_home.as_deref().unwrap_or(Path::new(account));
            env.push((
                "CLAUDE_CONFIG_DIR".into(),
                dir.to_string_lossy().into_owned(),
            ));
        }
        Runtime::Codex => {
            argv.extend(codex_args(q, resume));
            env.push(("CODEX_HOME".into(), account.into()));
        }
        Runtime::Muse => {
            argv.extend(muse_args(q, &session));
            // The CLI finds its configuration directory as `$XDG_CONFIG_HOME/muse`.
            let home = Path::new(account).parent().and_then(Path::to_str);
            env.push(("XDG_CONFIG_HOME".into(), home.unwrap_or_default().into()));
            env.push(("MUSE_NO_AUTO_UPDATE".into(), "1".into()));
        }
        Runtime::Agy => {
            argv.extend(agy_args(q, resume));
            // The stage's home is the CLI's `HOME`, in place of the caller's.
            env.retain(|(k, _)| k != "HOME");
            env.push((
                "HOME".into(),
                home.as_deref()
                    .and_then(Path::to_str)
                    .unwrap_or_default()
                    .into(),
            ));
            env.push(("AGY_CLI_DISABLE_AUTO_UPDATE".into(), "1".into()));
        }
    }
    if let Some(limits @ Limits { scratch, .. }) = &q.limits {
        // After `systemd-run … --`, in front of the CLI: its own user namespace, in which it can
        // make no other, and its own pid namespace (no other stage in its `/proc`); a root built
        // from nothing but the view of R16, everything read-only but the launch's home, the
        // scratch directory and the paths the request names, no capability (R5); temporary files
        // and builds go to the scratch directory (R6).
        let mut bwrap = args(&[BWRAP, "--unshare-user", "--disable-userns", "--unshare-pid"]);
        bwrap.extend(view.words.iter().cloned());
        bwrap.extend(args(&["--dev", "/dev", "--proc", "/proc"]));
        bwrap.extend(args(&["--ro-bind", "/proc/sys", "/proc/sys"]));
        bwrap.extend(args(&[
            "--bind", account, account, "--bind", scratch, scratch,
        ]));
        bwrap.extend(limits.named());
        // Each account of the request hidden but the home of the launch (R5, R14, R15).
        if let Some(home) = &stage_home {
            bwrap.extend(home_words(&q.accounts, account, home, sign.is_some()));
        }
        // After the binds: the host's credentials and sockets, hidden (R10).
        bwrap.extend(hiding(&hidden));
        // The root of the view itself, read-only after its last mount (R5).
        bwrap.extend(args(&["--remount-ro", "/"]));
        bwrap.extend(args(&["--cap-drop", "ALL", "--chdir", &q.cwd, "--"]));
        argv.splice(cli + 1..cli + 1, bwrap);
        env.retain(|(k, _)| k != "TMPDIR" && k != "CARGO_TARGET_DIR" && k != "AP_DOOR");
        env.push(("TMPDIR".into(), format!("{scratch}/tmp")));
        env.push(("CARGO_TARGET_DIR".into(), format!("{scratch}/target")));
        // The stage's door, beside its scratch directory (R13).
        env.push(("AP_DOOR".into(), crate::door::path_of(scratch)));
        // The stage's proxy, in place of any the caller or the request names; no name passes by
        // it (R17).
        env.retain(|(k, _)| !PROXY_VARIABLES.contains(&k.as_str()));
        if let Some(proxy) = proxy {
            for name in &PROXY_VARIABLES[..4] {
                env.push(((*name).into(), proxy.url()));
            }
        }
    }
    let started = s.now();
    let namespace = proxy.and_then(egress::Proxy::namespace);
    let mut child = match launch_in(s, namespace, &argv, &env, &q.cwd, [prompt, stdout, stderr]) {
        Ok(child) => child,
        Err(_) => return not_launched(record, "launch"),
    };
    // A reviewer's or a writer's stage ends at its first result; an implementer's or a fixer's only
    // at the CLI's exit or the bound, because a result is not final while a shell it started lives.
    let mut seen = Seen {
        first_only: first_result(&q.role),
        runtime: q.runtime,
        spent: q.spent,
        model: q.model.clone(),
        ..Seen::default()
    };
    let (mut offset, mut partial, mut kept) = (0, vec![], true);
    let end = loop {
        let exited = !matches!(child.try_wait(), Ok(None));
        follow(reader, &mut offset, &mut partial, &mut seen);
        kept &= named(&q.out, &mut record, &seen);
        // Another serving model stops the stage at once, as the bound does: nothing it says counts.
        if seen.done() || seen.served.is_some() {
            break End::Result;
        }
        if exited {
            break if s.now().duration_since(started) >= bound {
                End::Timeout
            } else {
                End::Exit
            };
        }
        if s.now().duration_since(started) >= bound {
            break End::Timeout;
        }
        // The derived sign-in follows the account's, read again every 30 seconds (R15).
        if let Some(sign) = &mut sign
            && s.now().duration_since(sign.last) >= READING
        {
            if let Reading::Derived(text) = reading(s, account, renewed) {
                sign.rewrite(text);
            }
            sign.last = s.now();
        }
        s.pause(Duration::from_millis(100));
    };
    let stopped = stop(s, &unit, &mut child);
    // The launch has ended: its derived sign-in goes (R15).
    drop(sign);
    // The writer is stopped: the stream is read to its end, a last line without newline included.
    loop {
        let read = offset;
        follow(reader, &mut offset, &mut partial, &mut seen);
        if offset == read {
            break;
        }
    }
    if !partial.is_empty() {
        seen.line(&partial);
    }
    kept &= named(&q.out, &mut record, &seen);
    // The bound decides for every role: a result read during the shutdown gives the saved answer
    // and `expect_met`, never another state.
    let timed_out = matches!(end, End::Timeout);
    record["end"] = json!(stamp(SystemTime::now()));
    record["results"] = json!(seen.results.len());
    let mut write_failed = false;
    let state = match chosen(q, &seen) {
        _ if seen.served.is_some() => {
            record["unmet"] = json!("model");
            record["served_by"] = json!(seen.served);
            "failed"
        }
        None if timed_out => "timeout",
        None => {
            record["unmet"] = json!("result_event");
            "failed"
        }
        Some(e) => {
            let judged = judge(q, &mut record, &seen, e, &mut write_failed);
            record["expect_met"] = json!(judged == "ok");
            // A session that never exits keeps its last answer, but its state is the bound's.
            if timed_out { "timeout" } else { judged }
        }
    };
    record["state"] = json!(state);
    match q.runtime {
        Runtime::Muse => (record["turns"], record["tokens"]) = muse_tally(seen.usage.as_ref()),
        Runtime::Claude | Runtime::Codex | Runtime::Agy => (),
    }
    if !stopped {
        // Something of the stage may still live: never `ok`.
        record["state"] = json!("failed");
        record["unmet"] = json!("stop");
    }
    record["limit"] = match q.runtime {
        Runtime::Claude | Runtime::Codex => limit_of(seen.limit.as_ref()),
        Runtime::Muse => muse_limit(seen.usage.as_ref()),
        Runtime::Agy => Value::Null,
    };
    mirror(&mut record, seen.worked);
    let saved = persist(&q.out, &record);
    (record, seen.worked, saved && kept && !write_failed)
}
/// The `run` object of a stage that names a `unit` or an `intent` (`specs/ap/run-object.md`).
#[derive(Default)]
struct Object {
    target: Option<(&'static str, String)>,
    key: Option<String>,
    start: Option<String>,
    close: Option<String>,
    /// The id and the parameters of the first `start_run`, kept for its repeat.
    request: String,
    parameters: Value,
    /// The start gave no answer, so it may have been applied.
    unanswered: bool,
    /// The session known before the first launch.
    session: Option<String>,
}
impl Object {
    fn member(&self) -> Value {
        match self.target {
            None => Value::Null,
            Some(_) => json!({"key": self.key, "start": self.start, "close": self.close}),
        }
    }
    fn reference(&self) -> Value {
        let (kind, key) = self.target.as_ref().map_or(("", ""), |(k, v)| (*k, v));
        json!({ kind: crate::host::id(kind, key) })
    }
    /// The start: after admission and the claim, before the first launch. `account` is the first
    /// launch's; the paths and the session are as the stage will hold them.
    fn begin(&mut self, s: &mut impl System, q: &Request, account: &str) {
        self.session = match (&q.resume, q.runtime) {
            (Some((session, _, _)), _) => Some(session.clone()),
            (None, Runtime::Claude | Runtime::Muse) => new_session_id(s).ok(),
            (None, Runtime::Codex | Runtime::Agy) => None,
        };
        let fits = |p: String| {
            (p.len() <= 400 && p.bytes().all(|c| (b'!'..=b'~').contains(&c))).then_some(p)
        };
        let component = Path::new(account).file_name().and_then(|n| n.to_str());
        let mut p = self.reference();
        p["role"] = json!(q.actor_role);
        p["runtime"] = json!(q.runtime.word());
        p["model"] = json!(q.model);
        p["effort"] = json!(q.effort);
        p["account"] = json!(component.filter(|n| n.len() <= 200));
        p["host"] = crate::host::id("host", "local");
        p["uses"] = component
            .filter(|n| n.len() <= 200)
            .map_or(Value::Null, |n| crate::host::id("account", n));
        p["session"] = json!(self.session);
        p["record"] = json!(fits(record_path(&q.out)));
        p["stream"] = json!(fits(format!("{}.stream.jsonl", q.out)));
        p.as_object_mut()
            .into_iter()
            .for_each(|o| o.retain(|_, v| !v.is_null()));
        self.request = format!("run-{}", &digest(q.out.as_bytes())[..32]);
        self.parameters = p;
        self.apply(s, q, true);
    }
    /// One `start_run` with the kept id and parameters (the first call, or its repeat after the end).
    fn apply(&mut self, s: &mut impl System, q: &Request, first: bool) {
        let call = (format!("{}-start", self.request), &self.parameters);
        let done = store::start_run(s, q.store.as_deref(), (&call.0, &q.actor_role, call.1));
        match done {
            Ok(a) => {
                self.key = a
                    .made("run")
                    .and_then(|i| i["native_key"].as_str())
                    .map(str::to_owned);
                let late = !first && a.outcome == "applied";
                self.start = Some(match (self.key.is_some(), late) {
                    (false, _) => "store_answer_unreadable".to_owned(),
                    (true, true) => "applied_late".to_owned(),
                    (true, false) => a.outcome,
                });
            }
            Err(f) if first => {
                self.unanswered = f.code != store::REFUSED;
                self.start = Some(store::code_of(&f));
            }
            Err(_) => (),
        }
    }
    /// After the stage ended: the repeat of a start that gave no answer, then the close.
    fn finish(&mut self, s: &mut impl System, q: &Request, last: &Value) {
        if self.target.is_none() {
            return;
        }
        if self.unanswered {
            self.apply(s, q, false);
        }
        let Some(key) = &self.key else {
            return;
        };
        let text =
            |k: &str, max: usize| last[k].as_str().filter(|v| !v.is_empty() && v.len() <= max);
        let mut p = json!({"run": crate::host::id("run", key), "state": last["state"],
            "session": text("session", 200), "served_by": text("served_by", 80),
            "limit": limit_text_of(&last["limit"])});
        for (to, from) in [
            ("input_tokens", "input"),
            ("cache_read_tokens", "cache_read"),
            ("cache_write_tokens", "cache_creation"),
            ("output_tokens", "output"),
        ] {
            p[to] = last["tokens"][from].clone();
        }
        p.as_object_mut()
            .into_iter()
            .for_each(|o| o.retain(|_, v| !v.is_null()));
        let id = format!("{}-close", self.request);
        let done = store::close_run(s, q.store.as_deref(), (&id, &q.actor_role, &p));
        self.close = Some(done.map_or_else(|f| store::code_of(&f), |a| a.outcome));
    }
}
/// The record's limit reading as the `limit` of `close_run`: compact JSON of at most 300 bytes, else
/// its window of the highest utilization, else nothing.
fn limit_text_of(limit: &Value) -> Option<String> {
    let fits = |t: String| (t.len() <= 300).then_some(t);
    let whole = serde_json::to_string(limit)
        .ok()
        .filter(|_| limit.is_object());
    whole.clone().and_then(fits).or_else(|| {
        let windows = limit["windows"].as_object()?;
        let (name, w) = windows
            .iter()
            .filter_map(|(n, w)| Some((n, w, w["utilization"].as_f64()?)))
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(n, w, _)| (n, w))?;
        let one = json!({"window": name, "utilization": w["utilization"], "status": w["status"],
            "resets_at": w["resets_at"]});
        fits(one.to_string())
    })
}
/// The record after an ask (R28): `turns` and `tokens` are the sums of the asked launch and its ask
/// (a member null in both stays null, else a null counts 0), and an ask that wrote no answer keeps
/// the asked launch's `answer`, which is what `out` still holds.
fn asked_after(record: &mut Value, asked: &Value) {
    let add = |a: &Value, b: &Value| match (a.as_u64(), b.as_u64()) {
        (None, None) => Value::Null,
        (a, b) => json!(a.unwrap_or(0) + b.unwrap_or(0)),
    };
    record["turns"] = add(&record["turns"], &asked["turns"]);
    for k in TOKENS {
        record["tokens"][k] = add(&record["tokens"][k], &asked["tokens"][k]);
    }
    if record["answer"].is_null() {
        record["answer"] = asked["answer"].clone();
    }
}
/// The variables that name the stage's proxy (the first four) and those it holds none of
/// (`specs/ap/run-limits.md` R17).
const PROXY_VARIABLES: [&str; 8] = [
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
    "ALL_PROXY",
    "all_proxy",
];
/// The port's launch, from the stage's network namespace when the call has one
/// (`specs/ap/run-limits.md` R17): the calling thread enters it, launches, and returns to the
/// namespace it was in before its next step. A thread that cannot return ends `ap run` at once, so
/// no later step of it runs in the stage's namespace.
fn launch_in(
    s: &mut impl System,
    namespace: Option<std::os::fd::BorrowedFd<'_>>,
    argv: &[String],
    env: &[(String, String)],
    cwd: &str,
    files: [File; 3],
) -> std::io::Result<Child> {
    use rustix::thread::{LinkNameSpaceType, move_into_link_name_space};
    use std::os::fd::AsFd;
    let Some(namespace) = namespace else {
        return s.launch(argv, env, cwd, files);
    };
    let own = File::open("/proc/thread-self/ns/net")?;
    move_into_link_name_space(namespace, Some(LinkNameSpaceType::Network))?;
    let child = s.launch(argv, env, cwd, files);
    if move_into_link_name_space(own.as_fd(), Some(LinkNameSpaceType::Network)).is_err() {
        std::process::abort();
    }
    child
}
/// What the door of a stage knows of it (`specs/ap/door.md`): the role and agent the engine gets
/// from the stage's environment, its record, its `cwd` and its `store`.
fn door_stage(q: &Request) -> crate::door::Stage {
    let of = |name: &str| {
        let found = q.base_env.iter().rev().find(|(k, _)| k == name);
        found.map(|(_, v)| v.clone()).unwrap_or_default()
    };
    crate::door::Stage {
        role: q.actor_role.clone(),
        agent: of("ONT_AGENT"),
        run: record_path(&q.out),
        cwd: q.cwd.clone(),
        store: q.store.clone(),
    }
}
/// One `ap run` request. A refusal (exit 2) comes before any effect and writes no file.
pub fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let q = request(s, r)?;
    // A unit or an intent the store does not have refuses the call, before any effect.
    if let Some((kind, key)) = &q.target
        && store::absent(s, q.store.as_deref(), kind, key)
    {
        return Err(error(
            if *kind == "unit" {
                "unknown_unit"
            } else {
                "unknown_intent"
            },
            kind,
        ));
    }
    // The claim: the record is created exclusively, so of two requests on one `out` started at
    // the same instant only one passes. Every effect comes after it.
    let create = |p: &str| {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .mode(OWNER_ONLY)
            .open(p)
    };
    create(&record_path(&q.out)).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => error("out_exists", "out"),
        _ => error("out_unwritable", "out"),
    })?;
    let errors = format!("{}.err", q.out);
    let files = (create(&format!("{}.stream.jsonl", q.out)), create(&errors));
    // A resumed call tries its record's account first, then the request's, each once.
    let accounts = match &q.resume {
        Some((_, account, _)) => {
            let mut all = vec![account.clone()];
            for a in &q.accounts {
                if !all.contains(a) {
                    all.push(a.clone());
                }
            }
            all
        }
        None => q.accounts.clone(),
    };
    let (mut last, mut clean, mut worked_before) = (Value::Null, true, false);
    let mut object = Object {
        target: q.target.clone(),
        ..Object::default()
    };
    if let (Ok(output), Ok(errs)) = files {
        let files = (output, errs);
        if q.target.is_some() {
            object.begin(s, &q, &accounts[0]);
        }
        // The scratch directory: made once, after the claim and before the first launch (R4).
        let scratch = q.limits.as_ref().map(|l| l.scratch.as_str());
        let hooks = host_hooks(s, &q);
        let (made, ready) = scratch.map_or((false, true), |d| make_scratch(d, hooks.is_some()));
        // Only the call whose `mkdir` made it removes it, after its last launch (R8): the guard is
        // dropped where the removal runs, and on a panic that unwinds out of this block.
        let removal = scratch
            .filter(|_| made)
            .map(|scratch| std::mem::DropGuard::new(scratch, remove_scratch));
        // The door: opened after the scratch directory and before the first launch; one that cannot
        // be opened is a scratch directory that cannot be made (R13, `specs/ap/door.md` R1).
        let mut door = None;
        let ready = match scratch {
            Some(scratch) if ready => {
                let opened = crate::door::open(s, door_stage(&q), &crate::door::path_of(scratch));
                door = opened.ok();
                door.is_some()
            }
            _ => ready,
        };
        // The stage's network and its proxy: after the door and before the first launch; one that
        // cannot be made is a scratch directory that cannot be made (`specs/ap/run-limits.md` R17).
        let mut proxy = None;
        let ready = match scratch {
            Some(_) if ready => {
                proxy = egress::open(s, record_path(&q.out), &q.actor_role).ok();
                proxy.is_some()
            }
            _ => ready,
        };
        // When the last launch began: it and its ask share one `timeout_seconds` (R20).
        let mut began = s.now();
        // The calls of `claude auth status` of this call, by account (`specs/ap/run-limits.md` R15).
        let mut renewed = Renewed::default();
        for (i, account) in accounts.iter().enumerate() {
            // Once a launch of an implementer or a fixer has worked, every later one gets the text;
            // a launch of a resumed call after the first is a move and gets its own.
            let lead = match &q.moved {
                Some(text) if i > 0 => Some(text.as_str()),
                _ if q.resume.is_none() && worked_before && !first_result(&q.role) => {
                    Some(CONTINUATION)
                }
                _ => None,
            };
            let earlier = last["attempts"].as_array().cloned().unwrap_or_default();
            // Only the first launch of a resumed call resumes the record's session.
            let resume = q.resume.as_ref().filter(|_| i == 0);
            let launch = Launch {
                earlier: &earlier,
                lead,
                resume: resume.map(|(session, _, _)| session.as_str()),
                ask: false,
                ready,
                proxy: proxy.as_ref(),
                hooks: hooks.as_deref(),
                bound: Duration::from_secs(q.timeout),
            };
            began = s.now();
            let (record, worked, saved) =
                attempt(s, &q, account, &files, &launch, &object, &mut renewed);
            clean &= saved;
            worked_before |= worked;
            // A run cut at any point goes on at the next account: before any tool ran, and after
            // (a reviewer changes no file; an implementer or a fixer is told to read the state its
            // predecessor left). A resumed call goes on only when it can tell the new session
            // which prompt and answer the cut one had.
            let movable = q.resume.is_none() || q.moved.is_some();
            let next = record["state"] == "cut" && movable && i + 1 < accounts.len();
            last = record;
            if !next || !saved {
                break;
            }
        }
        // A Claude stage whose answer's first line missed `expect.first_line`, or whose second line
        // missed `expect.second_line`, is asked once more, in its own session on its own account,
        // within what is left of the bound of the launch that answered; whatever the ask ends in
        // ends the call (R20).
        let left = Duration::from_secs(q.timeout).saturating_sub(s.now().duration_since(began));
        let lead = match last["unmet"].as_str() {
            Some("first_line") => Some(ASK),
            Some("second_line") => Some(ASK_SECOND),
            _ => None,
        };
        let asked = q.runtime == Runtime::Claude
            && clean
            && last["state"] == "no_result"
            && lead.is_some()
            && !left.is_zero();
        if let (true, Some(session), Some(account)) =
            (asked, last["session"].as_str(), last["account"].as_str())
        {
            let earlier = last["attempts"].as_array().cloned().unwrap_or_default();
            let launch = Launch {
                earlier: &earlier,
                lead,
                resume: Some(session),
                ask: true,
                ready,
                proxy: proxy.as_ref(),
                hooks: hooks.as_deref(),
                bound: left,
            };
            let (mut record, _, saved) =
                attempt(s, &q, account, &files, &launch, &object, &mut renewed);
            asked_after(&mut record, &last);
            clean &= saved && persist(&q.out, &record);
            last = record;
        }
        // The proxy is closed after the last launch, before the scratch directory goes (R20).
        if let Some(proxy) = proxy {
            proxy.close();
        }
        // The door is closed after the last launch, before the scratch directory goes (R13).
        if let Some(door) = door {
            door.close();
        }
        drop(removal);
    } else {
        last = json!({"state": "failed", "unmet": "launch", "account": accounts[0]});
        let _ = persist(&q.out, &last);
        clean = false;
    }
    if object.start.is_some() {
        // Whatever the store answers, the stage's state, exit code and answer stand: only the record's member changes.
        object.finish(s, &q, &last);
        last["run_object"] = object.member();
        let _ = persist(&q.out, &last);
    }
    if std::fs::metadata(&errors).is_ok_and(|m| m.len() == 0) {
        let _ = std::fs::remove_file(&errors);
    }
    let status = match last["state"].as_str() {
        _ if !clean => WRITE_FAILED,
        Some("ok") => 0,
        Some("cut") => 75,
        _ => 3,
    };
    Ok((last, status))
}
/// A refusal of the close form, or a failure of its store call, with its exit (2 or 3).
#[derive(Debug)]
pub struct Refused {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub status: u8,
}
impl From<Failure> for Refused {
    fn from(f: Failure) -> Self {
        Self {
            code: f.code.to_owned(),
            field: f.field,
            detail: f.detail,
            status: 2,
        }
    }
}
/// The members of the close form (`specs/ap/run-object.md` R14).
const CLOSING: [&str; 2] = ["close", "store"];
/// The youngest record of a stage that an orphan's close takes, and the pause between its two
/// readings: a stage's record is younger, or changes, while its stage lives (R14).
const SETTLED: Duration = Duration::from_secs(60);
const SECOND_READING: Duration = Duration::from_secs(10);
/// `ap run` with the member `close`: a `run` object left `running` by a stage whose process is gone
/// closed with its record's state (R14). A store call that fails ends the call with exit 3.
pub fn close(s: &mut impl System, r: &Value) -> Result<Value, Refused> {
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !CLOSING.contains(&k.as_str())) {
        return Err(error("invalid_input", k).into());
    }
    let key = r["close"]
        .as_str()
        .filter(|k| {
            (1..=64).contains(&k.len())
                && k.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
        .ok_or_else(|| error("invalid_input", "close"))?;
    let store = match r.get("store") {
        Some(_) => Some(path(r, "store")?),
        None => None,
    };
    let failed = |f: Failure| Refused {
        code: store::code_of(&f),
        field: "close".into(),
        detail: f.detail,
        status: 3,
    };
    let run = store::get(s, store.as_deref(), "run", key).map_err(failed)?;
    let run = run.ok_or_else(|| error("unknown_run", "close"))?;
    if run["status"] != "running" {
        return Ok(json!({"run": key, "closed": false}));
    }
    let mismatch = |why: &str| fail("record_mismatch", "close", json!(why));
    let record = run["record"]
        .as_str()
        .ok_or_else(|| mismatch("no record"))?;
    let rec = std::fs::read(record)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(Value::is_object)
        .ok_or_else(|| mismatch("the record is missing or unreadable"))?;
    if rec["run_object"]["key"] != key {
        return Err(mismatch("the record names another run object").into());
    }
    let scope = rec["unit"]
        .as_str()
        .filter(|n| scope_name(n))
        .ok_or_else(|| mismatch("the record names no scope of ap run"))?;
    let still = |why: &str| fail("run_still_active", "close", json!(why));
    // The scope reads ended, and the record is old and does not change across a second reading.
    let first = settled(s, record, scope)?;
    let age = s.wall().duration_since(first.0).unwrap_or_default();
    if age < SETTLED {
        return Err(still("the record is younger than 60 seconds").into());
    }
    s.pause(SECOND_READING);
    if settled(s, record, scope)? != first {
        return Err(still("the record changed").into());
    }
    let state = match rec["state"].as_str() {
        Some("running") => "killed",
        other => other.unwrap_or("failed"),
    };
    let mut p = json!({"run": crate::host::id("run", key), "state": state});
    if let Some(t) = rec["session"]
        .as_str()
        .filter(|t| (1..=200).contains(&t.len()))
    {
        p["session"] = json!(t);
    }
    let out = record.strip_suffix(".run.json").unwrap_or(record);
    // The stage's own request id: the close its `ap run` would have applied (unit-run R7).
    let rid = format!("run-{}-close", &digest(out.as_bytes())[..32]);
    let role = run["role"].as_str().unwrap_or_default();
    store::close_run(s, store.as_deref(), (&rid, role, &p)).map_err(failed)?;
    Ok(json!({"run": key, "closed": true, "reason": "orphan"}))
}
/// One reading of an orphan: the record's modification time and size, when the scope reads ended.
fn settled(s: &mut impl System, record: &str, scope: &str) -> Result<(SystemTime, u64), Failure> {
    let still = |why: &str| fail("run_still_active", "close", json!(why));
    let at = std::fs::metadata(record).map_err(|_| still("the record cannot be read again"))?;
    let modified = at.modified().map_err(|_| still("the record has no time"))?;
    if !ended(s, scope) {
        return Err(still(scope));
    }
    Ok((modified, at.len()))
}
/// The name of a stage's transient scope: `ap-run-<36 of 0-9a-f->.scope`.
fn scope_name(name: &str) -> bool {
    let id = name
        .strip_prefix("ap-run-")
        .and_then(|r| r.strip_suffix(".scope"));
    let digit = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || c == b'-';
    id.is_some_and(|id| id.len() == 36 && id.bytes().all(digit))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    // The close form has a path of its own: a failed store call exits 3 (R14).
    answered(argv, |s, r| match r.get("close") {
        Some(_) => close(s, r).map(|v| (v, 0)),
        None => execute(s, r).map_err(Refused::from),
    })
}
/// One JSON request on stdin, the answer on stdout, a refusal on stderr with exit 2.
pub(crate) fn cli_with(
    argv: &[OsString],
    execute: impl FnOnce(&mut crate::host::Host, &Value) -> Result<(Value, u8), Failure>,
) -> ExitCode {
    answered(argv, |s, r| execute(s, r).map_err(Refused::from))
}
/// One JSON request on stdin, the answer on stdout, a refusal on stderr with its exit.
fn answered(
    argv: &[OsString],
    execute: impl FnOnce(&mut crate::host::Host, &Value) -> Result<(Value, u8), Refused>,
) -> ExitCode {
    let result = (|| {
        require!(argv.is_empty() => "usage", "args");
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail("io_error", "stdin", Value::Null))?;
        require!(bytes.len() as u64 <= LIMIT => "input_too_large", "stdin");
        let r: Value = serde_json::from_slice(&bytes)
            .map_err(|_| fail("invalid_input", "stdin", Value::Null))?;
        Ok(execute(&mut crate::host::Host, &r))
    })()
    .map_err(Refused::from)
    .and_then(|answer| answer);
    match result {
        Ok((v, status)) => {
            if writeln!(std::io::stdout(), "{v}").is_err() {
                return ExitCode::from(4);
            }
            ExitCode::from(status)
        }
        Err(e) => {
            let v = json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}});
            if writeln!(std::io::stderr(), "{v}").is_err() {
                return ExitCode::from(4);
            }
            ExitCode::from(e.status)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MOVED, MOVED_WORK, glob, limit_text, stamp};
    use std::time::{Duration, UNIX_EPOCH};
    #[test]
    fn stamps_are_utc_rfc3339() {
        assert_eq!(stamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_794_648_600);
        assert_eq!(stamp(t), "2026-11-14T09:30:00Z");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(stamp(leap), "2000-02-29T00:00:00Z");
    }
    #[test]
    fn first_line_patterns() {
        assert!(glob("Verdict: *", "Verdict: accept"));
        assert!(glob("Verdict: accept", "Verdict: accept"));
        assert!(!glob("Verdict: accept", "Verdict: accepted"));
        assert!(glob("*done", "all done"));
        assert!(glob("a*b*c", "aXbYc"));
        assert!(!glob("a*b*c", "aXbY"));
        assert!(!glob("Verdict: *", "verdict: accept"));
        assert!(glob("*", ""));
    }
    #[test]
    fn the_moved_text_holds_each_placeholder_once_and_in_order() {
        // `split_once` on the constant turns a move off silently when a placeholder is lost.
        assert_eq!(
            (MOVED.matches("{P}").count(), MOVED.matches("{A}").count()),
            (1, 1)
        );
        assert!(MOVED.find("{P}") < MOVED.find("{A}"));
        assert!(!MOVED_WORK.contains('{'));
    }
    #[test]
    fn usage_limit_texts() {
        assert!(limit_text(
            "You have hit your 5-hour limit · resets 6:30pm (UTC)"
        ));
        assert!(!limit_text("API Error: 500 internal"));
    }
}
