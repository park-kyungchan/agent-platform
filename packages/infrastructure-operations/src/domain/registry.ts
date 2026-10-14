import { assertOntologyRegistry } from "@palantirkc/ontology/operational";
import type {
  ObjectTypeDefinition,
  PropertyDefinition,
  OntologyRegistry,
  LinkTypeDefinition,
} from "@palantirkc/ontology/operational";
import { domainPropertySchemas } from "./schemas";
import { DOMAIN_ID, DOMAIN_VERSION } from "./types";
import type { DomainObjectType } from "./types";

export const domainObjectTypeNames = [
  "VpsHost",
  "ContainerRuntime",
  "GatewayRuntime",
  "SharedVolume",
  "Observation",
  "VerificationResult",
  "AcceptanceDecision",
  "OperationRequest",
] as const satisfies readonly DomainObjectType[];

export const objectTypeId = <K extends DomainObjectType>(name: K): `infrastructure:${K}` =>
  `infrastructure:${name}`;
export function domainObjectType(value: string): DomainObjectType | null {
  return domainObjectTypeNames.find((name) => objectTypeId(name) === value) ?? null;
}

const references: Partial<Record<string, { type: DomainObjectType; many: boolean }>> = {
  hostId: { type: "VpsHost", many: false },
  sharedVolumeId: { type: "SharedVolume", many: false },
  observationRefs: { type: "Observation", many: true },
  verificationRefs: { type: "VerificationResult", many: true },
};
const immutable = new Set([
  "hostId",
  "backingPath",
  "source",
  "evidence",
  "observation",
  "observedAtUtc",
  "requestedAtUtc",
  "idempotencyKey",
]);
function definition(name: DomainObjectType): ObjectTypeDefinition {
  const schema = domainPropertySchemas[name];
  const properties = schema.properties as Record<string, Record<string, unknown>>;
  return {
    schemaVersion: "1.0.0",
    kind: "object-type",
    id: objectTypeId(name),
    version: DOMAIN_VERSION,
    name,
    domainId: DOMAIN_ID,
    identity: {
      property: "id",
      strategy: "provided",
      immutable: true,
      prefix: `vps-operations:object:${name}:`,
    },
    properties: Object.entries(properties).map(([key, property]): PropertyDefinition => {
      const valueType = [
        "string",
        "number",
        "integer",
        "boolean",
        "object",
        "array",
        "null",
      ].includes(String(property.type))
        ? (property.type as PropertyDefinition["valueType"])
        : "json";
      const ref = references[key];
      return {
        name: key,
        valueType,
        required: true,
        immutable: immutable.has(key),
        ...(typeof property.format === "string" ? { format: property.format } : {}),
        ...(ref ? { reference: { objectType: objectTypeId(ref.type), many: ref.many } } : {}),
      };
    }),
    additionalProperties: false,
    description: `Infrastructure ${name} record; shape validation does not grant authority or prove freshness.`,
  };
}
const link = (
  name: string,
  from: DomainObjectType,
  to: DomainObjectType,
  toMax: number | null = null,
): LinkTypeDefinition => ({
  schemaVersion: "1.0.0",
  kind: "link-type",
  id: `infrastructure:${name}`,
  version: DOMAIN_VERSION,
  name,
  from: { objectType: objectTypeId(from), cardinality: { min: 0, max: toMax } },
  to: { objectType: objectTypeId(to), cardinality: { min: 0, max: null } },
});

export const domainRegistry: OntologyRegistry = {
  schemaVersion: "1.0.0",
  id: "vps-operations:registry:infrastructure",
  version: DOMAIN_VERSION,
  objectTypes: domainObjectTypeNames.map(definition) as [
    ObjectTypeDefinition,
    ...ObjectTypeDefinition[],
  ],
  linkTypes: [
    link("host-has-container", "VpsHost", "ContainerRuntime"),
    link("container-mounts-volume", "ContainerRuntime", "SharedVolume"),
    link("container-runs-gateway", "ContainerRuntime", "GatewayRuntime"),
    link("observation-describes-host", "Observation", "VpsHost", 1),
    link("verification-uses-observation", "VerificationResult", "Observation"),
    link("acceptance-evaluates-verification", "AcceptanceDecision", "VerificationResult"),
    link("operation-targets-container", "OperationRequest", "ContainerRuntime", 1),
  ],
  // Ontology Actions are local transactions. External lifecycle requests stay records;
  // native Codex and the installed lifecycle retain the execution/authority boundary.
  actionTypes: [],
};
assertOntologyRegistry(domainRegistry);
