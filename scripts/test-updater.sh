#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_root=$(mktemp -d "${TMPDIR:-/tmp}/winlux-update-test.XXXXXX")
trap 'rm -rf "$test_root"' EXIT HUP INT TERM
updater="$root/scripts/winlux-update.sh"
fixture="$test_root/root"
stage="$test_root/stage"
boot="$test_root/boot"
assets="$test_root/assets"
mkdir -p "$fixture/usr/bin" "$fixture/usr/share/winlux" "$fixture/home/user" "$fixture/etc/winlux" "$assets"
printf '#!/bin/sh\necho old\n' >"$fixture/usr/bin/winlux"
chmod 755 "$fixture/usr/bin/winlux"
printf 'old\n' >"$fixture/usr/share/winlux/version"
printf 'my setting\n' >"$fixture/etc/winlux/settings.conf"
printf 'my document\n' >"$fixture/home/user/document.txt"
printf '#!/bin/sh\necho new\n' >"$test_root/runtime"
chmod 755 "$test_root/runtime"

update() { WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST="file://$assets/manifest.txt" WINLUX_UPDATE_ASSET_DIR="$assets" "$updater" "$@"; }
expect_failure() {
    if "$@" >"$test_root/expected-error.log" 2>&1; then echo "échec attendu mais commande réussie : $*" >&2; exit 1; fi
    test "$(cat "$fixture/usr/share/winlux/version")" = "$expected_version"
}
build_runtime() {
    WINLUX_UPDATE_RUNTIME="$test_root/runtime" "$root/scripts/build-update-bundle.sh" --runtime "$1" >/dev/null
    cp "$root/build/winlux-update-$1.tar.gz" "$assets/"
    cp "$root/build/winlux-update-manifest.txt" "$assets/manifest.txt"
}
publish_custom() {
    custom_version=$1
    tar -C "$test_root/custom" -czf "$assets/custom.tar.gz" META FILES LINKS payload
    { cat "$test_root/custom/META"; printf 'asset=custom.tar.gz\nsha256=%s\n' "$(sha256sum "$assets/custom.tar.gz" | cut -d' ' -f1)"; } >"$assets/manifest.txt"
}
reset_custom() {
    rm -rf "$test_root/custom"
    mkdir -p "$test_root/custom"
    tar -xzf "$assets/winlux-update-updater-runtime.tar.gz" -C "$test_root/custom"
}

test "$(update --auto status)" = disabled
WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST=file:///not-present "$updater" --auto-run
update --list | grep -q 'Aucune vérification'
build_runtime updater-runtime
update --check

# Private repositories use a guest-specific credential file. A fake curl checks
# the selected authorization header without making any network call or logging
# the secret, and confirms environment overrides retain their priority.
mkdir "$test_root/fake-network"
printf '{"assets":[{"url":"https://api.github.com/repos/Univers4craft/winlux/releases/assets/123","name":"winlux-update-manifest.txt"}]}\n' >"$test_root/release.json"
cat >"$test_root/fake-network/curl" <<'CURL'
#!/bin/sh
set -eu
authorized=0
destination=
url=
while test "$#" -gt 0; do
    case "$1" in
        --header) if test "$2" = "Authorization: Bearer $WINLUX_TEST_EXPECT_TOKEN"; then authorized=1; fi; shift 2 ;;
        --output) destination=$2; shift 2 ;;
        https://*) url=$1; shift ;;
        *) shift ;;
    esac
done
test "$authorized" = 1 || { echo 'autorisation de test incorrecte' >&2; exit 1; }
case "$url" in
    */releases/latest) cp "$WINLUX_TEST_RELEASE" "$destination" ;;
    */releases/assets/123) cp "$WINLUX_TEST_MANIFEST" "$destination" ;;
    *) exit 1 ;;
esac
CURL
chmod 755 "$test_root/fake-network/curl"
printf 'file_token\n' >"$fixture/etc/winlux/update.token"
chmod 600 "$fixture/etc/winlux/update.token"
env -u WINLUX_UPDATE_MANIFEST -u WINLUX_UPDATE_TOKEN -u GH_TOKEN PATH="$test_root/fake-network:$PATH" WINLUX_UPDATE_ROOT="$fixture" WINLUX_TEST_EXPECT_TOKEN=file_token WINLUX_TEST_RELEASE="$test_root/release.json" WINLUX_TEST_MANIFEST="$assets/manifest.txt" "$updater" --check >"$test_root/token-test.log" 2>&1
env -u WINLUX_UPDATE_MANIFEST PATH="$test_root/fake-network:$PATH" WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_TOKEN=explicit_token GH_TOKEN=github_token WINLUX_TEST_EXPECT_TOKEN=explicit_token WINLUX_TEST_RELEASE="$test_root/release.json" WINLUX_TEST_MANIFEST="$assets/manifest.txt" "$updater" --check >>"$test_root/token-test.log" 2>&1
env -u WINLUX_UPDATE_MANIFEST -u WINLUX_UPDATE_TOKEN PATH="$test_root/fake-network:$PATH" WINLUX_UPDATE_ROOT="$fixture" GH_TOKEN=github_token WINLUX_TEST_EXPECT_TOKEN=github_token WINLUX_TEST_RELEASE="$test_root/release.json" WINLUX_TEST_MANIFEST="$assets/manifest.txt" "$updater" --check >>"$test_root/token-test.log" 2>&1
if grep -Eq 'file_token|explicit_token|github_token' "$test_root/token-test.log"; then echo 'jeton affiché dans le journal' >&2; exit 1; fi
chmod 644 "$fixture/etc/winlux/update.token"
if env -u WINLUX_UPDATE_TOKEN -u GH_TOKEN WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST="file://$assets/manifest.txt" "$updater" --check >>"$test_root/token-test.log" 2>&1; then echo 'jeton non privé accepté' >&2; exit 1; fi
rm "$fixture/etc/winlux/update.token"

WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST=file:///not-present "$updater" --list | grep -q 'runtime,updater'
update --apply
test "$(cat "$fixture/usr/share/winlux/version")" = updater-runtime
test "$("$fixture/usr/bin/winlux")" = new
test "$(cat "$fixture/run/winlux/update-action")" = restart-session
cmp -s "$updater" "$fixture/usr/bin/winlux-update"
WINLUX_UPDATE_ROOT="$fixture" "$fixture/usr/bin/winlux-update" --list | grep -q 'à jour'
update --rollback
test "$(cat "$fixture/usr/share/winlux/version")" = old
test "$("$fixture/usr/bin/winlux")" = old
test ! -e "$fixture/usr/bin/winlux-update"
expected_version=old

# Exercise the same BusyBox utilities available inside the initramfs, not only
# the GNU tools installed on the development machine.
mkdir -p "$test_root/busybox-tools"
for applet in cp cut grep head mkdir mktemp mv rm sed sha256sum tar tr wc find sort uniq readlink dirname chmod ln rmdir awk cmp sync; do
    ln -s /bin/busybox "$test_root/busybox-tools/$applet"
done
PATH="$test_root/busybox-tools" WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST="file://$assets/manifest.txt" WINLUX_UPDATE_ASSET_DIR="$assets" /bin/busybox sh "$updater" --apply >/dev/null
test "$("$fixture/usr/bin/winlux")" = new
PATH="$test_root/busybox-tools" WINLUX_UPDATE_ROOT="$fixture" /bin/busybox sh "$updater" --rollback >/dev/null
test "$("$fixture/usr/bin/winlux")" = old

# Simulate an interrupted process and stale lock. Recovery happens before the
# next check and restores the complete previous transaction first.
saved_backup=$(cat "$fixture/var/lib/winlux/rollback-current")
printf '%s\n' "$saved_backup" >"$fixture/var/lib/winlux/update-transaction"
printf '#!/bin/sh\necho interrupted\n' >"$fixture/usr/bin/winlux"
mkdir "$fixture/var/lib/winlux/update.lock"
printf '2147483647\n' >"$fixture/var/lib/winlux/update.lock/pid"
update --check >/dev/null
test "$("$fixture/usr/bin/winlux")" = old
test ! -e "$fixture/var/lib/winlux/update-transaction"

# A bad checksum, duplicate record, unexpected payload, or archive link must fail
# before changing the installed version or touching the user's files.
cp "$assets/manifest.txt" "$test_root/good-manifest"
sed 's/^sha256=.*/sha256=0000000000000000000000000000000000000000000000000000000000000000/' "$test_root/good-manifest" >"$assets/manifest.txt"
expect_failure update --apply
cp "$test_root/good-manifest" "$assets/manifest.txt"
reset_custom
cat "$test_root/custom/FILES" >>"$test_root/custom/FILES.copy"
cat "$test_root/custom/FILES.copy" >>"$test_root/custom/FILES"
rm "$test_root/custom/FILES.copy"
publish_custom updater-runtime
expect_failure update --apply
reset_custom
printf 'unexpected\n' >"$test_root/custom/payload/usr/bin/not-in-inventory"
publish_custom updater-runtime
expect_failure update --apply
reset_custom
ln -s /etc/shadow "$test_root/custom/payload/usr/bin/unsafe"
publish_custom updater-runtime
expect_failure update --apply
reset_custom
ln "$test_root/custom/payload/usr/bin/winlux" "$test_root/custom/payload/usr/bin/hardlink"
publish_custom updater-runtime
expect_failure update --apply
reset_custom
tar -C "$test_root/custom" --transform='s#^payload/usr/bin/winlux$#payload/../../escaped#' -czf "$assets/custom.tar.gz" META FILES LINKS payload
{ cat "$test_root/custom/META"; printf 'asset=custom.tar.gz\nsha256=%s\n' "$(sha256sum "$assets/custom.tar.gz" | cut -d' ' -f1)"; } >"$assets/manifest.txt"
expect_failure update --apply
test ! -e "$test_root/escaped"
reset_custom
printf 'usr/bin/unsafe ../../../../etc/shadow\n' >"$test_root/custom/LINKS"
publish_custom updater-runtime
expect_failure update --apply
reset_custom
sed 's/^version=.*/version=wrong/' "$test_root/custom/META" >"$test_root/META"
mv "$test_root/META" "$test_root/custom/META"
publish_custom updater-runtime
sed 's/^version=.*/version=updater-runtime/' "$assets/manifest.txt" >"$test_root/manifest"
mv "$test_root/manifest" "$assets/manifest.txt"
expect_failure update --apply
cp "$test_root/good-manifest" "$assets/manifest.txt"
mkdir -p "$fixture/var/lib/winlux/update.lock"
expect_failure update --apply
rmdir "$fixture/var/lib/winlux/update.lock"

# A destination parent that escapes WINLUX_UPDATE_ROOT is rejected.
mkdir -p "$test_root/escaped"
mv "$fixture/usr/bin" "$fixture/usr/bin-safe"
ln -s "$test_root/escaped" "$fixture/usr/bin"
expect_failure update --apply
test ! -e "$test_root/escaped/winlux"
rm "$fixture/usr/bin"
mv "$fixture/usr/bin-safe" "$fixture/usr/bin"

# Inject a real filesystem failure after the runtime has been replaced. A
# directory blocking the later atomic version-file rename cannot be removed by
# rm -f. This also tests rollback with the installer's private static utilities.
(
    while ! test -f "$test_root/start-installer"; do sleep 0.01; done
    exec env WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST="file://$assets/manifest.txt" WINLUX_UPDATE_ASSET_DIR="$assets" "$updater" --apply
) >"$test_root/expected-error.log" 2>&1 &
installer_pid=$!
mkdir "$fixture/usr/share/winlux/version.winlux-new.$installer_pid"
touch "$test_root/start-installer"
if wait "$installer_pid"; then echo 'installation en échec attendue' >&2; exit 1; fi
test "$(cat "$fixture/usr/share/winlux/version")" = "$expected_version"
test "$("$fixture/usr/bin/winlux")" = old
test ! -e "$fixture/usr/bin/winlux-update"
rmdir "$fixture/usr/share/winlux/version.winlux-new.$installer_pid"

# A complete fixture includes shell, desktop, helper, kernel, modules, libraries,
# symbolic links and new defaults; user configuration must survive its update.
mkdir -p "$stage/bin" "$stage/usr/bin" "$stage/usr/share/winlux" "$stage/lib/modules/test-kernel" "$stage/lib64" "$stage/opt/winlux/tests" "$stage/etc/winlux" "$boot"
cp "$test_root/runtime" "$stage/usr/bin/winlux"
printf '#!/bin/sh\nexit 0\n' >"$stage/init"
printf '#!/bin/sh\nexit 0\n' >"$stage/bin/busybox"
chmod 755 "$stage/init" "$stage/bin/busybox"
ln -s busybox "$stage/bin/sh"
ln -s ../../bin/busybox "$stage/usr/bin/dirname"
printf 'desktop-new\n' >"$stage/opt/winlux/tests/gui_demo.exe"
printf 'driver-new\n' >"$stage/lib/modules/test-kernel/input.ko"
printf 'library-new\n' >"$stage/lib64/winlux.so"
printf 'new default\n' >"$stage/etc/winlux/settings.conf"
printf 'kernel-new\n' >"$boot/vmlinuz"
printf 'initramfs-new\n' >"$boot/winlux-initramfs.cpio.gz"
printf 'updater-system\n' >"$stage/usr/share/winlux/version"
if test -f "$root/scripts/source-fingerprint.sh"; then sh "$root/scripts/source-fingerprint.sh" >"$stage/usr/share/winlux/source-fingerprint"; fi
WINLUX_UPDATE_STAGE="$stage" WINLUX_UPDATE_BOOT="$boot" "$root/scripts/build-update-bundle.sh" --system updater-system >/dev/null
cp "$root/build/winlux-update-updater-system.tar.gz" "$assets/"
cp "$root/build/winlux-update-manifest.txt" "$assets/manifest.txt"
mkdir -p "$fixture/opt/winlux/tests" "$fixture/boot" "$fixture/lib/modules/test-kernel"
printf 'desktop-old\n' >"$fixture/opt/winlux/tests/gui_demo.exe"
printf 'kernel-old\n' >"$fixture/boot/vmlinuz"
printf 'driver-old\n' >"$fixture/lib/modules/test-kernel/input.ko"
update --auto enable
test "$(update --auto status)" = enabled
update --auto-run
test "$(cat "$fixture/run/winlux/update-action")" = reboot
test "$(cat "$fixture/usr/share/winlux/update-scope")" = system
test "$(cat "$fixture/opt/winlux/tests/gui_demo.exe")" = desktop-new
test "$(cat "$fixture/boot/vmlinuz")" = kernel-new
test "$(cat "$fixture/lib/modules/test-kernel/input.ko")" = driver-new
test "$(readlink "$fixture/bin/sh")" = busybox
test "$(cat "$fixture/etc/winlux/settings.conf")" = 'my setting'
test "$(cat "$fixture/home/user/document.txt")" = 'my document'
test "$(cat "$fixture/usr/share/winlux/defaults/etc/winlux/settings.conf")" = 'new default'
update --auto-run | grep -q 'à jour'
update --auto disable
test "$(update --auto status)" = disabled

# An otherwise identical system bundle with only a desktop change restarts the
# session; a changed kernel requires a reboot. Directory links remain links.
printf 'desktop-newer\n' >"$stage/opt/winlux/tests/gui_demo.exe"
ln -s /usr/share/winlux "$stage/usr/share/directory-link"
printf 'updater-desktop\n' >"$stage/usr/share/winlux/version"
if test -f "$root/scripts/source-fingerprint.sh"; then sh "$root/scripts/source-fingerprint.sh" >"$stage/usr/share/winlux/source-fingerprint"; fi
WINLUX_UPDATE_STAGE="$stage" WINLUX_UPDATE_BOOT="$boot" "$root/scripts/build-update-bundle.sh" --system updater-desktop >/dev/null
cp "$root/build/winlux-update-updater-desktop.tar.gz" "$assets/"
cp "$root/build/winlux-update-manifest.txt" "$assets/manifest.txt"
rm "$fixture/run/winlux/update-action"
PATH="$test_root/busybox-tools" WINLUX_UPDATE_ROOT="$fixture" WINLUX_UPDATE_MANIFEST="file://$assets/manifest.txt" WINLUX_UPDATE_ASSET_DIR="$assets" /bin/busybox sh "$updater" --apply
test "$(cat "$fixture/run/winlux/update-action")" = restart-session
test -L "$fixture/usr/share/directory-link"
# Replacing an existing directory link must replace the link itself, never move
# a temporary file into its (absolute, and on this test host external) target.
printf 'desktop-newest\n' >"$stage/opt/winlux/tests/gui_demo.exe"
printf 'updater-desktop-2\n' >"$stage/usr/share/winlux/version"
if test -f "$root/scripts/source-fingerprint.sh"; then sh "$root/scripts/source-fingerprint.sh" >"$stage/usr/share/winlux/source-fingerprint"; fi
WINLUX_UPDATE_STAGE="$stage" WINLUX_UPDATE_BOOT="$boot" "$root/scripts/build-update-bundle.sh" --system updater-desktop-2 >/dev/null
cp "$root/build/winlux-update-updater-desktop-2.tar.gz" "$assets/"
cp "$root/build/winlux-update-manifest.txt" "$assets/manifest.txt"
update --apply
test "$(cat "$fixture/run/winlux/update-action")" = restart-session
test -L "$fixture/usr/share/directory-link"
update --rollback
update --rollback
# Restore the baseline system version for the runtime-only preservation check.
cp "$assets/winlux-update-updater-system.tar.gz" "$assets/baseline-system.tar.gz"
{ tar -xOzf "$assets/baseline-system.tar.gz" META; printf 'asset=baseline-system.tar.gz\nsha256=%s\n' "$(sha256sum "$assets/baseline-system.tar.gz" | cut -d' ' -f1)"; } >"$assets/manifest.txt"
update --apply

# Installing a runtime-only release must retain the existing system components.
build_runtime updater-runtime-2
update --apply
test "$(cat "$fixture/boot/vmlinuz")" = kernel-new
test "$(cat "$fixture/opt/winlux/tests/gui_demo.exe")" = desktop-new
update --rollback
test "$(cat "$fixture/usr/share/winlux/update-scope")" = system
test "$(cat "$fixture/usr/share/winlux/version")" = updater-system
# Restore the preceding complete-system backup explicitly (one rollback per apply).
system_backup=$(find "$fixture/var/lib/winlux" -name metadata -type f | while IFS= read -r metadata; do if grep -q 'action=reboot' "$metadata"; then dirname "$metadata"; fi; done | head -1)
printf '%s\n' "${system_backup##*/}" >"$fixture/var/lib/winlux/rollback-current"
update --rollback
test "$(cat "$fixture/usr/share/winlux/version")" = old
test "$(cat "$fixture/opt/winlux/tests/gui_demo.exe")" = desktop-old
test "$(cat "$fixture/boot/vmlinuz")" = kernel-old
test "$(cat "$fixture/lib/modules/test-kernel/input.ko")" = driver-old
test ! -e "$fixture/lib64/winlux.so"
test ! -L "$fixture/bin/sh"
test "$(cat "$fixture/etc/winlux/settings.conf")" = 'my setting'
test "$(cat "$fixture/home/user/document.txt")" = 'my document'

# Legacy v1 runtime releases remain installable with the same guarded transaction.
mkdir -p "$test_root/legacy/usr/bin" "$test_root/legacy/usr/share/winlux"
cp "$test_root/runtime" "$test_root/legacy/usr/bin/winlux"
cp "$updater" "$test_root/legacy/usr/bin/winlux-update"
printf 'updater-legacy\n' >"$test_root/legacy/usr/share/winlux/version"
tar -C "$test_root/legacy" -czf "$assets/legacy.tar.gz" .
printf 'format=1\nversion=updater-legacy\nasset=legacy.tar.gz\nsha256=%s\n' "$(sha256sum "$assets/legacy.tar.gz" | cut -d' ' -f1)" >"$assets/manifest.txt"
update --apply
test "$(cat "$fixture/usr/share/winlux/version")" = updater-legacy
update --rollback
test "$(cat "$fixture/usr/share/winlux/version")" = old
echo "Mises à jour runtime/système, auto, restauration complète et rejets de paquets dangereux validés."
