---
name: herdr-hermes
description: Control and verify native Hermes sessions through Herdr, including SSH or Docker backed terminals, with explicit profile and stored-session identity. Use with the existing herdr skill when the user requests Hermes terminal work; preserve native runtime and authentication ownership.
---

# Herdr–Hermes

User instructions and existing authorization take precedence. Dated restrictions from the recorded evaluation do not become blanket restrictions for other tasks. This is a thin extension of the installed `herdr` skill. Read that skill first and require `HERDR_ENV=1` before inspecting or controlling Herdr. Use its installed-CLI discovery and terminal mechanics; do not implement a second agent loop, scheduler, MCP client, tool registry, or credential bridge.

Read [control.md](references/control.md) for Hermes launch, correlation, interruption and recovery. Read [deployment.md](references/deployment.md) when selecting local/native versus SSH/Docker control. Read [documentation.md](references/documentation.md) when using Runtime Index routes or reconciling documentation with the installed release.

For post-turn self-improvement, background skill/memory writes or source-pinned skill drift, read [native-learning.json](references/native-learning.json). It separates official guidance, version-qualified behavior and required native observations; Herdr adds no Memory engine.

## Essential decisions

- Reconcile inherited IDs against live inventory and foreground argv. Use the verified explicit pane ID or live agent name; stale caller context is not an authority. Preserve user focus. Control only resources authorized for this task, and never treat read access to another session as permission to send input.
- Choose the profile and fresh-versus-resume intent explicitly. Resolve the actual home, launcher, release, UI and side effects before execution. A new profile can register an s6 service and notify gateways; a new session can run hooks, maintenance and auxiliary model calls. Isolation must cover those effects, credentials and caches, not just conversation text.
- Before a new `agent start`, resolve the executable in the actual launch executor. For an existing genuinely recognized local or remote agent, use its live server-scoped identity; before input verify foreground ownership and readiness. A local executable is not required to read, attach to or prompt an already-running remote agent through a supported remote Herdr API. Follow the base skill’s remote selector rules. Kind-list membership, manual `report-agent`, a terminal title and input admission are insufficient.
- For an SSH/Docker process that Herdr does not recognize, use the authorized raw pane surface. Keep authentication in native SSH/Hermes mechanisms. Do not manufacture recognition or infer setup permission from failed detection. Separately authorized maintained installation or integration is allowed; establish actual native recognition and readiness afterward before claiming support.
- Bound model work using native capabilities that preserve existing model/profile/permissions. Inspect installed `oneshot` semantics: the observed release internally enables YOLO, so it is unsuitable when approval bypass is forbidden. If native isolation is unavailable, defer the affected execution and continue read-only validation and skill work.
- A successful result needs the submitted correlation marker, actual Hermes response, actual stored session key, and observed native call/usage evidence where available. Keep frontend IDs separate. Unexposed native Task/Run/call IDs stay `null`; never fill them with logical Work IDs or invented values.
- After uncertain submission, timeout, idle/done or connection failure, read the existing state before any retry. An interruption signal is not evidence that tools or provider calls never executed.
- Hermes owns models, loop/retries, tools, sessions/profiles, approvals and credentials. Context injection does not grant execution permission or prove an execution veto. Inspect installed hook return semantics and actual store writes independently.
