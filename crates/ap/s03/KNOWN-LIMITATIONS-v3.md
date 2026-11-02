# S03 reviewed operating limits

This additive clarification records independent review of executable source
`cc5a88b7ced6c1485a666709bd5d5b080c74c753`. It preserves CONTRACT-v1, both frozen
filesystem corpora, command transcripts and their pins. It changes no executable
behavior or expected output. The original author-time static records remain
historical; they do not override later native evidence.

## Writer coordination and the final replacement window

External writer quiescence is required for apply and recovery. A nonempty native
evidence string and `writers_quiesced:true` are caller attestations, not a lock
that excludes vendor processes or editors. Participating ap locks alone do not
establish that precondition.

Topology and target rechecks refuse an ancestor or value change observed before
the final replacement. They cannot prevent a nonparticipating writer from
renaming an ancestor or replacing a target after the last check and before
descriptor-relative rename/unlink. In that window a replacement can reach the
displaced directory. CONTRACT-v1's unconditional renamed-ancestor sentence must
be read under its external-quiescence precondition. More rechecks do not turn
this into a general compare-and-swap guarantee. Refuse native application when
actual required writer coordination has not been established.

## Recovery reports for entries this command never wrote

Recovery safely preserves an entry for which durable state has no staged write
fingerprint. It reports that entry as `recovered` without checking whether its
current bytes equal the original snapshot. For such an entry, the phase means
this command has no remaining rollback action; it does not assert restoration
or validation of the current value. Overall `recovered` is therefore not a
claim that every selected file again contains its original bytes. Consumers
needing current-value validation must perform a fresh bounded plan/inspection.
No recovery report grants ownership of unrelated current text.

## Independent one-MiB limits

The source/target-file, command stdin and serialized durable-state limits are
each 1,048,576 bytes. A file passing the file limit is not guaranteed to fit its
plan/apply request or state: these contain several copies of instruction text,
and JSON escaping may expand it further. There is no single promised maximum
installable file size independent of its contents and selected scope.

Independent review reproduced a 600,006-byte Claude file refused at apply stdin
with exit 2 (`input_too_large`, field `stdin`), and a 400,006-byte file whose plan
fit but whose staged state exceeded the state limit. The latter returned exit 5
(`recovery_required`, error `input_too_large`, field `state`) before any target
rename. The target remained unchanged and explicit recovery succeeded from the
last durable state. These are safe refusals, not evidence that a large file can
be installed. A failure report must be reconciled with its command directory;
it must never be treated as permission to replay an ambiguous application.

## Native evidence and retained integrations

Disposable compiled proof and independent review do not establish global
installation, P6 completion or consumer deletion. On the observed native
profiles, read-only plan/doctor reported the two selectors already correct;
no actual global apply was needed or performed. Such a diagnosis alone is not
fresh-session loading proof.

Fresh Codex native input and a fresh Claude native instruction attachment/API
request independently establish loading through the unchanged native selectors.
Claude elides HTML comments and boundary blank lines; all operative canonical
lines and their order were checked, not claimed byte-identical. Existing
vendor-managed Herdr/Hindsight hooks are retained; doctor executes no hooks.

Muse retains trusted project rules and foreign personal rules. The observed
Claude personal `@/root/.agents/AGENTS.md` import is literal in Muse standing
rules, not automatic expansion of the canonical file; the trusted project rule
explicitly requires reading that file before effects. No ap Muse adapter or
automatic Muse transcript-retention hook is claimed. Antigravity's native
aliases/configuration remain retained; fresh input/deduplication is unverified.
Support deletion and whole-P6 completion remain held until their own consumer
and runtime evidence is complete.
