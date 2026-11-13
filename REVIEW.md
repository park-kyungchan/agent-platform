# Review policy of agent-platform

For the one reviewer session of a pull request (and for the automatic reviewer). Review the head whose required check `rust` passed.

## A finding blocks only when it is one of these, shown by a command, a test or two quoted sentences

1. Wrong behaviour of `ap` or of a script: a request that must be refused is admitted or the reverse; an answer that contradicts `crates/ap/README.md`; an install, a recovery or a close that leaves the host or the store in a state the contract excludes.
2. Loss or exposure of data: an installation that destroys bytes it does not own or keeps no previous bytes; a credential, a private path's content or student data in the tree.
3. A broken contract: `ap` parses an engine answer in a shape the engine revision of `ontology/ENGINE-PIN` does not give; the schema `work` changes its installed meaning; `content/SHA256SUMS`, the inventory and the files disagree.
4. Instructions an agent cannot follow: two sentences of `content/` that contradict each other so that an agent following both cannot act (quote both with file and line); a route to a missing file or section; a sentence that removes a safety rule (holds, never force-push, merge only on the reviewed head, install with readback, privacy limits).
5. An irreversible effect on a real target without a guard.

## Not blocking (at most five notes)

Wording, length, a test gap, a surviving mutant, a name, a preference for another structure. Do not write mutants by hand. A change that deletes the only tests of code that stays says so in the pull request; when it does not, that is a finding of kind 3.

## Form

First line exactly `Verdict: accept` or `Verdict: repair`; second line the head revision; then the blocking findings (each with file, line and what shows it), then the notes.
