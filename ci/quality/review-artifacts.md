# Native independent review artifacts

Consumer target: agent-platform's exact-head native review job and owner-scoped
source-projected gates, including education-platform. Baseline is AP
`f6649663fb78261de2499f0920306c32f5e22f1f`: immutable repository receipt/result
files, with the owner consumer requiring source/raw/evidence/receipt ancestry.

Goal: preserve a real independent native review and exact source/publisher
bindings while allowing unchanged raw output to travel as one CI artifact.
CI transports and verifies evidence; it cannot create native review observations.
No automatic merge, credential provisioning, native activation or Acceptance.

Validation: valid externally produced zero-finding raw output must pass; wrong
source, author/reviewer identity, publisher, run, attempt, repository, workflow,
artifact, digest, archive member, current hold and changed latest status must
refuse. Existing receipt controls continue to pass. Focused tests and complete
source/quality checks precede a fresh read-only native exact-head review under the user-approved reviewer policy.
The 2026-10-28 temporary user direction admits gpt-6.1-sol/high with distinct
native author/reviewer threads; it changes no active/global model settings.
Hosted transport must be observed before claiming operational completion.

Allowed treatment: CI review transport, source policy/tests/projections, shared
action runtime resolution and source CI Bun pins. Preserve domain source,
qualified generations, releases and the protected trusted-ci-v1 policy. That
historical policy's bootstrap version is independent of the active source CI.

Limits and decision: one source candidate per independent review round; repair
located defects without replaying uncertain external effects. Missing hosted
permissions or native originals defer the affected adoption, never weaken trust.
Propose delivery only after the declared controls pass; the Lead merges an
explicit expected head and P8 remains the user's Acceptance gate.

## Publication

A fresh independent native reviewer emits `native-independent-review-output.v1`
(AP) or `owner-scoped-native-review-output.v1` (owner projection), with complete
review status, exact binding/head/tree/manifest, distinct author/reviewer task
identities, `conclusion: pass`, `unresolvedFindings: 0` and `findings: []`.
The native parent keeps those bytes unchanged. AP's native publisher runs:

```sh
bash <trusted-common>/ci/reuse/review-pr.sh <repository> <head> <reviewer-runtime> <controller-sha>
```

The helper checks existing publisher authentication, committed source-author
identity and actual GitHub source. It dispatches the evidence-only workflow,
dispatches only the default main workflow, checks the successful external run
and requires its artifact bytes to equal the native original before status
publication and on final readback, then posts
its authenticated status with the artifact's immutable ID and archive digest.
No model runs in Actions and no CI token receives status or dispatch permission.
Uncertain dispatch/publication is reconciled by the printed request/run/artifact
IDs before any further action. Artifact expiry/deletion fails closed; retain the
native raw file for a fresh external publication when required.

Bootstrap the workflow through a reviewed legacy-receipt PR first: GitHub must
know the workflow on the default branch before dispatch. Existing immutable
receipt and explicit identical-tree successor paths remain supported. Owner
consumers project these shared validators and retain their fixed owner policy.

GitHub documents artifact IDs, archive digests and ZIP downloads in its
[artifact REST API](https://docs.github.com/en/rest/actions/artifacts), and run
actors/attempts in its [workflow run API](https://docs.github.com/en/rest/actions/workflow-runs).
Sources retrieved with verified TLS on 2026-10-28; bounded extracts are retained
in the operations lane-B source directory.

The native controller is selected from a separate immutable reviewed checkout.
An owner-created draft may establish authenticated PR provenance before review;
all real review/evidence/CI/thread gates are required before ready-for-review.
The pinned private-owner policy refuses untrusted contributor/head-repository
provenance and unsupported unassociated heads before subject data or native
review. Its temporary native bypass has no OS credential-isolation claim. Source
checkouts, config, hooks, JavaScript and package scripts are never executed by
the controller; subjects enter through a fresh Git-object data view.
