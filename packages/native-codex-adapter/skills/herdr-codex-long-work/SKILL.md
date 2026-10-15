---
name: herdr-codex-long-work
description: "Maintain a native Goal, plan, and source-owned continuity records when the user entrusts long, complex, unattended Codex work in Herdr. Apply automatically to that scope, including orchestration, compaction, interruption, and handoff. Ordinary one-off work does not create a Goal. Herdr controls require HERDR_ENV=1 and the existing herdr skill."
---

# Herdr Codex long work

Use native Codex to carry the entrusted outcome through investigation, implementation, verification, and delivery. Preserve the selected model, reasoning effort, permissions, and project ownership. This skill supplies continuity instructions; native Codex retains its agent loop, Goal lifecycle, delegation, sessions, and compaction.

## Select the actual work

1. Read the owning project's current purpose, actual user decisions, selected Work, latest checkpoint, and holds through its bounded entrypoint. Select the newly entrusted Work explicitly; a stale default Work is not the current task.
2. Apply the [standing user decision](references/standing-user-decision.json) only when this user entrusts sustained, complex work intended to continue unattended. The user need not repeat requests for a Goal, plan, or continuity. A short explanation, ordinary edit, or brief review does not qualify merely because it is difficult.
3. Preserve explicit stop, pause, manual-resume, effect, evaluation-budget, and source-currentness boundaries. This standing decision does not resume held Work, repair an unrelated provider drift, change hooks/configuration, or widen authority. Ask only about consequential uncertainty that current evidence cannot resolve; continue independent authorized work where possible.

## Bind a native Goal and plan

- Call `get_goal` before considering `create_goal`. Reuse the existing active Goal for its current scope; do not create a competing Goal, silently broaden it, or mark it complete to free the slot. If an unfinished Goal belongs to different work, preserve it and record the scope conflict before any dependent dispatch. Do not infer a resume of a paused/blocked Goal from this standing decision.
- If no unfinished Goal exists and the qualifying user instruction applies, call `create_goal` without another permission question. Keep `objective` at most 4,000 characters: desired outcome, verification, constraints, allowed scope, and stopping conditions. Set `token_budget` only when the user explicitly provides one. Existing budget/usage limits remain in force.
- Use native `update_plan` for concrete phases and keep it aligned with the selected checkpoint. Choose the next useful authorized action from current evidence. If Goal/plan tools are unavailable, record that limitation and use the owning project's continuity record; do not emulate the runtime with a scheduler or model loop.
- Follow the callable native Goal tool's exact status rules. Complete only after the objective and required verification/delivery are fulfilled. Do not mark completion because time/tokens are low or work is stopping. Pause only on an actual user pause instruction. Use `blocked` only after the tool's consecutive-turn threshold, with no meaningful independent progress possible. A budgeted Goal's completion report includes the tool's final usage.

## Delegate and wait

- Allocate bounded independent work when useful. Create workers on demand, with explicit scope, output, ownership, and effect limits; there is no mandatory roster or pane count. Native internal subagents remain allowed. A separate CLI pane and a native child thread are different runtime identities.
- Before any Herdr control, verify `HERDR_ENV=1` and apply the existing `herdr` skill. If the guard fails, do not inspect or control Herdr; retain authorized native work. Follow installed help, live machine/session/pane IDs, caller context, and `--no-focus`. Do not derive identities from pane order or reuse another server's IDs.
- Prefer native completion notifications and Herdr state waits to repeated status prompts or alarm-driven polling. Use bounded waits compatible with timely user updates; do useful independent work while workers run. A timeout, `unknown`, settled UI, or accepted prompt is not proof of dispatch failure or completed work. Inspect existing state and correlated output before any retry.
- Do not run the declined direct pane-to-pane messaging test. Avoid creating or starting another agent merely to check status.

## Write a bounded checkpoint

Use the selected project's existing typed checkpoint contract and CLI first. Do not replace or bypass a held source provider. If that path is unavailable, record the reason and keep a separate, clearly scoped English JSON/TypeScript checkpoint in the selected project's authorized Work directory, using the [fallback contract](references/checkpoint-contract.json). This is a work record, not a native session replica.

Preserve intent and success criteria; actual authority and exclusions; current phase/plan; source revisions; observed runtime and delegation IDs; in-flight effects; completed/consumed effects and receipts; verification versus acceptance; holds; and the next safe action. Use references to large evidence instead of transcripts, credentials, full environment dumps, or copied session stores. Keep the readable record within 16,000 characters; archive/reference older details without dropping unresolved effects or holds.

Update at material decisions, delegation, verification, long waits, interruption, handoff, and before compaction when practical. Read back the saved file and preserve its version/hash. Where the native runtime exposes no pre-compaction boundary, the latest material-boundary record is the recovery source. A checkpoint must not claim that an accepted tool call or source test proves live behavior or user acceptance.

## Recover and finish

After compaction, reconnection, or handoff, reread the selected Work's latest checkpoint, actual decisions, receipt summaries, and current holds. Re-observe relevant source revisions, native Goal state, process/container incarnation, and live worker/pane/thread IDs before steering them. Reconcile the plan with those observations; do not restart from a remembered summary alone.

Never replay completed/consumed dispatches or dispatches with an uncertain outcome. Audit the original correlated receipt and live state read-only first. Lost output, disconnects, or missing workers do not establish that an effect never happened; record uncertainty and its next safe observation. A justified new attempt is a new bounded action after reconciliation, with its own identity and authority.

Continue within native lifecycle and authorized scope until verified completion or a real stop condition. Preserve explicit stop/manual-resume boundaries even if independent workers are still running; record their exact known state and handle only authorized owned shutdowns. Report artifacts, verification, remaining limitations, and any required user action concisely. Herdr Server/host/container liveness, authentication, network, input, and budgets still matter: this skill does not prevent compaction or guarantee that a multi-day process survives.
