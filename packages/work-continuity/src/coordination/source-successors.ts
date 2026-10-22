/** Source-only paired proposals. Root serializes their CAS/application; emitting
 * a marker or a registration is not native adoption or a currentness receipt. */
import {
  normalizeBinding,
  providerBytes,
  providerDigest,
  contentDigest,
  locationDigest,
  sourceHash,
  sourceAbsolutePath,
  type SourceBindingInput,
  type SourceProviderId,
} from "./source-contract.ts";
export interface ExistingProviderSource {
  id: SourceProviderId;
  registration: { path: string; sha256: string };
  markers: readonly { path: string; sha256: string; utf8: string }[];
}
export function prepareSourceProviderSuccessors(
  inputs: readonly SourceBindingInput[],
  before: readonly ExistingProviderSource[],
) {
  const ids: SourceProviderId[] = ["codex-context", "codex-native-source", "hermes-context-source"];
  if (
    inputs.length !== 3 ||
    before.length !== 3 ||
    ids.some(
      (id) =>
        inputs.filter((row) => row.registration.id === id).length !== 1 ||
        before.filter((row) => row.id === id).length !== 1,
    )
  )
    throw Error("SOURCE_EXACT_THREE_PROVIDER_PAIR_REQUIRED");
  const bindings = inputs.map(normalizeBinding);
  if (new Set(bindings.map((binding) => contentDigest(binding.content))).size !== 1)
    throw Error("SOURCE_PAIRED_CANONICAL_CONTENT_REQUIRED");
  return {
    schema: "common.source-provider-successor-set.v1" as const,
    sourceOnly: true as const,
    authority: "none" as const,
    nativePermissionGranted: false as const,
    adoptionEstablished: false as const,
    providers: ids.map((id) => {
      const binding = bindings.find((row) => row.registration.id === id)!,
        old = before.find((row) => row.id === id)!;
      if (
        sourceAbsolutePath(old.registration.path) !== binding.registrationPath ||
        !/^[a-f0-9]{64}$/u.test(old.registration.sha256) ||
        !old.markers.length ||
        new Set(old.markers.map((row) => row.path)).size !== old.markers.length
      )
        throw Error("SOURCE_EXACT_PREDECESSOR_PAIR_REQUIRED");
      const bytes = providerBytes(binding.registration),
        digest = providerDigest(binding.registration);
      return {
        id,
        namespace: binding.location.identity,
        immutableLocation: binding.location.root,
        registration: {
          path: binding.registrationPath,
          expectedSha256: old.registration.sha256,
          nextSha256: digest,
          utf8: bytes,
        },
        markers: old.markers.map((marker) => {
          sourceAbsolutePath(marker.path);
          if (sourceHash(marker.utf8) !== marker.sha256)
            throw Error("SOURCE_MARKER_PREDECESSOR_MISMATCH");
          const occurrences = marker.utf8.split(old.registration.sha256).length - 1;
          if (occurrences !== 1) throw Error("SOURCE_EXACT_REGISTRATION_MARKER_REQUIRED");
          const utf8 = marker.utf8.replace(old.registration.sha256, digest);
          return {
            path: marker.path,
            expectedSha256: marker.sha256,
            nextSha256: sourceHash(utf8),
            utf8,
          };
        }),
        contentDigest: contentDigest(binding.content),
        locationDigest: locationDigest(binding.location),
        binding,
        consumerRequirement:
          "Install the separately reviewed explicit source/v1 native-owner loader with this pair; old v1-v6 parsers must refuse the new registration. Never repin an old parser or clear its cache to claim adoption.",
      };
    }),
  };
}
