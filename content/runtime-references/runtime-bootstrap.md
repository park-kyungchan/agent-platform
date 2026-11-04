# Runtime entry (on demand)

Use the active runner's already loaded instructions. Read the selected task's STATE.md and applicable local rules before effects. Do not re-probe known runner/host/pane facts unless they change the next action.

For a missing workspace/repository location, read `codebase-orientation.md` in this directory. It links the approved directory standard and current transition paths. For a runner-specific operation, use `global-runtime-routing.md`; for actual instruction/MCP registrations use `runtime-wiring.md`; for what each CLI loads at session start and per turn use `RUNTIME-LOADING.md`; for coordinating several CLIs through Herdr use `HERDR-ORCHESTRATION.md`; for a runtime's official documentation use `OFFICIAL-DOCS.md`. Read only the reference needed for the action.

These files do not load themselves. Canonical host instructions are `/root/.agents/AGENTS.md`, containing the user-approved runtime core v2. Its exact source is runtime-core.v2.md in this directory; old core versions and archived harness contracts are not alternate authorities. Rust `ap` render/install/doctor is implemented in `/home/palantirkc/platforms/agent-platform` and `/usr/local/bin/ap` is installed. Content diagnosis does not prove native CLI loading; use the current task state for accepted loading evidence and effect authority.
