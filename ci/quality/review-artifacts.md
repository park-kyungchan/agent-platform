# Historical native independent review artifacts

The native review publisher/controller CLI is retired. Its artifact dispatch,
authenticated pending/status publication and bootstrap procedures are historical;
they are no longer a current review or publication workflow.

For exact protocol interpretation, use the preserved Git bytes at
`87b5cd62b89b8e8d550104f5bf037793f6b89e86:ci/quality/review-artifacts.md` and
`ci/reuse/publish-review-artifact.ts` at that same revision. Those bytes describe
the raw native output, source and reviewer bindings, artifact/run identity,
digests, pending-attempt fence and historical receipt compatibility. Remaining
artifact and receipt validators, helper tests and manual workflows retain their
existing behavior until their own consumers are dispositioned.

Current source changes receive ordinary independent native review of the exact
candidate revision. The Lead evaluates findings and actual check evidence before
separately authorized expected-head integration. This closure creates no new
receipt, controller, metadata or status gate and grants no runtime activation or
user Acceptance.
