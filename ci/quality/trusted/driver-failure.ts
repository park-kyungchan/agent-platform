import type { WorkerObservation } from "./policy";

export const driverFailureLimit = 2048;
const drivers = [
  "prepare",
  "audit",
  "static-quality",
  "probe",
  "source",
  "runtime-layout",
  "candidate-quality",
] as const;
type Driver = (typeof drivers)[number] | "unknown";
export type DriverFailurePhase =
  | "startup"
  | "copy-install-inputs"
  | "frozen-install"
  | "install-tools"
  | "copy-full-source"
  | "copy-git"
  | "prepare-result"
  | "audit-inputs"
  | "audit"
  | "static-quality"
  | "probe"
  | "materialize"
  | "source"
  | "runtime-layout"
  | "candidate-quality";
const phases: Record<Driver, readonly DriverFailurePhase[]> = {
  unknown: ["startup"],
  prepare: [
    "startup",
    "copy-install-inputs",
    "frozen-install",
    "install-tools",
    "copy-full-source",
    "copy-git",
    "prepare-result",
  ],
  audit: ["startup", "audit-inputs", "audit"],
  "static-quality": ["startup", "static-quality"],
  probe: ["startup", "probe"],
  source: ["startup", "materialize", "source"],
  "runtime-layout": ["startup", "materialize", "runtime-layout"],
  "candidate-quality": ["startup", "materialize", "candidate-quality"],
};
const workerRoles: Record<(typeof drivers)[number], WorkerObservation["role"]> = {
  prepare: "prepare",
  audit: "audit",
  "static-quality": "checker",
  probe: "probe",
  source: "candidate",
  "runtime-layout": "candidate",
  "candidate-quality": "candidate",
};
// Exact declared guards only. Unknown messages, including code-like strings, never leave the child.
const codes = [
  "TRUSTED_CI_DRIVER_FAILED",
  "TRUSTED_CI_WORKER_INPUT_LIMIT",
  "TRUSTED_CI_OBJECT_REQUIRED",
  "TRUSTED_CI_SOURCE_SYMLINK_REFUSED",
  "TRUSTED_CI_REGULAR_SOURCE_REQUIRED",
  "TRUSTED_CI_FROZEN_INSTALL_FAILED",
  "TRUSTED_CI_REQUIRED_NETWORK_AUDIT_FAILED",
  "TRUSTED_CI_TRUSTED_STATIC_CHECK_FAILED",
  "TRUSTED_CI_FIXED_PROBE_REQUIRED",
  "TRUSTED_CI_SAFE_PATH_REQUIRED",
  "TRUSTED_CI_EXPLICIT_EXTERNAL_AUDIT_REQUIRED",
  "TRUSTED_CI_CANDIDATE_QUALITY_FAILED",
  "TRUSTED_CI_UNKNOWN_DRIVER",
  "TRUSTED_CI_PROCESS_OUTPUT_LIMIT",
  "TRUSTED_CI_FIXED_ARGV_REQUIRED",
  "TRUSTED_CI_FIXED_SCENARIO_REQUIRED",
  "TRUSTED_CI_UNKNOWN_FINITE_SCENARIO",
] as const;
const errorTypes = [
  "Error",
  "TypeError",
  "RangeError",
  "SyntaxError",
  "ReferenceError",
  "URIError",
  "EvalError",
  "AggregateError",
  "OtherException",
] as const;
const osCodes = [
  "EACCES",
  "EPERM",
  "EROFS",
  "ENOENT",
  "ENOSPC",
  "EEXIST",
  "ENOTDIR",
  "EISDIR",
  "ELOOP",
  "EMFILE",
  "ENFILE",
  "ENOMEM",
  "EIO",
  "ETIMEDOUT",
] as const;
export interface DriverFailure {
  schema: "agent-platform.trusted-driver-failure.v1";
  driver: Driver;
  phase: DriverFailurePhase;
  code: (typeof codes)[number];
  errorType: (typeof errorTypes)[number];
  osCode: (typeof osCodes)[number] | null;
  errno: number | null;
}
const listed = (values: readonly string[], value: unknown): value is string =>
  typeof value === "string" && values.includes(value);
const boundedErrno = (value: unknown): value is number =>
  typeof value === "number" &&
  Number.isSafeInteger(value) &&
  value !== 0 &&
  Math.abs(value) <= 4095;
function dataProperty(value: unknown, key: string): unknown {
  try {
    if (value === null || (typeof value !== "object" && typeof value !== "function"))
      return undefined;
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    return descriptor && "value" in descriptor ? descriptor.value : undefined;
  } catch {
    return undefined;
  }
}
const line = (failure: DriverFailure): string => JSON.stringify(failure) + "\n";

/** One finite diagnostic line; never raw message, path, stack, environment or process output. */
export function encodeDriverFailure(
  error: unknown,
  rawDriver: unknown,
  phase: DriverFailurePhase,
): string {
  const driver = listed(drivers, rawDriver) ? (rawDriver as (typeof drivers)[number]) : "unknown";
  let name: unknown;
  try {
    name = dataProperty(Object.getPrototypeOf(error), "name");
  } catch {
    // Unknown exception type remains a finite generic label.
  }
  const message = dataProperty(error, "message"),
    osCode = dataProperty(error, "code"),
    errno = dataProperty(error, "errno");
  const failure: DriverFailure = {
    schema: "agent-platform.trusted-driver-failure.v1",
    driver,
    phase: phases[driver].includes(phase) ? phase : "startup",
    code: listed(codes, message) ? (message as DriverFailure["code"]) : "TRUSTED_CI_DRIVER_FAILED",
    errorType: listed(errorTypes, name) ? (name as DriverFailure["errorType"]) : "OtherException",
    osCode: listed(osCodes, osCode) ? (osCode as DriverFailure["osCode"]) : null,
    errno: boundedErrno(errno) ? errno : null,
  };
  const encoded = line(failure);
  return Buffer.byteLength(encoded) <= driverFailureLimit
    ? encoded
    : line({
        schema: "agent-platform.trusted-driver-failure.v1",
        driver: "unknown",
        phase: "startup",
        code: "TRUSTED_CI_DRIVER_FAILED",
        errorType: "OtherException",
        osCode: null,
        errno: null,
      });
}

/** Validate a complete canonical line for the actual fixed driver/role; data only, never authentication. */
export function parseDriverFailure(
  stderr: Uint8Array,
  role: WorkerObservation["role"],
  argv: readonly string[],
): DriverFailure | null {
  if (
    stderr.byteLength === 0 ||
    stderr.byteLength > driverFailureLimit ||
    argv.length !== 5 ||
    argv[0] !== "/usr/local/bin/bun" ||
    argv[1] !== "--no-env-file" ||
    argv[2] !== "--no-install" ||
    argv[3] !== "/policy/ci/quality/trusted/driver.ts" ||
    !listed(drivers, argv[4])
  )
    return null;
  const driver = argv[4] as (typeof drivers)[number];
  if (workerRoles[driver] !== role) return null;
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(stderr);
    const value: unknown = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value)) return null;
    const v = value as Record<string, unknown>,
      fields = ["schema", "driver", "phase", "code", "errorType", "osCode", "errno"];
    if (
      Object.keys(v).length !== fields.length ||
      !fields.every((key) => Object.hasOwn(v, key)) ||
      v.schema !== "agent-platform.trusted-driver-failure.v1" ||
      v.driver !== driver ||
      !listed(phases[driver], v.phase) ||
      !listed(codes, v.code) ||
      !listed(errorTypes, v.errorType) ||
      !(v.osCode === null || listed(osCodes, v.osCode)) ||
      !(v.errno === null || boundedErrno(v.errno))
    )
      return null;
    const result: DriverFailure = {
      schema: "agent-platform.trusted-driver-failure.v1",
      driver,
      phase: v.phase as DriverFailurePhase,
      code: v.code as DriverFailure["code"],
      errorType: v.errorType as DriverFailure["errorType"],
      osCode: v.osCode as DriverFailure["osCode"],
      errno: v.errno as number | null,
    };
    // Duplicate keys, extra lines, truncated/invalid UTF-8 and noncanonical mixed stderr stay unknown.
    return Buffer.from(line(result)).equals(Buffer.from(stderr)) ? result : null;
  } catch {
    return null;
  }
}

export const driverDiagnosticLimit = 8192;
export const qualityCheckIds = [
  "mutation-policy",
  "scope",
  "source-snapshot",
  "source-identity",
  "explicit-any-baseline-write",
  "build-public-ontology-package",
  "architecture",
  "format",
  "typed-lint-and-explicit-any-audit",
  "workflow-correctness-and-security",
  "dependency-vulnerabilities",
  "maintained-source-secrets",
  "quality-negative-fixtures",
  "source-currentness",
  "architecture-currentness",
  "arguments",
  "report-write",
] as const;
type CheckStatus = "pass" | "fail" | "skipped";
interface Location {
  path: string;
  line: number;
  column: number;
}
export interface QualityDriverDiagnostic {
  schema: "agent-platform.trusted-quality-diagnostic.v1";
  driver: "candidate-quality";
  status: "fail";
  total: number;
  failed: number;
  skipped: number;
  checks: { id: string; status: CheckStatus }[];
  reportBytes: number;
  reportSha256: string;
  failureLocations: Location[];
  locationTotal: number | null;
  locationOmitted: number | null;
  diagnosticComplete: boolean;
  retained: false;
  reference: null;
}
const finiteCount = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
const digest = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const fields = (v: Record<string, unknown>, names: readonly string[]) =>
  Object.keys(v).join("\0") === names.join("\0");
const row = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
const safeLocation = (value: unknown, knownPaths?: readonly string[]): value is Location => {
  const v = row(value);
  return (
    !!v &&
    fields(v, ["path", "line", "column"]) &&
    typeof v.path === "string" &&
    v.path.length <= 512 &&
    /^(?:[A-Za-z0-9_.-]+\/)*[A-Za-z0-9_.-]+\.(?:test|spec)\.[cm]?tsx?$/.test(v.path) &&
    !v.path.split("/").some((part) => part === "." || part === "..") &&
    (!knownPaths || knownPaths.includes(v.path)) &&
    finiteCount(v.line) &&
    v.line > 0 &&
    v.line <= 9_999_999 &&
    finiteCount(v.column) &&
    v.column > 0 &&
    v.column <= 9_999_999
  );
};

/** A report fingerprint and admitted locations, never arbitrary details or an authenticated result. */
export function qualityDriverDiagnostic(
  report: { checks: { id: string; status: CheckStatus; details: unknown }[] },
  reportBytes: number,
  reportSha256: string,
  maxBytes = driverDiagnosticLimit,
): QualityDriverDiagnostic {
  const locations: Location[] = [];
  let complete = true;
  let totalKnown = true;
  let total = 0;
  for (const check of report.checks) {
    const details = row(check.details),
      diagnostics = row(details?.failureDiagnostics);
    if (!diagnostics) continue;
    if (!Array.isArray(diagnostics.locations) || !finiteCount(diagnostics.total)) {
      complete = false;
      totalKnown = false;
      continue;
    }
    total += diagnostics.total;
    complete &&= diagnostics.diagnosticComplete === true;
    for (const location of diagnostics.locations)
      if (
        safeLocation(location) &&
        !locations.some((x) => JSON.stringify(x) === JSON.stringify(location))
      )
        locations.push(location);
  }
  const result: QualityDriverDiagnostic = {
    schema: "agent-platform.trusted-quality-diagnostic.v1",
    driver: "candidate-quality",
    status: "fail",
    total: report.checks.length,
    failed: report.checks.filter((c) => c.status === "fail").length,
    skipped: report.checks.filter((c) => c.status === "skipped").length,
    checks: report.checks.map(({ id, status }) => ({ id, status })),
    reportBytes,
    reportSha256,
    failureLocations: locations.slice(0, 16),
    locationTotal: totalKnown ? total : null,
    locationOmitted: totalKnown ? total - Math.min(locations.length, 16) : null,
    diagnosticComplete: complete && total === Math.min(locations.length, 16),
    retained: false,
    reference: null,
  };
  while (
    Buffer.byteLength(JSON.stringify(result) + "\n") > maxBytes &&
    result.failureLocations.length
  ) {
    result.failureLocations.pop();
    result.diagnosticComplete = false;
    if (result.locationTotal !== null)
      result.locationOmitted = result.locationTotal - result.failureLocations.length;
  }
  return result;
}

const finiteExit = (value: unknown) =>
  value === null ||
  (typeof value === "number" && Number.isSafeInteger(value) && Math.abs(value) <= 65535);
function finiteTransport(value: unknown): boolean {
  const v = row(value);
  if (!v) return false;
  if (
    fields(v, [
      "stdoutBytes",
      "stderrBytes",
      "stdoutSha256",
      "stderrSha256",
      "captureComplete",
      "hashScope",
    ])
  )
    return (
      finiteCount(v.stdoutBytes) &&
      finiteCount(v.stderrBytes) &&
      digest(v.stdoutSha256) &&
      digest(v.stderrSha256) &&
      typeof v.captureComplete === "boolean" &&
      v.hashScope === "observed-raw-bytes"
    );
  if (fields(v, ["failureKind", "errorSha256", "captureComplete", "retryAllowed"]))
    return (
      v.failureKind === "spawn-or-transport" &&
      (v.errorSha256 === null || digest(v.errorSha256)) &&
      v.captureComplete === false &&
      v.retryAllowed === false
    );
  return (
    fields(v, [
      "failureKind",
      "maxBytes",
      "retainedBytes",
      "observedStdoutBytes",
      "observedStderrBytes",
      "observedCombinedBytes",
      "observedStdoutSha256",
      "observedStderrSha256",
      "hashScope",
      "captureComplete",
      "limitExceeded",
      "timedOut",
      "exitCode",
      "signal",
      "retryAllowed",
    ]) &&
    v.failureKind === "output-limit" &&
    finiteCount(v.maxBytes) &&
    v.maxBytes > 0 &&
    finiteCount(v.retainedBytes) &&
    v.retainedBytes <= v.maxBytes &&
    finiteCount(v.observedStdoutBytes) &&
    finiteCount(v.observedStderrBytes) &&
    finiteCount(v.observedCombinedBytes) &&
    v.observedCombinedBytes === v.observedStdoutBytes + v.observedStderrBytes &&
    v.observedCombinedBytes > v.maxBytes &&
    digest(v.observedStdoutSha256) &&
    digest(v.observedStderrSha256) &&
    v.hashScope === "observed-raw-bytes" &&
    v.captureComplete === false &&
    v.limitExceeded === true &&
    typeof v.timedOut === "boolean" &&
    finiteExit(v.exitCode) &&
    (v.signal === null || listed(["SIGKILL", "SIGTERM"], v.signal)) &&
    v.retryAllowed === false
  );
}

/** Strict stdout diagnostics at the actual fixed driver boundary. Failure v1 stderr stays separate. */
export function parseDriverDiagnostic(
  stdout: Uint8Array,
  role: WorkerObservation["role"],
  argv: readonly string[],
  knownPaths: readonly string[],
): Record<string, unknown> | null {
  // TextDecoder otherwise strips a leading BOM before canonical-byte checks.
  if (stdout[0] === 0xef && stdout[1] === 0xbb && stdout[2] === 0xbf) return null;
  if (
    !stdout.byteLength ||
    argv.length !== 5 ||
    argv.slice(0, 4).join("\0") !==
      [
        "/usr/local/bin/bun",
        "--no-env-file",
        "--no-install",
        "/policy/ci/quality/trusted/driver.ts",
      ].join("\0") ||
    !listed(drivers, argv[4]) ||
    workerRoles[argv[4] as (typeof drivers)[number]] !== role
  )
    return null;
  try {
    // Source commands export a finite final line before throw. Earlier stdout is hashed only.
    const source = argv[4] === "source";
    if ((!source && stdout.byteLength > driverDiagnosticLimit) || stdout.byteLength > 8_000_000)
      return null;
    const text = new TextDecoder("utf-8", { fatal: true }).decode(stdout);
    const lines = text.split("\n");
    if (lines.pop() !== "") return null;
    const encoded = source
      ? [...lines].reverse().find((value) => {
          if (
            Buffer.byteLength(value + "\n") > 2048 ||
            !value.startsWith('{"schema":"agent-platform.source-command-diagnostic.v1",')
          )
            return false;
          try {
            return row(JSON.parse(value))?.status === "fail";
          } catch {
            return false;
          }
        })
      : lines[0];
    if (!encoded || Buffer.byteLength(encoded + "\n") > (source ? 2048 : driverDiagnosticLimit))
      return null;
    const v = row(JSON.parse(encoded));
    if (!v || JSON.stringify(v) !== encoded) return null;
    if (argv[4] === "static-quality") {
      const ids = [
        "format",
        "typed-lint",
        "architecture",
        "workflow-security",
        "maintained-secrets",
      ];
      if (
        lines.length !== 1 ||
        !fields(v, ["schema", "status", "checks", "tests", "files", "nativePermissionGranted"]) ||
        v.schema !== "agent-platform.trusted-static-quality.v1" ||
        v.nativePermissionGranted !== false ||
        !finiteCount(v.tests) ||
        !finiteCount(v.files) ||
        !Array.isArray(v.checks) ||
        v.checks.length !== ids.length
      )
        return null;
      if (
        !v.checks.every((c, i) => {
          const x = row(c);
          return (
            !!x &&
            fields(x, ["id", "status"]) &&
            x.id === ids[i] &&
            listed(["pass", "fail"], x.status)
          );
        })
      )
        return null;
      return v.status === (v.checks.every((c) => row(c)!.status === "pass") ? "pass" : "fail")
        ? v
        : null;
    }
    if (source) {
      if (
        !fields(v, [
          "schema",
          "commandOrdinal",
          "label",
          "status",
          "failureKind",
          "exitCode",
          "timedOut",
          "transport",
          "failureLocations",
          "locationTotal",
          "locationOmitted",
          "diagnosticComplete",
        ]) ||
        v.schema !== "agent-platform.source-command-diagnostic.v1" ||
        !finiteCount(v.commandOrdinal) ||
        v.commandOrdinal < 1 ||
        v.commandOrdinal > 512 ||
        v.label !== "source-command" ||
        v.status !== "fail" ||
        !listed(["nonzero", "timeout", "output-limit", "spawn-or-transport"], v.failureKind) ||
        !finiteExit(v.exitCode) ||
        typeof v.timedOut !== "boolean" ||
        !Array.isArray(v.failureLocations) ||
        v.failureLocations.length > 16 ||
        !v.failureLocations.every((x) => safeLocation(x, knownPaths))
      )
        return null;
      const transport = row(v.transport);
      if (!finiteTransport(transport) || !transport) return null;
      if (v.failureKind === "output-limit" || v.failureKind === "spawn-or-transport") {
        if (transport.failureKind !== v.failureKind || v.diagnosticComplete !== false) return null;
      } else if (v.failureKind === "timeout") {
        if (!v.timedOut || transport.captureComplete !== false || v.diagnosticComplete !== false)
          return null;
      } else if (v.timedOut || v.exitCode === null || v.exitCode === 0) return null;
      if (
        typeof v.diagnosticComplete !== "boolean" ||
        (!(v.locationTotal === null && v.locationOmitted === null && !v.diagnosticComplete) &&
          !(
            finiteCount(v.locationTotal) &&
            finiteCount(v.locationOmitted) &&
            v.locationTotal === v.failureLocations.length + v.locationOmitted &&
            (!v.diagnosticComplete || v.locationOmitted === 0)
          ))
      )
        return null;
      return v;
    }
    if (argv[4] !== "candidate-quality" || lines.length !== 2) return null;
    const diagnostic = row(JSON.parse(lines[1]!));
    if (
      !diagnostic ||
      JSON.stringify(diagnostic) !== lines[1] ||
      !fields(v, ["status", "reportSha256", "checks", "externalAuditSha256"]) ||
      v.status !== "fail" ||
      !digest(v.reportSha256) ||
      !digest(v.externalAuditSha256) ||
      !fields(diagnostic, [
        "schema",
        "driver",
        "status",
        "total",
        "failed",
        "skipped",
        "checks",
        "reportBytes",
        "reportSha256",
        "failureLocations",
        "locationTotal",
        "locationOmitted",
        "diagnosticComplete",
        "retained",
        "reference",
      ]) ||
      diagnostic.schema !== "agent-platform.trusted-quality-diagnostic.v1" ||
      diagnostic.driver !== "candidate-quality" ||
      diagnostic.status !== "fail" ||
      diagnostic.retained !== false ||
      diagnostic.reference !== null ||
      !finiteCount(diagnostic.total) ||
      diagnostic.total > qualityCheckIds.length ||
      !finiteCount(diagnostic.failed) ||
      !finiteCount(diagnostic.skipped) ||
      !finiteCount(diagnostic.reportBytes) ||
      !digest(diagnostic.reportSha256) ||
      diagnostic.reportSha256 !== v.reportSha256 ||
      JSON.stringify(diagnostic.checks) !== JSON.stringify(v.checks) ||
      !Array.isArray(diagnostic.checks) ||
      diagnostic.checks.length !== diagnostic.total ||
      !Array.isArray(diagnostic.failureLocations) ||
      diagnostic.failureLocations.length > 16 ||
      !diagnostic.failureLocations.every((x) => safeLocation(x, knownPaths)) ||
      typeof diagnostic.diagnosticComplete !== "boolean"
    )
      return null;
    const checks = diagnostic.checks.map(row);
    if (
      checks.some(
        (c) =>
          !c ||
          !fields(c, ["id", "status"]) ||
          !listed(qualityCheckIds, c.id) ||
          !listed(["pass", "fail", "skipped"], c.status),
      ) ||
      new Set(checks.map((c) => c!.id)).size !== checks.length ||
      checks.filter((c) => c!.status === "fail").length !== diagnostic.failed ||
      checks.filter((c) => c!.status === "skipped").length !== diagnostic.skipped ||
      diagnostic.failed + diagnostic.skipped === 0
    )
      return null;
    if (
      !(
        diagnostic.locationTotal === null &&
        diagnostic.locationOmitted === null &&
        !diagnostic.diagnosticComplete
      ) &&
      !(
        finiteCount(diagnostic.locationTotal) &&
        finiteCount(diagnostic.locationOmitted) &&
        diagnostic.locationTotal ===
          diagnostic.failureLocations.length + diagnostic.locationOmitted &&
        (!diagnostic.diagnosticComplete || diagnostic.locationOmitted === 0)
      )
    )
      return null;
    return diagnostic;
  } catch {
    return null;
  }
}
