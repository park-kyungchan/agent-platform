import {
  object,
  parse,
  integer,
  nonempty,
  hash,
  type Check,
  type ReviewBinding,
  type RecordValue,
} from "./source-coverage.ts";
import { readerResult, discoveryResult, ownList } from "./result-contract.ts";
import { dataSnapshot } from "./data-snapshot.ts";
export type RecordCounts = {
  messageRows: number;
  toolCallEntries: number;
  distinctCallIds: number;
  duplicateCallEntries: number;
  duplicateResultRows: number;
};
export type Call = {
  name: string;
  args: RecordValue;
  rowId: number;
  signature: string;
  outer: string;
  callId: string;
};
/** Decode only the installed one-local-call bridge and retained singular form.
 * Mixed, multi-call, null and unknown shapes are not silently normalized. */
function decode(name: any, args: any): { name: string; args: RecordValue } {
  if (!nonempty(name) || !object(args)) throw new Error("arguments");
  if (name !== "tool_call") return { name, args };
  let item = args;
  if (Object.hasOwn(args, "calls")) {
    if (Object.keys(args).length !== 1 || !Array.isArray(args.calls) || args.calls.length !== 1)
      throw new Error("bridge");
    item = args.calls[0];
  }
  if (
    !object(item) ||
    Object.keys(item).some((k) => !["name", "arguments"].includes(k)) ||
    !nonempty(item.name) ||
    item.name === "tool_call" ||
    !object(item.arguments)
  )
    throw new Error("bridge");
  return { name: item.name, args: item.arguments };
}
/** No writes or native authority. Complete ordered rows remain caller-owned evidence. */
export function reviewTrace(
  observation: any,
  expected: ReviewBinding,
  reader: string,
  lifecycle: string[],
  check: Check,
) {
  observation = dataSnapshot(observation);
  expected = dataSnapshot(expected);
  lifecycle = dataSnapshot(lifecycle);
  const { session, messages } = observation;
  if (!object(session) || !Array.isArray(messages) || messages.length < 3 || messages.length > 1000)
    throw new Error("observation");
  check(session.id === expected.sessionId, "SESSION_IDENTITY_MISMATCH");
  check(
    nonempty(session.model) &&
      nonempty(session.billing_provider) &&
      session.model === expected.model &&
      session.billing_provider === expected.provider,
    "MODEL_ROUTE_MISMATCH",
  );
  check(
    typeof session.ended_at === "number" && Number.isFinite(session.ended_at),
    "RUN_NOT_TERMINAL",
  );
  check(session.message_count === messages.length, "MESSAGE_TOTAL_MISMATCH");
  const calls = new Map<string, Call>(),
    results = new Map<string, RecordValue>(),
    users: RecordValue[] = [];
  const unsettled = new Set<string>();
  let previousId = -1,
    toolCallEntries = 0,
    duplicateCallEntries = 0,
    duplicateResultRows = 0;
  for (const row of messages) {
    if (!object(row)) throw new Error("row");
    check(integer(row.id) && row.id > previousId, "MESSAGE_ORDER_INVALID");
    previousId = row.id;
    check(row.session_id === expected.sessionId, "FOREIGN_MESSAGE");
    check(row.active !== 0 && row.compacted !== 1, "UNSUPPORTED_COMPACTED_EVIDENCE");
    check(["user", "assistant", "tool"].includes(row.role), "UNEXPECTED_MESSAGE_ROLE");
    if (row.role === "user") users.push(row);
    const entries = parse(row.tool_calls) ?? [];
    if (!Array.isArray(entries)) throw new Error("calls");
    check(!entries.length || row.role === "assistant", "CALL_ROLE_INVALID");
    // Native synchronous rounds settle before the next model decision. A pure
    // same-ID record replay is not independently established as a new decision.
    const newDecision =
      row.role === "assistant" &&
      (!entries.length || entries.some((raw) => !calls.has(raw?.call_id ?? raw?.id)));
    if (newDecision) check(unsettled.size === 0, "UNSETTLED_PREREQUISITE");
    toolCallEntries += entries.length;
    for (const raw of entries) {
      if (!object(raw) || !object(raw.function)) throw new Error("call");
      const callId = raw.call_id ?? raw.id;
      if (!nonempty(callId)) throw new Error("call id");
      check(!raw.id || raw.id === callId, "CALL_ID_MISMATCH");
      const outer = raw.function.name,
        originalArgs = parse(raw.function.arguments);
      const { name, args } = decode(outer, originalArgs);
      check(
        [reader, "tool_search", "tool_describe", ...lifecycle].includes(name),
        "FORBIDDEN_TOOL",
      );
      check(outer !== "tool_call" || name === reader, "FORBIDDEN_TOOL");
      if (name === "tool_describe") {
        const names = typeof args.names === "string" ? [args.names] : args.names;
        check(ownList(names, (n: any) => n === reader) && names.length > 0, "FORBIDDEN_TOOL");
      }
      const signature = JSON.stringify({ outer, args: originalArgs });
      const prior = calls.get(callId);
      if (prior) {
        duplicateCallEntries++;
        check(prior.signature === signature, "CONFLICTING_CALL_ID");
      } else {
        calls.set(callId, { name, args, rowId: row.id, signature, outer, callId });
        unsettled.add(callId);
      }
    }
    if (row.role === "tool") {
      if (!nonempty(row.tool_call_id)) throw new Error("result");
      const prior = results.get(row.tool_call_id);
      if (prior) {
        duplicateResultRows++;
        check(
          prior.content === row.content && prior.tool_name === row.tool_name,
          "CONFLICTING_TOOL_RESULT",
        );
      } else {
        results.set(row.tool_call_id, row);
        unsettled.delete(row.tool_call_id);
      }
    }
  }
  check(
    users.length === 1 &&
      typeof users[0]?.content === "string" &&
      hash(users[0].content) === expected.requestSha256,
    "REQUEST_IDENTITY_MISMATCH",
  );
  check(
    calls.size > 0 && [...calls.values()].every((call) => users[0]?.id < call.rowId),
    "REQUEST_AFTER_WORK",
  );
  // Native persistence counts serialized entries, not unique identities or executions.
  check(session.tool_call_count === toolCallEntries, "TOOL_TOTAL_MISMATCH");
  for (const key of results.keys()) check(calls.has(key), "UNMATCHED_TOOL_RESULT");
  for (const [key, call] of calls) {
    const row = results.get(key);
    if (!row) {
      check(false, "MISSING_TOOL_RESULT");
      continue;
    }
    check(row.id > call.rowId, "RESULT_BEFORE_CALL");
    check(row.tool_name === call.outer || row.tool_name === call.name, "RESULT_TOOL_MISMATCH");
    const value = parse(row.content);
    if (!object(value)) throw new Error("result data");
    check(!Object.hasOwn(value, "error"), "TOOL_ERROR");
    if (call.name === reader)
      check(readerResult(call.args, value), "UNSUPPORTED_RESULT_ANNOTATION");
    else if (call.name === "tool_search" || call.name === "tool_describe")
      check(discoveryResult(call.name, call.args, value, reader), "UNSUPPORTED_RESULT_ANNOTATION");
  }
  const records: RecordCounts = {
    messageRows: messages.length,
    toolCallEntries,
    distinctCallIds: calls.size,
    duplicateCallEntries,
    duplicateResultRows,
  };
  return { calls, results, messages, session, records };
}
