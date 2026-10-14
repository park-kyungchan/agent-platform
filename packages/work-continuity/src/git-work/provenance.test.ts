import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { readGitWorkContext } from "./context.ts";
import { gitRecordPath, openGitWork } from "./source.ts";
import type {
  GitWorkRecord,
  JsonValue,
  NativeSource,
  SnapshotSource,
  SourceReference,
} from "./types.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
const scope = {
  domainId: "native-harness",
  projectId: "provenance-fixture",
  workId: "source-owners",
};
const base = { schema: "common.git-work.v1" as const, ...scope };
const repositoryId = "fixture/provenance";
const proof: GitWorkRecord = {
  ...base,
  kind: "input",
  id: "proof",
  inputKind: "source-owner-record",
  origin: { ownerId: "fixture", locator: "fixture:source", kind: "source-owner-capture" },
  content: { observed: "one synthetic blob shared by multiple declarations" },
};
const proofBytes = JSON.stringify(proof);
const receipt: SnapshotSource = {
  kind: "snapshot",
  path: gitRecordPath(scope.projectId, scope.workId, { kind: "input", id: "proof" }),
  fragment: "#/content",
  sha256: createHash("sha256").update(proofBytes).digest("hex"),
};
const native = (ownerId: string, locator: string): NativeSource => ({
  kind: "native",
  ownerId,
  locator,
  receipt: structuredClone(receipt),
});
const declarations: [string, SourceReference][] = [
  ["snapshot", receipt],
  ["owner-a", native("owner-a", "native:a:1")],
  ["owner-b", native("owner-b", "native:b:1")],
  ["owner-a-other-locator", native("owner-a", "native:a:2")],
  ["owner-a-repeat", native("owner-a", "native:a:1")],
];
async function git(root: string, ...args: string[]): Promise<string> {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    stdout: "pipe",
    stderr: "pipe",
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "Fixture",
      GIT_AUTHOR_EMAIL: "fixture@example.invalid",
      GIT_COMMITTER_NAME: "Fixture",
      GIT_COMMITTER_EMAIL: "fixture@example.invalid",
    },
  });
  const [out, err, status] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (status !== 0) throw Error(`provenance fixture git ${args[0]}: ${err}`);
  return out.trim();
}
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "git-native-provenance-"));
  roots.push(root);
  const records: GitWorkRecord[] = [
    {
      ...base,
      kind: "work",
      id: scope.workId,
      repositoryId,
      issueId: "issue",
      purpose: {
        objective: "Keep declared native owners beside source proof",
        successCriteria: ["No declaration loss"],
        inScope: ["Source-only fixture"],
        outOfScope: ["Native authentication"],
      },
      spec: { id: "spec", revision: 1 },
      plan: { id: "plan", revision: 1 },
      governingDecisionIds: [],
      historicalFenceIds: [],
    },
    {
      ...base,
      kind: "spec",
      id: "spec",
      revision: 1,
      outcome: "Retained declarations",
      criteria: ["Source identity remains scoped"],
    },
    {
      ...base,
      kind: "plan",
      id: "plan",
      revision: 1,
      specId: "spec",
      specRevision: 1,
      objective: "Read the same committed blob through different source declarations",
    },
    {
      ...base,
      kind: "task",
      id: "inspect",
      title: "Inspect source declarations",
      logical: {
        ...scope,
        issueId: "issue",
        specId: "spec",
        specRevision: 1,
        planId: "plan",
        planRevision: 1,
        taskId: "inspect",
      },
      state: {
        execution: "planned",
        verification: "pending",
        localAudit: "intent-durable",
        sharedAudit: "pending",
        formalCompletion: "pending",
      },
      executionClass: "source-only",
      dependsOn: [],
      decisionIds: [],
      effectIds: [],
      verificationEvidenceIds: declarations.map(([id]) => id),
    },
    proof,
    ...declarations.map(([id, source]) => ({
      ...base,
      kind: "evidence" as const,
      id,
      evidenceKind: "observation" as const,
      source,
    })),
  ];
  await git(root, "init", "-q", "-b", "main");
  for (const record of records) {
    const path = join(
      root,
      gitRecordPath(scope.projectId, scope.workId, {
        kind: record.kind,
        id: record.id,
        ...(record.kind === "spec" || record.kind === "plan" ? { revision: record.revision } : {}),
      }),
    );
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, JSON.stringify(record));
  }
  await git(root, "add", ".");
  await git(root, "commit", "-qm", "Disposable provenance fixture");
  const commit = await git(root, "rev-parse", "HEAD");
  return openGitWork({
    repositoryRoot: root,
    repositoryId,
    commit,
    projectId: scope.projectId,
    workId: scope.workId,
  });
}

test("readSource retains a frozen complete native declaration alongside its Git receipt proof", async () => {
  const reader = await fixture();
  const declaration = native("owner-a", "native:a:1");
  const expected = structuredClone(declaration);
  const pending = reader.readSource(declaration);
  declaration.ownerId = "changed-after-call";
  declaration.locator = "changed-after-call";
  declaration.receipt.path = "changed-after-call.json";
  const result = await pending;
  expect(result.declaredNativeSource).toEqual(expected);
  expect(result.reference).toEqual(receipt);
  expect(result.value).toEqual(proof.content);
  expect(result.commit).toBe(reader.commit);
  expect(result.sha256).toBe(receipt.sha256!);
  expect(result.verificationClass).toBe("git-object-source-only");
  expect(result.nativeAuthorshipVerified).toBe(false);
  expect(Object.isFrozen(result.declaredNativeSource)).toBe(true);
  expect(Object.isFrozen(result.declaredNativeSource!.receipt)).toBe(true);
  expect(() => Object.assign(result.declaredNativeSource!, { ownerId: "forged" })).toThrow();
  expect(() => Object.assign(result.declaredNativeSource!.receipt, { fragment: "#" })).toThrow();
});

test("context preserves snapshot/native owners/locators separately and reuses identical declarations", async () => {
  const context = await readGitWorkContext(await fixture(), {
    taskId: "inspect",
    latestRelevant: "current",
  });
  expect(context.status).toBe("source-qualified");
  expect(context.sources).toHaveLength(4);
  const projected = new Map(
    context.records
      .filter((record) => record.kind === "evidence")
      .map((record) => [record.id, record.source]),
  );
  const sourceIds = ["snapshot", "owner-a", "owner-b", "owner-a-other-locator"].map((id) =>
    projected.get(id),
  );
  expect(new Set(sourceIds).size).toBe(4);
  expect(projected.get("owner-a-repeat")).toBe(projected.get("owner-a"));
  expect(new Set(context.sources.map((source) => source.blobOid)).size).toBe(1);
  const rows = new Map(context.sources.map((source) => [source.id, source]));
  expect(rows.get(String(projected.get("snapshot")))!.declaredNativeSource).toBeUndefined();
  for (const [id, declaration] of declarations.filter(([, source]) => source.kind === "native")) {
    if (declaration.kind !== "native") throw Error("Native fixture declaration required");
    const row = rows.get(String(projected.get(id)))!;
    expect(row.declaredNativeSource).toEqual(declaration);
    expect(row.nativeAuthorshipVerified).toBe(false);
    expect(Object.isFrozen(row.declaredNativeSource)).toBe(true);
    expect(Object.isFrozen(row.declaredNativeSource!.receipt)).toBe(true);
  }
  expect(context.authority).toBe("none");
  expect(context.nativePermissionGranted).toBe(false);
  expect(context.semanticAcceptanceEstablished).toBe(false);
});

test("ordinary snapshot and explicit Git reads preserve their existing proof shape", async () => {
  const reader = await fixture();
  for (const reference of [
    receipt,
    { ...receipt, kind: "git" as const, repositoryId, commit: reader.commit },
  ]) {
    const result = await reader.readSource(reference);
    expect(result.reference).toEqual(reference);
    expect(Object.hasOwn(result, "declaredNativeSource")).toBe(false);
    expect(result.value).toEqual(proof.content);
    expect(result.document).toEqual(JSON.parse(proofBytes) as JsonValue);
    expect(result.nativeAuthorshipVerified).toBe(false);
  }
});

test("readSource rejects invalid native owner declarations before source use", async () => {
  const reader = await fixture();
  for (const declaration of [
    native(" ", "native:a:1"),
    native("owner-a", ""),
    native("owner\0a", "native:a:1"),
  ]) {
    await expect(reader.readSource(declaration)).rejects.toThrow("INVALID_NATIVE_SOURCE_REFERENCE");
  }
});
