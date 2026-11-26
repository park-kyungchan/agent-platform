#!/usr/bin/env bash
# Retires the legacy long-term memory service from the host, as scripts/retire-hindsight.json lists it.
#
#   retire-hindsight.sh check                    one line a target, present or absent, and the scan's hits
#   retire-hindsight.sh copy                     the recovery copy: final-<UTC time>/ under the copy root
#   retire-hindsight.sh delete <copy directory>  the entries, containers, network, volumes, images, directory
#   retire-hindsight.sh verify <copy directory>  exit 0 when nothing of the list is left
#   options: --root <dir> (every path of the list under <dir>, for the tests), --docker <program>
#
# Exit 0: done. Exit 1: a failure (a read-back, a removal, something left). Exit 2: a refusal before any
# change (usage, list_invalid, copy_not_verified, container_running, json_unreadable and others).
# The script prints paths, names, ids and digests only, never a file's bytes. The logic is the Python
# program below, run with python3 -I; it touches nothing the list does not name.
set -euo pipefail

here=$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
list=$here/retire-hindsight.json

IFS= read -r -d '' program <<'PY' || true
import copy as copy_module
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import tomllib
from json.decoder import scanstring

MANIFEST = "MANIFEST.sha256"
HEADER = "# retire-hindsight manifest 1"
NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]*\Z")
ID_RE = re.compile(r"sha256:[0-9a-f]{64}\Z")
DIGEST_LINE_RE = re.compile(r"([0-9a-f]{64})  ([^/\n]+)\Z")
FINAL_RE = re.compile(r"final-[^/]+\Z")
STOPPED = {"exited", "created", "dead"}
USAGE = ("usage: retire-hindsight.sh check | copy | delete <copy directory> | verify <copy directory>"
         " [--root <dir>] [--docker <program>]")


class Stop(Exception):
    def __init__(self, rc, *words):
        super().__init__()
        self.rc = rc
        self.words = words


def refuse(*words):
    raise Stop(2, *words)


def fail(*words):
    raise Stop(1, *words)


def say(*words):
    print(" ".join(str(w) for w in words), flush=True)


# ---- the arguments and the list ---------------------------------------------------------------------------

def parse_args(argv):
    list_path, rest = argv[0], argv[1:]
    options, words = {}, []
    i = 0
    while i < len(rest):
        arg = rest[i]
        if arg in ("--root", "--docker"):
            if i + 1 >= len(rest) or arg in options:
                refuse(USAGE)
            options[arg] = rest[i + 1]
            i += 2
        elif arg.startswith("-"):
            refuse(USAGE)
        else:
            words.append(arg)
            i += 1
    if not words or words[0] not in ("check", "copy", "delete", "verify"):
        refuse(USAGE)
    wanted = 1 if words[0] in ("check", "copy") else 2
    if len(words) != wanted:
        refuse(USAGE)
    root = options.get("--root")
    if root is not None:
        root = os.path.normpath(root)
        if not os.path.isabs(root) or root == "/" or not os.path.isdir(root):
            refuse("root_invalid", root)
    docker = options.get("--docker", "docker")
    if not docker:
        refuse(USAGE)
    return list_path, words[0], (words[1] if wanted == 2 else None), root, docker


def strict_json(data):
    """(text, value) of bytes that are strict JSON: UTF-8, no NaN or Infinity, no key given twice."""
    text = data.decode("utf-8")

    def pairs(items):
        obj = {}
        for key, value in items:
            if key in obj:
                raise ValueError("duplicate key")
            obj[key] = value
        return obj

    def constant(name):
        raise ValueError("constant " + name)

    return text, json.loads(text, object_pairs_hook=pairs, parse_constant=constant)


def load_list(path):
    try:
        with open(path, "rb") as f:
            _, spec = strict_json(f.read())
    except (OSError, ValueError, UnicodeDecodeError):
        refuse("list_invalid", path)

    def need(cond, what):
        if not cond:
            refuse("list_invalid", what)

    def names(key):
        value = spec.get(key)
        need(isinstance(value, list) and value, key)
        for name in value:
            need(isinstance(name, str) and NAME_RE.match(name), key)
        return value

    def abspath(value, what):
        need(isinstance(value, str) and os.path.isabs(value) and os.path.normpath(value) == value
             and value != "/", what)
        return value

    def strings(value, what):
        need(isinstance(value, list) and value and all(isinstance(s, str) and s for s in value), what)
        return value

    need(isinstance(spec, dict), "top")
    need(isinstance(spec.get("name"), str) and NAME_RE.match(spec["name"]), "name")
    names("containers")
    names("volumes")
    names("networks")
    abspath(spec.get("directory"), "directory")
    abspath(spec.get("copy_root"), "copy_root")
    entries = spec.get("entries")
    need(isinstance(entries, list) and entries, "entries")
    files = set()
    for e in entries:
        need(isinstance(e, dict), "entries")
        abspath(e.get("file"), "entries.file")
        need(e["file"] not in files, "entries.file twice")
        files.add(e["file"])
        kind = e.get("kind")
        if kind == "json_member":
            strings(e.get("member"), "entries.member")
        elif kind == "toml_table":
            strings(e.get("table"), "entries.table")
        elif kind == "json_key_suffix":
            strings(e.get("suffixes"), "entries.suffixes")
        else:
            need(kind == "file", "entries.kind")
    scanned = spec.get("scanned")
    need(isinstance(scanned, list), "scanned")
    for s in scanned:
        need(isinstance(s, dict), "scanned")
        abspath(s.get("file"), "scanned.file")
        need(s.get("format") in ("json", "toml"), "scanned.format")
        paths = s.get("key_paths")
        need(isinstance(paths, list) and paths, "scanned.key_paths")
        for kp in paths:
            strings(kp, "scanned.key_paths")
    return spec


# ---- docker ------------------------------------------------------------------------------------------------

def run_docker(args):
    try:
        return subprocess.run([DOCKER, *args], stdin=subprocess.DEVNULL, capture_output=True, text=True)
    except OSError:
        fail("docker_failed", DOCKER)


def first_line(text):
    lines = [line for line in text.strip().splitlines() if line.strip()]
    return lines[0] if lines else ""


def listed(kind):
    args = {
        "container": ["container", "ls", "-a", "--no-trunc", "--format", "{{.Names}}"],
        "network": ["network", "ls", "--no-trunc", "--format", "{{.Name}}"],
        "volume": ["volume", "ls", "--format", "{{.Name}}"],
        "image": ["image", "ls", "-a", "--no-trunc", "--format", "{{.ID}}"],
    }[kind]
    r = run_docker(args)
    if r.returncode != 0:
        fail("docker_failed", kind, "ls")
    found = set()
    for line in r.stdout.splitlines():
        for name in line.strip().split(","):
            if name.strip():
                found.add(name.strip().lstrip("/"))
    return found


def container_state(name):
    r = run_docker(["container", "inspect", "--format", "{{.State.Status}} {{.Image}}", name])
    parts = r.stdout.split()
    if r.returncode != 0 or len(parts) != 2 or not ID_RE.match(parts[1]):
        fail("docker_failed", "container", "inspect", name)
    return parts[0], parts[1]


def volume_mountpoint(name):
    r = run_docker(["volume", "inspect", "--format", "{{.Mountpoint}}", name])
    point = r.stdout.strip()
    if r.returncode != 0 or not os.path.isabs(point):
        fail("docker_failed", "volume", "inspect", name)
    if os.path.islink(point) or not os.path.isdir(point):
        fail("volume_data_missing", name)
    return point


def running_containers():
    present = listed("container")
    states = {}
    for name in SPEC["containers"]:
        if name in present:
            states[name] = container_state(name)
    running = [n for n, (status, _) in states.items() if status not in STOPPED]
    if running:
        refuse("container_running", *running)
    return states


# ---- paths -------------------------------------------------------------------------------------------------

def at(path):
    """The path of the list under --root."""
    return ROOT + path if ROOT else path


def read_bytes(path):
    with open(path, "rb") as f:
        return f.read()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ---- JSON entries: members found by their byte spans, so every other byte stays ----------------------------

SCALAR_RE = re.compile(r"-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?|true|false|null")


class Obj:
    def __init__(self, start):
        self.start = start  # the index of "{"
        self.end = None  # the index of "}"
        self.members = []  # (key, index of the key's quote, index after the value, node)


def skip_ws(s, i):
    while i < len(s) and s[i] in " \t\n\r":
        i += 1
    return i


def spans(s, i):
    """The node at s[i:] of a text that is already known to be strict JSON, and the index after it."""
    i = skip_ws(s, i)
    c = s[i]
    if c == "{":
        obj = Obj(i)
        j = skip_ws(s, i + 1)
        if s[j] == "}":
            obj.end = j
            return obj, j + 1
        while True:
            key_start = j
            key, j = scanstring(s, j + 1)
            j = skip_ws(s, j) + 1  # the colon
            node, value_end = spans(s, j)
            obj.members.append((key, key_start, value_end, node))
            j = skip_ws(s, value_end)
            if s[j] == ",":
                j = skip_ws(s, j + 1)
                continue
            obj.end = j
            return obj, j + 1
    if c == "[":
        items = []
        j = skip_ws(s, i + 1)
        if s[j] == "]":
            return items, j + 1
        while True:
            node, j = spans(s, j)
            items.append(node)
            j = skip_ws(s, j)
            if s[j] == ",":
                j += 1
                continue
            return items, j + 1
    if c == '"':
        return None, scanstring(s, i + 1)[1]
    return None, SCALAR_RE.match(s, i).end()


def cut_member(s, obj, k):
    """The text without member k of obj, with the comma that joined it."""
    ms = obj.members
    if len(ms) == 1:
        return s[:obj.start + 1] + s[obj.end:]
    if k < len(ms) - 1:
        return s[:ms[k][1]] + s[ms[k + 1][1]:]
    return s[:ms[k - 1][2]] + s[ms[k][2]:]


def find_json_target(node, entry):
    if entry["kind"] == "json_member":
        path = entry["member"]
        for seg in path[:-1]:
            if not isinstance(node, Obj):
                return None
            nxt = [m for m in node.members if m[0] == seg]
            if not nxt:
                return None
            node = nxt[0][3]
        if isinstance(node, Obj):
            for k, m in enumerate(node.members):
                if m[0] == path[-1]:
                    return node, k
        return None
    suffixes = tuple(entry["suffixes"])
    if isinstance(node, Obj):
        for k, m in enumerate(node.members):
            if m[0].endswith(suffixes):
                return node, k
            found = find_json_target(m[3], entry)
            if found:
                return found
    elif isinstance(node, list):
        for item in node:
            found = find_json_target(item, entry)
            if found:
                return found
    return None


def drop_json_target(value, entry):
    """The value with the entry's members removed, as Python sees it; the count removed."""
    if entry["kind"] == "json_member":
        node = value
        for seg in entry["member"][:-1]:
            if not isinstance(node, dict) or seg not in node:
                return 0
            node = node[seg]
        if isinstance(node, dict) and entry["member"][-1] in node:
            del node[entry["member"][-1]]
            return 1
        return 0
    suffixes = tuple(entry["suffixes"])
    count = 0
    if isinstance(value, dict):
        for key in list(value):
            if key.endswith(suffixes):
                del value[key]
                count += 1
            else:
                count += drop_json_target(value[key], entry)
    elif isinstance(value, list):
        for item in value:
            count += drop_json_target(item, entry)
    return count


def plan_json(entry, data):
    """(new bytes, count) for a JSON entry file; ValueError when it is not strict JSON."""
    text, value = strict_json(data)
    expected = copy_module.deepcopy(value)
    count = drop_json_target(expected, entry)
    cuts = 0
    while True:
        tree, _ = spans(text, 0)
        found = find_json_target(tree, entry)
        if not found:
            break
        text = cut_member(text, *found)
        cuts += 1
    new = text.encode("utf-8")
    try:
        _, after = strict_json(new)
    except ValueError:
        fail("rewrite_invalid", entry["file"])
    if after != expected or (count == 0) != (cuts == 0):
        fail("rewrite_mismatch", entry["file"])
    return new, count


# ---- TOML entries: tables removed by their lines -----------------------------------------------------------

BARE_RE = re.compile(r"[A-Za-z0-9_-]+")


def header_keys(line):
    """The key of a table header line ([a.b] or [[a.b]]), or None."""
    s = line.strip(" \t\r\n")
    if not s.startswith("["):
        return None
    double = s.startswith("[[")
    i = 2 if double else 1
    keys = []
    while True:
        while i < len(s) and s[i] in " \t":
            i += 1
        if i >= len(s):
            return None
        if s[i] == '"':
            j = i + 1
            while j < len(s) and s[j] != '"':
                j += 2 if s[j] == "\\" else 1
            if j >= len(s):
                return None
            try:
                keys.append(json.loads(s[i:j + 1]))
            except ValueError:
                return None
            i = j + 1
        elif s[i] == "'":
            j = s.find("'", i + 1)
            if j < 0:
                return None
            keys.append(s[i + 1:j])
            i = j + 1
        else:
            m = BARE_RE.match(s, i)
            if not m:
                return None
            keys.append(m.group())
            i = m.end()
        while i < len(s) and s[i] in " \t":
            i += 1
        if i < len(s) and s[i] == ".":
            i += 1
            continue
        break
    close = "]]" if double else "]"
    if not s.startswith(close, i):
        return None
    rest = s[i + len(close):].strip(" \t")
    if rest and not rest.startswith("#"):
        return None
    return keys


def advance(line, ml, depth):
    """The multi-line string and bracket depth after a line that is not a header."""
    i, n = 0, len(line)
    while i < n:
        if ml:
            j = line.find(ml, i)
            while ml == '"""' and j > 0 and (len(line[:j]) - len(line[:j].rstrip("\\"))) % 2 == 1:
                j = line.find(ml, j + 1)
            if j < 0:
                return ml, depth
            i = j + 3
            while i < n and line[i] == ml[0]:
                i += 1
            ml = None
            continue
        c = line[i]
        if c == "#":
            break
        if line.startswith('"""', i) or line.startswith("'''", i):
            ml = line[i:i + 3]
            i += 3
            continue
        if c == '"':
            j = i + 1
            while j < n and line[j] != '"':
                j += 2 if line[j] == "\\" else 1
            i = j + 1
            continue
        if c == "'":
            j = line.find("'", i + 1)
            i = j + 1 if j >= 0 else n
            continue
        if c in "[{":
            depth += 1
        elif c in "]}":
            depth = max(0, depth - 1)
        i += 1
    return ml, depth


def toml_cut(text, table):
    lines = re.findall(r"[^\n]*\n|[^\n]+\Z", text)
    headers, ml, depth = [], None, 0
    for idx, line in enumerate(lines):
        if ml is None and depth == 0:
            keys = header_keys(line)
            if keys is not None:
                headers.append((idx, keys))
                continue
        ml, depth = advance(line, ml, depth)

    def blank(line):
        return line.strip() == ""

    def comment(line):
        return line.lstrip(" \t").startswith("#")

    drop, tables = set(), 0
    for h, (idx, keys) in enumerate(headers):
        if keys[:len(table)] != table:
            continue
        tables += 1
        end = headers[h + 1][0] if h + 1 < len(headers) else len(lines)
        last = end
        while last > idx + 1 and (blank(lines[last - 1]) or comment(lines[last - 1])):
            last -= 1
        stop = last
        while stop < end and blank(lines[stop]):
            stop += 1
        drop.update(range(idx, stop))
    return "".join(line for idx, line in enumerate(lines) if idx not in drop), tables


def toml_has(value, table):
    for seg in table:
        if not isinstance(value, dict) or seg not in value:
            return False
        value = value[seg]
    return True


def plan_toml(entry, data):
    """(new bytes, count of tables) for a TOML entry file; ValueError when it does not parse."""
    table = entry["table"]
    text = data.decode("utf-8")
    before = tomllib.loads(text)
    if not toml_has(before, table):
        return data, 0
    new_text, tables = toml_cut(text, table)
    try:
        after = tomllib.loads(new_text)
    except ValueError:
        fail("rewrite_invalid", entry["file"])
    expected = copy_module.deepcopy(before)
    node = expected
    for seg in table[:-1]:
        node = node[seg]
    del node[table[-1]]
    accepted = [expected]
    pruned = copy_module.deepcopy(expected)
    for cut in range(len(table) - 1, 0, -1):  # a parent left empty may vanish with its last table
        parent = pruned
        for seg in table[:cut - 1]:
            parent = parent[seg]
        if parent[table[cut - 1]] != {}:
            break
        del parent[table[cut - 1]]
        accepted.append(copy_module.deepcopy(pruned))
    if tables == 0 or after not in accepted:
        fail("rewrite_mismatch", entry["file"])
    return new_text.encode("utf-8"), tables


# ---- the entries' state -----------------------------------------------------------------------------------

def describe(entry):
    kind = entry["kind"]
    if kind == "json_member":
        return "member " + json.dumps(entry["member"], ensure_ascii=False, separators=(",", ":"))
    if kind == "toml_table":
        return "table " + ".".join(entry["table"])
    if kind == "json_key_suffix":
        return "keys_ending " + ",".join(entry["suffixes"])
    return "file"


def entry_state(entry):
    """('absent' | 'present' | 'unreadable' | 'unremovable', count, bytes read, new bytes or None)."""
    path = at(entry["file"])
    if not os.path.lexists(path):
        return "absent", 0, None, None
    if entry["kind"] == "file":
        return "present", 1, None, None
    try:
        data = read_bytes(path)
    except OSError:
        return "unreadable", 0, None, None
    try:
        if entry["kind"] == "toml_table":
            new, count = plan_toml(entry, data)
        else:
            new, count = plan_json(entry, data)
    except (ValueError, UnicodeDecodeError):
        return "unreadable", 0, data, None
    except Stop:  # the planned bytes did not hold the rest of the file
        return "unremovable", 0, data, None
    return ("present" if count else "absent"), count, data, new


# ---- the scan of the settings files ------------------------------------------------------------------------

class Pairs(list):
    pass


def items_of(node):
    if isinstance(node, Pairs):
        return list(node), True
    if isinstance(node, dict):
        return list(node.items()), True
    if isinstance(node, list):
        return list(enumerate(node)), False
    return [], False


def scan_file(scanned):
    """('absent' | 'read' | 'unreadable', hits); a hit is the key path of a key or string naming the service."""
    path = at(scanned["file"])
    if not os.path.lexists(path):
        return "absent", []
    try:
        text = read_bytes(path).decode("utf-8")
        if scanned["format"] == "json":
            value = json.loads(text, object_pairs_hook=Pairs)
        else:
            value = tomllib.loads(text)
    except (OSError, ValueError, UnicodeDecodeError):
        return "unreadable", []
    name = SPEC["name"].lower()
    hits = []

    def inside(node, trail):
        if isinstance(node, str):
            if name in node.lower():
                hits.append(trail)
            return
        items, is_map = items_of(node)
        for key, child in items:
            if is_map and name in key.lower():
                hits.append(trail + [key])
                continue
            inside(child, trail + [key])

    def walk(node, segs, trail):
        if not segs:
            inside(node, trail)
            return
        items, is_map = items_of(node)
        for key, child in items:
            if segs[0] == "*" or (is_map and key == segs[0]):
                walk(child, segs[1:], trail + [key])

    for key_path in scanned["key_paths"]:
        walk(value, key_path, [])
    return "read", hits


def key_path_text(trail):
    return json.dumps(trail, ensure_ascii=False, separators=(",", ":"))


# ---- the manifest -----------------------------------------------------------------------------------------

def tar_list(archive):
    r = subprocess.run(["tar", "-tf", archive], stdin=subprocess.DEVNULL, capture_output=True)
    return r.stdout if r.returncode == 0 else None


def read_manifest(directory, bad):
    path = os.path.join(directory, MANIFEST)
    if os.path.islink(path) or not os.path.isfile(path):
        bad("manifest_missing")
    try:
        lines = read_bytes(path).decode("utf-8").split("\n")
    except (OSError, UnicodeDecodeError):
        bad("manifest_unreadable")
    if lines and lines[-1] == "":
        lines.pop()
    if not lines or lines[0] != HEADER:
        bad("manifest_malformed")
    records, digests = [], {}
    for line in lines[1:]:
        if line.startswith("# "):
            try:
                record = json.loads(line[2:])
            except ValueError:
                bad("manifest_malformed")
            if not isinstance(record, dict) or not isinstance(record.get("kind"), str) or any(
                    key in record and not isinstance(record[key], str)
                    for key in ("archive", "copy", "members_sha256", "state", "id", "name", "path")):
                bad("manifest_malformed")
            records.append(record)
            continue
        m = DIGEST_LINE_RE.match(line)
        if not m or m.group(2) in digests or m.group(2) in (MANIFEST, ".", ".."):
            bad("manifest_malformed")
        digests[m.group(2)] = m.group(1)
    return records, digests


def check_files(directory, records, digests, bad):
    """Every listed file's digest now, and every archive's member list read back whole."""
    for name, digest in digests.items():
        path = os.path.join(directory, name)
        if os.path.islink(path) or not os.path.isfile(path):
            bad("file_missing", name)
        if sha256_file(path) != digest:
            bad("digest_mismatch", name)
    for record in records:
        for key in ("archive", "copy"):
            if key in record and record[key] not in digests:
                bad("file_unlisted", str(record[key]))
        if "archive" in record:
            listing = tar_list(os.path.join(directory, record["archive"]))
            if listing is None or hashlib.sha256(listing).hexdigest() != record.get("members_sha256"):
                bad("members_mismatch", record["archive"])


def entry_files():
    return [e["file"] for e in SPEC["entries"]]


def check_coverage(records, bad):
    """The manifest names every target of the list once, and every target present now as copied."""
    def one(kind, key, value):
        found = [r for r in records if r.get("kind") == kind and r.get(key) == value]
        if len(found) != 1 or found[0].get("state") not in ("present", "absent"):
            bad("target_unrecorded", value)
        return found[0]

    containers, volumes, networks = listed("container"), listed("volume"), listed("network")
    for name in SPEC["containers"]:
        r = one("container", "name", name)
        if name in containers and r["state"] != "present":
            bad("target_unrecorded", name)
    for name in SPEC["volumes"]:
        r = one("volume", "name", name)
        if name in volumes and (r["state"] != "present" or "archive" not in r):
            bad("target_unrecorded", name)
    for name in SPEC["networks"]:
        r = one("network", "name", name)
        if name in networks and r["state"] != "present":
            bad("target_unrecorded", name)
    r = one("directory", "path", SPEC["directory"])
    if os.path.lexists(at(SPEC["directory"])) and (r["state"] != "present" or "archive" not in r):
        bad("target_unrecorded", SPEC["directory"])
    for path in entry_files():
        r = one("file", "path", path)
        if os.path.lexists(at(path)) and (r["state"] != "present" or "copy" not in r):
            bad("target_unrecorded", path)
    for r in records:
        if r["kind"] == "image" and not (isinstance(r.get("id"), str) and ID_RE.match(r["id"])):
            bad("manifest_malformed")


def verified_copy(argument):
    """The copy directory, after every check of delete and verify; refuses copy_not_verified."""
    def bad(*why):
        refuse("copy_not_verified", *why)

    root = at(SPEC["copy_root"])
    if os.path.islink(root) or not os.path.isdir(root):
        bad("copy_root_missing")
    directory = os.path.abspath(argument.rstrip("/") or "/")
    if not FINAL_RE.match(os.path.basename(directory)):
        bad("not_final")
    if os.path.islink(directory) or not os.path.isdir(directory):
        bad("not_a_directory")
    if os.path.realpath(os.path.dirname(directory)) != os.path.realpath(root):
        bad("outside_copy_root")
    records, digests = read_manifest(directory, bad)
    check_files(directory, records, digests, bad)
    check_coverage(records, bad)
    return directory, records


# ---- the words --------------------------------------------------------------------------------------------

def word_check():
    containers, volumes, networks = listed("container"), listed("volume"), listed("network")
    for name in SPEC["containers"]:
        if name in containers:
            status, image = container_state(name)
            say("container", name, "present", status, image)
        else:
            say("container", name, "absent")
    for name in SPEC["volumes"]:
        say("volume", name, "present" if name in volumes else "absent")
    for name in SPEC["networks"]:
        say("network", name, "present" if name in networks else "absent")
    say("directory", SPEC["directory"], "present" if os.path.lexists(at(SPEC["directory"])) else "absent")
    say("copy_root", SPEC["copy_root"], "present" if os.path.lexists(at(SPEC["copy_root"])) else "absent")
    for entry in SPEC["entries"]:
        state, count, _, _ = entry_state(entry)
        say("entry", entry["file"], describe(entry), state, *([count] if state == "present" else []))
    for scanned in SPEC["scanned"]:
        state, hits = scan_file(scanned)
        say("scan", scanned["file"], state, len(hits))
        for trail in hits:
            say("hit", scanned["file"], key_path_text(trail))


def tar_create(archive, parent, member):
    r = subprocess.run(["tar", "--numeric-owner", "-cvf", archive, "-C", parent, member],
                       stdin=subprocess.DEVNULL, capture_output=True)
    if r.returncode != 0:
        fail("archive_failed", os.path.basename(archive))
    return r.stdout


def word_copy():
    states = running_containers()
    root = at(SPEC["copy_root"])
    if not os.path.lexists(root):
        try:
            os.mkdir(root, 0o700)
        except OSError:
            refuse("copy_root_invalid", SPEC["copy_root"])
        os.chmod(root, 0o700)
    elif os.path.islink(root) or not os.path.isdir(root):
        refuse("copy_root_invalid", SPEC["copy_root"])
    directory = os.path.join(root, time.strftime("final-%Y%m%dT%H%M%SZ", time.gmtime()))
    try:
        os.mkdir(directory, 0o700)
    except FileExistsError:
        refuse("copy_exists", directory)
    try:
        os.chmod(directory, 0o700)
        copy_into(directory, states)
    except BaseException:
        subprocess.run(["rm", "-rf", "--one-file-system", "--", directory], stdin=subprocess.DEVNULL)
        raise


def copy_into(directory, states):
    records, digests = [], []

    def archived(record, archive, parent, member):
        path = os.path.join(directory, archive)
        listing = tar_create(path, parent, member)
        record.update(archive=archive, members=listing.count(b"\n"),
                      members_sha256=hashlib.sha256(listing).hexdigest())
        digests.append((archive, sha256_file(path)))

    images = []
    for name in SPEC["containers"]:
        if name in states:
            status, image = states[name]
            records.append({"kind": "container", "name": name, "state": "present", "status": status,
                            "image": image})
            images.append((image, name))
            say("container", name, "recorded", status, image)
        else:
            records.append({"kind": "container", "name": name, "state": "absent"})
            say("container", name, "absent")
    volumes = listed("volume")
    for name in SPEC["volumes"]:
        record = {"kind": "volume", "name": name, "state": "absent"}
        if name in volumes:
            point = volume_mountpoint(name)
            record.update(state="present", mountpoint=point)
            archived(record, "volume-" + name + ".tar", point, ".")
            say("volume", name, "archived", record["archive"])
        else:
            say("volume", name, "absent")
        records.append(record)
    networks = listed("network")
    for name in SPEC["networks"]:
        state = "present" if name in networks else "absent"
        records.append({"kind": "network", "name": name, "state": state})
        say("network", name, state)
    path = at(SPEC["directory"])
    record = {"kind": "directory", "path": SPEC["directory"], "state": "absent"}
    if os.path.lexists(path):
        if os.path.islink(path) or not os.path.isdir(path):
            fail("directory_not_a_directory", SPEC["directory"])
        record["state"] = "present"
        archived(record, "directory.tar", os.path.dirname(path), os.path.basename(path))
        say("directory", SPEC["directory"], "archived", record["archive"])
    else:
        say("directory", SPEC["directory"], "absent")
    records.append(record)
    for index, file in enumerate(entry_files(), 1):
        path = at(file)
        record = {"kind": "file", "path": file, "state": "absent"}
        if os.path.lexists(path):
            if not os.path.isfile(path):
                fail("entry_not_a_file", file)
            info = os.stat(path)
            data = read_bytes(path)
            name = "file-%d-%s" % (index, os.path.basename(file))
            fd = os.open(os.path.join(directory, name), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "wb") as f:
                f.write(data)
            record.update(state="present", copy=name, mode="%04o" % (info.st_mode & 0o7777),
                          uid=info.st_uid, gid=info.st_gid)
            digests.append((name, hashlib.sha256(data).hexdigest()))
            say("file", file, "copied", name)
        else:
            say("file", file, "absent")
        records.append(record)
    for image, name in images:
        records.append({"kind": "image", "id": image, "container": name})
    text = HEADER + "\n"
    for record in records:
        text += "# " + json.dumps(record, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n"
    for name, digest in digests:
        text += "%s  %s\n" % (digest, name)
    manifest = os.path.join(directory, MANIFEST)
    fd = os.open(manifest, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as f:
        f.write(text.encode("utf-8"))
        f.flush()
        os.fsync(f.fileno())

    def bad(*why):
        fail("read_back_failed", *why)

    read_records, read_digests = read_manifest(directory, bad)
    if read_records != records or read_digests != dict(digests):
        bad("manifest_differs")
    check_files(directory, read_records, read_digests, bad)
    say("copy", directory)
    say("manifest_sha256", sha256_file(manifest))


def write_in_place(entry, before, new):
    path = at(entry["file"])
    try:
        if read_bytes(path) != before:
            fail("entry_changed", entry["file"])
        fd = os.open(path, os.O_WRONLY)
        try:
            view = memoryview(new)
            while view:
                view = view[os.write(fd, view):]
            os.ftruncate(fd, len(new))
            os.fsync(fd)
        finally:
            os.close(fd)
        if read_bytes(path) != new:
            fail("removal_failed", "entry", entry["file"], "read_back")
    except OSError:
        fail("removal_failed", "entry", entry["file"])


def removal(kind, name, args, gone_marker):
    r = run_docker(args)
    if r.returncode == 0:
        say(kind, name, "removed")
    elif gone_marker in r.stderr.lower():
        say(kind, name, "gone")
    else:
        print("docker: " + first_line(r.stderr), file=sys.stderr)
        fail("removal_failed", kind, name)


def word_delete(argument):
    directory, records = verified_copy(argument)
    running_containers()
    plans = [(entry, *entry_state(entry)) for entry in SPEC["entries"]]
    for entry, state, _, _, _ in plans:
        if state == "unreadable" and entry["kind"] in ("json_member", "json_key_suffix"):
            refuse("json_unreadable", entry["file"])
    for entry, state, _, _, _ in plans:
        if state in ("unreadable", "unremovable"):
            fail("removal_failed", "entry", entry["file"], state)
    # 1. the entries
    for entry, state, count, data, new in plans:
        label = ("entry", entry["file"], describe(entry))
        if state == "absent":
            say(*label, "gone")
        elif entry["kind"] == "file":
            try:
                os.unlink(at(entry["file"]))
                say(*label, "removed")
            except FileNotFoundError:
                say(*label, "gone")
            except OSError:
                fail("removal_failed", *label)
        else:
            write_in_place(entry, data, new)
            say(*label, "removed", count)
    # 2. the containers, 3. the network, 4. the volumes
    for kind, names, args in (("container", SPEC["containers"], ["container", "rm"]),
                              ("network", SPEC["networks"], ["network", "rm"]),
                              ("volume", SPEC["volumes"], ["volume", "rm"])):
        present = listed(kind)
        for name in names:
            if name not in present:
                say(kind, name, "gone")
                continue
            removal(kind, name, args + [name], "no such")
    # 5. the images, by the id the manifest records only
    ids = []
    for record in records:
        if record["kind"] == "image" and record["id"] not in ids:
            ids.append(record["id"])
    present = listed("image")
    for image in ids:
        if image not in present:
            say("image", image, "gone")
            continue
        r = run_docker(["image", "rm", image])
        if r.returncode == 0:
            say("image", image, "removed")
        elif "conflict" in r.stderr.lower():
            say("image", image, "kept")
        elif "no such image" in r.stderr.lower():
            say("image", image, "gone")
        else:
            print("docker: " + first_line(r.stderr), file=sys.stderr)
            fail("removal_failed", "image", image)
    # 6. the directory
    path = at(SPEC["directory"])
    if not os.path.lexists(path):
        say("directory", SPEC["directory"], "gone")
    elif os.path.islink(path) or not os.path.isdir(path):
        fail("removal_failed", "directory", SPEC["directory"], "not_a_directory")
    else:
        r = subprocess.run(["rm", "-rf", "--one-file-system", "--", path], stdin=subprocess.DEVNULL,
                           capture_output=True)
        if r.returncode != 0 or os.path.lexists(path):
            fail("removal_failed", "directory", SPEC["directory"])
        say("directory", SPEC["directory"], "removed")
    say("deleted", directory)


def word_verify(argument):
    directory, records = verified_copy(argument)
    left = 0
    for kind, names in (("container", SPEC["containers"]), ("network", SPEC["networks"]),
                        ("volume", SPEC["volumes"])):
        present = listed(kind)
        for name in names:
            if name in present:
                say("left", kind, name)
                left += 1
    if os.path.lexists(at(SPEC["directory"])):
        say("left", "directory", SPEC["directory"])
        left += 1
    images = listed("image")
    for record in records:
        if record["kind"] == "image":
            say("image", record["id"], "kept" if record["id"] in images else "gone")
    for entry in SPEC["entries"]:
        state, _, _, _ = entry_state(entry)
        if state != "absent":
            say("left", "entry", entry["file"], describe(entry), state)
            left += 1
    for scanned in SPEC["scanned"]:
        state, hits = scan_file(scanned)
        if state == "unreadable":
            say("left", "scan", scanned["file"], "unreadable")
            left += 1
        for trail in hits:
            say("hit", scanned["file"], key_path_text(trail))
            left += 1
    if left:
        fail("not_retired", left)
    say("verified", directory)


def main():
    global SPEC, ROOT, DOCKER
    try:
        list_path, word, argument, ROOT, DOCKER = parse_args(sys.argv[1:])
        SPEC = load_list(list_path)
        os.umask(0o077)
        if word == "check":
            word_check()
        elif word == "copy":
            word_copy()
        elif word == "delete":
            word_delete(argument)
        else:
            word_verify(argument)
    except Stop as stop:
        sys.stdout.flush()
        print(" ".join(str(w) for w in stop.words), file=sys.stderr)
        sys.exit(stop.rc)
    except OSError as error:
        sys.stdout.flush()
        print("os_error", error.strerror, error.filename or "", file=sys.stderr)
        sys.exit(1)


SPEC = ROOT = DOCKER = None
main()
PY

exec python3 -I -c "$program" "$list" "$@"
