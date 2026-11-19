#!/usr/bin/env python3
"""Source check for content/: the shared instructions every coding CLI loads.

Fails the build when (1) SHA256SUMS and the files disagree, (2) always-loaded text
(AGENTS.md, skill frontmatter) carries a value that changes over time, (3) an owned file
names a model (the Lead's table of the allowed models alone excepted), (4) a file that content/ routes a reader to does not exist, (5) a path
is elided or points into a main checkout's files, (6) on a host that has the corpus of
design sources, a line range of OFFICIAL-DOCS.md section 4 falls outside its file, or (7) AGENTS.md
breaks its form (specs/content/agents-file.md): over 6,500 bytes, its eight section headings not in
order, a line over 400 characters or a table row, or, on the host, an absolute path that does not exist.
Reads the repository, and the corpus and the host's paths when they are present. Run: python3 -I -B ci/check-content.py
"""
import hashlib
import os
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
        fail("file_not_in_sha256sums", name, "add it to SHA256SUMS")
    elif hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != listed[name]:
        fail("sha256_mismatch", name, "regenerate SHA256SUMS")
for name in listed:
    if name not in owned:
        fail("sha256sums_names_missing_file", name, "remove the line or restore the file")

texts = {name: (ROOT / name).read_text(encoding="utf-8") for name in owned if name.endswith((".md", ".json"))}
# A file name of the corpus of design sources is a citation, not a model assignment.
CORPUS_PATH = re.compile(r"`docs/[^`\s]+`")
# The one file that names models: the Lead's table of the allowed models and the rule that chooses them by a page's size.
MODEL_TABLE = "skills/lead/references/unit-page.md"
for name, text in texts.items():
    if name != MODEL_TABLE:
        for m in MODEL.finditer(CORPUS_PATH.sub("``", text)):
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
        if "_" in token or token in ("STATE.md", "TASK.md", "STRATEGY.md", "LENS.md", "SKILL.md", "MEMORY.md", "AGENTS.md", "CLAUDE.md", "GEMINI.md", "README.md"):
            continue
        if token not in references:
            fail("route_to_missing_reference", name, f"`{token}` is not in content/runtime-references")

# One word for one thing: a word the glossary lists under "Do not write" fails anywhere else.
GLOSSARY = "runtime-references/GLOSSARY.md"
banned = {}
terms = []
if GLOSSARY not in texts:
    fail("glossary_missing", GLOSSARY, "the word check needs the glossary")
in_terms = False
for line in texts.get(GLOSSARY, "").splitlines():
    if not line.startswith("|"):
        in_terms = False
        continue
    cells = [c.strip() for c in line.strip().strip("|").split(" | ")]
    if cells[-1] == "Do not write":
        in_terms = True
        continue
    if not in_terms or set(line) <= set("|- "):
        continue
    if len(cells) != 4:
        fail("glossary_row_malformed", GLOSSARY, f"a term row needs four cells: {line[:60]}")
        continue
    for word in re.findall(r"`([^`]+)`", cells[3]):
        banned[word.lower()] = cells[0]
    terms.append(cells[0])
for name, text in texts.items():
    if name == GLOSSARY:
        continue
    low = text.lower()
    for word, term in banned.items():
        if re.search(r"(?<![\w-])" + re.escape(word) + r"(?![\w-])", low):
            fail("word_the_glossary_replaces", name, f"'{word}': write '{term}' ({GLOSSARY})")

# The vocabulary is a closed set with a budget, not a list that grows: a term must be used, and the files have limits.
CORE_TERM_LIMIT, GLOSSARY_BYTE_LIMIT, USER_ROW_LIMIT, AGENTS_BYTE_LIMIT, SKILL_BYTE_LIMIT, LEAD_REFERENCE_BYTE_LIMIT = 16, 12000, 30, 6500, 6500, 8000
if not terms or not banned:
    fail("glossary_no_terms", GLOSSARY, "no term table with a last column 'Do not write' was found")
others = " ".join(text.lower() for name, text in texts.items() if name != GLOSSARY)
for cell in terms:
    for term in [w.strip().lower() for w in cell.split(",")]:
        if not re.search(r"(?<![\w-])" + re.escape(term) + r"s?(?![\w-])", others):
            fail("glossary_term_unused", GLOSSARY, f"'{term}' is used in no other file: remove the row")
core = len(re.findall(r"\*\*[^*]+\*\*", texts.get("AGENTS.md", "").split("## 8. ")[-1]))
if core > CORE_TERM_LIMIT:
    fail("core_terms_over_limit", "AGENTS.md", f"{core} entries, limit {CORE_TERM_LIMIT}: move one to the glossary before adding one")
size = len(texts.get(GLOSSARY, "").encode())
if size > GLOSSARY_BYTE_LIMIT:
    fail("glossary_over_limit", GLOSSARY, f"{size} bytes, limit {GLOSSARY_BYTE_LIMIT}: remove or move rows before adding")
LIMITS = [("AGENTS.md", AGENTS_BYTE_LIMIT), ("skills/lead/SKILL.md", SKILL_BYTE_LIMIT), ("skills/report/SKILL.md", SKILL_BYTE_LIMIT)]
LIMITS += [(f"skills/lead/references/{n}.md", LEAD_REFERENCE_BYTE_LIMIT) for n in ("interview", "unit-page", "judging")]
for path, limit in LIMITS:
    used = len(texts.get(path, "").encode())
    if used > limit:
        fail("file_over_limit", path, f"{used} bytes, limit {limit}: cut or move text before adding")
user_rows = [l for l in texts.get(GLOSSARY, "").split("## 7. ")[-1].splitlines() if l.startswith("| ") and not set(l) <= set("|- ")]
if len(user_rows) - 1 > USER_ROW_LIMIT:
    fail("user_table_over_limit", GLOSSARY, f"{len(user_rows) - 1} rows, limit {USER_ROW_LIMIT}")

# What content/ says about the command `ap` must exist: a verb in backticks, a section of its README by its heading.
CRATE = ROOT.parent / "crates/ap"
VERB_SOURCES = ("src/main.rs",)  # the dispatch of the command: `arg == "gate"`, `args[0] == "version"`
verbs = set()
for source in VERB_SOURCES:
    for found in re.findall(r'\barg == "([a-z][\w-]*)"|\bargs\[0\] (?:==|!=) "([a-z][\w-]*)"', (CRATE / source).read_text()):
        verbs.update(v for v in found if v)
headings = set(re.findall(r"^## (.+?)\s*$", (CRATE / "README.md").read_text(), re.M))
if not verbs or not headings:
    fail("ap_interface_unreadable", "SHA256SUMS", "no verb of `ap` or no `##` heading of its README was found")
QUOTED = r'"([^"\n]+)"'
QUOTED_LIST = QUOTED + r'(?:(?:,|,? and|,? or) ' + QUOTED + r')*'
SECTION_REFERENCES = (  # `crates/ap/README.md` "A" and "B" | README sections "A" | section "A" of `crates/ap/README.md`
    r"`crates/ap/README\.md`,? (?:sections? )?(" + QUOTED_LIST + ")",
    r"\bREADME,? (?:sections? )?(" + QUOTED_LIST + ")",
    r"\bsections? (" + QUOTED_LIST + r") of `crates/ap/README\.md`",
)
for name, text in texts.items():
    line_of = lambda at: text.count("\n", 0, at) + 1
    for m in re.finditer(r"`ap ([a-z][\w-]*)(?=[\s`])", text):
        if m.group(1) not in verbs:
            fail("unknown_ap_verb", f"{name}:{line_of(m.start())}", f"`ap {m.group(1)}`; the verbs of the command: {', '.join(sorted(verbs))}")
    for rx in SECTION_REFERENCES:
        for m in re.finditer(rx, text):
            for quoted in re.finditer(QUOTED, m.group(1)):
                if quoted.group(1) not in headings:
                    fail("unknown_readme_section", f"{name}:{line_of(m.start(1) + quoted.start())}", f'"{quoted.group(1)}"; the headings of crates/ap/README.md: {", ".join(sorted(headings))}')

# Paths a reader can open as written: never elided, never into the files of a main checkout (they lag origin/main).
PATHLIKE = re.compile(r"`((?:/|~/|\$[A-Z_]+/|\./|docs/|content/|crates/)[^`\n]*)`")
MAIN_CHECKOUT_FILE = re.compile(r"/home/palantirkc/platforms/([A-Za-z0-9][\w.-]*)/([A-Za-z0-9._-][^\s`)|,;]*)")
for name, text in texts.items():
    line_of = lambda at: text.count("\n", 0, at) + 1
    for m in PATHLIKE.finditer(text):
        if "\u2026" in m.group(1) or "..." in m.group(1):
            fail("elided_path", f"{name}:{line_of(m.start())}", f"`{m.group(1)}`: write the path in full or as a <pattern>")
    for m in MAIN_CHECKOUT_FILE.finditer(text):
        fail("main_checkout_path", f"{name}:{line_of(m.start())}", f"{m.group(0)}: read it as `git -C /home/palantirkc/platforms/{m.group(1)} show origin/main:{m.group(2)}`")

# The form of the always-loaded file (specs/content/agents-file.md): its eight sections in order, one page of prose.
AGENTS_SECTIONS = ("Who decides", "The order of work", "Four commands", "Evidence", "Effects",
                   "The user's standing limits", "Language", "Where things are")
AGENTS_LINE_LIMIT = 400
agents = texts.get("AGENTS.md", "")
expected = [f"## {n}. {title}" for n, title in enumerate(AGENTS_SECTIONS, 1)]
found = [line.rstrip() for line in agents.splitlines() if line.startswith("## ")]
for heading in expected:
    if heading not in found:
        fail("agents_section_missing", "AGENTS.md", f"'{heading}'")
for heading in found:
    if heading not in expected:
        fail("agents_section_unexpected", "AGENTS.md", f"'{heading}': the file has the eight sections of specs/content/agents-file.md")
if sorted(found) == sorted(expected) and found != expected:
    fail("agents_sections_out_of_order", "AGENTS.md", f"found {' / '.join(found)}")
for number, line in enumerate(agents.splitlines(), 1):
    if len(line) > AGENTS_LINE_LIMIT:
        fail("agents_line_too_long", f"AGENTS.md:{number}", f"{len(line)} characters, limit {AGENTS_LINE_LIMIT}")
    if line.startswith("|"):
        fail("agents_table", f"AGENTS.md:{number}", "a table row: the always-loaded file is prose and lists")
# Every absolute path the file names exists, or is a pattern (`<name>` or `*`). The paths are this host's, so this
# runs where the host is (the installed file exists, or the tests name a stand-in) and is skipped in GitHub's CI.
HOST = Path(os.environ.get("CONTENT_CHECK_HOST", INSTALLED + "AGENTS.md"))
ABSOLUTE = re.compile(r"(?<![\w.~:/<>@$-])/[\w.*<>@+-]+(?:/[\w.*<>@+-]*)*")
if HOST.exists():
    for number, line in enumerate(agents.splitlines(), 1):
        for m in ABSOLUTE.finditer(line):
            path = m.group(0).rstrip(".")
            if "<" not in path and "*" not in path and not Path(path).exists():
                fail("agents_path_missing", f"AGENTS.md:{number}", f"{path}: name a path that exists, or write it as a pattern (<name> or *)")
else:
    print(f"agents paths: skipped, {HOST} is absent (not this host)")

# The route table of the design sources (OFFICIAL-DOCS.md section 4): every line range inside its file. The corpus is on
# the host only, so this runs where it is present (./check on the host) and is skipped in GitHub's CI.
CORPUS = Path(os.environ.get("CONTENT_CHECK_CORPUS", "/home/palantirkc/external/reference-docs"))
ROUTES = "runtime-references/OFFICIAL-DOCS.md"
section = texts.get(ROUTES, "").split("\n## 4. ", 1)
if CORPUS.is_dir() and len(section) != 2:
    fail("design_routes_section_missing", ROUTES, "no section '## 4. ' to check against the corpus")
if CORPUS.is_dir() and len(section) == 2:
    checked = 0
    body = section[1].split("\n## ", 1)[0]
    for row in body.splitlines():
        if not row.startswith("| ") or row.startswith(("| Question", "| ---")):
            continue
        current, present = None, False
        for span in re.findall(r"`([^`]*)`", row):
            # A path `docs/<file>.md`, then any number of ranges `:a` or `:a-b` (after the path, or alone after a space,
            # a comma or a semicolon): each range belongs to the newest path of the row.
            for m in re.finditer(r"(?<![\w.-])(docs/[^\s`:,;]+\.md)|(?:(?<=\.md)|(?<![^\s,;(]))(?::(\d+)(?:-(\d+))?)(?!\d)", span):
                if m.group(1):
                    current, present = m.group(1), (CORPUS / m.group(1)).is_file()
                    if not present:
                        fail("design_route_missing_file", ROUTES, current)
                    continue
                if not current:
                    fail("design_route_missing_file", ROUTES, f"{m.group(0)} (a range with no path before it)")
                    continue
                if not present:
                    continue
                first, last = m.group(2), m.group(3) or m.group(2)
                lines = (CORPUS / current).read_text(encoding="utf-8", errors="replace").count("\n") + 1
                checked += 1
                if not (1 <= int(first) <= int(last) <= lines):
                    fail("design_route_out_of_range", ROUTES, f"{current}:{first}-{last} (the file has {lines} lines)")
    print(f"design routes: {checked} line ranges checked against {CORPUS}")

for e in errors:
    print(e)
print(f"content check: {len(owned)} files, {len(errors)} errors")
sys.exit(1 if errors else 0)
