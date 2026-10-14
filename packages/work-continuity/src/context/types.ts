/** Work-owned continuity contracts. Source material is context, never permission. */
export interface ContextIo {
  read(path: string): Promise<Uint8Array>;
  canonical(path: string): Promise<string>;
}
export interface SourcePin {
  path: string;
  sha256: string;
  role?: "purpose" | "user-answer" | "decision" | "evidence" | "context" | "code";
  recordKind?: "project" | "task" | "proposal" | "decision" | "evidence" | "verification" | "run";
}
export interface AnswerProvenance extends SourcePin {
  kind: "actual-user-answer";
  locator: string;
}
export interface UserDecision {
  id: string;
  question?: string;
  answer: string;
  status: "accepted" | "superseded";
  source: AnswerProvenance;
  supersedes?: string;
}
export interface CompletedEffect {
  id: string;
  action: string;
  completedAt: string;
  verificationRefs: string[];
  replay: "do-not-repeat";
}
export interface PendingTask {
  id: string;
  action: string;
  dependsOn: string[];
  status: "ready" | "blocked" | "unknown";
}
export interface ContinuityCheckpoint {
  schema: "codex.continuity.checkpoint.v1";
  projectId: string;
  workId: string;
  root: string;
  owner: { agentId: string; threadId: string };
  version: number;
  updatedAt: string;
  purpose: {
    objective: string;
    successCriteria: string[];
    inScope: string[];
    outOfScope: string[];
  };
  userDecisions: UserDecision[];
  completed: CompletedEffect[];
  pending: PendingTask[];
  nextAction: { taskId: string; action: string } | null;
  nativeThreads: Array<{
    threadId: string;
    relation: "owner" | "child" | "predecessor";
    taskId?: string;
    status: "active" | "complete" | "interrupted" | "unknown";
  }>;
  evidenceRefs: Array<
    SourcePin & {
      kind: "observation" | "verification" | "acceptance" | "source";
      recordId?: string;
    }
  >;
  sourcePins: SourcePin[];
  uncertainEffects: Array<{
    id: string;
    action: string;
    reason: string;
    requiredObservation: string;
  }>;
  uninterpretedInputs: Array<
    SourcePin & { kind: "pending-user-input"; locator: string; observedAt: string }
  >;
  authority: "none";
  nativePermissionGranted: false;
}
export interface WorkContextRequest {
  schema: "codex.work-context.request.v1";
  projectId: string;
  workId: string;
  root: string;
  cwd: string;
  checkpointPath: string;
  checkpointSha256?: string;
  maxChars?: number;
}
export type SourceCheckStatus = "current" | "missing" | "drifted" | "unsafe" | "invalid-record";
export interface SourceStatus {
  status: "current" | "repair-required";
  checks: Array<{
    path: string;
    expectedSha256: string;
    actualSha256?: string;
    status: SourceCheckStatus;
  }>;
  repair: Array<{
    path: string;
    action: "reobserve-source" | "restore-source" | "repair-reference";
    reason: string;
  }>;
  authority: "none";
}
export interface CheckpointRead {
  checkpoint: ContinuityCheckpoint;
  sha256: string;
  sourceStatus: SourceStatus;
}
export interface BoundedProjection {
  text: string;
  chars: number;
  truncated: boolean;
  omitted: Record<string, number>;
  authority: "none";
  executionReady: false;
}
export interface BoundaryEvent {
  event: string;
  atUtc: string;
  threadId?: string;
  turnId?: string;
  checkpointPath?: string;
  pendingPrompt?: { path: string; sha256: string; bytes: number };
  nativeSource?: string;
  trigger?: string;
  nativeAgentId?: string;
  nativeAgentType?: string;
  nativeTranscriptPath?: string;
}
