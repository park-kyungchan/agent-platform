import { readArtifactMember } from "../artifact-archive";

/** Legacy trusted policy transport retains its exact member and size contract. */
export function readReportArchive(bytes: Uint8Array, expectedSha256: string): Buffer {
  return readArtifactMember(bytes, expectedSha256, "trusted-execution.json", 4_000_000);
}
