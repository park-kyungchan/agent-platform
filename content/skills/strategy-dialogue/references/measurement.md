# Measurement: terms and how to build a KPI

The user rarely names a KPI. The Lead derives it from what the user said they want, measures it, and shows it in a form the user can correct.

## Terms

| Term | Meaning | Must be written with |
| --- | --- | --- |
| Objective | Core term: `/root/.agents/AGENTS.md` section 8 | Who or what changes, and how one would see it |
| Diagnosis | The measured cause of the gap between the present state and the objective | The evidence; assumptions marked as assumptions |
| Metric | A number computed by a fixed formula from a named source at a stated time | Formula, unit, source, time |
| KPI | Core term: `/root/.agents/AGENTS.md` section 8 | Everything a metric has, plus baseline, target, the direction that is good, how often it is measured |
| Baseline | The value of a metric before the work | Source and time |
| Target | The value at which the objective counts as reached | Who set it and why that value |
| Guardrail | Core term: `/root/.agents/AGENTS.md` section 8 | The limit and what is done when it is reached |
| Leading indicator | A metric that moves during the work and can still change the course | What it predicts |
| Lagging indicator | A metric that shows the result after the work | How long after |
| Output | The thing the work produced: a merged pull request, a file, a report | — |
| Outcome | The state that changed because of the output: sessions load the new text, a check runs faster | — |
| Proxy | A metric used in place of something that cannot be measured directly | What it does not capture |
| Acceptance criterion | A yes-or-no check that decides whether one piece of work is complete | The command or readback that answers it |
| Evidence | The command output, file or record that shows a value | Path or command, and time |

The three core terms are defined once, in `/root/.agents/AGENTS.md` section 8; their rows above add only what each must be written with.

A KPI measures an outcome, not an output. "Three files rewritten" is an output; "sessions that loaded the rewritten file, out of sessions started" is a KPI.

## From the user's words to a KPI

1. Write the objective as a checkable sentence. "Clean this up" becomes "no agent opens a file under the retired path".
2. Ask what would be different in the world if the objective were reached, and where that difference can be read: Git, the hosting provider's CLI, CI results, session records, a database, `df`, a test.
3. Write the metric: formula, unit, source. A rate needs its denominator; a count needs its window.
4. Measure the baseline now. If it cannot be measured yet, write "unmeasured" and make measuring it the first piece of work. Do not invent a value and do not skip the KPI.
5. Choose the target. Prefer a value with a reason (all, none, a limit the user named, the level of a comparable case) over a round number. Record that the Lead set it.
6. Add a guardrail for each way the KPI could be raised by harming something else: a KPI on removal gets a guardrail on what must keep working.
7. Tell the user in one line per KPI: what it counts, where it is read, which direction is good, the baseline and the target. The user corrects in their own words; section 6 of the skill says how to take that.

## Checks before adopting

- At most three KPIs for one objective. More than that and none of them steers.
- Each KPI can be measured by a command or a readback that another agent could repeat.
- Each KPI has at least one guardrail, or a written reason why it needs none.
- A proxy says what it misses. Bytes of instruction text stand in for the cost of reading it; they miss how often the text is read.
- A metric that only the worker's own report can supply is not evidence; read it from the source.
- A KPI definition that will outlive the task goes into the owning repository's documents; the values stay in the task's files.

## Reporting a value

```text
<KPI name> — <what it counts>, read from <source> at <time>: <baseline> -> <current> (target <target>; <higher|lower> is better); <observed | estimate | unmeasured>
```

## Example with invented values

```text
User: "agents keep touching old files, tidy that up"
Objective: no agent session opens a file under the retired directory.
Metric: sessions with at least one read of the retired directory / sessions started, per day; source: session records.
Baseline: unmeasured -> first work: count over the last seven days.
Target: 0 of all sessions (set by the Lead: the directory has no current consumer).
Guardrail: every path a live service reads still exists (checked by the service's own start-up check).
```
