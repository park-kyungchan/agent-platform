import type { LogicalIdentity } from "../logical-identity.ts";

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export const JEV_MODEL = "typesafe-ai/jev-1.14.0" as const;
export const ABSTAIN_ID = "__abstain" as const;
export const semanticPurposes = [
  "skill_selection",
  "evidence_selection",
  "research_sufficiency",
  "external_screening",
  "tool_semantics",
  "worker_supervision",
  "engineering_coverage",
  "delegation_coverage",
  "integration_coverage",
  "draft_support",
  "design_choice",
  "context_relevance",
  "next_read",
] as const;
export type SemanticPurpose = (typeof semanticPurposes)[number];
export const semanticEvents = [
  "session_start",
  "task_start",
  "turn",
  "tool_result",
  "source_batch",
  "skill_catalog",
  "worker_start",
  "worker_phase",
  "worker_result",
  "worker_exit",
  "input_request",
  "engineering_result",
  "integration",
  "draft",
  "design",
  "pre_compact",
  "post_compact",
  "interrupt",
  "stop",
  "carrier",
] as const;
export type SemanticEventKind = (typeof semanticEvents)[number];

/** Nullable native IDs remain native. Logical task IDs never fill a missing native ID. */
export interface NativeBinding {
  runtime: "native-codex" | "native-hermes";
  sessionId: string;
  agentId: string | null;
  agentType: string | null;
  parentSessionId: string | null;
  /** Null with empty lineage means the native event does not expose its root/ancestry. */
  rootSessionId: string | null;
  lineageSessionIds: string[];
  profileId: string | null;
  nativeTaskId: string | null;
  nativeRunId: string | null;
  logical: LogicalIdentity | null;
  /** Existing native owner supplied evidence; this library does not authenticate ownership. */
  admissionRef: string;
}
export interface WholeLens {
  currentAuthority: { text: string; sourceRefs: string[] };
  wholeObjective: string;
  taskGoal: string;
  acceptedWhy: {
    text: string;
    attribution: "current-user" | "historical" | "inference";
    sourceRefs: string[];
  }[];
  acceptedDecisions: { id: string; revision: string; sourceRefs: string[] }[];
  owners: { role: "native" | "source" | "domain" | "data"; id: string }[];
  dependencies: {
    id: string;
    revision: string | null;
    currentness: "current" | "stale" | "unknown";
  }[];
  completedEffectRefs: string[];
  uncertainEffectRefs: string[];
  remainingAcceptance: string[];
}
export interface SemanticEvent {
  eventId: string;
  kind: SemanticEventKind;
  binding: NativeBinding;
  sourceRevision: string;
  /** Native owner advances this only for materially changed meaning, including real deadline changes. */
  materialRevision: string;
  material: boolean;
  privacy: "admitted" | "private";
  externalEvidence: boolean;
  facts: { [key: string]: Json };
}
export interface ReadonlyCandidate {
  candidateId: string;
  kind: "read" | "skill" | "wait" | "collect" | "reobserve";
  effectClass: "read_only";
  description: string;
  /** Fixed source-owned handler key, never executable model text. */
  handlerKey: string | null;
  targetId: string | null;
  parameters: { [key: string]: Json };
  preconditionDigest: string;
  authorizationRef: string | null;
  sourceRefs: string[];
}
export interface NoulQuestion {
  type: "noul";
  instructions: Json;
}
export interface ChoiceQuestion {
  type: "choice";
  instructions: Json;
  criteria: Record<string, string>;
}
export type SemanticQuestion = NoulQuestion | ChoiceQuestion;
export interface SemanticWireRequest {
  schema: "jev.semantic-request.v1";
  requestId: string;
  model: typeof JEV_MODEL;
  purpose: "semantic_bundle";
  stateDigest: string;
  state: { [key: string]: Json };
  questions: Record<string, SemanticQuestion>;
  scope: {
    sessionId: string;
    agentId: string | null;
    agentType: string | null;
    taskGoal: string;
    wholeObjective: string;
    why: string;
    owners: string[];
  };
}
export interface PreparedDecision {
  status: "prepared";
  request: SemanticWireRequest;
  requestDigest: string;
  reuseKey: string;
  bindingDigest: string;
  sourceRevision: string;
  candidates: readonly ReadonlyCandidate[];
  candidateDigests: Readonly<Record<string, string>>;
  questionMeanings: Readonly<Record<string, string>>;
  selectionQuestionId: string | null;
}
export type PrepareResult =
  | PreparedDecision
  | {
      status: "skipped";
      reason: "private" | "carrier" | "not-material" | "unchanged";
      reuseKey: string | null;
    };
export interface SemanticDecision {
  status: "decision";
  requestId: string;
  stateDigest: string;
  reuseKey: string;
  selectedCandidateId: string;
  judgments: Readonly<Record<string, number>>;
  nativeChoiceConfidence: number | null;
  /** Unmodified native option probabilities; null when no Choice question was asked. */
  nativeChoiceProbabilities: Readonly<Record<string, number>> | null;
  selectionReason: "model-selection" | "model-abstained" | "no-candidates" | "tied-distribution";
  /** No judgment in this record establishes task/parent/Goal completion or permission. */
  executionAuthorityGranted: false;
}
export interface ReadObservation {
  status: "observed" | "failed" | "unknown";
  observationId: string;
  evidenceRefs: string[];
  postconditionSatisfied: boolean;
}
export interface CurrentReadBinding {
  authorized: boolean;
  bindingDigest: string;
  sourceRevision: string;
  candidateDigest: string;
  authorizationRef: string;
  evidenceRefs: string[];
}
export type ReadonlyHandler = (candidate: ReadonlyCandidate) => Promise<ReadObservation>;
export interface ReadConsumptionReceipt {
  requestId: string;
  selectedCandidateId: string;
  status:
    "abstained" | "selected-no-effect" | "held" | "verified-read" | "read-failed" | "read-unknown";
  reason: string;
  dispatched: boolean;
  observation: ReadObservation | null;
  preconditionEvidenceRefs: string[];
  executionAuthorityGranted: false;
  taskCompletionGranted: false;
  retryAllowed: false;
}

/** Source-owned pins are declarations; native owners authenticate their receipts. */
export interface ContractPin {
  path: string;
  sha256: string;
}
export interface ParentAuthorityRevision {
  assignmentId: string;
  revision: number;
  domainId: string;
  projectId: string;
  workId: string;
  ownerId: string;
  status: "active" | "revoked" | "retired";
  authorityReference: ContractPin;
}
/** One assignment relation, embedded in the packet; observations reference its digest. */
export interface ChildAssignment {
  schema: "jev.child-assignment.v1";
  assignmentId: string;
  revision: number;
  status: "admitted" | "revoked" | "retired";
  actorId: string;
  logical: LogicalIdentity;
  sourceRevision: string;
  sourceClosure: ContractPin;
  packetDigest: string;
  parentAuthority: ParentAuthorityRevision;
  route:
    | { kind: "native-child" }
    | {
        kind: "logical-assignment";
        parentBindingDigest: string;
        childBindingDigest: string;
        relationEvidenceRefs: string[];
      };
}
export interface ChildAssignmentReference {
  assignmentId: string;
  revision: number;
  assignmentDigest: string;
}
export interface CarryForwardReceipt {
  schema: "jev.child-carry-forward.v1";
  from: ParentAuthorityRevision;
  to: ParentAuthorityRevision;
  childAssignment: ChildAssignmentReference;
  successorOwnerDecision: ContractPin;
  pendingAndUnknownEffects: ContractPin[];
  permitsReplay: false;
}
export interface CurrentChildAuthority {
  parentAuthority: ParentAuthorityRevision;
  childAssignment: ChildAssignmentReference;
  childStatus: ChildAssignment["status"];
  carryForward: CarryForwardReceipt | null;
}
