#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-notes-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/root/usr/share/winlux" "$work/assets"
printf 'older\n' >"$work/root/usr/share/winlux/version"
printf '# Nouveautés\n\n- Correction détaillée avec accents : réseau, paramètres.\n' >"$work/notes.txt"
printf '#!/bin/sh\nexit 0\n' >"$work/runtime"
chmod 755 "$work/runtime"
WINLUX_UPDATE_NOTES="$work/notes.txt" WINLUX_UPDATE_RUNTIME="$work/runtime" \
    sh "$root/scripts/build-update-bundle.sh" --runtime notes-test >/dev/null
cp "$root/build/winlux-update-manifest.txt" "$work/assets/manifest.txt"
cp "$root/build/winlux-release-notes.txt" "$work/assets/"
update() {
    WINLUX_UPDATE_ROOT="$work/root" WINLUX_UPDATE_MANIFEST="file://$work/assets/manifest.txt" \
    WINLUX_UPDATE_ASSET_DIR="$work/assets" sh "$root/scripts/winlux-update.sh" "$@"
}
update --check >"$work/output"
grep -q 'Modifications détaillées' "$work/output"
grep -q 'Correction détaillée avec accents' "$work/output"
cmp "$work/notes.txt" "$work/root/var/lib/winlux/available-notes"
rm "$work/assets/winlux-release-notes.txt"
update --list >"$work/offline"
grep -q 'Correction détaillée avec accents' "$work/offline"
# A tampered download must leave the previously verified cache untouched.
printf 'notes modifiées\n' >"$work/assets/winlux-release-notes.txt"
if update --check >"$work/error" 2>&1; then echo 'Corrupt notes accepted' >&2; exit 1; fi
grep -q 'SHA-256 des notes incorrect' "$work/error"
cmp "$work/notes.txt" "$work/root/var/lib/winlux/available-notes"
# Old manifests and releases remain usable without inventing changes.
sed '/^notes_asset=/d; /^notes_sha256=/d' "$work/assets/manifest.txt" >"$work/old-manifest"
cp "$work/old-manifest" "$work/assets/manifest.txt"
update --check >"$work/legacy"
grep -q 'ne fournit pas encore de notes détaillées' "$work/legacy"
echo 'OK: detailed notes, French accents, offline cache, checksum rejection and legacy releases'
