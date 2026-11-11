# Brief, options, proposal and record formats

Copy the block you need. Values in angle brackets are placeholders; a filled block lives in the task's STATE, `STRATEGY.md` or board, never in this file. Terms: `/root/.agents/skills/strategy-dialogue/references/measurement.md`.

## Brief (the Lead's record of its interpretation)

```text
The user said: "<their words>"
Objective: <the state wanted, as a sentence that can be checked true or false>
Diagnosis: <measured cause with its evidence; assumptions and unknowns named>
KPI: <what it counts; formula and source> <baseline at time, or unmeasured> -> <target and why that value>
Guardrails: <limits that must hold while the KPI moves; what must not be done>
Domain: <facts only the user supplied; facts still missing and how the Lead proceeds without them>
```

## Options (only where a real choice of route exists)

| Option | Predicted KPI effect | Guardrail effect | Risk or unknown | How it is undone |
| --- | --- | --- | --- | --- |
| <chosen> | <estimate and its basis, or unknown> | <bound> | <main risk> | <route> |
| <alternative> | <estimate and its basis, or unknown> | <bound> | <main risk> | <route> |

Chosen: <option>, because <the measurement that decides it>.

## Proposal the user did not ask for

```text
Proposal: <what>
Evidence: <measured value with source and time, or file and line>
Expected effect: <estimate, labelled as an estimate>
Cost or risk: <what it uses or could break>
Undo: <route>
Status: <added as a work thread of the current objective | a different objective, offered to the user>
```

## Decision record (latest per topic)

```text
<date> Lead: <decision>; reason <measurement>; undo <route>; supersedes <decision or none>
<date> user: "<their words>" -> changes <intent | KPI or target | guardrail | way of working | fact>; reach <this task | every task>; written in <file>; supersedes <decision or none>
```

## Kept for the user

```text
Words the user used: "<expression>" -> <glossary term and path>
Fact from the user: <fact> (<date>)
Standing go-ahead: "<their words>" (<date>) covers <class of action>
Waiting for a yes: <prepared irreversible outward action> | recommendation <do it | do not> | effect <on which KPI>
Reminder owed: <what> | when <condition that triggers it> | context <file>
```
