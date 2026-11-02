# S04 CI fixture preparation correction

The initial source candidate `3f66a22` invoked retained S03 tests without their
required `AP_S03_TEST_ROOT`. It also invoked the frozen foreign-owner case as
an ordinary hosted user, although that case calls `os.chown`. Root-only local
success would not establish that the unprivileged workflow could run it.

`ci/check-rust-source.sh` now creates an explicit private test root beneath the
job-owned workspace or the caller's explicit safe parent. It supplies that
root to the native Rust fault tests and cleans up its empty directory on exit.
It resolves the compiled binary to an absolute path before comparator calls.

For an unprivileged runner, only the frozen 41-case filesystem comparator is
run through noninteractive `sudo`, with a private root-owned scratch parent
under `/root`. Using the runner-owned workspace as that root parent would
violate the candidate's root-or-selected-uid ancestor requirement. An executor
already running as root uses its ordinary private test root. No case is skipped
or weakened; no native profile, configuration or business data is selected.
Missing required sudo capability fails the check explicitly. The workflow's
token permissions remain read-only, with no added secrets or publisher step.

Native Cargo tests and the other comparators retain the job's original uid.
Existing source/oracle bytes are unchanged. Actual hosted execution of the
unprivileged path remains evidence to collect from the candidate CI run;
neither static inspection nor a local root run establishes hosted success.

This correction does not resolve the separate retained-consumer or hosted
ruleset holds recorded in `s04-closed-consumers.md`.
