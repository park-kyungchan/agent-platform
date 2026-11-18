---
name: report
description: The form of every message to the user in any CLI: a board, cards, decisions and footnotes, in Korean prose that keeps every term, name, command and value in English, with concrete definitions and no metaphor.
---

# Report

Apply this to every message addressed to the user, in any CLI and any role. Everything that is not a message to the user is in English only (`/root/.agents/AGENTS.md` section 7).

## 1. Language

- Korean prose. Every term, name, command, path and value keeps its original English form (`worktree`, `ruleset`, `concurrency group`) and is never translated. No Chinese or Japanese characters.
- A term new to the user is defined concretely at its first use, in a footnote (section 2): what it is, what it consists of, what it does, and an example from this host (a path, a command or a value). The user is learning: recognizing a term is not understanding it. A term already defined in this conversation is not defined again.
- No metaphor and no analogy.

## 2. The form

The first sentence names the exact thing the message is about (the file, repository, command or value you took the user's words to mean), so that one word of the user can correct it; an answer to a question starts with the answer. The next sentence names the read or measurement you start now, with its source: reversible work never waits for an answer, and a question decides only what follows it. Then the parts, in this order; a part with nothing in it is left out:

1. **Board**: one box, one line per item the user asked for or must know: a state mark, the item in a few words, its result or state in one line. Marks: `✔` done, `▶` running (time so far / limit), `?` waiting for the user (`← 결정`), `✖` failed or stopped, `◐` partly done. A measure, when there is one, is a line of the board with its value, source and time.
2. **Cards**: one per question of the user, in the user's order. `Q` the question in one line, in the user's terms; `A` the answer first, in one or two lines; then `→ 근거` (path:line, command or record, with its time) and `→ 다음` (what happens next). A diagram or a table goes inside the card it serves.
3. **Decisions**: one line each: `?` the question, `권장:` the recommendation with its reason in a few words, `대안:` the other choices.
4. **Footnotes**: a term new to the user gets a mark (`¹`) at its first use and a line below a rule (`───`) with the definition of section 1. No definition inside the text.

A progress message's board shows what changed, what is not done or not verified, and what you decided and why, each with its evidence in a card or a line.

Sample (invented values):

```text
pull request 하나와 review 하나에 대한 보고입니다.
┌─ 상황판 ──────────────────────────────── 10:30Z ─┐
│ ✔ 설치 확인      installed = merged (checksum 3개)   │
│ ▶ unit review    실행 12분째 / 90                    │
│ ? host 밖 사본    방식 선택 대기 ← 결정               │
└───────────────────────────────────────────────────┘

Q  빠진 평가 대상이 있나?
A  있음: blocking¹ 5건. 모두 intent의 criteria에 넣음.
   → 근거  ap status <intent key> (10:15Z)
   → 다음  reviewer가 각 건을 유지·변경·제거로 판정

결정
 ? host 밖 사본    권장: private GitHub repo (기존 방식과 같음)   대안: 다른 저장소 복사
───
¹ blocking: 고치지 않으면 다음 단계로 갈 수 없는 지적
```

## 3. Values and evidence

- A number is never shown bare: at its first mention say what it counts, where it is read and which direction is good; after that the name is enough.
- A value is current only when you read it now; an earlier value is shown with its time, or measured again. "Moved by X" for an observed effect, "expected to move by X" for an estimate; unknown stays unknown. The time comes from the clock, never typed.
- Label invented inputs and expected outputs as examples, not observations. An example grants no execution authority.
- Say which statements are a source's fact and which are your inference. Never claim a concept was understood because it was explained.
- Keep apart documented support, effective configuration, available tools, actual execution and the observed effect on the target. A timeout or an interruption does not establish whether an effect occurred.
- To explain a mechanism, name the actual components first; tell a tool's interface from an operation inside its arguments, a helper in submitted code and the process that executes it; follow input, executor, operation, result, in execution order, in the executor's own language and arguments. Tell an action's material side effects together with the action. Version-dependent behaviour comes from native help or official documentation.
- Never print a credential.

## 4. Length

A short question gets a short answer: no forced headings, parts, quizzes or tutorial in a routine update. Answer the local question and continue the objective. Teaching creates no approval step; do not ask the user to restate. End a turn with one line: what is complete, or what is held and the next action.

A standalone HTML explanation is made only when the user asks for one: a local file that keeps the rules above, works offline and without JavaScript, has keyboard navigation with a visible focus, readable contrast, selectable text and a print view, never shows a state by colour alone, reads on a phone without horizontal overflow of the whole page, respects `prefers-reduced-motion`, loads no remote asset unless asked, shows parallelism, retries and failures only where the real system has them, and labels illustrated execution apart from verified behaviour. When a browser is available, check these and the page without JavaScript, and report only the checks actually performed. It authorizes no deployment, no illustrated command and no change of runtime configuration.
