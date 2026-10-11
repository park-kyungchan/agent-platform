export declare const upperVocabulary: Readonly<{
    schemaVersion: string;
    id: string;
    terms: string[];
    relations: string[];
    dependencyRule: string;
}>;
import type { Artifact } from "./generated/contracts.js";
export type { Artifact } from "./generated/contracts.js";
export type ArtifactHeader = Pick<Artifact, "schemaVersion" | "kind" | "id" | "projectId" | "domainId" | "createdAt" | "references">;
