# The interview and the intent

Read when a message starts or changes an objective, a word of the user is unclear, or you write an intent.

## 1. Hearing a message

1. Resolve what each word refers to, in this order: the table of section 6; what the conversation was just about; what exists on the host under the nearest name.
2. Connect it to the objective and the open work threads; add what the objective needs and it did not say.
3. Name the exact thing you took it to mean (file, repository, command, value) in the answer's first sentence, so that one word can correct it, and proceed. Ask only when two readings lead to different effects and no source decides; give both and the one you take without an answer.
4. A question is answered from source, unrecorded; a stop is a hold at once; one `plain` unit in one repository that a revert undoes is the lane `small`; anything else (several units, a contract, an irreversible effect, a new capability) is the lane `objective`: the interview.

## 2. The interview

Due at every new objective: one message, before the intent is drafted, of at most five questions, each asked only where neither the request, the current requests (`ap request`, `op` `list`) nor a read or measurement you can take now answers it, each with its recommended default:

1. Outcome: what is true when this is done, and how it will be seen (the criteria).
2. First use: who or what uses the result first, on which real case.
3. Edges: what is not included.
4. Risk: which effect could not be undone by a revert, or touches private data, a real external target or the login path.
5. Depth (only when it changes the plan): how far down, in which order.

The interview holds no work: its message first says what you took the request to mean, then asks with defaults, then says what you do now without an answer (the first read or measurement with its source; the draft intent on the defaults). An ambiguity you do not ask about goes into `questions` with the default you took. It ends when the intent's required fields are filled and never repeats. An idea not worth building ends with `retire` and the reason.

## 3. The intent, its criteria, its card

`ap intent` `draft` holds `outcome`, `first_user`, `origin` (the starting request), `why`, `not_included`, `constraints`, `questions` (at most five, each with a `default`) and up to 12 `criteria`, each a statement with its check: a command, a test, an eval with its threshold, a number with its baseline and direction, or a real run to observe.

- A criterion counts the result in use, never the task's own artifacts (pages, tests, merges), which may only be a denominator. A rate needs its denominator; a count needs its window.
- A baseline is measured before any change, or written as unmeasured and measured first. A target has a reason (all, none, a limit the user named).
- Each way a number could be raised by harming something else gets a guardrail criterion. A proxy says what it misses. A value only a worker's report supplies is not evidence.

The card, in the form of skill `report`: the outcome, the criteria, what is not included, the defaults taken. The user's answer is recorded as a request routed to the intent; `confirm` names it in `by`.

## 4. Assumed and delegated intents

- When the user is away, the Lead may mark a draft `assumed` (`assume`, with the reason): units of class `plain` may start; `contract` and `effect` wait for `confirmed`. The user sees each assumed intent on the next board with the mark `?`.
- A request of kind `delegation` hands the confirmation of named intents to the Lead while the user does not step in: `confirm` with that `delegation` and a `reason`; every class may start; each such confirmation is marked on the status print until the user saw it. The user's later word ends it: `end_delegation`.
- An agent given an everyday task is not a Lead: it drafts, asks and waits, or works in the lane `small`.

## 5. The order is taught in one message

> When a request asks for the output of a later stage while the record of an earlier stage is missing, name the missing stage in one line, ask that stage's questions with defaults, and say what you will do without an answer. One message, at most five questions. Not for a question, a stop, an incident, or work that is already in order.

The user's example: «학원 앱을 지금 당장 내 세션 데이터만 분석해서 만들어줘» (build the Academy app now, analysing only my session data). The answer names the missing stage (an intent: outcome, first use, what "my session data only" excludes), asks with defaults, and drafts meanwhile:

```text
"Academy 앱을 만드는 objective"로 읽었습니다. intent¹가 없어 그 질문을 기본값과 함께 드립니다.
1. Outcome: 끝나면 무엇이 됩니까? 기본값: 세션 기록의 수업 요약을 앱에서 봄
2. First use: 누가 어느 수업에 처음 씁니까? 기본값: 사용자, 다음 수업
3. Edges: "내 세션 데이터만"이 빼는 것은? 기본값: 학생 데이터, 다른 계정
답이 없어도 세션 기록의 형식을 읽고 기본값으로 draft intent를 씁니다.
───
¹ intent: 끝났을 때 참일 결과와 그 확인 방법의 기록
```

It does not rest on the model: `add_unit` in the lane `objective` without an intent is refused (`intent_required`).

## 6. How the user refers to things

| The user says | Usually means | Check before acting |
| --- | --- | --- |
| 하네스, Harness Engineering | The shared instructions, pointer files and how sessions load them | Not the retired repository of a similar name |
| 지침, 인스트럭션 | The shared instructions; sometimes a skill or a page | Which file |
| 런타임 레퍼런스, runtime-reference | `/root/.agents/runtime-references/` (plural) | — |
| 스킬 | A skill under `/root/.agents/skills/`, or one a single CLI installed | A misspelled name means the nearest one |
| 메모리 | STATE and its files ("단기"), long-term memory ("중장기"), or private memory ("CLI의 메모리") | Which of the three |
| 하인드사이트, HindSight | The long-term memory server and its MCP tools | — |
| 온톨로지, Ontology | Palantir's sense: object types, links, actions in `ontology-platform` | Not a taxonomy |
| REA, Ghidra Framework | The reverse-engineering tool at `/home/palantirkc/tools/rea`, Ghidra its backend | Static or dynamic; built? |
| Rust 전환, 마이그레이션 | Moving an implementation to Rust | Which repository and consumer |
| 컷오버, cutover | Moving a service's live consumers to the new implementation | Which consumers; rollback |
| 수업 세션 | A CLI session the user runs while teaching | Never send input to it or close it |
| 워커, 서브에이전트, 에이전트들 | Workers; sometimes every agent, Leads too | — |
| 새 Lead, 새 세션 | The Lead session that takes the task over next | — |
| 라우팅 | Which file an agent opens, or which model does which work | `AGENTS.md` section 8, or a unit's tiers |
| 컨텍스트 거버넌스, Context Governance | Which context each agent loads, from which home, in what order | `CONTEXT-GOVERNANCE.md` |
| reference-docs, 레퍼런스 문서 | The design sources under `/home/palantirkc/external/reference-docs/` | Only the lines `OFFICIAL-DOCS.md` section 4 names |
| 지표, 메트릭, KPI | A number the user wants watched | Write it as a criterion |
| VPS, 코드베이스 전체 | This host: repositories, task directories, shared instructions | `CODEBASE-ORIENTATION.md` |
| CI | The required checks of a pull request | Which repository |
| 보드 | The board of the current task | — |
| 정리, 손봐, 개선 | A request whose target and extent are open | Say first what you change and leave |

What a new expression resolved to is its request's reading (`ap request`); at its second task, a row of the glossary, by pull request.
