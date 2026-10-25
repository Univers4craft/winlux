#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
scope=system
case "${1:-}" in --system) shift ;; --runtime) scope=runtime; shift ;; esac
version=${1:-$(cat "$root/VERSION")}
case "$version" in *[!A-Za-z0-9._+-]*|'') echo "version invalide : $version" >&2; exit 1 ;; esac
test "$#" -le 1 || { echo 'usage: build-update-bundle.sh [--system|--runtime] [version]' >&2; exit 2; }
for tool in tar sha256sum find sort readlink; do command -v "$tool" >/dev/null 2>&1 || { echo "$tool est requis" >&2; exit 1; }; done
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/update-$version.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
raw="$work/raw"
payload="$work/package/payload"
mkdir -p "$raw" "$payload/usr/bin" "$payload/usr/share/winlux"
runtime=${WINLUX_UPDATE_RUNTIME:-"$root/target/release/winlux"}
if test "$scope" = system; then
    stage=${WINLUX_UPDATE_STAGE:-"$root/build/winlux-live-test/initramfs-root"}
    boot=${WINLUX_UPDATE_BOOT:-"$root/build/winlux-live-test/iso-root/boot"}
    for required in init usr/bin/winlux opt/winlux/tests/gui_demo.exe; do test -f "$stage/$required" || { echo "stage système incomplet : $required ; reconstruisez l'ISO" >&2; exit 1; }; done
    test "$(cat "$stage/usr/share/winlux/version")" = "$version" || { echo 'version du stage différente du paquet demandé ; reconstruisez l’ISO avec cette version' >&2; exit 1; }
    test -d "$stage/lib/modules" || { echo 'modules noyau absents du stage' >&2; exit 1; }
    # Shared libraries are replaced by a complete OS update; the installer must
    # retain a utility binary that does not depend on those live libraries.
    busybox_linkage=$(LC_ALL=C ldd "$stage/bin/busybox" 2>&1 || true)
    case "$busybox_linkage" in *'not a dynamic executable'*|*'statically linked'*) ;;
        *) echo 'le paquet système exige BusyBox statique (busybox-static)' >&2; exit 1 ;;
    esac
    if test -f "$root/scripts/source-fingerprint.sh"; then
        expected=$(sh "$root/scripts/source-fingerprint.sh")
        test -f "$stage/usr/share/winlux/source-fingerprint" && test "$(cat "$stage/usr/share/winlux/source-fingerprint")" = "$expected" || { echo "stage système périmé ; reconstruisez l'ISO avant le paquet système" >&2; exit 1; }
    fi
    for path in bin usr lib lib64 opt/winlux init; do
        if test -e "$stage/$path"; then mkdir -p "$raw/$(dirname -- "$path")"; cp -a "$stage/$path" "$raw/$path"; fi
    done
    mkdir -p "$raw/boot"
    for name in vmlinuz winlux-initramfs.cpio.gz; do test -s "$boot/$name" || { echo "fichier de démarrage absent : $boot/$name" >&2; exit 1; }; cp "$boot/$name" "$raw/boot/$name"; done
    # New configuration defaults are visible without overwriting the user's /etc.
    if test -d "$stage/etc"; then mkdir -p "$raw/usr/share/winlux/defaults/etc"; cp -a "$stage/etc/." "$raw/usr/share/winlux/defaults/etc/"; fi
    runtime="$stage/usr/bin/winlux"
    action=reboot
    components=runtime,desktop,applications,updater,integration,kernel,drivers,libraries,boot
else
    test -x "$runtime" || cargo build --release -p winlux --manifest-path "$root/Cargo.toml"
    action=restart-session
    components=runtime,updater
fi
mkdir -p "$raw/usr/bin" "$raw/usr/share/winlux"
cp "$runtime" "$raw/usr/bin/winlux"
cp "$root/scripts/winlux-update.sh" "$raw/usr/bin/winlux-update"
chmod 755 "$raw/usr/bin/winlux" "$raw/usr/bin/winlux-update"
printf '%s\n' "$version" >"$raw/usr/share/winlux/version"
printf '%s\n' "$scope" >"$raw/usr/share/winlux/update-scope"
files="$work/package/FILES"
links="$work/package/LINKS"
: >"$files"; : >"$links"
find "$raw" -mindepth 1 -print | LC_ALL=C sort >"$work/paths"
while IFS= read -r source; do
    path=${source#"$raw"/}
    case "$path" in ''|*[!A-Za-z0-9._+@%=/:-]*) echo "nom de fichier non pris en charge : $path" >&2; exit 1 ;; esac
    if test -L "$source"; then
        target=$(readlink "$source")
        case "$target" in ''|*[!A-Za-z0-9._+@%=/:-]*) echo "cible de lien invalide : $path" >&2; exit 1 ;; esac
        printf '%s %s\n' "$path" "$target" >>"$links"
    elif test -d "$source"; then mkdir -p "$payload/$path";
    elif test -f "$source"; then
        mkdir -p "$payload/$(dirname -- "$path")"
        cp "$source" "$payload/$path"
        mode=644; if test -x "$source"; then mode=755; fi
        chmod "$mode" "$payload/$path"
        printf '%s %s %s\n' "$(sha256sum "$payload/$path" | cut -d' ' -f1)" "$mode" "$path" >>"$files"
    else echo "fichier spécial refusé : $path" >&2; exit 1; fi
done <"$work/paths"
printf 'format=2\nversion=%s\nscope=%s\naction=%s\ncomponents=%s\n' "$version" "$scope" "$action" "$components" >"$work/package/META"
bundle="$root/build/winlux-update-$version.tar.gz"
manifest="$root/build/winlux-update-manifest.txt"
tar -C "$work/package" --owner=0 --group=0 --numeric-owner -czf "$work/bundle.tar.gz" META FILES LINKS payload
checksum=$(sha256sum "$work/bundle.tar.gz" | cut -d' ' -f1)
{ cat "$work/package/META"; printf 'asset=%s\nsha256=%s\n' "$(basename -- "$bundle")" "$checksum"; } >"$work/manifest.txt"
mv "$work/bundle.tar.gz" "$bundle"
mv "$work/manifest.txt" "$manifest"
echo "Paquet de mise à jour $scope : $bundle"
echo "Manifeste : $manifest"
