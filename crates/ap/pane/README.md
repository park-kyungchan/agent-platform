# Pane compatibility

`ap pane [--exports | --focused | --resume-candidate]` ports the host helper
without replacing it. `-h`/`--help`, output, warnings and exit codes are preserved.
Top-level S02 help remains byte-compatible; `ap pane --help` lists these flags.

Only HERDR_ENV=1 permits lookup. The resolver compares process ancestors,
the caller terminal and reported Codex session, in that order. Default lookup
may fall back to an explicitly warned resume/new-session candidate; exports
never use those fallbacks. Every non-focused result is rechecked against a fresh
list and process association. Inherited pane variables and UI focus do not bind
a caller. The residual race after returning an ID and inability to update a
parent shell are unchanged. No hook or session lifecycle controller is added.

Herdr queries have five-second deadlines and captured output; raw command lines
and database contents are never printed. Native Codex metadata is opened
read-only through SQLite (the build needs system libsqlite3 development headers;
Ubuntu hosted CI includes them). No subprocess Python is used by the product.

`reference-pane.sh` is an immutable byte copy (SHA256
`c58948fc590fe1d30c14b0c6e2e069f36f8b2979f808253200d42d0cfe1aaa4b`)
of `/root/.local/bin/pane`, captured 2026-11-04. It is a test oracle, never an
installed executable. Golden fixtures substitute proc, TTY, database and Herdr
I/O around those exact Python source bytes. Literal expected stdout/stderr/status
is checked against both oracle and Rust fixture adapter. CI never regenerates
expectations. `make-fixtures.py` is an explicit maintainer-only generation tool.
The fixture adapter is an example binary, not a production fixture-path option.
