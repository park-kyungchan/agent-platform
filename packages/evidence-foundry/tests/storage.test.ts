import { test, expect, afterEach } from "bun:test";
import { mkdtemp, rm, writeFile, readdir, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Foundry, createObject, createLink } from "./owner-policy-fixture";
import { Store, hash, canonical } from "../src/modules/storage/store";
import { serve, ownerCredential } from "../src/api";
import { importLegacyReceipt } from "../src/adapters/legacy-receipts";
const roots: string[] = [];
const stores: Store[] = [];
async function fresh() {
  const root = await mkdtemp(join(tmpdir(), "foundry-test-"));
  roots.push(root);
  const f = await Foundry.open(root);
  stores.push(f.store);
  return f;
}
afterEach(async () => {
  for (const store of stores.splice(0)) store.close();
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});

test("blob deduplication, hash corruption and explicit cleanup preserve references", async () => {
  const f = await fresh();
  const first = await f.store.putBlob("original bytes");
  const second = await f.store.putBlob("original bytes");
  expect(first).toEqual(second);
  expect((await f.store.stats()).blobCount).toBe(1);
  const capture = await f.ingestCapture({
    projectId: "p",
    requestedUrl: "https://example.com",
    retrievedAt: new Date().toISOString(),
    outcome: "success",
    artifacts: [{ kind: "response-content", bytes: "original bytes" }],
  });
  await expect(f.store.cleanup([first.sha256])).rejects.toThrow("referenced");
  await writeFile(f.store.blobPath(first.sha256), Buffer.from("broken gzip"));
  await expect(f.store.readBlob(first.sha256)).rejects.toThrow();
  expect(await f.getObject(capture.id)).toBeDefined();
});

test("concurrent ingestion serializes committed batches and rejects conflicting versions", async () => {
  const f = await fresh();
  await Promise.all(
    Array.from({ length: 12 }, (_, i) =>
      f.putObjects([
        createObject(
          "Project",
          "p",
          { domainId: "research", name: "Project " + i },
          { id: "p-" + i },
        ),
      ]),
    ),
  );
  expect((await f.store.commits()).length).toBe(12);
  expect((await f.queryObjects({ projectId: "p" })).length).toBe(12);
  const p = (await f.getObject("p-0"))!;
  const next = {
    ...p,
    version: 2,
    updatedAt: new Date().toISOString(),
    properties: { domainId: "research", name: "updated" },
  };
  const outcomes = await Promise.allSettled([f.putObjects([next]), f.putObjects([next])]);
  expect(outcomes.filter((o) => o.status === "fulfilled")).toHaveLength(1);
  expect((await f.store.history(p.id)).length).toBe(2);
  expect((await f.store.snapshot()).audits.some((a) => a.status === "failure")).toBe(true);
});

test("failed ingest preserves acquired bytes and diagnostic", async () => {
  const f = await fresh();
  await expect(
    f.ingestCapture({
      projectId: "p",
      requestedUrl: "https://example.com",
      retrievedAt: new Date().toISOString(),
      outcome: "failure",
      artifacts: [
        { kind: "response-content", bytes: "retained" },
        { kind: "extracted-text", path: "/missing/foundry-fixture" },
      ],
    }),
  ).rejects.toThrow();
  expect(new TextDecoder().decode(await f.store.readBlob(hash("retained")))).toBe("retained");
  expect((await f.store.snapshot()).audits.at(-1)?.operation).toBe("ingestCapture");
});

test("before/after publish interruptions recover solely from canonical commits", async () => {
  const f = await fresh();
  const unpublished = createObject("Project", "p", { domainId: "research", name: "Unpublished" });
  await expect(
    f.store.transaction(() => [unpublished], {
      beforePublish: () => {
        throw new Error("simulated crash before publish");
      },
    }),
  ).rejects.toThrow();
  expect((await f.store.snapshot()).objects.size).toBe(0);
  const published = createObject("Project", "p", { domainId: "research", name: "Published" });
  await expect(
    f.store.transaction(() => [published], {
      afterPublish: () => {
        throw new Error("simulated crash after publish");
      },
    }),
  ).rejects.toThrow();
  const reopened = await new Store(f.store.root).initialize();
  stores.push(reopened);
  expect((await reopened.snapshot()).objects.has(published.id)).toBe(true);
  expect((await reopened.query({ projectId: "p", search: "Published" })).length).toBe(1);
});

test("SQLite deletion rebuilds object, links, history and FTS; backup restores exact canonical data", async () => {
  const f = await fresh();
  const project = createObject("Project", "p", { domainId: "research", name: "Pilot" });
  const capture = createObject("SourceCapture", "p", {
    url: "https://example.com",
    retrievedAt: new Date().toISOString(),
    outcome: "success",
    metadata: {},
    artifacts: [],
  });
  const link = createLink("project-capture", project, capture);
  await f.putObjects([project, capture], [link]);
  const before = {
    query: await f.queryObjects({ projectId: "p", search: "Pilot" }),
    provenance: await f.getProvenance(project.id),
  };
  await rm(join(f.store.root, "indexes"), { recursive: true, force: true });
  await import("node:fs/promises").then((fs) => fs.mkdir(join(f.store.root, "indexes")));
  await f.store.refresh();
  expect({
    query: await f.queryObjects({ projectId: "p", search: "Pilot" }),
    provenance: await f.getProvenance(project.id),
  }).toEqual(before);
  const backup = f.store.root + "-backup";
  const restoredPath = f.store.root + "-restore";
  roots.push(backup, restoredPath);
  await f.store.backup(backup);
  const restored = await Store.restore(backup, restoredPath);
  stores.push(restored);
  expect(await restored.snapshot()).toEqual(await f.store.snapshot());
});

test("typed endpoints, dangling references, cardinality and scoped queries", async () => {
  const f = await fresh();
  const project = createObject("Project", "p", { domainId: "research", name: "P" });
  const other = createObject("Project", "q", { domainId: "research", name: "Q" });
  const shared = createObject(
    "Project",
    "shared",
    { domainId: "research", name: "Shared" },
    { scope: "shared" },
  );
  await f.putObjects([project, other, shared]);
  expect((await f.queryObjects({ projectId: "p" })).map((o) => o.id)).toEqual(
    [project.id, shared.id].sort(),
  );
  expect((await f.queryObjects({ projectId: "p", includeShared: false })).length).toBe(1);
  await expect(f.putObjects([], [createLink("project-capture", project, other)])).rejects.toThrow(
    "endpoint",
  );
  await expect(
    f.putObjects([], [{ ...createLink("project-capture", project, other), to: "dangling" }]),
  ).rejects.toThrow("dangling-reference");
  const definition = {
    schemaVersion: "1.0.0" as const,
    kind: "link-type" as const,
    id: "one-project",
    version: "1.0.0",
    name: "One Project",
    from: { objectType: "Project", cardinality: { min: 0, max: 1 } },
    to: { objectType: "Project", cardinality: { min: 0, max: null } },
  };
  f.registry.register(definition);
  const sameProject = createObject("Project", "p", { domainId: "research", name: "Second" });
  const third = createObject("Project", "p", { domainId: "research", name: "Third" });
  await f.putObjects([sameProject, third]);
  await f.putObjects([], [createLink("one-project", project, sameProject)]);
  await expect(f.putObjects([], [createLink("one-project", project, third)])).rejects.toThrow(
    "cardinality",
  );
});

test("local Action audits success/failure/idempotency and simulation does not mutate objects", async () => {
  const f = await fresh();
  const object = createObject("Project", "p", { domainId: "research", name: "Simulate" });
  const request = {
    actionType: "putObject",
    projectId: "p",
    idempotencyKey: "one",
    input: { object: object as any },
  };
  expect((await f.runAction(request, { simulate: true })).status).toBe("simulated");
  expect(await f.getObject(object.id)).toBeUndefined();
  expect((await f.runAction(request)).error).toContain("Duplicate");
  expect((await f.runAction({ ...request, idempotencyKey: "two" })).status).toBe("success");
  expect(await f.getObject(object.id)).toBeDefined();
  const next = {
    ...object,
    version: 2,
    updatedAt: new Date().toISOString(),
    properties: { domainId: "research", name: "Next" },
  };
  expect(
    (
      await f.runAction({
        ...request,
        idempotencyKey: "three",
        input: { object: next as any },
        expectedVersions: { [object.id]: 9 },
      })
    ).status,
  ).toBe("failure");
  expect((await f.getObject(object.id))?.version).toBe(1);
});

test("loopback API rejects missing owner credential and calls the same query service", async () => {
  const f = await fresh();
  await f.putObjects([createObject("Project", "p", { domainId: "research", name: "API" })]);
  const server = await serve(f);
  try {
    expect(server.hostname).toBe("127.0.0.1");
    expect(
      (await fetch(new URL("query", server.url), { method: "POST", body: '{"projectId":"p"}' }))
        .status,
    ).toBe(401);
    const response = await fetch(new URL("query", server.url), {
      method: "POST",
      headers: { authorization: "Bearer " + (await ownerCredential(f.store.root)) },
      body: '{"projectId":"p"}',
    });
    expect(await response.json()).toEqual(await f.queryEvidence({ projectId: "p" }));
  } finally {
    server.stop(true);
  }
});

test("legacy import preserves original retrieval time and unknown HTTP metadata", async () => {
  const f = await fresh();
  const path = join(f.store.root, "legacy.json");
  const originalTime = "2026-01-01T00:00:00Z";
  await writeFile(
    path,
    JSON.stringify({
      schemaVersion: "1.0.0",
      kind: "evidence",
      id: "p:evidence:old",
      projectId: "p",
      domainId: "research",
      createdAt: originalTime,
      payload: {
        claim: "Source extract",
        method: "legacy CLI",
        sourceUrl: "https://example.com",
        fetchedAt: originalTime,
        result: { outcome: "success", processExitCode: 0, contentMatched: null },
        attempts: [{ attempt: 1, outcome: "success" }],
        limitations: ["Text only"],
      },
    }),
  );
  const imported = await importLegacyReceipt(f, path);
  expect(imported.properties.retrievedAt).toBe(originalTime);
  expect((imported.properties.metadata as any).httpStatus).toBeNull();
  expect(imported.properties.originalPath).toBe(path);
  expect((await importLegacyReceipt(f, path)).id).toBe(imported.id);
});

test("independent processes serialize and release kernel lock after a real interrupted commit", async () => {
  const f = await fresh();
  const fixture = new URL("fixtures/writer.ts", import.meta.url).pathname;
  const spawn = (mode: string, id: string) =>
    Bun.spawn([process.execPath, fixture, f.store.root, mode, id], {
      stdout: "pipe",
      stderr: "pipe",
    });
  const processes = Array.from({ length: 4 }, (_, i) => spawn("write", String(i)));
  const completed = await Promise.all(
    processes.map(async (p) => ({
      exit: await p.exited,
      stderr: await new Response(p.stderr).text(),
    })),
  );
  expect(completed.map((r) => r.exit)).toEqual([0, 0, 0, 0]);
  expect((await f.queryObjects({ projectId: "multiprocess" })).length).toBe(4);
  expect(await spawn("crash-before", "before").exited).toBe(137);
  expect(await f.getObject("worker-before")).toBeUndefined();
  expect(await spawn("crash-after", "after").exited).toBe(137);
  expect(await f.getObject("worker-after")).toBeDefined();
  expect(await spawn("write", "recovered").exited).toBe(0);
  expect((await f.store.verifyIntegrity()).objects).toBe(6);
}, 15000);

test("failed capture metadata redacts credential headers while retaining acquired evidence", async () => {
  const f = await fresh();
  await expect(
    f.ingestCapture({
      projectId: "p",
      requestedUrl: "https://example.com",
      retrievedAt: new Date().toISOString(),
      outcome: "failure",
      attempts: [
        {
          requestHeaders: { Authorization: "private-auth", Cookie: "private-cookie" },
          responseHeaders: { "Set-Cookie": "private-cookie" },
        },
      ],
      artifacts: [
        { kind: "response-content", bytes: "body" },
        { kind: "extracted-text", path: "/missing/foundry-fixture" },
      ],
    }),
  ).rejects.toThrow();
  const serialized = JSON.stringify((await f.store.snapshot()).audits);
  expect(serialized).not.toContain("private-auth");
  expect(serialized).not.toContain("private-cookie");
  expect(serialized).toContain("[REDACTED]");
  expect(await f.store.readBlob(hash("body"))).toBeDefined();
});

test("generic writes cannot promote a model evaluator to deterministic verification", async () => {
  const f = await fresh();
  const unit = createObject("VerificationUnit", "p", {
    objective: "One predicate",
    scope: "Fixture",
    inputSnapshot: { text: "value" },
    expectedPredicate: { operator: "nonempty", path: "text" },
    method: "native-codex",
  });
  const evaluator = createObject("Evaluator", "p", { method: "native-codex" });
  const dataset = createObject("EvalDataset", "p", { caseRefs: [unit.id] });
  const suite = createObject("EvalSuite", "p", {
    datasetRef: dataset.id,
    evaluatorRefs: [evaluator.id],
    target: { kind: "snapshot" },
  });
  const run = createObject("EvalRun", "p", { suiteRef: suite.id, status: "completed" });
  await f.putObjects([unit, evaluator, dataset, suite, run]);
  const result = createObject("CaseResult", "p", {
    runRef: run.id,
    unitRef: unit.id,
    evaluatorRef: evaluator.id,
    verdict: "supported",
    verificationClass: "deterministic-verified",
    status: "completed",
    passed: true,
  });
  await expect(f.putObjects([result])).rejects.toThrow("Model judgment");
  expect(await f.getObject(result.id)).toBeUndefined();
});

test("missing historical artifact blobs fail integrity and backup; mismatched ingest retains observed hashes", async () => {
  const f = await fresh();
  const reference = await f.ingestCapture({
    projectId: "p",
    requestedUrl: "https://example.com",
    retrievedAt: new Date().toISOString(),
    outcome: "success",
    artifacts: [{ kind: "response-content", bytes: "historical" }],
  });
  await expect(
    f.ingestCapture({
      projectId: "p",
      requestedUrl: "https://example.com",
      retrievedAt: new Date().toISOString(),
      outcome: "failure",
      artifacts: [{ kind: "response-content", bytes: "mismatch", sha256: "0".repeat(64) }],
    }),
  ).rejects.toThrow("hash mismatch");
  const diagnostic = (await f.store.snapshot()).audits.at(-1)!;
  expect((diagnostic.details.artifacts as any[])[0].sha256).toBe(hash("mismatch"));
  expect((diagnostic.details.artifacts as any[])[0].declaredSha256).toBe("0".repeat(64));
  const artifact = (reference.properties.artifacts as any[])[0];
  await rm(f.store.blobPath(artifact.sha256));
  await expect(f.store.verifyIntegrity()).rejects.toThrow();
  await expect(f.store.backup(f.store.root + "-invalid-backup")).rejects.toThrow();
});

test("restore detects a truncated backup committed head", async () => {
  const f = await fresh();
  await f.putObjects([createObject("Project", "p", { name: "Head", domainId: "research" })]);
  const backup = f.store.root + "-backup";
  const target = f.store.root + "-restored";
  roots.push(backup, target);
  await f.store.backup(backup);
  const manifest = JSON.parse(await readFile(join(backup, "backup.json"), "utf8"));
  manifest.files = manifest.files.filter((file: any) => !file.path.startsWith("commits/"));
  await writeFile(join(backup, "backup.json"), JSON.stringify(manifest));
  await expect(Store.restore(backup, target)).rejects.toThrow("head mismatch");
});

test("empty owner credential fails closed before API starts", async () => {
  const f = await fresh();
  await writeFile(join(f.store.root, "owner-credential"), "");
  await expect(serve(f)).rejects.toThrow("Invalid owner credential");
});

test("registered property references expose source lineage without explicit Links", async () => {
  const f = await fresh();
  const capture = await f.ingestCapture({
    projectId: "p",
    requestedUrl: "https://example.com",
    retrievedAt: new Date().toISOString(),
    outcome: "success",
    artifacts: [{ kind: "extracted-text", bytes: "source" }],
  });
  const unit = createObject("VerificationUnit", "p", {
    objective: "Observe source",
    scope: "One capture",
    inputSnapshot: { text: "source" },
    expectedPredicate: { operator: "nonempty", path: "text" },
    method: "source-supported",
    sourceRefs: [capture.id],
  });
  const claim = createObject("Claim", "p", {
    statement: "Source is captured",
    verificationUnitIds: [unit.id],
    sourceRefs: [capture.id],
    temporal: "fixed-revision",
  });
  await f.putObjects([unit, claim]);
  const provenance = await f.getProvenance(claim.id);
  expect(provenance.objects.map((o) => o.id).sort()).toEqual(
    [claim.id, unit.id, capture.id].sort(),
  );
  expect(provenance.references.some((r) => r.to === capture.id)).toBe(true);
  expect(provenance.links).toHaveLength(0);
});

test("legacy text hash mismatch is retained separately without asserting original provenance", async () => {
  const f = await fresh();
  const contentPath = join(f.store.root, "changed.txt");
  await writeFile(contentPath, "new text");
  const path = join(f.store.root, "old.json");
  await writeFile(
    path,
    JSON.stringify({
      schemaVersion: "1.0.0",
      kind: "evidence",
      id: "p:evidence:old",
      projectId: "p",
      domainId: "research",
      createdAt: "2026-01-01T00:00:00Z",
      payload: {
        claim: "Old extract",
        method: "CLI",
        sourceUrl: "https://example.com",
        fetchedAt: "2026-01-01T00:00:00Z",
        result: {
          outcome: "success",
          processExitCode: 0,
          contentMatched: null,
          details: { contentPath, contentSha256: hash("old text") },
        },
        attempts: [{ attempt: 1, outcome: "success" }],
        limitations: [],
      },
    }),
  );
  const imported = await importLegacyReceipt(f, path);
  expect(imported.properties.outcome).toBe("partial");
  expect((imported.properties.metadata as any).integrityMismatch).toBe(true);
  expect(
    (imported.properties.artifacts as any[]).some((a) => a.kind === "legacy-content-current"),
  ).toBe(true);
});

test("local runSuite Action publishes one batch and its simulation publishes no eval objects", async () => {
  const f = await fresh();
  const unit = createObject("VerificationUnit", "p", {
    objective: "One invariant",
    scope: "Snapshot",
    inputSnapshot: { value: 3 },
    expectedPredicate: { operator: "equals", path: "value", value: 3 },
    method: "deterministic",
    temporal: "fixed-revision",
  });
  const evaluator = createObject("Evaluator", "p", { method: "deterministic" });
  const dataset = createObject("EvalDataset", "p", { caseRefs: [unit.id] });
  const suite = createObject("EvalSuite", "p", {
    datasetRef: dataset.id,
    evaluatorRefs: [evaluator.id],
    target: { kind: "snapshot" },
  });
  await f.putObjects([unit, evaluator, dataset, suite]);
  const before = (await f.store.commits()).length;
  const request = {
    actionType: "runSuite",
    projectId: "p",
    idempotencyKey: "suite-action",
    input: { suiteId: suite.id },
  };
  const result = await f.runAction(request);
  expect(result.status).toBe("success");
  expect((await f.store.commits()).length).toBe(before + 1);
  expect(
    result.objects.some(
      (o) => o.objectType === "CaseResult" && o.properties.verdict === "supported",
    ),
  ).toBe(true);
  expect(result.objects.find((o) => o.objectType === "EvalRun")?.version).toBe(1);
  const objectCount = (await f.store.snapshot()).objects.size;
  const simulation = await f.runAction(
    { ...request, idempotencyKey: "simulate-suite" },
    { simulate: true },
  );
  expect(simulation.status).toBe("simulated");
  expect((await f.store.snapshot()).objects.size).toBe(objectCount);
});

test("capture Action simulation computes hashes without adding raw blobs", async () => {
  const f = await fresh();
  const path = join(f.store.root, "input.txt");
  await writeFile(path, "input");
  const result = await f.runAction(
    {
      actionType: "ingestCapture",
      projectId: "p",
      idempotencyKey: "simulation",
      input: {
        capture: {
          projectId: "p",
          requestedUrl: "https://example.com",
          retrievedAt: new Date().toISOString(),
          outcome: "success",
          artifacts: [{ kind: "extracted-text", path }],
        },
      },
    },
    { simulate: true },
  );
  expect(result.status).toBe("simulated");
  expect((await f.store.stats()).blobCount).toBe(0);
  expect((await f.store.snapshot()).objects.size).toBe(0);
});

test("malformed Action input records failure without publishing operation objects", async () => {
  const f = await fresh();
  const result = await f.runAction(null as any);
  expect(result.status).toBe("failure");
  expect((await f.store.snapshot()).audits.at(-1)?.status).toBe("failure");
  expect((await f.store.snapshot()).objects.size).toBe(0);
});
