# ontology

`work.schema.json` declares the `work` domain for the `ont` command of the
`ontology-platform` repository: object types built from files of this host's
daily work. The files stay the home of the facts; `ont index` rebuilds the objects.

Declared in version 1: `task` (STATE.md files), `packet` (TASK*.md at directory
depth 1 to 3), `report` (REPORT*.md at the same depths), `installed_artifact`
(the four files under `/usr/local/bin`) and `instruction_file` (shared
instructions installed under `/root/.agents` and their sources under `content/`).
A file that is absent has no object (`complete_listing` retracts it). No action or function.
Packets and reports are keyed by their path below the work root.

Links: `packet_task` and `report_task` (the first directory below the work root
equals the task key), `report_packet` (`REPORT-<id>.md` points to
`TASK-<id>.md` in the same directory) and the reference `source_copy` (an
installed instruction file points to its source under `content/`).
"Dangling": the reference is stored but its target object does not exist (an
installed skill whose source is in another repository). "No link": the reference
has no value (a `REPORT.md` has no id; a source file has no source copy).

## Who may change it
Only a pull request to this repository, reviewed and merged like any source.
Nothing here is installed by merging; installation is a separate, authorized step.

## Check
`ont schema check ontology/work.schema.json` must exit 0 with `"clean":true`.

## Directories the sources must never read
Each `markdown_fields` source carries an `exclude` list. A pattern
prunes a directory only when it matches the directory itself, so each pattern
is written per depth below `/home/palantirkc/work/`:
- `ws-daniel-prayer-login`, `problem-edit/context`, `problem-edit/runs`,
  `ws-replan/preparation/slices`: private or custody content.
- any directory named `wt`, `node_modules`, `.git`, or whose name starts with
  `target`, or contains `private` or `custody`, at depth 1, 2 and 3: worktrees,
  build output and private or custody data.
The list exists because a glob under `work/` would otherwise read those paths.
Before changing a glob or the list, enumerate matched names (no content) and
show that none lies under an excluded path.
