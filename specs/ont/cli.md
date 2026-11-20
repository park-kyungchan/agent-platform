# CLI

Purpose: Installing the binary and the diagnostics of command use and input.

Moved from crates/ont/README.md ("binary install script", "Usage and input diagnostics") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## binary install script

- R1: `scripts/install-ont.sh install <commit>` prints and runs `git fetch origin +refs/heads/main:refs/remotes/origin/main --refmap=` in the checkout before checking ancestry; `--dry-run` prints the fetch step without executing it.
- R2: Run the script directly or with `bash`.
- R3: The build disk floor is checked on its temporary build directory's file system.
- R4: Before replacing `ont`, the script verifies the retained previous copy's sha256.
- R5: After writing `installed.json` it keeps at most three previous copies.
- R6: The script unsets inherited `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and `GIT_COMMON_DIR`.
- R7: An unknown commit after fetch refuses with `commit_unknown`; Git execution failures remain `git_unavailable`.
- R8: A state-write failure after replacement in either install or rollback exits nonzero and prints manual recovery commands. For rollback the printed command repeats rollback to record the restored binary.
- R9: `rollback` validates the previous binary's version before replacement.
- R10: Test directory overrides require `ONT_INSTALL_TEST=1` and must resolve outside the real bin and state directories.

## Usage and input diagnostics

- R11: `ont help`, `ont --help`, `ont -h`, and `ont COMMAND --help` return JSON usage with the command forms, options, stdin fields and examples.
- R12: Refusals for malformed fields carry `field` (a JSON pointer, or option name) and `rule`, without the rejected value; unknown command/option refusals also carry `hint: "ont help"`.

- R13: A render consumer records the required block count N and always passes `ont render --check FILE --expect-blocks N`; a mismatch refuses with `render_block_count`, `expected`, and `found`, including a balanced fence that hides a real block from the parser.

