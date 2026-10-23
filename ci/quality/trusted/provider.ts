import { operationalTimeouts } from "../operational-timeouts";
import {
  insist,
  object,
  positive,
  repository,
  repositoryId,
  workflowPath,
  jobName,
  requiredSteps,
  runTitle,
  sha256,
  oid,
  type ProviderRun,
  type ProviderJob,
  type TrustedPolicy,
} from "./policy";
import { readReportArchive } from "./artifact";

export interface ProviderReader {
  json(path: string): Promise<unknown>;
  archive(artifactId: number): Promise<Uint8Array>;
}
export async function boundedBody(response: Response, maximum: number): Promise<Buffer> {
  insist(response.body, "PROVIDER_BODY_REQUIRED");
  const reader = response.body.getReader(),
    chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      size += next.value.length;
      insist(size <= maximum, "PROVIDER_BODY_LIMIT");
      chunks.push(next.value);
    }
    return Buffer.concat(chunks);
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
}
/** Credential remains in this closure. No URL or bearer is returned in evidence. */
export function githubReader(token: string): ProviderReader {
  async function response(path: string): Promise<Response> {
    insist(
      path.startsWith("/") &&
        !/[\r\n\0\\]/.test(path) &&
        !decodeURIComponent(path).split(/[/?]/).includes(".."),
      "FIXED_PROVIDER_PATH_REQUIRED",
    );
    const url = new URL("https://api.github.com" + path);
    insist(url.origin === "https://api.github.com", "FIXED_PROVIDER_REQUIRED");
    return fetch(url, {
      redirect: "manual",
      signal: AbortSignal.timeout(operationalTimeouts.networkMs),
      headers: {
        Authorization: "Bearer " + token,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2026-03-10",
      },
    });
  }
  return {
    async json(path) {
      const r = await response(path);
      const bytes = await boundedBody(r, 8_000_000);
      insist(r.status === 200, "PROVIDER_READ_HTTP_" + r.status);
      return JSON.parse(bytes.toString()) as unknown;
    },
    async archive(id) {
      insist(positive(id), "EXACT_ARTIFACT_ID_REQUIRED");
      const r = await response(`/repos/${repository}/actions/artifacts/${id}/zip`);
      insist(r.status === 302, "ARTIFACT_REDIRECT_REQUIRED");
      const location = r.headers.get("location");
      await r.body?.cancel();
      insist(location, "ARTIFACT_LOCATION_REQUIRED");
      const url = new URL(location);
      insist(
        url.protocol === "https:" &&
          !url.username &&
          !url.password &&
          !url.port &&
          !url.hash &&
          (url.hostname.endsWith(".blob.core.windows.net") ||
            url.hostname.endsWith(".actions.githubusercontent.com")),
        "ARTIFACT_STORAGE_ORIGIN_REFUSED",
      );
      const archive = await fetch(url, {
        redirect: "error",
        credentials: "omit",
        signal: AbortSignal.timeout(operationalTimeouts.networkMs),
      });
      const bytes = await boundedBody(archive, 8_000_000);
      insist(archive.status === 200, "ARTIFACT_DOWNLOAD_FAILED");
      return bytes;
    },
  };
}
export const repoPath = (suffix: string): string => `/repos/${repository}/${suffix}`;
export function verifyRun(
  p: TrustedPolicy,
  raw: unknown,
  id: number,
  attempt: number,
  requestId: string,
  completed: boolean,
): ProviderRun {
  const r = raw as ProviderRun;
  insist(
    r.id === id &&
      r.run_attempt === attempt &&
      r.repository.id === repositoryId &&
      r.event === "workflow_dispatch" &&
      r.head_sha === p.release &&
      r.head_branch === p.protectedTag &&
      r.path === `${workflowPath}@refs/tags/${p.protectedTag}` &&
      r.actor.id === p.dispatcher.id &&
      r.triggering_actor.id === p.dispatcher.id &&
      r.display_title === runTitle(requestId),
    "UNTRUSTED_WORKFLOW_OR_ATTEMPT",
  );
  if (completed)
    insist(
      r.status === "completed" && r.conclusion === "success",
      "CURRENT_SUCCESSFUL_RUN_REQUIRED",
    );
  return r;
}
async function pages(
  reader: ProviderReader,
  suffix: string,
  key: string,
  maximum: number,
): Promise<unknown[]> {
  const all: unknown[] = [];
  let total: number | undefined;
  for (let page = 1; page <= Math.ceil(maximum / 100) + 1; page++) {
    const r = object(
      await reader.json(
        repoPath(suffix + (suffix.includes("?") ? "&" : "?") + `per_page=100&page=${page}`),
      ),
    );
    insist(
      Number.isInteger(r.total_count) &&
        Number(r.total_count) <= maximum &&
        (total === undefined || r.total_count === total),
      "PROVIDER_PAGE_DENOMINATOR_CHANGED",
    );
    total = Number(r.total_count);
    const items = r[key];
    insist(Array.isArray(items) && items.length <= 100, "PROVIDER_PAGE_REQUIRED");
    all.push(...items);
    if (all.length === total) return all;
    insist(items.length === 100 && all.length < total, "PROVIDER_PAGE_TRUNCATED");
  }
  throw Error("TRUSTED_CI_PROVIDER_PAGE_LIMIT");
}
export async function fetchExecution(
  reader: ProviderReader,
  p: TrustedPolicy,
  id: number,
  attempt: number,
  requestId: string,
) {
  verifyRun(p, await reader.json(repoPath(`actions/runs/${id}`)), id, attempt, requestId, true);
  const jobs = (await pages(
    reader,
    `actions/runs/${id}/attempts/${attempt}/jobs`,
    "jobs",
    100,
  )) as ProviderJob[];
  insist(jobs.length === 1 && positive(jobs[0]!.id), "REQUIRED_JOB_DENOMINATOR_CHANGED");
  const j = jobs[0]!;
  insist(
    j.run_id === id &&
      j.run_attempt === attempt &&
      j.head_sha === p.release &&
      j.name === jobName &&
      j.status === "completed" &&
      j.conclusion === "success",
    "REQUIRED_JOB_NOT_SUCCESSFUL",
  );
  for (const name of requiredSteps) {
    const hits = j.steps.filter((s) => s.name === name);
    insist(
      hits.length === 1 && hits[0]!.status === "completed" && hits[0]!.conclusion === "success",
      "REQUIRED_STEP_NOT_SUCCESSFUL",
    );
  }
  insist(
    new Set(j.steps.map((s) => s.number)).size === j.steps.length &&
      j.steps.every((s) => s.status === "completed" && s.conclusion === "success"),
    "SKIPPED_OR_DUPLICATE_STEP_REFUSED",
  );
  const artifacts = await pages(reader, `actions/runs/${id}/artifacts`, "artifacts", 100);
  const name = `trusted-source-${id}-${attempt}`;
  const matches = artifacts.map(object).filter((a) => a.name === name);
  insist(matches.length === 1, "EXACT_UNSHADOWED_ARTIFACT_REQUIRED");
  const a = matches[0]!,
    origin = object(a.workflow_run);
  insist(
    positive(a.id) &&
      a.expired === false &&
      Number(a.size_in_bytes) > 0 &&
      Number(a.size_in_bytes) <= 8_000_000 &&
      origin.id === id &&
      origin.head_sha === p.release &&
      origin.head_branch === p.protectedTag &&
      typeof a.digest === "string" &&
      /^sha256:[a-f0-9]{64}$/.test(a.digest),
    "ARTIFACT_ORIGIN_OR_DIGEST_MISMATCH",
  );
  const archive = await reader.archive(a.id),
    archiveSha256 = sha256(archive);
  const reportBytes = readReportArchive(archive, a.digest.slice(7));
  // Archive digest, exact report bytes and canonical-object digest remain separate.
  return {
    reportBytes,
    artifactId: a.id,
    archiveSha256,
    reportSha256: sha256(reportBytes),
    jobsSha256: sha256(JSON.stringify(jobs)),
  };
}
export async function verifyPolicyRef(reader: ProviderReader, p: TrustedPolicy): Promise<void> {
  const ref = object(await reader.json(repoPath(`git/ref/tags/${p.protectedTag}`))),
    item = object(ref.object);
  // Only a lightweight protected immutable tag is supported initially; no implicit tag peeling.
  insist(
    ref.ref === "refs/tags/" + p.protectedTag && item.type === "commit" && item.sha === p.release,
    "POLICY_TAG_MOVED",
  );
  const file = object(await reader.json(repoPath(`contents/${workflowPath}?ref=${p.release}`)));
  insist(
    file.type === "file" &&
      file.encoding === "base64" &&
      typeof file.content === "string" &&
      oid(file.sha) &&
      sha256(Buffer.from(file.content, "base64")) === p.workflowSha256,
    "ACTUAL_WORKFLOW_BYTES_MISMATCH",
  );
}
