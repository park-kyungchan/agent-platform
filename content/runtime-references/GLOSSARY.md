# Glossary

One word for one thing. Read this when a word in an instruction, a packet or the user's message is unclear, before you introduce a new term, and when you are the Lead turning the user's words into a packet.

## 1. Rules

- The core terms (Lead, worker, task directory, STATE, packet, report, effect, hold, evidence, merged, installed, loaded, objective, KPI, guardrail) are defined in `/root/.agents/AGENTS.md` section 8 and are not repeated here.
- Use the term in the first column exactly. Do not write the words in the last column: the content check fails a pull request that contains one of them outside this file.
- A term that is not here and not in `AGENTS.md` gets a definition at its first use in the file that needs it. Add it here when a second file needs it.
- Packets, STATE files and reports use these terms. The user's own wording stays in the conversation and in the decision record that quotes it.
- Vocabulary of a domain (lessons, students, papers, churches) belongs to the repository that owns the domain, and to the object types of `ontology-platform` once STATE says they are in use. It is not copied here.
- This is a closed set with a budget, not a list that grows. The content check fails when a term here is used in no other shared file, when the core list, this file or the user table passes its limit (the limits are constants in `agent-platform` `ci/check-content.py`; raising one is a reviewed change with its reason), so adding a row at the limit means removing or moving one first. A term enters only when a second file needs it; a user expression enters only when it appears in a second task; a row leaves when its term is no longer used or the user stopped using the expression.

## 2. Work coordination

| Term | Meaning | Example on this host | Do not write |
| --- | --- | --- | --- |
| board | The one file of a multi-thread task that shows KPI values, work threads, live workers, decisions and open questions; STATE points to it | `/home/palantirkc/work/<scope>-<topic>/BOARD.md` | `dashboard file` |
| checkpoint | The content of STATE at a moment, complete enough for a new session to continue from it | A STATE whose header carries the time it was written | `snapshot file` |
| work thread | One line of work inside a task that serves a named KPI or guardrail and has one owner and one next action | A row of the board's thread table | `workstream` |
| tier | The amount of reasoning a piece of work needs: mechanical, bounded judgment, open judgment | A packet line `Tier: mechanical` | `difficulty level` |
| assignment file | The one file of a task that says which runtime and model serves which tier and in which order; STATE names it | A section of a task's orchestration document | `routing table` |
| dispatch | Starting one worker run with one packet | `herdr agent start <name> --kind <kind> --pane <pane-id>`, then `herdr agent prompt <name> "<text naming the packet>"` | — |
| progress message | A message to the user that says where the task stands; not a worker's report | The KPI table and what changed | — |
| acceptance | The Lead reading back the actual output or effect of a worker and deciding to use it | Reading the diff and rerunning the check | `sign-off` |
| review | A second agent, in a fresh context, judging a frozen candidate against the request and the contracts | A reviewer subagent that writes findings by severity | — |
| candidate | The exact revision that is offered for review or merge | A commit on a task branch | — |
| handoff | Passing a piece of work, with a packet, to another agent; also one Lead passing a task to the next Lead | `concurrent-work.md` section 2 | `hand-off`, `handover` |
| closure | Ending a worker run or a task: record, reconcile, close what the task created | `lessons-loop/references/worker-closure.md` | — |
| write set | The files one writer may change | `Exact targets` line of a packet | — |
| build slot | The right to run the one expensive build at a time | Named in STATE at dispatch | — |
| delegation | Giving a piece of work to a worker | A dispatch | — |
| standing go-ahead | The user's recorded words that let the Lead carry out one class of irreversible outward action without asking each time; kept in STATE only | A quoted line in STATE with its date | `blanket approval` |

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
| correction | A dated document that says what an earlier record claimed, what is true now and the evidence | A Hindsight document titled "Correction: <topic>" | — |

Where each kind is written: `/root/.agents/AGENTS.md` section 6.

## 5. Measurement

Metric, baseline, target, leading and lagging indicator, output and outcome, proxy, acceptance criterion, and the way to build a KPI from the user's words: `/root/.agents/skills/strategy-dialogue/references/measurement.md`.

## 6. Repositories and paths

Names and locations of platforms, modules, tasks and services: `codebase-orientation.md`. Use the repository names written there.

## 7. How the user refers to things

Only the Lead needs this section. The user speaks Korean, shortens names, and sometimes uses a neighbouring word. Resolve the reference in this order: this table; what the conversation was just about; what exists on the host under the nearest name. Then name the resolved thing with its path in the first sentence of the reply. Ask only when two readings would lead to different effects and no source decides between them.

| The user says | Usually means | Check before acting |
| --- | --- | --- |
| 하네스, Harness Engineering | The shared instructions, the pointer files and how sessions load them | Not the retired repository of a similar name |
| 지침, 인스트럭션 | The shared instructions; sometimes one skill or one packet | Which file: `AGENTS.md`, a reference, a skill, or a task's packet |
| 런타임 레퍼런스, runtime-reference | `/root/.agents/runtime-references/` (plural) | — |
| 스킬 | A shared skill under `/root/.agents/skills/`; sometimes a skill a single CLI installed | A misspelled skill name means the nearest existing one |
| 메모리 | One of three: the task's STATE and board, long-term memory, private memory | "단기" is the task's STATE and board, "중장기" is long-term memory, "CLI의 메모리" is private memory |
| 하인드사이트, HindSight | Hindsight: the long-term memory server and its MCP tools | — |
| 온톨로지, Ontology | Ontology in Palantir's sense: object types, links and actions in `ontology-platform` | Not a general word list or taxonomy |
| Rust 전환, 마이그레이션 | Moving an implementation to Rust in the platform repositories | Which repository and which consumer |
| 컷오버, cutover | Switching the live consumers of a service from the old implementation to the new one | Which service; whether a standing go-ahead is recorded |
| 수업 세션 | A CLI session the user runs while teaching | Never send input to it or close it |
| 워커, 서브에이전트, 에이전트들 | Workers; sometimes every agent on the host including Leads | — |
| 새 Lead, 새 세션 | The Lead session that takes the task over next | — |
| 라우팅 | Either which file an agent opens for a situation, or which model gets which work | The first is the table in `AGENTS.md` section 3; the second is the assignment file |
| 지표, 메트릭, KPI | A number the user wants watched; often a metric that is not yet a KPI | Define it with the measurement reference of section 5 before using it |
| VPS, 코드베이스 전체 | This host: the platform repositories, the task directories and the shared instructions | Scope it with `codebase-orientation.md` |
| CI | The required checks that run on a pull request in the platform repositories | Which repository |
| 보드 | The board of the current task | — |
| 정리, 손봐, 개선 | A request whose target and extent are open | State what you will change and what you will leave before starting |

A new expression is written in the task's STATE under "Words the user used" with what it resolved to. When the same expression appears in a second task, add a row here through a pull request (skill `lessons-loop`); most expressions never need a row, because the three-step resolution above already finds them.
