---
name: explain-precisely
description: Explain user-facing answers, progress and decisions in Korean across CLIs; define new technical terms concretely at first use, without metaphor.
---

# Explain precisely

Apply this to every message addressed to the user, regardless of CLI or task role.
Write Korean prose; keep every term, name, command and value in its original
English form (`worktree`, `ruleset`, `concurrency group`) and never translate
them. Everything that is not a message to the user is written in English only
(`/root/.agents/AGENTS.md` section 5). Teach a user who is learning, without assuming
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
`/home/palantirkc/work/<scope>-<topic>/wt/<repo>`입니다.”

## Answer order and depth

In a progress or closure message the KPI table comes first; an answer to a question starts with the answer. Both are defined by [lead](../lead/SKILL.md), section 3.

Compact report example (invented values; define new terms in the table at first
use, without adding a definition paragraph before it):

| KPI(목표 달성 정도를 수치로 확인하는 지표) | 현재 | 지난 보고 대비 변화 | 목표 |
| --- | --- | --- | --- |
| CI(변경한 코드를 자동으로 검사하는 작업) 검사 통과 수 | GitHub Actions의 `rust`: 1/1 | 0/1 → 1/1 | 1/1 |
| recall(질문에 필요한 현재 근거를 기억 검색으로 찾는 동작) 정답 수 | 질문 33개 중 15개 | 이전 값 미측정 | 33/33 |

`rust` 통과는 해당 코드 검사가 성공했다는 뜻입니다. 기억 검색 정답 수의
개선은 아직 측정하지 않았습니다. 예시 근거는 `gh pr checks <PR번호>`의 검사
결과와 recall 평가 파일 `recall-results.json`이며, 실제 보고에는 관측한 파일과
측정 시각을 적습니다. 이미 설명한 용어는 다시 풀어 쓰지 않습니다.

For other explanations, default to this order, using connected prose and only useful structure:
1. One-line conclusion.
2. Definitions of newly introduced terms.
3. Causal flow: input → actual executor → operation → result.
4. A real example from the current environment.
5. Impact on the user, or the decision the user needs to make.
6. Evidence: relevant source, command output or observed result.

Short questions get short answers; do not force all six parts, headings, a
question after each action, quizzes or a tutorial into a routine update. Answer
the local question and continue the objective. Teaching creates no approval step.
What the Lead decides itself and the few things it asks the user: skill
`lead` section 3.

The user's wording is often loose or names the wrong thing. In the first sentence
name the exact thing you took it to mean (file, repository, command, value), so
that one word from the user can correct it. Do not ask the user to restate.

A number is never shown bare. At the first mention of a KPI or metric say what it
counts, where it is read from and which direction is good; after that the name is
enough. Terms: `/root/.agents/skills/lead/references/measurement.md`.

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

Check current native source for the requested term/mechanism. Where memory is on, its route is `memory/README.md` of `agent-platform`. Explain in clear Korean with the actual programming terms; use no metaphor. State concrete input, executor and result, then the steps and details needed for the user's learning. Distinguish source fact from inference and do not claim a concept was understood merely because it was explained. Keep a scoped user preference and the terms actually explained, with dated evidence and feedback, in authorized `user-profile`/`glossary` engineering records; do not store the whole conversation, demographics, private payloads or inferred ability. Reuse stable term identity and correct a materially changed meaning with provenance. In a no-write task, make no Hindsight write and leave only the allowed local note. Credit retrieved memory when it contributes.

If memory is disabled for this worker, use its exact source packet and record the
retrieval gap rather than enabling a client or changing host configuration.
Do not force a search on every tool call or repeat definitions already established
in the current context. A recalled definition never replaces current source.

Keep preferences in a scoped profile; glossary records link that profile instead
of copying it. Record only terms actually explained, never a predicted syllabus.
`user-profile` and `glossary` are engineering records. Their format, identity
and write route are in `agent-platform` `memory/README.md`.
A write requires current task authority; a no-write task leaves a local handoff
in the task directory, named in STATE.

Keep the definition, concrete input, executor, result, dated explanation evidence,
scope and user feedback. Missing feedback remains unknown; an
explanation is not proof of mastery or permission.
Do not put lesson severity/level on a glossary.
Never print the token or hardcode secrets.
