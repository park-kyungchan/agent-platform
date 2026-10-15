import { mkdir, writeFile } from 'node:fs/promises';
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
}
export async function executeRemotePython(target: HermesTarget, operation: string, source: string, runDirectory: string): Promise<ExecutionReceipt> {
  if (!/^[a-zA-Z0-9_.-]+$/.test(operation)) throw new Error('Invalid operation identifier');
  const startedAtUtc = new Date().toISOString();
  const child = Bun.spawn(['ssh', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', `HostKeyAlias=${target.ssh.hostKeyAlias}`, '-o', 'ConnectTimeout=10', '-p', String(target.ssh.port), `${target.ssh.user}@${target.ssh.address}`, 'python3 -'], { stdin: new Blob([source]), stdout: 'pipe', stderr: 'pipe' });
  const [stdout, stderr, exitCode] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
  const receipt: ExecutionReceipt = { schemaVersion: 1, operation, startedAtUtc, completedAtUtc: new Date().toISOString(), exitCode, stdout, stderr };
  await mkdir(runDirectory, { recursive: true });
  await writeFile(join(runDirectory, `${operation}-execution.json`), JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 });
  return receipt;
}
