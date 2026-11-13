# Building, checking and delivering a change

Read this when you build or test Rust on this host, push a branch, open or update a pull request, merge, install a merged change, or change a shared instruction. A command below shows the mechanism. Whether you may push, merge or install comes from your packet and the task's STATE.

## 1. The stages of one change

Each stage is a separate fact with its own evidence. A later stage does not show an earlier one.

| Stage | Where it happens | Evidence |
| --- | --- | --- |
| Edit | A task worktree `work/<scope>-<topic>/wt/<repo>/`, on a branch from `origin/main` (`NAMING.md` section 2) | `git status` and the diff against the base revision |
| Check on the host | The checks the packet names, under the build lock (section 2) | The observed output of each check |
| Push | The branch on GitHub; a pull request that stays a draft while its packets are still arriving | `gh pr view <pr> --json headRefOid,isDraft` |
| Check in CI | The job `rust` on the pushed head (section 3) | `gh pr checks <pr> --required` |
| Review | One independent review of one frozen head (skill `lead` section 5) | A review file that names that head |
| Merge | `origin/main` | The merge commit; section 4 |
| Install | The host | The readback of section 5 |
| Load | A session started after the installation | That session's own record (`CLI-RUNTIMES.md` section 3) |

## 2. Building on the host

One Cargo build or test runs on the host at a time. Every Cargo command of every task takes the same lock:

```sh
flock /home/palantirkc/work/ws-replan/wt/.host-cargo.lock env \
  CARGO_TARGET_DIR="<target root>" CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 \
  CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo <arguments>
```

| If | Then |
| --- | --- |
| The packet or STATE names a target root | Use exactly that `CARGO_TARGET_DIR`. A target root belongs to one repository; it is outside the worktree and is never a main checkout's `target` |
| No target root is named | Stop and ask the Lead. A new target root costs gigabytes |
| You hold the lock | Read free space (`df -BM --output=avail /`) before the build and stop under the floor the task's STATE states |
| Another build holds the lock | Wait. Never delete the lock file, a target root or a Cargo cache to get past it |
| You run `scripts/install-ap.sh` or `scripts/install-ont.sh` | Do not wrap it in `flock`: the script takes the lock itself |
| A test needs a private directory | Use the root the packet names |
| A workspace test run left a synthetic root behind | Remove it when the part is accepted, after checking that no process has a file open in it |

- Each repository pins its toolchain in `rust-toolchain.toml`; do not install or select another one.
- Run the checks the packet names; without such a line, the checks your change affects. A check the packet leaves to CI is read on the pushed head, not rerun on the host.
- Before a push run what CI runs first and fails on most often: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

## 3. What CI runs

Every platform repository has one workflow, `.github/workflows/rust.yml`, with one job, `rust`, on every pull request and on every push to `main`. `rust` is the required check of `main`.

| Repository | Steps of the job `rust` |
| --- | --- |
| All | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --locked`. A change that touches only documentation skips the Rust steps inside the job, and the job still reports |
| `agent-platform` | Also `python3 -I -B ci/check-content.py` (always), `bash scripts/test-install-ap.sh` and the memory record checks |
| `ontology-platform` | Also the test of its install script |
| `education-platform` | Also the offline teacher harness and `cargo build --workspace --bins --examples --locked`. The job does not run on a draft pull request: it runs when the pull request is made ready |

- CI minutes are limited per month. Push when a packet is complete, not after every commit.
- A workflow file, a required check and a ruleset are shared files with one integrator (`CONCURRENT-WORK.md` section 1). Do not change one to get a change through.
- When the job fails, read its log (`gh run view <run> --log-failed`), fix the cause in the worktree and push a new commit. Do not rerun an unchanged failing job.

## 4. Pull request, review and merge

| Rule | Command or detail |
| --- | --- |
| Never force-push and never rewrite a pushed branch | A repair is a new commit |
| A review belongs to one head | The reviewer is given the commit; a push after the review needs that commit reviewed again or the difference read and accepted by the Lead |
| The merged head is the reviewed head | `gh pr merge <pr> --repo <owner/repo> --merge --match-head-commit <reviewed head>`; before it, print the review's head and `gh pr view <pr> --json headRefOid` side by side |
| The required check ran on that head | `gh pr checks <pr> --required` |
| Review threads on GitHub | Read every page of them. Only the one the task's STATE names resolves a thread |
| After the merge | `git -C <main checkout> fetch origin`; move a main checkout only with `merge --ff-only`, and not at all while it holds someone's uncommitted edits |
| Worktree and branch | Remove only your own merged worktree, without force, then its branch |

Commit, push, merge and installation are four effects; each needs the task's authority.

## 5. Installing and reading back

Install from a clean checkout of the merged commit, never from a branch.

| What | Install | Read back |
| --- | --- | --- |
| The engine `ont` | `bash <checkout>/scripts/install-ont.sh install <commit>`; also `check` and `rollback` | `ont --version` names the commit; `install-ont.sh check`; one real read; then `ont index` |
| The command `ap` | `CARGO_TARGET_DIR=<target root> bash <checkout>/scripts/install-ap.sh install <commit>`; also `check` and `rollback` | `ap --version` names the commit; `install-ap.sh check` |
| Shared instructions, references and skills of `agent-platform` | `bash /home/palantirkc/work/agent-harness-slim/install-content.sh <checkout> <commit>`: it verifies the checkout against `content/SHA256SUMS`, keeps the previous bytes as a tar file in its `recovery/` directory, replaces `AGENTS.md`, `runtime-references/` and the owned skill directories as wholes, and verifies every installed file; `rollback` restores the last tar file. `ap` embeds the same files, so `ap` is installed from the same commit | In `/root/.agents`: `sha256sum -c <checkout>/content/SHA256SUMS`; then a new session of your own shows the changed text |
| Academy skills, the Academy MCP server, memory records | Their own owners and holds; none of the forms above installs them | The owner's record (`CODEBASE-ORIENTATION.md` section 4) |

- Keep the previous binary or bytes and know the rollback command before you install.
- A session that was running before an installation keeps what it loaded. A session whose `ont` MCP server started before an engine installation reconnects that server.
- When an installation ends uncertain, read the target (`check`, the version, the checksums) before you run anything again.

## 6. Changing a shared instruction, a reference or a skill

1. Find the source: `CLI-RUNTIMES.md` section 7 names the repository of each skill; `AGENTS.md` and `runtime-references/` are in `agent-platform` `content/`. Never edit only the installed copy.
2. Apply `/root/.agents/AGENTS.md` section 7: one home per fact, no changing value in always-loaded text.
3. In `agent-platform`, a file added to or removed from `content/` is also added to or removed from `content/SHA256SUMS` and `crates/ap/src/native/content_inventory.rs`, in the same order; every changed file gets its new checksum. A new reference gets a row in `AGENTS.md` section 3.
4. Run `python3 -I -B ci/check-content.py`. It refuses a checksum mismatch, a route to a missing file or section, a word the glossary replaces, a model name, and a file over its limit.
5. Deliver through sections 3 to 5. The review is by a session on a model other than the writer's.
