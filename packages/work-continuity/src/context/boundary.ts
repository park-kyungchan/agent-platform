import { randomUUID } from 'node:crypto';
import { mkdir, open, realpath } from 'node:fs/promises';
import { join } from 'node:path';
import type { BoundaryEvent } from './types';
import { assertAbsolutePath, insideRoot, localContextIo, sha256 } from './source';

/** Immutable lifecycle observation; never infers answers or edits semantic checkpoints. */
export async function recordBoundary(root: string, input: BoundaryEvent): Promise<{ path: string; sha256: string }> {
  assertAbsolutePath(root);
  if (await realpath(root) !== root) throw Error('Boundary requires canonical owning root');
  if (typeof input.event !== 'string' || !/^[a-zA-Z][a-zA-Z0-9_-]{0,79}$/.test(input.event) || typeof input.atUtc !== 'string' || !Number.isFinite(Date.parse(input.atUtc)) || !input.atUtc.endsWith('Z')) throw Error('Invalid native boundary event');
  const fields = ['event', 'atUtc', 'threadId', 'turnId', 'checkpointPath', 'pendingPrompt', 'nativeSource', 'trigger', 'nativeAgentId', 'nativeAgentType', 'nativeTranscriptPath'];
  if (Object.keys(input).some((key) => !fields.includes(key))) throw Error('Unsupported boundary event fields');
  for (const field of ['threadId', 'turnId', 'nativeSource', 'trigger', 'nativeAgentId', 'nativeAgentType', 'nativeTranscriptPath'] as const) {
    if (input[field] !== undefined && (typeof input[field] !== 'string' || input[field]!.length > 4096)) throw Error(`Invalid boundary ${field}`);
  }
  if (input.checkpointPath !== undefined) {
    assertAbsolutePath(input.checkpointPath);
    if (!insideRoot(root, input.checkpointPath) || await realpath(input.checkpointPath) !== input.checkpointPath) throw Error('Boundary checkpoint must belong to root');
  }
  if (input.nativeTranscriptPath !== undefined) assertAbsolutePath(input.nativeTranscriptPath);
  if (input.pendingPrompt) {
    const pin = input.pendingPrompt; assertAbsolutePath(pin.path);
    if (await realpath(pin.path) !== pin.path || !/^[a-f0-9]{64}$/.test(pin.sha256) || !Number.isSafeInteger(pin.bytes) || pin.bytes < 0) throw Error('Invalid pending prompt locator');
    const bytes = await localContextIo.read(pin.path);
    if (bytes.length !== pin.bytes || sha256(bytes) !== pin.sha256) throw Error('Pending prompt locator drifted');
  }
  const directory = join(root, 'state', 'continuity', 'boundaries');
  await mkdir(directory, { recursive: true, mode: 0o700 });
  if (await realpath(directory) !== directory) throw Error('Boundary directory must be canonical');
  const path = join(directory, `${input.atUtc.replace(/[:.]/g, '-')}-${randomUUID()}.json`);
  const bytes = new TextEncoder().encode(`${JSON.stringify({ schema: 'codex.continuity.boundary.v1', projectRoot: root, ...input, semanticInterpretation: 'none', authority: 'none', nativePermissionGranted: false }, null, 2)}\n`);
  const file = await open(path, 'wx', 0o600);
  try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
  return { path, sha256: sha256(bytes) };
}
