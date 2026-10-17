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

Validate these launchers again in another environment. In the recorded evaluation only the companion raw shell transport was exercised; Hermes TUI launch/dispatch was withheld because isolation was unavailable. This is a fallback control path, not a promise of native Herdr recognition.

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
5. Independent actual-model usage, subscription source and limit. A single turn can include retries, title generation or other auxiliary calls; do not equate one turn with one billable request.

Use a supported native read/export or selected read-only SQLite query only after confirming that the reader does not open a writable SessionDB or perform maintenance. Never query unrelated session bodies or infer identity from timestamp-shaped examples. A transport marker, CLI help, source audit, routing success, idle/done, timeout, or empty output cannot establish model completion or nonexecution.

## Interruption and recovery

Verify installed TUI keys and foreground identity before sending a cancellation key to an owned resource. Captured Ink turn cancellation requests `session.interrupt`; late output and persisted partials can still arrive. Confirm outcome by subsequent reads and owned native records. A terminal kill, `/stop` background-process action or interrupted badge does not prove a provider/tool effect was avoided.

After timeout/SSH loss: preserve the original marker and stored key, inspect pane process/UI, then inspect native durable evidence for that same owned session. Classify dispatch as confirmed, not-executed only with positive native evidence, or uncertain. Do not resubmit an uncertain effect. Native attach requires an actually recognized agent; a failed attach is not permission to use `--takeover` or manual state reports.

At closeout, read final state and honor the user’s retain/stop/close intent. Limit incidental cleanup to task-created disposable resources or existing resources explicitly authorized for closure; retain requested persistent workers. For a diagnostic raw shell with a no-existing-file-write contract, disable history persistence in that owned process before exit; do not edit global shell configuration. Return an owned raw shell to its parent shell before closing the pane. Do not delete saved sessions, profiles, existing panes or services as a cleanup shortcut. Record anything retained and why.
