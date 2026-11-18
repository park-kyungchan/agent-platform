#!/usr/bin/env python3
import json
import re
import subprocess
import sys
from pathlib import Path

NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
ENTRY_RE = re.compile(r"^(.+):([0-9]+)-([0-9]+)$")


def tracked_files(root):
    try:
        out = subprocess.run(
            ["git", "-c", "core.quotePath=false", "ls-files", "-z"],
            cwd=str(root),
            capture_output=True,
            check=True,
        ).stdout
    except Exception:
        return None
    return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]


def bad_path(p):
    if not isinstance(p, str) or p == "":
        return True
    if "*" in p or p.startswith("/"):
        return True
    parts = p.split("/")
    if ".." in parts:
        return True
    return False


def path_missing(p, tracked):
    if p.endswith("/"):
        return not any(t.startswith(p) for t in tracked)
    return p not in tracked


def covers(paths, f):
    for p in paths:
        if p.endswith("/"):
            if f.startswith(p):
                return True
        elif f == p:
            return True
    return False


def line_count(root, rel):
    try:
        data = (root / rel).read_bytes()
    except OSError:
        return None
    if not data:
        return 0
    # Last line counts even without trailing newline.
    return data.count(b"\n") + (0 if data.endswith(b"\n") else 1)


def main(argv):
    roots = argv[1:]
    if not roots:
        print("usage: check-map.py <root>...")
        return 2
    root = Path.cwd()
    tracked = tracked_files(root)
    if tracked is None:
        print("map_unreadable git ls-files failed")
        return 1
    tracked_set = set(tracked)
    faults = []

    # Roots validation.
    for r in roots:
        if bad_path(r):
            faults.append(f"path_missing {r}")
        elif path_missing(r, tracked_set):
            faults.append(f"path_missing {r}")

    # Load map.
    map_path = root / "map.json"
    try:
        raw = map_path.read_bytes()
        data = json.loads(raw.decode("utf-8"))
    except Exception as e:
        print(f"map_unreadable {e}")
        return 1
    if not isinstance(data, dict) or not isinstance(data.get("components"), list):
        print("component_invalid components")
        return 1
    components = data["components"]
    if set(data.keys()) - {"components"}:
        # Extra top-level keys are not specified; treat as invalid component shape.
        pass

    names = []
    all_paths = []
    for idx, comp in enumerate(components):
        label = f"components[{idx}]"
        if not isinstance(comp, dict):
            faults.append(f"component_invalid {label}")
            continue
        name = comp.get("name")
        if not isinstance(name, str) or not NAME_RE.match(name):
            faults.append(f"component_invalid {label}.name")
        else:
            names.append(name)
        purpose = comp.get("purpose")
        if not isinstance(purpose, str) or purpose == "" or len(purpose) > 160:
            faults.append(f"component_invalid {label}.purpose")
        paths = comp.get("paths")
        if not isinstance(paths, list) or len(paths) == 0 or not all(isinstance(x, str) for x in paths):
            faults.append(f"component_invalid {label}.paths")
            paths = []
        entry = comp.get("entry")
        if not isinstance(entry, list) or not all(isinstance(x, str) for x in entry):
            faults.append(f"component_invalid {label}.entry")
            entry = []
        for key in ("spec", "tests"):
            v = comp.get(key)
            if not isinstance(v, list) or not all(isinstance(x, str) for x in v):
                faults.append(f"component_invalid {label}.{key}")
        # Path checks.
        for key in ("paths", "spec", "tests"):
            v = comp.get(key)
            if isinstance(v, list):
                for p in v:
                    if not isinstance(p, str):
                        continue
                    if bad_path(p):
                        faults.append(f"component_invalid {p}")
                    elif path_missing(p, tracked_set):
                        faults.append(f"path_missing {p}")
        if isinstance(paths, list):
            all_paths.extend([p for p in paths if isinstance(p, str) and not bad_path(p)])
        # Entry checks.
        if isinstance(entry, list):
            for e in entry:
                if not isinstance(e, str):
                    continue
                if bad_path(e):
                    faults.append(f"component_invalid {e}")
                    continue
                m = ENTRY_RE.match(e)
                if not m:
                    faults.append(f"component_invalid {e}")
                    continue
                fname, first_s, last_s = m.group(1), m.group(2), m.group(3)
                first, last = int(first_s), int(last_s)
                if fname not in tracked_set:
                    faults.append(f"entry_out_of_range {e}")
                    continue
                lc = line_count(root, fname)
                if lc is None:
                    faults.append(f"entry_out_of_range {e}")
                    continue
                if first == 0 or last < first or last > lc:
                    faults.append(f"entry_out_of_range {e}")

    seen = set()
    for n in names:
        if n in seen:
            faults.append(f"name_repeated {n}")
        seen.add(n)

    # Coverage.
    for r in roots:
        if bad_path(r):
            continue
        if r.endswith("/"):
            under = [t for t in tracked if t.startswith(r)]
        elif r in tracked_set:
            under = [r]
        else:
            under = []
        for f in under:
            if not covers(all_paths, f):
                faults.append(f"file_unmapped {f}")

    for f in faults:
        print(f)
    return 1 if faults else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
