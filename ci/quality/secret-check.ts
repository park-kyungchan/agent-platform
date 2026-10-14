import { createHash } from "node:crypto";
import { lstat, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

export interface VerifiedSecretTool {
  path: string;
  binarySha256: string;
  pin: { name: "gitleaks"; version: string; sha256: string };
}

export interface SecretFinding {
  ruleId: string;
  path: string;
  line: number;
}

export interface SecretCheckResult {
  verificationClass: "maintained-source-secret-scan-only";
  status: "pass" | "fail";
  scannedFiles: number;
  findings: SecretFinding[];
  durationMs: number;
  toolVersion: string;
  binarySha256: string;
  archiveSha256: string;
  redaction: "metadata-only";
  historyScanned: false;
  nativePermissionGranted: false;
}

const protectedSegment =
  /^(?:\.git|\.private|private|sessions?|history|work|evidence|discovery|runs|state|vendor|vendored|generated|dist|node_modules|owner-fixture|releases)$/;

export function permittedSecretSource(path: string): boolean {
  // These are immutable vocabulary definitions owned by the architecture policy, not evidence stores.
  if (
    [
      "packages/ontology/ontology/domains/evidence/registry.json",
      "packages/ontology/ontology/domains/evidence/vocabulary.json",
    ].includes(path)
  )
    return true;
  return (
    path.length > 0 &&
    !isAbsolute(path) &&
    !path.includes("\\") &&
    !path
      .split("/")
      .some((part) => ["", ".", ".."].includes(part) || protectedSegment.test(part)) &&
    !/(?:^|\/)(?:\.env(?:\.[^/]*)?|\.gitleaksignore|\.gitleaks\.toml)$/.test(path) &&
    !path.endsWith(".d.ts") &&
    /\.(?:[cm]?tsx?|json|ya?ml|toml|sh|py|md)$/.test(path)
  );
}

function safeFinding(
  value: unknown,
  sourceRoot: string,
  allowed: ReadonlySet<string>,
): SecretFinding {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("Invalid native secret finding.");
  const finding = value as Record<string, unknown>;
  if (
    typeof finding.File !== "string" ||
    typeof finding.RuleID !== "string" ||
    !/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(finding.RuleID) ||
    !Number.isSafeInteger(finding.StartLine) ||
    Number(finding.StartLine) <= 0
  ) {
    throw Error("Native secret finding metadata is invalid.");
  }
  const path = relative(sourceRoot, resolve(sourceRoot, finding.File)).split(sep).join("/");
  if (!allowed.has(path) || !permittedSecretSource(path))
    throw Error("Native secret finding escaped the selected source denominator.");
  // Secret, Match, Description, fingerprint, line text and raw tool output are discarded.
  return { ruleId: finding.RuleID, path, line: Number(finding.StartLine) };
}

/** Scan an explicit existing quality denominator in an isolated tree, never git/history or live owner data. */
export async function runSecretCheck(
  rootPath: string,
  files: readonly string[],
  tool: VerifiedSecretTool,
  timeoutMs = 90_000,
): Promise<SecretCheckResult> {
  const started = performance.now(),
    root = resolve(rootPath);
  if (
    (await realpath(root)) !== root ||
    !files.length ||
    files.length > 4096 ||
    new Set(files).size !== files.length ||
    files.some((path) => !permittedSecretSource(path))
  ) {
    throw Error("Secret scan requires an exact nonempty maintained source denominator.");
  }
  if (
    tool.pin.name !== "gitleaks" ||
    !/^\d+\.\d+\.\d+$/.test(tool.pin.version) ||
    !/^[a-f0-9]{64}$/.test(tool.pin.sha256) ||
    !/^[a-f0-9]{64}$/.test(tool.binarySha256) ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 1000 ||
    timeoutMs > 90_000
  ) {
    throw Error("Secret scan requires an exact verified native tool and bounded timeout.");
  }
  const binaryInfo = await lstat(tool.path);
  if (
    !binaryInfo.isFile() ||
    binaryInfo.isSymbolicLink() ||
    binaryInfo.size > 100_000_000 ||
    (await realpath(tool.path)) !== tool.path ||
    createHash("sha256")
      .update(await readFile(tool.path))
      .digest("hex") !== tool.binarySha256
  ) {
    throw Error("Verified secret scanner binary changed.");
  }
  const temporary = await mkdtemp(join(tmpdir(), "agent-platform-secret-scan-"));
  try {
    const sourceRoot = join(temporary, "source");
    await mkdir(sourceRoot, { mode: 0o700 });
    let sourceBytes = 0;
    for (const path of files) {
      const input = resolve(root, path),
        info = await lstat(input);
      if (
        !info.isFile() ||
        info.isSymbolicLink() ||
        (await realpath(input)) !== input ||
        info.size > 2_000_000
      ) {
        throw Error("Selected secret-scan source is not a bounded exact regular file.");
      }
      const bytes = await readFile(input);
      sourceBytes += bytes.length;
      if (sourceBytes > 32_000_000 || bytes.includes(0))
        throw Error("Secret-scan source byte budget or text requirement failed.");
      const output = join(sourceRoot, path);
      await mkdir(dirname(output), { recursive: true, mode: 0o700 });
      await writeFile(output, bytes, { mode: 0o600 });
    }
    const config = join(temporary, "trusted-defaults.toml"),
      report = join(temporary, "findings.json");
    await writeFile(config, "[extend]\nuseDefault = true\n", { mode: 0o600 });
    const child = Bun.spawn(
      [
        tool.path,
        "dir",
        "--no-banner",
        "--no-color",
        "--redact=100",
        "--log-level=error",
        "--exit-code=2",
        "--report-format=json",
        "--report-path=" + report,
        "--config=" + config,
        "--gitleaks-ignore-path=" + join(temporary, "no-ignore-file"),
        "--ignore-gitleaks-allow",
        "--max-archive-depth=0",
        "--max-decode-depth=0",
        "--max-target-megabytes=2",
        "--timeout=60",
        sourceRoot,
      ],
      {
        cwd: temporary,
        stdout: "ignore",
        stderr: "ignore",
        env: { PATH: "/usr/bin:/bin", LANG: "C", CI: "true" },
      },
    );
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill();
    }, timeoutMs);
    let exitCode: number;
    try {
      exitCode = await child.exited;
    } finally {
      clearTimeout(timer);
    }
    if (timedOut || ![0, 2].includes(exitCode))
      throw Error("Native secret scanner did not finish successfully.");
    const info = await lstat(report);
    if (
      !info.isFile() ||
      info.isSymbolicLink() ||
      info.size > 8_000_000 ||
      (await realpath(report)) !== report
    ) {
      throw Error("Native secret report is not a bounded exact regular file.");
    }
    const raw: unknown = JSON.parse(await readFile(report, "utf8"));
    if (!Array.isArray(raw) || raw.length > 4096)
      throw Error("Native secret report denominator is invalid.");
    const findings = raw.map((value) => safeFinding(value, sourceRoot, new Set(files)));
    if ((exitCode === 0) !== (findings.length === 0))
      throw Error("Native secret exit code and findings disagree.");
    return {
      verificationClass: "maintained-source-secret-scan-only",
      status: findings.length ? "fail" : "pass",
      scannedFiles: files.length,
      findings,
      durationMs: Math.round(performance.now() - started),
      toolVersion: tool.pin.version,
      binarySha256: tool.binarySha256,
      archiveSha256: tool.pin.sha256,
      redaction: "metadata-only",
      historyScanned: false,
      nativePermissionGranted: false,
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

/** A synthetic credential proves the real pinned detector and metadata-only output path. */
export async function verifySecretCanary(tool: VerifiedSecretTool): Promise<{
  cleanPassed: true;
  syntheticCredentialDetected: true;
  sensitiveFieldsDiscarded: true;
}> {
  const temporary = await mkdtemp(join(tmpdir(), "agent-platform-secret-canary-"));
  try {
    const cleanPath = "src/clean.ts",
      canaryPath = "src/canary.ts";
    await mkdir(join(temporary, "src"));
    await writeFile(join(temporary, cleanPath), "export const value = 1;\n");
    const prefix = ["g", "h", "p", "_"].join("");
    const material = createHash("sha256")
      .update("nonfunctional local gitleaks canary")
      .digest("hex");
    const synthetic = prefix + material.slice(0, 36);
    await writeFile(join(temporary, canaryPath), 'export const token = "' + synthetic + '";\n');
    const clean = await runSecretCheck(temporary, [cleanPath], tool);
    const detected = await runSecretCheck(temporary, [canaryPath], tool);
    const safe = JSON.stringify(detected);
    if (
      clean.status !== "pass" ||
      detected.status !== "fail" ||
      !detected.findings.length ||
      !detected.findings.some((finding) => finding.ruleId === "github-pat") ||
      safe.includes(synthetic) ||
      safe.includes("export const token") ||
      detected.findings.some(
        (finding) => Object.keys(finding).sort().join(",") !== "line,path,ruleId",
      )
    ) {
      throw Error("Native secret scanner synthetic canary or redaction contract failed.");
    }
    return { cleanPassed: true, syntheticCredentialDetected: true, sensitiveFieldsDiscarded: true };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}
