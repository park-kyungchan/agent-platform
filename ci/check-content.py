#!/usr/bin/env python3
"""Source check for content/: the shared instructions every coding CLI loads.

Fails the build when (1) SHA256SUMS and the files disagree, (2) always-loaded text
(AGENTS.md, skill frontmatter) carries a value that changes over time, (3) any owned
file names a model, or (4) a file that content/ routes a reader to does not exist.
Reads only the repository. Run: python3 -I -B ci/check-content.py
"""
import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "content"
INSTALLED = "/root/.agents/"
MODEL = re.compile(r"\b(gpt-\d[\w.-]*|muse-[a-z]+-\d[\w.]*|claude-(?:opus|fable|sonnet|haiku)[\w.-]*|gemini-\d[\w.-]*|Gemini \d \w+|Opus|Sonnet|Haiku|Fable)\b", re.I)
CHANGING = {
    "date": re.compile(r"\b20\d\d[-/]\d\d[-/]\d\d\b|\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\.? ?\d{1,2}\b"),
    "version": re.compile(r"\b\d+\.\d+\.\d+\b|\bv\d+(?:\.\d+)*\b"),
    "token window": re.compile(r"\d{3,}k\b|\b\d+M\b|\b\d{1,3}(?:,\d{3})+ tokens\b"),
    "effort": re.compile(r"\beffort (?:low|medium|high|xhigh|max)\b|/(?:low|medium|high|xhigh)\b"),
    "pane or session id": re.compile(r"\bw[0-9A-Za-z]{1,4}:[pt]\d+\b|\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"),
    "git sha": re.compile(r"\b[0-9a-f]{40}\b|`[0-9a-f]{7,12}`"),
}
errors = []


def fail(code, path, detail):
    errors.append(f"{code}@content/{path}: {detail}")


listed = {}
for line in (ROOT / "SHA256SUMS").read_text().splitlines():
    digest, name = line.split("  ", 1)
    listed[name] = digest
owned = sorted(str(p.relative_to(ROOT)) for p in ROOT.rglob("*") if p.is_file() and p.name not in ("SHA256SUMS", "README.md"))
for name in owned:
    if name not in listed:
        fail("file_not_in_sha256sums", name, "add it to SHA256SUMS and content_inventory.rs")
    elif hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != listed[name]:
        fail("sha256_mismatch", name, "regenerate SHA256SUMS")
for name in listed:
    if name not in owned:
        fail("sha256sums_names_missing_file", name, "remove the line or restore the file")

inventory = re.findall(r'^\s*\(?\s*"((?:AGENTS\.md|runtime-references/|skills/)[^"]*)",', (ROOT.parent / "crates/ap/src/native/content_inventory.rs").read_text(), re.M)
if inventory != list(listed):
    fail("inventory_differs_from_sha256sums", "SHA256SUMS", "content_inventory.rs and SHA256SUMS must list the same files in the same order")

texts = {name: (ROOT / name).read_text(encoding="utf-8") for name in owned if name.endswith((".md", ".json"))}
for name, text in texts.items():
    for m in MODEL.finditer(text):
        fail("model_name_in_content", name, f"'{m.group(0)}' belongs in a task STATE, not in shared instructions")
    always = text if name == "AGENTS.md" else ""
    if name.endswith("/SKILL.md"):
        fm = re.match(r"---\n(.*?)\n---", text, re.S)
        always = fm.group(1) if fm else ""
        if not fm:
            fail("skill_without_frontmatter", name, "a SKILL.md starts with a --- name/description --- block")
    for kind, rx in CHANGING.items():
        for m in rx.finditer(always):
            fail("changing_value_in_always_loaded_text", name, f"{kind} '{m.group(0)}'")
    # Every route must lead to a file: installed paths, relative links, and bare reference names.
    for m in re.finditer(re.escape(INSTALLED) + r"((?:runtime-references|skills)/[\w./-]*\w)", text):
        target = m.group(1)
        if not (ROOT / target).exists():
            fail("route_to_missing_file", name, f"{INSTALLED}{target}")
    prose = re.sub(r"`[^`\n]*`", "", text)  # a link written inside inline code is an example, not a route
    for m in re.finditer(r"\]\((?!https?:|#)([^)\s]+)\)", prose):
        if not (ROOT / name).parent.joinpath(m.group(1).split("#")[0]).exists():
            fail("broken_relative_link", name, m.group(1))
def has_section(target, number):
    return re.search(rf"^## {number}\. ", texts.get(target, ""), re.M) is not None


for name, text in texts.items():
    for m in re.finditer(r"`(?:/root/\.agents/)?(?:runtime-references/)?([A-Za-z][\w-]*\.md)` section (\d+)", text):
        target = "AGENTS.md" if m.group(1) == "AGENTS.md" else "runtime-references/" + m.group(1)
        if not has_section(target, m.group(2)):
            fail("route_to_missing_section", name, f"{m.group(1)} section {m.group(2)}")
    for m in re.finditer(r"\b[Ss]ection (\d+) (?:below|above)", text):
        if not has_section(name, m.group(1)):
            fail("route_to_missing_section", name, f"section {m.group(1)} of this file")
references = {p.name for p in (ROOT / "runtime-references").iterdir()}
for name in [n for n in texts if n.endswith(".md")]:
    for m in re.finditer(r"`([A-Za-z][\w-]*\.md)`", texts[name]):
        token = m.group(1)
        if "_" in token or token in ("STATE.md", "TASK.md", "STRATEGY.md", "SKILL.md", "MEMORY.md", "AGENTS.md", "CLAUDE.md", "GEMINI.md", "README.md"):
            continue
        if token not in references:
            fail("route_to_missing_reference", name, f"`{token}` is not in content/runtime-references")

for e in errors:
    print(e)
print(f"content check: {len(owned)} files, {len(errors)} errors")
sys.exit(1 if errors else 0)
