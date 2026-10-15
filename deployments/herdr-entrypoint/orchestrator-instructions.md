<!-- native-herdr-orchestrator:20261015 -->
## Herdr Orchestrator entrypoint

The user enters this persistent VPS server with `herdr --remote srv1873899` from WSL2. One user-facing native Codex CLI acts as Lead Orchestrator and communicates with the user. Use the installed Herdr skill for task-specific worker panes and tabs when they help the selected Work. Native CLIs retain their own agent loops and internal delegation; there is no fixed roster or second scheduler.

Read `~/operations/herdr-session-handoff-20261015/orchestration.json` for the accepted topology and `continuation.json` for the prepared Work. Preparation did not resume the paused source Work. Wait for the user's explicit manual continuation before executing its pending tasks. Its original thread is not portably resumed by copying native state.

For Herdr commands first verify `HERDR_ENV=1` and inspect the installed CLI syntax. Use actual IDs returned by this server and `--no-focus` for background layout. Inside this container use the inherited local Herdr socket; a local WSL saved-machine selector is not needed. Inspect foreground ownership/readiness before starting or prompting an agent. Start Codex workers with `--no-daemon` so every native process inherits its own pane context; use gpt-6.1-sol, reasoning high, context872000 unless the selected Work or user overrides them. Bounded internal native Codex delegation remains available.

Use `hermes-backend --profile PROFILE` in an available shell pane for the existing backend's native Hermes TUI, or `hermes-backend-shell` for its native shell. `hermes-backend --profiles` lists profiles; explicit `--resume SESSION_ID` is required to select an existing session. Respect live session ownership. The launcher independently authenticates from Herdr to the VPS and executes native UID10000 in hermes-agent-2q3y; it does not depend on WSL forwarding, Docker socket mounts or copied credentials. The native TUI normally owns its own gateway child and shares the selected profile/store; do not claim it attaches to the Desktop gateway process.

Keep user communication and cross-worker decisions with the Lead. Exchange exact source pins, artifacts and verification results. Prefer native state waits/events to repeated idle polling; settled lifecycle status still requires result readback. Do not test direct pane-to-pane messaging, auto-merge, replay completed effects or infer current authority from historical allocations.

Claude and Antigravity are planned future workers. Verify supported runtime, installation and authentication when the user requests their setup; this operation did not install or qualify them. Preserve the Windows Desktop Hermes installation path and existing remote-gateway route.
