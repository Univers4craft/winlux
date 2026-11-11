#!/bin/sh
# Prepare the shared Linux support libraries of downloaded browsers (no browser payload).
set -eu
stage=${1:?stage required}
cache=${2:?browser cache required}
support=${WINLUX_BROWSER_SUPPORT:-}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
target="$stage/usr/share/winlux/browsers"
mkdir -p "$target" "$stage/usr/bin"
copy_library() {
    lib=$1
    case "$lib" in "$work"/*) return ;; esac
    relative=$lib
    if test -n "$support"; then case "$lib" in "$support"/*) relative=${lib#"$support"} ;; esac; fi
    mkdir -p "$stage$(dirname "$relative")"
    cp -L "$lib" "$stage$relative"
}
libraries() {
    result=$(LD_LIBRARY_PATH="${vendor:-}:${support:+$support/usr/lib/x86_64-linux-gnu:$support/lib/x86_64-linux-gnu:}${LD_LIBRARY_PATH:-}" ldd "$1" 2>&1 || true)
    if printf '%s\n' "$result" | grep -q 'not found'; then echo "$1 : $result" >&2; exit 1; fi
    printf '%s\n' "$result" | awk '$2 == "=>" && $3 ~ /^\// {print $3} $1 ~ /^\// {print $1}' | sort -u > "$work/libraries"
    while IFS= read -r lib; do copy_library "$lib"; done < "$work/libraries"
}
# gpgv et xz : archive signée de Tor Browser (#1716).
for tool in dpkg-deb setpriv xhost tar gpgv xz; do
    binary=$(command -v "$tool")
    cp --remove-destination "$binary" "$stage/usr/bin/$tool"
    libraries "$binary"
done
# Aucun navigateur n'est copié dans l'image : l'utilisateur choisit le sien,
# téléchargé chez son éditeur (winlux-browsers --install). La licence d'Edge
# en interdit d'ailleurs la redistribution (section 2.8 f).
for id in brave chrome edge firefox opera operagx tor vivaldi; do
    if test -e "$cache/$id.deb" || test -e "$cache/$id.tar.xz"; then
        echo "$id.deb refusé : aucun navigateur dans l'image, téléchargement à la demande uniquement" >&2
        exit 1
    fi
done
# Chromium loads GTK/NSS dynamically; they do not all appear in ldd(main).
vendor=
for name in libgtk-3.so.0 libgdk-3.so.0 libnss3.so libnssutil3.so libsmime3.so libnspr4.so libasound.so.2 libgbm.so.1 libxkbcommon.so.0; do
    lib=$(ldconfig -p | awk -v name="$name" '$1 == name && /x86-64/ {print $NF;exit}')
    test -n "$lib" || { echo "Bibliothèque navigateur absente : $name" >&2; exit 1; }
    copy_library "$lib"; libraries "$lib"
done
if test -f /usr/share/glib-2.0/schemas/gschemas.compiled; then
    mkdir -p "$stage/usr/share/glib-2.0/schemas"
    cp /usr/share/glib-2.0/schemas/gschemas.compiled "$stage/usr/share/glib-2.0/schemas/"
fi
