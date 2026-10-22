/** Source-only successor; legacy v1/v2 generators/guards stay immutable contracts.
 * No sibling discovery, native profile selection, activation or data reads. */
import {
  normalizeBinding,
  withQualifiedSource,
  type SourceBindingInput,
  type NativeSourceReadOwner,
  type QualifiedSourceView,
} from "@palantirkc/work-continuity/source/v1";
export async function withHermesSource<T>(
  binding: SourceBindingInput,
  owner: NativeSourceReadOwner,
  consume: (
    modules: Readonly<Record<string, Readonly<Record<string, unknown>>>>,
    source: QualifiedSourceView,
  ) => T | Promise<T>,
): Promise<T> {
  binding = normalizeBinding(binding);
  if (binding.registration.id !== "hermes-context-source")
    throw Error("HERMES_SOURCE_PROVIDER_REQUIRED");
  return withQualifiedSource(binding, owner, async (source) => {
    const modules: Record<string, Readonly<Record<string, unknown>>> = Object.create(
      null,
    ) as Record<string, Readonly<Record<string, unknown>>>;
    for (const entry of binding.registration.entries)
      modules[entry.role] = await source.load(entry.entry);
    await source.verify();
    const result = await consume(Object.freeze(modules), source);
    await source.verify();
    return result;
  });
}
