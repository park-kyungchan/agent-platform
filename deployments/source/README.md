# Immutable source distribution and native binding

The approved target map is the `immutableSourceSuccessor` section of
`architecture/runtime-filesystem-contract.json`. Its existing v1 paths remain
current exceptions until their native owners qualify activation. This source
slice does not move native homes, Hermes data, Herdr backing, code-server,
credentials, the Hermes updater or its lock.

Use `@palantirkc/work-continuity/source/v1`. Root owns the exact package export,
registry/definition pins and provider/marker/context successors. The canonical
implementation is under `packages/work-continuity/src/coordination`; native Codex
and Hermes adapters call it through separate explicit exports. Historical entry
points and finite v5/v6 CI projections are unchanged.

A native source owner supplies a selected Git snapshot and exclusive destination
lease to `materializeSourceDistribution`. The existing native Git owner independently supplies selected immutable blobs for
every selected-tree member, including files labeled dependency/tool. An explicit
`null` means the native Git owner actually observed selected-tree absence; an
unknown/error does not select an external fallback. Dirty checkout hashes cannot
be relabeled as a commit. Dependency files must belong to exactly one declared
`node_modules/<package-name>` root and package manifest; tool files must match an
explicit `tools/` image entry. Orphan/foreign classifications reject before writes.
For genuinely external files the native snapshot lease additionally supplies
`readInstalled`: actual package/tool identity, complete inventory digest, exact
immutable bytes and independently authenticated installation provenance. Its
receipt digest joins the fixed sorted `[path, sha256, bytes, mode, class]` inventory
tuples, reproducible with `sourceInstalledInventoryDigest`. This native callback
must observe its installation, never echo caller JSON. Every ownership/blob and
manifest join is checked before creating an output stage and again during copy.
Every input file/tool/dependency is
explicitly classified and hash/size pinned. The writer copies only those bytes,
rechecks the source, writes an exclusive immutable digest directory and returns
source evidence. It never installs dependencies, changes refs/current pointers,
activates code or publishes. Existing final/staging output requires reconciliation;
it is never removed or silently replayed. Snapshot and destination writer fencing
remain native-owner obligations, not a JSON lock or a hostile-process sandbox.

`SourceContentBinding` pins repository, full commit/tree, exact files, entrypoints,
installed dependency identities and tool images. `SourceLocationBinding` separately
pins concrete physical root, observed host/mount namespace/UID/GID, authenticated
profile/role and readonly source policy. A native read lease independently selects
that exact tuple and observed loader images. Neither the descriptor nor a directory
name establishes permission, native identity or adoption.

`withQualifiedSource` checks the selected tuple, registration bytes, whole immutable
file inventory, modes, realpaths and hashes before reading/loading. It rejects
undeclared executable/package edges, unsafe paths, symlinks, package escapes and
unqualified cached imports. A load rechecks the full source before and after import;
consumption rechecks before returning. A changed loaded image requires a fresh
process. There is no cache-clearing API. Unsupported computed loaders must first
be built/qualified through an explicit finite owner bundle; they cannot be accepted
by relabeling an old source closure.

The direct-load proof names `restricted-esm-expression.v1`. The whole original
module is parsed before any compiler/transpiler/macro operation, under a deliberately restricted grammar: static ESM imports and
reexports, const declarations, expression-returning named functions, literal data,
lexical references, named members, calls and arithmetic/conditional expressions.
There are no ambient globals, computed members, templates, mutation statements,
reflection, TypeScript-only syntax, dynamic imports (including literal-prefix expressions), CommonJS or
loader factories. Only `node:path` (including posix/win32) and `node:url` are builtin
imports on this route. Aliased createRequire, VM/worker/Bun loaders and arbitrary
package capabilities reject before evaluation, including already cached outside
modules. Installed packages must have one ESM route; conditional exports,
imports/browser/module remapping and more general executable syntax require a
separately qualified finite owner bundle. This API does not pretend that a regex
and scanImports establish an arbitrary JavaScript closure, and is not an OS
sandbox or a general native runtime admission. Static declared source and fully
declared external ESM dependencies remain usable; source/native adoption must
qualify its actual needed syntax/bundle route separately. Cache/first-import
refusal and the native source/current/original-recovery lease remain mandatory.

The native owner must bootstrap this verifier from its already pinned trusted source
image before importing a candidate package. `source/build-v1` compiles the fixed
builtin-only entry into a standalone projection with exact compiler/input/metafile
provenance; no hand-maintained generated guard is introduced. This build is allowed
only in the qualified source runner or separately admitted native build owner.
That owner's lease must independently cover the exact exclusive output writer
and parent namespace through readback/publication. Before mkdir the builder opens
every physical output ancestor without symlinks, holds the parent FD, creates only
an absent child and pins its directory identity. It checks parent/child identities
before the compiler and through proof readback. The compiler writes via the
parent process's held directory FD under procfs, so replacing the name cannot
redirect writes to an alias target; unsupported procfs access refuses without a
path fallback. Existing, incomplete and uncertain outputs remain for exact owner
reconciliation, with no deletion or automatic retry. Compiler output is a new
projection of canonical source, never a second maintained guard.
The original native wrapper supplies the current source-read owner and immutable
bootstrap pin. A Work/data path, JSON flag or dependency export cannot supply it.

`prepareSourceProviderSuccessors` emits a complete three-provider registration and
marker CAS proposal for one canonical content binding and independently captured
native locations. Root installs it with the matching explicit owner loader, source
export and first-import pins. Old parsers must reject the new version. Source
successors are not installed-native/adoption receipts. Capture the actual owning
namespace and principal; do not derive Hermes container paths from a Host path.

Current reads track the actual current provider generation. Recovery binds the
original immutable source, location, registration and actual original owner receipt.
A later main/current-provider does not replace those references. Current native
permission still applies. Source rollback selects the previous qualified source
through its owner while retaining all new data and completed/uncertain effects.

The command libraries `scripts/source-distribution.ts` and `source-binding.ts`
require native owner objects supplied in process; standalone JSON cannot invent
one. H20-H22/H33-H34/H41-H42, primary Codex/Hermes C3 and later Academy remain actual
native acceptance gates. No source test changes R&D/Ontology/Wiki/measurement,
original/decision/effect custody or the full S05-S08 outcome.
