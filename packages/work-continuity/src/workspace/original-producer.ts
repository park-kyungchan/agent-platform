import { open, realpath, lstat } from "node:fs/promises";
import { constants } from "node:fs";
import { isAbsolute } from "node:path";
import { sha256 } from "../context/source";

export interface NativeOriginalRow {
  line: number;
  bytes: Uint8Array;
  byteOffset?: number;
}
interface OriginalAdmissionBase {
  projectId: string;
  workId: string;
  threadId: string;
  turnId: string;
  sourcePath: string;
  firstLine: number;
  lastLine: number;
  segmentSha256: string;
  scope: "explicit-selected-work-turn";
  anchors: Array<{
    line: number;
    lineSha256: string;
    ownerRecordPath: string;
    ownerRecordSha256: string;
  }>;
}
/** The optional discriminator keeps existing closed-turn callers compatible. */
export interface OriginalSegmentAdmission extends OriginalAdmissionBase {
  kind?: "closed-turn";
}
export interface PrefixRowBoundary {
  line: number;
  byteOffset: number;
  byteLength: number;
  rowSha256: string;
}
export interface OriginalPrefixAdmission extends OriginalAdmissionBase {
  kind: "immutable-prefix";
  sourceStatus: "sourceOpen" | "terminalUnknown";
  completedEpisode: false;
  observedAt: string;
  session: { line: 1; rowSha256: string };
  taskStart: { line: number; rowSha256: string };
  firstCompleteRow: PrefixRowBoundary;
  lastCompleteRow: PrefixRowBoundary;
  omittedTail?: { line: number; byteLength: number; sha256: string };
}
export interface OriginalPrefixCheckpoint {
  readonly schema: "codex.native-original-prefix-checkpoint.v1";
  readonly admission: Readonly<OriginalPrefixAdmission>;
  readonly sha256: string;
}
export interface OriginalPrefixEvidence {
  sessionRow: NativeOriginalRow;
  /** Only the immediately following incomplete row, never captured as an original. */
  trailingBytes?: Uint8Array;
  predecessor?: OriginalPrefixCheckpoint;
}
const timestamp = (value: string) =>
  typeof value === "string" &&
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value) &&
  Number.isFinite(Date.parse(value)) &&
  new Date(value).toISOString() === value;
function nonterminalTail(text: string): boolean {
  const types = ["response_item", "token_usage_record", "turn_context", "world_state"];
  try {
    const value = JSON.parse(text);
    return types.includes(value.type);
  } catch {
    /* An incomplete payload is allowed only after an unambiguous native header. */
  }
  const header =
    /^\s*\{\s*"type"\s*:\s*"(?:response_item|token_usage_record|turn_context|world_state)"\s*,\s*"payload"\s*:\s*\{/.exec(
      text,
    );
  if (!header) return false;
  let depth = 2,
    quoted = false,
    escaped = false;
  for (const char of text.slice(header[0].length)) {
    if (quoted) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === '"') quoted = false;
    } else if (char === '"') quoted = true;
    else if (char === "{" || char === "[") depth++;
    else if (char === "}" || char === "]") {
      if (--depth < 2) return false;
    }
  }
  return true;
}
function freezeRecord<T>(value: T): T {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeRecord(child);
    Object.freeze(value);
  }
  return value;
}
export interface OriginalProducerBounds {
  maxLines: number;
  maxBytes: number;
  maxLineBytes: number;
}
const uuid = (value: string) => /^[a-f0-9-]{36}$/.test(value);
const hash = (value: string) => /^[a-f0-9]{64}$/.test(value);
const rowBytesWithoutLf = (bytes: Uint8Array) =>
  bytes.at(-1) === 10 ? bytes.subarray(0, -1) : bytes;
const textOf = (content: any): string | null =>
  typeof content === "string"
    ? content
    : Array.isArray(content) &&
        content.every(
          (item) =>
            ["text", "Text", "input_text", "output_text"].includes(item.type) &&
            typeof item.text === "string",
        )
      ? content.map((item) => item.text).join("")
      : null;
const generatedText = (text: string) =>
  /^\s*(?:# AGENTS\.md instructions|<environment_context>|<INSTRUCTIONS>|<turn_aborted>|<subagent_notification>|<system-reminder>|<user_instructions>)/.test(
    text,
  );
const sensitive = (text: string) => {
  const scan = text.replace(/\\+"/g, '"');
  return (
    /https?:[^\s"<>]*(?:[?&](?:sig|signature|x-amz-signature|x-goog-signature|access_token|token|key|authorization|jwt)=)|\bBearer\s+[A-Za-z0-9._~+\/-]{16,}|\b(?:sk-[A-Za-z0-9_-]{16,}|-----BEGIN [^-]*PRIVATE KEY-----)/i.test(
      scan,
    ) ||
    /["'](?:access_token|refresh_token|id_token|client_secret|api_key|password)["']\s*:\s*["'][^"'\s]+["']/i.test(
      scan,
    )
  );
};
const nestedHiddenContext = (text: string) =>
  /"(?:channel|phase)"\s*:\s*"analysis"|"role"\s*:\s*"(?:system|developer)"|"type"\s*:\s*"reasoning"/.test(
    text.replace(/\\+"/g, '"'),
  );

/** Exact source-owner adapter, with no native lifecycle or new session store. */
export async function readAdmittedNativeRows(input: {
  sourcePath: string;
  firstLine: number;
  lastLine: number;
  allowedSessionPaths: ReadonlySet<string>;
  bounds: OriginalProducerBounds;
}) {
  const { sourcePath, firstLine, lastLine, bounds } = input;
  if (
    !input.allowedSessionPaths.has(sourcePath) ||
    !isAbsolute(sourcePath) ||
    (await realpath(sourcePath)) !== sourcePath
  )
    throw Error("Native source is not an explicitly admitted canonical session");
  const before = await lstat(sourcePath);
  if (!before.isFile() || before.isSymbolicLink())
    throw Error("Native source must be an admitted regular file");
  if (
    ![firstLine, lastLine, bounds.maxLines, bounds.maxBytes, bounds.maxLineBytes].every(
      (n) => Number.isSafeInteger(n) && n > 0,
    ) ||
    lastLine < firstLine ||
    lastLine - firstLine + 1 > bounds.maxLines
  )
    throw Error("Invalid bounded native segment");
  const file = await open(sourcePath, constants.O_RDONLY | constants.O_NOFOLLOW),
    rows: NativeOriginalRow[] = [];
  let pending = Buffer.alloc(0),
    line = 0,
    position = 0,
    rowOffset = 0,
    selectedBytes = 0,
    header: any;
  let sessionRow: NativeOriginalRow | undefined;
  try {
    const descriptor = await file.stat();
    if (
      !descriptor.isFile() ||
      descriptor.dev !== before.dev ||
      descriptor.ino !== before.ino ||
      (await realpath(sourcePath)) !== sourcePath
    )
      throw Error("Native source identity changed at descriptor open");
    while (line < lastLine) {
      const chunk = Buffer.alloc(65536),
        { bytesRead } = await file.read(chunk, 0, chunk.length, position);
      if (!bytesRead) break;
      position += bytesRead;
      pending = Buffer.concat([pending, chunk.subarray(0, bytesRead)]);
      let end: number;
      while ((end = pending.indexOf(10)) >= 0 && line < lastLine) {
        const bytes = new Uint8Array(pending.subarray(0, end + 1));
        pending = pending.subarray(end + 1);
        line++;
        if (line === 1) {
          const row = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
          if (row.type !== "session_meta") throw Error("Native session metadata missing");
          header = { threadId: row.payload.id, cwd: row.payload.cwd };
          sessionRow = { line, bytes, byteOffset: rowOffset };
        }
        if (line >= firstLine) {
          selectedBytes += bytes.length;
          if (bytes.length > bounds.maxLineBytes || selectedBytes > bounds.maxBytes)
            throw Error("Native segment exceeds byte bounds");
          rows.push({ line, bytes, byteOffset: rowOffset });
        }
        rowOffset += bytes.length;
      }
      if (pending.length > bounds.maxLineBytes) throw Error("Native line exceeds byte bound");
    }
    const after = await lstat(sourcePath),
      descriptorAfter = await file.stat();
    if (
      !after.isFile() ||
      after.isSymbolicLink() ||
      after.dev !== descriptor.dev ||
      after.ino !== descriptor.ino ||
      descriptorAfter.dev !== descriptor.dev ||
      descriptorAfter.ino !== descriptor.ino ||
      (await realpath(sourcePath)) !== sourcePath
    )
      throw Error("Native source identity changed during read");
  } finally {
    await file.close();
  }
  return { header, sessionRow, rows, complete: rows.length === lastLine - firstLine + 1 };
}

/** Pure deterministic classification. Native-record coverage never proves UI display or human authority. */
export function produceOriginalManifest(
  admission: OriginalSegmentAdmission | OriginalPrefixAdmission,
  rows: readonly NativeOriginalRow[],
  bounds: OriginalProducerBounds,
  prefixEvidence?: OriginalPrefixEvidence,
) {
  // Detach caller-owned pins before classification; prefix metadata is frozen on return.
  admission = structuredClone(admission);
  const prefix = admission.kind === "immutable-prefix" ? admission : undefined;
  const problems: string[] = [],
    omissions: Array<{ line: number; reason: string }> = [];
  const originals: Array<{
    id: string;
    kind: "user" | "assistant" | "tool";
    authorship: string;
    contentSha256: string;
    provenance: Array<{ line: number; sha256: string; byteLength: number }>;
    nativePermissionGranted: false;
  }> = [];
  const captured: Array<{ sha256: string; bytes: Uint8Array }> = [];
  if (
    !admission.projectId ||
    !admission.workId ||
    !uuid(admission.threadId) ||
    !uuid(admission.turnId) ||
    admission.scope !== "explicit-selected-work-turn" ||
    !isAbsolute(admission.sourcePath) ||
    !hash(admission.segmentSha256) ||
    !admission.anchors.length ||
    admission.anchors.some(
      (anchor) =>
        !hash(anchor.lineSha256) ||
        !hash(anchor.ownerRecordSha256) ||
        !isAbsolute(anchor.ownerRecordPath),
    )
  )
    problems.push("Explicit owner/task/segment admission is required.");
  const ordered = [...rows].sort((a, b) => a.line - b.line);
  const unique: NativeOriginalRow[] = [];
  for (const row of ordered) {
    const previous = unique.at(-1);
    if (previous?.line === row.line) {
      if (
        sha256(previous.bytes) !== sha256(row.bytes) ||
        (prefix && previous.byteOffset !== row.byteOffset)
      )
        problems.push("Conflicting duplicate native line.");
      else omissions.push({ line: row.line, reason: "duplicate-input-locator" });
      continue;
    }
    unique.push({
      line: row.line,
      bytes: new Uint8Array(row.bytes),
      ...(row.byteOffset === undefined ? {} : { byteOffset: row.byteOffset }),
    });
  }
  if (
    unique.length !== admission.lastLine - admission.firstLine + 1 ||
    unique.some((row, i) => row.line !== admission.firstLine + i)
  )
    problems.push("Native segment is missing or contains unadmitted rows.");
  const totalBytes = unique.reduce((sum, row) => sum + row.bytes.length, 0);
  if (
    ![bounds.maxLines, bounds.maxBytes, bounds.maxLineBytes].every(
      (n) => Number.isSafeInteger(n) && n > 0,
    ) ||
    unique.length > bounds.maxLines ||
    totalBytes > bounds.maxBytes ||
    unique.some((row) => row.bytes.length > bounds.maxLineBytes || row.bytes.at(-1) !== 10)
  )
    problems.push("Native segment is oversized or truncated.");
  const segmentBytes = Buffer.concat(unique.map((row) => Buffer.from(row.bytes)));
  if (sha256(segmentBytes) !== admission.segmentSha256)
    problems.push("Native source segment drift.");
  for (const anchor of admission.anchors) {
    const row = unique.find((row) => row.line === anchor.line);
    if (!row || sha256(rowBytesWithoutLf(row.bytes)) !== anchor.lineSha256)
      problems.push("Selected owner anchor is missing or changed.");
  }
  const parsed: Array<{ row: NativeOriginalRow; value: any }> = [];
  for (const row of unique)
    try {
      const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(row.bytes));
      if (
        prefix &&
        (!value ||
          typeof value !== "object" ||
          Array.isArray(value) ||
          typeof value.type !== "string")
      )
        throw Error();
      parsed.push({ row, value });
    } catch {
      problems.push("Invalid or truncated native JSON row.");
    }
  const first = parsed[0]?.value,
    last = parsed.at(-1)?.value;
  if (prefix) validatePrefix(prefix);
  else if (admission.kind !== undefined && admission.kind !== "closed-turn")
    problems.push("Unknown native admission discriminator.");
  else if (
    first?.type !== "event_msg" ||
    first.payload?.type !== "task_started" ||
    first.payload.turn_id !== admission.turnId ||
    last?.type !== "event_msg" ||
    last.payload?.type !== "task_complete" ||
    last.payload.turn_id !== admission.turnId
  )
    problems.push("Exact closed native task boundaries are absent.");
  if (
    parsed
      .slice(1, -1)
      .some(({ value }) => value.type === "event_msg" && value.payload?.type === "task_started")
  )
    problems.push("Nested or overlapping native task admission is ambiguous.");
  if (problems.length) return result();

  function validatePrefix(pin: OriginalPrefixAdmission) {
    const session = prefixEvidence?.sessionRow;
    if (
      !["sourceOpen", "terminalUnknown"].includes(pin.sourceStatus) ||
      pin.completedEpisode !== false ||
      !timestamp(pin.observedAt) ||
      !Number.isSafeInteger(pin.firstLine) ||
      pin.firstLine < 2 ||
      !Number.isSafeInteger(pin.lastLine) ||
      pin.lastLine < pin.firstLine
    )
      problems.push("Explicit unfinished prefix status, bounds and snapshot time are required.");
    try {
      if (
        !session ||
        session.line !== 1 ||
        session.byteOffset !== 0 ||
        session.bytes.at(-1) !== 10 ||
        session.bytes.length > bounds.maxLineBytes ||
        pin.session?.line !== 1 ||
        !hash(pin.session.rowSha256) ||
        sha256(session.bytes) !== pin.session.rowSha256
      )
        throw Error();
      const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(session.bytes));
      if (value.type !== "session_meta" || value.payload?.id !== pin.threadId) throw Error();
    } catch {
      problems.push("Pinned native session evidence is unavailable or changed.");
    }
    if (
      first?.type !== "event_msg" ||
      first.payload?.type !== "task_started" ||
      first.payload.turn_id !== pin.turnId ||
      (first.payload.thread_id !== undefined && first.payload.thread_id !== pin.threadId) ||
      pin.taskStart?.line !== pin.firstLine ||
      !hash(pin.taskStart?.rowSha256 ?? "") ||
      !unique[0] ||
      sha256(unique[0].bytes) !== pin.taskStart.rowSha256
    )
      problems.push("Pinned native task start is missing or changed.");
    for (const [boundary, row, expectedLine] of [
      [pin.firstCompleteRow, unique[0], pin.firstLine],
      [pin.lastCompleteRow, unique.at(-1), pin.lastLine],
    ] as const) {
      if (
        !boundary ||
        !row ||
        boundary.line !== expectedLine ||
        !Number.isSafeInteger(boundary.byteOffset) ||
        boundary.byteOffset < (session?.bytes.length ?? 0) ||
        !Number.isSafeInteger(boundary.byteLength) ||
        boundary.byteLength !== row.bytes.length ||
        boundary.byteOffset !== row.byteOffset ||
        !hash(boundary.rowSha256) ||
        sha256(row.bytes) !== boundary.rowSha256
      )
        problems.push("Explicit complete-row byte boundary is missing or changed.");
    }
    if (
      unique.some(
        (row, i) =>
          !Number.isSafeInteger(row.byteOffset) ||
          (i > 0 && row.byteOffset !== unique[i - 1]!.byteOffset! + unique[i - 1]!.bytes.length),
      ) ||
      (pin.firstLine === 2 && unique[0]?.byteOffset !== session?.bytes.length)
    )
      problems.push("Prefix row byte offsets are not contiguous with the pinned source.");
    if (
      parsed
        .slice(1)
        .some(
          ({ value }) =>
            value.type === "event_msg" && /^(?:task_|turn_)/.test(value.payload?.type ?? ""),
        )
    )
      problems.push("Prefix contains an overlapping task or terminal marker.");
    if (
      parsed.some(
        ({ value }) =>
          value.type === "session_meta" ||
          (value.payload?.thread_id !== undefined && value.payload.thread_id !== pin.threadId) ||
          (value.payload?.turn_id !== undefined && value.payload.turn_id !== pin.turnId),
      )
    )
      problems.push("Native prefix session or turn identity drift.");
    const tail = prefixEvidence?.trailingBytes;
    if (pin.omittedTail || tail) {
      // A partial event row could hide task_complete/task_started. Only an explicit
      // non-lifecycle top-level row prefix is safely omittable; everything else blocks.
      let text = "";
      try {
        text = new TextDecoder("utf-8", { fatal: true }).decode(tail);
      } catch {
        /* fail closed below */
      }
      if (
        !tail ||
        !pin.omittedTail ||
        !tail.length ||
        tail.includes(10) ||
        tail.length > bounds.maxLineBytes ||
        totalBytes + tail.length > bounds.maxBytes ||
        unique.length + 1 > bounds.maxLines ||
        pin.omittedTail.line !== pin.lastLine + 1 ||
        pin.omittedTail.byteLength !== tail.length ||
        !hash(pin.omittedTail.sha256) ||
        sha256(tail) !== pin.omittedTail.sha256 ||
        !nonterminalTail(text)
      )
        problems.push("Ambiguous, terminal or unpinned partial trailing bytes.");
      else
        omissions.push({ line: pin.omittedTail.line, reason: "incomplete-trailing-row-excluded" });
    }
    const old = prefixEvidence?.predecessor;
    if (old) {
      const prior = old.admission;
      const identity = (a: OriginalPrefixAdmission) =>
        JSON.stringify([
          a.projectId,
          a.workId,
          a.threadId,
          a.turnId,
          a.sourcePath,
          a.scope,
          a.firstLine,
          a.session,
          a.taskStart,
          a.firstCompleteRow,
          a.anchors,
        ]);
      const precedingRows = unique.filter((row) => row.line <= prior.lastLine);
      if (
        old.schema !== "codex.native-original-prefix-checkpoint.v1" ||
        !hash(old.sha256) ||
        sha256(JSON.stringify(prior)) !== old.sha256 ||
        prior.kind !== "immutable-prefix" ||
        prior.completedEpisode !== false ||
        !timestamp(prior.observedAt) ||
        identity(prior) !== identity(pin) ||
        prior.lastLine > pin.lastLine ||
        Date.parse(prior.observedAt) >= Date.parse(pin.observedAt) ||
        sha256(Buffer.concat(precedingRows.map((row) => Buffer.from(row.bytes)))) !==
          prior.segmentSha256 ||
        JSON.stringify(prior.lastCompleteRow) !==
          JSON.stringify({
            line: precedingRows.at(-1)?.line,
            byteOffset: precedingRows.at(-1)?.byteOffset,
            byteLength: precedingRows.at(-1)?.bytes.length,
            rowSha256: precedingRows.at(-1) ? sha256(precedingRows.at(-1)!.bytes) : "",
          })
      )
        problems.push("Immutable predecessor prefix bytes, source, task or owner anchor changed.");
      if (prior.omittedTail) {
        const next = unique.find((row) => row.line === prior.omittedTail!.line);
        const continuation = next?.bytes ?? tail;
        if (
          !continuation ||
          continuation.length < prior.omittedTail.byteLength ||
          sha256(continuation.subarray(0, prior.omittedTail.byteLength)) !==
            prior.omittedTail.sha256
        )
          problems.push("Previously observed partial tail bytes changed.");
      }
    }
  }
  const nativeByLine = new Map<number, (typeof originals)[number]>();
  const byIdentity = new Map<string, (typeof originals)[number]>();
  function omit(line: number, reason: string) {
    omissions.push({ line, reason });
  }
  function capture(
    row: NativeOriginalRow,
    value: { id: string; kind: "user" | "assistant" | "tool"; authorship: string; content: string },
  ) {
    const provenance = { line: row.line, sha256: sha256(row.bytes), byteLength: row.bytes.length };
    const contentSha256 = sha256(value.content),
      key = value.kind + ":" + value.id;
    const old = byIdentity.get(key);
    if (old && old.contentSha256 !== contentSha256) {
      problems.push("Native original identity has conflicting content.");
      omit(row.line, "conflicting-original-identity");
      return;
    }
    captured.push({ sha256: provenance.sha256, bytes: new Uint8Array(row.bytes) });
    if (old) {
      old.provenance.push(provenance);
      nativeByLine.set(row.line, old);
      omit(row.line, "native-mirror-deduplicated");
      return;
    }
    const original = {
      id: key,
      kind: value.kind,
      authorship: value.authorship,
      contentSha256,
      provenance: [provenance],
      nativePermissionGranted: false as const,
    };
    originals.push(original);
    byIdentity.set(key, original);
    nativeByLine.set(row.line, original);
  }
  for (let i = 0; i < parsed.length; i++) {
    const { row, value } = parsed[i]!,
      p = value.payload ?? {},
      item = p.item;
    if (
      p.channel === "analysis" ||
      p.phase === "analysis" ||
      p.role === "system" ||
      p.role === "developer" ||
      p.type === "reasoning" ||
      item?.type === "Reasoning"
    ) {
      omit(row.line, "hidden-analysis-or-instruction-context");
      continue;
    }
    if (
      value.type === "compacted" ||
      value.type === "inter_agent_communication_metadata" ||
      p.type === "agent_message"
    ) {
      omit(row.line, "generated-or-replayed-context");
      continue;
    }
    if (
      value.type === "event_msg" &&
      p.type === "item_completed" &&
      item?.type === "ContextCompaction"
    ) {
      omit(row.line, "generated-compaction-metadata");
      continue;
    }
    if (
      value.type === "event_msg" &&
      p.type === "item_completed" &&
      item?.type === "SubAgentActivity" &&
      Object.keys(item).every((key) =>
        [
          "type",
          "model",
          "reasoning_effort",
          "id",
          "kind",
          "agent_thread_id",
          "agent_path",
        ].includes(key),
      )
    ) {
      omit(row.line, "native-agent-activity-metadata");
      continue;
    }
    if (
      value.type === "event_msg" &&
      p.type === "item_completed" &&
      item?.type === "Extension" &&
      item.kind === "clock.sleep"
    ) {
      omit(row.line, "native-clock-sleep-metadata");
      continue;
    }
    if (
      value.type === "event_msg" &&
      p.type === "item_completed" &&
      ["UserMessage", "AgentMessage"].includes(item?.type)
    ) {
      if (p.thread_id !== admission.threadId || p.turn_id !== admission.turnId) {
        omit(row.line, "foreign-native-item");
        problems.push("Foreign native item inside admitted task.");
        continue;
      }
      const text = textOf(item.content);
      if (text === null) {
        omit(row.line, "unsupported-content-or-attachment-original");
        continue;
      }
      if (generatedText(text)) {
        omit(row.line, "generated-input-context");
        continue;
      }
      if (sensitive(text)) {
        omit(row.line, "sensitive-original-withheld");
        continue;
      }
      if (item.type === "UserMessage") {
        if (!item.client_id) {
          omit(row.line, "user-authorship-unproven");
          continue;
        }
        capture(row, {
          id: item.id,
          kind: "user",
          authorship: "native-user-input-observed-not-an-accepted-decision",
          content: text,
        });
        const previous = parsed[i - 1];
        if (previous?.value.type === "response_item" && previous.value.payload?.role === "user") {
          const prior = previous.value.payload,
            retained = previous.value.metadata?.retained_source;
          if (
            retained?.complete === true &&
            retained.id?.turn_id === admission.turnId &&
            textOf(prior.content) === text &&
            prior.id === retained.id?.message_id
          )
            capture(previous.row, {
              id: item.id,
              kind: "user",
              authorship: "native-user-input-observed-not-an-accepted-decision",
              content: text,
            });
          else
            omit(
              previous.row.line,
              retained?.complete === false
                ? "truncated-retained-source"
                : "generated-or-user-lineage-unproven",
            );
        }
      } else if (["commentary", "final_answer", "final"].includes(item.phase))
        capture(row, {
          id: item.id,
          kind: "assistant",
          authorship: "native-visible-channel-record-ui-display-unverified",
          content: text,
        });
      else omit(row.line, "assistant-visibility-unproven");
      continue;
    }
    if (value.type === "response_item" && p.type === "message") {
      if (p.role === "user") {
        // Classified only after an adjacent exact native UserMessage match.
        const next = parsed[i + 1];
        if (!(next?.value.type === "event_msg" && next.value.payload?.item?.type === "UserMessage"))
          omit(row.line, "generated-or-user-lineage-unproven");
      } else if (p.role === "assistant") {
        const previous = parsed[i - 1],
          native = previous && nativeByLine.get(previous.row.line),
          retained = value.metadata?.retained_source;
        if (
          native?.kind === "assistant" &&
          retained?.complete === true &&
          retained.id?.turn_id === admission.turnId &&
          p.id === previous!.value.payload.item.id &&
          textOf(p.content) !== null &&
          sha256(textOf(p.content)!) === native.contentSha256
        )
          capture(row, {
            id: p.id,
            kind: "assistant",
            authorship: native.authorship,
            content: textOf(p.content)!,
          });
        else
          omit(
            row.line,
            retained?.complete === false
              ? "truncated-retained-source"
              : "assistant-native-lineage-unproven",
          );
      } else omit(row.line, "nonvisible-message-role");
      continue;
    }
    if (
      value.type === "response_item" &&
      [
        "function_call",
        "custom_tool_call",
        "function_call_output",
        "custom_tool_call_output",
      ].includes(p.type)
    ) {
      const boundTurn =
        p.internal_chat_message_metadata_passthrough?.turn_id ??
        value.metadata?.retained_source?.id?.turn_id;
      if (boundTurn !== admission.turnId) {
        omit(row.line, boundTurn ? "foreign-tool-turn" : "tool-turn-lineage-unproven");
        if (boundTurn) problems.push("Foreign response tool item inside admitted task.");
        continue;
      }
      if (nestedHiddenContext(JSON.stringify(p))) {
        omit(row.line, "nested-hidden-context-withheld");
        continue;
      }
      if (sensitive(new TextDecoder().decode(row.bytes))) {
        omit(row.line, "sensitive-original-withheld");
        continue;
      }
      capture(row, {
        id: (p.call_id ?? p.id ?? row.line) + ":" + p.type,
        kind: "tool",
        authorship: "native-tool-record-ui-display-unverified",
        content: JSON.stringify(p),
      });
      continue;
    }
    if (
      value.type === "event_msg" &&
      p.type === "item_completed" &&
      ([
        "CommandExecution",
        "FileChange",
        "McpToolCall",
        "WebSearch",
        "CollabAgentToolCall",
      ].includes(item?.type) ||
        (item?.type === "Extension" && item.kind === "web.search"))
    ) {
      if (p.thread_id !== admission.threadId || p.turn_id !== admission.turnId) {
        omit(row.line, "foreign-native-item");
        problems.push("Foreign native tool item.");
        continue;
      }
      if (nestedHiddenContext(JSON.stringify(item))) {
        omit(row.line, "nested-hidden-context-withheld");
        continue;
      }
      if (sensitive(JSON.stringify(item))) {
        omit(row.line, "sensitive-original-withheld");
        continue;
      }
      capture(row, {
        id: item.id + ":" + item.type,
        kind: "tool",
        authorship: "native-tool-record-ui-display-unverified",
        content: JSON.stringify(item),
      });
      continue;
    }
    omit(
      row.line,
      value.type === "event_msg" && p.type === "item_completed"
        ? "unknown-native-item"
        : ["event_msg", "token_usage_record", "turn_context", "world_state"].includes(value.type)
          ? "native-metadata-not-original-content"
          : "unknown-native-row",
    );
  }
  function result() {
    const accounted = new Set([
      ...originals.flatMap((row) => row.provenance.map((pin) => pin.line)),
      ...omissions.map((row) => row.line),
    ]);
    const unaccounted = unique.filter((row) => !accounted.has(row.line)).map((row) => row.line);
    const unresolved = omissions.filter((row) =>
      /unproven|unknown|unsupported|truncated|conflicting|foreign/.test(row.reason),
    );
    const checkpoint: OriginalPrefixCheckpoint | undefined =
      prefix && !problems.length
        ? freezeRecord({
            schema: "codex.native-original-prefix-checkpoint.v1" as const,
            admission: structuredClone(prefix),
            sha256: sha256(JSON.stringify(prefix)),
          })
        : undefined;
    const manifest = {
      schema: "codex.native-original-segment-manifest.v1" as const,
      admission: structuredClone(admission),
      ...(prefix
        ? {
            custodyKind: "immutable-prefix" as const,
            sourceStatus: prefix.sourceStatus,
            completedEpisode: false as const,
            completePrefixBytesVerified: problems.length === 0,
            fullConversationComplete: false as const,
            memoryPromotionGranted: false as const,
            ...(checkpoint ? { checkpoint } : {}),
            ...(prefixEvidence?.predecessor
              ? { predecessorSha256: prefixEvidence.predecessor.sha256 }
              : {}),
          }
        : {}),
      status: problems.length
        ? ("blocked" as const)
        : unresolved.length || unaccounted.length
          ? ("native-segment-covered-with-unresolved-visibility" as const)
          : ("eligible-native-records-captured" as const),
      nativeSegmentComplete: problems.length === 0 && unaccounted.length === 0,
      fullUserVisibleOriginalPopulationComplete: false as const,
      uiRenderingVerified: false as const,
      acceptedDecisionAuthorityGranted: false as const,
      rawSegmentLines: unique.length,
      rawSegmentBytes: totalBytes,
      eligibleLogicalOriginals: originals.length,
      capturedRawLines: captured.length,
      originals,
      omissions,
      omissionCounts: Object.fromEntries(
        [...new Set(omissions.map((row) => row.reason))].map((reason) => [
          reason,
          omissions.filter((row) => row.reason === reason).length,
        ]),
      ),
      unresolvedVisibilityRows: unresolved.length,
      unaccounted,
      problems,
    };
    return {
      manifest: prefix ? freezeRecord(manifest) : manifest,
      captured: problems.length ? [] : captured,
    };
  }
  return result();
}
