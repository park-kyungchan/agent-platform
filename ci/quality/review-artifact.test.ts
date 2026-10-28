import { expect, test } from "bun:test";
import { ownerScopedReviewGate } from "./receipt-validation-gate";
import { createHash } from "node:crypto";
import {
  authenticatedReviewReader,
  validateReviewArtifact,
  validatePublishedReviewArtifact,
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
      if (path === `commits/${head}/statuses?per_page=100`) return [currentStatus];
      throw Error("unexpected fixture path");
    },
  };
  return {
    head,
    raw,
    run,
    artifact,
    status,
    reader,
    input: { policy: { repository, publisher, binding }, head, status },
    seal,
    hold() {
      currentStatus = { ...status, state: "pending" };
    },
    corrupt() {
      bytes[40] ^= 1;
    },
  };
}

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
