import { join } from "node:path";
import { readFile, mkdir, symlink } from "node:fs/promises";
import { scratch, write, hash, bytes, capture, fileState } from "./support";
import { ownerFixture } from "./owner";
async function inputs() {
  return JSON.parse(
    await readFile(join(import.meta.dir, "../fixtures/domain-inputs.json"), "utf8"),
  ) as {
    protocol: import("../../../../packages/operational-context/src/backend/work-artifacts/lead-protocol").LeadProtocol;
    operating: import("../../../../packages/operational-context/src/backend/work-artifacts/operating-context").OperatingContext;
  };
}
export async function domainScenario(source: string, id: string): Promise<unknown> {
  if (id.startsWith("service-"))
    return scratch("service", async (root) => {
      const api = (await import(
        join(source, "packages/evidence-foundry/src/service.ts")
      )) as typeof import("../../../../packages/evidence-foundry/src/service");
      if (id === "service-policy-missing") return api.Foundry.open(join(root, "store"));
      const f = await api.Foundry.open(join(root, "store"), {
        excludedDomains: id === "service-policy-excluded" ? ["p"] : [],
      });
      const request = {
        actionType: "createClaim",
        projectId: "p",
        idempotencyKey: "fixture-create",
        input: { statement: "Fixture source choice" },
      };
      const first = await f.runAction(request);
      const duplicate = await f.runAction(request);
      const objects = await f.queryObjects({ projectId: "p" });
      const reopened = await api.Foundry.open(join(root, "store"), { excludedDomains: [] });
      const stored = await reopened.queryObjects({ projectId: "p" });
      if (!["service-policy-positive", "service-policy-excluded"].includes(id))
        throw Error("TRUSTED_CI_UNKNOWN_SERVICE_SCENARIO");
      return { first, duplicate, objects, stored };
    });
  if (id.startsWith("handoff-"))
    return scratch("handoff", async (root) => {
      const api = (await import(
        join(source, "packages/work-continuity/src/handoff.ts")
      )) as typeof import("../../../../packages/work-continuity/src/handoff");
      const input = join(root, "input"),
        receiver = join(root, "receiver");
      await mkdir(input);
      await mkdir(receiver);
      await write(input, "source.txt", "Fixture source bytes\n");
      if (id === "handoff-source-symlink")
        await symlink(join(input, "source.txt"), join(input, "alias.txt"));
      const bundle = await api.packHandoff(
        input,
        [id === "handoff-source-symlink" ? "alias.txt" : "source.txt"],
        {
          issueId: "issue",
          sourceRevision: "1",
          completedEffectIds: ["done"],
          checks: [
            {
              id: "check",
              status: "passed",
              evidencePaths: [id === "handoff-source-symlink" ? "alias.txt" : "source.txt"],
            },
          ],
          nextAction: {
            id: "next",
            action: "Observe source",
            path: id === "handoff-source-symlink" ? "alias.txt" : "source.txt",
          },
        },
      );
      if (id === "handoff-partial-publication") {
        const first = {
          path: "a",
          base64: Buffer.from("first").toString("base64"),
          sha256: hash("first"),
          byteLength: 5,
        };
        const second = {
          path: "a/b",
          base64: Buffer.from("second").toString("base64"),
          sha256: hash("second"),
          byteLength: 6,
        };
        const partial = {
          ...bundle,
          files: [first, second],
          checks: [],
          nextAction: { ...bundle.nextAction, path: "a" },
        };
        const observed = await capture(() => api.receiveHandoff(receiver, "partial", partial));
        return {
          observed,
          root: await fileState(join(receiver, "partial")),
          first: await fileState(join(receiver, "partial/a")),
          second: await fileState(join(receiver, "partial/a/b")),
        };
      }
      if (id === "handoff-custody-positive") {
        const received = await api.receiveHandoff(receiver, "received", bundle);
        const replay = await capture(() => api.receiveHandoff(receiver, "received", bundle));
        return {
          received,
          source: await fileState(join(input, "source.txt")),
          destination: await fileState(join(receiver, "received/source.txt")),
          replay,
        };
      }
      const canonical = join(root, "canonical");
      await mkdir(canonical);
      const initial = await api.initializeCanonicalHandoff(canonical, "writer", bundle);
      const token = {
        writerId: initial.writerId,
        epoch: initial.epoch,
        revision: initial.revision,
      };
      const transferred = await api.mutateCanonicalHandoff(canonical, token, "transfer", {
        kind: "transfer-writer",
        toWriterId: "successor",
      });
      if (id === "handoff-stale-writer")
        return api.mutateCanonicalHandoff(canonical, token, "stale", { kind: "commit", bundle });
      if (id === "handoff-completed-effect-loss")
        return api.mutateCanonicalHandoff(
          canonical,
          {
            writerId: transferred.writerId,
            epoch: transferred.epoch,
            revision: transferred.revision,
          },
          "loss",
          { kind: "commit", bundle: { ...bundle, completedEffectIds: [] } },
        );
      if (id !== "handoff-cas-positive") throw Error("TRUSTED_CI_UNKNOWN_HANDOFF_SCENARIO");
      return { initial, transferred, current: await api.readCanonicalHandoff(canonical) };
    });
  if (id.startsWith("projection-"))
    return scratch("projection", async (root) => {
      const f = await ownerFixture(source, root);
      const api = (await import(
        join(source, "ci/source-projection.ts")
      )) as typeof import("../../../source-projection");
      const policy = JSON.parse(
        await readFile(join(source, "architecture/runtime-filesystem-contract.json"), "utf8"),
      );
      // The pure validator requires admitted canonical addresses, not relocated physical roots.
      // These declarations are data only; no path below is opened or activated.
      const projection = policy.sourceProviderProjections[0];
      const context = { ...f.registration,
        consumerRoot: projection.canonicalConsumerRoot,
        providerRoot: projection.canonicalProviderRoot,
        sourceProjections: policy.sourceProviderProjections,
      };
      const native = JSON.parse(
        await readFile(join(f.owner, "architecture/native-codex-provider.json"), "utf8"),
      );
      Object.assign(native, { consumerRoot: projection.canonicalConsumerRoot,
        providerRoot: projection.canonicalProviderRoot, sourceProjections: policy.sourceProviderProjections });
      if (id === "projection-foreign-root") native.providerRoot = join(root, "foreign");
      else if (id !== "projection-native-positive")
        throw Error("TRUSTED_CI_UNKNOWN_PROJECTION_SCENARIO");
      return {
        observed: await capture(() =>
          api.assertSourceProjectionAgreement(policy, context, native),
        ),
        contextAuthority: f.registration.authority,
        nativePermission: native.nativePermissionGranted,
      };
    });
  if (id.startsWith("operational-intent-")) {
    const api = (await import(
      join(source, "packages/operational-context/src/backend/intent-context/resolve.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/intent-context/resolve");
    const contract = (await import(
      join(source, "packages/operational-context/src/backend/intent-context/json-ingress.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/intent-context/json-ingress");
    if (id === "operational-intent-duplicate-json")
      return contract.parseLosslessJson('{"a":1,"a":2}');
    const content = "Fictional source direction";
    const candidate = {
      schema: "backend.intent-context.candidate.v1",
      stage: "candidate",
      meaningStatus: "curated-user-source-interpretations",
      sources: [
        {
          id: "source",
          kind: "user-message",
          sessionId: "fixture",
          messageId: 1,
          contentSha256: hash(content),
        },
      ],
      decisions: [
        {
          id: "decision",
          revision: 1,
          topic: "topic",
          scope: { kind: "project", profile: "fixture", projectId: "p" },
          kind: "confirmed-direction",
          statement: content,
          sourceIds: ["source"],
          supersedes: [],
        },
      ],
    };
    const request = {
      schema: "backend.intent-context.request.v1",
      purpose: "Fictional read",
      profile: "fixture",
      projectId: "p",
      workstreamId: "w",
      topics: ["topic"],
    };
    if (!["operational-intent-positive", "operational-intent-source-negative"].includes(id))
      throw Error("TRUSTED_CI_UNKNOWN_INTENT_SCENARIO");
    return api.resolveContext(candidate, request, [
      {
        id: 1,
        session_id: "fixture",
        role: "user",
        tool_name: null,
        tool_call_id: null,
        content: id.endsWith("negative") ? "forged" : content,
        timestamp: 1,
      },
    ]);
  }
  if (id.startsWith("operational-review-")) {
    const api = (await import(
      join(source, "packages/operational-context/src/backend/review/review-evidence.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/review/review-evidence");
    const handoff = (await import(
      join(source, "packages/operational-context/src/backend/review/handoff-contract.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/review/handoff-contract");
    const content = "Source",
      request = "Fictional source review",
      sessionId = "20261020_120000_abcdef";
    const packet = JSON.stringify({
      review_id: "review",
      documents: [{ id: "doc", source: "fictional:source", content, sha256: hash(content) }],
    });
    const expected = {
      packetSha256: hash(packet),
      requestSha256: hash(request),
      sessionId,
      model: "fictional-model",
      provider: "fictional-provider",
    };
    const page = {
      review_id: "review",
      packet_sha256: hash(packet),
      document_id: "doc",
      source: "fictional:source",
      source_sha256: hash(content),
      offset: 0,
      content: id === "operational-review-forged-page" ? "forged" : content,
      total_chars: 6,
      has_more: false,
    };
    const messages = [
      { id: 1, session_id: sessionId, role: "user", content: request, tool_calls: null },
      {
        id: 2,
        session_id: sessionId,
        role: "assistant",
        content: "",
        tool_calls: JSON.stringify([
          {
            id: "call",
            function: {
              name: "ocp_review_read",
              arguments: JSON.stringify({ document_id: "doc", offset: 0, limit: 8000 }),
            },
          },
        ]),
      },
      {
        id: 3,
        session_id: sessionId,
        role: "tool",
        tool_name: "ocp_review_read",
        tool_call_id: "call",
        content: JSON.stringify(page),
        tool_calls: null,
      },
      {
        id: 4,
        session_id: sessionId,
        role: "assistant",
        content: JSON.stringify({
          review_id: "review",
          verdict: "pass",
          findings: [],
          limits: ["Fictional evidence only"],
        }),
        tool_calls: null,
      },
    ];
    if (!["operational-review-positive", "operational-review-forged-page"].includes(id))
      throw Error("TRUSTED_CI_UNKNOWN_REVIEW_SCENARIO");
    return {
      review: api.assessReview(packet, expected, {
        session: {
          id: sessionId,
          model: expected.model,
          billing_provider: expected.provider,
          ended_at: 1,
          message_count: 4,
          tool_call_count: 1,
        },
        messages,
      }),
      handoff: handoff.readHandoff({ verdict: "PASS", findings: [], limits: ["Fictional"] }),
      invalidHandoff: handoff.readHandoff({ verdict: "PASS", findings: [], limits: null }),
    };
  }
  if (/^operational-artifact-v[1-4]$/.test(id) || id === "operational-artifact-duplicate") {
    const api = (await import(
      join(source, "packages/operational-context/src/backend/work-artifacts/contract.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/work-artifacts/contract");
    const version = /v([1-4])$/.exec(id)?.[1] ?? "4";
    const record = {
      type: "constraint",
      id: "constraint",
      category: "ownership",
      rule: "Source evidence grants no native permission",
      sourceRefs: ["fictional:source"],
    };
    return api.parseArtifact({
      schema: "backend.work-artifact.v" + version,
      kind: "current_work",
      id: "fixture",
      profile:
        Number(version) < 3
          ? "default"
          : version === "3"
            ? "ontology-control-plane"
            : "ontology-platform",
      projectId: Number(version) < 3 ? "p_566ac0ba" : "p_366c7ae4",
      authority: "none",
      observedAt: "2026-10-20T00:00:00Z",
      sourceRefs: ["fictional:source"],
      records: id.endsWith("duplicate") ? [record, record] : [record],
    });
  }
  if (
    [
      "operational-runtime-reference-positive",
      "operational-runtime-reference-stale",
      "operational-runtime-reference-ambiguous",
    ].includes(id)
  ) {
    const api = (await import(
      join(source, "packages/work-continuity/src/runtime-index.ts")
    )) as typeof import("../../../../packages/work-continuity/src/runtime-index");
    const selected = {
      runtime: "native-codex",
      profileId: null,
      sessionId: "fictional-session",
      agentId: null,
      nativeProjectId: null,
      nativeTaskId: null,
      nativeRunId: null,
      logical: {
        domainId: "d",
        projectId: "p",
        workId: "w",
        issueId: "issue",
        specId: "spec",
        specRevision: 1,
        planId: "plan",
        planRevision: 1,
        taskId: "task",
      },
    };
    const entry = {
      ...selected,
      observation: {
        class: "declared",
        observedAtUtc: null,
        evidenceRefs: [],
        currentness: id.endsWith("stale") ? "stale" : "current",
      },
      sourceOnly: true,
    };
    return api.selectRuntimeReference(
      {
        schema: "native-harness.runtime-index.v1",
        entries: id.endsWith("ambiguous") ? [entry, entry] : [entry],
      },
      selected,
    );
  }
  if (["operational-docs-positive", "operational-docs-hash-negative"].includes(id))
    return scratch("docs", async (root) => {
      const api = (await import(
        join(source, "packages/work-continuity/src/runtime-docs/index.ts")
      )) as typeof import("../../../../packages/work-continuity/src/runtime-docs/index");
      const registry = JSON.parse(
        await readFile(
          join(source, "architecture/runtime-documentation-routing/registry.json"),
          "utf8",
        ),
      ) as import("../../../../packages/work-continuity/src/runtime-docs/types").DocumentationRegistry;
      const route = registry.routes.find((r) => r.id === "codex.cli.overview");
      if (!route) throw Error("SCENARIO_REQUIRED_DOCUMENTATION_ROUTE");
      const extract = "Fixture source extract\n";
      route.retainedExtract = {
        fileName: "fixture.md",
        extractSha256: hash(extract),
        extractBytes: Buffer.byteLength(extract),
        lineRanges: [[1, 1]],
        retrievedAt: null,
        etag: null,
        lastModified: null,
      };
      await write(
        root,
        "fixture.md",
        id.endsWith("negative") ? "Changed source extract\n" : extract,
      );
      const reader = api.createDocumentationReader(registry);
      const result = await reader.readSelectedExtract(
        { surface: "codex.cli", topic: "overview", freshness: "general" },
        { directory: root },
      );
      return { result, lastGap: result.plan.evidenceGaps.at(-1) ?? null };
    });
  const data = await inputs();
  if (
    [
      "operational-inception-positive",
      "operational-inception-effect-negative",
      "operational-inception-missing-interview",
    ].includes(id)
  ) {
    const api = (await import(
      join(source, "packages/operational-context/src/backend/work-artifacts/lead-protocol.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/work-artifacts/lead-protocol");
    const record = api.LeadProtocolRecordSchema.parse(data.protocol),
      binding = {
        path: "fictional:role",
        sha256: hash(bytes(record)),
        record,
        authority: "none" as const,
      };
    const subject = { profile: "fixture", revision: 1, domainLead: "owner" };
    const sources = [
      {
        id: "interview",
        revision: 1,
        kind: "human_interview",
        sha256: hash("Fictional declared admission"),
        subject,
      },
    ];
    const interview = id.endsWith("missing-interview")
      ? {}
      : {
          purpose: {
            status: "settled",
            sourceId: "interview",
            answer: "Fictional source contract",
          },
          data: {
            status: "settled",
            sourceId: "interview",
            classes: ["synthetic"],
            owner: "owner",
          },
          effects: {
            status: "settled",
            sourceId: "interview",
            allowed: ["local_contract"],
            excluded: ["native_configuration"],
          },
        };
    return api.assessInception(
      {
        schema: "backend.profile-inception.request.v2",
        requestId: "request",
        protocol: { id: record.protocolId, revision: 1, roleSha256: binding.sha256 },
        sources,
        subject,
        interview,
        ontology: {
          identity: ["source"],
          grain: "commit",
          evidence: ["interview"],
          relations: [],
          logic: ["Preserve source owners"],
          actions: [id.endsWith("effect-negative") ? "native_configuration" : "local_contract"],
          access: { dataOwner: "owner", businessDecisionOwner: "owner" },
          unknowns: [],
        },
        work: {
          action: "design_contract",
          dataClasses: ["synthetic"],
          effects: ["local_contract"],
          evaluator: "platform_and_domain",
          recovery: "Reconcile exact source",
        },
        nativePermission: "not_established",
      },
      binding,
    );
  }
  if (["operational-governance-positive", "operational-governance-unadmitted"].includes(id)) {
    const api = (await import(
      join(source, "packages/operational-context/src/backend/work-artifacts/lead-protocol.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/work-artifacts/lead-protocol");
    const record = api.LeadProtocolRecordSchema.parse(data.protocol);
    const binding = {
      path: "fictional:role",
      sha256: hash(bytes(record)),
      record,
      authority: "none" as const,
    };
    return api.assessGovernance(
      {
        schema: "backend.lead-governance.request.v1",
        requestId: "request",
        protocol: { id: record.protocolId, revision: 1, roleSha256: binding.sha256 },
        workload: {
          kind: id.endsWith("unadmitted") ? "external_event" : "mechanical",
          complexity: "low",
        },
        route: {
          tools: [],
          lifecycleWorker: false,
          restrictions: "verified",
          eventProducerAdmitted: false,
        },
      },
      binding,
    );
  }
  if (["operational-evaluation-positive", "operational-evaluation-wrong-source"].includes(id)) {
    const protocol = (await import(
      join(source, "packages/operational-context/src/backend/work-artifacts/lead-protocol.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/work-artifacts/lead-protocol");
    const api = (await import(
      join(source, "packages/operational-context/src/backend/work-artifacts/lead-evaluation.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/work-artifacts/lead-evaluation");
    const record = protocol.LeadProtocolRecordSchema.parse(data.protocol),
      binding = {
        path: "fictional:role",
        sha256: hash(bytes(record)),
        record,
        authority: "none" as const,
      };
    const criterion = {
      schema: "backend.lead-behavior-source.v1",
      evidenceClass: "synthetic",
      profile: "fixture",
      sessionId: "session",
      messageId: 1,
      role: "user",
      timestamp: 1,
      content: "Reuse the settled answer",
    };
    const behavior = {
      ...criterion,
      messageId: 2,
      role: "assistant",
      timestamp: 2,
      content: "Reuse the settled answer",
    };
    const q = {
      schema: "backend.lead-evaluation.request.v1",
      runId: "run",
      protocol: { id: record.protocolId, revision: 1, roleSha256: binding.sha256 },
      target: { profile: "fixture", sessionId: "session", evidenceClass: "synthetic" },
      suite: {
        id: "suite",
        cases: [
          {
            id: "case",
            parent: "suite",
            kind: "settled_interview",
            subject: "subject",
            criterionEvidence: {
              path: "/fictional/criterion.json",
              sha256: hash(bytes(criterion)),
              quote: criterion.content,
            },
          },
        ],
      },
      observations: [
        {
          caseId: "case",
          behavior: "reuse_answer",
          support: {
            path: "/fictional/behavior.json",
            sha256: hash(bytes(behavior)),
            quote: behavior.content,
          },
        },
      ],
      annotationAuthority: "lead_self_review",
    };
    return api.assessLeadEvaluation(q, binding, (path) => {
      const value = path.endsWith("criterion.json") ? criterion : behavior;
      return { value, sha256: id.endsWith("wrong-source") ? "0".repeat(64) : hash(bytes(value)) };
    });
  }
  if (["operational-context-positive", "operational-context-forbidden-effect"].includes(id)) {
    const api = (await import(
      join(source, "packages/operational-context/src/backend/work-artifacts/operating-context.ts")
    )) as typeof import("../../../../packages/operational-context/src/backend/work-artifacts/operating-context");
    const c = structuredClone(data.operating);
    if (id.endsWith("forbidden-effect")) c.action.requestedEffects = ["native_configuration"];
    return api.projectOperatingContext(
      c,
      [
        { id: "work", parent: null, kind: "work", owner: "owner" },
        { id: "consumer", parent: "work", kind: "consumer", owner: "owner" },
      ],
      "consumer",
      [
        {
          id: "interview",
          subject: "work",
          owner: "owner",
          status: "settled",
          answer: "Fixture source choice",
        },
      ],
      "alternative",
    );
  }
  if (["operational-wiki-positive", "operational-wiki-forged-quote"].includes(id)) {
    const api = (await import(
      join(source, "packages/operational-context/src/shared-wiki/v1/contract.ts")
    )) as typeof import("../../../../packages/operational-context/src/shared-wiki/v1/contract");
    const knowledge = {
      schema: "shared.ontology.knowledge.v1",
      revision: "1",
      authority: "knowledge-not-permission",
      sources: [
        {
          id: "source",
          url: "https://fixture.invalid/source",
          publisher: "Fictional",
          sourceClass: "synthetic-fixture",
          asOf: "2026-10-20",
          gaps: ["Fictional only"],
        },
      ],
      claims: [
        {
          id: "claim",
          sourceId: "source",
          claimClass: "SOURCE_OBSERVATION",
          quote: "Fixture source",
          scope: "fictional",
        },
      ],
      nodes: [
        {
          id: "principle",
          title: "Source ownership",
          claimRefs: ["claim"],
          claimClass: "SOURCE_OBSERVATION",
          kind: "principle",
          definition: "Source evidence only",
          decisionConsequences: ["No native authority"],
        },
      ],
      relations: [],
      coverage: [
        { area: "fixture", state: "selected-scope-qualified", gap: "No product adoption" },
      ],
    };
    return api.selectContext(
      api.parseKnowledge(
        knowledge,
        new Map([["source", id.endsWith("forged-quote") ? "forged" : "Fixture source"]]),
      ),
      { kind: "context", id: "principle", purpose: "review" },
    );
  }
  throw Error("TRUSTED_CI_UNKNOWN_DOMAIN_SCENARIO");
}
