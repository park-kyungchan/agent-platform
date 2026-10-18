# Recorded deployment, 2026-10-17 UTC

This is dated evidence, not configuration to install. Revalidate before use. A later authorized actual-use test is recorded in [live-use.md](live-use.md); the original hold below belongs to its earlier scope.

| Observation | Evidence and limit |
|---|---|
| Independent worker | Live w7:p3, workspace w7/tab w7:t1; foreground native Codex 0.163.0, gpt-6.1-sol with ultra. Main w7:p1/w7:p2 remained inventory-only. |
| Herdr | Client/server 0.9.5, protocol22, endpoint generation1, compatible. `hermes` is listed as a kind. No integration was installed by this task. |
| Local native start | Owned w7:p4 `agent start hh-indep-native --kind hermes --timeout 8000` timed out; pane read showed `hermes: command not found`. Local `command -v hermes` had no result. This proves failure in this executor, not absence of Hermes everywhere. |
| Existing remote deployment | Inventory-only argv showed SSH→`docker exec -it --user 10000:10000`, container `hermes-agent-2q3y-hermes-serve-1`, `/opt/hermes/bin/hermes -p odi-platform --tui`. Local Herdr did not recognize that existing SSH process as Hermes. It was not prompted or attached. |
| Installed release | `/opt/hermes/install-stamp.json`: Hermes0.22.0, commit e06df9c2eae6901f4c88f464e0ae6e753c0428b8, external Docker distribution. No `.git` was present; commit is install-stamp evidence, not independently checked repository HEAD. |
| Packaged TUI | `entry.js` exists, SHA25618bf50a86badaf3ab49591c208fab7d71e48113d16ac3ec30f7e96fd90f7bacd. Original compiled `dist` tree hash 2e3043f55fe4ed078d50ec7c65a36140a5242fbf4f5d14ead3efcee1b019d392 matches the build receipt. The relocated `hermes_cli/tui_dist` also contains a later workspace package.json outside the original hash scope; hashing that larger tree produces df887edb... and must not be called bundle drift. Native `/opt/hermes/ui-tui/dist/entry.js` and the relocated copy have identical bytes. No rebuild or TUI launch was performed. |
| Raw transport actual test | Owned w7:p4 ran existing `hermes-backend-shell` with bash `--noprofile --norc`. A single Python stdlib read returned the unique marker, UID10000, cwd `/opt/data`, version/commit. Local foreground was SSH. `agent get` and `agent attach` returned `agent_not_found`; no takeover/manual reporting was used. |
| Actual model test | Deferred. No Hermes prompt, model call, new native session/profile or credential bridge was created. No model-result/stored-key/call-ID correlation exists. Native Task/Run IDs are null. |

The official annotated v0.22.0 tag peels to the same e06df9c commit as the install stamp. Nine inspected source files are byte-identical to that tag, and the current installed TUI build-input sourceHash62a54f3b972485f7432c8c99eb8fc4333510feb4eb6c8e54407551a28de691ae matches the official archive closure under the native hashing algorithm. The TUI npm package version0.0.1 is also present in the official v0.22.0 source; it is not the Hermes product version. The receipt’s build-time sourceHash161ad5d... differs from the current source closure; staging-origin details remain unverified. There was no evidence of a different product version requiring the user’s conditional update.

The installed `hermes-backend` is a native SSH/Docker launcher, not a Herdr executable alias. It requires a concrete profile, prevents implicit fresh-launch auto-resume unless the preference is false, clears TUI query/image/gateway/sidecar environment values, and supports explicit `--resume`. Its SSH uses existing config with BatchMode and StrictHostKeyChecking. Never change host keys, servers or credentials to make this route pass.

## Why isolation held model execution

Installed profile creation invokes s6 `register_profile_gateway(..., start_now=False)` and `_notify_multiplexer` after creating files. Existing-profile startup initializes/maintains SessionDB and scratch caches. Native auth can refresh a shared root store or adopt Codex OAuth tokens, affecting a shared refresh-token family. Arbitrary isolated home plus disabled adoption lacks independently owned existing-subscription auth. The request forbade registration/cache/config/credential changes and approval bypass. The native `oneshot` path sets `HERMES_YOLO_MODE=1`; it was not used.

## Existing hooks must be distinguished

Read-only config/source inspection found root `/opt/data` shell hooks targeting `/opt/data/workspace/hermes-harness-audit/platform/current/native-hook.js`. That file pins old `hermes-platform-rearchitecture` engineering Work, injects context on pre_llm_call and writes boundary-observation JSON receipts under that deployment. Running it is not a pure read.

The observed `odi-platform` profile instead has a pre_llm_call hook at `/opt/data/runtime/harness/current-git-work/odi-20261017/candidate/current-work-native-hook.js`; its captured entrypoint reads pinned current Git Work and emits context/diagnostics. This static distinction does not prove actual turn activation, absence of every native/plugin write, or successful execution veto. Global `ontology-work-policy` plugin has separate pre_tool_call block logic. Shell pre_llm_call/pre_api_request context return and execution veto must be evaluated separately.

Reproducible receipts live in the independent report's `evidence/` directory. Do not hardcode w7 IDs, the container name, credentials or this dated release into another task.
