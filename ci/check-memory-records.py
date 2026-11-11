#!/usr/bin/env python3
"""Check engineering record originals. Run with python3 -I -B."""
import datetime
import json
import re
import sys
from pathlib import Path

SECTIONS = {
    "rationale": ("Decision reference", "Question", "Reasoning", "Tradeoffs", "Alternatives", "Assumptions", "Counterevidence", "Revisit trigger"),
    "incident": ("Occurrence", "Expected and actual behavior", "Boundary", "Impact", "Cause", "Mitigation", "Verification"),
    "lesson": ("Incident references", "Prevention rule", "Conditions and exclusions", "Why it worked", "Counterexample", "Verification"),
    "pattern": ("Independent occurrences", "Common mechanism", "Competing explanation", "Scope limits", "Codification candidate", "Recurrence evidence"),
    "direction-proposal": ("Supporting records", "Contradicted assumption", "Alternatives", "Expected effects", "Reversible trial", "Acceptance and stop conditions", "Decision owner", "Proposal reference"),
    "user-profile": ("Preference", "Explicit user evidence", "Applicability", "Valid from", "Revisit trigger", "Confidence"),
    "glossary": ("Term", "Canonical glossary reference", "Explanation", "Concrete input and executor", "Result", "Explanation evidence", "Scope", "User feedback"),
}
SUBJECTS = {"platform", "module", "host", "task", "procedure", "user", "term", "memory-map"}
FIELDS = {"schema_version", "record_id", "kind", "title", "subject", "scope", "owner_ref", "sources"}
ID = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def exact_object(value, keys):
    return isinstance(value, dict) and set(value) == set(keys)


def object_id(value):
    return (exact_object(value, ("object_type", "namespace", "native_key"))
            and nonempty(value["object_type"]) and nonempty(value["native_key"])
            and isinstance(value["namespace"], dict) and bool(value["namespace"])
            and all(nonempty(k) and nonempty(v) for k, v in value["namespace"].items()))


def source(value):
    if not exact_object(value, ("date", "locator")) or not nonempty(value["locator"]):
        return False
    date = value["date"]
    if not isinstance(date, str) or not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", date):
        return False
    try:
        datetime.date.fromisoformat(date)
        return True
    except ValueError:
        return False


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def check(root):
    errors, seen = [], set()
    if root.exists() and not root.is_dir():
        return 0, [f"record_path@{root.as_posix()}: record root must be a directory"]
    files = sorted(p for p in root.rglob("*") if p.is_symlink() or not p.is_dir())
    for path in files:
        def fail(code, cause):
            errors.append(f"{code}@{path.as_posix()}: {cause}")
        if path.is_symlink() or path.suffix != ".md" or path.parent != root:
            fail("record_path", "use a regular memory/records/<record_id>.md file")
            continue
        try:
            raw = path.read_bytes()
            if raw.startswith(b"\xef\xbb\xbf"):
                raise UnicodeError("byte order mark")
            text = raw.decode("utf-8")
        except (OSError, UnicodeError):
            fail("record_encoding", "file must be readable UTF-8 without a byte order mark")
            continue
        if "\r" in text:
            fail("record_carriage_return", "use line feed only; carriage returns are forbidden anywhere")
            continue
        lines = text.split("\n")
        try:
            if not lines or lines[0] != "---":
                raise ValueError("missing opening delimiter")
            end = lines.index("---", 1)
            header = json.loads("\n".join(lines[1:end]), object_pairs_hook=unique_pairs,
                                parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-JSON constant")))
            if not isinstance(header, dict):
                raise ValueError("header must be an object")
        except (ValueError, RecursionError):
            fail("record_header", "start with --- then a JSON object then ---; keys must be unique")
            continue
        if isinstance(header.get("record_id"), str) and header.get("supersedes") == header["record_id"]:
            fail("memory_successor_is_self", "a record cannot name itself as its predecessor")
            continue
        allowed = FIELDS | ({"owner_ref_reason"} if header.get("owner_ref") == "none" else set())
        if set(header) != allowed:
            fail("record_fields", "supply exactly the documented header fields (reason only with none)")
            continue
        rid, kind, subject, owner = (header[k] for k in ("record_id", "kind", "subject", "owner_ref"))
        if isinstance(rid, str) and rid in seen:
            # A duplicate necessarily also has a different filename; report its
            # identity collision once rather than adding that secondary error.
            fail("duplicate_record_id", "record_id must be unique across files")
        elif not isinstance(rid, str) or not ID.fullmatch(rid) or rid != path.stem:
            fail("record_id", "record_id must match the filename and use lowercase letters, digits and hyphens")
        else:
            seen.add(rid)
        valid_owner = (owner == "none" and nonempty(header.get("owner_ref_reason"))) or object_id(owner)
        if exact_object(owner, ("repository", "path")):
            valid_owner = all(nonempty(owner[k]) for k in owner)
        valid_subject = (exact_object(subject, ("type", "id")) and isinstance(subject["type"], str)
                         and subject["type"] in SUBJECTS and nonempty(subject["id"]))
        if valid_subject and kind in ("user-profile", "glossary"):
            valid_subject = subject["type"] == ("user" if kind == "user-profile" else "term")
        if not (type(header["schema_version"]) is int and header["schema_version"] == 1
                and isinstance(kind, str) and kind in SECTIONS and valid_subject and valid_owner
                and nonempty(header["title"]) and nonempty(header["scope"])
                and isinstance(header["sources"], list) and bool(header["sources"])
                and all(source(s) for s in header["sources"])):
            fail("record_value", "use schema_version 1, closed kind/subject vocabularies and documented field shapes")
            continue
        sections, current, fenced = {}, None, None
        for line in lines[end + 1:]:
            fence = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
            if fence:
                marker, tail = fence.groups()
                if fenced is None:
                    fenced = marker
                elif marker[0] == fenced[0] and len(marker) >= len(fenced) and not tail.strip():
                    fenced = None
                if current is not None:
                    sections[current].append(line)
                continue
            heading = re.match(r"^## (.+?)\s*#*\s*$", line) if fenced is None else None
            if heading:
                current = heading[1]
                if current in sections:
                    fail("record_sections", "second-level headings must be unique")
                sections.setdefault(current, [])
            elif current is not None:
                sections[current].append(line)
        for name in SECTIONS[kind]:
            if not "\n".join(sections.get(name, [])).strip():
                fail("record_sections", f"missing or empty required second-level section: {name}")
    return len(files), errors


def main():
    root = Path(sys.argv[1]) if len(sys.argv) == 2 else Path(__file__).resolve().parents[1] / "memory/records"
    count, errors = check(root)
    for error in errors:
        print(error)
    if not errors:
        print(f"memory records: {count} files, 0 errors")
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(main())
