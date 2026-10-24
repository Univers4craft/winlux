#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=${1:-$(git -C "$root" describe --tags --always)}
case "$version" in
    *[!A-Za-z0-9._-]*|'') echo "version invalide : $version" >&2; exit 1 ;;
esac

command -v tar >/dev/null 2>&1 || { echo "tar est requis" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "sha256sum est requis" >&2; exit 1; }

runtime="$root/target/release/winlux"
test -x "$runtime" || cargo build --release -p winlux --manifest-path "$root/Cargo.toml"

work="$root/build/update-$version"
payload="$work/payload"
bundle="$root/build/winlux-update-$version.tar.gz"
manifest="$root/build/winlux-update-manifest.txt"
rm -rf "$work"
mkdir -p "$payload/usr/bin" "$payload/usr/share/winlux"
cp "$runtime" "$payload/usr/bin/winlux"
cp "$root/scripts/winlux-update.sh" "$payload/usr/bin/winlux-update"
chmod 755 "$payload/usr/bin/winlux" "$payload/usr/bin/winlux-update"
printf '%s\n' "$version" >"$payload/usr/share/winlux/version"

tar -C "$payload" --owner=0 --group=0 --numeric-owner -czf "$bundle" .
checksum=$(sha256sum "$bundle" | cut -d' ' -f1)
{
    printf 'format=1\n'
    printf 'version=%s\n' "$version"
    printf 'asset=%s\n' "$(basename -- "$bundle")"
    printf 'sha256=%s\n' "$checksum"
} >"$manifest"

echo "Paquet de mise à jour : $bundle"
echo "Manifeste : $manifest"
