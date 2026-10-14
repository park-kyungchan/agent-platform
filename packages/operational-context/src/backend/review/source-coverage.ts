import { createHash } from "node:crypto";
import { readerResult } from "./result-contract.ts";
import { dataSnapshot, parseData } from "./data-snapshot.ts";
export type RecordValue = Record<string, any>;
export type ReviewBinding = {
  packetSha256: string;
  sessionId: string;
  requestSha256: string;
  model: string;
  provider: string;
};
export type Check = (condition: any, code: string) => void;
export const hash = (text: string) => createHash("sha256").update(text).digest("hex");
export const object = (v: any): v is RecordValue =>
  v !== null && typeof v === "object" && !Array.isArray(v);
export const parse = parseData;
export const integer = (v: any) => Number.isSafeInteger(v) && v >= 0;
export const nonempty = (v: any) => typeof v === "string" && v.trim().length > 0;
export const sha = (v: any) => typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
const id = (v: any) => typeof v === "string" && /^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/.test(v);

/** Pure pinned input and exact Unicode-code-point exposure. Source labels never become paths. */
export class SourceCoverage {
  readonly reviewId: string;
  #expected: ReviewBinding;
  get expected(): Readonly<ReviewBinding> {
    return this.#expected;
  }
  private docs = new Map<string, RecordValue>();
  private ranges = new Map<string, [number, number][]>();
  pageReads = 0;
  constructor(
    text: string,
    expected: ReviewBinding,
    private check: Check,
  ) {
    expected = dataSnapshot(expected);
    this.#expected = expected;
    if (
      !object(expected) ||
      !sha(expected.packetSha256) ||
      !sha(expected.requestSha256) ||
      !nonempty(expected.model) ||
      !nonempty(expected.provider) ||
      typeof expected.sessionId !== "string" ||
      !/^\d{8}_\d{6}_[0-9a-f]{6}$/.test(expected.sessionId) ||
      typeof text !== "string" ||
      Buffer.byteLength(text) > 524288 ||
      hash(text) !== expected.packetSha256
    )
      throw new Error("binding");
    const packet = JSON.parse(text);
    if (
      !object(packet) ||
      !id(packet.review_id) ||
      !Array.isArray(packet.documents) ||
      packet.documents.length < 1 ||
      packet.documents.length > 32
    )
      throw new Error("packet");
    this.reviewId = packet.review_id;
    for (const d of packet.documents) {
      if (
        !object(d) ||
        !id(d.id) ||
        this.docs.has(d.id) ||
        typeof d.source !== "string" ||
        !d.source ||
        d.source.length > 2048 ||
        typeof d.content !== "string" ||
        !d.content ||
        !sha(d.sha256) ||
        hash(d.content) !== d.sha256
      )
        throw new Error("document");
      this.docs.set(d.id, { ...d, chars: [...d.content] });
      this.ranges.set(d.id, []);
    }
  }
  read(args: any, result: any) {
    args = dataSnapshot(args);
    result = dataSnapshot(result);
    const check = this.check;
    if (!object(args) || !object(result)) throw new Error("reader");
    const supported = readerResult(args, result);
    check(supported, "UNSUPPORTED_RESULT_ANNOTATION");
    if (!supported) return;
    check(
      Object.keys(args).every((k) => ["document_id", "offset", "limit"].includes(k)),
      "READER_ARGUMENT_INVALID",
    );
    const pinned =
      result.review_id === this.reviewId && result.packet_sha256 === this.#expected.packetSha256;
    check(pinned, "PACKET_IDENTITY_MISMATCH");
    if (!Object.keys(args).length) {
      const index = [...this.docs.values()].map((d) => ({
        id: d.id,
        source: d.source,
        sha256: d.sha256,
        total_chars: d.chars.length,
      }));
      check(JSON.stringify(result.documents) === JSON.stringify(index), "INDEX_MISMATCH");
      return;
    }
    const doc = this.docs.get(args.document_id);
    const start = Object.hasOwn(args, "offset") ? args.offset : 0;
    const limit = Object.hasOwn(args, "limit") ? args.limit : 8000;
    if (
      !doc ||
      !integer(start) ||
      !integer(limit) ||
      limit < 1 ||
      limit > 8000 ||
      start > doc.chars.length
    ) {
      check(false, "READER_ARGUMENT_INVALID");
      return;
    }
    const text = doc.chars.slice(start, start + limit).join(""),
      end = start + [...text].length;
    const valid =
      result.document_id === doc.id &&
      result.source === doc.source &&
      result.source_sha256 === doc.sha256 &&
      result.offset === start &&
      result.content === text &&
      result.total_chars === doc.chars.length &&
      result.has_more === end < doc.chars.length;
    check(valid, "SOURCE_PAGE_MISMATCH");
    if (valid && pinned) {
      this.ranges.get(doc.id)!.push([start, end]);
      this.pageReads++;
    }
  }
  finish() {
    return [...this.docs].map(([documentId, doc]) => {
      let end = 0;
      for (const [start, stop] of this.ranges.get(documentId)!.sort((a, b) => a[0] - b[0])) {
        if (start > end) break;
        end = Math.max(end, stop);
      }
      this.check(end === doc.chars.length, "INCOMPLETE_SOURCE_COVERAGE");
      return { documentId, coveredChars: end, totalChars: doc.chars.length };
    });
  }
}
