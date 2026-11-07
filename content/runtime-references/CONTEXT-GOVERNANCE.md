# Context Governance — on demand

Context Governance routes each question to its owning source and gives each
agent only the context its authorized task needs. This is a cross-CLI convention,
not a common instruction loader, memory engine, scheduler or permission system.
Source basis: Hindsight architecture design of 2026-11-05, its Skill contract and
the approved strategy/structured-ingestion addenda. These documents authorize
content changes, not Hindsight migration, configuration or deployment.

## Memory tiers and write ownership

| Information | Owner store | Writer / path | Read rule |
| --- | --- | --- | --- |
| Active conversation, reasoning and resumable session | CLI-native session/memory: Claude auto-memory, Codex native facilities, Muse and agy sessions | Native CLI/user; preserve vendor formats, trust, resume and retention | Resume only the exact needed session; do not import another transcript for convenience |
| Owner, authority, holds, next action, task metrics and verification | Current TASK/STATE; task STRATEGY.md for its brief | Assigned owner edits task files and dates changed decisions/addenda | Read on entry/reset/handoff and before effects when changed |
| Implemented behavior, schemas, APIs, tests, build instructions and KPI definitions | Owning repository source/docs and native observations | Authorized implementation owner, reviewed source/tests | Read the selected current revision and contract; do not rely on a remembered implementation |
| Durable decisions, rationale, lessons, corrections, scoped preferences, explained terms and strategy decisions | One shared Hindsight bank, `coding-agent::shared` | Authorized owner/curator through official HTTP/client for structured records; MCP only for simple notes/initiatives | Search/read selected pages first; recall exact evidence; reflect only for unresolved reasoning |
| Current operational objects, observations, KPI time series, policies, actions and recovery | Domain/task owner today; future Ontology boundary. Exception: the Academy lesson archive in Hindsight (user decision 2026-11-06; education-platform `content/skills/academy-student-context/references/lesson-archive.md`), written by the lesson agent through the MCP `hindsight_ingest_document` tool with a deterministic title | Trusted domain owner under current admission and task authority | Fresh authorized owner read with revision, scope, coverage and freshness; memory is only a citation |

A confirmed Strategy Brief lives in task STATE/STRATEGY.md. Hindsight may retain
its strategy decision and an authorized KPI definition/rationale linked to repo
docs. Measured KPI values/time series stay with their current task/domain owner,
then Ontology when implemented. Targets are strategy decisions; observations are
operational facts. Do not turn a strategy document into a second execution table.

A durable decision another agent needs must not remain solely in a CLI's personal
memory: put it in the authoritative repository contract or an authorized,
source-backed Hindsight document. Native memory never gains cross-runner loading
semantics merely because the same bank is configured.

## Isolation and continuity

- Default: a fresh worker receives a small context packet: goal, exact targets,
  allowed effects, expected output, current STATE path, exact TASK/canonical paths,
  source revision and selected raw evidence. Never transfer a transcript.
- Continue a session only when the follow-up stays in the same scope/files,
  reuses needed loaded context and has enough context remaining. Record the
  concrete unfinished continuation in STATE; otherwise save/close and start fresh.
- Reviews are always isolated from implementation reasoning. Give the reviewer
  the request/contracts and frozen candidate/raw evidence, not an intended verdict.
- Context isolation is not filesystem, build, service or authority isolation.
  Verify worktree, disjoint write set, shared-resource ownership and actual runtime.
  An agent name or parallel-tools flag proves none of them.
- Workers keep native Hindsight hooks and MCP (user decision 2026-11-06: never
  block a native feature). When a task needs memory isolation, run it under a
  native coding-agents `paths.<prefix>` override (for example
  `"/home/palantirkc/work/eval-isolated": {"disabled": true}` or a separate bank)
  and verify the effective override. Do not enable clients/configuration to
  overcome a deliberately disabled interface.
- **CLI-agnostic model reasoning tiers & ~400k compaction (user decision 2026-11-07):**
  assign open-ended reasoning, architecture, authoring and semantic review according
  to current user/task authority and actual runtime support, using a suitable
  high-reasoning model. Examples include `muse-wonder-1.0/high`,
  `gpt-6.1-sol/high`, `claude-opus-5-5/high` and `Gemini 4 Argon`; this is not an
  exhaustive allowlist. Restrict `muse-spark-1.3/medium` and
  `gemini-3.8-flash-medium` strictly to deterministic, closed-spec tasks (fixed
  checklists, hash/count/diff verification, structured table/JSON projection).
  Target ~400k tokens for the original four CLIs using supported native controls;
  verify actual enforcement per runtime. Manual compaction is not an automatic
  ceiling. Qualify Grok separately (see `CLI-LAUNCH.md`).
- **4-step Context Governance Optimization Loop (user decision 2026-11-07):**
  1. *Pre-spawn Parameter & Path Pinning*: verify CLI launch flags, ~400k
     compaction settings, `cwd` (`paths.<prefix>` isolation when needed) and
     exact `Read first:` file list before spawning a worker.
  2. *First-Line Readback Gate*: require the worker's first output line to state
     `First output after reading: <exact paths>; Context-Injected: <none | injected knowledge-page titles>`.
     If stale or unrequested context is injected and interferes with the task,
     halt immediately and adjust `paths.<prefix>` or knowledge-page scope.
  3. *Post-run Session JSONL Audit*: inspect the worker's native session log
     (`muse` `session.jsonl`, `agy` `transcript.jsonl`, `claude` project `.jsonl`,
     `codex` `rollout-*.jsonl`) to inspect recorded tool calls and targets. Logs
     cannot establish absence of unrecorded subprocess, service, read or transmission
     effects. Use separate isolation and access controls, plus appropriate independent
     evidence, when an absence claim matters.
  4. *Leading Indicator `C` Recording*: classify injected context items at worker
     closure as `useful / irrelevant / misleading` (`misleading = 0` guardrail)
     and feed any `irrelevant`/`misleading` hit back into page/path refinement.
- After reset/resume, reread current task rules/STATE/addenda before effects.
  A native session's retained context does not renew authority.

## Source of truth per question

| Question | Authority / verification |
| --- | --- |
| What may I do now, and what is next? | Current user instructions, canonical AGENTS and task rules/STATE; memory is historical context |
| What did the user decide and why? | Dated source decision and provenance, newer corrections and current task authority; a generated page is navigation |
| What does the program implement? | Current native source/Git revision, compiler/runtime output and relevant tests |
| What KPI means what? | Owning repo/docs definition and adopted strategy target; measured values come from native owner evidence |
| What does an operational object currently say? | Fresh authorized domain read; future Ontology read under current admission, with source revision/coverage/freshness |
| May a caller read, derive or act? | Authenticated domain policy and actual task grant; neither memory nor tags grant access |
| Did an effect happen, and is retry safe? | Authoritative target reconciliation and durable domain effect state; neither prose nor binary rollback settles it |
| What was explained to the user? | Dated glossary evidence and feedback; an explanation does not establish mastery |

When memory and current source disagree, preserve dated history and use an
authorized scoped Correction; do not rewrite one store to make it agree with the
other. Unknown/stale evidence remains unknown/stale. Current truth still needs
native readback even when a page reports `is_stale:false`.

## Read and official write routes

Search relevant pages before new work, bugs, tests/conventions, integrations,
why/remaining-work questions and commits. Read only selected pages, then recall
exact evidence if needed. Reflect only when pages/recall cannot resolve reasoning
or conflicts. Credit useful retrieved memory. Do not search on every tool call.
If the interface is unavailable or disabled, disclose the gap and use the task's
source packet; do not start a client or change global configuration to orient.

Structured incidents/lessons, decisions, Corrections, profiles/glossary, strategy
and KPI-definition records use official HTTP/client with explicit stable
`document_id`, then complete-tag-set PATCH and document GET readback. MCP
`ingest_document(title,content)` derives its ID from title and has no arbitrary
ID/tag/frontmatter arguments; use it only for simple unstructured notes.
`capture_initiative` is for approved initiatives, reusing the existing page on
material change. The endpoint/token-file locations and exact safe sequence are
in [hindsight-api.md](../skills/lessons-loop/references/hindsight-api.md).
No-write tasks leave an allowed local handoff; they do not ingest, PATCH, refresh
or delete. A skill is not a write grant.

Preserve pinned originals and their content/source pins. Version mutable new
records through same-ID complete-body replacement with a recoverable preimage;
use linked Corrections/new versions for pinned sources. Complete-tag-set PATCH
replaces tags and may requeue co-sources. Document history is not a text backup.
Read original document, extraction/consolidation, page scope/content and recall
separately; acknowledgement or a queued refresh proves none of those outcomes.
Reconcile uncertain effects before retrying through supported official surfaces.

## Shared provenance and derived views

Use genuine repository and platform provenance. Platform values are `agent`,
`education`, `ontology`; modules carry their owner's platform plus repository.
Cross-platform work may carry several applicable tags. Preserve genuine
`source:*`, `harness:*`, `project:*` and historical tags; never impersonate plugin
attribution. Direct retain callers supply expanded tags; client `retainTags`
templates are not server defaults. Keep bank routing under its actual owner.

Tags/source queries route topics, not privacy or permission. Strict tagged scopes
can omit shared untagged observations. Where authorized, non-strict `all`
includes them; verify actual coverage/readback before claiming source scope.
Do not change consolidation/routing simply to make a query appear successful.

Lessons ledger, Structural risk review, User glossary and Conventions and patterns
are proposed official derived views, not assumed installed. Authorized curators
use official page/model configuration and affected refreshes; plugin-managed
queries belong in official `pages`/`customPages` because sync can overwrite API-only
edits. Do not seed duplicates or synthesize new evidence from sibling views.

Keep retrieval, retention, extraction, server synthesis and loaded agent context
separate. `pageRefreshEveryTurns:5` is client roster/tool-guide refetch/reinjection,
not server synthesis cadence. A page refresh does not rewrite a running agent's
context. Native hook settings are read per invocation; persistent MCP settings
normally apply at process startup. Codex/Claude hooks, agy PreInvocation/Stop and
Muse's MCP-only wiring differ; registration is not proof of injection/retention.
Use current native wiring, not a CLI-name assumption.

## Anti-duplication and domain boundary

Write once at the proper owner. Codebase docs define contracts; Hindsight stores
why, selected durable knowledge and the source locator. The glossary links the
scoped profile instead of copying it. Never re-ingest a generated page as source.

Academy lesson records are the one exception (user decision 2026-11-06): the academy
server removes old rows, so Hindsight archives dated teacher statements and source
observations under real student names, as education-platform
`content/skills/academy-student-context/references/lesson-archive.md` specifies,
excluding credentials, answer-key literals and parent contact details. An archived
observation is a dated snapshot, not live state; a fresh owner read wins while
the server still shows the row. Never retain credentials. Apart from that
archive, do not retain raw student/exam/lesson records, operational rows,
action receipts/journals, KPI measurements/time series, full STATE copies,
pane/model/quota assignments, CI logs, repeated transcripts, executable schema
copies or a second current-state table in Hindsight. Keep detailed artifacts at
their owner; retain only permitted locators/hashes and a reusable student-free
lesson. Historical aggregate engineering closure metrics are allowed evidence,
not a live operational replica. Never infer user ability or broad permission.

Promote lessons by independent occurrence/mechanism evidence, not document count.
Route verified L3 proposals through Lead to STATE `DIRECTION PROPOSALS` and the
user; no promotion or memory write enacts architecture or policy.

Respect [ontology-platform's Hindsight boundary](https://github.com/park-kyungchan/ontology-platform/blob/main/docs/hindsight-boundary.md):
Ontology owns source-qualified operational state, admission, named actions and
recovery; Hindsight owns contextual decisions/lessons. Operational facts remain
(except the dated Academy lesson archive above) with their domain owner until a separately authorized implementation/cutover.
Do not assume a production Ontology endpoint. There is no shared DB, state replica,
automatic prose-to-object promotion, provenance service or ingestion pipeline
created by this reference. `ap` does not require a running Ontology.

Grok Build is the fifth runtime as of 2026-11-07. Its native MCP registrations and canonical guidance pointer are qualified separately in runtime-wiring.md. Do not infer a Grok compaction ceiling from another CLI's flags; qualify the installed native control before claiming the same threshold. The shared task/data/authority boundaries apply to all five.
