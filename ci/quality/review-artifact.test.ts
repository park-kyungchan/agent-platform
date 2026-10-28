import { expect, test } from "bun:test";
import { ownerScopedReviewGate } from "./receipt-validation-gate";
import { createHash } from "node:crypto";
import {
  authenticatedReviewReader,
  validateReviewArtifact,
  validatePublishedReviewArtifact,
  validateCompletedReviewObservation,
  type ReviewArtifactReader,
} from "./review-artifact";
import { deriveReviewedSourceManifest } from "./receipt-validator";
import { reviewGate } from "../source-check";

const hash = (v: Uint8Array) => createHash("sha256").update(v).digest("hex");
function zip(text: string, name = "native-review.json") {
  const data = Buffer.from(text),
    filename = Buffer.from(name),
    local = Buffer.alloc(30),
    central = Buffer.alloc(46),
    end = Buffer.alloc(22);
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  crc = (crc ^ 0xffffffff) >>> 0;
  local.writeUInt32LE(0x04034b50);
  local.writeUInt16LE(20, 4);
  local.writeUInt32LE(crc, 14);
  local.writeUInt32LE(data.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(filename.length, 26);
  central.writeUInt32LE(0x02014b50);
  central.writeUInt16LE(20, 6);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(data.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(filename.length, 28);
  end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(central.length + filename.length, 12);
  end.writeUInt32LE(local.length + filename.length + data.length, 16);
  return Buffer.concat([local, filename, data, central, filename, end]);
}
function fixture(owner = false) {
  const head = "a".repeat(40),
    tree = "b".repeat(40),
    repository = "fixture/owner";
  const publisher = { id: "42", login: "native-publisher", type: "User" as const };
  const binding: Record<string, string> = owner
    ? {
        repository,
        projectId: "fixture-owner",
        domainId: "fixture-domain",
        semanticOwner: "fixture-owner",
        scope: "source-check",
      }
    : {
        codeOwner: "agent-platform",
        evidenceOwner: "codex-harness-engineering",
        scope: "canonical-source-ci-quality",
      };
  const treeResponse = {
    sha: tree,
    truncated: false,
    tree: [{ path: "source.ts", type: "blob", mode: "100644", sha: "c".repeat(40) }],
  };
  const raw = {
    schema: owner ? "owner-scoped-native-review-output.v1" : "native-independent-review-output.v1",
    reviewStatus: "complete",
    binding: { ...binding },
    headSha: head,
    reviewedTreeSha: tree,
    reviewedSourceManifestSha256: deriveReviewedSourceManifest(treeResponse, tree).sha256,
    reviewer: { runtime: "claude", taskId: "independent-native-review" },
    author: { runtime: "codex", taskId: "actual-author" },
    conclusion: "pass",
    unresolvedFindings: 0,
    findings: [],
    nativeReviewPendingStatusId: 32,
  };
  const actor = { ...publisher, id: 42 },
    repo = { id: 123, full_name: repository };
  const run = {
    id: 11,
    run_attempt: 1,
    event: "workflow_dispatch",
    status: "completed",
    conclusion: "success",
    actor,
    triggering_actor: { ...actor },
    repository: repo,
    head_repository: { ...repo },
    head_sha: head,
    head_branch: "main",
    path: ".github/workflows/native-review-evidence.yml",
    display_title: "Native review evidence 00000000-0000-4000-8000-000000000001",
  };
  const artifact = {
    id: 22,
    name: `native-review-${head}`,
    expired: false,
    size_in_bytes: 2000,
    digest: "",
    workflow_run: { id: 11, repository_id: 123, head_repository_id: 123, head_sha: head },
  };
  const status = {
    id: 33,
    context: "native-independent-review",
    state: "success",
    creator: { ...actor, type: "User" as "User" | "Bot" },
    target_url: "",
  };
  const pending = {
    ...status,
    id: 32,
    state: "pending",
    target_url: `https://github.com/${repository}/commit/${head}#native-independent-review`,
  };
  let between: (typeof status)[] = [],
    includePending = true;
  let overrideStatuses: unknown[] | undefined;
  let bytes = Buffer.alloc(0),
    currentStatus = status;
  function seal(name?: string) {
    bytes = zip(JSON.stringify(raw), name);
    artifact.digest = "sha256:" + hash(bytes);
    status.target_url = `https://github.com/${repository}/actions/runs/11/artifacts/22#sha256=${hash(bytes)}`;
  }
  seal();
  const reader: ReviewArtifactReader = {
    async archive(id) {
      expect(id).toBe(22);
      return bytes;
    },
    async json(path) {
      if (path === "actions/runs/11") return run;
      if (path === "actions/artifacts/22") return artifact;
      if (path === `git/commits/${head}`) return { sha: head, tree: { sha: tree } };
      if (path === `git/trees/${tree}?recursive=1`) return treeResponse;
      if (path === `commits/${head}/statuses?per_page=100`)
        return (
          overrideStatuses ?? [currentStatus, ...between, ...(includePending ? [pending] : [])]
        );
      throw Error("unexpected fixture path");
    },
  };
  return {
    head,
    tree,
    raw,
    run,
    artifact,
    status,
    pending,
    setStatuses(values: unknown[]) {
      overrideStatuses = values;
    },
    reader,
    input: { policy: { repository, publisher, binding }, head, status },
    seal,
    supersede() {
      between = [
        { ...pending, id: 34, target_url: pending.target_url.replace(/000001$/, "000002") },
      ];
    },
    noAttempt() {
      includePending = false;
    },
    hold() {
      currentStatus = { ...status, state: "pending" };
    },
    corrupt() {
      bytes[40] ^= 1;
    },
  };
}

function mergeCorrespondenceFixture(owner = false) {
  const f = fixture(owner),
    parent = "e".repeat(40),
    base = "d".repeat(40),
    receiptCommit = "f".repeat(40),
    resultCommit = "1".repeat(40);
  const parentResult: Record<string, unknown> = {
    ...structuredClone(f.raw),
    schema: "agent-platform.independent-review-result.v1",
    recordClass: "verification",
    headSha: parent,
    nativeReviewPendingStatusId: 40,
  };
  const parentStatus = {
    ...f.status,
    creator: structuredClone(f.status.creator),
    id: 41,
    target_url: `https://github.com/${f.input.policy.repository}/blob/${receiptCommit}/native-reviews/${parent}.json`,
  };
  const parentPending = {
    ...f.pending,
    creator: structuredClone(f.pending.creator),
    id: 40,
    target_url: `https://github.com/${f.input.policy.repository}/commit/${parent}#native-independent-review`,
  };
  const receipt: Record<string, unknown> = {};
  const assignment = {
    kind: "merge-correspondence",
    schema: "native-review-merge-correspondence.v1",
    orderedParents: [base, parent],
    parentReview: {
      statusId: parentStatus.id,
      targetUrl: parentStatus.target_url,
      proofSha256: "",
    },
  };
  const state = { protected: true, mainHead: f.head, parents: [base, parent], parentTree: f.tree };
  const file = (path: string, body: unknown) => {
    const bytes = Buffer.from(JSON.stringify(body));
    return {
      type: "file",
      path,
      encoding: "base64",
      content: bytes.toString("base64"),
      size: bytes.length,
    };
  };
  function sealParent() {
    Object.assign(receipt, parentResult, {
      schema: "native-independent-review.v2",
      provenance: {
        owner: "codex-harness-engineering",
        resultRef: `https://github.com/${f.input.policy.repository}/blob/${resultCommit}/native-review-results/${parent}.json`,
        sha256: hash(Buffer.from(JSON.stringify(parentResult))),
      },
    });
    assignment.parentReview.proofSha256 = hash(Buffer.from(JSON.stringify(receipt)));
  }
  sealParent();
  f.raw.schema = owner
    ? "owner-scoped-native-merge-correspondence-output.v1"
    : "native-independent-merge-correspondence-output.v1";
  (f.raw as Record<string, unknown>).reviewAssignment = assignment;
  f.seal();
  const reader: ReviewArtifactReader = {
    archive: f.reader.archive,
    async json(path) {
      if (path === "branches/main")
        return { name: "main", protected: state.protected, commit: { sha: state.mainHead } };
      if (path === `git/commits/${f.head}`)
        return {
          sha: f.head,
          tree: { sha: f.tree },
          parents: state.parents.map((sha) => ({ sha })),
        };
      if (path === `git/commits/${parent}`) return { sha: parent, tree: { sha: state.parentTree } };
      if (path === `commits/${parent}/statuses?per_page=100`) return [parentStatus, parentPending];
      if (path === `contents/native-reviews/${parent}.json?ref=${receiptCommit}`)
        return file(`native-reviews/${parent}.json`, receipt);
      if (path === `contents/native-review-results/${parent}.json?ref=${resultCommit}`)
        return file(`native-review-results/${parent}.json`, parentResult);
      return f.reader.json(path);
    },
  };
  return {
    ...f,
    reader,
    assignment,
    state,
    parentStatus,
    parentPending,
    parentResult,
    receipt,
    sealParent,
  };
}

function withArtifactParent(f: ReturnType<typeof mergeCorrespondenceFixture>, nested = false) {
  const parent = f.assignment.orderedParents[1],
    raw = {
      ...structuredClone(f.raw),
      headSha: parent,
      nativeReviewPendingStatusId: 40,
      schema: Object.hasOwn(f.input.policy.binding, "codeOwner")
        ? "native-independent-review-output.v1"
        : "owner-scoped-native-review-output.v1",
    } as Record<string, unknown>;
  delete raw.reviewAssignment;
  if (nested) raw.reviewAssignment = structuredClone(f.assignment);
  const rawBytes = Buffer.from(JSON.stringify(raw)),
    archive = zip(rawBytes.toString()),
    digest = hash(archive);
  f.parentStatus.target_url = `https://github.com/${f.input.policy.repository}/actions/runs/15/artifacts/25#sha256=${digest}`;
  f.assignment.parentReview.targetUrl = f.parentStatus.target_url;
  f.assignment.parentReview.proofSha256 = hash(rawBytes);
  f.seal();
  const json = f.reader.json.bind(f.reader),
    readArchive = f.reader.archive.bind(f.reader);
  f.reader.archive = async (id) => (id === 25 ? archive : readArchive(id));
  f.reader.json = async (path) => {
    if (path === "actions/runs/15") return { ...f.run, id: 15, head_sha: parent };
    if (path === "actions/artifacts/25")
      return {
        ...f.artifact,
        id: 25,
        name: `native-review-${parent}`,
        digest: `sha256:${digest}`,
        workflow_run: { ...f.artifact.workflow_run, id: 15, head_sha: parent },
      };
    return json(path);
  };
  return f;
}

for (const owner of [false, true])
  test(
    "MERGE-CORRESPONDENCE accepts authenticated full artifact parent " +
      (owner ? "owner" : "platform"),
    async () => {
      const f = withArtifactParent(mergeCorrespondenceFixture(owner));
      expect((await validateReviewArtifact(f.input, f.reader)).headSha).toBe(f.head);
    },
  );

test("MERGE-CORRESPONDENCE rejects nested bounded artifact parent", async () => {
  const f = withArtifactParent(mergeCorrespondenceFixture(), true);
  await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow("FULL_PARENT_REQUIRED");
});

test("MERGE-CORRESPONDENCE current ordered same-tree main with genuine full parent proof remains eligible", async () => {
  const f = mergeCorrespondenceFixture();
  expect((await validateReviewArtifact(f.input, f.reader)).headSha).toBe(f.head);
});

const mergeMutations: [string, (f: ReturnType<typeof mergeCorrespondenceFixture>) => void][] = [
  [
    "unprotected-main",
    (f) => {
      f.state.protected = false;
    },
  ],
  [
    "stale-main",
    (f) => {
      f.state.mainHead = "2".repeat(40);
    },
  ],
  [
    "swapped-parents",
    (f) => {
      f.state.parents.reverse();
    },
  ],
  [
    "single-parent",
    (f) => {
      f.state.parents.pop();
    },
  ],
  [
    "different-reviewed-tree",
    (f) => {
      f.state.parentTree = "2".repeat(40);
    },
  ],
  [
    "held-parent",
    (f) => {
      f.parentStatus.state = "pending";
    },
  ],
  [
    "foreign-parent-publisher",
    (f) => {
      f.parentStatus.creator.login = "foreign";
    },
  ],
  [
    "superseded-parent-status",
    (f) => {
      f.parentStatus.id++;
    },
  ],
  [
    "substituted-parent-digest",
    (f) => {
      f.assignment.parentReview.proofSha256 = "0".repeat(64);
    },
  ],
  [
    "foreign-parent-url",
    (f) => {
      f.assignment.parentReview.targetUrl = f.parentStatus.target_url.replace(
        "fixture/owner",
        "foreign/owner",
      );
    },
  ],
  [
    "nested-bounded-parent",
    (f) => {
      f.parentResult.reviewAssignment = structuredClone(f.assignment);
      f.sealParent();
    },
  ],
  [
    "successor-parent",
    (f) => {
      f.receipt.schema = "native-independent-review-successor.v1";
    },
  ],
  [
    "unknown-assignment-key",
    (f) => {
      (f.assignment as Record<string, unknown>).override = true;
    },
  ],
];
for (const [name, mutate] of mergeMutations)
  test("MERGE-CORRESPONDENCE rejects " + name, async () => {
    const f = mergeCorrespondenceFixture();
    mutate(f);
    f.seal();
    await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow();
  });

for (const phase of ["source-manifest", "terminal-status"] as const)
  test("MERGE-CORRESPONDENCE rejects parent hold during " + phase, async () => {
    const f = mergeCorrespondenceFixture(),
      json = f.reader.json.bind(f.reader);
    f.reader.json = async (path) => {
      if (
        path ===
        (phase === "source-manifest"
          ? `git/trees/${f.tree}?recursive=1`
          : `commits/${f.head}/statuses?per_page=100`)
      )
        f.parentStatus.state = "pending";
      return json(path);
    };
    await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow();
  });

test("MERGE-CORRESPONDENCE absent parent proof cannot be consumed as a full review", async () => {
  const f = fixture();
  (f.raw as Record<string, unknown>).reviewAssignment = {
    kind: "merge-correspondence",
    schema: "native-review-merge-correspondence.v1",
    orderedParents: ["d".repeat(40), "e".repeat(40)],
    parentReview: {
      statusId: 41,
      targetUrl: "https://github.com/fixture/owner/invalid",
      proofSha256: "f".repeat(64),
    },
  };
  f.seal();
  await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow();
});

for (const mode of ["platform-legacy", "platform-artifact", "owner-artifact"] as const)
  test(
    "CURRENTNESS T0 consumed-evidence contract rejects every pre-check authority invalidation " +
      mode,
    async () => {
      const fixture = () =>
        mode === "platform-legacy"
          ? mergeCorrespondenceFixture()
          : withArtifactParent(mergeCorrespondenceFixture(mode === "owner-artifact"));
      const baseline = fixture(),
        boundaries: string[] = [],
        baselineJson = baseline.reader.json.bind(baseline.reader),
        baselineArchive = baseline.reader.archive.bind(baseline.reader);
      await validateReviewArtifact(baseline.input, {
        json: async (path) => {
          boundaries.push("json:" + path);
          return baselineJson(path);
        },
        archive: async (id) => {
          boundaries.push("archive:" + id);
          return baselineArchive(id);
        },
      });
      // Root-adopted Guider proposal (not user-ratified): T0 starts ONE final REST batch after proof consumption.
      // There is no claim of transactional or simultaneous return-time currentness.
      // The old 312-case return-time sweep remains failed immutable evidence.
      const terminal = [
        "json:branches/main",
        `json:commits/${baseline.head}/statuses?per_page=100`,
        `json:commits/${baseline.assignment.orderedParents[1]}/statuses?per_page=100`,
      ];
      const terminalPresent = JSON.stringify(boundaries.slice(-3)) === JSON.stringify(terminal);
      const proofBoundaries = terminalPresent ? boundaries.slice(0, -3) : boundaries;
      const failures: { anchor: string; ordinal: number; boundary: string }[] = [];
      let cases = 0;
      for (const anchor of ["head-status", "parent-status", "protected-main"] as const)
        for (let selected = 0; selected < proofBoundaries.length; selected++) {
          const f = fixture(),
            json = f.reader.json.bind(f.reader),
            archive = f.reader.archive.bind(f.reader);
          let ordinal = 0,
            injected = false;
          const before = () => {
            if (ordinal++ !== selected) return;
            injected = true;
            if (anchor === "head-status")
              f.setStatuses([{ ...f.pending, id: 34 }, f.status, f.pending]);
            if (anchor === "parent-status") f.parentStatus.state = "pending";
            if (anchor === "protected-main") f.state.protected = false;
          };
          try {
            await validateReviewArtifact(f.input, {
              json: async (path) => {
                before();
                return json(path);
              },
              archive: async (id) => {
                before();
                return archive(id);
              },
            });
            failures.push({ anchor, ordinal: selected, boundary: boundaries[selected]! });
          } catch {
            expect(injected).toBe(true);
          }
          cases++;
        }
      expect(cases).toBeGreaterThan(30);
      expect(failures).toEqual([]);
      expect(boundaries.slice(-3)).toEqual(terminal);
      const parentProofRead =
        mode === "platform-legacy"
          ? `json:contents/native-review-results/${baseline.assignment.orderedParents[1]}.json?ref=${"1".repeat(40)}`
          : "archive:25";
      expect(boundaries.filter((path) => path === parentProofRead)).toHaveLength(1);
    },
  );

test("MERGE-CORRESPONDENCE foreign parent predecessor witness uses an independent main publisher object", async () => {
  const prior = Bun.spawnSync(
    ["git", "show", "c49f75c6d912aa7662344575a7109a67950b285d:ci/quality/review-artifact.ts"],
    { cwd: import.meta.dir, stdout: "pipe", stderr: "pipe" },
  );
  expect(prior.exitCode).toBe(0);
  const source = prior.stdout
    .toString()
    .replaceAll(
      '"./artifact-archive"',
      JSON.stringify(new URL("./artifact-archive.ts", import.meta.url).href),
    )
    .replaceAll(
      '"./receipt-validator"',
      JSON.stringify(new URL("./receipt-validator.ts", import.meta.url).href),
    );
  const old = await import(
    "data:text/javascript;base64," +
      Buffer.from(new Bun.Transpiler({ loader: "ts" }).transformSync(source)).toString("base64")
  );
  const f = mergeCorrespondenceFixture();
  // Retain the predecessor's scope-confusing envelope for its genuine counterexample.
  f.raw.schema = "native-independent-review-output.v1";
  f.seal();
  f.parentStatus.creator.login = "foreign";
  expect(f.status.creator.login).toBe("native-publisher");
  expect((await old.validateReviewArtifact(f.input, f.reader)).headSha).toBe(f.head);
  await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow();
});

test("MERGE-CORRESPONDENCE bounded schema is rejected by the actual old full consumer", async () => {
  const prior = Bun.spawnSync(
    ["git", "show", "c49f75c6d912aa7662344575a7109a67950b285d:ci/quality/review-artifact.ts"],
    { cwd: import.meta.dir, stdout: "pipe", stderr: "pipe" },
  );
  expect(prior.exitCode).toBe(0);
  const source = prior.stdout
    .toString()
    .replaceAll(
      '"./artifact-archive"',
      JSON.stringify(new URL("./artifact-archive.ts", import.meta.url).href),
    )
    .replaceAll(
      '"./receipt-validator"',
      JSON.stringify(new URL("./receipt-validator.ts", import.meta.url).href),
    );
  const old = await import(
    "data:text/javascript;base64," +
      Buffer.from(new Bun.Transpiler({ loader: "ts" }).transformSync(source)).toString("base64")
  );
  const f = mergeCorrespondenceFixture();
  await expect(old.validateReviewArtifact(f.input, f.reader)).rejects.toThrow(
    "NATIVE_RAW_OUTPUT_REQUIRED",
  );
  expect((await validateReviewArtifact(f.input, f.reader)).headSha).toBe(f.head);
});

test("CURRENTNESS actual bounded publisher fences its pending head immediately before each existing effect", async () => {
  const { readFile } = await import("node:fs/promises"),
    { decodeBoundedReviewFile } = await import("./receipt-validator"),
    { validateRawReviewArtifact } = await import("./review-artifact");
  const source = await readFile(
      new URL("../reuse/publish-review-artifact.ts", import.meta.url),
      "utf8",
    ),
    body = new Bun.Transpiler({ loader: "ts" }).transformSync(
      source
        .slice(
          source.indexOf("export async function publishNativeReviewArtifact("),
          source.indexOf("\nif (import.meta.main)"),
        )
        .replace("export async function", "async function"),
    );
  const f = mergeCorrespondenceFixture(),
    effects: string[] = [],
    rawBytes = Buffer.from(JSON.stringify(f.raw)),
    context = { head: f.head, policy: f.input.policy, revalidate: async () => {} },
    attempt = { id: "00000000-0000-4000-8000-000000000001", pending: f.pending, context };
  f.setStatuses([f.pending]);
  const reader = {
    archive: f.reader.archive,
    json: async (path: string): Promise<unknown> => {
      if (path === "actions/workflows/native-review-evidence.yml")
        return { path: ".github/workflows/native-review-evidence.yml", state: "active" };
      if (path.startsWith("actions/workflows/native-review-evidence.yml/runs?"))
        return { workflow_runs: [f.run] };
      if (path === "actions/runs/11/artifacts?per_page=100") return { artifacts: [f.artifact] };
      if (path.startsWith("contents/.github/source-author.json?")) {
        const bytes = Buffer.from(
          JSON.stringify({
            author: { runtime: f.raw.author.runtime, threadId: f.raw.author.taskId },
          }),
        );
        return {
          type: "file",
          path: ".github/source-author.json",
          encoding: "base64",
          size: bytes.length,
          content: bytes.toString("base64"),
        };
      }
      return f.reader.json(path);
    },
  };
  const fakeFetch = async (url: string, options: RequestInit) => {
    if (url.endsWith("/dispatches")) {
      effects.push("dispatch");
      return new Response(null, { status: 204 });
    }
    effects.push("status");
    const success = { ...f.status, ...JSON.parse(String(options.body)), id: 35 };
    f.setStatuses([success, f.pending]);
    return new Response(JSON.stringify(success), { status: 201 });
  };
  const publish = new Function(
    "Bun",
    "process",
    "console",
    "fetch",
    "requireTrustedReviewContext",
    "requireNativeReviewAttempt",
    "readCompletedNativeObservation",
    "requireCurrentReviewAttempt",
    "authenticatedReviewReader",
    "deriveReviewedSourceManifest",
    "validateRawReviewArtifact",
    "decodeBoundedReviewFile",
    "reviewArtifactWorkflow",
    "reviewArtifactBranch",
    "validatePublishedReviewArtifact",
    "validateMergeCorrespondenceAssignment",
    body + "\nreturn publishNativeReviewArtifact;",
  )(
    {
      spawnSync: (args: string[]) => ({
        exitCode: 0,
        stdout: Buffer.from(
          args.includes("user") ? JSON.stringify(f.status.creator) : "synthetic-token",
        ),
      }),
    },
    { env: {} },
    { log: () => {} },
    fakeFetch,
    () => {},
    () => {},
    () => ({ bytes: rawBytes }),
    async () => [f.pending],
    () => reader,
    deriveReviewedSourceManifest,
    validateRawReviewArtifact,
    decodeBoundedReviewFile,
    ".github/workflows/native-review-evidence.yml",
    "main",
    async () => ({ headSha: f.head }),
    async (
      _policy: unknown,
      _head: unknown,
      _assignment: unknown,
      _reader: unknown,
      pending: unknown,
    ) => {
      if (pending !== attempt.pending) throw Error("MISSING_PENDING_HEAD_ANCHOR");
      effects.push("fence");
      return f.assignment;
    },
  ) as (...args: unknown[]) => Promise<void>;
  await publish(context, { kind: "completed-native-observation" }, attempt);
  expect(effects).toEqual(["fence", "dispatch", "fence", "status"]);
});

for (const mutation of ["missing-assignment", "full-schema", "wrong-owner-schema"] as const)
  test("MERGE-CORRESPONDENCE bounded raw schema and assignment reject " + mutation, async () => {
    const f = mergeCorrespondenceFixture();
    if (mutation === "missing-assignment")
      delete (f.raw as Record<string, unknown>).reviewAssignment;
    if (mutation === "full-schema") f.raw.schema = "native-independent-review-output.v1";
    if (mutation === "wrong-owner-schema")
      f.raw.schema = "owner-scoped-native-merge-correspondence-output.v1";
    f.seal();
    await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow();
  });

test("externally dispatched raw native review passes and source CI consumes it without repository receipt commits", async () => {
  const f = fixture();
  expect((await validateReviewArtifact(f.input, f.reader)).rawSha256).toHaveLength(64);
  const result = await reviewGate({
    env: {
      GITHUB_REPOSITORY: f.input.policy.repository,
      REVIEW_HEAD_SHA: f.head,
      GITHUB_TOKEN: "fictional-token",
      NATIVE_REVIEW_STATUS_CREATOR_ID: "42",
      NATIVE_REVIEW_STATUS_CREATOR_LOGIN: "native-publisher",
    },
    readApi: f.reader.json,
    readArchive: f.reader.archive,
  });
  expect(result?.schema).toBe("native-independent-review-artifact-ci.v1");
});

test("a late old success cannot cover a newer review hold, and artifacts require their own prior pending attempt", async () => {
  const result = Bun.spawnSync(
    ["git", "show", "a0678ade528b005496ae9a6ea5f67aa65ffb81ce:ci/quality/review-artifact.ts"],
    { cwd: import.meta.dir, stdout: "pipe", stderr: "pipe" },
  );
  expect(result.exitCode).toBe(0);
  const source = result.stdout
    .toString()
    .replaceAll(
      '"./artifact-archive"',
      JSON.stringify(new URL("./artifact-archive.ts", import.meta.url).href),
    )
    .replaceAll(
      '"./receipt-validator"',
      JSON.stringify(new URL("./receipt-validator.ts", import.meta.url).href),
    );
  const old = await import(
    "data:text/javascript;base64," +
      Buffer.from(new Bun.Transpiler({ loader: "ts" }).transformSync(source)).toString("base64")
  );
  for (const mutate of [
    (f: ReturnType<typeof fixture>) => f.supersede(),
    (f: ReturnType<typeof fixture>) => f.noAttempt(),
  ]) {
    const f = fixture();
    mutate(f);
    await expect(old.validateReviewArtifact(f.input, f.reader)).resolves.toHaveProperty(
      "rawSha256",
    );
    await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow(
      "REVIEW_ATTEMPT_SUPERSEDED",
    );
  }
});

test("completed negative observations retain exact source and independent identities without becoming passing evidence", () => {
  const f = fixture();
  const fail = {
    ...f.raw,
    conclusion: "fail",
    unresolvedFindings: 1,
    findings: [{ severity: "P1", path: "source.ts", line: 1, explanation: "synthetic defect" }],
  };
  const bytes = Buffer.from(JSON.stringify(fail));
  expect(
    validateCompletedReviewObservation(
      bytes,
      f.input.policy,
      f.head,
      f.tree,
      f.raw.reviewedSourceManifestSha256,
    ).conclusion,
  ).toBe("fail");
  for (const changed of [
    { ...fail, headSha: "f".repeat(40) },
    { ...fail, reviewer: fail.author },
    { ...fail, unresolvedFindings: 0 },
    { ...fail, conclusion: "pass" },
  ]) {
    expect(() =>
      validateCompletedReviewObservation(
        Buffer.from(JSON.stringify(changed)),
        f.input.policy,
        f.head,
        f.tree,
        f.raw.reviewedSourceManifestSha256,
      ),
    ).toThrow();
  }
});

test("artifact review rejects source, publisher, run and raw native identity substitutions", async () => {
  const mutations: ((f: ReturnType<typeof fixture>) => void)[] = [
    (f) => {
      f.status.state = "pending";
    },
    (f) => {
      f.status.creator.id = 99;
    },
    (f) => {
      f.status.creator.type = "Bot";
    },
    (f) => {
      f.run.actor.id = 99;
    },
    (f) => {
      f.run.triggering_actor.id = 99;
    },
    (f) => {
      f.run.event = "pull_request";
    },
    (f) => {
      f.run.run_attempt = 2;
    },
    (f) => {
      f.run.conclusion = "failure";
    },
    (f) => {
      f.run.repository.full_name = "foreign/repo";
    },
    (f) => {
      f.run.path = "other.yml";
    },
    (f) => {
      f.run.head_branch = "authored-branch";
    },
    (f) => {
      f.artifact.expired = true;
    },
    (f) => {
      f.artifact.id++;
    },
    (f) => {
      f.artifact.name = "foreign";
    },
    (f) => {
      f.artifact.digest = "sha256:" + "0".repeat(64);
    },
    (f) => {
      f.artifact.workflow_run.repository_id++;
    },
    (f) => {
      f.artifact.workflow_run.id++;
    },
    (f) => {
      f.raw.headSha = "f".repeat(40);
      f.seal();
    },
    (f) => {
      f.raw.reviewedTreeSha = "f".repeat(40);
      f.seal();
    },
    (f) => {
      f.raw.reviewedSourceManifestSha256 = "f".repeat(64);
      f.seal();
    },
    (f) => {
      f.raw.reviewer = f.raw.author;
      f.seal();
    },
    (f) => {
      f.raw.unresolvedFindings = 1;
      f.seal();
    },
    (f) => {
      f.raw.reviewStatus = "incomplete";
      f.seal();
    },
    (f) => {
      f.raw.binding.scope = "foreign";
      f.seal();
    },
    (f) => f.seal("../native-review.json"),
    (f) => f.corrupt(),
    (f) => f.hold(),
  ];
  for (const mutate of mutations) {
    const f = fixture();
    mutate(f);
    await expect(validateReviewArtifact(f.input, f.reader)).rejects.toThrow();
  }
});

test("only the fixed API receives the bearer and signed storage redirects receive no credentials", async () => {
  const original = globalThis.fetch,
    calls: { url: string; init?: RequestInit }[] = [];
  try {
    globalThis.fetch = (async (url: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(url), init });
      return calls.length === 1
        ? new Response(null, {
            status: 302,
            headers: { location: "https://fixture.blob.core.windows.net/archive?sig=synthetic" },
          })
        : new Response("archive");
    }) as unknown as typeof fetch;
    await authenticatedReviewReader("fixture/owner", "fictional-test-token").archive(22);
    expect(calls[0]!.init!.headers).toHaveProperty("Authorization", "Bearer fictional-test-token");
    expect(calls[1]!.init!.headers).toBeUndefined();
    expect(calls[1]!.init!.credentials).toBe("omit");
    expect(calls[1]!.init!.redirect).toBe("error");
  } finally {
    globalThis.fetch = original;
  }
});

test("untrusted storage origins and redirect chains refuse before any credential-free storage fetch", async () => {
  const original = globalThis.fetch;
  try {
    for (const location of [
      "http://fixture.blob.core.windows.net/a",
      "https://evil.invalid/a",
      "https://user:pass@fixture.blob.core.windows.net/a",
    ]) {
      let calls = 0;
      globalThis.fetch = (async () => {
        calls++;
        return new Response(null, { status: 302, headers: { location } });
      }) as unknown as typeof fetch;
      await expect(
        authenticatedReviewReader("fixture/owner", "fictional-test-token").archive(22),
      ).rejects.toThrow("STORAGE_ORIGIN");
      expect(calls).toBe(1);
    }
  } finally {
    globalThis.fetch = original;
  }
});

test("native publication refuses policy-valid substituted raw bytes before an authenticated status can be posted", async () => {
  const f = fixture(),
    original = Buffer.from(JSON.stringify(f.raw));
  expect(
    (await validatePublishedReviewArtifact({ ...f.input, rawBytes: original }, f.reader)).rawSha256,
  ).toBe(hash(original));
  f.raw.reviewer.taskId = "CI-fabricated-independent-id";
  f.seal();
  // The ordinary CI verifier sees a structurally valid proof; only the native original binds custody.
  await expect(validateReviewArtifact(f.input, f.reader)).resolves.toHaveProperty("rawSha256");
  await expect(
    validatePublishedReviewArtifact({ ...f.input, rawBytes: original }, f.reader),
  ).rejects.toThrow("ORIGINAL_NATIVE_BYTES_REQUIRED");
});

test("owner-scoped CI consumes the same artifact transport under its own fixed policy", async () => {
  const f = fixture(true),
    original = globalThis.fetch,
    names = ["GITHUB_REPOSITORY", "REVIEW_HEAD_SHA", "GITHUB_TOKEN"] as const,
    old = names.map((n) => process.env[n]);
  process.env.GITHUB_REPOSITORY = f.input.policy.repository;
  process.env.REVIEW_HEAD_SHA = f.head;
  process.env.GITHUB_TOKEN = "fictional-token";
  try {
    globalThis.fetch = (async (url: string | URL | Request) => {
      const target = new URL(String(url));
      if (target.hostname === "fixture.blob.core.windows.net")
        return new Response(Buffer.from(await f.reader.archive(22)));
      const path =
        target.pathname.slice(("/repos/" + f.input.policy.repository + "/").length) + target.search;
      if (path === "actions/artifacts/22/zip")
        return new Response(null, {
          status: 302,
          headers: { location: "https://fixture.blob.core.windows.net/archive" },
        });
      return new Response(JSON.stringify(await f.reader.json(path)), { status: 200 });
    }) as unknown as typeof fetch;
    await ownerScopedReviewGate({
      repository: f.input.policy.repository,
      projectId: "fixture-owner",
      domainId: "fixture-domain",
      semanticOwner: "fixture-owner",
      scope: "source-check",
      statusContext: "native-independent-review",
      publisher: f.input.policy.publisher,
    });
  } finally {
    globalThis.fetch = original;
    names.forEach((name, i) => {
      if (old[i] === undefined) delete process.env[name];
      else process.env[name] = old[i];
    });
  }
});

test("new pending inserted after the last publisher read but before its success POST defeats the actual old publisher and new fence", async () => {
  const { readFile } = await import("node:fs/promises"),
    { join } = await import("node:path");
  const prior = Bun.spawnSync(
    ["git", "show", "a0678ade528b005496ae9a6ea5f67aa65ffb81ce:ci/quality/review-artifact.ts"],
    { cwd: import.meta.dir, stdout: "pipe", stderr: "pipe" },
  );
  expect(prior.exitCode).toBe(0);
  const oldArtifactSource = prior.stdout
    .toString()
    .replaceAll(
      '"./artifact-archive"',
      JSON.stringify(new URL("./artifact-archive.ts", import.meta.url).href),
    )
    .replaceAll(
      '"./receipt-validator"',
      JSON.stringify(new URL("./receipt-validator.ts", import.meta.url).href),
    );
  const oldArtifact = await import(
    "data:text/javascript;base64," +
      Buffer.from(new Bun.Transpiler({ loader: "ts" }).transformSync(oldArtifactSource)).toString(
        "base64",
      )
  );
  for (const current of [false, true]) {
    const f = fixture(),
      bytes = Buffer.from(JSON.stringify(f.raw)),
      id = "00000000-0000-4000-8000-000000000001";
    f.setStatuses([f.pending]);
    const context = {
        head: f.head,
        policy: f.input.policy,
        revalidate: async () => {},
        readApi: f.reader.json,
      },
      attempt = { id, pending: f.pending, context };
    let posts = 0;
    const reader = {
      archive: f.reader.archive,
      json: async (path: string): Promise<unknown> => {
        if (path === "actions/workflows/native-review-evidence.yml")
          return { path: ".github/workflows/native-review-evidence.yml", state: "active" };
        if (path.startsWith("actions/workflows/native-review-evidence.yml/runs?"))
          return { workflow_runs: [f.run] };
        if (path === "actions/runs/11/artifacts?per_page=100") return { artifacts: [f.artifact] };
        if (path.startsWith("contents/.github/source-author.json?")) {
          const body = Buffer.from(
            JSON.stringify({
              author: { runtime: f.raw.author.runtime, threadId: f.raw.author.taskId },
            }),
          );
          return {
            type: "file",
            path: ".github/source-author.json",
            encoding: "base64",
            size: body.length,
            content: body.toString("base64"),
          };
        }
        return f.reader.json(path);
      },
    };
    const fakeFetch = async (url: string, options: RequestInit) => {
      if (url.endsWith("/dispatches")) return new Response(null, { status: 204 });
      expect(url).toBe(
        `https://api.github.com/repos/${f.input.policy.repository}/statuses/${f.head}`,
      );
      posts++;
      const success = { ...f.status, ...JSON.parse(String(options.body)), id: 35 };
      // Deterministic read/POST race: all old checks saw pending32; pending34 starts now.
      f.setStatuses([success, { ...f.pending, id: 34 }, f.pending]);
      return new Response(JSON.stringify(success), { status: 201 });
    };
    const fakeBun = {
      spawnSync: (args: string[]) => ({
        exitCode: 0,
        stdout: Buffer.from(
          args.includes("user") ? JSON.stringify(f.status.creator) : "synthetic-token",
        ),
      }),
    };
    const oldPub = Bun.spawnSync(
      [
        "git",
        "show",
        "a0678ade528b005496ae9a6ea5f67aa65ffb81ce:ci/reuse/publish-review-artifact.ts",
      ],
      { cwd: import.meta.dir, stdout: "pipe", stderr: "pipe" },
    );
    expect(oldPub.exitCode).toBe(0);
    const source = current
      ? await readFile(join(import.meta.dir, "../reuse/publish-review-artifact.ts"), "utf8")
      : oldPub.stdout.toString();
    const body = new Bun.Transpiler({ loader: "ts" }).transformSync(
      source
        .slice(
          source.indexOf("export async function publishNativeReviewArtifact("),
          source.indexOf("\nif (import.meta.main)"),
        )
        .replace("export async function", "async function"),
    );
    const requireCurrent = async () => {
      const states = (await f.reader.json(`commits/${f.head}/statuses?per_page=100`)) as {
        id: number;
        state: string;
      }[];
      if (states[0]?.id !== 32 || states[0]?.state !== "pending") throw Error("SUPERSEDED");
      return states;
    };
    const names = [
      "Bun",
      "process",
      "fetch",
      "requireTrustedReviewContext",
      "requireNativeReviewAttempt",
      "readCompletedNativeObservation",
      "requireCurrentReviewAttempt",
      "authenticatedReviewReader",
      "deriveReviewedSourceManifest",
      "decodeBoundedReviewFile",
      "validateRawReviewArtifact",
      "validatePublishedReviewArtifact",
      "reviewArtifactWorkflow",
      "reviewArtifactBranch",
      "readFile",
      "resolve",
      "randomUUID",
      "console",
    ];
    const { decodeBoundedReviewFile } = await import("./receipt-validator"),
      { validateRawReviewArtifact } = await import("./review-artifact");
    const publish = new Function(...names, body + "\nreturn publishNativeReviewArtifact;")(
      fakeBun,
      { env: {} },
      fakeFetch,
      () => {},
      () => {},
      () => ({ bytes: Buffer.from(bytes) }),
      requireCurrent,
      () => reader,
      deriveReviewedSourceManifest,
      decodeBoundedReviewFile,
      validateRawReviewArtifact,
      current ? validatePublishedReviewArtifact : oldArtifact.validatePublishedReviewArtifact,
      ".github/workflows/native-review-evidence.yml",
      "main",
      async () => bytes,
      (p: string) => p,
      () => id,
      { log: () => {} },
    ) as (...args: unknown[]) => Promise<void>;
    const result = current
      ? publish(context, { kind: "completed-native-observation" }, attempt)
      : publish(context, "/synthetic-native.raw.json");
    if (current) await expect(result).rejects.toThrow("REVIEW_ATTEMPT_SUPERSEDED");
    else await expect(result).resolves.toBeUndefined();
    expect(posts).toBe(1);
  }
});
