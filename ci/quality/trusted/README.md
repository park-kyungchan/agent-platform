# Retired trusted source controller

The current source no longer exposes the manual `trusted-source.yml` workflow, its `controller.ts` entry point or the exclusive `buildPolicy` bootstrap and test. Its unused caller and policy example templates are also retired. Default source and quality checks use `bash ci/check-rust-source.sh`; ordinary native Git and independent Lead review own source integration.

The complete predecessor workflow, controller, bootstrap, documentation and original qualification artifacts remain readable at `00ff3eefb3948ba669031ca08789f118eea30d87`. This source retirement does not delete that history, immutable policy releases or native profile data. It does not prove that an old remote tag cannot be dispatched; no workflow dispatch or remote activation change is performed here.

Retained helpers still have separate source consumers. `release.ts` keeps `fileInventory`, `readPolicyMinimum` and `verifyInstalledPolicyInputs` for existing native/semantic readers. The bounded process helper is also used outside this retired controller. Their presence does not require a new policy release, coverage renewal, source-author record, receipt or controller run. They must receive their own consumer closure before removal.

`SCENARIOS.md`, coverage, catalogues, fixtures and other predecessor records describe historical finite qualification work. They grant no current execution or acceptance authority. No legacy aggregate result, full S04/P6 completion, native activation or production effect is claimed by this retirement.
