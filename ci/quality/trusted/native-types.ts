import type { Target, TrustedPolicy } from "./policy";
export interface NativeProfile {
  schema: "agent-platform.native-trusted-ci-profile.v2";
  active: true;
  ownerId: "git-source-owner:1413326539";
  ownerThread: string;
  authorityRevision: number;
  sourceRoot: string;
  sourceHead: string;
  sourceTree: string;
  fileSha256: Record<string, string>;
  policy: TrustedPolicy;
  ghPath: "/usr/bin/gh";
  ghSha256: string;
}
export interface NativeIntent {
  schema: "agent-platform.native-trusted-ci-intent.v2";
  id: string;
  ownerThread: string;
  authorityRevision: number;
  action: "dispatch" | "attest";
  requestId: string;
  policyRelease: string;
  target: Target;
  checkoutRoot: string;
  runId: number | null;
  attempt: number | null;
  expiresAt: string;
}
export interface NativeContext {
  readonly profile: NativeProfile;
  readonly intent: NativeIntent;
  assertCurrent(): void;
  record(value: unknown): void;
}
