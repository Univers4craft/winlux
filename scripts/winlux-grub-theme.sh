#!/bin/sh
# Installe le thème GRUB Winlux (fond, surbrillance, theme.txt) et la police
# unicode.pf2 de GRUB dans un dossier de thème.
#   winlux-grub-theme.sh install <source> <destination>
# Sans police disponible, le thème est copié quand même : grub.cfg teste
# `loadfont` et reste en mode texte si elle manque.
set -eu
test "${1:-}" = install && test $# = 3 || { echo "usage : $0 install <source> <destination>" >&2; exit 2; }
src=$2
dst=$3
test -f "$src/theme.txt" || { echo "thème GRUB absent : $src" >&2; exit 1; }
mkdir -p "$dst"
cp "$src"/theme.txt "$src"/*.png "$dst/"
if ! test -f "$src/unicode.pf2"; then
    for font in /usr/share/grub/unicode.pf2 /boot/grub/fonts/unicode.pf2 /usr/share/winlux/grub-theme/unicode.pf2; do
        if test -f "$font"; then
            cp "$font" "$dst/unicode.pf2"
            break
        fi
    done
else
    cp "$src/unicode.pf2" "$dst/unicode.pf2"
fi
