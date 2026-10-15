# Native terminal entrypoints for Herdr

From WSL2, attach with `herdr --remote srv1873899`. The compatible
`srv1873899-herdr` alias reaches the same persistent server. The SSH transport
uses the trusted Tailscale SSH endpoint and VPS loopback2222.

`codex-orchestrator` starts a blank native Codex TUI with gpt-6.1-sol and
reasoning high; native configuration supplies context872000. It submits no task.
The Orchestrator reads the persistent Home instructions and uses the installed
Herdr skill for worker panes/tabs. `continue-harness-work --help` describes the
prepared logical Work; invoking it without arguments manually starts a new
native session with that Work's continuation prompt. Preparation does not invoke
this continuation command. An existing Orchestrator can instead read
`~/operations/herdr-session-handoff-20261015/continuation.json` when the user
explicitly asks it to continue.

`hermes-backend` opens the installed native Hermes TUI inside the existing
`hermes-agent-2q3y-hermes-serve-1` container. Profile defaults to `default` and can
be selected with `--profile PROFILE`. UID/GID remain `10000:10000`. Native Hermes
resolves the selected profile and its existing store from the `/opt/data` root.

```text
Herdr pane
  -> native SSH to srv1873899 over Tailscale
  -> docker exec in the existing Hermes container
  -> hermes -p PROFILE --tui
  -> native tui_gateway.entry child for this TUI
```

The Windows Desktop client keeps its existing authenticated remote backend route.
This launcher creates a native terminal frontend in the same installation/profile;
it does not configure the terminal as a client of the Desktop port 9120 process.
It adds no custom agent runtime or WebSocket authentication extractor.

## Files and deployment

Keep `hermes-backend`, `hermes-backend-shell`, and `hermes-backend.lib.sh` together.
Place their directory on the intended Herdr server user's `PATH`. Native SSH must
already authenticate to `root@100.95.133.99` and trust the host key under alias
`srv1873899`. Provision that independent SSH authentication through the owning
deployment workflow. These files contain no credentials and do not install keys.

`HERMES_BACKEND_SSH_TARGET` can select an SSH-config alias for the same VPS when
the deployment uses a dedicated identity. `StrictHostKeyChecking=yes` and
`BatchMode=yes` are retained.

## Commands

```bash
hermes-backend
hermes-backend --profile dc-context
hermes-backend --native-help
hermes-backend --profile dc-context --native-help
hermes-backend --profile dc-context --profiles
hermes-backend-shell

# Only after explicitly selecting a stored session in that profile:
hermes-backend --profile dc-context --resume SESSION_ID
```

Profile names must contain 1-64 letters, digits, underscores or hyphens and start
with a letter or digit. Hermes performs native profile existence and resolution
checks. Profile selection does not change the executing UID or create profiles.

The shell launcher starts Bash without profile/rc startup files. It opens the
container shell only. The default TUI launcher does not request `--continue`,
`--resume`, a query, an image, `--yolo`, or a gateway restart. It clears inherited
startup-query/image and internal attachment variables for this child process.
Persisted settings, service configuration, Desktop routing, and credentials are
unchanged. Before a fresh TUI launch, the wrapper reads that profile's resolved
`display.tui_auto_resume_recent` through the native CLI. Any result other than
`false` refuses startup with exit 78 and requests an explicit `--resume` selection.
The wrapper does not alter the preference or silently resume the most recent chat.

Native per-session ownership checks remain enabled. An active owner can refuse
resume with `SESSION_NOT_OWNED`. The native launcher can cooperate with an owner
only when that owner advertises the supported attachment handshake. Do not force
takeover, remove locks, close other clients, or replay an interrupted turn.

## Read-only preparation evidence

The observed container includes Node `v26.7.0`, Python `3.14.7`, Bash, the native
`/opt/hermes/bin/hermes` launcher, and the prebuilt TUI bundle. The native launcher
chooses `hermes_cli/tui_dist/entry.js` before source-build fallback. No build is
needed for the observed artifact. If that bundle disappears, the wrapper refuses
startup and directs recovery to the owning Hermes image release workflow.

For the observed default profile, resolved
`display.tui_auto_resume_recent` was `false`. Installed source submits a startup
prompt only when `HERMES_TUI_QUERY` or `HERMES_TUI_IMAGE` is nonempty; this wrapper
clears both. Native startup can still create session metadata, initialize tools,
and discover configured MCP servers. The source-preparation observations in
`facts.json` predate deployment. Subsequent native startup verification is
recorded separately in
`runs/herdr-session-handoff-20261015/hermes-tui-startup-readback.json`: an owned
temporary tab rendered the native ready/input screen, no prompt was submitted,
and Ctrl+C returned to the shell before the owned tab was closed. This did not
instrument provider model traffic or establish Desktop gateway attachment.

The installed CLI also reported that its venv was out of sync with `uv.lock`.
That diagnostic was investigated read-only in
`runs/herdr-session-handoff-20261015/hermes-tui-artifact-diagnosis.json`.
The warning prints and continues; shipped image dependency stamps matched while
the preferred persisted dependency-selection ledger differed. Native TUI startup
succeeded. Reconciliation remains with the native Hermes installation owner;
the image was not repaired or rebuilt.

`facts.json` separates observations, source-derived behavior, and pending runtime
verification. Its adjacent JSON Schema defines the record shape. The owning agent
must independently verify the deployed pane, current container identity, native
TUI readiness, and absence of an unintended initial model turn.
