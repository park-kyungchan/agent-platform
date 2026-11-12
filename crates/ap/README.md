# ap

`ap --version` (and `ap version`) prints one line `ap <Cargo package version> <40-hex commit>[-dirty]`, or `unknown` for the commit when Git was unavailable at build time.
`scripts/install-ap.sh` is the install script for `ap`: `check` compares the installed binary (`AP_INSTALL_DIR`, default `/usr/local/bin`) with `origin/main`, `install <expected commit>` builds from a temporary detached worktree of that exact commit and installs it after verifying the staged and installed `--version` report exactly that commit, and `rollback <before-file>` restores a backup.
For `install`, `origin/main` means the local ref, so run `git fetch` first; the previous binary is backed up as `ap.before-<first 7 hex of its sha256>` in `AP_BACKUP_DIR` (default `/var/lib/agent-platform/install`), and `rollback` accepts only that name in that directory and prints the restored version (or `no_version_output`). The script lock is `AP_INSTALL_LOCK` (default `/run/lock/ap-install.lock`).
Limitation: a source tarball nested inside another Git repository reports the outer repository's revision, because the build derives the revision from the enclosing repository.

## One worker run

`ap work start`, `ap work wait` and `ap work close` each read one JSON object from stdin (at most 1 MiB) and emit one JSON object. Refusals emit `{"error":{"code","field","detail"}}` on stderr with a nonzero exit. These commands execute the installed `ont` and `herdr`; they do not install either program.

`start` takes `task_dir`, `name`, `packet`, `report`, `thread`, `layer`, `tier`, `cwd`, `model`, `effort`, `workspace`, and optional `worktree`, `store`, `continue_dirty`. Paths are absolute; packet/report references are relative to `/home/palantirkc/work/`. Names match `^[a-z0-9][a-z0-9-]{0,39}$`. A worktree requires a clean Git status unless `continue_dirty` is true, plus packet lines beginning `Callers searched:`, `Broken tests:` and `Controls:`.

The store validates admission before any Herdr call. After admission, `start` creates one Codex session, records the returned full run Id using a stable packet-hash request id, and sends one prompt. Its output includes `run`, `session`, `pane`, `tab` and `prompt` (`sent` or `uncertain`). Keep the full `run` for closing. An uncertain start/apply leaves the tab open for the Lead to reconcile; a confirmed store refusal closes the unprompted tab. A failed refusal cleanup reports `tab_closed: false`.

`wait` takes `name`, `report`, `timeout_seconds` (unsigned integer). It waits once and checks the report even if waiting fails. A present report returns its `sha256` and `bytes` with exit 0; an absent report returns `report_missing` with exit 3. `agent_status` is null when unavailable. It sends no prompt and reads no pane.

`close` takes the full `run`, `result` (at most 600 UTF-8 bytes), `name`, `tab`, and optional `report`, `store`, `agent_gone`. It verifies the tab label and that the named agent is `idle`, `done` or `blocked`; a missing agent requires `agent_gone: true`, supplied after the Lead observed that it ended. Only a successful store close permits tab closure. A failed tab close returns `tab_closed: false` with exit 3. An uncertain store close leaves the tab open.

`store` forwards `--store <absolute path>` to every Ontology call, permitting disposable-store use. Uncertainty requires readback by the Lead; no command automatically retries a prompt or sends keys.
