/** Explicit immutable-source successor. A native wrapper supplies its actual
 * current owner; selected Work/data paths cannot select executable source. */
import {
  normalizeBinding,
  withQualifiedSource,
  type SourceBindingInput,
  type NativeSourceReadOwner,
  type QualifiedSourceView,
} from "@palantirkc/work-continuity/source/v1";
export async function withCodexSource<T>(
  binding: SourceBindingInput,
  owner: NativeSourceReadOwner,
  consume: (
    modules: Readonly<Record<string, Readonly<Record<string, unknown>>>>,
    source: QualifiedSourceView,
  ) => T | Promise<T>,
): Promise<T> {
  binding = normalizeBinding(binding);
  if (!["codex-context", "codex-native-source"].includes(binding.registration.id))
    throw Error("CODEX_SOURCE_PROVIDER_REQUIRED");
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
