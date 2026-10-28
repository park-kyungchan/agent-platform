import { mkdtemp, mkdir, readFile, writeFile, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import policyInput from "./trusted-review-policy.json";
import { authenticatedReviewReader, type ReviewArtifactPolicy } from "../quality/review-artifact";
import { verifyCommittedSourceAuthor } from "./source-author";

const trustedPolicy = structuredClone(policyInput);
export const reviewControllerRepository = trustedPolicy.controllerRepository;
export const reviewControllerRoot = resolve(import.meta.dir, "../..");
export type TrustedRepository = (typeof trustedPolicy.repositories)[number];
type RecordValue = Record<string, unknown>;
const record = (v: unknown): RecordValue =>
  v && typeof v === "object" && !Array.isArray(v) ? (v as RecordValue) : {};
const oid = (v: unknown): v is string => typeof v === "string" && /^[a-f0-9]{40}$/.test(v);
function need(v: unknown, code: string): asserts v {
  if (!v) throw Error("TRUSTED_NATIVE_REVIEW_" + code);
}
export function reviewRepository(name: string): TrustedRepository {
  need(
    trustedPolicy.schema === "agent-platform.trusted-native-review-policy.v1" &&
      trustedPolicy.mode === "private-owner-run" &&
      trustedPolicy.untrustedContributors === "refuse" &&
      trustedPolicy.credentialIsolationEstablished === false,
    "FIXED_OPERATING_PROFILE_REQUIRED",
  );
  const row = trustedPolicy.repositories.find((r) => r.repository === name);
  need(row, "PINNED_REPOSITORY_REQUIRED");
  return structuredClone(row);
}
export function requireOwnerIdentity(value: unknown): void {
  const actor = record(value),
    expected = trustedPolicy.publisher;
  need(
    String(actor.id) === expected.id &&
      actor.login === expected.login &&
      actor.type === expected.type,
    "AUTHENTICATED_OWNER_REQUIRED",
  );
}
export function requirePrivateRepository(value: unknown, row: TrustedRepository): void {
  const repo = record(value);
  need(
    repo.id === row.id &&
      repo.full_name === row.repository &&
      repo.private === true &&
      String(record(repo.owner).id) === trustedPolicy.publisher.id,
    "PINNED_PRIVATE_OWNER_REPOSITORY_REQUIRED",
  );
}
export type AdmittedSubject = {
  kind: "owner-pull-request" | "protected-main";
  head: string;
  base: string;
  pr: number;
  headRef: string;
};
/** Only authenticated GitHub objects enter here; Git author names and caller booleans never admit a subject. */
export function admitTrustedSubject(
  row: TrustedRepository,
  head: string,
  repo: unknown,
  pulls: unknown,
  main: unknown,
  commit: unknown,
): AdmittedSubject {
  requirePrivateRepository(repo, row);
  need(oid(head), "EXACT_HEAD_REQUIRED");
  need(Array.isArray(pulls) && pulls.length < 100, "BOUNDED_PULL_PROVENANCE_REQUIRED");
  const eligible = pulls.map(record).filter((p) => {
    const h = record(p.head),
      b = record(p.base),
      user = record(p.user);
    return (
      p.state === "open" &&
      h.sha === head &&
      record(h.repo).id === row.id &&
      record(b.repo).id === row.id &&
      record(h.repo).private === true &&
      String(record(record(h.repo).owner).id) === trustedPolicy.publisher.id &&
      String(user.id) === trustedPolicy.publisher.id &&
      user.type === "User" &&
      typeof b.ref === "string" &&
      row.bases.includes(b.ref) &&
      oid(b.sha) &&
      typeof h.ref === "string"
    );
  });
  need(eligible.length <= 1, "UNAMBIGUOUS_OWNER_PULL_REQUIRED");
  if (eligible.length === 1) {
    const p = eligible[0]!,
      h = record(p.head),
      b = record(p.base);
    need(Number.isSafeInteger(p.number) && Number(p.number) > 0, "OWNER_PULL_NUMBER_REQUIRED");
    return {
      kind: "owner-pull-request",
      head,
      base: String(b.sha),
      pr: Number(p.number),
      headRef: String(h.ref),
    };
  }
  const branch = record(main),
    c = record(commit),
    parents = c.parents;
  need(
    branch.name === "main" &&
      branch.protected === true &&
      record(branch.commit).sha === head &&
      c.sha === head &&
      Array.isArray(parents) &&
      parents.length > 0 &&
      oid(record(parents[0]).sha),
    "OWNER_PULL_OR_EXACT_PROTECTED_MAIN_REQUIRED",
  );
  return {
    kind: "protected-main",
    head,
    base: String(record(parents[0]).sha),
    pr: 0,
    headRef: "main",
  };
}
function nativeGh(args: string[]): Buffer {
  const r = Bun.spawnSync(["gh", ...args], { stdin: "ignore", stdout: "pipe", stderr: "pipe" });
  need(r.exitCode === 0, "NATIVE_AUTH_READ_REQUIRED");
  return Buffer.from(r.stdout);
}
function gitEnvironment(extra: Record<string, string> = {}): Record<string, string> {
  return {
    PATH: process.env.PATH ?? "/usr/bin:/bin",
    LANG: "C.UTF-8",
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_NO_REPLACE_OBJECTS: "1",
    GIT_TERMINAL_PROMPT: "0",
    GIT_OPTIONAL_LOCKS: "0",
    ...extra,
  };
}
/** Fixed local data commands; no inherited repository hooks, filters, config environment or replace refs. */
export function reviewDataGit(
  root: string,
  args: string[],
  extra: Record<string, string> = {},
): string {
  const r = Bun.spawnSync(
    [
      "git",
      "--no-replace-objects",
      "-c",
      "core.hooksPath=/dev/null",
      "-c",
      "core.fsmonitor=false",
      "-c",
      "core.attributesFile=/dev/null",
      "-C",
      root,
      ...args,
    ],
    { env: gitEnvironment(extra), stdin: "ignore", stdout: "pipe", stderr: "pipe" },
  );
  need(r.exitCode === 0 && r.stdout.length <= 8_000_000, "GIT_DATA_READ_REQUIRED");
  return r.stdout.toString().trimEnd();
}
export async function requireControllerCheckout(expectedSha: string): Promise<void> {
  need(
    oid(expectedSha) && (await realpath(process.cwd())) === reviewControllerRoot,
    "SEPARATE_PINNED_CONTROLLER_CWD_REQUIRED",
  );
  need(
    reviewDataGit(reviewControllerRoot, ["rev-parse", "HEAD"]) === expectedSha &&
      reviewDataGit(reviewControllerRoot, ["rev-parse", "--abbrev-ref", "HEAD"]) === "HEAD" &&
      reviewDataGit(reviewControllerRoot, ["status", "--porcelain=v1", "--untracked-files=all"]) ===
        "",
    "IMMUTABLE_DETACHED_CONTROLLER_REQUIRED",
  );
}
export async function requireControllerQualification(
  expectedSha: string,
  token: string,
): Promise<void> {
  const reader = authenticatedReviewReader(trustedPolicy.controllerRepository, token),
    repo = await reader.json(".");
  requirePrivateRepository(repo, reviewRepository(trustedPolicy.controllerRepository));
  const commit = record(await reader.json("git/commits/" + expectedSha));
  need(
    commit.sha === expectedSha &&
      record(commit.tree).sha === reviewDataGit(reviewControllerRoot, ["rev-parse", "HEAD^{tree}"]),
    "CONTROLLER_SOURCE_TREE_REQUIRED",
  );
  const { reviewGate } = await import("../source-check");
  await reviewGate({
    env: {
      GITHUB_REPOSITORY: trustedPolicy.controllerRepository,
      REVIEW_HEAD_SHA: expectedSha,
      GITHUB_TOKEN: "authenticated-controller-read-callback",
      NATIVE_REVIEW_STATUS_CREATOR_ID: trustedPolicy.publisher.id,
      NATIVE_REVIEW_STATUS_CREATOR_LOGIN: trustedPolicy.publisher.login,
    },
    readApi: reader.json,
    readArchive: reader.archive,
  });
  const pulls = await reader.json(`commits/${expectedSha}/pulls?per_page=100`);
  need(Array.isArray(pulls) && pulls.length < 100, "CONTROLLER_PULL_INVENTORY_REQUIRED");
  for (const pull of pulls.map(record)) {
    need(
      Number.isSafeInteger(pull.number) && Number(pull.number) > 0,
      "CONTROLLER_PULL_NUMBER_REQUIRED",
    );
    const response = await fetch("https://api.github.com/graphql", {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(30_000),
      headers: {
        Authorization: "Bearer " + token,
        "Content-Type": "application/json",
        "X-GitHub-Api-Version": "2026-03-10",
      },
      body: JSON.stringify({
        query:
          'query($number:Int!){repository(owner:"park-kyungchan",name:"agent-platform"){pullRequest(number:$number){reviewThreads(first:100){nodes{isResolved}pageInfo{hasNextPage}}}}}',
        variables: { number: pull.number },
      }),
    });
    need(response.ok, "CONTROLLER_THREAD_READ_REQUIRED");
    const data = record(await response.json()),
      threads = record(record(record(record(data.data).repository).pullRequest).reviewThreads);
    need(
      !data.errors &&
        Array.isArray(threads.nodes) &&
        record(threads.pageInfo).hasNextPage === false &&
        threads.nodes.every((t) => record(t).isResolved === true),
      "CONTROLLER_UNRESOLVED_THREADS_REFUSED",
    );
  }
}
export type TrustedReviewContext = {
  row: TrustedRepository;
  head: string;
  sourceRoot: string;
  controllerSha: string;
  subject: AdmittedSubject;
  policy: ReviewArtifactPolicy;
  author: Awaited<ReturnType<typeof verifyCommittedSourceAuthor>>;
  readApi: (path: string) => Promise<unknown>;
  archive: (id: number) => Promise<Uint8Array>;
  revalidate: () => Promise<void>;
};
const contexts = new WeakSet<object>();
export function requireTrustedReviewContext(value: TrustedReviewContext): void {
  need(contexts.has(value), "AUTHENTICATED_CONTEXT_REQUIRED");
}
export function validateCommittedOwnerPolicy(row: TrustedRepository, value: unknown): void {
  const provider = record(value),
    owner = record(provider.policy),
    expected = nativeArtifactPolicy(row),
    publisher = record(owner.publisher);
  need(
    provider.codeOwner === "agent-platform" &&
      provider.generatedOnly === true &&
      provider.sourceOnly === true &&
      provider.runtimeAuthority === false &&
      provider.nativePermissionGranted === false,
    "FIXED_COMMITTED_OWNER_MANIFEST_REQUIRED",
  );
  need(
    Object.entries(expected.binding).every(([key, v]) => owner[key] === v) &&
      owner.statusContext === "native-independent-review" &&
      publisher.id === expected.publisher.id &&
      publisher.login === expected.publisher.login &&
      publisher.type === expected.publisher.type &&
      Object.keys(publisher).sort().join("|") === "id|login|type",
    "PINNED_COMMITTED_OWNER_POLICY_REQUIRED",
  );
}
function nativeArtifactPolicy(row: TrustedRepository): ReviewArtifactPolicy {
  return {
    repository: row.repository,
    publisher: { ...trustedPolicy.publisher, type: "User" },
    binding:
      row.projectId === "agent-platform"
        ? { codeOwner: "agent-platform", evidenceOwner: row.semanticOwner, scope: row.scope }
        : {
            repository: row.repository,
            projectId: row.projectId,
            domainId: row.domainId,
            semanticOwner: row.semanticOwner,
            scope: row.scope,
          },
  };
}
/** The subject is fetched as Git objects into a fresh data repository; no subject checkout/config/hooks are executed. */
export async function createTrustedReviewContext(
  controllerSha: string,
  repository: string,
  head: string,
): Promise<TrustedReviewContext> {
  need(!process.env.CI && !process.env.GITHUB_ACTIONS, "NATIVE_OWNER_CONTEXT_REQUIRED");
  await requireControllerCheckout(controllerSha);
  const row = reviewRepository(repository);
  const identity = JSON.parse(nativeGh(["api", "user"]).toString());
  requireOwnerIdentity(identity);
  const token = nativeGh(["auth", "token"]).toString().trim();
  await requireControllerQualification(controllerSha, token);
  const reader = authenticatedReviewReader(repository, token);
  async function observe() {
    const repo = await reader.json("."),
      pulls = await reader.json(`commits/${head}/pulls?per_page=100`),
      main = await reader.json("branches/main"),
      commit = await reader.json("git/commits/" + head);
    return admitTrustedSubject(row, head, repo, pulls, main, commit);
  }
  const subject = await observe(),
    sourceRoot = await mkdtemp(join(tmpdir(), "trusted-review-data-"));
  reviewDataGit(sourceRoot, ["init", "--quiet", "--template="]);
  const gh = Bun.which("gh");
  need(gh && /^\/[A-Za-z0-9_./-]+$/.test(gh), "NATIVE_GH_EXECUTABLE_REQUIRED");
  // Existing official gh owns authentication. This per-command helper stores no credential/config.
  reviewDataGit(
    sourceRoot,
    [
      "-c",
      "credential.helper=",
      "-c",
      `credential.helper=!${gh} auth git-credential`,
      "-c",
      "protocol.allow=never",
      "-c",
      "protocol.https.allow=always",
      "fetch",
      "--recurse-submodules=no",
      "--quiet",
      "--no-tags",
      "--no-write-fetch-head",
      `https://github.com/${repository}.git`,
      head,
      subject.base,
    ],
    { GH_TOKEN: token, GH_HOST: "github.com" },
  );
  reviewDataGit(sourceRoot, ["update-ref", "HEAD", head]);
  reviewDataGit(sourceRoot, ["read-tree", head]);
  await mkdir(join(sourceRoot, ".github"));
  await writeFile(
    join(sourceRoot, ".github/source-author.json"),
    reviewDataGit(sourceRoot, ["show", head + ":.github/source-author.json"]) + "\n",
    { mode: 0o600 },
  );
  const author = await verifyCommittedSourceAuthor({
    workspace: sourceRoot,
    repository,
    projectId: row.projectId,
    sourceHead: head,
  });
  need(author.metadata.sourceOwner.domainId === row.domainId, "PINNED_SUBJECT_DOMAIN_REQUIRED");
  if (row.projectId !== "agent-platform") {
    const manifests: unknown[] = [];
    for (const path of ["architecture/common-ci-provider.json", "ci/provider.json"]) {
      try {
        manifests.push(JSON.parse(reviewDataGit(sourceRoot, ["show", head + ":" + path])));
      } catch {}
    }
    need(manifests.length === 1, "FIXED_COMMITTED_OWNER_MANIFEST_REQUIRED");
    validateCommittedOwnerPolicy(row, manifests[0]);
  }
  const policy = nativeArtifactPolicy(row);
  Object.freeze(row.bases);
  Object.freeze(author.metadata.author);
  Object.freeze(author.metadata.sourceOwner);
  Object.freeze(author.metadata.logicalWork);
  Object.freeze(author.metadata);
  Object.freeze(author);
  Object.freeze(policy.binding);
  Object.freeze(policy.publisher);
  Object.freeze(policy);
  const context: TrustedReviewContext = {
    row: Object.freeze(row),
    head,
    sourceRoot,
    controllerSha,
    subject: Object.freeze(subject),
    policy,
    author,
    readApi: reader.json,
    archive: reader.archive,
    revalidate: async () => {
      await requireControllerCheckout(controllerSha);
      await requireControllerQualification(controllerSha, token);
      need(
        JSON.stringify(await observe()) === JSON.stringify(subject),
        "AUTHENTICATED_SUBJECT_CHANGED",
      );
      need(
        reviewDataGit(sourceRoot, ["rev-parse", "HEAD"]) === head &&
          reviewDataGit(sourceRoot, ["rev-parse", "HEAD^{tree}"]) === author.sourceTree,
        "SUBJECT_GIT_DATA_CHANGED",
      );
    },
  };
  contexts.add(context);
  await context.revalidate();
  return Object.freeze(context);
}
