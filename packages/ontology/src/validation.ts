import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import schema from "./schema.js";
import type { Artifact, EvidenceArtifact, VerificationArtifact } from "./generated/contracts.js";
import { jsonIssue } from "./json-validation.js";

export interface ValidationIssue {
  code: string;
  path: string;
  message: string;
}
export type ValidationResult<T = Artifact> =
  { valid: true; value: T; issues: [] } | { valid: false; issues: ValidationIssue[] };
export interface ReferenceValidationResult {
  valid: boolean;
  issues: ValidationIssue[];
}

const ajv = new Ajv2020({
  strict: true,
  allErrors: true,
  coerceTypes: false,
  useDefaults: false,
  removeAdditional: false,
});
addFormats(ajv);
const validateShape = ajv.compile<Artifact>(schema);

/** Validate unknown JSON before narrowing it to generated schema types. Never mutates input. */
export function validateArtifact(value: unknown): ValidationResult {
  const invalidJson = jsonIssue(value);
  if (invalidJson) return { valid: false, issues: [invalidJson] };
  if (!validateShape(value)) {
    return {
      valid: false,
      issues: (validateShape.errors ?? []).map((error) => ({
        code: `schema:${error.keyword}`,
        path: error.instancePath,
        message: error.message ?? "Schema validation failed.",
      })),
    };
  }
  const issues: ValidationIssue[] = [];
  const issue = (code: string, path: string, message: string) =>
    issues.push({ code, path, message });
  if (!value.id.startsWith(`${value.projectId}:${value.kind}:`))
    issue("identity-mismatch", "/id", "ID project and kind must match the envelope.");
  if (
    value.kind === "decision" &&
    value.payload.status === "accepted" &&
    !value.payload.authority
  ) {
    issue(
      "missing-authority",
      "/payload/authority",
      "An accepted decision requires a scoped authority declaration.",
    );
  }
  if (
    value.kind === "task" &&
    value.payload.status === "complete" &&
    !value.payload.verificationRefs?.length
  ) {
    issue(
      "missing-completion-verification",
      "/payload/verificationRefs",
      "A completed task requires verification references.",
    );
  }
  if (value.kind === "evidence") {
    if (
      value.payload.result.outcome === "success" &&
      value.payload.result.processExitCode !== null &&
      value.payload.result.processExitCode !== 0
    ) {
      issue(
        "contradictory-exit",
        "/payload/result",
        "Successful evidence cannot declare a nonzero process exit code.",
      );
    }
    value.payload.attempts.forEach((attempt, index) => {
      if (attempt.attempt !== index + 1)
        issue(
          "attempt-order",
          `/payload/attempts/${index}/attempt`,
          "Attempts must retain contiguous execution order starting at 1.",
        );
      if (attempt.outcome === "failure" && !attempt.error)
        issue(
          "missing-attempt-failure",
          `/payload/attempts/${index}/error`,
          "A failed attempt requires its observed failure message.",
        );
    });
    if (
      value.payload.result.outcome === "success" &&
      value.payload.attempts.at(-1)?.outcome === "failure"
    ) {
      issue(
        "contradictory-final-attempt",
        "/payload/result/outcome",
        "Successful evidence must retain a final successful attempt after failures.",
      );
    }
  }
  if (value.kind === "verification") {
    const ids = new Set<string>();
    value.payload.criteria.forEach((criterion, index) => {
      if (ids.has(criterion.id))
        issue(
          "duplicate-criterion",
          `/payload/criteria/${index}/id`,
          "Criterion IDs must be unique within the verification.",
        );
      ids.add(criterion.id);
      if (criterion.status !== "unverified" && !criterion.evidenceRefs.length)
        issue(
          "unsupported-result",
          `/payload/criteria/${index}/evidenceRefs`,
          "Pass or fail results require observed evidence.",
        );
    });
  }
  if (value.kind === "run") {
    if (value.payload.state === "running" && value.payload.finishedAt !== null)
      issue(
        "running-finished",
        "/payload/finishedAt",
        "A running execution has no finish timestamp.",
      );
    if (value.payload.state !== "running" && value.payload.finishedAt === null)
      issue(
        "terminal-unfinished",
        "/payload/finishedAt",
        "A terminal execution requires its finish timestamp.",
      );
    if (
      value.payload.finishedAt !== null &&
      Date.parse(value.payload.finishedAt) < Date.parse(value.payload.startedAt)
    )
      issue("negative-duration", "/payload/finishedAt", "Finish time cannot precede start time.");
  }
  if (value.kind === "run" || value.kind === "verification") {
    const paths = new Set<string>();
    value.payload.inputHashes.forEach((input, index) => {
      if (paths.has(input.path))
        issue(
          "duplicate-input",
          `/payload/inputHashes/${index}/path`,
          "Each input path has one fingerprint within this record.",
        );
      paths.add(input.path);
    });
  }
  return issues.length ? { valid: false, issues } : { valid: true, value, issues: [] };
}

export function assertArtifact(value: unknown): asserts value is Artifact {
  const result = validateArtifact(value);
  if (!result.valid)
    throw new Error(
      `Invalid artifact: ${result.issues.map((issue) => `${issue.path} ${issue.code}: ${issue.message}`).join("; ")}`,
    );
}

/** Resolve a closed record set. External records must be supplied explicitly. Hashes are syntax-checked, not authenticated. */
export function validateReferences(records: readonly unknown[]): ReferenceValidationResult {
  const issues: ValidationIssue[] = [];
  const artifacts = new Map<string, Artifact>();
  const issue = (code: string, path: string, message: string) =>
    issues.push({ code, path, message });
  for (const [index, record] of records.entries()) {
    const result = validateArtifact(record);
    if (!result.valid) {
      issues.push(
        ...result.issues.map((item) => ({ ...item, path: `/records/${index}${item.path}` })),
      );
      continue;
    }
    if (artifacts.has(result.value.id))
      issue("duplicate-id", `/records/${index}/id`, `Duplicate artifact ID ${result.value.id}.`);
    else artifacts.set(result.value.id, result.value);
  }
  const resolve = (id: string, path: string, kind?: Artifact["kind"]): Artifact | undefined => {
    const target = artifacts.get(id);
    if (!target) issue("dangling-reference", path, `Referenced artifact ${id} was not supplied.`);
    else if (kind && target.kind !== kind)
      issue("reference-kind", path, `Expected ${kind}, found ${target.kind}.`);
    return target && (!kind || target.kind === kind) ? target : undefined;
  };
  const evidence = (id: string, path: string): EvidenceArtifact | undefined => {
    const target = resolve(id, path, "evidence");
    return target?.kind === "evidence" ? target : undefined;
  };
  const verification = (id: string, path: string): VerificationArtifact | undefined => {
    const target = resolve(id, path, "verification");
    return target?.kind === "verification" ? target : undefined;
  };
  for (const artifact of artifacts.values()) {
    const base = `/artifacts/${artifact.id}`;
    for (const [index, reference] of (artifact.references ?? []).entries()) {
      const target = resolve(reference.artifactId, `${base}/references/${index}`);
      if (reference.artifactId === artifact.id)
        issue(
          "self-reference",
          `${base}/references/${index}`,
          "An artifact cannot declare itself as a related predecessor or support.",
        );
      if (reference.relation === "supersedes" && target && target.kind !== artifact.kind)
        issue(
          "supersedes-kind",
          `${base}/references/${index}`,
          "A correction supersedes the same artifact kind.",
        );
    }
    if (artifact.kind === "proposal" || artifact.kind === "decision" || artifact.kind === "run") {
      artifact.payload.evidenceRefs.forEach((id, index) =>
        evidence(id, `${base}/payload/evidenceRefs/${index}`),
      );
    }
    if (artifact.kind === "decision") {
      if (artifact.payload.proposalRef)
        resolve(artifact.payload.proposalRef, `${base}/payload/proposalRef`, "proposal");
      const authority = artifact.payload.authority;
      if (artifact.payload.status === "accepted" && authority) {
        const source = evidence(
          authority.sourceEvidenceRef,
          `${base}/payload/authority/sourceEvidenceRef`,
        );
        const declaration = source?.payload.details?.authority;
        if (
          source &&
          (source.payload.result.outcome !== "success" ||
            typeof declaration !== "object" ||
            declaration === null ||
            Array.isArray(declaration) ||
            declaration.actorType !== authority.actorType ||
            declaration.actorId !== authority.actorId ||
            declaration.scope !== authority.scope)
        ) {
          issue(
            "authority-scope-mismatch",
            `${base}/payload/authority`,
            "Authority evidence must explicitly declare the same actor and scope and have a successful observation.",
          );
        }
      }
    }
    if (artifact.kind === "verification") {
      artifact.payload.targetRefs.forEach((id, index) =>
        resolve(id, `${base}/payload/targetRefs/${index}`),
      );
      artifact.payload.criteria.forEach((criterion, index) =>
        criterion.evidenceRefs.forEach((id, refIndex) => {
          const source = evidence(id, `${base}/payload/criteria/${index}/evidenceRefs/${refIndex}`);
          if (
            criterion.status === "pass" &&
            source &&
            (source.payload.result.outcome !== "success" ||
              source.payload.result.contentMatched === false)
          ) {
            issue(
              "inapplicable-pass-evidence",
              `${base}/payload/criteria/${index}`,
              "Passing criteria require successful evidence with no observed content mismatch.",
            );
          }
        }),
      );
    }
    if (artifact.kind === "task") {
      (artifact.payload.verificationRefs ?? []).forEach((id, index) => {
        const receipt = verification(id, `${base}/payload/verificationRefs/${index}`);
        if (
          artifact.payload.status === "complete" &&
          receipt &&
          (!receipt.payload.targetRefs.includes(artifact.id) ||
            receipt.payload.criteria.some((criterion) => criterion.status !== "pass"))
        ) {
          issue(
            "incomplete-task-verification",
            `${base}/payload/verificationRefs/${index}`,
            "Task completion requires a receipt targeting this task with all declared criteria passing.",
          );
        }
      });
    }
    if (artifact.kind === "run") {
      resolve(artifact.payload.taskRef, `${base}/payload/taskRef`, "task");
      artifact.payload.verificationRefs.forEach((id, index) =>
        verification(id, `${base}/payload/verificationRefs/${index}`),
      );
    }
  }
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (id: string) => {
    if (visiting.has(id)) {
      issue(
        "supersedes-cycle",
        `/artifacts/${id}/references`,
        "The supersedes relation cannot contain a cycle.",
      );
      return;
    }
    if (visited.has(id)) return;
    visiting.add(id);
    for (const reference of artifacts.get(id)?.references ?? [])
      if (reference.relation === "supersedes" && artifacts.has(reference.artifactId))
        visit(reference.artifactId);
    visiting.delete(id);
    visited.add(id);
  };
  for (const id of artifacts.keys()) visit(id);
  return { valid: issues.length === 0, issues };
}
