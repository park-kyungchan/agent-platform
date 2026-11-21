# Functions

Purpose: The functions a domain declares in its schema, and `ont fn`, which answers a question by running one: the engine knows the declaration and no domain's code.

A function is a question an agent asks instead of reading files. Its body here is a program: an executable the domain's own installer puts on the host. The engine checks the call, starts the program, bounds its answer and its time, and keeps it from writing through `ont`.

## Declaration

- R1: An entry of `functions` in a schema is `{"description"?: string, "parameters": {<name>: {"type": <type>, "required": bool, "description"?: string}}, "program": "<absolute path>", "answer_bytes": <integer 1 to 1048576>, "timeout_ms": <integer 1 to 600000>}`. A parameter's `type` is a type as an action parameter declares it (a scalar, `{ref}`, `{array}`, `{struct}` or `{value_type}`, its names declared in the schema); a parameter's name follows the rule of action parameter names (1 to 64 ASCII characters, a lowercase letter first, then lowercase letters, digits or underscore; `now` is reserved). A description is at most 300 UTF-8 bytes. A `program` is an absolute path of at most 4096 bytes without NUL.
- R2: `ont schema check` and `ont schema install` refuse a malformed entry with the code `invalid_function` and the JSON pointer of the member at fault, one diagnostic per fault: `/functions/<name>/<member>` for a missing, unknown or invalid member of the entry (an entry without `program` is malformed), `/functions/<name>/parameters/<parameter>` for a parameter name against the rule or a parameter that is not an object, `/functions/<name>/parameters/<parameter>/<member>` for a missing, unknown or invalid member of a parameter (one diagnostic for a `type`, at whatever depth it fails), and `/functions/<name>` for an entry that is not an object. A function name against the name rule stays `invalid_name`.
- R3: A schema whose `functions` is `{}` checks and installs as before this change; a newer version that declares functions installs over it.

## Call

- R4: `ont [--store PATH] fn <name> < request.json` takes one JSON object `{"parameters"?: object, "domain"?: string}` on standard input; empty or whitespace-only input is `{}`. Input that is not one JSON object, another member, or a member of another form is `invalid_request`.
- R5: The function is found among the domains `ont describe` lists for the caller (those the principal's grants admit): when none of them declares a function of that name, or `domain` names a domain that is not one of those that do, the call is refused `unknown_function`; when more than one declares it and `domain` is absent, it is refused `function_ambiguous` with `field` `/domain` and `domains`, the domains that declare it.
- R6: The parameters are checked against the declaration with the codes `ont action validate` gives for an action's parameters: an undeclared one, a missing required one and a value that does not satisfy its declared type (value types' constraints included) are `invalid_parameter` with `field` `/parameters/<name>` and `parameter`; `parameters` that is not an object is `invalid_request` with `field` `/parameters`. An optional parameter given `null` is left out. A refused call starts no program.
- R7: Before it starts, the program must be an absolute path of a regular file (symbolic links followed) that is executable by the caller, owned by uid 0 or by the caller's real uid as the OS reports it, and not writable by group or others; else the call is refused `function_program_unusable` with `detail` naming the first condition that failed: `not_absolute`, `not_found`, `not_accessible`, `not_regular_file`, `not_executable`, `owner_not_trusted` or `writable_by_group_or_others`. A program the OS cannot start is refused with `detail` `start_failed`.
- R8: The program starts with no argument, the current directory `/`, in a process group of its own, with an environment that holds only `PATH`, `HOME` and `LANG` (the caller's values, else `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`, `/` and `C.UTF-8`), `ONT_FN_STORE` (the absolute path of the call's store), `ONT_FN_NAME` (the function's name) and `ONT_FN_READ_ONLY=1`. Its standard input is the checked parameters as one JSON object, then closed.
- R9: Its standard output is read up to `answer_bytes`: one byte more kills its process group and the call is refused `function_answer_too_large` with `answer_bytes`. Its standard error is drained as it comes and only its byte count is kept.
- R10: Past `timeout_ms` from its start the process group is killed and the call is refused `function_timeout` with `timeout_ms`; a process the program started in its group ends with it.
- R11: An exit other than 0 is refused `function_failed` with `detail` `{"exit", "stderr_bytes"}`; `exit` is `null` when a signal ended the program.
- R12: Standard output that is not one JSON value (empty output included) is refused `function_answer_not_json` with `bytes`.
- R13: The answer of a call that holds is one JSON line on standard output, `{"function", "domain", "answer", "elapsed_ms"}`, exit 0. A refusal has the shape and exit of the other read commands' refusals (`{"status":"refused","code",…,"trust_limit"}`, exit 2); once the function is found it also carries `function` and `domain`.
- R14: `ont fn` keeps R1 of `specs/ont/mcp.md`: no listener and no thread; the program's standard input, output and error are served by one `poll` loop of the calling thread, so a program that writes much to standard error before it reads its input does not wait forever.

## Read only

- R15: While `ONT_FN_READ_ONLY` is `1` in its environment, `ont` refuses every subcommand that writes to a store, installs or writes a file (`schema install` in both forms, `backup`, `index`, `action apply` and `render --write`) with `read_only_call`, before the check of the real uid and before any store is opened. Another value is not read only. The read commands work, a program's own call of `ont` included.
- R16: `run` reads the variable once into `Context::read_only` (`Context::new` leaves it false). Each library operation that writes (`ont::action::apply_action`, `ont::schema::install`, `ont::schema::install_commit`, `ont::backup::backup`, `ont::index::index`, `ont::render::write_file`) refuses `read_only_call` first when it is set: before a store is opened for writing and before anything is appended to `refusals.jsonl`. The read operations work with it set.
- R17: In `ont mcp` started with the variable `1` (`mcp::Config::read_only`), the uid check stays at its start (R4 of `specs/ont/mcp.md`), and the tools that write, `ont_action_apply` and `ont_index`, are answered `read_only_call` (`isError` true) before a store is opened; the other tools work.

## Discovery

- R18: `ont describe` and `ont_describe` list each function of a listed domain as `{"domain", "name", "description", "parameters": [{"name", "description", "type", "required"}], "answer_bytes", "timeout_ms"}`; the program's path is not listed.
- R19: `ont mcp` offers the tool `ont_fn`, whose arguments are `{"name", "domain"?, "parameters"?}` (no other member), and which runs the same operation as `ont fn`: its `text` is the subcommand's line for the same request. An answer whose `text` is over the bound of R14 of `specs/ont/mcp.md` comes back as `response_too_large`, as every tool's does.
- R20: `ont help` and `ont fn --help` list the form `fn NAME` with its standard-input fields.

## Not included

- The read-plan body of a function (several starting sets, link hops, filters, run by the engine): a later change. A program serves the first functions meanwhile.
- Grants per function: every function of a listed domain is listed and callable. One MCP tool per function.
- A sandbox against a program that means harm: the program is installed by its domain under a path only its owner writes, and the read-only rule binds `ont` only. A program that calls `ont fn` starts the inner program in a process group of its own, which a kill of the outer group does not reach; the inner call keeps its own time limit while its `ont` lives.
- Forward only: a store whose latest schema of a domain declares a function cannot be opened by an `ont` older than this change (each open checks the latest schema, and the older one refuses the new members of an entry). Install the new `ont` before such a schema.
- No function of any domain ships with the engine.
