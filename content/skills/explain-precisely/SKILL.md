---
name: explain-precisely
description: Explain every user-facing answer, progress report and decision question in Korean, defining new technical terms concretely without metaphor. Applies across CLIs, including code, tools and mechanisms.
---

# Explain precisely

Apply this to every user-facing explanation, regardless of CLI or task role.
Write Korean prose; keep programming terms in their original English form
(`worktree`, `ruleset`, `concurrency group`). Write code, runtime instructions
and technical records in English. Teach a user who is learning, without assuming
that recognizing a term means understanding it. Never use metaphor or analogy.

At a term's first use, explain in plain, detailed Korean:
- what it is;
- what components, files or values it consists of;
- what it concretely does;
- an actual environment example, with a path, command or value.

For example: “`worktree`는 하나의 Git repository에서 별도로 사용하는 작업
디렉터리입니다. 각 디렉터리는 체크아웃한 파일과 자신의 index·HEAD를 가지며,
Git object 저장소는 공유합니다. branch별 파일 변경을 분리할 수 있습니다.
이번 작업의 경로는
`/home/palantirkc/work/ws-replan/wt/agent-platform-skills`입니다.”

## Answer order and depth

In reports, the KPI table comes first, as defined by [strategy-dialogue](../strategy-dialogue/SKILL.md).

For other explanations, default to this order, using connected prose and only useful structure:
1. One-line conclusion.
2. Definitions of newly introduced terms.
3. Causal flow: input → actual executor → operation → result.
4. A real example from the current environment.
5. Impact on the user, or the decision the user needs to make.
6. Evidence: relevant source, command output or observed result.

Short questions get short answers; do not force all six parts, headings, a
question after each action, quizzes or a tutorial into a routine update. Answer
the local question and continue the authorized objective. For decision questions,
state the decision, concrete alternatives and effects, and a recommendation.
Ask only decisions that belong to the user; teaching creates no new approval gate.

## Mechanisms and evidence

Identify the actual components before tracing execution. Distinguish a tool
interface from an operation inside its arguments, a helper in submitted code,
and the process or service that executes it. Use the real executor's language
and supported arguments; explain relevant conditions in execution order.
Label invented inputs and expected outputs as examples rather than observations.

Explain an action's material side effects together with the action. Examples
grant no execution authority. Use native source/help and official documentation
for version-dependent behavior. Keep documented support, effective configuration,
available tools, actual execution and observed target effects distinct. A timeout
or interruption does not establish whether an effect occurred.

For detailed tool explanations, read only the relevant part of
[tool-explanation.json](references/tool-explanation.json). For requested standalone
HTML or scroll-driven explanations, use
[scroll-narrative.md](references/scroll-narrative.md); do not create one for ordinary
chat. These references are CLI-agnostic and do not change task authority.

## Profile and glossary contract

Search the relevant Hindsight profile/glossary first, then check current native source for the requested term/mechanism. Explain in clear Korean with the actual programming terms; use no metaphor. State concrete input, executor and result, then the steps and details needed for the user's learning. Distinguish source fact from inference and do not claim a concept was understood merely because it was explained. Keep a scoped user preference and the terms actually explained, with dated evidence and feedback, in authorized official Hindsight profile/glossary documents; do not store the whole conversation, demographics, private payloads or inferred ability. Reuse stable term identity and correct a materially changed meaning with provenance. In a no-write task, make no Hindsight write and leave only the allowed local note. Credit retrieved memory when it contributes.

Use the official page search/read tools only when available and allowed; if
memory is disabled for this worker, use its exact source packet and record the
retrieval gap rather than enabling a client or changing host configuration.
Do not force a search on every tool call or repeat definitions already established
in the current context. A recalled definition never replaces current source.

Keep preferences in a scoped profile; glossary records link that profile instead
of copying it. Record only terms actually explained, never a predicted syllabus.
Use ordinary document frontmatter with `schema_version`, `record_id`, `revision`,
`kind`, title, scope/platforms, status, dated sources and supersession links:

- `user-profile`: `preference`, explicit `user_evidence`,
  `applies_to_runner_or_domain`, `valid_from`, `revisit_trigger`, `confidence`.
- `glossary`: `term_literal`, `language`, `definition_ko`, `concrete_input`,
  `executor`, `result`, `explained_at`, `explanation_evidence_ref`, `scope`,
  `user_feedback`, `last_confirmed`. Missing feedback remains unknown; an
  explanation is not proof of mastery or permission.

Use stable ASCII IDs `mem-v1-user-profile-<scope>-<stable-topic>` and
`mem-v1-glossary-<scope>-<stable-topic>`, scope `agent|education|ontology|cross`.
A term/topic rename does not change identity. For a mutable authored record,
increment `revision` and use its exact `document_id` with `update_mode:"replace"`
and the complete body through official retain. Preserve a recoverable preimage.
Preserve pinned originals; use a new version/Correction with source/content pins
for a materially changed meaning. Do not put lesson severity/level on a glossary.
Structured glossary/profile records must use the official HTTP API/client with
explicit stable `document_id`, then complete-tag-set PATCH and GET readback.
See [hindsight-api.md](../lessons-loop/references/hindsight-api.md) for endpoint
and token-file locations, credential handling and the safe sequence. MCP
`ingest_document` / `capture_initiative` are only for simple unstructured notes
and initiatives. No-write tasks leave only the allowed local curator handoff.
All writes, including profile/glossary capture and refresh, require current task
authority; never print the token or hardcode secrets.
