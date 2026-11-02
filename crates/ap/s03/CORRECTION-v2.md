# Additive S03 conformance correction v2

No candidate behavior has been executed. Static sequence review found v1's
`recovery-killed-after-rename` expected `[recovering,applied]` contradicted the
contract's reverse-order recovery. For ordered Codex,Claude entries, death after
the FIRST recovery rename leaves `[applied,recovering]`. v2 changes only that
internal fault expectation and the version marker; all 41 CLI filesystem cases
remain byte-equivalent as JSON values. v1 bytes and pins are preserved.

Independent Wonder must review this correction before acceptance. The v2 judge
uses the same independent literal comparison and 13 controls; it does not use
candidate outputs to author expectations. The compiled Rust internal test consumes
v2's literal fault vectors. Source now also persists a staged inode fingerprint
before replacement to distinguish an interrupted own rename from an unrelated
same-value replacement. No-write-intent entries have no rollback authority.
