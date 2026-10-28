/** Native CLI adapters only; never a provider client, credential owner or model-setting mutation. */
export function reviewerInvocation(
  runtime: string,
  sessionId: string,
  smoke = false,
  finalPath?: string,
): string[] {
  if (!/^[a-f0-9-]{36}$/.test(sessionId)) throw Error("Native reviewer session UUID required");
  if (runtime === "native-codex") {
    if (!finalPath) throw Error("Native Codex final-message path required");
    return [
      "codex",
      "exec",
      "-m",
      "gpt-6.1-sol",
      "-c",
      'model_reasoning_effort="high"',
      "--dangerously-bypass-approvals-and-sandbox",
      "--ephemeral",
      "--json",
      "-o",
      finalPath,
      "-",
    ];
  }
  if (runtime === "native-muse" && smoke)
    return [
      "muse",
      "exec",
      "--model",
      "muse-spark-1.3",
      "--reasoning-effort",
      "xhigh",
      "--yolo",
      "--session-id",
      sessionId,
      "--disable-write",
      "--disable-shell",
      "--disable-web-tools",
      "--disable-reminders",
      "--disable-request-user-input",
      "--no-foreign-personal-context",
    ];
  throw Error(
    "Reviewer adapter unavailable for this scope; Muse currently admits mechanical smoke only",
  );
}

/** The real native Codex thread comes from its JSONL event, never from a logical caller ID. */
export function codexReviewThread(events: Uint8Array): string {
  if (events.length === 0 || events.length > 8_000_000)
    throw Error("Native Codex event budget required");
  const threads: string[] = [];
  for (const line of new TextDecoder("utf-8", { fatal: true })
    .decode(events)
    .split("\n")
    .filter(Boolean)) {
    const event: unknown = JSON.parse(line);
    if (event && typeof event === "object" && "type" in event && event.type === "thread.started") {
      if (
        !("thread_id" in event) ||
        typeof event.thread_id !== "string" ||
        !/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(event.thread_id)
      )
        throw Error("Actual native Codex thread ID required");
      threads.push(event.thread_id);
    }
  }
  if (threads.length !== 1) throw Error("Exactly one fresh native Codex thread required");
  return threads[0]!;
}

/** Closed one-turn completion grammar, bound to the actual final-message bytes. */
export function bindCompletedCodexReview(events: Uint8Array, raw: Uint8Array): string {
  const thread = codexReviewThread(events);
  let turnStarted = false,
    completed = false,
    finalText: string | undefined,
    lastMessage = false;
  for (const line of new TextDecoder("utf-8", { fatal: true })
    .decode(events)
    .split("\n")
    .filter(Boolean)) {
    const event = JSON.parse(line) as Record<string, unknown>;
    if (!event || typeof event !== "object" || Array.isArray(event) || completed)
      throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
    switch (event.type) {
      case "thread.started":
        if (turnStarted) throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
        break;
      case "turn.started":
        if (turnStarted) throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
        turnStarted = true;
        break;
      case "item.started":
      case "item.updated":
      case "item.completed": {
        if (!turnStarted) throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
        const item = event.item as Record<string, unknown> | undefined;
        lastMessage =
          event.type === "item.completed" &&
          item?.type === "agent_message" &&
          typeof item.text === "string";
        if (lastMessage) finalText = item!.text as string;
        break;
      }
      case "turn.completed":
        if (!turnStarted || !lastMessage || finalText === undefined)
          throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
        completed = true;
        break;
      default:
        throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
    }
  }
  if (!completed) throw Error("ACTUAL_NATIVE_COMPLETION_REQUIRED");
  if (!Buffer.from(finalText!, "utf8").equals(Buffer.from(raw)))
    throw Error("ACTUAL_NATIVE_FINAL_MESSAGE_REQUIRED");
  return thread;
}

/** Join every report to the actual requested native invocation before publication. */
export function bindNativeReviewer(
  raw: unknown,
  runtime: string,
  actualTaskId: string,
  authorTaskId: string | null,
): void {
  const record =
    raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const reviewer =
    record.reviewer && typeof record.reviewer === "object" && !Array.isArray(record.reviewer)
      ? (record.reviewer as Record<string, unknown>)
      : {};
  if (
    reviewer.runtime !== runtime ||
    reviewer.taskId !== actualTaskId ||
    actualTaskId === authorTaskId
  )
    throw Error("Native review report differs from the actual reviewer runtime/session");
}
