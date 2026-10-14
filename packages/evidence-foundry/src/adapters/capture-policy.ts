const credentialHeaders = new Set([
  "authorization",
  "proxy-authorization",
  "cookie",
  "set-cookie",
  "x-api-key",
  "api-key",
]);
/** Only header values are redacted. Response content bytes remain original evidence. */
export function redactCaptureMetadata(value: unknown, parent = ""): unknown {
  if (Array.isArray(value)) return value.map((v) => redactCaptureMetadata(v, parent));
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [
        key,
        /headers/i.test(parent) && credentialHeaders.has(key.toLowerCase())
          ? "[REDACTED]"
          : redactCaptureMetadata(nested, key),
      ]),
    );
  return value;
}
