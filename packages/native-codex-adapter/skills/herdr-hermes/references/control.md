# Hermes control and evidence

## Preflight and launch

Use installed Herdr help/status, live agent/pane inventory and foreground process-info as described by the base skill. Verify client/server compatibility and the target machine. For SSH/Docker, the local process can be `ssh` while Hermes runs remotely; a saved Herdr machine profile is a different control path. Do not infer one from the other.

For a new start, resolve `command -v hermes` in the executor that Herdr will actually launch; this is not a local-executable gate for an already-running genuine recognized agent. Inspect the executable/wrapper and native parser before supplying arguments. Select a concrete profile, the UI (`--tui` versus classic `--cli`), working directory, and either a fresh owned session or an explicit owned stored key for `--resume`. Verify auto-resume is disabled for a fresh launch without changing settings. A failure does not authorize `--takeover`, trust bypass or cache clearing; honor actual user authorization and installed native semantics.

Before creating a test profile/home, inspect native creation and bootstrap:

- `profile create/clone` may seed config/skills/credentials, write alias/identity, register services and notify a gateway even with `--no-alias --no-skills`.
- An arbitrary `HERMES_HOME` must be supplied before the launcher imports bootstrap. Do not launch an existing profile with an intent to isolate it afterward. Scratch/cache maintenance and SessionDB maintenance can happen before a prompt.
- Native OAuth refresh and external-login adoption can write the original credential store or share a single-use refresh-token family. A credential metadata read or diagnostic `read_only=True` resolver is not a model execution isolation guarantee. Do not copy/link auth files or export credentials into a bridge.
- A packaged TUI artifact must already exist when installation/build changes are forbidden. The native launcher can otherwise install dependencies or rebuild source. A missing artifact is a hold, not permission to build it.

When these conditions hold for a local supported executable, an authorized shell pane can use:

```bash
herdr agent start <unique-name> --kind hermes --pane <owned-pane-id> -- -p <explicit-profile> --tui
```

Append `--resume <owned-stored-session-key>` only for an explicitly chosen resume. Verify the installed startup response, native `agent get/read`, foreground process and native profile/session display. Do not claim this example succeeded merely because `agent start` accepted arguments. In the observed deployment it failed because local `hermes` was absent.

For raw SSH/Docker control, use an already authorized launcher in an owned pane. The observed launcher syntax is:

```bash
herdr pane run <owned-pane-id> 'hermes-backend --profile <explicit-profile>'
# For explicitly authorized resume only:
herdr pane run <owned-pane-id> 'hermes-backend --profile <explicit-profile> --resume <owned-stored-key>'
```

Validate these launchers again in another environment. The initial isolated evaluation withheld TUI dispatch; the later authorized [actual-use test](live-use.md) and the October18 common-Work test exercised raw SSH/Docker Hermes TUI successfully. Those results establish this transport under their recorded conditions, not native Herdr recognition. Do not carry the earlier isolation hold into an already authorized later task.

## Readiness and input

Read the target before input. Require the expected UI and profile in the foreground, an actual ready composer and no unresolved approval/question. For a raw pane, Herdr `unknown` is expected and is not readiness. Do not answer a trust dialog or auth flow based on generic readiness heuristics.

For recognized Hermes use `agent prompt` with a bounded timeout, then read even if the wait reports settled. For raw TUI, send the text once with `pane send-text`, read the ready composer, then send `pane send-keys <pane> enter`. In the observed Ink deployment, a long `pane run` text-plus-Enter admission left the body in the composer; completing that same verified draft with one Enter started the turn. Reconcile the composer and native activity before any key-only completion, and never retransmit an uncertain body. Send one prompt with a unique marker within the authorized effect scope. Admission and input echo are weaker evidence than an assistant response. Do not resend because a wait stalled or a snapshot lacked the answer.

Before slash input, inspect installed full command names and surface semantics. Avoid ambiguous abbreviations. On the captured Ink source ambiguous best-tier prefix matches are refused; current public prose described first-match behavior. `/sessions` or `/switch` lists live process sessions; `/resume` concerns saved sessions. `/tasks` can alias the `/agents` overlay and is not evidence of a native Task ID. `/goal`, `/loop`, `/new`, `/clear`, `/reset` and `--delete` have stateful effects and require matching task scope. Do not use `/stop` generically: its classic and messaging meanings differ.

## Result and identity

Record separately:

1. Herdr server/machine, pane/name, foreground process and selected profile/home.
2. Exact prompt and marker, dispatch receipt, working activity, read response and terminal state.
3. Frontend session ID and durable stored key. Captured Ink uses `session_id` for live routing and `stored_session_id` or `session_key` for durable identity; exit/resume must use the latter.
4. Native response/call correlation and usage from the owned session only. Record unavailable call/Task/Run fields as null and state the missing observation.
5. Actual-model usage, subscription source and limit, independently verified when the claim needs it. A single turn can include retries, title generation or other auxiliary calls; do not equate one turn with one billable request. Qualify the installed normalizer and disjoint usage buckets before adding counters. In the observed0.22.0 source, bucket input excludes cache, reasoning is already within output, and session main counters duplicate the main bucket. The October18 test reported4main calls plus1title call:77,364+667=78,031 tokens. Preserve empty auxiliary billing fields and unknown usage; neither an estimated zero nor a subscription label is an invoice receipt.

Use a supported native read/export or selected read-only SQLite query only after confirming that the reader does not open a writable SessionDB or perform maintenance. Never query unrelated session bodies or infer identity from timestamp-shaped examples. A transport marker, CLI help, source audit, routing success, idle/done, timeout, or empty output cannot establish model completion or nonexecution.

## Interruption and recovery

Verify installed TUI keys and foreground identity before sending a cancellation key to an owned resource. Captured Ink turn cancellation requests `session.interrupt`; late output and persisted partials can still arrive. Confirm outcome by subsequent reads and owned native records. A terminal kill, `/stop` background-process action or interrupted badge does not prove a provider/tool effect was avoided.

After timeout/SSH loss: preserve the original marker and stored key, inspect pane process/UI, then inspect native durable evidence for that same owned session. Classify dispatch as confirmed, not-executed only with positive native evidence, or uncertain. Do not resubmit an uncertain effect. Native attach requires an actually recognized agent; a failed attach is not permission to use `--takeover` or manual state reports.

At closeout, read final state and honor the user’s retain/stop/close intent. Limit incidental cleanup to task-created disposable resources or existing resources explicitly authorized for closure; retain requested persistent workers. For a diagnostic raw shell with a no-existing-file-write contract, disable history persistence in that owned process before exit; do not edit global shell configuration. Return an owned raw shell to its parent shell before closing the pane. Do not delete saved sessions, profiles, existing panes or services as a cleanup shortcut. Record anything retained and why.

## Verified common-Work TUI use, October18 UTC

The bounded native Hermes test consumed the current canonical Git Work and G2 Memory through one terminal invocation, plus two native skill reads. Its persisted user API content includes the exact11,902-character hook context; the durable final response identified the withheld logical Task and Plan revision and proposed three concrete whole-scope actions. Prompt and tool output each omit exactly one final newline compared with the prepared/manual bytes; parsed tool JSON matches. The independent verification found zero unresolved findings. This proves that caller occurrence, not full document-body review, native authorization, comparable decision cost or whole-Goal Acceptance.

Its native user-message UID and terminal call ID are observed event anchors. If a common Work receipt uses them as a source-owned occurrence join, state the normalization and keep unexposed native Task/Run/backend Attempt resource IDs null. A common join does not authenticate native authorship or confer permission.

The installed Ink source supported `/quit` for the owned session; after its final result and usage were durable, one verified `/quit` ended that session with `tui_shutdown`, stopped its owned children and preserved the stored transcript. The returned parent shell was observed before the owned pane was closed. Recheck installed exit semantics before another use; do not add `--delete` or stop a shared service.

The observed launcher exit summary displayed78,421 by adding1,057 reasoning tokens to the77,364 main subtotal and omitting667 auxiliary tokens. That display is not the qualified78,031 total. The source-only finding/fix was not installed; inspect the current installed implementation rather than treating this dated behavior as universal. Actual session/message/tool evidence, independent verification, final usage reconciliation and cleanup receipts remain under the owning Work evidence directory `odi05-native-memory-caller-actual-v1/`. They do not authorize replay or automatic resume.

## Bot Mode and per-profile Screen

Use the existing Runtime Index `hermes.agent` topics `bot-mode` and `bot-screen` before relying on public instructions; their bounded partial-page evidence is described in [documentation.md](documentation.md). The native Hermes Desktop Bot connection and profile selection belong to that client/runtime. Reconcile the actual selected connection/profile and current user intent; preserve an already selected/open Screen rather than starting a new connection or repeating a settled interview. Do not export private Gateway URLs, viewer tickets/capabilities or credentials into a portable registry or custom client.

The October18 pre-update native CLI source supports `hermes -p <actual-selected-profile> computer-use screen status --json`; read its payload because exit1 can mean stopped, unsupported or missing packages. `hermes -p <same-profile> computer-use screen start` has no `--json` flag in that parser and allocates native per-profile resources. Starting the backend and opening its native Desktop Screen viewer are separate actions. Use only the existing native route within the authorized task; start is not an effect-free status probe.

The separately inspected pre-update source places a profile-scoped Xfce/Xvnc desktop in the native runtime's chosen gateway or terminal sandbox. A missing or stopped published screen must not fall back to a base `DISPLAY`. The existing human/agent lease owns control: corrupt/unreadable lease state fails closed to human, and public lease status exposes a hash rather than the viewer capability. Never force takeover, clear that lease or stop a foreign desktop as a diagnostic shortcut.

Backend readiness or a backend screenshot proves neither Windows rendering nor useful visual E2E. Correlate the actual native Desktop viewer and selected profile/placement/lease with a permitted synthetic GUI action/result and client-visible evidence. Retain native conversation/data/config owners and the user's requested visible state; cleanup is limited to owned resources and actual retain/close intent. These docs/source observations grant no native permission and do not establish an actual Windows end-to-end result.
