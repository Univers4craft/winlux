#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=${WINLUX_UPDATE_TEST_VERSION:-test-local}
test_root=$(mktemp -d "${TMPDIR:-/tmp}/winlux-update-test.XXXXXX")
trap 'rm -rf "$test_root"' EXIT HUP INT TERM

mkdir -p "$test_root/root/usr/bin" "$test_root/root/usr/share/winlux"
printf '#!/bin/sh\necho old\n' >"$test_root/root/usr/bin/winlux"
chmod 755 "$test_root/root/usr/bin/winlux"
printf 'old\n' >"$test_root/root/usr/share/winlux/version"

"$root/scripts/build-update-bundle.sh" "$version" >/dev/null
WINLUX_UPDATE_ROOT="$test_root/root" \
WINLUX_UPDATE_MANIFEST="file://$root/build/winlux-update-manifest.txt" \
WINLUX_UPDATE_ASSET_DIR="$root/build" \
    "$root/scripts/winlux-update.sh" --apply
test "$(sed -n '1p' "$test_root/root/usr/share/winlux/version")" = "$version"
test -x "$test_root/root/usr/bin/winlux"

WINLUX_UPDATE_ROOT="$test_root/root" "$root/scripts/winlux-update.sh" --rollback
test "$(sed -n '1p' "$test_root/root/usr/share/winlux/version")" = old
test "$("$test_root/root/usr/bin/winlux")" = old
echo "Mise à jour atomique et retour arrière validés."
