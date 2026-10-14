import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { assertArtifact } from "@palantirkc/ontology";
import type { Foundry } from "../service";
import { createObject } from "../service";
import { redactCaptureMetadata } from "./capture-policy";
import { hash } from "../modules/storage/store";
/** Text-only receipts preserve their original provenance; absent HTTP metadata stays unknown. */
export async function importLegacyReceipt(
  foundry: Foundry,
  path: string,
  excludedDomains: string[] = [],
) {
  const originalPath = resolve(path);
  const bytes = await readFile(originalPath);
  const value = JSON.parse(bytes.toString());
  assertArtifact(value);
  if (value.kind !== "evidence") throw new Error("Only original evidence receipts are supported");
  if (excludedDomains.includes(value.domainId) || excludedDomains.includes(value.projectId))
    throw new Error("Excluded legacy domain");
  const id = "legacy-" + hash(bytes);
  const existing = await foundry.getObject(id);
  if (existing) return existing;
  const receipt = await foundry.store.putBlob(bytes);
  const artifacts = [{ kind: "legacy-receipt", ...receipt, originalPath }];
  const details = value.payload.result.details;
  const contentPath = typeof details?.contentPath === "string" ? details.contentPath : null;
  const missing: string[] = [];
  let integrityMismatch = false;
  if (contentPath)
    try {
      const content = await readFile(
        contentPath.startsWith("/") ? contentPath : join(originalPath, "..", contentPath),
      );
      const observedHash = hash(content);
      const expectedHash =
        typeof details?.contentSha256 === "string" ? details.contentSha256 : null;
      const expectedLength = typeof details?.byteLength === "number" ? details.byteLength : null;
      integrityMismatch =
        (expectedHash !== null && observedHash !== expectedHash) ||
        (expectedLength !== null && content.byteLength !== expectedLength);
      if (integrityMismatch)
        missing.push(
          "Legacy content fingerprint differs from the original receipt; current file bytes are retained without attributing them to the original observation",
        );
      artifacts.push({
        kind: integrityMismatch ? "legacy-content-current" : "extracted-text",
        ...(await foundry.store.putBlob(content)),
        originalPath: contentPath,
      });
    } catch (error) {
      missing.push(String(error));
    }
  const object = createObject(
    "SourceCapture",
    value.projectId,
    {
      url: value.payload.sourceUrl,
      retrievedAt: value.payload.fetchedAt,
      outcome: integrityMismatch ? "partial" : value.payload.result.outcome,
      artifacts,
      originalPath,
      originalRetrievedAt: value.payload.fetchedAt,
      metadata: {
        legacy: true,
        integrityMismatch,
        currentContentReadAt: new Date().toISOString(),
        httpStatus: null,
        requestHeaders: null,
        responseHeaders: null,
        redirects: null,
        unavailable: ["Legacy text receipt did not observe HTTP metadata", ...missing],
        originalEvidence: redactCaptureMetadata(value) as any,
      },
    },
    { id },
  );
  await foundry.putObjects([object]);
  return object;
}
