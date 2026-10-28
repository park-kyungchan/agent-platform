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
import { isAbsolute, join, normalize } from "node:path";

test("merge correspondence selects its closed eligibility guard before any native attempt", async () => {
  const { readFile } = await import("node:fs/promises");
  const text = await readFile(join(import.meta.dir, "review-pr.ts"), "utf8");
  const body = new Bun.Transpiler({ loader: "ts" }).transformSync(
    text
      .slice(
        text.indexOf("export async function reviewPr("),
        text.indexOf("\nif (import.meta.main)"),
      )
      .replace("export async function", "async function"),
  );
  const effects: string[] = [];
  const run = new Function(
    "process",
    "reviewControllerRepository",
    "createTrustedReviewContext",
    "prepareMergeCorrespondenceAssignment",
    "beginNativeReviewAttempt",
    "randomUUID",
    "invokeNativeReview",
    "readCompletedNativeObservation",
    "failNativeReviewAttempt",
    "publishNativeReviewArtifact",
    body + "\nreturn reviewPr;",
  )(
    { env: {} },
    "fixture/controller",
    async () => {
      effects.push("context");
      return {};
    },
    async () => {
      effects.push("eligibility");
      throw Error("CLOSED_MERGE_ELIGIBILITY_REQUIRED");
    },
    async () => {
      effects.push("pending");
      return {};
    },
    () => "00000000-0000-4000-8000-000000000001",
    async () => {
      effects.push("native");
      return {};
    },
    () => ({ raw: { conclusion: "pass" } }),
    async () => {
      effects.push("failure");
    },
    async () => {
      effects.push("publish");
    },
  ) as (...args: unknown[]) => Promise<void>;
  await expect(
    run("fixture/owner", "a".repeat(40), "native-codex", "b".repeat(40), false, false, true),
  ).rejects.toThrow("CLOSED_MERGE_ELIGIBILITY_REQUIRED");
  expect(effects).toEqual(["context", "eligibility"]);
});

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
  beginNativeReviewAttempt: (c: unknown, id: string, assignment?: unknown) => Promise<unknown>;
  prepareMergeCorrespondenceAssignment: (c: unknown) => Promise<unknown>;
  invokeNativeReview: (c: unknown, a: unknown, r: string, promptFile?: unknown) => Promise<unknown>;
  smokeNativeReview: (c: unknown, r: string, promptFile?: unknown) => Promise<void>;
  failNativeReviewAttempt: (a: unknown, o: unknown) => Promise<void>;
  readCompletedNativeObservation: (
    c: unknown,
    a: unknown,
    o: unknown,
  ) => {
    bytes: Buffer;
    raw: Record<string, unknown>;
    prompt?: Readonly<{ path: string; sha256: string; bytes: number }>;
  };
};
async function nativeCase(scenario = "pass", predecessor: boolean | string = false) {
  const { readFile, open, realpath } = await import("node:fs/promises"),
    { constants: fsConstants } = await import("node:fs"),
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
        [
          "git",
          "show",
          (typeof predecessor === "string"
            ? predecessor
            : "8f78f54111f59e3af2bf702826eeefa761d2568d") + ":ci/reuse/review-attempt.ts",
        ],
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
    revoked = false,
    promptReads = 0,
    promptOpens = 0,
    afterPromptClose: (() => Promise<void>) | undefined;
  const childInputs: Buffer[] = [],
    writes = new Map<string, Buffer>(),
    logs: unknown[] = [];
  const trackedOpen = async (...args: Parameters<typeof open>) => {
    promptOpens++;
    const file = await open(...args);
    return new Proxy(file, {
      get(target, property) {
        if (property === "read")
          return async (...readArgs: unknown[]) => {
            promptReads++;
            return Reflect.apply(target.read, target, readArgs);
          };
        if (property === "close")
          return async () => {
            await target.close();
            await afterPromptClose?.();
          };
        const value: unknown = Reflect.get(target, property, target);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
  };
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
      schema:
        scenario === "smoke"
          ? "native-review-smoke.v1"
          : scenario.startsWith("merge-")
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
      ...(scenario === "smoke"
        ? {}
        : { nativeReviewPendingStatusId: scenario === "wrong-pending" ? 999 : 101 }),
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
    spawn: (_args: string[], options: { stdin: string | Uint8Array }) => {
      launches++;
      childInputs.push(Buffer.from(options.stdin));
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
    "open",
    "realpath",
    "fsConstants",
    "isAbsolute",
    "normalize",
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
      "\nreturn {beginNativeReviewAttempt,invokeNativeReview,smokeNativeReview,failNativeReviewAttempt,readCompletedNativeObservation,requireNativeReviewAttempt,requireCurrentReviewAttempt,...(typeof prepareMergeCorrespondenceAssignment === 'function' ? {prepareMergeCorrespondenceAssignment} : {})};",
  )(
    nativeBun,
    nativeFetch,
    (c: unknown) => {
      if (c !== context) throw Error("FOREIGN_CONTEXT");
    },
    reviewAttemptTarget,
    validateCompletedReviewObservation,
    { log: (value: unknown) => logs.push(value) },
    () => id,
    createHash,
    async () => "/synthetic-native-output",
    async (path: string) => (path.endsWith("events.jsonl") ? eventsFile : bytes),
    async (path: string, data: string | Uint8Array) => {
      writes.set(path, Buffer.from(data));
    },
    trackedOpen,
    realpath,
    fsConstants,
    isAbsolute,
    normalize,
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
    childInputs,
    writes,
    logs,
    get promptReads() {
      return promptReads;
    },
    get promptOpens() {
      return promptOpens;
    },
    afterPromptClose: (callback: () => Promise<void>) => {
      afterPromptClose = callback;
    },
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

const fullPromptPredecessor = process.env.FULL_PROMPT_PREDECESSOR;
if (fullPromptPredecessor && fullPromptPredecessor !== "90784712250c1e27bf255d11ba660deeb595131b")
  throw Error("FULL_PROMPT_PREDECESSOR_EXACT_907_REQUIRED");
const fullPromptBytes = Buffer.from(
  "Independent FULL source review fixture.\nPreserve every byte: π.\n",
);
async function withFullPrompt(
  run: (directory: string, path: string) => Promise<void>,
  bytes: Uint8Array = fullPromptBytes,
) {
  const { mkdtemp, writeFile, rm } = await import("node:fs/promises"),
    { tmpdir } = await import("node:os");
  const directory = await mkdtemp(join(tmpdir(), "full-prompt-input-")),
    path = join(directory, "prompt.md");
  await writeFile(path, bytes);
  try {
    await run(directory, path);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test("FULL-PROMPT-INPUT snapshots one read into actual stdin, persisted bytes and immutable metadata", async () => {
  const { writeFile, readFile } = await import("node:fs/promises"),
    { createHash } = await import("node:crypto");
  await withFullPrompt(async (_directory, path) => {
    const h = await nativeCase("pass", fullPromptPredecessor ?? false),
      attempt = await h.api.beginNativeReviewAttempt(h.context, h.id);
    h.afterPromptClose(async () => {
      await writeFile(path, "replacement after immutable snapshot");
    });
    const observation = await h.api.invokeNativeReview(h.context, attempt, "native-codex", path),
      result = h.api.readCompletedNativeObservation(h.context, attempt, observation),
      expectedHash = createHash("sha256").update(fullPromptBytes).digest("hex");
    expect(h.childInputs).toEqual([fullPromptBytes]);
    expect(h.promptReads).toBe(1);
    expect(h.promptOpens).toBe(1);
    expect(await readFile(path, "utf8")).toBe("replacement after immutable snapshot");
    expect(result.prompt).toEqual({
      path: "/synthetic-native-output/prompt.txt",
      sha256: expectedHash,
      bytes: fullPromptBytes.length,
    });
    expect(h.writes.get(result.prompt!.path)).toEqual(fullPromptBytes);
    expect(Object.isFrozen(result.prompt)).toBe(true);
    expect(Reflect.set(result.prompt!, "sha256", "0".repeat(64))).toBe(false);
    expect(
      h.api.readCompletedNativeObservation(h.context, attempt, observation).prompt?.sha256,
    ).toBe(expectedHash);
    expect(h.posts).toBe(1);
    expect(h.launches).toBe(1);
  });
});

test("FULL-PROMPT-INPUT metadata belongs to the actual opaque observation", async () => {
  await withFullPrompt(async (_directory, path) => {
    const h = await nativeCase("pass", fullPromptPredecessor ?? false),
      attempt = await h.api.beginNativeReviewAttempt(h.context, h.id),
      observation = await h.api.invokeNativeReview(h.context, attempt, "native-codex", path),
      result = h.api.readCompletedNativeObservation(h.context, attempt, observation);
    expect(result.prompt?.bytes).toBe(fullPromptBytes.length);
    expect(() =>
      h.api.readCompletedNativeObservation(h.context, attempt, {
        ...(observation as object),
        prompt: result.prompt,
      }),
    ).toThrow("COMPLETED_NATIVE_OBSERVATION_REQUIRED");
    expect(() =>
      h.api.readCompletedNativeObservation({ ...h.context }, attempt, observation),
    ).toThrow("FOREIGN_CONTEXT");
  });
});

for (const kind of [
  "missing",
  "symlink",
  "parent-symlink",
  "relative",
  "traversal",
  "nul-path",
  "directory",
  "oversized",
  "empty",
  "whitespace",
  "invalid-utf8",
  "nul-bytes",
  "wrong-type",
]) {
  test("FULL-PROMPT-INPUT refuses " + kind + " before native launch", async () => {
    const { writeFile, symlink, mkdir } = await import("node:fs/promises");
    await withFullPrompt(async (directory, path) => {
      let supplied: unknown = path;
      if (kind === "missing") supplied = join(directory, "missing.md");
      if (kind === "symlink") {
        supplied = join(directory, "linked.md");
        await symlink(path, String(supplied));
      }
      if (kind === "parent-symlink") {
        const realDirectory = join(directory, "real"),
          linkedDirectory = join(directory, "linked");
        await mkdir(realDirectory);
        await writeFile(join(realDirectory, "prompt.md"), fullPromptBytes);
        await symlink(realDirectory, linkedDirectory, "dir");
        supplied = join(linkedDirectory, "prompt.md");
      }
      if (kind === "relative") supplied = "prompt.md";
      if (kind === "traversal")
        supplied = directory + "/../" + directory.split("/").at(-1) + "/prompt.md";
      if (kind === "nul-path") supplied = path + "\0";
      if (kind === "directory") supplied = directory;
      if (kind === "oversized") await writeFile(path, Buffer.alloc(45001, 65));
      if (kind === "empty") await writeFile(path, "");
      if (kind === "whitespace") await writeFile(path, " \t\n");
      if (kind === "invalid-utf8") await writeFile(path, Buffer.from([0xc3, 0x28]));
      if (kind === "nul-bytes") await writeFile(path, Buffer.from("source\0prompt"));
      if (kind === "wrong-type") supplied = 42;
      const h = await nativeCase("pass", fullPromptPredecessor ?? false),
        attempt = await h.api.beginNativeReviewAttempt(h.context, h.id);
      await expect(
        h.api.invokeNativeReview(h.context, attempt, "native-codex", supplied),
      ).rejects.toThrow("EXTERNAL_PROMPT");
      expect(h.launches).toBe(0);
      expect(h.posts).toBe(1);
    });
  });
}

test("FULL-PROMPT-INPUT rejects bounded invocation without reading the file", async () => {
  await withFullPrompt(async (_directory, path) => {
    const h = await nativeCase("merge-pass", fullPromptPredecessor ?? false),
      assignment = await h.api.prepareMergeCorrespondenceAssignment(h.context),
      attempt = await h.api.beginNativeReviewAttempt(h.context, h.id, assignment);
    await expect(
      h.api.invokeNativeReview(h.context, attempt, "native-codex", path),
    ).rejects.toThrow("EXTERNAL_PROMPT");
    expect(h.promptOpens).toBe(0);
    expect(h.launches).toBe(0);
    expect(h.posts).toBe(1);
  });
});

test("FULL-PROMPT-INPUT rejects smoke invocation without reading the file", async () => {
  await withFullPrompt(async (_directory, path) => {
    const h = await nativeCase("smoke", fullPromptPredecessor ?? false);
    await expect(h.api.smokeNativeReview(h.context, "native-codex", path)).rejects.toThrow(
      "EXTERNAL_PROMPT",
    );
    expect(h.promptOpens).toBe(0);
    expect(h.launches).toBe(0);
    expect(h.posts).toBe(0);
  });
});

for (const kind of ["pending-held", "controller-held"]) {
  test("FULL-PROMPT-INPUT rechecks " + kind + " after consuming the file", async () => {
    await withFullPrompt(async (_directory, path) => {
      const h = await nativeCase("pass", fullPromptPredecessor ?? false),
        attempt = await h.api.beginNativeReviewAttempt(h.context, h.id);
      h.afterPromptClose(async () => {
        if (kind === "controller-held") h.revoke();
        else h.states.unshift({ ...h.states[0], id: 102, state: "pending" });
      });
      await expect(
        h.api.invokeNativeReview(h.context, attempt, "native-codex", path),
      ).rejects.toThrow();
      expect(h.promptReads).toBe(1);
      expect(h.launches).toBe(0);
      expect(h.posts).toBe(1);
    });
  });
}

for (const scenario of ["wrong-source", "wrong-pending"]) {
  test("FULL-PROMPT-INPUT retains actual native proof rejection for " + scenario, async () => {
    await withFullPrompt(async (_directory, path) => {
      const h = await nativeCase(scenario, fullPromptPredecessor ?? false),
        attempt = await h.api.beginNativeReviewAttempt(h.context, h.id);
      await expect(
        h.api.invokeNativeReview(h.context, attempt, "native-codex", path),
      ).rejects.toThrow();
      expect(h.childInputs).toEqual([fullPromptBytes]);
      expect(h.posts).toBe(1);
    });
  });
}

test("FULL-PROMPT-INPUT authenticates opaque attempts before opening a prompt file", async () => {
  await withFullPrompt(async (_directory, path) => {
    const h = await nativeCase("pass", fullPromptPredecessor ?? false),
      attempt = await h.api.beginNativeReviewAttempt(h.context, h.id);
    await expect(
      h.api.invokeNativeReview(h.context, { ...(attempt as object) }, "native-codex", path),
    ).rejects.toThrow("AUTHENTICATED_ATTEMPT_REQUIRED");
    expect(h.promptOpens).toBe(0);
    expect(h.launches).toBe(0);
  });
});

test("FULL-PROMPT-INPUT keeps default FULL stdin and observation shape unchanged", async () => {
  const h = await nativeCase("pass", fullPromptPredecessor ?? false),
    attempt = await h.api.beginNativeReviewAttempt(h.context, h.id),
    observation = await h.api.invokeNativeReview(h.context, attempt, "native-codex");
  expect(h.childInputs[0]!.toString()).toStartWith(
    "You are a fresh independent native source reviewer.",
  );
  expect(
    h.api.readCompletedNativeObservation(h.context, attempt, observation).prompt,
  ).toBeUndefined();
  expect(h.promptReads).toBe(0);
  expect(h.launches).toBe(1);
});

test("FULL-PROMPT-INPUT keeps default smoke transport unchanged", async () => {
  const h = await nativeCase("smoke", fullPromptPredecessor ?? false);
  await h.api.smokeNativeReview(h.context, "native-codex");
  expect(h.childInputs[0]!.toString()).toStartWith("Mechanical transport smoke only.");
  expect(h.promptReads).toBe(0);
  expect(h.posts).toBe(0);
  expect(h.launches).toBe(1);
});

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
