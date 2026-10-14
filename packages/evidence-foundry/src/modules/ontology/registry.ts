import Ajv2020 from "ajv/dist/2020.js";
import {
  assertOperationalRecord,
  validateObjectProperties,
  validateOntologyGraph,
} from "@palantirkc/ontology/operational";
import type {
  ObjectRecord,
  LinkRecord,
  ObjectTypeDefinition,
  LinkTypeDefinition,
  ActionTypeDefinition,
} from "@palantirkc/ontology/operational";
import { evidenceRegistry, assertEvidenceObject } from "@palantirkc/ontology/domains/evidence";

export class Registry {
  readonly objectTypes = new Map<string, ObjectTypeDefinition>();
  readonly linkTypes = new Map<string, LinkTypeDefinition>();
  readonly actionTypes = new Map<string, ActionTypeDefinition>();
  constructor() {
    for (const value of evidenceRegistry.objectTypes) this.register(value);
    for (const value of evidenceRegistry.linkTypes) this.register(value);
    for (const value of evidenceRegistry.actionTypes) this.register(value);
  }
  register(value: ObjectTypeDefinition | LinkTypeDefinition | ActionTypeDefinition) {
    assertOperationalRecord(value);
    const map =
      value.kind === "object-type"
        ? this.objectTypes
        : value.kind === "link-type"
          ? this.linkTypes
          : this.actionTypes;
    const key = value.id + "@" + value.version;
    if (map.has(key)) throw new Error("Duplicate registry version: " + key);
    (map as Map<string, typeof value>).set(key, value);
  }
  objectType(id: string, version = "1.0.0") {
    const value = this.objectTypes.get(id + "@" + version);
    if (!value) throw new Error("Unknown object type/version: " + id + "@" + version);
    return value;
  }
  linkType(id: string, version = "1.0.0") {
    const value = this.linkTypes.get(id + "@" + version);
    if (!value) throw new Error("Unknown link type/version: " + id + "@" + version);
    return value;
  }
  actionType(id: string, version = "1.0.0") {
    const value = this.actionTypes.get(id + "@" + version);
    if (!value) throw new Error("Unknown action type/version: " + id + "@" + version);
    return value;
  }
  validateObjects(objects: Map<string, ObjectRecord>, previous: Map<string, ObjectRecord>) {
    for (const object of objects.values()) {
      assertOperationalRecord(object);
      const type = this.objectType(object.objectType, object.objectTypeVersion);
      if (evidenceRegistry.objectTypes.some((t) => t.id === type.id)) assertEvidenceObject(object);
      const checked = validateObjectProperties(object.properties, type);
      if (!checked.valid)
        throw new Error("Property validation failed: " + JSON.stringify(checked.issues));
      for (const property of type.properties) {
        const value = object.properties[property.name];
        if (value === undefined) continue;
        if (
          property.immutable &&
          previous.has(object.id) &&
          JSON.stringify(previous.get(object.id)!.properties[property.name]) !==
            JSON.stringify(value)
        )
          throw new Error("Immutable property conflict: " + property.name);
        if (property.reference)
          for (const id of Array.isArray(value) ? value : [value]) {
            if (typeof id !== "string" || !objects.has(id))
              throw new Error("Dangling property reference: " + id);
            const target = objects.get(id)!;
            if (target.objectType !== property.reference.objectType)
              throw new Error("Property reference endpoint mismatch");
            if (target.projectId !== object.projectId && target.scope !== "shared")
              throw new Error("Property reference project scope mismatch");
          }
      }
      const prior = previous.get(object.id);
      if (
        prior &&
        (prior.objectType !== object.objectType ||
          prior.projectId !== object.projectId ||
          prior.scope !== object.scope ||
          prior.createdAt !== object.createdAt)
      )
        throw new Error("Immutable object identity conflict");
    }
  }
  validateGraph(objects: Map<string, ObjectRecord>, links: Map<string, LinkRecord>) {
    const checked = validateOntologyGraph([...objects.values()], [...links.values()], {
      schemaVersion: "1.0.0",
      id: "evidence-foundry-runtime",
      version: "1.0.0",
      objectTypes: [...this.objectTypes.values()] as [
        ObjectTypeDefinition,
        ...ObjectTypeDefinition[],
      ],
      linkTypes: [...this.linkTypes.values()],
      actionTypes: [...this.actionTypes.values()],
    });
    if (!checked.valid)
      throw new Error("Ontology graph validation: " + JSON.stringify(checked.issues));
  }
  validateActionInput(type: ActionTypeDefinition, input: unknown) {
    const validate = new Ajv2020({ strict: false, allErrors: true }).compile(type.inputSchema);
    if (!validate(input))
      throw new Error("Action input schema mismatch: " + JSON.stringify(validate.errors));
  }
}
