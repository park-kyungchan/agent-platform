#!/usr/bin/env bash
# Test of scripts/install-content.sh in temporary directories: install, a second install, rollback,
# a damaged kept copy, a failure in the middle of the swap, and the refusals. Run by ./check (content part).
set -uo pipefail
script=${1:-$(cd "$(dirname "$0")" && pwd -P)/install-content.sh}
t=$(mktemp -d); trap 'rm -rf -- "$t"' EXIT
r=$t/repo; mkdir -p "$r/scripts" "$r/content/runtime-references" "$r/content/skills/x"
cp "$script" "$r/scripts/install-content.sh"
cd "$r" && git init -q . && git config user.email t@example.invalid && git config user.name t
sums() { (cd "$r/content" && find . -type f ! -name SHA256SUMS | sed 's|^\./||' | sort | xargs sha256sum > SHA256SUMS); }
commit() { sums; git add -A; git commit -q -m "$1"; git update-ref refs/remotes/origin/main HEAD; git rev-parse HEAD; }
tree() { (cd "$1" && find . -type f ! -path './.new-*' ! -path './.old-*' | sort | xargs sha256sum 2>/dev/null | sha256sum | cut -c1-16); }
echo one > content/AGENTS.md; echo ref1 > content/runtime-references/A.md; echo skill1 > content/skills/x/SKILL.md
c1=$(commit one)
export AP_CONTENT_DST=$t/dst AP_CONTENT_BACKUP_DIR=$t/bk
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
inst() { bash "$r/scripts/install-content.sh" "$@" > "$t/out" 2> "$t/err"; }
inst install "$c1"; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(cat "$t/dst/skills/x/SKILL.md")" = skill1 ] && ! ls -d "$t/dst"/.new-* "$t/dst"/.old-* > /dev/null 2>&1; } && ok "first install into an empty target" || bad "first install rc=$rc $(cat "$t/err")"
h1=$(tree "$t/dst")
echo two > content/AGENTS.md; mkdir -p content/skills/y; echo skill-y > content/skills/y/SKILL.md
c2=$(commit two); inst install "$c2"; rc=$?
tar2=$(sed -n 's/.*backup=//p' "$t/out")
{ [ "$rc" -eq 0 ] && [ "$(cat "$t/dst/AGENTS.md")" = two ] && [ -f "$t/dst/skills/y/SKILL.md" ] && [ -f "$tar2" ]; } && ok "second install replaces and keeps a copy" || bad "second install rc=$rc"
h2=$(tree "$t/dst")
# a damaged kept copy is refused and nothing changes.
cp "$tar2" "$t/bad.tar"; cp "$tar2.sha256" "$t/bad.tar.sha256"; sed -i "s|$(basename "$tar2")|bad.tar|" "$t/bad.tar.sha256"; cp "$tar2.paths" "$t/bad.tar.paths"; echo x >> "$t/bad.tar"
inst rollback "$t/bad.tar"; rc=$?
{ [ "$rc" -eq 2 ] && [ "$(tree "$t/dst")" = "$h2" ]; } && ok "damaged kept copy refused, target unchanged" || bad "damaged copy rc=$rc"
# rollback puts the first install back; the skill the second install added is gone.
inst rollback "$tar2"; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(tree "$t/dst")" = "$h1" ] && [ ! -e "$t/dst/skills/y" ]; } && ok "rollback restores the previous bytes" || bad "rollback rc=$rc $(cat "$t/err")"
# a failure in the middle of the swap restores what was already moved.
rm -rf "$t/dst2"; mkdir -p "$t/dst2/runtime-references"; echo old > "$t/dst2/AGENTS.md"; echo oldref > "$t/dst2/runtime-references/A.md"; echo not-a-directory > "$t/dst2/skills"
hb=$(tree "$t/dst2"); AP_CONTENT_DST=$t/dst2 inst install "$c2"; rc=$?
{ [ "$rc" -eq 1 ] && [ "$(tree "$t/dst2")" = "$hb" ] && grep -q '^install_failed_restored@' "$t/err"; } && ok "failed swap restores the target" || bad "failed swap rc=$rc $(cat "$t/err")"
# a path moves into the recovery list only after its move succeeded: a failing move of an EXISTING path
# (injected by a wrapper of mv on PATH) must leave that path where it was.
mkdir -p "$t/fakebin"; printf '#!/bin/sh\nif printf "%%s" "$*" | grep -Eq -- "$FAIL_MV_RE"; then exit 1; fi\nexec /bin/mv "$@"\n' > "$t/fakebin/mv"; chmod +x "$t/fakebin/mv"
rm -rf "$t/dst3"; AP_CONTENT_DST=$t/dst3 inst install "$c2"; hb=$(tree "$t/dst3")
echo five > content/AGENTS.md; c5=$(commit five)
PATH="$t/fakebin:$PATH" FAIL_MV_RE="dst3/runtime-references [^ ]*/\.old-" AP_CONTENT_DST=$t/dst3 inst install "$c5"; rc=$?
{ [ "$rc" -eq 1 ] && [ "$(tree "$t/dst3")" = "$hb" ] && [ -d "$t/dst3/runtime-references" ]; } && ok "failed move of an existing path leaves it in place" || bad "failed old move rc=$rc $(cat "$t/err")"
# a rollback that fails in the middle puts the target back as it was before the rollback.
AP_CONTENT_DST=$t/dst3 inst install "$c5"; tar5=$(sed -n 's/.*backup=//p' "$t/out"); hb=$(tree "$t/dst3")
PATH="$t/fakebin:$PATH" FAIL_MV_RE="/\.new-[^ ]*/runtime-references " AP_CONTENT_DST=$t/dst3 inst rollback "$tar5"; rc=$?
{ [ "$rc" -eq 1 ] && [ "$(tree "$t/dst3")" = "$hb" ] && grep -q '^rollback_failed_restored@' "$t/err"; } && ok "failed rollback restores the target" || bad "failed rollback rc=$rc $(cat "$t/err")"
# a skill the content no longer names is removed; a skill this installer never installed stays; rollback brings the retired one back.
mkdir -p "$t/dst3/skills/third-party"; echo theirs > "$t/dst3/skills/third-party/SKILL.md"
git rm -q -r content/skills/y; c6=$(commit six); AP_CONTENT_DST=$t/dst3 inst install "$c6"; rc=$?; tar6=$(sed -n 's/.*backup=//p' "$t/out")
{ [ "$rc" -eq 0 ] && [ ! -e "$t/dst3/skills/y" ] && [ -f "$t/dst3/skills/x/SKILL.md" ] && [ -f "$t/dst3/skills/third-party/SKILL.md" ]; } && ok "retired skill removed, foreign skill kept" || bad "retired skill rc=$rc $(cat "$t/err")"
AP_CONTENT_DST=$t/dst3 inst rollback "$tar6"; rc=$?
{ [ "$rc" -eq 0 ] && [ -f "$t/dst3/skills/y/SKILL.md" ] && [ -f "$t/dst3/skills/third-party/SKILL.md" ] && grep -qx 'skills/y' "$t/dst3/.content-paths"; } && ok "rollback brings the retired skill back" || bad "rollback of retirement rc=$rc $(cat "$t/err")"
# a previous path that is a relative symbolic link survives install and rollback as that link.
rm -rf "$t/dst4"; mkdir -p "$t/dst4"; echo linked > "$t/original.md"; ln -s ../original.md "$t/dst4/AGENTS.md"
AP_CONTENT_DST=$t/dst4 inst install "$c6"; rc=$?; tar7=$(sed -n 's/.*backup=//p' "$t/out")
{ [ "$rc" -eq 0 ] && [ ! -L "$t/dst4/AGENTS.md" ]; } || bad "install over a link rc=$rc $(cat "$t/err")"
AP_CONTENT_DST=$t/dst4 inst rollback "$tar7"; rc=$?
{ [ "$rc" -eq 0 ] && [ -L "$t/dst4/AGENTS.md" ] && [ "$(readlink "$t/dst4/AGENTS.md")" = ../original.md ] && [ "$(cat "$t/dst4/AGENTS.md")" = linked ]; } && ok "relative symbolic link restored by rollback" || bad "link rollback rc=$rc $(ls -la "$t/dst4" | tr '\n' ' ')"
# refusals: uncommitted content; a commit that is not on origin/main; a checkout at another commit.
echo dirty >> content/AGENTS.md; inst install "$c6"; rc=$?; git checkout -q -- content
[ "$rc" -eq 2 ] && ok "uncommitted content refused" || bad "uncommitted rc=$rc"
echo three > content/AGENTS.md; sums; git add -A; git commit -q -m three; c3=$(git rev-parse HEAD)
inst install "$c3"; rc=$?; [ "$rc" -eq 2 ] && ok "commit not on origin/main refused" || bad "unmerged rc=$rc"
inst install "$c1"; rc=$?; [ "$rc" -eq 2 ] && ok "checkout at another commit refused" || bad "other commit rc=$rc"
[ "$fails" -eq 0 ] && echo "test-install-content: PASS" || { echo "test-install-content: $fails FAILED"; exit 1; }
