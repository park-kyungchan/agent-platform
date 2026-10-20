/** One-shot transport: the owner binds the contract in reviewed compiler bytes, never stdin. */
import {
  deliverHermesNamespace,
  namespaceDeliveryLimits,
  type NamespaceDeliveryContract,
} from "./delivery.ts";

declare const HERMES_NAMESPACE_DELIVERY_CONTRACT: NamespaceDeliveryContract | undefined;

export async function runNativeNamespaceDelivery() {
  if (typeof HERMES_NAMESPACE_DELIVERY_CONTRACT === "undefined")
    throw Error("COMPILED_NAMESPACE_DELIVERY_CONTRACT_REQUIRED");
  if (Bun.argv.length !== 2) throw Error("NAMESPACE_DELIVERY_ARGUMENTS_FORBIDDEN");
  const reader = Bun.stdin.stream().getReader(),
    chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    while (true) {
      const part = await reader.read();
      if (part.done) break;
      bytes += part.value.length;
      if (bytes > namespaceDeliveryLimits.wireBytes) throw Error("NAMESPACE_DELIVERY_WIRE_LIMIT");
      chunks.push(part.value);
    }
  } finally {
    reader.releaseLock();
  }
  const payload: unknown = JSON.parse(
    new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)),
  );
  const receipt = await deliverHermesNamespace(HERMES_NAMESPACE_DELIVERY_CONTRACT, payload);
  process.stdout.write(JSON.stringify(receipt) + "\n");
  process.exitCode =
    receipt.status === "delivered-awaiting-owner-verification" && !receipt.persistenceFailure
      ? 0
      : 1;
}

if (import.meta.main) {
  try {
    await runNativeNamespaceDelivery();
  } catch {
    process.stdout.write(
      JSON.stringify({
        schema: "hermes.namespace-delivery-entry-held.v1",
        status: "held",
        nativeTaskId: null,
        nativeRunId: null,
        nativeEventId: null,
        nativeAttemptId: null,
        nativePermissionGranted: false,
        nativeActivation: false,
        retryAllowed: false,
      }) + "\n",
    );
    process.exitCode = 1;
  }
}
