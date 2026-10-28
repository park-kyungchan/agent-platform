/** Native CLI adapters only; never a provider client, credential owner or model-setting mutation. */
export function reviewerInvocation(runtime: string, sessionId: string, smoke = false): string[] {
  if (!/^[a-f0-9-]{36}$/.test(sessionId)) throw Error("Native reviewer session UUID required");
  if (runtime === "native-claude")
    return [
      "claude",
      "-p",
      "--model",
      "opus",
      "--permission-mode",
      "plan",
      "--session-id",
      sessionId,
      ...(smoke ? ["--tools", ""] : []),
    ];
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
