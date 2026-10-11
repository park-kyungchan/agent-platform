import type { DomainObjectType } from "../generated/evidence.js";
import type { ObjectRecord, OntologyRegistry } from "../generated/operational.js";
import type { ReferenceValidationResult, ValidationResult } from "../validation.js";
export type * from "../generated/evidence.js";
export type EvidenceObject = ObjectRecord & {
    objectType: DomainObjectType;
};
export declare const evidenceSchema: {
    $schema: string;
    $id: string;
    title: string;
    description: string;
    anyOf: {
        $ref: string;
    }[];
    $defs: {
        EvidenceJsonValue: {
            title: string;
            anyOf: ({
                type: string;
                items?: never;
                $ref?: never;
            } | {
                type: string;
                items: {
                    $ref: string;
                };
                $ref?: never;
            } | {
                $ref: string;
                type?: never;
                items?: never;
            })[];
        };
        EvidenceJsonObject: {
            title: string;
            type: string;
            additionalProperties: {
                $ref: string;
            };
        };
        DomainObjectType: {
            title: string;
            type: string;
            enum: string[];
        };
        EvidenceFingerprints: {
            title: string;
            type: string;
            additionalProperties: {
                type: string;
                pattern: string;
            };
        };
        EvalMetric: {
            title: string;
            type: string;
            properties: {
                name: {
                    type: string;
                    minLength: number;
                };
                value: {
                    $ref: string;
                };
                objectiveVersion: {
                    type: string;
                    minLength: number;
                };
                thresholdVersion: {
                    type: string;
                    minLength: number;
                };
                threshold: {
                    $ref: string;
                };
                passed: {
                    anyOf: {
                        type: string;
                    }[];
                };
            };
            required: string[];
            additionalProperties: boolean;
        };
        CaptureArtifact: {
            title: string;
            type: string;
            properties: {
                kind: {
                    type: string;
                    minLength: number;
                };
                sha256: {
                    type: string;
                    pattern: string;
                };
                byteLength: {
                    type: string;
                    minimum: number;
                };
                mediaType: {
                    type: string;
                };
                encoding: {
                    type: string;
                };
                blobRef: {
                    type: string;
                    minLength: number;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        ProjectProperties: {
            title: string;
            type: string;
            properties: {
                name: {
                    type: string;
                    minLength: number;
                };
                domainId: {
                    type: string;
                    minLength: number;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        SourceCaptureProperties: {
            title: string;
            type: string;
            properties: {
                url: {
                    type: string;
                    minLength: number;
                };
                retrievedAt: {
                    type: string;
                    format: string;
                };
                outcome: {
                    type: string;
                    enum: string[];
                };
                artifacts: {
                    type: string;
                    items: {
                        $ref: string;
                    };
                };
                metadata: {
                    $ref: string;
                };
                originalPath: {
                    type: string;
                };
                originalRetrievedAt: {
                    type: string;
                    format: string;
                };
                diagnostics: {
                    type: string;
                    items: {
                        $ref: string;
                    };
                };
                sourceFingerprint: {
                    type: string;
                    pattern: string;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        ClaimProperties: {
            title: string;
            type: string;
            properties: {
                statement: {
                    type: string;
                    minLength: number;
                };
                verificationUnitIds: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                temporal: {
                    type: string;
                    enum: string[];
                };
                conflictKey: {
                    type: string;
                    minLength: number;
                };
                sourceRefs: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                fingerprints: {
                    $ref: string;
                };
                freshnessHours: {
                    type: string;
                    exclusiveMinimum: number;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        VerificationUnitProperties: {
            title: string;
            type: string;
            properties: {
                objective: {
                    type: string;
                    minLength: number;
                };
                scope: {
                    type: string;
                    minLength: number;
                };
                inputSnapshot: {
                    $ref: string;
                };
                expectedPredicate: {
                    $ref: string;
                };
                method: {
                    type: string;
                    minLength: number;
                };
                fingerprints: {
                    $ref: string;
                };
                temporal: {
                    type: string;
                    enum: string[];
                };
                freshnessHours: {
                    type: string;
                    exclusiveMinimum: number;
                };
                sourceRefs: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                objectiveVersion: {
                    type: string;
                    minLength: number;
                };
                thresholdVersion: {
                    type: string;
                    minLength: number;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        EvalDatasetProperties: {
            title: string;
            type: string;
            properties: {
                caseRefs: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                name: {
                    type: string;
                    minLength: number;
                };
                datasetVersion: {
                    type: string;
                    minLength: number;
                };
                fingerprints: {
                    $ref: string;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        EvalBudget: {
            title: string;
            type: string;
            properties: {
                maxRuns: {
                    type: string;
                    minimum: number;
                };
                maxDurationMs: {
                    type: string;
                    minimum: number;
                };
                maxTotalTokens: {
                    type: string;
                    minimum: number;
                };
            };
            required: never[];
            additionalProperties: {
                $ref: string;
            };
        };
        EvalSuiteProperties: {
            title: string;
            type: string;
            properties: {
                datasetRef: {
                    type: string;
                    minLength: number;
                };
                target: {
                    $ref: string;
                };
                evaluatorRefs: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                name: {
                    type: string;
                    minLength: number;
                };
                suiteVersion: {
                    type: string;
                    minLength: number;
                };
                iterations: {
                    type: string;
                    minimum: number;
                };
                budget: {
                    $ref: string;
                };
                objectiveVersion: {
                    type: string;
                    minLength: number;
                };
                thresholdVersion: {
                    type: string;
                    minLength: number;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        EvaluatorProperties: {
            title: string;
            type: string;
            properties: {
                method: {
                    type: string;
                    minLength: number;
                };
                name: {
                    type: string;
                    minLength: number;
                };
                evaluatorVersion: {
                    type: string;
                    minLength: number;
                };
                objectiveVersion: {
                    type: string;
                    minLength: number;
                };
                thresholdVersion: {
                    type: string;
                    minLength: number;
                };
                configuration: {
                    $ref: string;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        EvalRunProperties: {
            title: string;
            type: string;
            properties: {
                suiteRef: {
                    type: string;
                    minLength: number;
                };
                status: {
                    type: string;
                    enum: string[];
                };
                results: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                metrics: {
                    $ref: string;
                };
                startedAt: {
                    type: string;
                    format: string;
                };
                finishedAt: {
                    anyOf: ({
                        type: string;
                        format: string;
                    } | {
                        type: string;
                        format?: never;
                    })[];
                };
                diagnostics: {
                    type: string;
                    items: {
                        $ref: string;
                    };
                };
                fingerprints: {
                    $ref: string;
                };
                datasetVersion: {
                    type: string;
                    minLength: number;
                };
                suiteVersion: {
                    type: string;
                    minLength: number;
                };
                evaluatorVersions: {
                    $ref: string;
                };
                targetFingerprint: {
                    type: string;
                    pattern: string;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        CaseResultProperties: {
            title: string;
            type: string;
            properties: {
                runRef: {
                    type: string;
                    minLength: number;
                };
                unitRef: {
                    type: string;
                    minLength: number;
                };
                verdict: {
                    type: string;
                    enum: string[];
                };
                verificationClass: {
                    type: string;
                    enum: string[];
                };
                status: {
                    type: string;
                    enum: string[];
                };
                passed: {
                    anyOf: {
                        type: string;
                    }[];
                };
                metrics: {
                    type: string;
                    items: {
                        $ref: string;
                    };
                };
                fingerprints: {
                    $ref: string;
                };
                verifiedAt: {
                    type: string;
                    format: string;
                };
                diagnostics: {
                    type: string;
                    items: {
                        $ref: string;
                    };
                };
                iteration: {
                    type: string;
                    minimum: number;
                };
                evaluatorRef: {
                    type: string;
                    minLength: number;
                };
                evaluatorVersion: {
                    type: string;
                    minLength: number;
                };
                objectiveVersion: {
                    type: string;
                    minLength: number;
                };
                thresholdVersion: {
                    type: string;
                    minLength: number;
                };
                evidenceRefs: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        DecisionAuthority: {
            title: string;
            type: string;
            properties: {
                actorId: {
                    type: string;
                    minLength: number;
                };
                scope: {
                    type: string;
                    minLength: number;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
        DecisionProperties: {
            title: string;
            type: string;
            properties: {
                subjectRef: {
                    type: string;
                    minLength: number;
                };
                status: {
                    type: string;
                    enum: string[];
                };
                rationale: {
                    type: string;
                    minLength: number;
                };
                verificationRefs: {
                    type: string;
                    items: {
                        type: string;
                        minLength: number;
                    };
                    uniqueItems: boolean;
                };
                authority: {
                    $ref: string;
                };
                choice: {
                    $ref: string;
                };
            };
            required: string[];
            additionalProperties: {
                $ref: string;
            };
        };
    };
};
export declare const evidenceVocabulary: Readonly<{
    schemaVersion: string;
    id: string;
    terms: string[];
    relations: string[];
    dependencyRule: string;
    authorityBoundary: string;
}>;
export declare const evidenceRegistry: OntologyRegistry;
export declare const evidenceObjectTypes: readonly DomainObjectType[];
/** Validate domain properties without granting truth, freshness, or actual authorization. */
export declare function validateEvidenceObject(value: unknown): ValidationResult<EvidenceObject>;
export declare function assertEvidenceObject(value: unknown): asserts value is EvidenceObject;
export declare function validateEvidenceGraph(objects: readonly unknown[], links: readonly unknown[]): ReferenceValidationResult;
