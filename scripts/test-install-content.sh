#!/usr/bin/env bash
# Test of scripts/install-content.sh in temporary directories: install, a second install, rollback,
# a damaged kept copy, a failure in the middle of the swap, the record with check, status and rollback without an
# argument, the lock, the sidecar files of a kept copy, and the refusals. Run by ./check (content part).
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
# only the latest kept copy rolls back: an older one would leave a later install's skill in place and unowned.
rm -rf "$t/dst5"; AP_CONTENT_DST=$t/dst5 inst install "$c6"; tara=$(sed -n 's/.*backup=//p' "$t/out")
mkdir -p content/skills/z; echo z > content/skills/z/SKILL.md; c7=$(commit seven)
AP_CONTENT_DST=$t/dst5 inst install "$c7"; tarb=$(sed -n 's/.*backup=//p' "$t/out"); hb=$(tree "$t/dst5")
AP_CONTENT_DST=$t/dst5 inst rollback "$tara"; rc=$?
{ [ "$rc" -ne 0 ] && [ "$(tree "$t/dst5")" = "$hb" ] && grep -q 'rollback_not_latest' "$t/err"; } && ok "older kept copy refused while a later install stands" || bad "older rollback rc=$rc $(cat "$t/err")"
AP_CONTENT_DST=$t/dst5 inst rollback "$tarb"; r1=$?; AP_CONTENT_DST=$t/dst5 inst rollback "$tara"; r2=$?
{ [ "$r1" -eq 0 ] && [ "$r2" -eq 0 ] && [ ! -e "$t/dst5/skills/z" ] && [ ! -e "$t/dst5/AGENTS.md" ] && [ ! -e "$t/dst5/.content-paths" ]; } && ok "rollbacks in reverse order reach the first state" || bad "reverse rollbacks $r1 $r2 $(cat "$t/err")"
# --- the record, check, status, rollback without an argument, the lock and the kept copy's sidecar files.
D=$t/rd; cdst() { AP_CONTENT_DST=$D inst "$@"; }
br=$(git rev-parse --abbrev-ref HEAD)
# install the commit <c> (the installer wants the checkout at it) into $D.
cat_() { git checkout -q "$1" && cdst install "$1"; }
st() { AP_CONTENT_DST=$D bash "$r/scripts/install-content.sh" status 2> "$t/sterr"; }
tarof() { sed -n 's/.*backup=//p' "$t/out"; }
echo r1 > content/AGENTS.md; cr1=$(commit r1); n1=$(grep -c . content/SHA256SUMS); cdst install "$cr1"; rc=$?; tk1=$(tarof); h1=$(tree "$D")
cdst check; rcc=$?
{ [ "$rc" -eq 0 ] && [ "$rcc" -eq 0 ] && grep -qx "ok commit=$cr1 files=$n1" "$t/out"; } && ok "record: check after the first install names the commit and the file count" || bad "check after first install rc=$rcc $(cat "$t/out" "$t/err")"
echo r2 > content/AGENTS.md; mkdir -p content/skills/w; echo w > content/skills/w/SKILL.md; cr2=$(commit r2); n2=$(grep -c . content/SHA256SUMS)
cdst install "$cr2"; rc=$?; tk2=$(tarof); cdst check; rcc=$?; chk_out=$(cat "$t/out"); s2=$(st); rcs=$?
{ [ "$rc" -eq 0 ] && [ "$rcc" -eq 0 ] && [ "$chk_out" = "ok commit=$cr2 files=$n2" ] && [ "$rcs" -eq 0 ] && [ "$(printf '%s\n' "$s2" | wc -l)" -eq 1 ] \
  && printf '%s\n' "$s2" | grep -Eq "^\{\"name\":\"content\",\"commit\":\"$cr2\",\"installed_at\":\"[0-9T:Z-]+\",\"check\":\"ok\",\"rollback\":\"available\"\}$"; } \
  && ok "A1 content: install 1, install 2, check ok, status names version 2 with rollback available" || bad "A1 after install 2 rc=$rc/$rcc/$rcs [$chk_out] [$s2]"
cdst rollback; rc=$?; hr=$(tree "$D"); cdst check; rcc=$?; s1=$(st)
{ [ "$rc" -eq 0 ] && [ "$hr" = "$h1" ] && [ "$rcc" -eq 0 ] && grep -qx "ok commit=$cr1 files=$n1" "$t/out" && [ ! -e "$D/skills/w" ] \
  && printf '%s\n' "$s1" | grep -Eq "^\{\"name\":\"content\",\"commit\":\"$cr1\",\"installed_at\":\"[0-9T:Z-]+\",\"check\":\"ok\",\"rollback\":\"none\"\}$"; } \
  && ok "A1 content: rollback without an argument restores version 1 and its record, check ok, status names version 1" || bad "A1 rollback rc=$rc/$rcc [$s1] $(cat "$t/err")"
cdst rollback; rc=$?
{ [ "$rc" -eq 2 ] && grep -q 'rollback_not_available' "$t/err" && [ "$(tree "$D")" = "$hr" ]; } && ok "A1 content: a second rollback without an argument is refused, target unchanged" || bad "A1 second rollback rc=$rc $(cat "$t/err")"
cat_ "$cr2"; rc=$?; cdst check; rcc=$?
{ [ "$rc" -eq 0 ] && [ "$rcc" -eq 0 ] && grep -qx "ok commit=$cr2 files=$n2" "$t/out"; } && ok "A1 content: install version 2 again, check ok" || bad "A1 reinstall rc=$rc/$rcc $(cat "$t/err")"
# the explicit path of a kept copy still rolls back too (the same bytes as the record-driven one), and a rollback needs a record.
rm -rf "$t/nr"; AP_CONTENT_DST=$t/nr inst rollback; rc=$?
{ [ "$rc" -eq 2 ] && grep -q 'rollback_not_available' "$t/err"; } && ok "rollback without a record is refused" || bad "rollback without record rc=$rc"
# A2: drift. One installed file changed, one removed: check exits 1 and names both; status says drift; neither writes.
hb=$(tree "$D"); echo zzcontentmark >> "$D/runtime-references/A.md"; rm -f "$D/skills/x/SKILL.md"; hd=$(tree "$D")
cdst check; rcc=$?; s=$(st); rcs=$?
{ [ "$rcc" -eq 1 ] && grep -qx 'runtime-references/A.md' "$t/out" && grep -qx 'skills/x/SKILL.md' "$t/out" && [ "$rcs" -eq 0 ] \
  && printf '%s\n' "$s" | grep -q "\"commit\":\"$cr2\".*\"check\":\"drift\"" && [ "$(tree "$D")" = "$hd" ]; } \
  && ok "A2 content: a changed and a removed file make check exit 1 naming both and status say drift, nothing written" || bad "A2 drift rc=$rcc/$rcs [$s] $(cat "$t/out")"
{ ! grep -q zzcontentmark "$t/out" && ! grep -q zzcontentmark "$t/err"; } && ok "check prints paths, never content" || bad "check printed content"
# not recorded: files that an install of an older version left, no record.
rm -rf "$t/nr"; mkdir -p "$t/nr"; echo old > "$t/nr/AGENTS.md"
AP_CONTENT_DST=$t/nr inst check; rcc=$?; s=$(AP_CONTENT_DST=$t/nr bash "$r/scripts/install-content.sh" status); rcs=$?
{ [ "$rcc" -eq 3 ] && grep -q '^not_recorded' "$t/err" && [ "$rcs" -eq 0 ] \
  && [ "$s" = '{"name":"content","commit":null,"installed_at":null,"check":"not_recorded","rollback":"none"}' ]; } \
  && ok "A2 content: a target without a record gives check exit 3 not_recorded and status not_recorded" || bad "not_recorded rc=$rcc/$rcs [$s]"
# A3: one call at a time. A call made while another holds the lock refuses install_busy and the target is unchanged.
rm -rf "$D"; cat_ "$cr1"; hb=$(tree "$D"); L=$t/the.lock; rm -f "$t/held"
( exec 8>"$L"; flock 8; : > "$t/held"; exec sleep 30 ) & hp=$!
for _ in $(seq 1 50); do [ -f "$t/held" ] && break; sleep 0.1; done
git checkout -q "$cr2"; AP_CONTENT_LOCK=$L cdst install "$cr2"; r1=$?; e1=$(cat "$t/err"); AP_CONTENT_LOCK=$L cdst rollback; r2=$?; e2=$(cat "$t/err")
AP_CONTENT_LOCK=$L cdst check; r3=$?
kill "$hp" 2>/dev/null; wait "$hp" 2>/dev/null
{ [ "$r1" -eq 2 ] && [ "$r2" -eq 2 ] && printf '%s\n%s\n' "$e1" "$e2" | grep -c 'install_busy' | grep -qx 2 && [ "$r3" -eq 0 ] && [ "$(tree "$D")" = "$hb" ]; } \
  && ok "A3 content: install and rollback refuse install_busy while the lock is held, check still reads, target unchanged" || bad "A3 lock rc=$r1/$r2/$r3 [$e1] [$e2]"
AP_CONTENT_LOCK=$L cat_ "$cr2"; rc=$?
[ "$rc" -eq 0 ] && ok "the lock is free again once its holder ends" || bad "after the lock rc=$rc $(cat "$t/err")"
# A4: the kept copy's sidecar files are inside its checksum file. A changed or missing one makes rollback refuse before any swap.
rm -rf "$D"; cat_ "$cr1"; cat_ "$cr2"; tk=$(tarof); hb=$(tree "$D"); sc_ok=1
for sc in paths owned-before record-before sums-before; do
  cp -p "$tk.$sc" "$t/sc.$sc" || { sc_ok=0; echo "no sidecar $sc"; continue; }
  echo x >> "$tk.$sc"; cdst rollback; rc=$?; { [ "$rc" -eq 2 ] && grep -q 'checksum' "$t/err" && [ "$(tree "$D")" = "$hb" ]; } || { sc_ok=0; echo "changed $sc rc=$rc"; }
  cp -p "$t/sc.$sc" "$tk.$sc"
  rm -f "$tk.$sc"; cdst rollback; rc=$?; { [ "$rc" -eq 2 ] && [ "$(tree "$D")" = "$hb" ]; } || { sc_ok=0; echo "missing $sc rc=$rc"; }
  cp -p "$t/sc.$sc" "$tk.$sc"
done
[ "$sc_ok" -eq 1 ] && ok "A4 content: a changed or missing sidecar file of the kept copy refuses rollback, target unchanged" || bad "A4 sidecars"
cdst rollback; rc=$?; [ "$rc" -eq 0 ] && ok "the restored sidecar files are accepted again" || bad "A4 restored rc=$rc $(cat "$t/err")"
# a kept copy of today's form (one checksum line, no record sidecar files) still rolls back by path; the record is then absent.
rm -rf "$D"; cat_ "$cr1"; cat_ "$cr2"; tk=$(tarof)
(cd "$(dirname "$tk")" && sha256sum "$(basename "$tk")" > "$tk.sha256"); rm -f "$tk.record-before" "$tk.sums-before"
cdst rollback "$tk"; rc=$?; cdst check; rcc=$?
{ [ "$rc" -eq 0 ] && [ "$(cat "$D/AGENTS.md")" = r1 ] && [ ! -e "$D/skills/w" ] && [ ! -e "$D/.content-record.json" ] && [ "$rcc" -eq 3 ]; } \
  && ok "A4 content: a kept copy of today's form still rolls back by path (the record is then absent)" || bad "A4 old form rc=$rc/$rcc $(cat "$t/err")"
# A5: an install that fails at the swap leaves the files AND the record of the previous install.
rm -rf "$D"; cat_ "$cr1"; hb=$(tree "$D"); rb=$(sha256sum "$D/.content-record.json")
PATH="$t/fakebin:$PATH" FAIL_MV_RE="/\.new-[^ ]*/runtime-references " cat_ "$cr2"; rc=$?; e5=$(cat "$t/err")
cdst check; rcc=$?; s=$(st)
{ [ "$rc" -eq 1 ] && printf '%s\n' "$e5" | grep -q '^install_failed_restored@' && [ "$(tree "$D")" = "$hb" ] && [ "$(sha256sum "$D/.content-record.json")" = "$rb" ] && [ "$rcc" -eq 0 ] && printf '%s\n' "$s" | grep -q "\"commit\":\"$cr1\""; } \
  && ok "A5 content: a failed swap of the files leaves the files and the previous record" || bad "A5 files rc=$rc/$rcc [$s] $e5"
PATH="$t/fakebin:$PATH" FAIL_MV_RE="/\.new-[^ ]*/\.content-record\.json " cat_ "$cr2"; rc=$?; e5=$(cat "$t/err")
cdst check; rcc=$?
{ [ "$rc" -eq 1 ] && printf '%s\n' "$e5" | grep -q '^install_failed_restored@' && [ "$(tree "$D")" = "$hb" ] && [ "$(sha256sum "$D/.content-record.json")" = "$rb" ] && [ "$rcc" -eq 0 ] && grep -qx "ok commit=$cr1 files=$n1" "$t/out"; } \
  && ok "A5 content: a failure after the files were swapped, at the record, puts files and record back" || bad "A5 record rc=$rc/$rcc $e5"
# --- round 2: rollback without an argument runs only when the target equals its record; one decision for status and rollback.
mkdir -p "$t/killbin"; printf '#!/bin/sh\n/bin/mv "$@" || exit $?\nif printf "%%s" "$*" | grep -Eq -- "$KILL_AFTER_MV_RE"; then kill -9 "$PPID"; fi\nexit 0\n' > "$t/killbin/mv"; chmod +x "$t/killbin/mv"
js_ok() { printf '%s' "$1" | python3 -I -c 'import json,sys; d=json.loads(sys.stdin.read()); assert d["name"]=="content"' 2> /dev/null; }
# a kill inside the swap of install 2 leaves new files under the record of install 1: rollback without an argument refuses.
rm -rf "$D"; cat_ "$cr1"
PATH="$t/killbin:$PATH" KILL_AFTER_MV_RE="/\.new-[^ ]*/runtime-references " cat_ "$cr2"; rk9=$?; hk=$(tree "$D")
cdst check; rcc=$?; s=$(st); cdst rollback; rc=$?; ek=$(cat "$t/err")
{ [ "$rk9" -eq 137 ] && [ "$rcc" -eq 1 ] && printf '%s' "$s" | grep -q '"check":"drift","rollback":"none"' && [ "$rc" -eq 2 ] \
  && printf '%s' "$ek" | grep -q 'rollback_target_not_recorded_state' && printf '%s' "$ek" | grep -q 'rollback <tarball>' && [ "$(tree "$D")" = "$hk" ]; } \
  && ok "R2 content: after a kill inside the swap, rollback without an argument refuses rollback_target_not_recorded_state and changes nothing" || bad "R2 kill rc=$rk9/$rcc/$rc [$s] [$ek]"
# the leftover directory of an interrupted run alone is enough: the target equals its record (check ok) but rollback refuses until it is gone.
rm -rf "$D"; cat_ "$cr1"; cat_ "$cr2"; mkdir "$D/.old-leftover"; hl=$(tree "$D")
cdst check; rcc=$?; s=$(st); cdst rollback; rc=$?
{ [ "$rcc" -eq 0 ] && printf '%s' "$s" | grep -q '"check":"ok","rollback":"none"' && [ "$rc" -eq 2 ] && grep -q 'rollback_target_not_recorded_state' "$t/err" && [ "$(tree "$D")" = "$hl" ]; } \
  && ok "R2 content: a .old-* directory of an interrupted run blocks rollback without an argument (check ok, status none)" || bad "R2 leftover rc=$rcc/$rc [$s]"
rmdir "$D/.old-leftover"; s=$(st); cdst rollback; rc=$?
{ printf '%s' "$s" | grep -q '"rollback":"available"' && [ "$rc" -eq 0 ]; } && ok "R2 content: without the leftover status says available and rollback runs" || bad "R2 clean rc=$rc [$s]"
# status and rollback decide alike: a record naming a kept copy outside the backup directory.
rm -rf "$D" "$t/elsewhere"; cat_ "$cr1"; cat_ "$cr2"; tk2=$(tarof); mkdir "$t/elsewhere"; cp -p "$tk2"* "$t/elsewhere/"
sed -i "s|\"kept\":\"[^\"]*/|\"kept\":\"$t/elsewhere/|" "$D/.content-record.json"; hb=$(tree "$D")
s=$(st); cdst rollback; rc=$?
{ printf '%s' "$s" | grep -q '"check":"ok","rollback":"none"' && [ "$rc" -eq 2 ] && grep -q 'rollback_not_available' "$t/err" && [ "$(tree "$D")" = "$hb" ]; } \
  && ok "R2 content: a kept copy outside the backup directory is none in status and refused by rollback" || bad "R2 outside rc=$rc [$s] $(cat "$t/err")"
# a damaged record or checksum list is drift, never ok; status stays one valid JSON line.
rm -rf "$D"; cat_ "$cr1"; cat_ "$cr2"; cp -p "$D/.content-record.json" "$t/rec.good"; cp -p "$D/.content-sums" "$t/sums.good"; dm_ok=1
damage() { # <name> <record command or ->  <sums command or ->
  cp -p "$t/rec.good" "$D/.content-record.json"; cp -p "$t/sums.good" "$D/.content-sums"
  [ "$2" = - ] || eval "$2"; [ "$3" = - ] || eval "$3"
  cdst check; local c=$?; local sj; sj=$(st); local rs=$?
  { [ "$c" -eq 1 ] && [ "$rs" -eq 0 ] && [ "$(printf '%s\n' "$sj" | wc -l)" -eq 1 ] && js_ok "$sj" && printf '%s' "$sj" | grep -q '"check":"drift"' && printf '%s' "$sj" | grep -q '"rollback":"none"'; } || { dm_ok=0; echo "damage $1: check rc=$c [$sj]"; }
}
damage empty-record ': > "$D/.content-record.json"' -
damage cut-record 'head -c 40 "$t/rec.good" > "$D/.content-record.json"' -
damage not-json 'echo garbage > "$D/.content-record.json"' -
damage control-characters 'sed "s/\"installed_at\":\"[^\"]*\"/\"installed_at\":\"a\\\\b\"/" "$t/rec.good" > "$D/.content-record.json"' -
damage tab-and-quote 'printf "{\"commit\":\"x\ty\",\"installed_at\":\"a\\\"b\",\"kept\":null,\"files\":1}\n" > "$D/.content-record.json"' -
damage cut-sums-mid-line - 'head -c -10 "$t/sums.good" > "$D/.content-sums"'
damage cut-sums-at-line - 'head -n 2 "$t/sums.good" > "$D/.content-sums"'
damage empty-sums - ': > "$D/.content-sums"'
damage files-differs 'sed "s/\"files\":[0-9]*/\"files\":99/" "$t/rec.good" > "$D/.content-record.json"' -
[ "$dm_ok" -eq 1 ] && ok "R2 content: an empty, cut or non-JSON record and a cut or short checksum list are drift, status stays valid JSON" || bad "R2 damaged state"
cp -p "$t/rec.good" "$D/.content-record.json"; cp -p "$t/sums.good" "$D/.content-sums"; cdst check; [ $? -eq 0 ] && ok "R2 content: the undamaged record is ok again" || bad "R2 restored record"
# a refused install creates neither the kept-copy directory nor the lock file (the refusals come first).
rm -rf "$t/bk-fresh" "$t/dst-fresh"
AP_CONTENT_BACKUP_DIR=$t/bk-fresh AP_CONTENT_DST=$t/dst-fresh inst install "$cr1"; rc=$?
echo dirty >> content/AGENTS.md; AP_CONTENT_BACKUP_DIR=$t/bk-fresh AP_CONTENT_DST=$t/dst-fresh inst install "$(git rev-parse HEAD)"; rd=$?; git checkout -q -- content
{ [ "$rc" -eq 2 ] && [ "$rd" -eq 2 ] && [ ! -e "$t/bk-fresh" ] && [ ! -e "$t/dst-fresh" ]; } && ok "R2 content: a refused install creates no kept-copy directory, no lock file and no target" || bad "R2 refusal side effects rc=$rc/$rd"
# a rollback that completed but could not write its marker exits 4 and says the rollback completed.
rm -rf "$D"; cat_ "$cr1"; tk1=$(tarof); cat_ "$cr2"; mkdir "$tk1.consumed"
cdst rollback; rc=$?; e4=$(cat "$t/err"); cdst check; rcc=$?
{ [ "$rc" -eq 4 ] && printf '%s' "$e4" | grep -q 'rollback_completed_marker_failed' && printf '%s' "$e4" | grep -q 'rollback itself completed' && [ "$(cat "$D/AGENTS.md")" = r1 ] && [ "$rcc" -eq 0 ]; } \
  && ok "R2 content: a failed marker after a completed rollback exits 4 and says the rollback itself completed" || bad "R2 marker rc=$rc/$rcc [$e4]"
# check names at most 10 paths.
for i in $(seq 1 12); do mkdir -p content/skills/m$i; echo m$i > content/skills/m$i/SKILL.md; done; cm=$(commit many)
rm -rf "$D"; cat_ "$cm"; for i in $(seq 1 12); do rm -f "$D/skills/m$i/SKILL.md"; done
cdst check; rcc=$?; nl=$(grep -c '^skills/m' "$t/out")
{ [ "$rcc" -eq 1 ] && [ "$nl" -eq 10 ]; } && ok "check names at most 10 paths" || bad "check path limit rc=$rcc lines=$nl"
git checkout -q "$br"
# refusals: uncommitted content; a commit that is not on origin/main; a checkout at another commit.
echo dirty >> content/AGENTS.md; inst install "$c7"; rc=$?; git checkout -q -- content
[ "$rc" -eq 2 ] && ok "uncommitted content refused" || bad "uncommitted rc=$rc"
echo three > content/AGENTS.md; sums; git add -A; git commit -q -m three; c3=$(git rev-parse HEAD)
inst install "$c3"; rc=$?; [ "$rc" -eq 2 ] && ok "commit not on origin/main refused" || bad "unmerged rc=$rc"
inst install "$c1"; rc=$?; [ "$rc" -eq 2 ] && ok "checkout at another commit refused" || bad "other commit rc=$rc"
[ "$fails" -eq 0 ] && echo "test-install-content: PASS" || { echo "test-install-content: $fails FAILED"; exit 1; }
