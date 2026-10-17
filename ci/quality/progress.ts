import { mkdir, mkdtemp, rename, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

/** Partial observations cannot stand in for the final quality report or its passing gate. */
export interface QualityProgress {
  schema: "agent-platform.quality-progress.v1";
  status: "running" | "complete";
  mode: string;
  startedAt: string;
  updatedAt: string;
  currentCheck: string | null;
  checks: { id: string; status: "pass" | "fail" | "skipped"; durationMs: number }[];
}

export type QualityProgressObserver = (progress: QualityProgress) => Promise<void>;

/** Retain only phase metadata, even when the check itself reports sensitive diagnostics. */
export async function writeQualityProgress(path: string, progress: QualityProgress): Promise<void> {
  const destination = resolve(path);
  await mkdir(dirname(destination), { recursive: true });
  const temporary = await mkdtemp(destination + ".tmp-");
  try {
    await writeFile(temporary + "/progress.json", JSON.stringify(progress, null, 2) + "\n");
    await rename(temporary + "/progress.json", destination);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}
