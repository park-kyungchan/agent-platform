# ontology

`work.schema.json` declares the `work` domain for the `ont` command of the
`ontology-platform` repository: object types built from files of this host's
daily work. The files stay the home of the facts; `ont index` rebuilds the objects.

Declared in version 1: `task` (STATE.md files), `installed_artifact` (the four
files under `/usr/local/bin`), `instruction_file` (shared instructions installed
under `/root/.agents` and their sources under `content/`). A file that is absent
has no object (`complete_listing` retracts it). No action type, no function.

## Who may change it
Only a pull request to this repository, reviewed and merged like any source.
Nothing here is installed by merging; installation is a separate, authorized step.

## How it is checked
`ont schema check ontology/work.schema.json` must exit 0 with `"clean":true`.
`python3 -I -B ci/check-content.py` does not read this directory.

## Directories the sources must never read
The `markdown_fields` source carries an `exclude` list. An exclude pattern
prunes a directory only when it matches the directory itself, so each pattern
is written per depth below `/home/palantirkc/work/`:
- `ws-daniel-prayer-login`, `problem-edit/context`, `problem-edit/runs`,
  `ws-replan/preparation/slices`: private or custody content.
- any directory named `wt`, `node_modules`, `.git`, or whose name starts with
  `target`, or contains `private` or `custody`, at depth 1, 2 and 3: worktrees,
  build output and private or custody data.
The list exists because a glob under `work/` would otherwise read those paths.
Before changing a glob or the list, enumerate the matched names (no content) and
show that none lies under an excluded path.
