import vocabulary from "../ontology/upper/vocabulary.json" with { type: "json" };
export const upperVocabulary = Object.freeze(vocabulary);
import type { Artifact } from "./generated/contracts.js";
export type { Artifact } from "./generated/contracts.js";
export type ArtifactHeader = Pick<
  Artifact,
  "schemaVersion" | "kind" | "id" | "projectId" | "domainId" | "createdAt" | "references"
>;
