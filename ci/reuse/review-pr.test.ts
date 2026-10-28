import {
  admitTrustedSubject,
  reviewRepository,
  requireOwnerIdentity,
  requireTrustedReviewContext,
  type TrustedReviewContext,
  validateCommittedOwnerPolicy,
} from "./trusted-review-source";
import { codexReviewThread, bindNativeReviewer } from "./review-runtimes";
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
      reviewer: { runtime: "native-claude", taskId: id },
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
      async () => raw,
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
    ) as (...a: unknown[]) => Promise<void>;
    if (current) {
      await expect(run("fixture/owner", head, "native-claude", head)).rejects.toThrow("distinct");
      expect(events).toEqual([]);
    }
    await expect(run("fixture/owner", head, "native-claude", "e".repeat(40))).rejects.toThrow();
    expect(events).toEqual(current ? ["pending", "launch", "failure"] : ["launch"]);
    expect(state).toBe(current ? "failure" : "success");
  }
});

test("native attempt writes authenticate, hold before review, publish bound negatives and never retry uncertain writes", async () => {
  const { readFile } = await import("node:fs/promises");
  const { reviewAttemptTarget, validateCompletedReviewObservation } =
    await import("../quality/review-artifact");
  const source = new Bun.Transpiler({ loader: "ts" })
    .transformSync(await readFile(join(import.meta.dir, "review-attempt.ts"), "utf8"))
    .replace(/^import[\s\S]*?;\n/gm, "")
    .replaceAll("export ", "");
  for (const scenario of [
    "normal",
    "negative-after-other-pass",
    "uncertain",
    "wrong-owner",
    "wrong-source",
  ]) {
    const head = "a".repeat(40),
      tree = "b".repeat(40),
      manifest = "f".repeat(64),
      id = "00000000-0000-4000-8000-000000000002";
    const actor = { id: 42, login: "native-owner", type: "User" };
    let posts = 0;
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
      policy: {
        repository: "fixture/owner",
        publisher: { ...actor, id: "42" },
        binding: {
          codeOwner: "agent-platform",
          evidenceOwner: "codex-harness-engineering",
          scope: "canonical-source-ci-quality",
        },
      },
      author: { metadata: { author: { runtime: "native-codex", threadId: "actual-author" } } },
      revalidate: async () => {},
      readApi: async () => states,
    };
    const nativeBun = {
      spawnSync: (args: string[]) => ({
        exitCode: 0,
        stdout: Buffer.from(
          args.includes("user")
            ? JSON.stringify({ ...actor, id: scenario === "wrong-owner" ? 99 : 42 })
            : "fixture-token-not-a-credential",
        ),
      }),
    };
    const nativeFetch = async (_url: string, options: RequestInit) => {
      posts++;
      const value = { ...JSON.parse(String(options.body)), id: 100 + posts, creator: actor };
      states.unshift(value);
      return new Response(JSON.stringify(value), { status: scenario === "uncertain" ? 503 : 201 });
    };
    const api = new Function(
      "Bun",
      "fetch",
      "requireTrustedReviewContext",
      "reviewAttemptTarget",
      "validateCompletedReviewObservation",
      "console",
      source +
        "\nreturn {beginNativeReviewAttempt,failNativeReviewAttempt,requireCurrentReviewAttempt};",
    )(nativeBun, nativeFetch, () => {}, reviewAttemptTarget, validateCompletedReviewObservation, {
      log: () => {},
    }) as {
      beginNativeReviewAttempt: (c: unknown, id: string) => Promise<unknown>;
      failNativeReviewAttempt: (a: unknown, b: Uint8Array, t: string, m: string) => Promise<void>;
    };
    if (scenario === "uncertain" || scenario === "wrong-owner") {
      await expect(api.beginNativeReviewAttempt(context, id)).rejects.toThrow(
        scenario === "uncertain" ? "OUTCOME_UNCERTAIN" : "AUTHENTICATED_OWNER_REQUIRED",
      );
      expect(posts).toBe(scenario === "uncertain" ? 1 : 0);
      continue;
    }
    const attempt = await api.beginNativeReviewAttempt(context, id);
    expect(states[0]!.state).toBe("pending");
    expect(posts).toBe(1);
    const fail = Buffer.from(
      JSON.stringify({
        schema: "native-independent-review-output.v1",
        reviewStatus: "complete",
        binding: context.policy.binding,
        headSha: scenario === "wrong-source" ? "e".repeat(40) : head,
        reviewedTreeSha: tree,
        reviewedSourceManifestSha256: manifest,
        reviewer: { runtime: "native-codex", taskId: "independent-reviewer" },
        author: { runtime: "native-codex", taskId: "actual-author" },
        conclusion: "fail",
        unresolvedFindings: 1,
        findings: [{ path: "source.ts", line: 1, severity: "P1", explanation: "synthetic defect" }],
      }),
    );
    if (scenario === "negative-after-other-pass")
      states.unshift({ ...states[0], id: 999, state: "success" });
    if (scenario === "wrong-source") {
      await expect(api.failNativeReviewAttempt(attempt, fail, tree, manifest)).rejects.toThrow();
      expect(posts).toBe(1);
      expect(states[0]!.state).toBe("pending");
    } else {
      await api.failNativeReviewAttempt(attempt, fail, tree, manifest);
      expect(posts).toBe(2);
      expect(states[0]!.state).toBe("failure");
      await expect(api.failNativeReviewAttempt(attempt, fail, tree, manifest)).rejects.toThrow(
        "AUTHENTICATED_ATTEMPT_REQUIRED",
      );
      expect(posts).toBe(2);
    }
  }
});
