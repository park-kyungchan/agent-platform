import { isDeepStrictEqual as same } from "node:util";
import {
  hash,
  object,
  requireValue,
  type DomainContextBinding,
  type WorkBinding,
} from "./contracts";

/** Consume the existing domain reader's selected result. Its atom validation,
 * Work selection and dependency algorithm remain owned by dc-gang-a. */
export function bindDomainContext(
  input: unknown,
  expected: DomainContextBinding,
  binding: WorkBinding,
) {
  object(input, "domain query result");
  // The existing owner transport preserves argv/exit/source readback alongside
  // its unchanged JSON result. Accept that measured wrapper without copying it.
  if (input.kind === undefined && input.result !== undefined) {
    const wrapper = input;
    requireValue(
      wrapper.exitCode === 0 &&
        wrapper.sourcePinsUnchanged === true &&
        wrapper.sourcePins &&
        expected.sourcePins.every((p) => wrapper.sourcePins[p.path] === p.sha256),
      "DOMAIN_QUERY_TRANSPORT_FAILED",
      expected.repositoryId,
    );
    input = input.result;
    object(input, "selected domain result");
  }
  requireValue(
    input.schemaVersion === 1 &&
      input.kind === "selected_atomic_workflow_context" &&
      input.executesNextStep === false &&
      input.authorizesProductionEffects === false &&
      input.fieldProjectionsEmitted === true,
    "DOMAIN_QUERY_REFUSED",
    "Expected a selected read-only producer result.",
  );
  requireValue(
    input.revision === expected.expectedRevision &&
      input.revision ===
        hash(
          JSON.stringify({
            purpose: input.purpose,
            atoms: input.atoms,
            relations: input.relations,
          }),
        ),
    "STALE_DOMAIN_CONTEXT",
    expected.repositoryId,
  );
  requireValue(
    input.entryReadPlan?.ok === true &&
      input.entryReadPlan.view === "current" &&
      Array.isArray(input.entryReadPlan.requiredReads),
    "DOMAIN_SOURCE_REFUSAL",
    expected.repositoryId,
  );
  const observed = input.entryReadPlan.requiredReads.map((r: any) => {
    requireValue(r.status === "verified_file_presence_and_hash", "DOMAIN_SOURCE_REFUSAL", r.id);
    return { id: r.id, path: r.path, sha256: r.sha256 };
  });
  requireValue(
    same(
      observed.sort((a: any, b: any) => a.id.localeCompare(b.id)),
      [...expected.sourcePins].sort((a, b) => a.id.localeCompare(b.id)),
    ),
    "DOMAIN_SOURCE_PIN_CONFLICT",
    expected.repositoryId,
  );
  requireValue(
    Array.isArray(input.atoms) && input.atoms.length <= 128,
    "INVALID_DOMAIN_ATOMS",
    expected.repositoryId,
  );
  const selected = input.atoms.filter((a: any) => a.kind === "work_intent");
  requireValue(
    selected.length === 1 &&
      selected[0].value.unit === expected.workUnit &&
      selected[0].value.stage === expected.workState,
    "DOMAIN_WORK_CONFLICT",
    expected.repositoryId,
  );
  const work = selected[0];
  for (const atom of input.atoms)
    requireValue(
      expected.sourcePins.some(
        (p) =>
          p.id === atom.source?.id &&
          p.path === atom.source.path &&
          p.sha256 === atom.source.sha256,
      ),
      "DOMAIN_ATOM_SOURCE_CONFLICT",
      atom.id,
    );
  if (expected.relation === "current-work")
    requireValue(binding.work.id === expected.workUnit, "DOMAIN_WORK_ID_MISMATCH", binding.work.id);
  return {
    producer: {
      schemaVersion: 1,
      kind: input.kind as "selected_atomic_workflow_context",
      revision: input.revision as string,
    },
    repositoryId: expected.repositoryId,
    relation: expected.relation,
    work: {
      programme: work.value.programme as string,
      unit: expected.workUnit,
      state: expected.workState,
    },
    atoms: structuredClone(input.atoms),
    relations: structuredClone(input.relations),
    dependencyOrder: [...input.dependencyOrder],
    unknowns: structuredClone(input.unknowns),
    vocabulary:
      "domain DATA/LOGIC/ACTION/GOVERNANCE retained; GOVERNANCE is not renamed to pack SECURITY",
    executesNextStep: false as const,
    authorizesProductionEffects: false as const,
  };
}
