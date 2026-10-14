type InputFailure =
  "REQUEST_TOO_LARGE" | "REQUEST_READ_FAILED" | "INVALID_REQUEST_JSON" | "INVALID_REQUEST_OBJECT";
export class InputError extends Error {
  constructor(readonly code: InputFailure) {
    super(code);
  }
}

/** All request routes share a byte bound. Parser exceptions and stdin never reach stderr. */
export async function readJsonRequest(requestMaxBytes = 1048576): Promise<Record<string, unknown>> {
  const buffer = Buffer.allocUnsafe(requestMaxBytes);
  let bytes = 0;
  try {
    for await (const chunk of Bun.stdin.stream()) {
      if (chunk.byteLength > requestMaxBytes - bytes) throw new InputError("REQUEST_TOO_LARGE");
      buffer.set(chunk, bytes);
      bytes += chunk.byteLength;
    }
  } catch (error) {
    if (error instanceof InputError) throw error;
    throw new InputError("REQUEST_READ_FAILED");
  }
  let request: unknown;
  try {
    request = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, bytes)),
    );
  } catch {
    throw new InputError("INVALID_REQUEST_JSON");
  }
  if (!request || typeof request !== "object" || Array.isArray(request))
    throw new InputError("INVALID_REQUEST_OBJECT");
  return request as Record<string, unknown>;
}
