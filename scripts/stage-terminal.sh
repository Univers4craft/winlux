#!/bin/sh
# Required terminal plus its dynamic libraries and terminal descriptions.
set -eu
stage=${1:?stage required}
support=${WINLUX_TERMINAL_SUPPORT:-}
terminal=${WINLUX_XTERM:-$(command -v xterm || true)}
test -x "$terminal" || { echo 'xterm requis : installez xterm ou indiquez WINLUX_XTERM.' >&2; exit 1; }
libraries=$(LD_LIBRARY_PATH="${support:+$support/usr/lib/x86_64-linux-gnu:$support/lib/x86_64-linux-gnu:}${LD_LIBRARY_PATH:-}" ldd "$terminal")
case "$libraries" in *'not found'*) echo "Dépendance terminal absente : $libraries" >&2; exit 1 ;; esac
mkdir -p "$stage/usr/bin" "$stage/usr/share/terminfo/x" "$stage/usr/share/applications"
cp --remove-destination "$terminal" "$stage/usr/bin/xterm"
chmod 755 "$stage/usr/bin/xterm"
printf '%s\n' "$libraries" | awk '$2 == "=>" && $3 ~ /^\// {print $3} $1 ~ /^\// {print $1}' | sort -u |
while IFS= read -r library; do
    relative=$library
    if test -n "$support"; then case "$library" in "$support"/*) relative=${library#"$support"} ;; esac; fi
    mkdir -p "$stage$(dirname "$relative")"
    cp -L "$library" "$stage$relative"
done
for name in xterm xterm-256color; do
    source=
    for directory in "${WINLUX_TERMINFO:-/usr/share/terminfo}" "$support/usr/share/terminfo" "$support/lib/terminfo" /lib/terminfo; do
        if test -f "$directory/x/$name"; then source="$directory/x/$name"; break; fi
    done
    test -n "$source" || { echo "Terminfo manquant : $name (paquet ncurses-base)." >&2; exit 1; }
    cp -L "$source" "$stage/usr/share/terminfo/x/$name"
done
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cp "$root/integration/winlux-terminal" "$stage/usr/bin/winlux-terminal"
chmod 755 "$stage/usr/bin/winlux-terminal"
ln -sf winlux-terminal "$stage/usr/bin/x-terminal-emulator"
cp "$root/integration/winlux-terminal.desktop" "$stage/usr/share/applications/"
