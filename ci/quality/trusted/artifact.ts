import { inflateRawSync } from "node:zlib";
import { insist, sha256 } from "./policy";

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
/** Extract one fixed member in memory. Never extract archive paths into any filesystem. */
export function readReportArchive(bytes: Uint8Array, expectedSha256: string): Buffer {
  insist(
    bytes.length > 22 && bytes.length <= 8_000_000 && sha256(bytes) === expectedSha256,
    "EXACT_ARTIFACT_ARCHIVE_REQUIRED",
  );
  const b = Buffer.from(bytes),
    end = b.length - 22;
  insist(
    b.readUInt32LE(end) === 0x06054b50 &&
      b.readUInt16LE(end + 4) === 0 &&
      b.readUInt16LE(end + 6) === 0 &&
      b.readUInt16LE(end + 8) === 1 &&
      b.readUInt16LE(end + 10) === 1 &&
      b.readUInt16LE(end + 20) === 0,
    "SINGLE_MEMBER_NON_ZIP64_ARCHIVE_REQUIRED",
  );
  const centralSize = b.readUInt32LE(end + 12),
    c = b.readUInt32LE(end + 16);
  insist(
    c >= 30 && c + centralSize === end && centralSize >= 46 && b.readUInt32LE(c) === 0x02014b50,
    "BOUNDED_ARCHIVE_DIRECTORY_REQUIRED",
  );
  const flags = b.readUInt16LE(c + 8),
    method = b.readUInt16LE(c + 10),
    crc = b.readUInt32LE(c + 16),
    compressed = b.readUInt32LE(c + 20),
    size = b.readUInt32LE(c + 24);
  const nameSize = b.readUInt16LE(c + 28),
    extra = b.readUInt16LE(c + 30),
    comment = b.readUInt16LE(c + 32),
    offset = b.readUInt32LE(c + 42);
  insist(
    (flags & ~0x808) === 0 &&
      [0, 8].includes(method) &&
      size > 0 &&
      size <= 4_000_000 &&
      compressed > 0 &&
      compressed <= 8_000_000 &&
      offset === 0 &&
      b.readUInt16LE(c + 34) === 0,
    "BOUNDED_REGULAR_MEMBER_REQUIRED",
  );
  insist(
    46 + nameSize + extra + comment === centralSize &&
      b.subarray(c + 46, c + 46 + nameSize).toString() === "trusted-execution.json",
    "EXACT_REPORT_MEMBER_REQUIRED",
  );
  const unixMode = b.readUInt32LE(c + 38) >>> 16;
  insist(
    (unixMode & 0xf000) === 0 || (unixMode & 0xf000) === 0x8000,
    "REGULAR_REPORT_MEMBER_REQUIRED",
  );
  insist(
    b.readUInt32LE(0) === 0x04034b50 &&
      b.readUInt16LE(6) === flags &&
      b.readUInt16LE(8) === method &&
      b.readUInt16LE(26) === nameSize,
    "ARCHIVE_LOCAL_HEADER_MISMATCH",
  );
  const start = 30 + nameSize + b.readUInt16LE(28),
    finish = start + compressed;
  insist(
    start <= c &&
      finish <= c &&
      b.subarray(30, 30 + nameSize).toString() === "trusted-execution.json",
    "ARCHIVE_MEMBER_BOUNDS",
  );
  if (!(flags & 8))
    insist(
      finish === c &&
        b.readUInt32LE(14) === crc &&
        b.readUInt32LE(18) === compressed &&
        b.readUInt32LE(22) === size,
      "ARCHIVE_LOCAL_SIZE_MISMATCH",
    );
  else {
    const signature = finish + 16 === c && b.readUInt32LE(finish) === 0x08074b50;
    const d = signature ? finish + 4 : finish;
    insist(
      d + 12 === c &&
        b.readUInt32LE(d) === crc &&
        b.readUInt32LE(d + 4) === compressed &&
        b.readUInt32LE(d + 8) === size,
      "ARCHIVE_DESCRIPTOR_MISMATCH",
    );
  }
  const report =
    method === 0
      ? b.subarray(start, finish)
      : inflateRawSync(b.subarray(start, finish), { maxOutputLength: 4_000_000 });
  insist(report.length === size && crc32(report) === crc, "ARCHIVE_REPORT_INTEGRITY_MISMATCH");
  return Buffer.from(report);
}
