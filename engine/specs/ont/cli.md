# CLI

Purpose: Installing the binary and the diagnostics of command use and input.

Moved from crates/ont/README.md ("binary install script", "Usage and input diagnostics") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## binary install script

- R1: `scripts/install-ont.sh install <commit>` fetches nothing and makes no ancestor test (the review that accepted the commit accepted its engine; `specs/ap/deliver.md` R67 of the repository around it): it builds `engine/` of `<commit>` of the repository around the script (the top level `git rev-parse --show-toplevel` names, where every Git command of the script runs), exported by `git archive <commit> engine`, with `ONT_TREE_ID` the Git tree id of `engine/` at the commit, requires the built binary's `--version` to print that id, and records it as the member `commit` of `installed.json`, which `check` compares with `--version`; `--dry-run` names the commit and its tree and prints no fetch.
- R2: Run the script directly or with `bash`.
- R3: The build disk floor is checked on the file system of the new build directory under the build base (R14).
- R4: Before replacing `ont`, the script verifies the retained previous copy's sha256.
- R5: After writing `installed.json` it keeps at most three previous copies.
- R6: The script unsets inherited `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and `GIT_COMMON_DIR`.
- R7: An unknown commit refuses with `commit_unknown`, a commit without `engine/` with `engine_missing`; Git execution failures remain `git_unavailable`.
- R8: A state-write failure after replacement in either install or rollback exits nonzero and prints manual recovery commands. For rollback the printed command repeats rollback to record the restored binary.
- R9: `rollback` validates the previous binary's version before replacement.
- R10: Test directory overrides require `ONT_INSTALL_TEST=1` and must resolve outside the real bin and state directories.
  The test hook `ONT_INSTALL_TEST_TRUST=<directory>` also requires `ONT_INSTALL_TEST=1` and is accepted only when the build base lies under it; the build base check (R14) then does not read that directory and its ancestors. No real install sets it.
- R14: `install <commit>` makes its temporary directory (the build's `src` and `target`; with `ONT_INSTALL_PREBUILT` only the directory it cleans up) as a new directory of mode 0700 under the build base `<state>/build` (`/var/lib/ontology-platform/build`; under `ONT_INSTALL_TEST=1`, `$ONT_INSTALL_STATE_DIR/build`), never under `/tmp` or `$TMPDIR`. A missing base and its missing parents are created with mode 0700. For an install that builds, after the commit checks (R7) and the return of `--dry-run`, the base is checked: it is absolute, its trailing slashes are stripped, and every existing component from `/` to the base, read as it stands without following a link, is a directory and not a symbolic link, is owned by uid 0 or by the uid that runs the script (`$EUID`), and has no write bit for group or other (a sticky directory counts as writable). The existing part is checked before anything is created, and the whole path again after the base is created. A failing path refuses `refused: build_dir_unsafe: <component>` (exit 2), naming the failing component nearest the base; a base that cannot be created refuses `refused: build_dir_unusable: <base>`. A dry run checks and creates nothing and names the base. Before making its directory, an install takes `flock -n` on `<base>/.lock` and holds it to its end (held by another install: `refused: install_busy`), then removes every other entry of the base, a link as a link, touching nothing outside the base.

## Usage and input diagnostics

- R11: `ont help`, `ont --help`, `ont -h`, and `ont COMMAND --help` return JSON usage with the command forms, options, stdin fields and examples.
- R12: Refusals for malformed fields carry `field` (a JSON pointer, or option name) and `rule`, without the rejected value; unknown command/option refusals also carry `hint: "ont help"`.

- R13: A render consumer records the required block count N and always passes `ont render --check FILE --expect-blocks N`; a mismatch refuses with `render_block_count`, `expected`, and `found`, including a balanced fence that hides a real block from the parser.

The tailnet HTTP server subcommand `ont serve` is specified in `specs/ont/serve.md`.

