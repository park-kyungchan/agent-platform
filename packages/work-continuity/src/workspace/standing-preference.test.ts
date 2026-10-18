import { afterEach, expect, spyOn, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createHash } from "node:crypto";
import { gitRecordPath, openGitWork } from "../git-work/index.ts";
import type { GitWorkRecord, InputRecord, JsonValue, WorkRecord } from "../git-work/types.ts";
import { prepareGitWorkMemoryArtifacts, readGitWorkMemoryRequest } from "./recall-request.ts";
import { episodeFingerprint, sourceKey, type MemoryPolicy } from "./qualification.ts";
import * as lexical from "./lexical-recall.ts";

// All identities, user quotes and review declarations in this fixture are fictional.
const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
const digest = (value: string | Uint8Array) => createHash("sha256").update(value).digest("hex");
async function git(root: string, ...args: string[]) {
  const child = Bun.spawn(["git", "-C", root, ...args], {
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "Fiction",
      GIT_AUTHOR_EMAIL: "fiction@example.invalid",
      GIT_COMMITTER_NAME: "Fiction",
      GIT_COMMITTER_EMAIL: "fiction@example.invalid",
    },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, error, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code) throw Error(error);
  return out.trim();
}
async function fixture(
  options: {
    review?: boolean;
    admission?: "absent" | "proposed" | "withdrawn" | "duplicate";
    exclude?: boolean;
  } = {},
) {
  const root = await mkdtemp(join(tmpdir(), "standing-preference-fiction-"));
  roots.push(root);
  const scope = {
    domainId: "fictional-domain",
    projectId: "fictional-project",
    workId: "preference-owner",
  };
  const base = { schema: "common.git-work.v1" as const, ...scope };
  const path = (kind: GitWorkRecord["kind"], id: string, workId = scope.workId) =>
    gitRecordPath(scope.projectId, workId, { kind, id });
  const input = (
    id: string,
    content: unknown,
    actual = false,
    workId = scope.workId,
  ): InputRecord => ({
    ...base,
    workId,
    kind: "input",
    id,
    inputKind: actual ? "actual-user-answer" : "source-owner-record",
    origin: {
      ownerId: "fictional-source-owner",
      locator: "fictional:" + id,
      kind: "source-owner-capture",
    },
    content: JSON.parse(JSON.stringify(content)) as JsonValue,
  });
  const put = async (row: GitWorkRecord) => {
    const file = join(root, path(row.kind, row.id, row.workId));
    await mkdir(dirname(file), { recursive: true });
    await writeFile(file, JSON.stringify(row, null, 2) + "\n");
  };
  const answer = "Future agents use typed JSON/TS engineering; native Markdown retains its owner.";
  const why = "Memory needs context, hierarchy and typing to reduce drift.";
  const work = (workId: string): WorkRecord => ({
    ...base,
    workId,
    kind: "work",
    id: workId,
    repositoryId: "fictional/source",
    issueId: "issue",
    purpose: {
      objective: "Fictional source qualification",
      successCriteria: ["Keep private data private"],
      inScope: ["Source-only tests"],
      outOfScope: ["Native permission"],
    },
    spec: { id: "spec", revision: 1 },
    plan: { id: "plan", revision: 1 },
    governingDecisionIds: workId === scope.workId ? ["typed-direction"] : [],
    historicalFenceIds: [],
  });
  const sourceInput = input(
    "actual-answer",
    {
      text: answer,
      question: "How should future agents engineer?",
      nodes: [
        { id: "typed", kind: "preference", meaning: answer, sources: ["current"] },
        {
          id: "drift",
          kind: "direct-why",
          meaning: "The user states the drift rationale.",
          sources: ["why"],
        },
        {
          id: "private",
          kind: "preference",
          meaning: "PRIVATE-MEDICAL-DETAIL",
          sources: ["private"],
        },
      ],
      relations: [
        { from: "typed", type: "motivatedBy", to: "drift" },
        { from: "private", type: "unrelated", to: "typed" },
      ],
    },
    true,
  );
  await put(work(scope.workId));
  await put(work("consumer-a"));
  await put(work("consumer-b"));
  await put(sourceInput);
  await put(input("direct-why", { text: why }, true, "historical-preferences"));
  await put(input("private-data", { text: "PRIVATE-MEDICAL-DETAIL" }, true, "private-work"));
  const answerRef = {
    kind: "snapshot" as const,
    path: path("input", "actual-answer"),
    fragment: "#/content/text",
  };
  await put({
    ...base,
    kind: "decision",
    id: "typed-direction",
    status: "accepted",
    answer,
    meaning:
      "Standing engineering preference and directly stated rationale, independent of consumer Work.",
    answerSource: answerRef,
    selectionContext: {
      text: "How should future agents engineer?",
      source: { ...answerRef, fragment: "#/content/question" },
    },
    supersedes: [],
  });
  await git(root, "init", "-q", "-b", "owner");
  await git(root, "add", ".");
  await git(root, "commit", "-qm", "fictional originals");
  const commit = await git(root, "rev-parse", "HEAD");
  const selection = {
    repositoryRoot: root,
    repositoryId: "fictional/source",
    commit,
    projectId: scope.projectId,
    workId: scope.workId,
  };
  const reader = await openGitWork(selection);
  const pin = async (id: string, fragment: string, workId = scope.workId) => ({
    kind: "snapshot" as const,
    path: path("input", id, workId),
    fragment,
    sha256: digest(await readFile(join(root, path("input", id, workId)))),
  });
  const whySource = await pin("direct-why", "#/content/text", "historical-preferences");
  const descriptor = {
    topicId: "fictional:topic:standing-engineering",
    generationId: "generation.1",
    receiptTaskIds: [],
    claims: [
      { key: "standing.typed", record: { kind: "decision" as const, id: "typed-direction" } },
    ],
    inputIds: { request: "request", dataset: "dataset", policy: "policy" },
    query: "drift",
    claimKey: "standing.typed",
    maxChars: 8000,
    preference: {
      schema: "common.git-work-standing-preference.v1",
      applicability: "fictional-user:standing-engineering",
      revision: "preference.1",
      inputId: "actual-answer",
      nodeIds: ["typed", "drift"],
      nodes: await pin("actual-answer", "#/content/nodes"),
      relations: await pin("actual-answer", "#/content/relations"),
      sources: [
        { id: "current", source: await pin("actual-answer", "#/content/text") },
        {
          id: "why",
          source: {
            kind: "git" as const,
            repositoryId: selection.repositoryId,
            commit,
            path: whySource.path,
            fragment: whySource.fragment,
            sha256: whySource.sha256,
          },
        },
      ],
    },
    policy: {
      schema: "codex.local-memory-policy.v1",
      projectId: scope.projectId,
      workId: scope.workId,
      activeGenerationId: "generation.1",
      policy: {
        revision: "policy.1",
        excludedSourceHashes: [] as string[],
        excludedFragmentIds: [] as string[],
        aliases: [],
        commonAdmissions: [] as MemoryPolicy["commonAdmissions"],
      },
    },
  };
  const initial = await prepareGitWorkMemoryArtifacts(reader, descriptor);
  if (options.exclude) descriptor.policy.policy.excludedSourceHashes.push(whySource.sha256);
  const memory = initial.dataset.memories[0]!;
  if (options.admission !== "absent") {
    const admission = {
      memoryId: memory.id,
      episodeId: memory.episodeId,
      contentHash: memory.contentHash,
      claim: memory.claim,
      applicability: descriptor.preference.applicability,
      status:
        options.admission === "proposed" || options.admission === "withdrawn"
          ? options.admission
          : ("accepted" as const),
    };
    descriptor.policy.policy.commonAdmissions.push(admission);
    if (options.admission === "duplicate")
      descriptor.policy.policy.commonAdmissions.push({ ...admission });
  }
  const proposal = await prepareGitWorkMemoryArtifacts(reader, descriptor);
  let compiled = proposal;
  if (options.review !== false) {
    await put(
      input("review", {
        schema: "common.git-work-memory-owner-review.v1",
        ownerId: "fictional-source-owner",
        bindingSha256: proposal.dataset.reviewBindingSha256,
        status: "reviewed",
        receipts: proposal.dataset.memories.map((memory) => ({
          memoryId: memory.id,
          episodeId: memory.episodeId,
          generationId: memory.generationId,
          policyRevision: memory.policyRevision,
          contentHash: memory.contentHash,
          sourceKeys: memory.sourceKeys,
          claim: memory.claim,
          scope: memory.scope,
          provenance: "owner-verified",
          episodeFingerprint: episodeFingerprint(proposal.dataset.episodes[0]!),
        })),
        authoritativeReceipts: proposal.dataset.authoritativeClaims.map((claim) => ({
          sourceKey: sourceKey(claim.source),
          key: claim.key,
          value: claim.value,
          recordClass: claim.recordClass,
          status: claim.status,
          provenance: "owner-verified",
        })),
      }),
    );
    await git(root, "add", ".");
    await git(root, "commit", "-qm", "fictional owner review");
    compiled = await prepareGitWorkMemoryArtifacts(reader, descriptor, {
      kind: "git",
      repositoryId: selection.repositoryId,
      commit: await git(root, "rev-parse", "HEAD"),
      path: path("input", "review"),
      fragment: "#/content",
    });
  }
  const persist = async (data = compiled.dataset, policy = compiled.policy) => {
    await put(input("dataset", data));
    await put(input("policy", policy));
    await put(
      input("request", {
        ...compiled.request,
        dataset: await pin("dataset", "#/content"),
        policy: await pin("policy", "#/content"),
      }),
    );
    await git(root, "add", ".");
    await git(root, "commit", "-qm", "fictional current triple");
    const head = await git(root, "rev-parse", "HEAD");
    const current = await openGitWork({ ...selection, commit: head });
    const request = await pin("request", "#/content");
    const ownerSelection = {
      owningRef: "refs/heads/owner",
      admittedCommit: head,
      policyRevision: descriptor.policy.policy.revision,
      activeGenerationId: descriptor.generationId,
      state: "current" as "current" | "unknown" | "publishing",
      commonPreferences: {
        topicId: descriptor.topicId,
        claimKey: descriptor.claimKey,
        applicability: descriptor.preference.applicability,
        descriptorSha256: digest(JSON.stringify(compiled.dataset.descriptor)),
        consumer: { repositoryId: "fictional/consumer-a", workId: "consumer-a" } as {
          repositoryId: string;
          workId?: string;
        },
      },
    };
    return {
      current,
      request,
      ownerSelection,
      read: () => readGitWorkMemoryRequest(current, request, ownerSelection),
    };
  };
  return {
    root,
    reader,
    descriptor,
    proposal,
    compiled,
    persist,
    put,
    work,
    path,
    pin,
    selection,
    answer,
    why,
    sourceInput,
    git: (...args: string[]) => git(root, ...args),
  };
}

test("accepted standing preference recalls typed rationale across consumer Works without reading private Work or mutating selection", async () => {
  const f = await fixture();
  const q = await f.persist();
  const before = await f.git("rev-parse", "HEAD^{tree}");
  const first = await q.read();
  q.ownerSelection.commonPreferences.consumer = {
    repositoryId: "fictional/consumer-b",
    workId: "consumer-b",
  };
  const second = await q.read();
  expect(first.selectedValue).toBe(f.answer);
  expect(second.selectedValue).toBe(f.answer);
  expect(first.hits).toHaveLength(1);
  expect(first.hits[0]!.text).toContain(f.why);
  const text = first.hits[0]!.text;
  expect(text).toContain('"type":"motivatedBy"');
  expect(text).toContain('"kind":"direct-why"');
  expect(text).not.toContain("PRIVATE-MEDICAL-DETAIL");
  expect(first.gitWork.topicId).toBe(second.gitWork.topicId);
  expect(first.gitWork.commonPreferences?.consumer.workId).toBe("consumer-a");
  expect(second.gitWork.commonPreferences?.consumer.workId).toBe("consumer-b");
  q.ownerSelection.commonPreferences.consumer = { repositoryId: "fictional/home" };
  expect((await q.read()).hits[0]!.text).toBe(text);
  expect(f.compiled.dataset.sourceBindings).toHaveLength(3);
  expect(
    f.compiled.dataset.sourceBindings.every((row) =>
      /actual-answer|typed-direction|direct-why/.test(row.source.relativePath),
    ),
  ).toBe(true);
  expect(f.compiled.dataset.memories[0]!.scope).toEqual({ kind: "common" });
  expect(await f.git("rev-parse", "HEAD^{tree}")).toBe(before);
  expect(first.outputChars).toBeLessThanOrEqual(8000);
  expect(first.nativePermissionGranted).toBe(false);
});

test.each(["absent", "proposed", "withdrawn", "duplicate"] as const)(
  "%s common admission withholds only this preference",
  async (admission) => {
    const f = await fixture({ admission });
    const result = await (await f.persist()).read();
    expect(result.hits).toEqual([]);
    expect(result.selectedValue).toBeNull();
    expect(result.counts.withheld).toBe(1);
  },
);

test("a proposed derivation does not become owner-verified from its accepted common admission", async () => {
  const f = await fixture({ review: false });
  const result = await (await f.persist()).read();
  expect(f.proposal.dataset.receipts).toEqual([]);
  expect(result.hits).toEqual([]);
  expect(result.selectedValue).toBeNull();
});

test("an excluded original withholds even an accepted reviewed preference", async () => {
  const f = await fixture({ exclude: true });
  const result = await (await f.persist()).read();
  expect(result.hits).toEqual([]);
  expect(result.selectedValue).toBeNull();
});

test("a real owning-ref change during lexical recall withholds the result", async () => {
  const f = await fixture();
  const q = await f.persist();
  const original = lexical.lexicalRecall;
  const mock = spyOn(lexical, "lexicalRecall").mockImplementation(((
    ...args: Parameters<typeof original>
  ) => {
    const result = Reflect.apply(original, undefined, args);
    const mutation = Bun.spawnSync(
      ["git", "-C", f.root, "update-ref", "refs/heads/owner", f.selection.commit],
      { stdout: "pipe", stderr: "pipe" },
    );
    expect(mutation.exitCode).toBe(0);
    return result;
  }) as typeof original);
  try {
    await expect(q.read()).rejects.toThrow("owning head advanced");
  } finally {
    mock.mockRestore();
  }
});

test.each(["unknown", "publishing"] as const)(
  "%s authoritative currentness refuses this read",
  async (state) => {
    const f = await fixture();
    const q = await f.persist();
    q.ownerSelection.state = state;
    await expect(q.read()).rejects.toThrow("current owner selection");
  },
);

test("ordinary Work selection cannot opt in to common preferences with a boolean or an unrelated claim", async () => {
  const f = await fixture();
  const q = await f.persist();
  const { commonPreferences: ignored, ...ordinary } = q.ownerSelection;
  void ignored;
  await expect(readGitWorkMemoryRequest(q.current, q.request, ordinary)).rejects.toThrow(
    "common preference owner selection",
  );
  await expect(
    readGitWorkMemoryRequest(q.current, q.request, {
      ...ordinary,
      commonEnabled: true,
    } as typeof ordinary),
  ).rejects.toThrow("owner selection");
  q.ownerSelection.commonPreferences.claimKey = "private.medical";
  await expect(q.read()).rejects.toThrow("common preference owner binding");
});

test("policy/generation and exact preference owner bindings fail closed", async () => {
  const f = await fixture();
  const q = await f.persist();
  q.ownerSelection.commonPreferences.applicability = "another-user";
  await expect(q.read()).rejects.toThrow("common preference owner binding");
  q.ownerSelection.commonPreferences.applicability = f.descriptor.preference.applicability;
  q.ownerSelection.activeGenerationId = "generation.third-state";
  await expect(q.read()).rejects.toThrow("policy/generation advanced");
});

test("third-state original bytes, changed authority membership and stale owning heads cannot reuse an old receipt", async () => {
  const f = await fixture();
  const q = await f.persist();
  await f.put({ ...f.sourceInput, content: { text: "third state" } });
  await f.git("add", ".");
  await f.git("commit", "-qm", "fictional third state");
  await expect(q.read()).rejects.toThrow("owning head advanced");
  const head = await f.git("rev-parse", "HEAD");
  const now = await openGitWork({ ...f.selection, commit: head });
  await expect(
    readGitWorkMemoryRequest(now, q.request, { ...q.ownerSelection, admittedCommit: head }),
  ).rejects.toThrow("original is no longer current");
  const g = await fixture();
  const current = await g.persist();
  await g.put({ ...g.work(g.selection.workId), governingDecisionIds: [] });
  await g.git("add", ".");
  await g.git("commit", "-qm", "fictional authority removal");
  const nextHead = await g.git("rev-parse", "HEAD");
  const next = await openGitWork({ ...g.selection, commit: nextHead });
  await expect(
    readGitWorkMemoryRequest(next, current.request, {
      ...current.ownerSelection,
      admittedCommit: nextHead,
    }),
  ).rejects.toThrow("authority is no longer current");
});

test("fresh current observation permits unrelated Work evolution while keeping preference originals exact", async () => {
  const f = await fixture();
  const q = await f.persist();
  await f.put({
    ...f.work(f.selection.workId),
    purpose: {
      ...f.work(f.selection.workId).purpose,
      objective: "Legitimate unrelated Work evolution",
    },
  });
  await f.git("add", ".");
  await f.git("commit", "-qm", "fictional Work successor");
  await expect(q.read()).rejects.toThrow("owning head advanced");
  const head = await f.git("rev-parse", "HEAD");
  const current = await openGitWork({ ...f.selection, commit: head });
  const result = await readGitWorkMemoryRequest(current, q.request, {
    ...q.ownerSelection,
    admittedCommit: head,
  });
  expect(result.selectedValue).toBe(f.answer);
  expect(result.hits[0]!.text).toContain(f.why);
});

test("unrelated private source, detached rationale and Work Task descriptors are not preference scope", async () => {
  const f = await fixture();
  const privateSource = await f.pin("private-data", "#/content/text", "private-work");
  await expect(
    prepareGitWorkMemoryArtifacts(f.reader, {
      ...f.descriptor,
      preference: {
        ...f.descriptor.preference,
        sources: [...f.descriptor.preference.sources, { id: "private", source: privateSource }],
      },
    }),
  ).rejects.toThrow("full selected lineage");
  await expect(
    prepareGitWorkMemoryArtifacts(f.reader, { ...f.descriptor, taskId: "some-task" }),
  ).rejects.toThrow("preference-only descriptor");
  await expect(
    prepareGitWorkMemoryArtifacts(f.reader, {
      ...f.descriptor,
      preference: { ...f.descriptor.preference, nodeIds: ["typed"] },
    }),
  ).rejects.toThrow("full selected lineage");
});

test.each(["outside-work-alias", "foreign-work-path"] as const)(
  "preference originals reject an exactly pinned %s despite a fresh descriptor",
  async (variant) => {
    const f = await fixture();
    const original = f.descriptor.preference.sources[1]!.source;
    const bytes = await readFile(join(f.root, original.path));
    const alias =
      variant === "outside-work-alias"
        ? "outside-canonical-work/aliased-input.json"
        : f.path("input", "direct-why", "unrelated-work");
    await mkdir(dirname(join(f.root, alias)), { recursive: true });
    await writeFile(join(f.root, alias), bytes);
    await f.git("add", ".");
    await f.git("commit", "-qm", "fictional exactly pinned noncanonical source alias");
    const commit = await f.git("rev-parse", "HEAD");
    const current = await openGitWork({ ...f.selection, commit });
    const descriptor = structuredClone(f.descriptor);
    descriptor.preference.sources[1]!.source = {
      ...original,
      kind: "git",
      repositoryId: f.selection.repositoryId,
      commit,
      path: alias,
      sha256: digest(bytes),
    };
    // The general Git source API still reads an explicit alias; only preference admission rejects it.
    expect((await current.readSource(descriptor.preference.sources[1]!.source)).sha256).toBe(
      digest(bytes),
    );
    await expect(prepareGitWorkMemoryArtifacts(current, descriptor)).rejects.toThrow(
      "preference canonical record path",
    );
  },
);

test("preference projection Input identity must match its canonical Work path", async () => {
  const f = await fixture();
  const inputPath = f.path("input", f.sourceInput.id);
  await writeFile(
    join(f.root, inputPath),
    JSON.stringify({ ...f.sourceInput, workId: "foreign-work" }, null, 2) + "\n",
  );
  await f.git("add", ".");
  await f.git("commit", "-qm", "fictional declared Input Work mismatch");
  const current = await openGitWork({ ...f.selection, commit: await f.git("rev-parse", "HEAD") });
  const descriptor = structuredClone(f.descriptor);
  const sha256 = digest(await readFile(join(f.root, inputPath)));
  descriptor.preference.nodes.sha256 = sha256;
  descriptor.preference.relations.sha256 = sha256;
  descriptor.preference.sources[0]!.source.sha256 = sha256;
  await expect(prepareGitWorkMemoryArtifacts(current, descriptor)).rejects.toThrow(
    "preference canonical record path",
  );
});
