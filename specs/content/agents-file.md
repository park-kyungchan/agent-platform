# The always-loaded file: `content/AGENTS.md`

Purpose: every agent of every CLI on this host loads `content/AGENTS.md`, installed as `/root/.agents/AGENTS.md`, at every start through its CLI's pointer file. The file holds what cannot change by task and what agents keep getting wrong, with the order of work of the life cycle, on one page; everything else is read where a task needs it: a skill, the unit's page, a repository's `AGENTS.md` and map, the store (`ap status`, `ap status --term <word>`), the source at `origin/main`.

## Requirements

- R1: The file is at most 6,500 bytes (`AGENTS_BYTE_LIMIT` of `ci/check-content.py`, `file_over_limit`).
- R2: The file has exactly eight sections, with the headings `## 1. Who decides`, `## 2. The order of work`, `## 3. Four commands`, `## 4. Evidence`, `## 5. Effects`, `## 6. The user's standing limits`, `## 7. Language` and `## 8. Where things are`, in this order, and no other `## ` heading (`agents_section_missing`, `agents_section_unexpected`, `agents_sections_out_of_order`). A file of `content/` routes to one of them as "`AGENTS.md` section <n>" (`route_to_missing_section` holds that the numbered heading exists).
- R3: What each section holds, held by the verifier's reading:
  1. Who decides: the user's words bind; the agent the user speaks to completes them, says what it took them to mean and proceeds on work that can be undone; a worker does its page and nothing more.
  2. The order of work: Request, Intent, Spec, Plan, Unit, Deliver, Operate, Retire, one line each; the lanes; and the rule that teaches the order, in its three sentences: "When a request asks for the output of a later stage while the record of an earlier stage is missing, name the missing stage in one line, ask that stage's questions with defaults, and say what you will do without an answer. One message, at most five questions. Not for a question, a stop, an incident, or work that is already in order."
  3. Four commands: `ap status` before anything else; `ap request` for every message of the user, as `specs/ap/request.md` says; `ap unit page <key>` when given a unit; `ap status --term <word>` for what one word means.
  4. Evidence: a fact from source, Git, a compiler, the store or a service's output, read now; a time from the clock; verification by output, not by a success message.
  5. Effects: no credential printed; no session, pane or tree touched that is not the agent's own; a CLI's private memory holds nothing another agent or a later session needs; before deleting, the copy and every consumer verified; a failure reported as `code@path:line` with one cause, and the same failing step not retried unchanged.
  6. The user's standing limits: the Academy's paths; private data as metadata and hashes unless the user authorizes more; the allowed models by pointer to the policy file; long-term memory off.
  7. Language: everything except a message to the user in English; a message to the user by the skill `report`.
  8. Where things are: six lines, each an absolute path or a pattern, or the store's commands.
- R4: The file is prose and lists: no line over 400 characters (`agents_line_too_long`) and no line beginning with `|` (`agents_table`), so it holds no table, among them no table of files to read and no table of where to write what is learned.
- R5: Every absolute path the file names exists on the host or is written as a pattern, with `<name>` or `*` (`agents_path_missing`). The check runs where the host is present (`/root/.agents/AGENTS.md` exists, or the file the variable `CONTENT_CHECK_HOST` names, which the tests set) and elsewhere prints one line and skips.
- R6: The file holds no value that changes by task: no model name (`model_name_in_content`), date, version, token window, effort, session or pane id or Git revision (`changing_value_in_always_loaded_text`); such a value belongs to a task's record or the policy file. A count that changes by task is held by the verifier's reading.
- R7: The file holds no glossary: what a word of the life cycle means is the description of its type, action, property or value in the store's schema, printed by `ap status --term <word>`; a word with no entry there is defined in the one file that needs it. Held by the verifier's reading.
- R8: A rule that leaves the file is, in the same pull request, either in another installed file (named in the description with that file and line) or listed as dropped with a reason a reviewer can check; a rule the user set (the standing limits, the language, the Academy, private data, credentials, long-term memory off) is never dropped. Held by the verifier's reading.
- R9: The gate: a pull request that changes the file merges only when the routing suite, the intake suite and the Lead suite (`evals/README.md`) each run on its head and none is under its recorded baseline; "under" is the routing suite's `run gate` rule. After delivery, one session of each CLI that loads the file (Claude Code, Muse, Antigravity) shows the new text in its own record. Held by the gate, which the Lead runs; `./check` does not run the suites.
- R10: What an agent learns is written once, at the home of its kind: a CLI's private memory and a session's conversation hold nothing another agent or a later session needs (section 5); a task's STATE and the files it points to are written by the task's owner, replacing old values; a rule or a contract (source, a schema, a test, a KPI's definition) by its owner, through review in the owning repository.
  A request, an intent, a decision, a hold, a measurement, a review or a finding is an action of the store, recorded by the Lead; an operational fact (a deployed state, a domain record, an action and its result) is read fresh from its owner and not copied. Held by the verifier's reading.
- R11: A copy yields to its source: a main checkout's files (section 8), the store's index objects of files (`instruction_file`, `packet`, `report`, `task`), `text/` and `sources/` of the corpus of design sources, instructions in another CLI's home, a copied skill and a generated file. When a copy and current source disagree, source wins and the report names the stale copy.
  An archive grants no authority, and a retired harness is not restored. Held by the verifier's reading.

## Interface

`content/AGENTS.md` in the form of R2 to R7. `ci/check-content.py` holds R1, R2, R4, R5 and R6, each with a test in `ci/test_check_content.py` that plants the fault; `CONTENT_CHECK_HOST` names the file whose presence means "this is the host" (default `/root/.agents/AGENTS.md`).

What a session loads, by its CLI's pointer file, each resolving to the installed file:
- Claude Code loads `~/.claude/CLAUDE.md`, which imports the installed file with an `@` line; Codex, Muse Code and Grok Build load a symbolic link `AGENTS.md` in `~/.codex/`, `~/.config/muse/` and `~/.grok/`; the Antigravity CLI loads the links `~/.gemini/AGENTS.md` and `~/.gemini/GEMINI.md`.
- Codex and Muse Code list `/root/.agents/skills/` directly; Claude Code and the Antigravity CLI list the links in `~/.claude/skills/` and `~/.gemini/config/skills/`. A listed skill is not a read one, and a child agent does not inherit its parent's skills.
- A session has loaded the text when its own record shows it (R9). A session started before an install, or resumed from one, keeps the text it loaded: it reads the task's limits again (section 1).

## Not included

- The pointer files and the skill links of the CLIs are the host's: delivery installs `/root/.agents/` and changes no pointer and no link.
- No check that a section holds its content (R3) or that a moved rule has its home (R8): the verifier reads the diff.
- A mention of a removed name anywhere under `content/` is held by `retired_name` of `ci/check-content.py` (its list `RETIRED_NAMES`), not by a requirement here.
