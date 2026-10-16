/** Source delivery policy only: character limits do not authorize native activation or model work. */
export interface CurrentWorkOutputPolicy {
  policyVersion: "hermes.current-work-output.v1" | "hermes.current-work-output.v2";
  maxChars: number;
  capChars: 6000 | 12000;
}
export function currentWorkOutputPolicy(version: "v1" | "v2", requested?: number) {
  const capChars = version === "v2" ? 12000 : 6000;
  const selected = requested === undefined ? capChars : requested;
  const valid = Number.isSafeInteger(selected) && selected >= 1024 && selected <= capChars;
  const policy: CurrentWorkOutputPolicy = {
    policyVersion:
      version === "v2" ? "hermes.current-work-output.v2" : "hermes.current-work-output.v1",
    maxChars: valid ? selected : capChars,
    capChars,
  };
  return { valid, policy };
}
