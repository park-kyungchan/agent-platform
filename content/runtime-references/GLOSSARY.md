# Glossary

One word for one thing. Read this when a word in an instruction, a packet or the user's message is unclear, before you introduce a new term, and when you are the Lead turning the user's words into a packet.

## 1. Rules

- The core terms are not repeated here: `ap status --term <word>` prints what a word of the life cycle means (`/root/.agents/AGENTS.md` section 3), and merged, installed and loaded are defined in `/root/.agents/AGENTS.md` section 4.
- Use the term in the first column exactly. Do not write the words in the last column: the content check fails a pull request that contains one of them outside this file.
- A term that is not here and not in `AGENTS.md` gets a definition at its first use in the file that needs it. Add it here when a second file needs it.
- Packets, STATE files and reports use these terms. The user's own wording stays in the conversation and in the decision record that quotes it.
- Vocabulary of a domain (lessons, students, papers, churches) belongs to the repository that owns the domain, and to the object types of `ontology-platform` once STATE says they are in use. It is not copied here.
- This is a closed set with a budget, not a list that grows. The content check fails when a term here is used in no other shared file, when the core list, this file or the user table passes its limit (the limits are constants in `agent-platform` `ci/check-content.py`; raising one is a reviewed change with its reason), so adding a row at the limit means removing or moving one first. A term enters only when a second file needs it; a user expression enters only when it appears in a second task; a row leaves when its term is no longer used or the user stopped using the expression.

## 2. Work coordination

| Term | Meaning | Example on this host | Do not write |
| --- | --- | --- | --- |
| board | A task's items with their state, its objectives and order, printed from the store (its holds: the view `active_holds`); where the store is not in use, an optional file that holds no value a command can produce | `ap backlog <task key> --text` | `dashboard file` |
| design note | The Lead's written design of a store format, the schema of an authoritative store, or the order of record and effect on a real target: use, decision, invariants as executables, pull requests, undo | `DESIGN-<topic>.md` in the task directory; its form: `/root/.agents/skills/lead/references/judging.md` section 1 | `design doc` |
| checkpoint | What a new session continues from: the intent's objects in the store and the STATE card | The answer of `ap status --brief <intent>` with the STATE card that names the intent | `snapshot file` |
| work thread | One line of work inside a task that serves a named KPI or guardrail and has one owner and one next action | A unit of the store, or a row of a board file where the task keeps one | `workstream` |
| tier | The amount of reasoning a piece of work needs: mechanical, bounded judgment, open judgment | A packet line `Tier: mechanical` | `difficulty level` |
| assignment file | Where a task says which runtime, account and model serves which work: each stage's request, written by the Lead; a standing choice is the policy file's `accounts` and the unit's tiers | The `model`, `effort`, `account` and `next_accounts` of a stage's request file | `routing table` |
| dispatch | Starting one worker run with one packet | `ap unit <page>` for a unit of work, `ap run < <request file>` for one stage; a Herdr pane only for work the user watches (`CONCURRENT-WORK.md` section 5) | — |
| progress message | A message to the user that says where the task stands; not a worker's report | The board of skill `report` and what changed | — |
| acceptance | A closed `review` object with the verdict `accept` for a head, written by `ap unit` after the gate, the page's acceptance executable, the write set and one accepting verifier; the status `lead-review=success` on that head is derived from it. For a unit with a real effect, a schema, a store format or a guard of a live request the Lead reads the diff before the merge | The `review` object that `ap unit` closed for a head | `sign-off` |
| review | A second agent, in a fresh context, judging a frozen candidate against the request and the contracts | The one reviewer session of a pull request (the verifier), which posts its findings there | — |
| candidate | The exact revision that is offered for review or merge | A commit on a task branch | — |
| handoff | Passing a piece of work, with a packet, to another agent; also one Lead passing a task to the next Lead | `CONCURRENT-WORK.md` section 2 | `hand-off`, `handover` |
| closure | Ending a worker run or a task: record, reconcile, close what the task created | `lead/SKILL.md`, section "Your own limits" | — |
| write set | The area one writer changes; a change outside it stops the unit until the Lead widens it | The `Write set:` line of a packet | — |
| delegation | Giving a piece of work to a worker | A dispatch | — |

## 3. Instructions and loading

| Term | Meaning | Example on this host | Do not write |
| --- | --- | --- | --- |
| shared instructions | The one instruction file every coding CLI loads, with its references and skills | `/root/.agents/AGENTS.md`, `/root/.agents/runtime-references/`, `/root/.agents/skills/` | `canonical rules`, `global rules` |
| pointer file | The file through which one CLI finds the shared instructions | `~/.claude/CLAUDE.md` | `loader file` |
| reference | A file under `runtime-references/` that is read only when its situation applies | This file | — |
| skill | A directory with a `SKILL.md` whose short description is always listed and whose body is read when it applies | `/root/.agents/skills/herdr/SKILL.md` | — |
| source | The repository files a change is made in, as opposed to the installed copy | `agent-platform` `content/` | — |
| session record | The file a CLI writes about one session, with its messages and tool calls | A `session.jsonl` | `transcript log` |
| headless run | A CLI started with one prompt that exits when the prompt is done | `CLI-RUNTIMES.md` section 2 | — |
| pane, tab, workspace | Herdr's units: a pane is one terminal, a tab holds panes, a workspace holds tabs | `w2:p3`, `w2:t1`, `w2` | — |

## 4. Memory and stores

| Term | Meaning | Example on this host | Do not write |
| --- | --- | --- | --- |
| long-term memory | Decisions with their reasons, lessons, corrections, preferences and explained terms that later tasks need | The Hindsight bank, reached through the MCP server `hindsight` | — |
| private memory | A store that one CLI keeps for itself and no other CLI reads | `CLI-RUNTIMES.md` section 8 | `personal memory` |
| knowledge page | A page Hindsight derives from stored documents; a way to find evidence, not evidence itself | A result of the page search tool | — |
| operational fact | What is true of a running system or a domain record now | A deployed revision, a KPI value | — |
| correction | A dated document that says what an earlier record claimed, what is true now and the evidence | For an engineering record, an edit of the same Git original through review; see `CONTEXT-GOVERNANCE.md` section 9 | — |

Where each kind is written: `CONTEXT-GOVERNANCE.md` section 6.

## 5. Measurement

Metric, baseline, target, guardrail and proxy, and the way to write the user's wish as an intent's criteria: `/root/.agents/skills/lead/references/interview.md` section 3.

## 6. Repositories and paths

Names and locations of platforms, modules, tasks and services: `CODEBASE-ORIENTATION.md`. Use the repository names written there.

## 7. How the user refers to things

Only the Lead needs this section. The user speaks Korean, shortens names, and sometimes uses a neighbouring word. Resolve the reference in this order: this table; what the conversation was just about; what exists on the host under the nearest name. Then name the resolved thing with its path in the first sentence of an answer. Ask only when two readings would lead to different effects and no source decides between them.

| The user says | Usually means | Check before acting |
| --- | --- | --- |
| 하네스, Harness Engineering | The shared instructions, the pointer files and how sessions load them | Not the retired repository of a similar name |
| 지침, 인스트럭션 | The shared instructions; sometimes one skill or one packet | Which file: `AGENTS.md`, a reference, a skill, or a task's packet |
| 런타임 레퍼런스, runtime-reference | `/root/.agents/runtime-references/` (plural) | — |
| 스킬 | A shared skill under `/root/.agents/skills/`; sometimes a skill a single CLI installed | A misspelled skill name means the nearest existing one |
| 메모리 | One of three: the task's STATE and the files it points to, long-term memory, private memory | "단기" is the task's STATE and the files it points to, "중장기" is long-term memory, "CLI의 메모리" is private memory |
| 하인드사이트, HindSight | Hindsight: the long-term memory server and its MCP tools | — |
| 온톨로지, Ontology | Ontology in Palantir's sense: object types, links and actions in `ontology-platform` | Not a general word list or taxonomy |
| REA, Ghidra Framework | REA, the reverse-engineering tool cloned at `/home/palantirkc/tools/rea`, with Ghidra beside it as its backend for native binaries | `CODEBASE-ORIENTATION.md` section External tools: static or dynamic use, and whether it is built |
| Rust 전환, 마이그레이션 | Moving an implementation to Rust in the platform repositories | Which repository and which consumer |
| 컷오버, cutover | Switching the live consumers of a service from the old implementation to the new one | Which service and which consumers; the rollback route |
| 수업 세션 | A CLI session the user runs while teaching | Never send input to it or close it |
| 워커, 서브에이전트, 에이전트들 | Workers; sometimes every agent on the host including Leads | — |
| 새 Lead, 새 세션 | The Lead session that takes the task over next | — |
| 라우팅 | Either which file an agent opens for a situation, or which model gets which work | The first is `AGENTS.md` section 8 and `CONTEXT-GOVERNANCE.md`; the second is the assignment file |
| 컨텍스트 거버넌스, Context Governance | Which context each agent loads, from which one home, in what order, without wasted reads or a wrong choice | `CONTEXT-GOVERNANCE.md` |
| reference-docs, 레퍼런스 문서 | The local copies of the Anthropic, Palantir, Rust and Spec-Driven Development sources under `/home/palantirkc/external/reference-docs/` | Read only the lines a row of `OFFICIAL-DOCS.md` section 4 names, never the whole corpus |
| 지표, 메트릭, KPI | A number the user wants watched; often a metric that is not yet a KPI | Write it as a criterion of the intent (section 5) before using it |
| VPS, 코드베이스 전체 | This host: the platform repositories, the task directories and the shared instructions | Scope it with `CODEBASE-ORIENTATION.md` |
| CI | The required checks that run on a pull request in the platform repositories | Which repository |
| 보드 | The board of the current task | — |
| 정리, 손봐, 개선 | A request whose target and extent are open | State what you will change and what you will leave before starting |

What a new expression resolved to is the reading of the request that holds it (`ap request`). When the same expression appears in a second task, add a row here through a pull request; most expressions never need a row, because the three-step resolution above already finds them.
