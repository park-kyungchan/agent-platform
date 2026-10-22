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
