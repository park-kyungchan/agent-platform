"""The checks of host/ that need no Rust build: the steps the operations repository's own check ran on its tree.

  python3 -I -B ci/check-host.py [<repository root>]

Over the tracked files under host/ (git ls-files): every *.json file parses; every shell script (a name ending in .sh,
or a first line #! naming bash or sh, directly or through env) passes `bash -n`; every row of
host/deployments/host-artifacts.json has source, destination and sha256, its destination (relative to host/) exists
and the SHA-256 of its bytes equals sha256; every tracked file under host/deployments/ is the inventory, a row's
destination or a README.md. The copies are compared with their recorded digests, never with the host. A finding is
one line `check: <code>@<path>` on stderr and the exit is 1; a clean run prints one count line and exits 0.
"""
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

HOST = "host/"
INVENTORY = "host/deployments/host-artifacts.json"
SHEBANG = re.compile(r"^#!\s*(\S*/)?(env\s+(-\S*\s+)*)?(bash|sh)(\s|$)")


def tracked(root):
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z", "--", HOST], capture_output=True, check=True)
    return [p for p in out.stdout.decode().split("\0") if p]


def is_shell(root, path):
    if path.endswith(".sh"):
        return True
    try:
        with open(root / path, "rb") as f:
            first = f.readline().decode("utf-8", "replace").rstrip("\n")
    except OSError:
        return False
    return bool(SHEBANG.match(first))


def main(argv):
    root = Path(argv[1] if len(argv) > 1 else Path(__file__).resolve().parent.parent)
    failures = []
    paths = tracked(root)
    if not paths:
        failures.append(f"no_tracked_files@{HOST}")
    json_count = shell_count = 0
    for path in paths:
        if path.endswith(".json"):
            json_count += 1
            try:
                json.loads((root / path).read_bytes())
            except (OSError, ValueError):
                failures.append(f"json_invalid@{path}")
        if is_shell(root, path):
            shell_count += 1
            if subprocess.run(["bash", "-n", str(root / path)], capture_output=True).returncode != 0:
                failures.append(f"shell_syntax@{path}")
    destinations = []
    try:
        rows = json.loads((root / INVENTORY).read_bytes())
    except FileNotFoundError:
        rows = None
        failures.append(f"inventory_missing@{INVENTORY}")
    except (OSError, ValueError):
        rows = None
        failures.append(f"inventory_unreadable@{INVENTORY}")
    if rows is not None and not isinstance(rows, list):
        failures.append(f"inventory_not_list@{INVENTORY}")
        rows = None
    for index, row in enumerate(rows or []):
        if not isinstance(row, dict):
            failures.append(f"row_not_object[{index}]@{INVENTORY}")
            continue
        missing = [k for k in ("source", "destination", "sha256") if not isinstance(row.get(k), str) or not row.get(k)]
        if missing:
            failures.append(f"row_missing_{'_'.join(missing)}[{index}]@{INVENTORY}")
            continue
        destination = HOST + row["destination"]
        destinations.append(destination)
        target = root / destination
        if ".." in row["destination"].split("/") or not target.is_file():
            failures.append(f"destination_missing@{destination}")
            continue
        if hashlib.sha256(target.read_bytes()).hexdigest() != row["sha256"]:
            failures.append(f"digest_mismatch@{destination}")
    for path in paths:
        if not path.startswith(HOST + "deployments/") or path == INVENTORY or path.endswith("/README.md"):
            continue
        if path not in destinations:
            failures.append(f"deployment_unlisted@{path}")
    for f in failures:
        print(f"check: {f}", file=sys.stderr)
    if failures:
        print(f"check: host failed ({len(failures)})", file=sys.stderr)
        return 1
    print(f"check: host {json_count} json, {shell_count} shell scripts, {len(destinations)} inventory rows")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
