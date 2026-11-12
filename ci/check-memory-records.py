#!/usr/bin/env python3
"""Check engineering record originals. Run with python3 -I -B."""
import argparse
import datetime
import hashlib
import json
import re
import sys
import subprocess
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
V2_FIELDS = {"abstract", "horizon", "review_on", "recall_terms", "derived_from", "codified_in"}
ROOT = ("memory-map", "engineering")
PARENTS = {"platform": {"memory-map"}, "host": {"memory-map"}, "user": {"memory-map"},
           "module": {"platform"}, "procedure": {"platform", "module", "host"},
           "task": {"memory-map", "platform", "module", "procedure", "host"},
           "term": {"memory-map", "platform", "module", "procedure"}}
SUBJECT_ID = re.compile(r"[a-z0-9][a-z0-9._-]{0,127}\Z")
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


def iso_date(value):
    try:
        return (isinstance(value, str) and bool(re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", value))
                and bool(datetime.date.fromisoformat(value)))
    except ValueError:
        return False


def source(value):
    if not exact_object(value, ("date", "locator")) or not nonempty(value["locator"]):
        return False
    return iso_date(value["date"])


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def validate(files, nodes=None, publication=False):
    """Shared validator for (path, regular-file bytes or None) inputs; no body in errors."""
    errors, seen, records = [], set(), {}
    for path, raw in files:
        def fail(code, cause):
            errors.append(f"{code}@{path.as_posix()}: {cause}")
        if raw is None or path.suffix != ".md":
            fail("record_path", "use a regular memory/records/<record_id>.md file")
            continue
        try:
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
            json.dumps(header, ensure_ascii=False).encode("utf-8")
            if not isinstance(header, dict):
                raise ValueError("header must be an object")
        except (ValueError, UnicodeError, RecursionError):
            fail("record_header", "start with --- then a JSON object then ---; keys must be unique")
            continue
        version = header.get("schema_version")
        allowed = FIELDS | (V2_FIELDS if type(version) is int and version == 2 else set()) | ({"owner_ref_reason"} if header.get("owner_ref") == "none" else set())
        if set(header) != allowed:
            code = "record_value" if version == 2 and not (set(header) & V2_FIELDS) else "record_fields"
            fail(code, "supply exactly the documented header fields (reason only with none)")
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
        if not (type(version) is int and version in (1, 2)
                and isinstance(kind, str) and kind in SECTIONS and valid_subject and valid_owner
                and nonempty(header["title"]) and nonempty(header["scope"])
                and isinstance(header["sources"], list) and bool(header["sources"])
                and all(source(s) for s in header["sources"])):
            fail("record_value", "use schema_version 1 or 2, closed vocabularies and documented field shapes")
            continue
        if publication and version != 2:
            fail("publication_schema", "publication requires explicit conversion to schema 2")
        if version == 2:
            terms, links, rules = (header[k] for k in ("recall_terms", "derived_from", "codified_in"))
            if not (nonempty(header["abstract"]) and header["horizon"] in ("mid-term", "long-term")
                    and iso_date(header["review_on"])):
                fail("record_value", "use nonempty abstract, mid-term/long-term horizon and ISO review_on")
            if not (isinstance(terms, list) and 1 <= len(terms) <= 8 and all(nonempty(t) for t in terms)
                    and len(set(terms)) == len(terms)):
                fail("recall_terms", "supply 1..8 distinct nonempty literal strings")
            if not (isinstance(links, list) and all(exact_object(x, ("record_id", "evidence_ref"))
                    and nonempty(x["record_id"]) and ID.fullmatch(x["record_id"])
                    and nonempty(x["evidence_ref"]) for x in links)):
                fail("derived_from", "use record_id and nonempty evidence_ref")
            if not (isinstance(rules, list) and all(exact_object(x, ("repository", "path", "commit", "anchor"))
                    and all(nonempty(v) for v in x.values()) and re.fullmatch(r"[0-9a-fA-F]{40}", x["commit"])
                    for x in rules)):
                fail("codified_in", "use repository, path, full commit pin and anchor")
        if len(raw) > 4096:
            fail("record_size", "original exceeds 4096 UTF-8 bytes")
        if (nodes is not None or version == 2) and (nodes is None or (subject["type"], subject["id"]) not in nodes):
            fail("record_subject", "subject must occur in the vocabulary")
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
        if isinstance(rid, str) and ID.fullmatch(rid):
            records[rid] = {"header": header, "body": "\n".join(lines[end + 1:]).encode("utf-8"),
                            "sections": sections, "path": path.as_posix()}
    if not errors:
        errors.extend(check_links(records))
    return records, errors


def check_links(records):
    errors, edges = [], {}
    allowed = {"incident": set(), "lesson": {"incident"}, "pattern": {"incident", "lesson"},
               "direction-proposal": {"pattern", "lesson", "incident"}}
    for rid, record in records.items():
        h = record["header"]
        edges[rid] = [x["record_id"] for x in h.get("derived_from", [])]
        def fail(code, cause):
            errors.append(f"{code}@{record['path']}: {cause}")
        if any(target not in records for target in edges[rid]):
            fail("derived_reference", "derived_from target must be local")
            continue
        kinds = [records[t]["header"]["kind"] for t in edges[rid]]
        if h["kind"] in allowed and any(k not in allowed[h["kind"]] for k in kinds):
            fail("derived_level", "derived_from target kind violates abstraction order")
        if h["schema_version"] == 2 and h["kind"] == "lesson" and not kinds:
            fail("derived_level", "lesson requires an incident")
        if h["schema_version"] == 2 and h["kind"] == "direction-proposal" and "pattern" not in kinds:
            if not any(line.startswith("Critical exception: ") and line[20:].strip()
                       for line in record["sections"].get("Supporting records", [])):
                fail("proposal_without_pattern_or_exception", "supply a pattern or Critical exception line")
    if errors:
        return errors
    # Iterative traversal also handles long migration-input graphs without recursion.
    done, active, roots = set(), set(), {}
    for start in edges:
        stack = [(start, False)]
        while stack:
            rid, leaving = stack.pop()
            if leaving:
                active.remove(rid)
                done.add(rid)
                roots[rid] = ({rid} if records[rid]["header"]["kind"] == "incident" else
                              set().union(*(roots[t] for t in edges[rid])))
            elif rid in active:
                return [f"derived_cycle@{records[rid]['path']}: derived_from must be acyclic"]
            elif rid not in done:
                active.add(rid)
                stack.append((rid, True))
                stack.extend((t, False) for t in edges[rid])
    for rid, record in records.items():
        if record["header"]["schema_version"] == 2 and record["header"]["kind"] == "pattern" and len(roots[rid]) < 2:
            errors.append(f"derived_chains@{record['path']}: pattern requires at least two incident chains")
    return errors


def parse_json(raw):
    if raw.startswith(b"\xef\xbb\xbf"):
        raise ValueError("byte order mark")
    data = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-JSON constant")))

    json.dumps(data, ensure_ascii=False).encode("utf-8")
    return data


def subject_key(value):
    return (exact_object(value, ("type", "id")) and isinstance(value["type"], str)
            and value["type"] in SUBJECTS and isinstance(value["id"], str)
            and bool(SUBJECT_ID.fullmatch(value["id"])))


def vocabulary(raw):
    data, nodes = parse_json(raw), {}
    if not exact_object(data, ("schema_version", "nodes")) or type(data["schema_version"]) is not int or data["schema_version"] != 1 or not isinstance(data["nodes"], list):
        raise ValueError("vocabulary shape")
    for node in data["nodes"]:
        if not (exact_object(node, ("type", "id", "parent", "display_name", "aliases"))
                and subject_key({k: node[k] for k in ("type", "id")}) and nonempty(node["display_name"])
                and isinstance(node["aliases"], list) and all(isinstance(x, str) for x in node["aliases"])
                and len(set(node["aliases"])) == len(node["aliases"])
                and (node["parent"] is None or subject_key(node["parent"]))):
            raise ValueError("node shape")
        key = (node["type"], node["id"])
        if key in nodes:
            raise ValueError("duplicate node")
        nodes[key] = node
    if ROOT not in nodes or nodes[ROOT]["parent"] is not None:
        raise ValueError("root")
    for key in nodes:
        chain = ancestors(key, nodes)
        if len(chain) > 5 or chain[-1] != ROOT:
            raise ValueError("depth or disconnected node")
        if key != ROOT and (key[0] == "memory-map" or nodes[key]["parent"] is None
                            or chain[1][0] not in PARENTS[key[0]]):
            raise ValueError("parent type")
    return nodes


def ancestors(key, nodes):
    chain = []
    while key is not None:
        if key in chain or key not in nodes:
            raise ValueError("cycle or unknown parent")
        chain.append(key)
        parent = nodes[key]["parent"]
        key = (parent["type"], parent["id"]) if parent is not None else None
    return chain


def derived_tags(header, nodes):
    key = (header["subject"]["type"], header["subject"]["id"])
    return ["subject:" + ":".join(key)] + ["within:" + ":".join(x) for x in ancestors(key, nodes)] + [
        "kind:" + header["kind"], "horizon:" + header["horizon"]]


def recall_tags(subject, descendants=False, horizon=None):
    return {"tags": [("within:" if descendants else "subject:") + subject["type"] + ":" + subject["id"]]
            + (["horizon:" + horizon] if horizon is not None else []), "tags_match": "all_strict"}


def publication_bytes(record):
    h = record["header"]
    if h["schema_version"] != 2:
        raise ValueError("publication requires schema 2")
    ordered = {k: h[k] for k in ("title", "subject", "abstract")}
    ordered.update((k, h[k]) for k in sorted(h) if k not in ordered and k != "review_on")
    # Sort nested objects only; the three leading keys have a fixed top-level order.
    def nested(value):
        if isinstance(value, dict):
            return {k: nested(value[k]) for k in sorted(value)}
        return [nested(x) for x in value] if isinstance(value, list) else value
    projected = {k: nested(v) for k, v in ordered.items()}
    return b"---\n" + json.dumps(projected, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n---\n" + record["body"]


def publication_hash(record):
    return hashlib.sha256(publication_bytes(record)).hexdigest()


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, timeout=15).stdout


def regular_bytes(path):
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError("regular file required")
    return path.read_bytes()


def load(root, subjects=None, commit=None, publication=False):
    """Return (file count, validated record map, vocabulary map, errors)."""
    files, nodes = [], None
    error_code, error_path = "record_path", root
    try:
        if commit is not None:
            repo = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
            commit = git(repo, "rev-parse", "--verify", "--end-of-options", commit + "^{commit}").decode().strip()
            paths = (root, subjects) if subjects is not None else (root,)
            if any(p.is_absolute() or ".." in p.parts or p == Path(".") for p in paths):
                raise ValueError("commit paths must be relative")
            entries = {}
            for entry in git(repo, "ls-tree", "-rzt", commit).split(b"\0"):
                if entry:
                    metadata, name = entry.split(b"\t", 1)
                    entries[name.decode("utf-8")] = metadata.decode().split()
            def blob(path):
                for parent in path.parents:
                    if parent != Path(".") and entries.get(parent.as_posix(), [None])[0] != "040000":
                        raise ValueError("non-directory ancestor")
                mode, kind, oid = entries[path.as_posix()]
                if mode not in ("100644", "100755") or kind != "blob":
                    raise ValueError("nonregular blob")
                return git(repo, "cat-file", "blob", oid)
            if root.as_posix() in entries and entries[root.as_posix()][0] != "040000":
                raise ValueError("record root must be a tree")
            for name, (mode, _, _) in entries.items():
                path = Path(name)
                if root in path.parents and mode != "040000":
                    files.append((path, blob(path) if path.parent == root and mode in ("100644", "100755") else None))
            # An absent root is allowed, but symlink/non-directory ancestors are not.
            for parent in root.parents:
                if parent != Path(".") and parent.as_posix() in entries and entries[parent.as_posix()][0] != "040000":
                    raise ValueError("non-directory ancestor")
            error_code, error_path = "subjects", subjects
            subject_raw = blob(subjects) if subjects is not None else None
        else:
            if any(p.is_symlink() or (p.exists() and not p.is_dir()) for p in (root, *root.parents)):
                raise ValueError("record root must be a directory")
            for path in sorted(root.rglob("*")):
                if path.is_symlink() or not path.is_dir():
                    try:
                        raw = regular_bytes(path) if path.parent == root else None
                    except (OSError, ValueError):
                        raw = None
                    files.append((path, raw))
            error_code, error_path = "subjects", subjects
            subject_raw = regular_bytes(subjects) if subjects is not None else None
        if subject_raw is not None:
            nodes = vocabulary(subject_raw)
    except (OSError, ValueError, KeyError, RecursionError, subprocess.SubprocessError):
        return len(files), {}, {}, [f"{error_code}@{error_path}: cannot read regular inputs or validate vocabulary/Git revision"]
    records, errors = validate(sorted(files), nodes, publication)
    return len(files), records if not errors else {}, nodes, errors


def check(root, subjects=None):
    count, _, _, errors = load(root, subjects)
    return count, errors


def tree_view(records, nodes):
    used = set()
    for record in records.values():
        h = record["header"]
        if h["schema_version"] == 2:
            used.update(ancestors((h["subject"]["type"], h["subject"]["id"]), nodes))
    return [{**nodes[k], "depth": len(ancestors(k, nodes)) - 1} for k in sorted(used)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, help="explicit legacy/migration input directory")
    parser.add_argument("--subjects", type=Path, help="subject vocabulary (default: memory/subjects.json)")
    parser.add_argument("--commit", help="read only Git objects at this commit")
    views = parser.add_mutually_exclusive_group()
    views.add_argument("--tree", action="store_true", help="emit admitted subjects and ancestors as JSON")
    views.add_argument("--due", help="emit review metadata due on or before YYYY-MM-DD as JSON")
    args = parser.parse_args()
    if args.due is not None and not iso_date(args.due):
        parser.error("--due requires a real ISO date")
    base = Path(".") if args.commit else Path(__file__).resolve().parents[1]
    root = args.root if args.root is not None else base / "memory/records"
    subjects = args.subjects if args.subjects is not None else (base / "memory/subjects.json" if args.root is None else None)
    count, records, nodes, errors = load(root, subjects, args.commit, publication=args.root is None)
    if errors:
        print("\n".join(errors))
        return 1
    if args.tree:
        print(json.dumps(tree_view(records, nodes), ensure_ascii=False, sort_keys=True))
    elif args.due:
        print(json.dumps([{"record_id": rid, "review_on": r["header"]["review_on"], "horizon": r["header"]["horizon"]}
                          for rid, r in sorted(records.items()) if r["header"]["schema_version"] == 2
                          and r["header"]["review_on"] <= args.due], sort_keys=True))
    else:
        print(f"memory records: {count} files, 0 errors")
    return 0


if __name__ == "__main__":
    sys.exit(main())
