#!/bin/sh
set -eu

root=${1:-/usr/local}
bin="$root/bin"
share="$root/share"
install -Dm755 target/release/winlux "$bin/winlux"
install -Dm644 integration/winlux.desktop "$share/applications/winlux.desktop"
install -Dm644 integration/x-ms-dos-executable.xml "$share/mime/packages/winlux.xml"
install -Dm644 integration/winlux-binfmt.conf /etc/binfmt.d/winlux.conf
update-mime-database "$share/mime" 2>/dev/null || true
systemctl restart systemd-binfmt.service 2>/dev/null || true
update-desktop-database "$share/applications" 2>/dev/null || true
echo "Winlux installé dans $bin/winlux"
