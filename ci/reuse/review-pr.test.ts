import {
  admitTrustedSubject,
  reviewRepository,
  requireOwnerIdentity,
  requireTrustedReviewContext,
  type TrustedReviewContext,
  validateCommittedOwnerPolicy,
} from "./trusted-review-source";
import {
  codexReviewThread,
  bindCompletedCodexReview,
  bindNativeReviewer,
  reviewerInvocation,
} from "./review-runtimes";
import { ProcessOutputCapture } from "../quality/trusted/process";
import { expect, test } from "bun:test";
import { join } from "node:path";

test("native Codex review identity must come from exactly one real thread event", () => {
  const id = "00000000-0000-4000-8000-000000000001";
  const event = JSON.stringify({ type: "thread.started", thread_id: id }) + "\n";
  expect(
    codexReviewThread(Buffer.from(event + JSON.stringify({ type: "turn.completed" }) + "\n")),
  ).toBe(id);
  for (const text of [
    "",
    JSON.stringify({ type: "thread.started", thread_id: "logical-task" }) + "\n",
    event + event,
    JSON.stringify({ type: "turn.completed" }) + "\n",
    event + "not-json\n",
  ])
    expect(() => codexReviewThread(Buffer.from(text))).toThrow();
});

test("native completion grammar binds only the actual terminal message and refuses every incomplete state", () => {
  const thread = "00000000-0000-4000-8000-000000000003",
    raw = Buffer.from('{"verdict":"synthetic"}');
  const start = { type: "thread.started", thread_id: thread },
    turn = { type: "turn.started" };
  const message = { type: "item.completed", item: { type: "agent_message", text: raw.toString() } };
  const done = { type: "turn.completed" };
  const encode = (rows: unknown[]) =>
    Buffer.from(rows.map((row) => JSON.stringify(row)).join("\n") + "\n");
  const valid = [
    start,
    turn,
    { type: "item.completed", item: { type: "agent_message", text: "commentary" } },
    message,
    done,
  ];
  expect(bindCompletedCodexReview(encode(valid), raw)).toBe(thread);
  const refusals = [
    [start, turn, message],
    [start, turn, done],
    [start, message, done],
    [start, turn, message, { type: "turn.failed" }],
    [start, turn, message, { type: "error" }],
    [start, turn, message, { type: "unknown" }, done],
    [start, turn, message, done, message],
    [start, turn, message, done, done],
    [start, turn, turn, message, done],
    [start, turn, message, { type: "item.completed", item: { type: "command_execution" } }, done],
    [start, start, turn, message, done],
    [{ ...start, thread_id: "logical-task" }, turn, message, done],
  ];
  for (const rows of refusals) expect(() => bindCompletedCodexReview(encode(rows), raw)).toThrow();
  for (const bytes of [
    Buffer.from('{"verdict":"replaced"}'),
    Buffer.concat([raw, Buffer.from("\n")]),
  ])
    expect(() => bindCompletedCodexReview(encode(valid), bytes)).toThrow(
      "ACTUAL_NATIVE_FINAL_MESSAGE_REQUIRED",
    );
});
test("current judgment invokes native sol only and never Claude/Opus; Muse remains mechanical", () => {
  const id = "00000000-0000-4000-8000-000000000001";
  expect(reviewerInvocation("native-codex", id, false, "/synthetic-final")).toContain(
    "gpt-6.1-sol",
  );
  for (const smoke of [false, true])
    expect(() => reviewerInvocation("native-claude", id, smoke)).toThrow();
  expect(() => reviewerInvocation("native-muse", id, false)).toThrow();
  expect(reviewerInvocation("native-muse", id, true)).toContain("muse-spark-1.3");
});

test("every formal native adapter binds the requested runtime and actual session before publication", () => {
  const author = "00000000-0000-4000-8000-000000000001",
    reviewer = "00000000-0000-4000-8000-000000000002";
  for (const runtime of ["native-claude", "native-codex"]) {
    expect(() =>
      bindNativeReviewer({ reviewer: { runtime, taskId: reviewer } }, runtime, reviewer, author),
    ).not.toThrow();
    for (const raw of [
      { reviewer: { runtime: "native-muse", taskId: reviewer } },
      { reviewer: { runtime, taskId: "unrelated-session" } },
      { reviewer: null },
      {},
    ])
      expect(() => bindNativeReviewer(raw, runtime, reviewer, author)).toThrow(
        "actual reviewer runtime/session",
      );
    expect(() =>
      bindNativeReviewer({ reviewer: { runtime, taskId: author } }, runtime, author, author),
    ).toThrow("actual reviewer runtime/session");
  }
});

function admissionFixture() {
  const row = reviewRepository("park-kyungchan/agent-platform"),
    head = "a".repeat(40),
    base = "b".repeat(40);
  const repo = { id: row.id, full_name: row.repository, private: true, owner: { id: 241037670 } };
  const pull = {
    number: 76,
    state: "open",
    draft: true,
    user: { id: 241037670, type: "User" },
    head: { sha: head, ref: "feature", repo: structuredClone(repo) },
    base: { sha: base, ref: "main", repo: structuredClone(repo) },
  };
  return {
    row,
    head,
    base,
    repo,
    pulls: [pull],
    main: { name: "main", protected: true, commit: { sha: "c".repeat(40) } },
    commit: { sha: head, parents: [{ sha: base }] },
  };
}
test("authenticated private owner draft and exact protected main are legitimate controls", () => {
  const f = admissionFixture();
  expect(admitTrustedSubject(f.row, f.head, f.repo, f.pulls, f.main, f.commit)).toMatchObject({
    kind: "owner-pull-request",
    pr: 76,
    head: f.head,
    base: f.base,
  });
  f.main.commit.sha = f.head;
  expect(admitTrustedSubject(f.row, f.head, f.repo, [], f.main, f.commit)).toMatchObject({
    kind: "protected-main",
    pr: 0,
  });
  expect(() =>
    requireOwnerIdentity({ id: 241037670, login: "park-kyungchan", type: "User" }),
  ).not.toThrow();
});
test("fork, external actor, public/foreign repo, unassociated/stale head and wrong base refuse before review capability", () => {
  const mutations: ((f: ReturnType<typeof admissionFixture>) => void)[] = [
    (f) => {
      f.repo.private = false;
    },
    (f) => {
      f.repo.id++;
    },
    (f) => {
      f.repo.owner.id = 99;
    },
    (f) => {
      f.pulls[0]!.head.repo.id++;
    },
    (f) => {
      f.pulls[0]!.head.repo.owner.id = 99;
    },
    (f) => {
      f.pulls[0]!.head.repo.private = false;
    },
    (f) => {
      f.pulls[0]!.user.id = 99;
    },
    (f) => {
      f.pulls[0]!.user.type = "Bot";
    },
    (f) => {
      f.pulls[0]!.head.sha = "d".repeat(40);
    },
    (f) => {
      f.pulls[0]!.base.ref = "foreign";
    },
    (f) => {
      f.pulls[0]!.base.repo.id++;
    },
    (f) => {
      f.pulls = [];
    },
    (f) => {
      f.pulls[0]!.state = "closed";
    },
  ];
  for (const mutate of mutations) {
    const f = admissionFixture();
    mutate(f);
    expect(() => admitTrustedSubject(f.row, f.head, f.repo, f.pulls, f.main, f.commit)).toThrow();
  }
  const f = admissionFixture();
  f.main.commit.sha = f.head;
  f.main.protected = false;
  expect(() => admitTrustedSubject(f.row, f.head, f.repo, [], f.main, f.commit)).toThrow(
    "OWNER_PULL_OR_EXACT_PROTECTED_MAIN_REQUIRED",
  );
  for (const actor of [
    { id: 99, login: "park-kyungchan", type: "User" },
    { id: 241037670, login: "foreign", type: "User" },
    { id: 241037670, login: "park-kyungchan", type: "Bot" },
  ])
    expect(() => requireOwnerIdentity(actor)).toThrow("AUTHENTICATED_OWNER_REQUIRED");
  expect(() => reviewRepository("foreign/repository")).toThrow("PINNED_REPOSITORY_REQUIRED");
  expect(() => requireTrustedReviewContext({} as TrustedReviewContext)).toThrow(
    "AUTHENTICATED_CONTEXT_REQUIRED",
  );
});
test("committed consumer policy is inert data and must match pinned owner scope and publisher", () => {
  for (const repository of [
    "park-kyungchan/education-platform",
    "park-kyungchan/academy-workbench",
  ]) {
    const row = reviewRepository(repository),
      policy = {
        repository,
        projectId: row.projectId,
        domainId: row.domainId,
        semanticOwner: row.semanticOwner,
        scope: row.scope,
        statusContext: "native-independent-review",
        publisher: { type: "User", login: "park-kyungchan", id: "241037670" },
      };
    const provider = {
      codeOwner: "agent-platform",
      generatedOnly: true,
      sourceOnly: true,
      runtimeAuthority: false,
      nativePermissionGranted: false,
      policy,
    };
    expect(() => validateCommittedOwnerPolicy(row, provider)).not.toThrow();
    expect(() =>
      validateCommittedOwnerPolicy(row, { ...provider, policy: { ...policy, scope: "foreign" } }),
    ).toThrow("PINNED_COMMITTED_OWNER_POLICY_REQUIRED");
    expect(() =>
      validateCommittedOwnerPolicy(row, { ...provider, nativePermissionGranted: true }),
    ).toThrow("FIXED_COMMITTED_OWNER_MANIFEST_REQUIRED");
  }
});

test("predecessor commit-only admission reaches reviewer on foreign PR provenance; current policy refuses", async () => {
  const f = admissionFixture();
  f.pulls[0]!.user.id = 99;
  f.pulls[0]!.head.repo.id = 99;
  const previous = Bun.spawnSync(
    ["git", "show", "e00292f6cb801561c834a8e59b5c2904f2d382bf:ci/reuse/review-pr.ts"],
    { cwd: join(import.meta.dir, "../.."), stdout: "pipe", stderr: "pipe" },
  );
  expect(previous.exitCode).toBe(0);
  const text = previous.stdout.toString(),
    start = text.indexOf("export async function reviewPr("),
    end = text.indexOf("\nif (import.meta.main)");
  const body = new Bun.Transpiler({ loader: "ts" }).transformSync(
    text.slice(start, end).replace("export async function", "async function"),
  );
  let launched = false;
  const requests: string[] = [];
  const empty = () =>
    new ReadableStream<Uint8Array>({
      start(c) {
        c.close();
      },
    });
  const fakeBun = {
    spawnSync: (args: string[]) => {
      let out = "";
      if (args[0] === "gh") {
        requests.push(args.at(-1)!);
        out = JSON.stringify({ sha: f.head });
      } else if (args.includes("HEAD")) out = f.head;
      else if (args.some((a) => a.endsWith("^{tree}"))) out = "d".repeat(40);
      else if (args.includes("ls-tree")) out = "100644 blob " + "e".repeat(40) + "\tci/source.ts\0";
      else if (args.includes("merge-base")) out = f.base;
      return { exitCode: 0, stdout: Buffer.from(out) };
    },
    which: () => "/synthetic-reviewer",
    spawn: () => {
      launched = true;
      return { stdout: empty(), stderr: empty(), exited: Promise.resolve(1) };
    },
    write: async () => 0,
  };
  const names = [
    "Bun",
    "process",
    "nativeReviewPolicy",
    "verifyCommittedSourceAuthor",
    "deriveReviewedSourceManifest",
    "mkdtemp",
    "join",
    "tmpdir",
    "randomUUID",
    "reviewerInvocation",
    "readFile",
    "writeFile",
    "resolve",
  ];
  const factory = new Function(...names, body + "\nreturn reviewPr;");
  const old = factory(
    fakeBun,
    { cwd: () => "/synthetic-subject", env: {} },
    async () => ({ repository: f.row.repository, binding: {} }),
    async () => ({
      metadata: { author: { runtime: "native-codex", threadId: "synthetic-author" } },
    }),
    () => ({ sha256: "f".repeat(64) }),
    async () => "/synthetic-output",
    join,
    () => "/tmp",
    () => "00000000-0000-4000-8000-000000000001",
    () => ["synthetic-reviewer"],
    async () => Buffer.from('{"projectId":"agent-platform"}'),
    async () => {},
    (p: string) => p,
  ) as (head: string, runtime: string) => Promise<void>;
  await expect(old(f.head, "native-claude")).rejects.toThrow("Native reviewer failed");
  expect(launched).toBe(true);
  expect(requests).toEqual([`repos/${f.row.repository}/git/commits/${f.head}`]);
  expect(() => admitTrustedSubject(f.row, f.head, f.repo, f.pulls, f.main, f.commit)).toThrow(
    "OWNER_PULL_OR_EXACT_PROTECTED_MAIN_REQUIRED",
  );
});

test("metadata data reads disable a configured filesystem-monitor executable", async () => {
  const { mkdtemp, writeFile, readFile, access, rm } = await import("node:fs/promises"),
    { tmpdir } = await import("node:os"),
    { sourceAuthorGit } = await import("./source-author");
  const root = await mkdtemp(join(tmpdir(), "review-git-hooks-")),
    marker = join(root, "monitor-ran"),
    monitor = join(root, "monitor.sh");
  function git(args: string[]) {
    const r = Bun.spawnSync(["git", "-C", root, ...args], { stdout: "pipe", stderr: "pipe" });
    expect(r.exitCode).toBe(0);
    return r.stdout.toString();
  }
  try {
    git(["init", "-q"]);
    await writeFile(monitor, '#!/bin/sh\nprintf called > "' + marker + '"\nprintf "token\\0"\n', {
      mode: 0o700,
    });
    git(["config", "core.fsmonitor", monitor]);
    git(["status", "--porcelain=v1"]);
    await expect(readFile(marker, "utf8")).resolves.toBe("called");
    await rm(marker);
    await sourceAuthorGit(root, ["status", "--porcelain=v1"]);
    await expect(access(marker)).rejects.toThrow();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

type NativeCaseApi = {
  requireNativeReviewAttempt: (c: unknown, a: unknown) => void;
  requireCurrentReviewAttempt: (c: unknown, a: unknown) => Promise<unknown[]>;
  beginNativeReviewAttempt: (c: unknown, id: string, assignment?: unknown) => Promise<unknown>;
  prepareMergeCorrespondenceAssignment: (c: unknown) => Promise<unknown>;
  invokeNativeReview: (c: unknown, a: unknown, r: string) => Promise<unknown>;
  failNativeReviewAttempt: (a: unknown, o: unknown) => Promise<void>;
  readCompletedNativeObservation: (
    c: unknown,
    a: unknown,
    o: unknown,
  ) => { bytes: Buffer; raw: Record<string, unknown> };
};
async function nativeCase(scenario = "pass", predecessor = false) {
  const { readFile } = await import("node:fs/promises"),
    { createHash } = await import("node:crypto");
  const { reviewAttemptTarget, validateCompletedReviewObservation } =
    await import("../quality/review-artifact");
  const {
    requireFullReviewAssignment,
    sameMergeCorrespondenceAssignment,
    parseMergeCorrespondenceAssignment,
  } = await import("../quality/review-assignment");
  const old = predecessor
    ? Bun.spawnSync(
        ["git", "show", "8f78f54111f59e3af2bf702826eeefa761d2568d:ci/reuse/review-attempt.ts"],
        { cwd: import.meta.dir, stdout: "pipe", stderr: "pipe" },
      )
    : null;
  if (old) expect(old.exitCode).toBe(0);
  const source = new Bun.Transpiler({ loader: "ts" })
    .transformSync(
      old
        ? old.stdout.toString()
        : await readFile(join(import.meta.dir, "review-attempt.ts"), "utf8"),
    )
    .replace(/^import[\s\S]*?;\n/gm, "")
    .replaceAll("export ", "");
  const head = "a".repeat(40),
    tree = "b".repeat(40),
    manifest = "f".repeat(64),
    id = "00000000-0000-4000-8000-000000000002",
    thread = "00000000-0000-4000-8000-000000000003";
  const actor = { id: 42, login: "native-owner", type: "User" };
  const mergeAssignment = parseMergeCorrespondenceAssignment({
    schema: "native-review-merge-correspondence.v1",
    kind: "merge-correspondence",
    orderedParents: ["c".repeat(40), "d".repeat(40)],
    parentReview: {
      statusId: 42,
      targetUrl: "https://github.com/fixture/owner/full-parent",
      proofSha256: "e".repeat(64),
    },
  });
  let posts = 0,
    launches = 0,
    revoked = false;
  const pendingFences: unknown[] = [];
  const states: Record<string, unknown>[] = [
    {
      id: 1,
      state: "success",
      context: "native-independent-review",
      creator: actor,
      target_url: "https://github.com/fixture/owner/old-proof",
    },
  ];
  const context = {
    head,
    controllerSha: scenario === "self" ? head : "e".repeat(40),
    sourceRoot: "/synthetic-data",
    policy: {
      repository: scenario === "self" ? "park-kyungchan/agent-platform" : "fixture/owner",
      publisher: { ...actor, id: "42" },
      binding: {
        codeOwner: "agent-platform",
        evidenceOwner: "codex-harness-engineering",
        scope: "canonical-source-ci-quality",
      },
    },
    row: { projectId: "agent-platform" },
    subject: {
      base: "c".repeat(40),
      kind: scenario.startsWith("merge-") ? "protected-main" : "owner-pull-request",
      pr: 1,
    },
    author: { metadata: { author: { runtime: "native-codex", threadId: "actual-author" } } },
    revalidate: async () => {
      if (revoked) throw Error("QUALIFIED_PIN_HELD");
      if (scenario === "replace-negative-raw" && launches > 0) {
        await Promise.resolve();
        const replacement = JSON.parse(bytes.toString());
        replacement.conclusion = "pass";
        replacement.unresolvedFindings = 0;
        replacement.findings = [];
        bytes = Buffer.from(JSON.stringify(replacement));
        eventsFile = nativeEvents(bytes);
      }
      if (scenario === "replace-events-file" && launches > 0)
        eventsFile = Buffer.from("substituted event pathname");
    },
    readApi: async () => states,
  };
  let bytes = Buffer.from(
    JSON.stringify({
      schema: scenario.startsWith("merge-")
        ? "native-independent-merge-correspondence-output.v1"
        : "native-independent-review-output.v1",
      reviewStatus: "complete",
      binding: context.policy.binding,
      headSha: scenario === "wrong-source" ? "e".repeat(40) : head,
      reviewedTreeSha: tree,
      reviewedSourceManifestSha256: manifest,
      reviewer: { runtime: "native-codex", taskId: thread },
      author: { runtime: "native-codex", taskId: "actual-author" },
      conclusion: ["negative", "replace-negative-raw"].includes(scenario) ? "fail" : "pass",
      unresolvedFindings: ["negative", "replace-negative-raw"].includes(scenario) ? 1 : 0,
      findings: ["negative", "replace-negative-raw"].includes(scenario)
        ? [{ path: "source.ts", line: 1, severity: "P1", explanation: "synthetic defect" }]
        : [],
      nativeReviewPendingStatusId: 101,
      ...(scenario.startsWith("merge-") && scenario !== "merge-dropped-raw"
        ? {
            reviewAssignment:
              scenario === "merge-substituted-raw"
                ? {
                    ...mergeAssignment,
                    parentReview: { ...mergeAssignment.parentReview, statusId: 43 },
                  }
                : mergeAssignment,
          }
        : {}),
    }),
  );
  const nativeEvents = (raw: Buffer) =>
    Buffer.from(
      JSON.stringify({ type: "thread.started", thread_id: thread }) +
        "\n" +
        JSON.stringify({ type: "turn.started" }) +
        "\n" +
        JSON.stringify({
          type: "item.completed",
          item: { type: "agent_message", text: raw.toString() },
        }) +
        "\n" +
        JSON.stringify({ type: "turn.completed" }) +
        "\n",
    );
  const events = nativeEvents(bytes);
  let eventsFile = events;
  const stream = (bytes = Buffer.alloc(0)) =>
    new ReadableStream<Uint8Array>({
      start(c) {
        if (bytes.length) c.enqueue(bytes);
        c.close();
      },
    });
  const nativeBun = {
    spawnSync: (args: string[]) => ({
      exitCode: 0,
      stdout: Buffer.from(
        args.includes("user")
          ? JSON.stringify({ ...actor, id: scenario === "wrong-owner" ? 99 : 42 })
          : "fixture-token-not-a-credential",
      ),
    }),
    which: () => "/synthetic-codex",
    spawn: () => {
      launches++;
      return {
        stdout: stream(events),
        stderr: stream(),
        exited: Promise.resolve(0),
        kill: () => {},
      };
    },
    write: async () => 0,
  };
  const nativeFetch = async (_url: string, options: RequestInit) => {
    posts++;
    const value = { ...JSON.parse(String(options.body)), id: 100 + posts, creator: actor };
    states.unshift(value);
    return new Response(JSON.stringify(value), { status: scenario === "uncertain" ? 503 : 201 });
  };
  const git = (_root: string, args: string[]) =>
    args.includes("ls-tree")
      ? "100644 blob " + "d".repeat(40) + "\tsource.ts\0"
      : args.includes("merge-base")
        ? "c".repeat(40)
        : tree;
  const names = [
    "Bun",
    "fetch",
    "requireTrustedReviewContext",
    "reviewAttemptTarget",
    "validateCompletedReviewObservation",
    "console",
    "randomUUID",
    "createHash",
    "mkdtemp",
    "readFile",
    "writeFile",
    "join",
    "tmpdir",
    "deriveReviewedSourceManifest",
    "reviewerInvocation",
    "codexReviewThread",
    "bindNativeReviewer",
    "reviewDataGit",
    "reviewControllerRoot",
    "reviewControllerRepository",
    "bindCompletedCodexReview",
    "ProcessOutputCapture",
    "deriveMergeCorrespondenceAssignment",
    "validateMergeCorrespondenceAssignment",
    "requireFullReviewAssignment",
    "sameMergeCorrespondenceAssignment",
    "parseMergeCorrespondenceAssignment",
  ];
  const api = new Function(
    ...names,
    source +
      "\nreturn {beginNativeReviewAttempt,invokeNativeReview,failNativeReviewAttempt,readCompletedNativeObservation,requireNativeReviewAttempt,requireCurrentReviewAttempt,...(typeof prepareMergeCorrespondenceAssignment === 'function' ? {prepareMergeCorrespondenceAssignment} : {})};",
  )(
    nativeBun,
    nativeFetch,
    (c: unknown) => {
      if (c !== context) throw Error("FOREIGN_CONTEXT");
    },
    reviewAttemptTarget,
    validateCompletedReviewObservation,
    { log: () => {} },
    () => id,
    createHash,
    async () => "/synthetic-native-output",
    async (path: string) => (path.endsWith("events.jsonl") ? eventsFile : bytes),
    async () => {},
    join,
    () => "/tmp",
    () => ({ sha256: manifest }),
    () => ["codex"],
    codexReviewThread,
    bindNativeReviewer,
    git,
    "/synthetic-controller",
    "park-kyungchan/agent-platform",
    bindCompletedCodexReview,
    ProcessOutputCapture,
    async () => mergeAssignment,
    async (...args: unknown[]) => {
      pendingFences.push(args[4]);
      if (scenario === "merge-parent-held" && launches) throw Error("FULL_PARENT_HELD");
      return mergeAssignment;
    },
    requireFullReviewAssignment,
    sameMergeCorrespondenceAssignment,
    parseMergeCorrespondenceAssignment,
  ) as NativeCaseApi;
  return {
    api,
    context,
    id,
    states,
    pendingFences,
    get posts() {
      return posts;
    },
    get launches() {
      return launches;
    },
    replaceRaw: () => {
      bytes = Buffer.from("not the captured observation");
    },
    revoke: () => {
      revoked = true;
    },
  };
}

test("MERGE-CORRESPONDENCE actual native invocation retains its opaque prepared assignment", async () => {
  const h = await nativeCase("merge-pass"),
    assignment = await h.api.prepareMergeCorrespondenceAssignment(h.context);
  expect(Object.isFrozen(assignment)).toBe(true);
  expect(Object.keys(assignment as object)).toEqual(["kind"]);
  const attempt = await h.api.beginNativeReviewAttempt(h.context, h.id, assignment),
    observation = await h.api.invokeNativeReview(h.context, attempt, "native-codex");
  expect(
    (
      h.api.readCompletedNativeObservation(h.context, attempt, observation).raw
        .reviewAssignment as { kind: string }
    ).kind,
  ).toBe("merge-correspondence");
  expect(h.launches).toBe(1);
  expect(h.posts).toBe(1);
});

test("CURRENTNESS native bounded checks carry the exact pending-head anchor through actual invocation", async () => {
  const h = await nativeCase("merge-pass"),
    assignment = await h.api.prepareMergeCorrespondenceAssignment(h.context),
    attempt = await h.api.beginNativeReviewAttempt(h.context, h.id, assignment);
  await h.api.invokeNativeReview(h.context, attempt, "native-codex");
  expect(
    h.pendingFences.slice(-2).map((value) => (value as { id?: number } | undefined)?.id),
  ).toEqual([101, 101]);
  expect(h.posts).toBe(1);
  expect(h.launches).toBe(1);
});

test("MERGE-CORRESPONDENCE caller JSON cannot mint an assignment or start pending", async () => {
  const h = await nativeCase("merge-pass");
  await expect(
    h.api.beginNativeReviewAttempt(h.context, h.id, { kind: "prepared-merge-correspondence" }),
  ).rejects.toThrow("OPAQUE_MERGE_ASSIGNMENT_REQUIRED");
  expect(h.launches).toBe(0);
  expect(h.posts).toBe(0);
});

for (const scenario of ["merge-dropped-raw", "merge-substituted-raw", "merge-parent-held"])
  test("MERGE-CORRESPONDENCE actual observation refuses " + scenario, async () => {
    const h = await nativeCase(scenario),
      assignment = await h.api.prepareMergeCorrespondenceAssignment(h.context),
      attempt = await h.api.beginNativeReviewAttempt(h.context, h.id, assignment);
    await expect(h.api.invokeNativeReview(h.context, attempt, "native-codex")).rejects.toThrow();
    expect(h.launches).toBe(1);
    expect(h.posts).toBe(1);
    expect(h.states[0]!.state).toBe("pending");
  });

test("raw pathname replacement during post-exit revalidation cannot promote actual negative native judgment", async () => {
  const old = await nativeCase("replace-negative-raw", true),
    oldAttempt = await old.api.beginNativeReviewAttempt(old.context, old.id);
  const forged = await old.api.invokeNativeReview(old.context, oldAttempt, "native-codex");
  expect(
    old.api.readCompletedNativeObservation(old.context, oldAttempt, forged).raw.conclusion,
  ).toBe("pass");
  const h = await nativeCase("replace-negative-raw"),
    a = await h.api.beginNativeReviewAttempt(h.context, h.id);
  await expect(h.api.invokeNativeReview(h.context, a, "native-codex")).rejects.toThrow(
    "ACTUAL_NATIVE_FINAL_MESSAGE_REQUIRED",
  );
  expect(h.launches).toBe(1);
  expect(h.posts).toBe(1);
  expect(h.states[0]!.state).toBe("pending");
});

test("completed native observation uses the owned event stream even if the event pathname changes", async () => {
  const h = await nativeCase("replace-events-file"),
    a = await h.api.beginNativeReviewAttempt(h.context, h.id);
  const observation = await h.api.invokeNativeReview(h.context, a, "native-codex");
  expect(h.api.readCompletedNativeObservation(h.context, a, observation).raw.conclusion).toBe(
    "pass",
  );
  expect(h.launches).toBe(1);
  expect(h.posts).toBe(1);
});

test("a pending handle cannot fabricate completion; actual invocation owns immutable raw bytes and is single-use", async () => {
  const h = await nativeCase(),
    a = await h.api.beginNativeReviewAttempt(h.context, h.id);
  await expect(
    h.api.failNativeReviewAttempt(a, { kind: "completed-native-observation" }),
  ).rejects.toThrow("COMPLETED_NATIVE_OBSERVATION_REQUIRED");
  expect(h.posts).toBe(1);
  expect(h.launches).toBe(0);
  const observation = await h.api.invokeNativeReview(h.context, a, "native-codex");
  expect(h.launches).toBe(1);
  const original = h.api.readCompletedNativeObservation(h.context, a, observation).bytes;
  h.replaceRaw();
  const copy = h.api.readCompletedNativeObservation(h.context, a, observation).bytes;
  copy.fill(0);
  expect(
    h.api.readCompletedNativeObservation(h.context, a, observation).bytes.equals(original),
  ).toBe(true);
  await expect(h.api.failNativeReviewAttempt(a, observation)).rejects.toThrow(
    "BOUND_NEGATIVE_REVIEW_REQUIRED",
  );
  expect(h.posts).toBe(1);
  await expect(h.api.invokeNativeReview(h.context, a, "native-codex")).rejects.toThrow(
    "NATIVE_INVOCATION_ALREADY_STARTED",
  );
  expect(h.launches).toBe(1);
});

test("bound actual negative invalidates another completed pass, and revoked qualification blocks every mutation API", async () => {
  const h = await nativeCase("negative"),
    a = await h.api.beginNativeReviewAttempt(h.context, h.id),
    observation = await h.api.invokeNativeReview(h.context, a, "native-codex");
  h.states.unshift({ ...h.states[0], id: 999, state: "success" });
  await h.api.failNativeReviewAttempt(a, observation);
  expect(h.posts).toBe(2);
  expect(h.states[0]!.state).toBe("failure");
  await expect(h.api.failNativeReviewAttempt(a, observation)).rejects.toThrow(
    "AUTHENTICATED_ATTEMPT_REQUIRED",
  );
  expect(h.posts).toBe(2);
  const r = await nativeCase("negative"),
    pending = await r.api.beginNativeReviewAttempt(r.context, r.id),
    completed = await r.api.invokeNativeReview(r.context, pending, "native-codex");
  r.revoke();
  await expect(r.api.failNativeReviewAttempt(pending, completed)).rejects.toThrow(
    "QUALIFIED_PIN_HELD",
  );
  expect(r.posts).toBe(1);
  await expect(r.api.beginNativeReviewAttempt(r.context, r.id)).rejects.toThrow(
    "QUALIFIED_PIN_HELD",
  );
  expect(r.posts).toBe(1);
});

test("formal mutation refuses same controller, wrong owner/source and uncertain POST without retry", async () => {
  for (const scenario of ["self", "wrong-owner", "uncertain", "wrong-source"]) {
    const h = await nativeCase(scenario);
    if (scenario === "wrong-source") {
      const a = await h.api.beginNativeReviewAttempt(h.context, h.id);
      await expect(h.api.invokeNativeReview(h.context, a, "native-codex")).rejects.toThrow();
      expect(h.posts).toBe(1);
      expect(h.states[0]!.state).toBe("pending");
    } else {
      await expect(h.api.beginNativeReviewAttempt(h.context, h.id)).rejects.toThrow(
        scenario === "self"
          ? "DISTINCT_CONTROLLER_REQUIRED"
          : scenario === "uncertain"
            ? "OUTCOME_UNCERTAIN"
            : "AUTHENTICATED_OWNER_REQUIRED",
      );
      expect(h.posts).toBe(scenario === "uncertain" ? 1 : 0);
      expect(h.launches).toBe(0);
    }
  }
});
