import { describe, expect, test } from "bun:test";
import { validateArtifact, validateOntologyRegistry } from "@palantirkc/ontology";
import manifest from "../project.json";
import {
  createDomainObject, validateDomainObject, validateDomainProperties, validateObservation,
  validateOperationRequest, domainRegistry, domainObjectTypeNames,
} from "../src/domain";
import { NOW, HOST, PROFILES, snapshot, objectProperties, operation } from "./domain.fixtures";

describe("infrastructure contracts and immutable package boundary", () => {
  test("project manifest uses the pinned shared Artifact contract", () => {
    expect(validateArtifact(manifest).valid).toBe(true);
    expect(manifest.payload.ontologyPackage.version).toBe("0.2.0");
    expect(validateOntologyRegistry(domainRegistry).valid).toBe(true);
    expect(domainRegistry.objectTypes).toHaveLength(8);
    expect(domainRegistry.actionTypes).toHaveLength(0);
  });

  test("stable domain objects validate against upper envelopes and nested domain schemas", () => {
    const props = objectProperties();
    for (const name of domainObjectTypeNames) {
      const record = createDomainObject(name, `vps-operations:object:${name}:test`, props[name], { createdAt: NOW });
      expect(validateDomainObject(record).valid).toBe(true);
    }
  });

  test("domain object construction retains independent cloned properties", () => {
    const props = objectProperties().VpsHost;
    const record = createDomainObject("VpsHost", HOST, props, { createdAt: NOW });
    props.excludedEffects.push("later-change");
    expect(record.properties.excludedEffects).not.toContain("later-change");
    expect(validateDomainObject({ ...record, objectTypeVersion: "0.0.0" }).valid).toBe(false);
    expect(validateDomainObject({ ...record, properties: { ...record.properties, password: "hidden" } }).valid).toBe(false);
  });

  test("the actual sibling-down null PID observation is retained", () => {
    expect(validateObservation(snapshot()).valid).toBe(true);
    const mixed = snapshot();
    mixed.runtimes[1]!.native.profiles![0]!.liveServed = false;
    expect(validateObservation(mixed).valid).toBe(true);
  });

  test("unknown fields, non-JSON values and duplicate container identities are rejected", () => {
    const data = snapshot();
    expect(validateObservation({ ...data, environment: { API_KEY: "omitted" } }).valid).toBe(false);
    expect(validateObservation({ ...data, extra: undefined }).valid).toBe(false);
    data.runtimes[1]!.containerId = data.runtimes[0]!.containerId;
    expect(validateObservation(data).valid).toBe(false);
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(validateDomainProperties("VpsHost", cyclic).valid).toBe(false);
  });

  test("structured adapter errors reject credential-shaped strings", () => {
    const props = objectProperties().GatewayRuntime;
    props.adapterErrors = [{
      adapter: "telegram", profile: "default", category: "authentication", code: "telegram_auth_error",
      message: "Token rejected; credential value omitted.", observedAtUtc: NOW, credentialsRedacted: true,
    }];
    expect(validateDomainProperties("GatewayRuntime", props).valid).toBe(true);
    props.adapterErrors[0]!.message = "Authorization: Bearer testToken123456789";
    expect(validateDomainProperties("GatewayRuntime", props).valid).toBe(false);
  });

  test("passing verification must preserve actual support and cannot imply acceptance", () => {
    const verification = objectProperties().VerificationResult;
    expect(validateDomainProperties("VerificationResult", verification).valid).toBe(true);
    verification.criteria[0]!.status = "unverified";
    expect(validateDomainProperties("VerificationResult", verification).valid).toBe(false);
    verification.status = "unverified";
    expect(validateDomainProperties("VerificationResult", verification).valid).toBe(true);
    verification.criteria[0]!.observationRefs = ["unrelated-observation"];
    expect(validateDomainProperties("VerificationResult", verification).valid).toBe(false);
    const acceptance = objectProperties().AcceptanceDecision;
    acceptance.authority = null;
    expect(validateDomainProperties("AcceptanceDecision", acceptance).valid).toBe(false);
    acceptance.status = "deferred";
    expect(validateDomainProperties("AcceptanceDecision", acceptance).valid).toBe(true);
  });

  test("an observation wrapper cannot rewrite measured time", () => {
    const props = objectProperties().Observation;
    props.observedAtUtc = "2026-10-06T12:01:00Z";
    expect(validateDomainProperties("Observation", props).valid).toBe(false);
  });

  test("stop and restart require the exact bounded holder and preserve conversation identity", () => {
    expect(validateOperationRequest(operation()).valid).toBe(true);
    for (const kind of ["native-gateway-stop", "native-gateway-restart"] as const) {
      const request = operation();
      request.kind = kind;
      request.target.processIdentity = null;
      expect(validateOperationRequest(request).valid).toBe(false);
    }
    const wrongIncarnation = operation();
    wrongIncarnation.target.processIdentity!.startTicks++;
    expect(validateOperationRequest(wrongIncarnation).valid).toBe(false);
    const preservedTarget = operation();
    preservedTarget.preserveProcesses.push(structuredClone(preservedTarget.target.processIdentity!));
    expect(validateOperationRequest(preservedTarget).valid).toBe(false);
  });

  test("request contracts exclude broad restart, deletion, credential effects and business scope", () => {
    const request = operation();
    expect(validateOperationRequest({ ...request, scope: "academy-business" }).valid).toBe(false);
    expect(validateOperationRequest({ ...request, constraints: { ...request.constraints, broadContainerRestart: true } }).valid).toBe(false);
    expect(validateOperationRequest({ ...request, constraints: { ...request.constraints, destructiveDeletion: true } }).valid).toBe(false);
    expect(validateOperationRequest({ ...request, constraints: { ...request.constraints, exposeCredentials: true } }).valid).toBe(false);
    request.authority = null;
    expect(validateOperationRequest(request).valid).toBe(false);
    request.status = "proposed";
    expect(validateOperationRequest(request).valid).toBe(true);
  });

  test("inventory enforces measured port bounds and namespace form", () => {
    const data = snapshot();
    data.ports![0]!.port = 70000;
    expect(validateObservation(data).valid).toBe(false);
    data.ports![0]!.port = 9120;
    data.ports![0]!.pidNamespace = "206";
    expect(validateObservation(data).valid).toBe(false);
    expect(PROFILES).toHaveLength(6);
  });
});

