import { randomUUID } from 'node:crypto';
import { mkdir, open, rename } from 'node:fs/promises';
import { join } from 'node:path';

export interface HermesTarget {
  schemaVersion: 1;
  hostId: string;
  ssh: { address: string; user: string; hostKeyAlias: string; port: number };
  project: string;
  gatewayService: string;
  profile: string;
  expectedProfiles: string[];
  nativeHome: string;
  nativeUid: number;
  nativeGid: number;
  nativePython: string;
  nativeRepository: string;
  gatewaySlot: string;
}
export interface ExecutionReceipt {
  schemaVersion: 1;
  operation: string;
  startedAtUtc: string;
  completedAtUtc: string;
  exitCode: number;
  stdout: string;
  stderr: string;
  stage: 'spawn' | 'transport' | 'stream' | 'completed';
  reason: 'completed' | 'spawn-failed' | 'nonzero-exit' | 'deadline-exceeded' | 'stdout-limit-exceeded' | 'stderr-limit-exceeded' | 'stream-read-failed' | 'reap-failed';
  ownedChildCreated: boolean;
  transportExitCode: number | null;
  transportSignal: string | null;
  reaped: boolean;
  cleanup: { termSent: boolean; killSent: boolean; signalFailed: boolean; streamCancellationRequested: boolean; streamsCancelled: boolean };
  output: { stdoutBytesObserved: number; stderrBytesObserved: number; stdoutComplete: boolean; stderrComplete: boolean };
  limits: Required<RemotePythonLimits>;
  /** Transport settlement never proves native effects, serving, verification or acceptance. */
  remoteOutcome: 'not-started' | 'unverified' | 'uncertain';
  automaticReplayPermitted: false;
}
export interface RemotePythonLimits {
  timeoutMs?: number;
  maxStdoutBytes?: number;
  maxStderrBytes?: number;
  terminationGraceMs?: number;
  reapTimeoutMs?: number;
}
const defaultLimits: Required<RemotePythonLimits> = {
  timeoutMs: 120_000, maxStdoutBytes: 8 * 1024 * 1024, maxStderrBytes: 64 * 1024,
  terminationGraceMs: 1000, reapTimeoutMs: 1000,
};

/** Limits apply only to this owned SSH transport, never the remote native job. */
export async function executeRemotePython(target: HermesTarget, operation: string, source: string, runDirectory: string, options: RemotePythonLimits = {}): Promise<ExecutionReceipt> {
  if (!/^[a-zA-Z0-9_.-]+$/.test(operation)) throw new Error('Invalid operation identifier');
  const limits = { ...defaultLimits, ...options };
  if (Object.values(limits).some(value => !Number.isSafeInteger(value) || value < 1 || value > 2_147_483_647)) throw new Error('Execution limits must be positive bounded integers');
  const startedAtUtc = new Date().toISOString();
  const receipt: ExecutionReceipt = {
    schemaVersion: 1, operation, startedAtUtc, completedAtUtc: startedAtUtc, exitCode: 125, stdout: '', stderr: '',
    stage: 'spawn', reason: 'spawn-failed', ownedChildCreated: false, transportExitCode: null, transportSignal: null, reaped: false,
    cleanup: { termSent: false, killSent: false, signalFailed: false, streamCancellationRequested: false, streamsCancelled: false },
    output: { stdoutBytesObserved: 0, stderrBytesObserved: 0, stdoutComplete: false, stderrComplete: false },
    limits, remoteOutcome: 'not-started', automaticReplayPermitted: false,
  };
  await mkdir(runDirectory, { recursive: true });
  const save = async (value: unknown) => {
    const path = join(runDirectory, `${operation}-execution.json`), temporary = `${path}.${randomUUID()}.tmp`;
    const file = await open(temporary, 'wx', 0o600);
    try { await file.writeFile(JSON.stringify(value, null, 2) + '\n'); await file.sync(); }
    finally { await file.close(); }
    await rename(temporary, path);
    const directory = await open(runDirectory, 'r');
    try { await directory.sync(); } finally { await directory.close(); }
  };
  // A process interruption at the launch boundary must not leave a no-effect claim.
  await save({ ...receipt, completedAtUtc: null, exitCode: null, state: 'launch-pending', reason: 'launch-pending', ownedChildCreated: null, remoteOutcome: 'uncertain' });
  const deadline = performance.now() + limits.timeoutMs;
  const spawn = () => Bun.spawn(['ssh', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', `HostKeyAlias=${target.ssh.hostKeyAlias}`, '-o', 'ConnectTimeout=10', '-p', String(target.ssh.port), `${target.ssh.user}@${target.ssh.address}`, 'python3 -'], { stdin: new Blob([source]), stdout: 'pipe', stderr: 'pipe' });
  let child: ReturnType<typeof spawn> | null = null;
  try { child = spawn(); }
  catch { receipt.exitCode = 127; }
  if (child) {
    const ownedChild = child;
    receipt.ownedChildCreated = true;
    receipt.remoteOutcome = 'uncertain';
    let stop: (reason: ExecutionReceipt['reason']) => void;
    const interrupted = new Promise<ExecutionReceipt['reason']>(resolve => { stop = resolve; });
    const stdoutChunks: Uint8Array[] = [];
    const readers = [child.stdout.getReader(), child.stderr.getReader()];
    const read = async (index: 0 | 1) => {
      const name = index === 0 ? 'stdout' : 'stderr';
      const byteKey = index === 0 ? 'stdoutBytesObserved' : 'stderrBytesObserved';
      const completeKey = index === 0 ? 'stdoutComplete' : 'stderrComplete';
      const maximum = index === 0 ? limits.maxStdoutBytes : limits.maxStderrBytes;
      try {
        while (true) {
          const { done, value } = await readers[index]!.read();
          if (done) { if (!receipt.cleanup.streamCancellationRequested) receipt.output[completeKey] = true; return; }
          receipt.output[byteKey] += value.byteLength;
          if (receipt.output[byteKey] > maximum) { stop(`${name}-limit-exceeded`); return; }
          // Stderr is never retained, and failure stdout is discarded below.
          if (index === 0) stdoutChunks.push(value.slice());
        }
      } catch { stop('stream-read-failed'); }
    };
    const exit = child.exited.then(code => { receipt.transportExitCode = code; receipt.transportSignal = child.signalCode; receipt.reaped = true; }, () => { stop('reap-failed'); });
    const settled = Promise.all([read(0), read(1), exit]).then(() => 'settled' as const);
    const timer = setTimeout(() => stop('deadline-exceeded'), Math.max(1, deadline - performance.now()));
    let outcome: ExecutionReceipt['reason'] | 'settled';
    try { outcome = await Promise.race([settled, interrupted]); }
    finally { clearTimeout(timer); }
    const wait = async (promise: Promise<unknown>, milliseconds: number) => {
      let timeout: ReturnType<typeof setTimeout>;
      try { return await Promise.race([promise.then(() => true), new Promise<false>(resolve => { timeout = setTimeout(() => resolve(false), milliseconds); })]); }
      finally { clearTimeout(timeout!); }
    };
    if (outcome !== 'settled') {
      const signal = (name: 'SIGTERM' | 'SIGKILL') => {
        if (receipt.reaped) return;
        try { ownedChild.kill(name); receipt.cleanup[name === 'SIGTERM' ? 'termSent' : 'killSent'] = true; }
        catch { receipt.cleanup.signalFailed = true; }
      };
      signal('SIGTERM');
      await wait(exit, limits.terminationGraceMs);
      if (!receipt.reaped) { signal('SIGKILL'); await wait(exit, limits.reapTimeoutMs); }
      receipt.cleanup.streamCancellationRequested = true;
      // Cancel only our pipes; a descendant or remote process is not ours to kill.
      let cancelled = false;
      const cancellation = Promise.allSettled(readers.map(reader => reader!.cancel())).then(results => { cancelled = results.every(result => result.status === 'fulfilled'); });
      await wait(cancellation, limits.reapTimeoutMs);
      receipt.cleanup.streamsCancelled = cancelled;
      receipt.stage = outcome.endsWith('limit-exceeded') || outcome === 'stream-read-failed' ? 'stream' : 'transport';
      receipt.reason = outcome;
      receipt.exitCode = outcome === 'deadline-exceeded' ? 124 : 125;
    } else {
      receipt.exitCode = receipt.transportExitCode ?? 125;
      receipt.reason = receipt.exitCode === 0 ? 'completed' : 'nonzero-exit';
      receipt.stage = receipt.exitCode === 0 ? 'completed' : 'transport';
      if (receipt.exitCode === 0) {
        receipt.stdout = Buffer.concat(stdoutChunks).toString('utf8');
        receipt.remoteOutcome = 'unverified';
      }
    }
  }
  receipt.completedAtUtc = new Date().toISOString();
  receipt.stderr = receipt.reason === 'completed' ? '' : `SSH transport ${receipt.reason}; remote outcome ${receipt.remoteOutcome}; automatic replay prohibited`;
  // A late exit after a failed reap must not rewrite the settled observation.
  const settledReceipt = structuredClone(receipt);
  await save({ ...settledReceipt, state: 'settled' });
  return settledReceipt;
}
