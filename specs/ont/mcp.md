# MCP

Purpose: The MCP server of the ont command.

Moved from crates/ont/README.md ("mcp") at cf328f3c07623d39f300cb3a84599bed30ca258a; requirement ids, "Not included" and "Open" are added when a section's contract changes.

## mcp
- R1: `ont [--store PATH] mcp [--client NAME]` is a Model Context Protocol server over stdio: newline-delimited JSON-RPC 2.0, one request per line on stdin, one JSON line per response on stdout, nothing else on stdout; no listener, no threads. `--client`, when given, matches `^[a-z0-9][a-z0-9-]{0,31}$` (else the refusal `invalid_request`, before stdin is read); when `--client` is absent, `initialize` supplies `clientInfo.name` only when `valid_client` accepts it; absent, empty or invalid names use `mcp`.
- R2: An explicit `--client` wins, except the reserved name `cli` also maps to `mcp`, so MCP writes never share the CLI action namespace.
- R3: Actions record this client, the per-call `agent` (falling back to nonempty `ONT_AGENT`), optional `session` and the per-call `role` (falling back to nonempty `ONT_ROLE`) through `Context`, subject to the library actor bounds.
- R4: The uid check is that of the other subcommands.
- Register it in a CLI as `{"command":"/usr/local/bin/ont","args":["mcp","--client","<cli>"]}`. This crate registers and installs nothing; each CLI's own configuration is changed by a separate packet.
- R5: Methods: `initialize` (answers the asked protocol version when it is `2025-11-25`, `2025-06-18`, `2025-03-26` or `2024-11-05`, else `2025-11-25`; `capabilities` `{"tools":{}}`; `serverInfo` `ont` and the build commit), `ping`, `tools/list`, `tools/call`.
- R6: Notifications get no response. Another method is `-32601`; a line that is not JSON is `-32700` (id null); a JSON value that is not a request object, a batch array included, is `-32600`; an unknown tool or `arguments` that is not an object is `-32602`.
- R7: Ids (string or number) are echoed. End of input exits 0.
- R8: Tools (eight): `ont_describe`, `ont_get`, `ont_search`, `ont_index`, `ont_action_list`, `ont_action_validate`, `ont_action_apply`, `ont_log`. The arguments are the JSON object that the subcommand reads on stdin (for `ont_index`, the object `index` takes: `{"sources":[...]}`).
- R9: Each call opens the store, runs the same operation as the subcommand, closes it and binds the principal for that call; the server never creates a store and holds nothing between calls. `ont_index` takes the same run lock (`index_busy`).
- R10: Result: `{"content":[{"type":"text","text":T}],"isError":B}`. `T` is the line the subcommand prints for the same request, store and `now`, without its trailing newline; an internal failure (exit 1, which prints no stdout) is `{"status":"internal","code":"<code>@<file>:<line>"}`.
- R11: `isError` is true exactly when the subcommand's exit code would not be 0: a refusal, `source_failed`, an internal failure (a panic inside one call is contained and answered `panic@<file>:<line>`).
- R12: There is no `structuredContent`.
- R13: Bounds: a request line over 1,048,576 bytes is answered `-32600` `request_too_large` and discarded as it is read;
- R14: a `T` over 262,144 bytes (measured on the `text` bytes; the wire line is that text JSON-escaped and can be up to about twice as long) is replaced by the refusal `response_too_large` (with `bytes` and, for `ont_search` and `ont_log`, `retry_with_smaller_limit`: true);
- R15: `tools/list` is constant and at most 6,000 bytes.

| Tool | Operation |
| --- | --- |
| `ont_describe` | Describe the installed schema. |
| `ont_get` | Read one object and its history. |
| `ont_search` | Filter objects. |
| `ont_index` | Refresh indexed sources. |
| `ont_action_list` | List action types with parameters and criteria codes; optional `domain` only. |
| `ont_action_validate` | Preview criteria, basis revisions and edits without writing. |
| `ont_action_apply` | Apply an action with a required `request_id` and optional `expected` revisions. |
| `ont_log` | Read applied actions or recorded refusals, newest first (the request of `ont log`). |

### Changing something through an action

Choose an action and its parameter names from `ont_action_list`, and object ids from `ont_search` when needed; call `ont_action_validate`, then pass its `basis` as `expected` to `ont_action_apply`. The following request is from `action_validate_apply_passthrough_and_refusals_write_nothing` using the shared test schema: `{"name":"ont_action_validate","arguments":{"action_type":"create_tag","parameters":{"label":"once"},"request_id":"tag-1","agent":"parity-agent"}}`; its `result` is shown below. Apply with the same arguments and `ont_action_apply` to receive `outcome: applied`; after a lost answer, repeat the same `request_id` with the same arguments, including `agent`, to receive `already_applied` without another change. Every value of `edits_log` means the change was made; a refusal carries `code`, and must not be retried unchanged.
- R16: All notifications, including `notifications/cancelled`, get no answer and change nothing; the server completes a started apply before reading the next line.

```json
{"action_type":"create_tag","basis":[],"criteria":[],"edits":[{"change":"initial","id":{"namespace":{"account":"root","instance":"vps://palantirkc"},"native_key":"act-rq-4876aec9a158d5ecf55564d16c398d05","object_type":"tag"},"revision":"act-rq-4876aec9a158d5ecf55564d16c398d05#0"}],"valid":true}
```

- R17: MCP action `request_id` must match `^[A-Za-z0-9._-]{1,64}$`: choose a new unique value per intended change (for example a UUID); invalid values are refused with `invalid_argument` and a message naming the allowed form.
- R18: The action schemas reject additional properties; `agent`, `session` and `role` match `^[A-Za-z0-9._:-]{1,64}$`, and validate accepts `expected` basis revisions.
- R19: If an apply answer exceeds the response bound after commit, `response_too_large` retains `action_id` and `outcome` so the caller can identify the committed change.

