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
# refusals: uncommitted content; a commit that is not on origin/main; a checkout at another commit.
echo dirty >> content/AGENTS.md; inst install "$c2"; rc=$?; git checkout -q -- content
[ "$rc" -eq 2 ] && ok "uncommitted content refused" || bad "uncommitted rc=$rc"
echo three > content/AGENTS.md; sums; git add -A; git commit -q -m three; c3=$(git rev-parse HEAD)
inst install "$c3"; rc=$?; [ "$rc" -eq 2 ] && ok "commit not on origin/main refused" || bad "unmerged rc=$rc"
inst install "$c1"; rc=$?; [ "$rc" -eq 2 ] && ok "checkout at another commit refused" || bad "other commit rc=$rc"
[ "$fails" -eq 0 ] && echo "test-install-content: PASS" || { echo "test-install-content: $fails FAILED"; exit 1; }
