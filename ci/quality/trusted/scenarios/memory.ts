import { join } from "node:path";
import { readFile } from "node:fs/promises";
import { scratch, write, hash, bytes, canonical, git, commit, capture } from "./support";
const now = "2026-10-20T12:00:00Z";
/** Narrow static public 4e0 Memory/v1 callback surface. No Memory implementation or evaluator. */
interface Address {
  kind: string;
  source: { owner: string; uri: string; revision: string; digest: string };
  header: Record<string, unknown>;
  currentness: string;
  claimKey?: string;
}
interface Lease {
  readerId: string;
  policyGeneration: string;
  ownerPolicyGenerations: Record<string, string>;
  nowUtc: string;
  expiresAtUtc: string;
  assertCurrent: () => void;
  discover: () => Promise<{ entries: Address[]; gaps: unknown[]; truncated: boolean }>;
  authorize: () => Promise<object | null>;
  readOriginal: (source: unknown, permit: object) => Promise<Uint8Array>;
  acquire: (address: Address, permit: object) => Promise<Uint8Array>;
}
interface PublicMemory {
  withMemoryQuery: (
    query: unknown,
    owner: {
      withCurrentRead: <T>(query: unknown, use: (lease: Lease) => Promise<T>) => Promise<T>;
    },
    consume: (value: unknown) => unknown,
  ) => Promise<unknown>;
  createMemoryQueryView: (
    query: unknown,
    owner: unknown,
  ) => {
    withCurrent: (consume: (value: unknown) => unknown) => Promise<unknown>;
    close: () => void;
    toJSON: () => unknown;
  };
}
function localData() {
  const original = Buffer.from("Fixture source choice");
  const source = {
    repositoryId: "p",
    revision: "r1",
    relativePath: "original.txt",
    sha256: hash(original),
    byteLength: original.length,
  };
  const key = JSON.stringify([
    source.repositoryId,
    source.revision,
    source.relativePath,
    source.sha256,
  ]);
  const episode = {
    id: "episode",
    provenance: "source-linked" as const,
    sources: [{ source, fragmentIds: ["choice"] }],
  };
  const memory = {
    id: "memory",
    episodeId: "episode",
    generationId: "g1",
    policyRevision: "p1",
    text: "Fixture source choice",
    contentHash: hash("Fixture source choice"),
    sourceKeys: [key],
    scope: { kind: "repository" as const, repositoryId: "p" },
    claim: { key: "choice", value: "Fixture source choice" },
  };
  const receipt = {
    memoryId: "memory",
    episodeId: "episode",
    generationId: "g1",
    policyRevision: "p1",
    contentHash: memory.contentHash,
    sourceKeys: [key],
    provenance: "owner-verified",
    claim: memory.claim,
    scope: memory.scope,
    episodeFingerprint: hash(
      JSON.stringify({
        id: episode.id,
        provenance: episode.provenance,
        sources: [{ key, byteLength: source.byteLength, fragmentIds: ["choice"] }],
      }),
    ),
  };
  const policy = {
    revision: "p1",
    excludedSourceHashes: [] as string[],
    excludedFragmentIds: [] as string[],
    aliases: [],
    commonAdmissions: [],
  };
  return { original, source, episode, memory, receipt, policy };
}
export async function memoryScenario(sourceRoot: string, id: string): Promise<unknown> {
  if (id.startsWith("memory-v1-")) {
    const api = (await import(
      join(sourceRoot, "packages/work-continuity/src/memory/v1.ts")
    )) as PublicMemory;
    const original = Buffer.from("Fictional adopted original");
    const ref = {
      owner: "fixture-owner",
      uri: "fictional:original",
      revision: "1",
      digest: hash(original),
    };
    const document = {
      schema: "common.memory-document.v1",
      record: {
        id: "fixture-memory",
        revision: 1,
        owner: { kind: "user", userId: "fixture-user" },
        subjects: [{ kind: "project", projectId: "p" }],
        kind: "knowledge",
        status: "active",
        authority: "model-inference",
        sourceRefs: [ref],
        text: "Fixture source choice",
        allowedReaders: ["fixture-reader"],
      },
      claim: { key: "choice", value: "advisory" },
    };
    const raw = Buffer.from(canonical(document));
    const { text: _text, ...header } = document.record;
    const address: Address = {
      kind: "memory",
      source: { owner: "fixture-owner", uri: "fictional:memory", revision: "1", digest: hash(raw) },
      header,
      currentness: "current",
      claimKey: "choice",
    };
    let allowed = true,
      authorizations = 0,
      originalReads = 0,
      bodyReads = 0,
      assertions = 0;
    const permits = new WeakSet<object>();
    const lease: Lease = {
      readerId: "fixture-reader",
      policyGeneration: "g1",
      ownerPolicyGenerations: { "fixture-owner": "p1" },
      nowUtc: now,
      expiresAtUtc: id === "memory-v1-expired" ? "2026-10-19T12:00:00Z" : "2026-10-21T12:00:00Z",
      assertCurrent() {
        assertions++;
        if (id === "memory-v1-policy-drift" && assertions > 1) lease.policyGeneration = "g2";
      },
      async discover() {
        return { entries: [address], gaps: [], truncated: false };
      },
      async authorize() {
        authorizations++;
        if (!allowed || id === "memory-v1-denied") return null;
        const permit = {};
        permits.add(permit);
        return permit;
      },
      async readOriginal(_source, permit) {
        if (!permits.has(permit)) throw Error("FICTIONAL_PERMIT_REQUIRED");
        originalReads++;
        return id === "memory-v1-original-hash" ? Buffer.from("changed original") : original;
      },
      async acquire(_address, permit) {
        if (!permits.has(permit)) throw Error("FICTIONAL_PERMIT_REQUIRED");
        bodyReads++;
        return raw;
      },
    };
    const owner = {
      async withCurrentRead<T>(_query: unknown, use: (current: Lease) => Promise<T>): Promise<T> {
        return use(lease);
      },
    };
    const query = {
      schema: "common.memory-query.v1",
      purpose: "Fictional broad recall",
      query: "Fixture",
      mode: "current",
      limit: 3,
      maxChars: 16000,
    };
    if (id === "memory-v1-view-reauthorize") {
      const view = api.createMemoryQueryView(query, owner);
      const first = await view.withCurrent((value) => value);
      allowed = false;
      const second = await view.withCurrent((value) => value);
      view.close();
      const closed = await capture(() => view.withCurrent((value) => value));
      return {
        first,
        second,
        closed,
        viewJSON: view.toJSON(),
        authorizations,
        originalReads,
        bodyReads,
      };
    }
    if (
      ![
        "memory-v1-positive",
        "memory-v1-denied",
        "memory-v1-expired",
        "memory-v1-original-hash",
        "memory-v1-policy-drift",
      ].includes(id)
    )
      throw Error("TRUSTED_CI_UNKNOWN_MEMORY_V1_SCENARIO");
    return {
      result: await api.withMemoryQuery(query, owner, (value) => value),
      authorizations,
      originalReads,
      bodyReads,
    };
  }
  const api = (await import(
    join(sourceRoot, "packages/work-continuity/src/workspace/recall-request.ts")
  )) as typeof import("../../../../packages/work-continuity/src/workspace/recall-request");
  if (
    [
      "memory-local-positive",
      "memory-local-excluded",
      "memory-local-pin-negative",
      "memory-local-scope-negative",
      "memory-local-original-denied",
    ].includes(id)
  )
    return scratch("local-memory", async (root) => {
      const d = localData();
      await write(root, "original.txt", d.original);
      if (id === "memory-local-excluded") d.policy.excludedFragmentIds = ["choice"];
      const dataset = {
        schema: "codex.local-memory-dataset.v1",
        projectId: "p",
        workId: "w",
        episodes: [d.episode],
        memories: [d.memory],
        receipts: [d.receipt],
        authoritativeClaims: [],
        authoritativeReceipts: [],
        sourceBindings: [{ source: d.source, path: join(root, "original.txt") }],
      };
      const policy = {
        schema: "codex.local-memory-policy.v1",
        projectId: "p",
        workId: "w",
        activeGenerationId: "g1",
        policy: d.policy,
      };
      await write(root, "dataset.json", JSON.stringify(dataset) + "\n");
      await write(root, "policy.json", JSON.stringify(policy) + "\n");
      const request = {
        schema: "codex.local-memory-recall.request.v1",
        projectId: id === "memory-local-scope-negative" ? "foreign" : "p",
        workId: "w",
        root,
        dataset: {
          path: join(root, "dataset.json"),
          sha256:
            id === "memory-local-pin-negative"
              ? "0".repeat(64)
              : hash(await readFile(join(root, "dataset.json"))),
        },
        policy: {
          path: join(root, "policy.json"),
          sha256: hash(await readFile(join(root, "policy.json"))),
        },
        query: "Fixture",
        claimKey: "choice",
        limit: 3,
        maxChars: 16000,
      };
      await write(root, "request.json", JSON.stringify(request) + "\n");
      if (id === "memory-local-original-denied")
        await import("node:fs/promises").then((fs) => fs.rm(join(root, "original.txt")));
      return api.readLocalRecallRequest(join(root, "request.json"), {
        root,
        projectId: "p",
        workId: "w",
      });
    });
  if (
    [
      "memory-git-positive",
      "memory-git-stale-generation",
      "memory-git-head-drift",
      "memory-git-reference-only",
      "memory-git-pin-negative",
    ].includes(id)
  )
    return scratch("git-memory", async (root) => {
      await git(root, ["init", "--initial-branch=fixture"]);
      const identity = {
        schema: "common.git-work.v1" as const,
        domainId: "d",
        projectId: "p",
        workId: "w",
      };
      const purpose = {
        objective: "Fixture source choice",
        successCriteria: ["safe"],
        inScope: ["fixture"],
        outOfScope: ["native"],
      };
      const input = (name: string, content: unknown) => ({
        ...identity,
        kind: "input" as const,
        id: name,
        inputKind: "source-owner-record" as const,
        origin: {
          kind: "source-owner-capture" as const,
          ownerId: "fictional-owner",
          locator: "fictional:" + name,
        },
        content,
      });
      const purposeRef = {
        kind: "snapshot" as const,
        path: "work/p/w/inputs/purpose.json",
        fragment: "#/content",
      };
      const inventoryRef = {
        kind: "snapshot" as const,
        path: "work/p/w/inputs/inventory.json",
        fragment: "#/content",
      };
      const records: [string, unknown][] = [
        [
          "work/p/w/work.json",
          {
            ...identity,
            kind: "work",
            id: "w",
            repositoryId: "fixture",
            issueId: "issue",
            purpose,
            spec: { id: "spec", revision: 1 },
            plan: { id: "plan", revision: 1 },
            governingDecisionIds: [],
            historicalFenceIds: [],
            continuity: {
              schema: "common.git-work-continuity.v1",
              purposeSources: [purposeRef],
              effectInventorySources: [inventoryRef],
            },
          },
        ],
        [
          "work/p/w/specs/spec@1.json",
          {
            ...identity,
            kind: "spec",
            id: "spec",
            revision: 1,
            outcome: "Fixture source choice",
            criteria: ["safe"],
          },
        ],
        [
          "work/p/w/plans/plan@1.json",
          {
            ...identity,
            kind: "plan",
            id: "plan",
            revision: 1,
            specId: "spec",
            specRevision: 1,
            objective: "Preserve source",
          },
        ],
        [
          "work/p/w/tasks/task.json",
          {
            ...identity,
            kind: "task",
            id: "task",
            logical: {
              domainId: "d",
              projectId: "p",
              workId: "w",
              taskId: "task",
              issueId: "issue",
              specId: "spec",
              specRevision: 1,
              planId: "plan",
              planRevision: 1,
            },
            title: "Fixture task",
            executionClass: "source-only",
            dependsOn: [],
            decisionIds: [],
            effectIds: [],
            verificationEvidenceIds: [],
            state: {
              execution: "planned",
              verification: "pending",
              localAudit: "intent-durable",
              sharedAudit: "pending",
              formalCompletion: "pending",
            },
          },
        ],
        [purposeRef.path, input("purpose", purpose)],
        [inventoryRef.path, input("inventory", { completedEffectIds: [], uncertainEffectIds: [] })],
      ];
      for (const [path, value] of records) await write(root, path, bytes(value));
      const original = await commit(root);
      const gitWork = (await import(
        join(sourceRoot, "packages/work-continuity/src/git-work/source.ts")
      )) as typeof import("../../../../packages/work-continuity/src/git-work/source");
      const selection = {
        repositoryRoot: root,
        repositoryId: "fixture",
        commit: original,
        projectId: "p",
        workId: "w",
      };
      const reader = await gitWork.openGitWork(selection);
      const descriptor = {
        topicId: "topic",
        generationId: "g1",
        taskId: "task",
        receiptTaskIds: [],
        claims: [{ key: "choice", record: { kind: "spec", id: "spec", revision: 1 } }],
        inputIds: { request: "request", dataset: "dataset", policy: "policy" },
        query: "Fixture",
        claimKey: "choice",
        limit: 3,
        maxChars: 16000,
        policy: {
          schema: "codex.local-memory-policy.v1",
          projectId: "p",
          workId: "w",
          activeGenerationId: "g1",
          policy: localData().policy,
        },
      };
      const prepared = await api.prepareGitWorkMemoryArtifacts(reader, descriptor);
      const episode = prepared.dataset.episodes[0]!;
      const sourceKey = (s: {
        repositoryId: string;
        revision: string;
        relativePath: string;
        sha256: string;
      }) => JSON.stringify([s.repositoryId, s.revision, s.relativePath, s.sha256]);
      const fingerprint = hash(
        JSON.stringify({
          id: episode.id,
          provenance: episode.provenance,
          sources: episode.sources
            .map((r) => ({
              key: sourceKey(r.source),
              byteLength: r.source.byteLength,
              fragmentIds: [...r.fragmentIds].sort(),
            }))
            .sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0)),
        }),
      );
      const review = {
        schema: "common.git-work-memory-owner-review.v1",
        ownerId: "fictional-owner",
        bindingSha256: prepared.diagnostics.reviewBindingSha256,
        status: "reviewed",
        receipts: prepared.dataset.memories.map((m) => ({
          memoryId: m.id,
          episodeId: m.episodeId,
          generationId: m.generationId,
          policyRevision: m.policyRevision,
          contentHash: m.contentHash,
          sourceKeys: m.sourceKeys,
          provenance: "owner-verified",
          claim: m.claim,
          scope: m.scope,
          episodeFingerprint: fingerprint,
        })),
        authoritativeReceipts: prepared.dataset.authoritativeClaims.map((c) => ({
          sourceKey: sourceKey(c.source),
          recordClass: c.recordClass,
          status: c.status,
          key: c.key,
          value: c.value,
          provenance: "owner-verified",
        })),
      };
      const reviewPath = "work/p/w/inputs/review.json";
      await write(root, reviewPath, bytes(input("review", review)));
      const reviewed = await commit(root);
      const compiled = await api.prepareGitWorkMemoryArtifacts(reader, descriptor, {
        kind: "git",
        repositoryId: "fixture",
        commit: reviewed,
        path: reviewPath,
        fragment: "#/content",
        sha256: hash(await readFile(join(root, reviewPath))),
      });
      const datasetPath = "work/p/w/inputs/dataset.json",
        policyPath = "work/p/w/inputs/policy.json";
      await write(root, datasetPath, bytes(input("dataset", compiled.dataset)));
      await write(root, policyPath, bytes(input("policy", compiled.policy)));
      const request = {
        ...compiled.request,
        dataset: {
          ...compiled.request.dataset,
          sha256: hash(await readFile(join(root, datasetPath))),
        },
        policy: {
          ...compiled.request.policy,
          sha256: hash(await readFile(join(root, policyPath))),
        },
      };
      const requestPath = "work/p/w/inputs/request.json";
      await write(root, requestPath, bytes(input("request", request)));
      const admitted = await commit(root);
      const ownerRef = "refs/heads/work/fixture";
      await git(root, ["update-ref", ownerRef, admitted]);
      if (id === "memory-git-head-drift") {
        const successor = await commit(root);
        await git(root, ["update-ref", ownerRef, successor]);
      }
      const fresh = await gitWork.openGitWork({ ...selection, commit: admitted });
      return api.readGitWorkMemoryRequest(
        fresh,
        {
          kind: "snapshot",
          path: requestPath,
          fragment: "#/content",
          sha256:
            id === "memory-git-pin-negative"
              ? "0".repeat(64)
              : hash(await readFile(join(root, requestPath))),
        },
        {
          owningRef: ownerRef,
          admittedCommit: admitted,
          policyRevision: "p1",
          activeGenerationId: id === "memory-git-stale-generation" ? "g2" : "g1",
          state: "current",
        },
      );
    });
  throw Error("TRUSTED_CI_UNKNOWN_MEMORY_SCENARIO");
}
