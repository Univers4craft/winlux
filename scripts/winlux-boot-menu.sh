#!/bin/sh
# Raccourcit le menu GRUB d'une installation Winlux existante (#188 pour le
# disque installé) : 8 s d'attente à chaque démarrage, et un noyau qui écrit
# tous ses messages à l'écran. Les nouvelles installations sont déjà écrites
# ainsi par l'installateur ; ce script met à niveau les anciennes, une fois,
# depuis le postinst du paquet winlux-desktop.
#   - menu caché 1 s, Échap ou Maj l'affiche (visible 5 s si une entrée
#     Windows est proposée) ;
#   - entrée « Winlux Desktop » : quiet loglevel=3 vt.global_cursor_default=0,
#     comme la session live. Les entrées diagnostic et secours restent bavardes.
# Rien n'est modifié si le fichier n'est pas celui écrit par Winlux, et
# l'ancien fichier revient si grub-script-check refuse le nouveau.
set -eu
cfg=${WINLUX_GRUB_CFG:-/boot/grub/grub.cfg}
test -f "$cfg" && ! test -L "$cfg" || exit 0
grep -q '^menuentry "Winlux Desktop' "$cfg" || exit 0
grep -q '^set timeout=8$' "$cfg" || grep -q 'init=/init .*winlux.interactive=1' "$cfg" || exit 0
timeout=1 style=hidden
if grep -q '^menuentry "Windows"' "$cfg"; then timeout=5 style=menu; fi
new="$cfg.winlux-new.$$"
trap 'rm -f "$new"' EXIT HUP INT TERM
awk -v timeout="$timeout" -v style="$style" '
    $0 == "set timeout=8" { print "set timeout=" timeout; print "set timeout_style=" style; next }
    /^menuentry "Winlux Desktop/ { desktop = 1 }
    /^}/ { desktop = 0 }
    desktop && $1 == "linux" && /winlux\.interactive=1/ && !/ quiet( |$)/ {
        sub(/winlux\.interactive=1/, "quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1")
    }
    { print }
' "$cfg" >"$new"
if cmp -s "$cfg" "$new"; then exit 0; fi
if command -v grub-script-check >/dev/null 2>&1 && ! grub-script-check "$new" >/dev/null 2>&1; then
    echo "winlux-boot-menu : nouveau grub.cfg refusé, ancien conservé" >&2
    exit 0
fi
chmod --reference="$cfg" "$new" 2>/dev/null || true
mv -f "$new" "$cfg"
sync
echo "winlux-boot-menu : menu GRUB ${style} ${timeout} s, démarrage silencieux"
