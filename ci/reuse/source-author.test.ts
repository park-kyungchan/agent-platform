import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  captureCodexSourceAuthor,
  captureHermesHookSourceAuthor,
  parseSourceAuthorJson,
  sourceAuthorGit,
  validateSourceAuthorMetadata,
  verifyCommittedSourceAuthor,
} from "./source-author";

// These are fictional source-attribution fixtures, not observations of real native callers.
const selection = {
  sourceOwner: {
    repository: "fixture/owner",
    projectId: "fixture-owner",
    domainId: "fixture-domain",
  },
  logicalWork: {
    domainId: "native-harness",
    projectId: "fixture-integration",
    workId: "fictional-work",
    taskId: "fictional-task",
  },
};
const env = { CODEX_THREAD_ID: "00000000-0000-4000-8000-000000000001" };
const base = "a".repeat(40),
  at = "2026-10-19T00:00:00.000Z";
const metadata = () => captureCodexSourceAuthor(selection, base, env, at);
const encode = (value: unknown) => Buffer.from(JSON.stringify(value, null, 2) + "\n");
const roots: string[] = [];
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});

test("native capture retains distinct source owner, logical Work, observed thread and null Task/Run", () => {
  const value = metadata();
  expect(value.sourceOwner.projectId).not.toBe(value.logicalWork.projectId);
  expect(value.author).toEqual({
    runtime: "native-codex",
    threadId: env.CODEX_THREAD_ID,
    sessionId: null,
    nativeTaskId: null,
    nativeRunId: null,
    observedVia: "CODEX_THREAD_ID",
  });
  expect(value).not.toHaveProperty("sourceHead");
  expect(value.nativeAuthorshipAuthenticated).toBe(false);
  expect(value.publisherIdentityInferred).toBe(false);
  expect(validateSourceAuthorMetadata(parseSourceAuthorJson(encode(value)))).toEqual(value);
});

test("CI, absent or unsafe native identity cannot capture an original source author", () => {
  for (const invalid of [
    {},
    { CODEX_THREAD_ID: "guessed-logical-task" },
    { ...env, CI: "true" },
    { ...env, GITHUB_ACTIONS: "true" },
    { CODEX_THREAD_ID: env.CODEX_THREAD_ID + "\n" },
  ])
    expect(() => captureCodexSourceAuthor(selection, base, invalid, at)).toThrow();
});

test("the new shared action and common workflow require author metadata without an opt-out", async () => {
  const action = Bun.YAML.parse(
    await readFile(join(import.meta.dir, "../../.github/actions/bun-source/action.yml"), "utf8"),
  ) as {
    inputs: Record<string, { required?: boolean; default?: string }>;
    runs: { steps: { env?: Record<string, string> }[] };
  };
  expect(action.inputs["source-author-metadata"]).toMatchObject({
    required: true,
    default: ".github/source-author.json",
  });
  expect(
    action.runs.steps.some(
      (step) =>
        step.env?.COMMON_CI_SOURCE_AUTHOR_METADATA === "${{ inputs.source-author-metadata }}",
    ),
  ).toBe(true);
  expect(
    Object.keys(action.inputs).filter((key) => /skip.*author|require.*author/i.test(key)),
  ).toEqual([]);
  const workflow = Bun.YAML.parse(
    await readFile(join(import.meta.dir, "../../.github/workflows/source.yml"), "utf8"),
  ) as {
    jobs: Record<
      string,
      { steps: { run?: string; uses?: string; with?: Record<string, unknown> }[] }
    >;
  };
  for (const name of ["source", "quality", "merge-integration", "native-review"]) {
    const steps = workflow.jobs[name]!.steps;
    expect(
      steps.find((step) => step.uses?.startsWith("actions/checkout@"))?.with?.["fetch-depth"],
    ).toBe(0);
    const commands = steps.flatMap((step) => (step.run ? [step.run] : []));
    expect(commands.findIndex((command) => command.includes("source-author-cli.ts verify"))).toBe(
      0,
    );
  }
});

test("missing, ambiguous, private or logical-as-native fields refuse", () => {
  const value = metadata();
  const missing = structuredClone(value) as unknown as { author: Record<string, unknown> };
  delete missing.author.nativeRunId;
  for (const invalid of [
    missing,
    { ...value, sourceHead: base },
    { ...value, cwd: "/private/native" },
    { ...value, author: { ...value.author, sessionId: "another-session" } },
    { ...value, author: { ...value.author, nativeTaskId: selection.logicalWork.taskId } },
    { ...value, author: { ...value.author, nativeRunId: "https://foreign.invalid/identity" } },
    { ...value, author: { ...value.author, credential: "unrequested" } },
    { ...value, capturedAtUtc: "2026-02-30T00:00:00.000Z" },
    { ...value, nativeAuthorshipAuthenticated: true },
    { ...value, publisherIdentityInferred: true },
  ])
    expect(() => validateSourceAuthorMetadata(invalid)).toThrow();
  const duplicate = encode(value)
    .toString()
    .replace('"threadId":', '"threadId": "00000000-0000-4000-8000-000000000002",\n    "threadId":');
  expect(() => parseSourceAuthorJson(Buffer.from(duplicate))).toThrow();
  expect(() => parseSourceAuthorJson(Buffer.from('{"schema":"x"}'))).toThrow();
  expect(() => parseSourceAuthorJson(Buffer.alloc(16_385))).toThrow();
});

test("Hermes observation uses documented session/extra fields without Task inference", () => {
  const hook = {
    hook_event_name: "pre_llm_call",
    session_id: "20990101_000000_fixture",
    extra: { task_id: null, run_id: null },
  };
  const value = captureHermesHookSourceAuthor(selection, base, hook, at, {});
  expect(value.author).toEqual({
    runtime: "native-hermes",
    threadId: null,
    sessionId: hook.session_id,
    nativeTaskId: null,
    nativeRunId: null,
    observedVia: "pre_llm_call.session_id",
  });
  for (const invalid of [
    { ...hook, hook_event_name: "guessed" },
    { ...hook, cwd: "/private/native" },
    { ...hook, extra: { task_id: null } },
    { ...hook, extra: { task_id: selection.logicalWork.taskId, run_id: null } },
    { ...hook, session_id: "" },
  ])
    expect(() => captureHermesHookSourceAuthor(selection, base, invalid, at, {})).toThrow();
  expect(() =>
    captureHermesHookSourceAuthor(selection, base, hook, at, { GITHUB_ACTIONS: "true" }),
  ).toThrow();
});

const schemaHook = {
  hook_event_name: "pre_llm_call",
  session_id: "20990101_000000_schema_fixture",
  extra: { task_id: null, run_id: null },
};

for (const { name, logicalWork } of [
  {
    name: "one delimiter-collapsed key",
    logicalWork: { "domainId|projectId|taskId|workId": "fixture-scope" },
  },
  {
    name: "two delimiter-collapsed keys",
    logicalWork: { "domainId|projectId": "fixture-scope", "taskId|workId": "fixture-task" },
  },
  {
    name: "a renamed key with the same field count",
    logicalWork: {
      domainId: selection.logicalWork.domainId,
      projectId: selection.logicalWork.projectId,
      workId: selection.logicalWork.workId,
      "taskId|foreign": selection.logicalWork.taskId,
    },
  },
  {
    name: "a missing named key",
    logicalWork: {
      domainId: selection.logicalWork.domainId,
      projectId: selection.logicalWork.projectId,
      workId: selection.logicalWork.workId,
    },
  },
  {
    name: "an extra named key",
    logicalWork: { ...selection.logicalWork, extraId: "fixture-extra" },
  },
  { name: "null", logicalWork: null },
  { name: "a string", logicalWork: "fixture-scope" },
  { name: "an empty array", logicalWork: [] },
  { name: "an array with all named fields", logicalWork: Object.assign([], selection.logicalWork) },
]) {
  test("logical Work rejects " + name + " across capture and metadata paths", () => {
    const invalidSelection = { ...selection, logicalWork };
    expect(() => captureCodexSourceAuthor(invalidSelection, base, env, at)).toThrow(
      "Exact source-author fields required",
    );
    expect(() => captureHermesHookSourceAuthor(invalidSelection, base, schemaHook, at, {})).toThrow(
      "Exact source-author fields required",
    );
    for (const valid of [
      metadata(),
      captureHermesHookSourceAuthor(selection, base, schemaHook, at, {}),
    ])
      expect(() => validateSourceAuthorMetadata({ ...valid, logicalWork })).toThrow(
        "Exact source-author fields required",
      );
  });
}

test("valid named scope fields retain values regardless of insertion order", () => {
  const reordered = {
    logicalWork: {
      taskId: selection.logicalWork.taskId,
      workId: selection.logicalWork.workId,
      projectId: selection.logicalWork.projectId,
      domainId: selection.logicalWork.domainId,
    },
    sourceOwner: {
      domainId: selection.sourceOwner.domainId,
      projectId: selection.sourceOwner.projectId,
      repository: selection.sourceOwner.repository,
    },
  };
  for (const value of [
    captureCodexSourceAuthor(reordered, base, env, at),
    captureHermesHookSourceAuthor(reordered, base, schemaHook, at, {}),
  ]) {
    expect(value.sourceOwner).toEqual(selection.sourceOwner);
    expect(value.logicalWork).toEqual(selection.logicalWork);
    expect(value.author.nativeTaskId).toBeNull();
    expect(value.author.nativeRunId).toBeNull();
    expect(value.nativeAuthorshipAuthenticated).toBe(false);
    expect(value.publisherIdentityInferred).toBe(false);
    expect(validateSourceAuthorMetadata(value)).toEqual(value);
  }
});

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "native-source-author-test-"));
  roots.push(root);
  await writeFile(
    join(root, "project.json"),
    JSON.stringify({
      schemaVersion: "1.0.0",
      kind: "project",
      projectId: "fixture-owner",
      domainId: "fixture-domain",
    }),
  );
  await sourceAuthorGit(root, ["init", "-q"]);
  const commit = async () => {
    await sourceAuthorGit(root, ["add", "."]);
    await sourceAuthorGit(root, [
      "-c",
      "user.name=Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "commit",
      "-qm",
      "Fictional source",
    ]);
    return sourceAuthorGit(root, ["rev-parse", "HEAD"]);
  };
  const sourceBaseSha = await commit();
  const value = captureCodexSourceAuthor(selection, sourceBaseSha, env, at);
  await mkdir(join(root, ".github"));
  await writeFile(join(root, ".github/source-author.json"), encode(value));
  const sourceHead = await commit();
  const input = {
    workspace: root,
    repository: "fixture/owner",
    projectId: "fixture-owner",
    sourceHead,
  };
  return { root, input, value, commit };
}

async function mergeFixture(f: Awaited<ReturnType<typeof fixture>>, reverse = false) {
  const authoredHead = await sourceAuthorGit(f.root, ["rev-parse", "HEAD"]);
  await sourceAuthorGit(f.root, ["checkout", "--detach", f.value.sourceBaseSha]);
  await writeFile(join(f.root, "target-only.ts"), "export const target = true;\n");
  const targetHead = await f.commit();
  if (reverse) await sourceAuthorGit(f.root, ["checkout", "--detach", authoredHead]);
  await sourceAuthorGit(f.root, [
    "-c",
    "user.name=Fixture",
    "-c",
    "user.email=fixture@example.invalid",
    "merge",
    "--no-ff",
    "-m",
    "Fictional prospective merge",
    reverse ? targetHead : authoredHead,
  ]);
  return {
    authoredHead,
    targetHead,
    sourceHead: await sourceAuthorGit(f.root, ["rev-parse", "HEAD"]),
  };
}

test("committed readback binds actual head/tree and exact metadata blob/hash without self-reference", async () => {
  const f = await fixture(),
    readback = await verifyCommittedSourceAuthor(f.input);
  expect(readback.sourceHead).toBe(f.input.sourceHead);
  expect(readback.sourceTree).toBe(await sourceAuthorGit(f.root, ["rev-parse", "HEAD^{tree}"]));
  expect(readback.metadataGitBlob).toBe(
    await sourceAuthorGit(f.root, ["rev-parse", "HEAD:.github/source-author.json"]),
  );
  expect(readback.metadataSha256).toBe(createHash("sha256").update(encode(f.value)).digest("hex"));
  expect(readback.metadata.sourceBaseSha).not.toBe(readback.sourceHead);
});

test("a later source commit cannot reuse unchanged metadata from a previous native attempt", async () => {
  const f = await fixture();
  await writeFile(join(f.root, "later.ts"), "export const laterAuthor = true;\n");
  const sourceHead = await f.commit();
  await expect(verifyCommittedSourceAuthor({ ...f.input, sourceHead })).rejects.toThrow();
});

test("recapture on the actual immediate authoring base admits the next native attempt", async () => {
  const f = await fixture();
  const value = captureCodexSourceAuthor(
    selection,
    f.input.sourceHead,
    { CODEX_THREAD_ID: "00000000-0000-4000-8000-000000000002" },
    "2026-10-19T01:00:00.000Z",
  );
  await writeFile(join(f.root, "later.ts"), "export const laterAuthor = true;\n");
  await writeFile(join(f.root, ".github/source-author.json"), encode(value));
  const sourceHead = await f.commit();
  const readback = await verifyCommittedSourceAuthor({ ...f.input, sourceHead });
  expect(readback.metadata).toEqual(value);
  expect(readback.nativeAuthorshipAuthenticated).toBe(false);
});

test("a changed metadata blob cannot nominate an arbitrary older ancestor as its authoring base", async () => {
  const f = await fixture();
  await writeFile(
    join(f.root, ".github/source-author.json"),
    encode({ ...f.value, capturedAtUtc: "2026-10-19T01:00:00.000Z" }),
  );
  const sourceHead = await f.commit();
  await expect(verifyCommittedSourceAuthor({ ...f.input, sourceHead })).rejects.toThrow();
});

test("a clean ordered prospective merge retains its fresh authored second-parent metadata", async () => {
  const f = await fixture();
  const merge = await mergeFixture(f);
  const index = await readFile(join(f.root, ".git/index"));
  const readback = await verifyCommittedSourceAuthor({ ...f.input, sourceHead: merge.sourceHead });
  expect(readback.metadata).toEqual(f.value);
  expect(readback.metadataGitBlob).toBe(
    await sourceAuthorGit(f.root, [
      "rev-parse",
      merge.authoredHead + ":.github/source-author.json",
    ]),
  );
  expect(await sourceAuthorGit(f.root, ["rev-parse", "HEAD"])).toBe(merge.sourceHead);
  expect(await readFile(join(f.root, ".git/index"))).toEqual(index);
  expect(await sourceAuthorGit(f.root, ["status", "--porcelain=v1"])).toBe("");
});

test("a prospective merge cannot launder stale second-parent metadata or reverse parent roles", async () => {
  const stale = await fixture();
  await writeFile(join(stale.root, "later.ts"), "export const laterAuthor = true;\n");
  await stale.commit();
  const staleMerge = await mergeFixture(stale);
  await expect(
    verifyCommittedSourceAuthor({ ...stale.input, sourceHead: staleMerge.sourceHead }),
  ).rejects.toThrow();
  const reversed = await fixture();
  const reversedMerge = await mergeFixture(reversed, true);
  await expect(
    verifyCommittedSourceAuthor({ ...reversed.input, sourceHead: reversedMerge.sourceHead }),
  ).rejects.toThrow();
});

test("a manually edited merge tree cannot carry forward a fresh authored-parent blob", async () => {
  const f = await fixture();
  const merge = await mergeFixture(f);
  await writeFile(join(f.root, "unattributed-merge-edit.ts"), "export const manualEdit = true;\n");
  await sourceAuthorGit(f.root, ["add", "."]);
  const tampered = await sourceAuthorGit(f.root, [
    "-c",
    "user.name=Fixture",
    "-c",
    "user.email=fixture@example.invalid",
    "commit-tree",
    await sourceAuthorGit(f.root, ["write-tree"]),
    "-p",
    merge.targetHead,
    "-p",
    merge.authoredHead,
    "-m",
    "Fictional manual merge edit",
  ]);
  await sourceAuthorGit(f.root, ["checkout", "--detach", tampered]);
  await expect(verifyCommittedSourceAuthor({ ...f.input, sourceHead: tampered })).rejects.toThrow();
  await writeFile(
    join(f.root, ".github/source-author.json"),
    encode(captureCodexSourceAuthor(selection, tampered, env, "2026-10-19T02:00:00.000Z")),
  );
  const recaptured = await f.commit();
  expect(
    (await verifyCommittedSourceAuthor({ ...f.input, sourceHead: recaptured })).metadata
      .sourceBaseSha,
  ).toBe(tampered);
});

test("unsupported nested or octopus merges and configured external drivers refuse", async () => {
  const f = await fixture();
  const merge = await mergeFixture(f);
  const mergedTree = await sourceAuthorGit(f.root, ["rev-parse", "HEAD^{tree}"]);
  for (const parents of [
    [merge.targetHead, merge.sourceHead],
    [merge.targetHead, merge.authoredHead, f.value.sourceBaseSha],
  ]) {
    const sourceHead = await sourceAuthorGit(f.root, [
      "-c",
      "user.name=Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "commit-tree",
      mergedTree,
      ...parents.flatMap((parent) => ["-p", parent]),
      "-m",
      "Fictional unsupported merge topology",
    ]);
    await sourceAuthorGit(f.root, ["checkout", "--detach", sourceHead]);
    await expect(verifyCommittedSourceAuthor({ ...f.input, sourceHead })).rejects.toThrow();
  }
  await sourceAuthorGit(f.root, ["checkout", "--detach", merge.sourceHead]);
  await sourceAuthorGit(f.root, ["config", "merge.fixture.driver", "touch .external-driver-ran"]);
  await expect(
    verifyCommittedSourceAuthor({ ...f.input, sourceHead: merge.sourceHead }),
  ).rejects.toThrow();
  expect(await Bun.file(join(f.root, ".external-driver-ran")).exists()).toBe(false);
});

test("depth-one source history cannot qualify a fresh-looking metadata record", async () => {
  const f = await fixture();
  const shallow = join(f.root, "shallow-copy");
  await sourceAuthorGit(f.root, ["clone", "--quiet", "--depth=1", "file://" + f.root, shallow]);
  await expect(verifyCommittedSourceAuthor({ ...f.input, workspace: shallow })).rejects.toThrow();
});

test("wrong repository/project/head, missing/unsafe path and modified working bytes refuse", async () => {
  const f = await fixture();
  for (const invalid of [
    { ...f.input, repository: "foreign/owner" },
    { ...f.input, projectId: "foreign-project" },
    { ...f.input, sourceHead: "b".repeat(40) },
    { ...f.input, metadataPath: ".github/missing.json" },
    { ...f.input, metadataPath: "../source-author.json" },
    { ...f.input, metadataPath: ".git/config.json" },
  ])
    await expect(verifyCommittedSourceAuthor(invalid)).rejects.toThrow();
  await writeFile(
    join(f.root, ".github/source-author.json"),
    encode({ ...f.value, capturedAtUtc: "2026-10-20T00:00:00.000Z" }),
  );
  await expect(verifyCommittedSourceAuthor(f.input)).rejects.toThrow();
});

test("symlink metadata and unrelated/missing source bases cannot enter the committed readback", async () => {
  const f = await fixture();
  const path = join(f.root, ".github/source-author.json");
  const unrelated = await sourceAuthorGit(f.root, [
    "-c",
    "user.name=Fixture",
    "-c",
    "user.email=fixture@example.invalid",
    "commit-tree",
    await sourceAuthorGit(f.root, ["rev-parse", "HEAD^{tree}"]),
    "-m",
    "Fictional unrelated source root",
  ]);
  await writeFile(path, encode({ ...f.value, sourceBaseSha: unrelated }));
  const unrelatedHead = await f.commit();
  await expect(
    verifyCommittedSourceAuthor({ ...f.input, sourceHead: unrelatedHead }),
  ).rejects.toThrow();
  await writeFile(path, encode({ ...f.value, sourceBaseSha: "b".repeat(40) }));
  const head = await f.commit();
  await expect(verifyCommittedSourceAuthor({ ...f.input, sourceHead: head })).rejects.toThrow();
  await rm(path);
  await symlink("../project.json", path);
  const linked = await f.commit();
  await expect(verifyCommittedSourceAuthor({ ...f.input, sourceHead: linked })).rejects.toThrow();
  expect((await readFile(join(f.root, "project.json"), "utf8")).includes("fixture-owner")).toBe(
    true,
  );
});
