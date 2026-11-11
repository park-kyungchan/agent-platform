# ap

`ap --version` (and `ap version`) prints one line `ap <Cargo package version> <40-hex commit>[-dirty]`, or `unknown` for the commit when Git was unavailable at build time.
`scripts/install-ap.sh` is the install script for `ap`: `check` compares the installed binary with `origin/main`, `install <expected commit>` builds and installs a verified revision, and `rollback <before-file>` restores a previous binary.
