# ont-core

Consumers: `ont-store`, the `ont` command and domain crates that pin this repository.
Public contract: Id/Namespace/Scope, observation knowledge/lineage, opaque Session,
Authority/Admission, Snapshot and restricted Query/Response (`Get`, `Traverse`, `Decide`). No internal-platform
dependency, host user mapping, transport or Academy parser.
Read and derive grants are independently required. Service keeps Admission alive
through serialization; raw Snapshot and Authority construction are trusted APIs.

`Scope` is a closed `Education { namespace, class_key, student_key }` or
`Program { namespace, program_key }` enum. `Scope::education` and `Scope::program`
validate nonempty exact keys and the same namespace bounds as `Id`; `namespace()`
borrows the namespace for either variant. No whitespace or UTF-8 normalization is
performed. Education JSON retains namespace/class_key/student_key order; Program
JSON uses namespace/domain/program_key order with `domain: "program"`. Decoding
rejects duplicate, mixed, unknown, missing, null or invalid fields. Grants compare
the entire enum value and object type. This is a Rust source API change requiring
consumers to recompile; legacy Education wire bytes remain compatible. Existing
schema-v1 stores refuse Program intake; Program storage is a separate change.
