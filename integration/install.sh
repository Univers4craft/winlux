#!/bin/sh
set -eu

prefix=${1:-/usr}
bin="$prefix/bin"
share="$prefix/share"
version=${WINLUX_VERSION:-$(git describe --tags --always 2>/dev/null || printf 'development')}
install -Dm755 target/release/winlux "$bin/winlux"
install -Dm755 scripts/winlux-update.sh "$bin/winlux-update"
printf '%s\n' "$version" | install -Dm644 /dev/stdin "$share/winlux/version"
install -Dm644 integration/winlux.desktop "$share/applications/winlux.desktop"
install -Dm644 integration/x-ms-dos-executable.xml "$share/mime/packages/winlux.xml"
install -Dm644 integration/winlux-binfmt.conf /etc/binfmt.d/winlux.conf
update-mime-database "$share/mime" 2>/dev/null || true
systemctl restart systemd-binfmt.service 2>/dev/null || true
update-desktop-database "$share/applications" 2>/dev/null || true
echo "Winlux $version installé dans $bin/winlux"
