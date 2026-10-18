/** Closed bootstrap diagnostics: never inspect an exception or include request/source bytes. */
const reasons = {
  input: "REQUEST_UNAVAILABLE_OR_INVALID",
  registration: "EXPLICIT_REGISTRATION_REQUIRED",
  source: "SOURCE_CLOSURE_UNAVAILABLE_OR_DRIFTED",
  bindings: "BINDINGS_UNAVAILABLE_OR_INVALID",
  "source-readback": "SOURCE_CLOSURE_READBACK_FAILED",
  "bindings-readback": "BINDINGS_READBACK_FAILED",
  implementation: "QUALIFIED_IMPLEMENTATION_IMPORT_FAILED",
  context: "CONTEXT_PROJECTION_FAILED",
  memory: "MEMORY_QUERY_FAILED",
} as const;

export type CurrentWorkDiagnosticStage = keyof typeof reasons;
export function currentWorkDiagnostic(
  error:
    | "NATIVE_CURRENT_WORK_INPUT_OR_SOURCE_HELD"
    | "CURRENT_WORK_INPUT_OR_SOURCE_FAILED"
    | "MEMORY_INPUT_OR_SOURCE_FAILED",
  stage: CurrentWorkDiagnosticStage,
) {
  return { error, stage, reason: reasons[stage] };
}
