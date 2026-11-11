# ontology
`work.schema.json` declares the `work` domain for the `ont` command of the
`ontology-platform` repository: object types built from files of this host's
daily work. The files stay the home of the facts; `ont index` rebuilds the objects.

Declared in version 1: `task` (STATE.md files), `packet` (TASK*.md at directory
depth 1 to 3), `report` (REPORT*.md at the same depths), `installed_artifact`
(four paths under `/usr/local/bin` are declared; only those that exist become
objects) and `instruction_file` (shared instructions under `/root/.agents` and
their sources under `content/`). An absent file has no object. No action or
function. Packets and reports are keyed by their path below the work root.

Links: `packet_task` and `report_task` (the first directory below the work root
equals the task key), `report_packet` (`REPORT-<id>.md` points to
`TASK-<id>.md` in the same directory) and the reference `source_copy` (an
installed instruction file points to its source under `content/`).
A reference has two special states. "No value": the rule could not build a key
(a `REPORT.md` has no packet id; a source file has no source copy). "Dangling":
the key is stored but no such object exists (the packet has another name, the
directory has no STATE.md, the skill source is in another repository).
`report.packet` is exact, with no fallback in version 1.

## Who may change it
Only a pull request to this repository, reviewed and merged like any source.
Nothing here is installed by merging; installation is a separate, authorized step.

## Check
`ont schema check ontology/work.schema.json` must exit 0, `"clean":true`.

## Directories the sources must never read
Each `markdown_fields` source carries an `exclude` list. A pattern prunes a
directory only when it matches the directory itself, so it is written per depth:
- `ws-daniel-prayer-login`, `problem-edit/context`, `problem-edit/runs`,
  `ws-replan/preparation/slices`: private or custody content.
- any directory named `wt`, `node_modules`, `.git`, or whose name starts with
  `target`, at depth 1, 2 and 3; any directory or file whose name contains
  `private`, `custody`, `student`, `oracle`, `secret` or `credential`, at every depth.
The list exists because a glob under `work/` would otherwise read those paths.
Before changing a glob or the list, enumerate matched names (no content) and
show none lies under an excluded path.
