#!/usr/bin/env bash
set -euo pipefail

# Finite hosted CI bootstrap only. Native trusted workers already own their /job.
# Never reuse or remove a previous /job, including after an ambiguous failure.
/usr/bin/python3 -I -S -B - <<'PY'
from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys

BUN_SHA256 = "7ce7d6b654eddeec20afddf2396317fba6dc19ab52788a27e894e1806bf8f24b"
uid, gid = os.getuid(), os.getgid()
job = Path("/job")
clean_env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8",
             "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
             "GIT_TERMINAL_PROMPT": "0", "GIT_ATTR_NOSYSTEM": "1"}


class Refused(Exception):
    pass


def need(condition, code):
    if not condition:
        raise Refused(code)


def command(argv, cwd):
    p = subprocess.run(argv, cwd=cwd, env=clean_env, stdin=subprocess.DEVNULL,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    need(p.returncode == 0, "FIXED_COMMAND_FAILED")
    need(len(p.stdout) <= 65536 and len(p.stderr) <= 65536, "COMMAND_METADATA_BUDGET")
    return p.stdout.decode("utf-8").strip()


def git(root, *args):
    return command(["/usr/bin/git", "-c", "core.hooksPath=/dev/null",
                    "-c", "core.fsmonitor=false", "-c", "protocol.allow=never",
                    "-c", "protocol.file.allow=always", "-C", str(root), *args], root)


def owned_directory(path):
    s = path.lstat()
    need(stat.S_ISDIR(s.st_mode) and s.st_uid == uid and s.st_gid == gid
         and stat.S_IMODE(s.st_mode) == 0o700 and path.resolve(strict=True) == path,
         "EXACT_OWNED_JOB_DIRECTORY_REQUIRED")


def identity(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink, info.st_uid,
            info.st_gid, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def regular_bytes(path, maximum, mode=None):
    before = path.lstat()
    need(stat.S_ISREG(before.st_mode) and before.st_uid == uid and before.st_nlink == 1
         and before.st_size <= maximum and not before.st_mode & 0o022
         and path.resolve(strict=True) == path, "EXACT_OWNED_REGULAR_INPUT_REQUIRED")
    if mode is not None:
        need(stat.S_IMODE(before.st_mode) == mode, "EXACT_FILE_MODE_REQUIRED")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as f:
        opened = os.fstat(f.fileno())
        need(identity(opened) == identity(before), "OPENED_INPUT_CHANGED")
        data = f.read(maximum + 1)
        need(identity(os.fstat(f.fileno())) == identity(before), "READ_INPUT_CHANGED")
    need(identity(path.lstat()) == identity(before) and len(data) == before.st_size, "INPUT_CHANGED")
    return data


def bun_bytes(path, mode=None):
    data = regular_bytes(path, 100_000_000, mode)
    need(hashlib.sha256(data).hexdigest() == BUN_SHA256, "EXACT_BUN_BYTES_REQUIRED")
    return data


def receipt(phase, **extra):
    value = {"schema": "agent-platform.source-ci-bootstrap.v1", "phase": phase,
             "job": os.environ.get("GITHUB_JOB"), "retryAllowed": False,
             "nativePermissionGranted": False, "acceptanceDecision": None, **extra}
    print(json.dumps(value, separators=(",", ":")), flush=True)
    if phase == "ready":
        with (job / "tmp/source-ci-bootstrap.json").open("x") as f:
            os.chmod(f.name, 0o600)
            f.write(json.dumps(value, indent=2) + "\n")


try:
    need(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("CI") == "true"
         and os.environ.get("RUNNER_OS") == "Linux" and os.environ.get("RUNNER_ARCH") == "X64"
         and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
         and sys.platform == "linux" and os.uname().machine == "x86_64"
         and uid == os.geteuid() and gid == os.getegid() and uid > 0 and gid > 0,
         "EXPLICIT_HOSTED_LINUX_X64_RUNNER_REQUIRED")
    need(os.environ.get("GITHUB_JOB") in ("source", "quality", "merge-integration")
         and os.environ.get("GITHUB_WORKFLOW") == "Source verification"
         and os.environ.get("GITHUB_REPOSITORY") == "park-kyungchan/agent-platform",
         "FINITE_SOURCE_CI_JOB_REQUIRED")
    need(os.environ.get("GITHUB_EVENT_NAME") in ("pull_request", "push", "workflow_dispatch")
         and (os.environ["GITHUB_JOB"] != "merge-integration"
              or os.environ["GITHUB_EVENT_NAME"] == "pull_request"), "SOURCE_EVENT_REQUIRED")
    for name in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"):
        need(re.fullmatch(r"[1-9][0-9]{0,19}", os.environ.get(name, "")), "ACTUAL_JOB_METADATA_REQUIRED")
    need(not any(name in ("NODE_OPTIONS", "BUN_OPTIONS", "BUN_CONFIG")
                 or name.startswith("BUN_CONFIG_") for name in os.environ), "INJECTOR_ENV_REFUSED")
    workspace = Path(os.environ["GITHUB_WORKSPACE"])
    need(workspace.is_absolute() and workspace.resolve(strict=True) == workspace
         and workspace == Path.cwd() and workspace.lstat().st_uid == uid,
         "EXACT_ORIGINAL_CHECKOUT_REQUIRED")
    need(git(workspace, "rev-parse", "--show-toplevel") == str(workspace)
         and git(workspace, "rev-parse", "--absolute-git-dir") == str(workspace / ".git")
         and (workspace / ".git").resolve(strict=True) == workspace / ".git"
         and git(workspace, "rev-parse", "--is-shallow-repository") == "false"
         and git(workspace, "status", "--porcelain=v1", "--untracked-files=all") == "",
         "CLEAN_FULL_HISTORY_CHECKOUT_REQUIRED")
    head = git(workspace, "rev-parse", "HEAD")
    tree = git(workspace, "rev-parse", "HEAD^{tree}")
    need(re.fullmatch(r"[a-f0-9]{40}", head) and re.fullmatch(r"[a-f0-9]{40}", tree)
         and head == os.environ.get("COMMON_CI_SOURCE_SHA"), "ACTUAL_AUTHORED_OR_MERGE_HEAD_REQUIRED")
    parents = git(workspace, "rev-list", "--parents", "-n", "1", head)
    history = git(workspace, "rev-list", "--count", head)
    # A supported runner command file is the only PATH mutation; retain every old byte.
    temporary = Path(os.environ["RUNNER_TEMP"])
    path_file = Path(os.environ["GITHUB_PATH"])
    need(temporary.is_absolute() and temporary.resolve(strict=True) == temporary
         and path_file.is_absolute() and path_file.parent.name == "_runner_file_commands"
         and path_file.parent.parent == temporary
         and re.fullmatch(r"add_path_[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}", path_file.name),
         "ACTUAL_RUNNER_PATH_COMMAND_FILE_REQUIRED")
    old_path = regular_bytes(path_file, 65536)
    addition = (b"" if not old_path or old_path.endswith(b"\n") else b"\n") + b"/job/bin\n"
    need(len(old_path) + len(addition) <= 65536, "COMMAND_FILE_BUDGET")
    installed = Path(shutil.which("bun") or "")
    need(installed.is_absolute(), "SETUP_BUN_REQUIRED")
    binary = bun_bytes(installed)
    need(command([str(installed), "--no-env-file", "--no-install", "--version"], "/") == "1.4.3",
         "EXACT_INSTALLED_BUN_VERSION_REQUIRED")
    need(bun_bytes(installed) == binary, "INSTALLED_BUN_CHANGED")
    need(not os.path.lexists(job), "EXISTING_OR_PARTIAL_JOB_REFUSED_DO_NOT_RETRY")
    receipt("intent", sourceHead=head, sourceTree=tree, historyCommits=int(history),
            githubRunId=os.environ["GITHUB_RUN_ID"], githubRunAttempt=os.environ["GITHUB_RUN_ATTEMPT"])
    os.umask(0o077)
    # mkdir is exclusive; install -d could silently reuse or chmod someone else's /job.
    command(["/usr/bin/sudo", "-n", "/usr/bin/mkdir", "--mode=0700", "--", "/job"], workspace)
    command(["/usr/bin/sudo", "-n", "/usr/bin/chown", "--no-dereference", "--",
             str(uid) + ":" + str(gid), "/job"], workspace)
    owned_directory(job)
    for name in ("home", "tmp", "bin", "bun-cache", "transpiler-cache"):
        (job / name).mkdir(mode=0o700)
        owned_directory(job / name)
    staged_bun = job / "bin/bun"
    fd = os.open(staged_bun, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o700)
    with os.fdopen(fd, "wb") as f:
        f.write(binary)
        f.flush()
        os.fsync(f.fileno())
    need(bun_bytes(staged_bun, 0o700) == binary, "STAGED_BUN_CHANGED")
    need(command([str(staged_bun), "--no-env-file", "--no-install", "--version"], job) == "1.4.3",
         "EXACT_STAGED_BUN_VERSION_REQUIRED")
    need(bun_bytes(staged_bun, 0o700) == binary, "STAGED_BUN_CHANGED")
    source = job / "source"
    git(workspace, "clone", "--no-hardlinks", "--no-checkout", "--template=", "--",
        str(workspace), str(source))
    owned_directory(source)
    git(source, "checkout", "--detach", head)
    need(git(source, "rev-parse", "HEAD") == head and git(source, "rev-parse", "HEAD^{tree}") == tree
         and git(source, "rev-parse", "--is-shallow-repository") == "false"
         and git(source, "rev-list", "--parents", "-n", "1", head) == parents
         and git(source, "rev-list", "--count", head) == history
         and git(source, "status", "--porcelain=v1", "--untracked-files=all") == "",
         "EXACT_COMPLETE_STAGED_SOURCE_REQUIRED")
    need(git(workspace, "rev-parse", "HEAD") == head
         and git(workspace, "rev-parse", "HEAD^{tree}") == tree
         and git(workspace, "status", "--porcelain=v1", "--untracked-files=all") == "",
         "ORIGINAL_SOURCE_CHANGED")
    need(bun_bytes(installed) == binary and bun_bytes(staged_bun, 0o700) == binary,
         "BUN_FINAL_READBACK_CHANGED")
    need(regular_bytes(path_file, 65536) == old_path, "COMMAND_FILE_CHANGED")
    fd = os.open(path_file, os.O_RDWR | os.O_APPEND | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "r+b") as f:
        info = os.fstat(f.fileno())
        need(info == path_file.lstat() and info.st_uid == uid and info.st_nlink == 1
             and stat.S_ISREG(info.st_mode) and not info.st_mode & 0o022
             and f.read(65537) == old_path, "OPENED_COMMAND_FILE_CHANGED")
        f.write(addition)
        f.flush()
        os.fsync(f.fileno())
    need(regular_bytes(path_file, 65536) == old_path + addition, "PATH_APPEND_READBACK_FAILED")
    receipt("ready", sourceHead=head, sourceTree=tree, historyCommits=int(history),
            sourceRoot=str(source), bunPath=str(staged_bun), bunSha256=BUN_SHA256,
            originalSourceUnchanged=True, originalCommandFilePreserved=True)
except (Refused, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
    receipt("held", failureCode=str(error) if isinstance(error, Refused) else type(error).__name__)
    raise SystemExit(1)
PY
