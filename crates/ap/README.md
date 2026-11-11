# ap

`ap --version` (and `ap version`) prints one line `ap <Cargo package version> <40-hex commit>[-dirty]`, or `unknown` for the commit when Git was unavailable at build time.
`scripts/install-ap.sh` is the install script for `ap`: `check` compares the installed binary (`AP_INSTALL_DIR`, default `/usr/local/bin`) with `origin/main`, `install <expected commit>` builds from a temporary detached worktree of that exact commit and installs it after verifying the staged and installed `--version` report exactly that commit, and `rollback <before-file>` restores a backup.
For `install`, `origin/main` means the local ref, so run `git fetch` first; the previous binary is backed up as `ap.before-<first 7 hex of its sha256>` in the install directory, and `rollback` accepts only that name in that directory and prints the restored version (or `no_version_output`).
Limitation: a source tarball nested inside another Git repository reports the outer repository's revision, because the build derives the revision from the enclosing repository.
