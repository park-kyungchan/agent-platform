import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import {
  readCurrentWork,
  currentWorkContextMaxChars,
  readOwnerSource,
  readExactFile,
  type CurrentWorkManifest,
  type PinnedOwnerSource,
} from "./work-reader";
interface SourceSet {
  id: string;
  ownerRoot: string;
  profiles: string[];
  workIds: string[];
  pins: PinnedOwnerSource[];
}
const hash = (x: Uint8Array | string) => createHash("sha256").update(x).digest("hex");
function bounds(input: any) {
  const offset = input.offset ?? 0,
    limit = input.limit ?? 6000;
  if (
    !Number.isSafeInteger(offset) ||
    offset < 0 ||
    !Number.isSafeInteger(limit) ||
    limit < 1 ||
    limit > 8000
  )
    throw Error("Invalid bounded review page");
  return { offset, limit };
}
export async function readReview(
  manifestPath: string,
  profile: string,
  input: any,
  expectedManifestSha256?: string,
) {
  const projection = await readCurrentWork(
      manifestPath,
      profile,
      undefined,
      currentWorkContextMaxChars,
      expectedManifestSha256,
    ),
    raw = await readExactFile(manifestPath, 1048576, projection.manifestPin.sha256),
    manifest = JSON.parse(raw.toString()) as CurrentWorkManifest & { sourceSets?: SourceSet[] };
  if (hash(raw) !== projection.manifestPin.sha256) throw Error("Review manifest changed");
  const { offset, limit } = bounds(input);
  if (input.mode === "pinned-source") {
    if (
      Object.keys(input).some(
        (x) => !["mode", "sourceSetId", "paths", "offset", "limit"].includes(x),
      ) ||
      !Array.isArray(input.paths) ||
      input.paths.length > 16 ||
      new Set(input.paths).size !== input.paths.length
    )
      throw Error("Exact bounded source selection required");
    const sets = (manifest.sourceSets ?? []).filter(
      (x) =>
        x.id === input.sourceSetId &&
        x.profiles.includes(profile) &&
        x.workIds.includes(projection.engineeringWork.workId),
    );
    if (sets.length !== 1) throw Error("Source set not admitted to actor/Work");
    const set = sets[0]!,
      documents = [];
    let chars = 0;
    if (!input.paths.length) {
      if (set.pins.length > 200) throw Error("Source inventory too large");
      return {
        schema: "hermes.native-review-read.v1",
        mode: "pinned-source",
        manifestPin: projection.manifestPin,
        sourceSetId: set.id,
        documents: set.pins,
        authority: "none",
        nativePermissionGranted: false,
      };
    }
    for (const path of input.paths) {
      const pins = set.pins.filter((x) => x.path === path);
      if (pins.length !== 1) throw Error("Unadmitted review source");
      const bytes = await readOwnerSource(set.ownerRoot, pins[0]!, 262144),
        content = bytes.toString("utf8").slice(offset, offset + limit);
      chars += content.length;
      if (chars > 8000) throw Error("Aggregate review page too large");
      documents.push({
        ...pins[0]!,
        offset,
        content,
        complete: offset + limit >= bytes.toString("utf8").length,
      });
    }
    await readExactFile(manifestPath, 1048576, projection.manifestPin.sha256);
    return {
      schema: "hermes.native-review-read.v1",
      mode: "pinned-source",
      manifestPin: projection.manifestPin,
      sourceSetId: set.id,
      documents,
      authority: "none",
      nativePermissionGranted: false,
    };
  }
  if (input.mode === "supplied-evidence") {
    if (
      Object.keys(input).some(
        (x) => !["mode", "packet", "packetSha256", "document_id", "offset", "limit"].includes(x),
      )
    )
      throw Error("Unknown supplied evidence fields");
    const p = input.packet;
    if (
      !p ||
      typeof p.review_id !== "string" ||
      !Array.isArray(p.documents) ||
      p.documents.length < 1 ||
      p.documents.length > 32 ||
      !/^[a-f0-9]{64}$/.test(input.packetSha256)
    )
      throw Error("Invalid supplied packet");
    const ids = new Set<string>();
    for (const d of p.documents) {
      if (
        !d ||
        typeof d.id !== "string" ||
        ids.has(d.id) ||
        typeof d.source !== "string" ||
        typeof d.content !== "string" ||
        d.content.length > 262144 ||
        hash(d.content) !== d.sha256
      )
        throw Error("Invalid supplied document");
      ids.add(d.id);
    }
    const documents = input.document_id
      ? p.documents.filter((d: any) => d.id === input.document_id)
      : p.documents;
    if (input.document_id && documents.length !== 1) throw Error("Unknown supplied document");
    return {
      schema: "hermes.native-review-read.v1",
      mode: "supplied-evidence",
      reviewId: p.review_id,
      frozenPacketSha256: input.packetSha256,
      packetProvenance:
        "Caller-supplied digest; the native Python loader must verify original packet bytes before invoking this utility.",
      packetDigestVerifiedByThisUtility: false,
      sourceLabelsOnly: true,
      liveSourceVerified: false,
      documents: input.document_id
        ? documents.map((d: any) => ({
            ...d,
            content: d.content.slice(offset, offset + limit),
            offset,
          }))
        : documents.map(({ id, source, sha256 }: any) => ({ id, source, sha256 })),
      authority: "none",
      nativePermissionGranted: false,
    };
  }
  throw Error("Review evidence mode must be explicit");
}
