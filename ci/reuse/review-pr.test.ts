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

test("generic review and publisher entrypoints refuse CI before source reads or native/authenticated effects", async () => {
  for (const entry of ["review-pr.ts", "publish-review-artifact.ts"]) {
    for (const marker of [{ CI: "true" }, { GITHUB_ACTIONS: "true" }]) {
      const child = Bun.spawn(
        [
          process.execPath,
          "--no-env-file",
          join(import.meta.dir, entry),
          "park-kyungchan/agent-platform",
          "a".repeat(40),
          "native-codex",
          "b".repeat(40),
        ],
        { cwd: "/tmp", env: { PATH: "/usr/bin:/bin", ...marker }, stdout: "pipe", stderr: "pipe" },
      );
      const [stdout, stderr, exit] = await Promise.all([
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
        child.exited,
      ]);
      expect(exit).toBe(1);
      expect(stdout).toBe("");
      expect(stderr).toMatch(
        new RegExp(
          "^review_artifact_refused@ci/reuse/" + entry.replace(".", "\\.") + ":[1-9][0-9]*\\n$",
        ),
      );
    }
  }
});

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
test("old subject entrypoint executes contributor code; pinned launcher enters only trusted controller", async () => {
  const { mkdtemp, mkdir, writeFile, readFile, rm, access } = await import("node:fs/promises"),
    { tmpdir } = await import("node:os");
  const root = await mkdtemp(join(tmpdir(), "review-controller-boundary-")),
    trusted = join(root, "trusted"),
    subject = join(root, "subject"),
    marker = join(root, "subject-executed");
  function git(args: string[]) {
    const r = Bun.spawnSync(["git", "-C", trusted, ...args], { stdout: "pipe", stderr: "pipe" });
    if (r.exitCode !== 0) throw Error("Synthetic Git fixture failed");
    return r.stdout.toString().trim();
  }
  try {
    await mkdir(join(trusted, "ci/reuse"), { recursive: true });
    await mkdir(join(subject, "ci/reuse"), { recursive: true });
    await writeFile(
      join(subject, "ci/reuse/review-pr.ts"),
      `await Bun.write(${JSON.stringify(marker)},"synthetic-publisher-capability");`,
    );
    const old = Bun.spawn(
      [process.execPath, "--no-env-file", join(subject, "ci/reuse/review-pr.ts")],
      { cwd: subject, stdout: "pipe", stderr: "pipe" },
    );
    expect(await old.exited).toBe(0);
    expect(await readFile(marker, "utf8")).toBe("synthetic-publisher-capability");
    await rm(marker);
    await writeFile(
      join(trusted, "ci/reuse/review-pr.sh"),
      await readFile(join(import.meta.dir, "review-pr.sh")),
    );
    await writeFile(
      join(trusted, "ci/reuse/review-pr.ts"),
      "console.log(JSON.stringify({cwd:process.cwd(),args:Bun.argv.slice(2)}));",
    );
    git(["init", "-q"]);
    git(["add", "."]);
    git([
      "-c",
      "user.name=Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "commit",
      "-qm",
      "Qualified fixture",
    ]);
    const pin = git(["rev-parse", "HEAD"]);
    git(["checkout", "--detach", pin]);
    const child = Bun.spawn(
      [
        "bash",
        join(trusted, "ci/reuse/review-pr.sh"),
        "park-kyungchan/agent-platform",
        "a".repeat(40),
        "native-codex",
        pin,
      ],
      {
        cwd: subject,
        env: { PATH: join(process.execPath, "..") + ":/usr/bin:/bin" },
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const output = await new Response(child.stdout).text();
    expect(await child.exited).toBe(0);
    expect(JSON.parse(output).cwd).toBe(trusted);
    await expect(access(marker)).rejects.toThrow();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
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

test("a genuine negative re-review supersedes old success, and the hold precedes native launch", async () => {
  const root = join(import.meta.dir, "../.."),
    head = "a".repeat(40),
    tree = "b".repeat(40),
    manifest = "f".repeat(64),
    id = "00000000-0000-4000-8000-000000000002";
  const binding = {
    codeOwner: "agent-platform",
    evidenceOwner: "codex-harness-engineering",
    scope: "canonical-source-ci-quality",
  };
  const raw = Buffer.from(
    JSON.stringify({
      schema: "native-independent-review-output.v1",
      reviewStatus: "complete",
      binding,
      headSha: head,
      reviewedTreeSha: tree,
      reviewedSourceManifestSha256: manifest,
      reviewer: { runtime: "native-codex", taskId: id },
      author: { runtime: "native-codex", taskId: "actual-native-author" },
      conclusion: "fail",
      unresolvedFindings: 1,
      findings: [{ path: "source.ts", line: 1, severity: "P1", explanation: "synthetic defect" }],
    }),
  );
  const { validateRawReviewArtifact, validateCompletedReviewObservation } =
    await import("../quality/review-artifact");
  const { readFile } = await import("node:fs/promises");
  const oldResult = Bun.spawnSync(
    ["git", "show", "a0678ade528b005496ae9a6ea5f67aa65ffb81ce:ci/reuse/review-pr.ts"],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  );
  expect(oldResult.exitCode).toBe(0);
  for (const [current, source] of [
    [false, oldResult.stdout.toString()],
    [true, await readFile(join(import.meta.dir, "review-pr.ts"), "utf8")],
  ] as const) {
    const events: string[] = [];
    let state = "success";
    const context = {
      sourceRoot: "/synthetic-Git-data",
      policy: {
        repository: "fixture/owner",
        publisher: { id: "42", login: "owner", type: "User" },
        binding,
      },
      author: {
        metadata: { author: { runtime: "native-codex", threadId: "actual-native-author" } },
      },
      row: { projectId: "agent-platform" },
      subject: { base: "c".repeat(40), kind: "owner-pull-request", pr: 1 },
      revalidate: async () => {},
    };
    const empty = () =>
      new ReadableStream<Uint8Array>({
        start(c) {
          c.close();
        },
      });
    const fakeBun = {
      which: () => "/synthetic-native",
      spawn: () => {
        events.push("launch");
        return { stdout: empty(), stderr: empty(), exited: Promise.resolve(0) };
      },
      write: async () => 0,
    };
    const git = (_root: string, args: string[]) =>
      args.includes("ls-tree")
        ? "100644 blob " + "d".repeat(40) + "\tsource.ts\0"
        : args.includes("merge-base")
          ? "c".repeat(40)
          : tree;
    const body = new Bun.Transpiler({ loader: "ts" }).transformSync(
      source
        .slice(
          source.indexOf("export async function reviewPr("),
          source.indexOf("\nif (import.meta.main)"),
        )
        .replace("export async function", "async function"),
    );
    const names = [
      "Bun",
      "process",
      "createTrustedReviewContext",
      "reviewDataGit",
      "deriveReviewedSourceManifest",
      "randomUUID",
      "mkdtemp",
      "join",
      "tmpdir",
      "reviewerInvocation",
      "reviewControllerRoot",
      "writeFile",
      "readFile",
      "bindNativeReviewer",
      "codexReviewThread",
      "validateRawReviewArtifact",
      "validateCompletedReviewObservation",
      "beginNativeReviewAttempt",
      "failNativeReviewAttempt",
      "publishNativeReviewArtifact",
      "console",
      "reviewControllerRepository",
      "invokeNativeReview",
      "readCompletedNativeObservation",
      "nativeReviewSource",
      "smokeNativeReview",
    ];
    const run = new Function(...names, body + "\nreturn reviewPr;")(
      fakeBun,
      { env: {} },
      async () => context,
      git,
      () => ({ sha256: manifest }),
      () => id,
      async () => "/synthetic-output",
      join,
      () => "/tmp",
      () => ["synthetic-native"],
      "/synthetic-controller",
      async () => {},
      async (path: string) =>
        path.endsWith("events.jsonl")
          ? Buffer.from(
              JSON.stringify({ type: "thread.started", thread_id: id }) +
                "\n" +
                JSON.stringify({ type: "turn.completed" }) +
                "\n",
            )
          : raw,
      bindNativeReviewer,
      codexReviewThread,
      validateRawReviewArtifact,
      validateCompletedReviewObservation,
      async () => {
        events.push("pending");
        state = "pending";
        return { id };
      },
      async () => {
        events.push("failure");
        state = "failure";
      },
      async () => {
        events.push("success");
        state = "success";
      },
      { log: () => {} },
      "park-kyungchan/agent-platform",
      async () => {
        events.push("launch");
        return {};
      },
      () => ({ raw: JSON.parse(raw.toString()) }),
      () => ({ tree, manifest: { sha256: manifest } }),
      async () => {},
    ) as (...a: unknown[]) => Promise<void>;
    if (current) {
      await expect(
        run("park-kyungchan/agent-platform", head, "native-codex", head),
      ).rejects.toThrow("distinct");
      expect(events).toEqual([]);
    }
    await expect(run("fixture/owner", head, "native-codex", "e".repeat(40))).rejects.toThrow();
    expect(events).toEqual(current ? ["pending", "launch", "failure"] : ["launch"]);
    expect(state).toBe(current ? "failure" : "success");
  }
});

type NativeCaseApi = {
  requireNativeReviewAttempt: (c: unknown, a: unknown) => void;
  requireCurrentReviewAttempt: (c: unknown, a: unknown) => Promise<unknown[]>;
  beginNativeReviewAttempt: (c: unknown, id: string) => Promise<unknown>;
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
  let posts = 0,
    launches = 0,
    revoked = false;
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
    subject: { base: "c".repeat(40), kind: "owner-pull-request", pr: 1 },
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
      schema: "native-independent-review-output.v1",
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
  ];
  const api = new Function(
    ...names,
    source +
      "\nreturn {beginNativeReviewAttempt,invokeNativeReview,failNativeReviewAttempt,readCompletedNativeObservation,requireNativeReviewAttempt,requireCurrentReviewAttempt};",
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
  ) as NativeCaseApi;
  return {
    api,
    context,
    id,
    states,
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

test("public artifact publisher rejects bare pending and revoked controller before native auth or transport effects", async () => {
  const { readFile } = await import("node:fs/promises"),
    source = await readFile(join(import.meta.dir, "publish-review-artifact.ts"), "utf8");
  const body = new Bun.Transpiler({ loader: "ts" }).transformSync(
    source
      .slice(
        source.indexOf("export async function publishNativeReviewArtifact("),
        source.indexOf("\nif (import.meta.main)"),
      )
      .replace("export async function", "async function"),
  );
  const h = await nativeCase(),
    a = await h.api.beginNativeReviewAttempt(h.context, h.id);
  let io = 0;
  const publish = new Function(
    "Bun",
    "process",
    "requireTrustedReviewContext",
    "requireNativeReviewAttempt",
    "requireCurrentReviewAttempt",
    "readCompletedNativeObservation",
    body + "\nreturn publishNativeReviewArtifact;",
  )(
    {
      spawnSync: () => {
        io++;
        throw Error("UNEXPECTED_NATIVE_AUTH");
      },
    },
    { env: {} },
    () => {},
    h.api.requireNativeReviewAttempt,
    h.api.requireCurrentReviewAttempt,
    h.api.readCompletedNativeObservation,
  ) as (...args: unknown[]) => Promise<void>;
  await expect(publish(h.context, { kind: "completed-native-observation" }, a)).rejects.toThrow(
    "COMPLETED_NATIVE_OBSERVATION_REQUIRED",
  );
  expect(io).toBe(0);
  expect(h.posts).toBe(1);
  const observation = await h.api.invokeNativeReview(h.context, a, "native-codex");
  h.revoke();
  await expect(publish(h.context, observation, a)).rejects.toThrow("QUALIFIED_PIN_HELD");
  expect(io).toBe(0);
  expect(h.posts).toBe(1);
});
