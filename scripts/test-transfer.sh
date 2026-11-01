#!/bin/sh
# Transfert des réglages (winlux-transfer) : export vers Documents et une clé
# USB, import sur une autre installation, liste blanche des clés, rejet des
# archives piégées et annulation de l'import.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-transfer-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
transfer() { sh "$project/scripts/winlux-transfer.sh" "$@" >/dev/null; }
fail() { echo "ÉCHEC : $*" >&2; cat "$WINLUX_CONTROL_ROOT/run/winlux/transfer.txt" >&2 2>/dev/null || true; exit 1; }

# Machine A
a="$work/a"; export WINLUX_CONTROL_ROOT="$a" HOME="$a/home/alice"
mkdir -p "$a/etc/winlux" "$HOME/Documents" "$HOME/.local/share/winlux" "$HOME/.config/winlux" "$a/media/alice/CLE"
printf 'THEME=light\nTASKBAR_ALIGN=center\nWELCOME_DONE=1\n' > "$a/etc/winlux/preferences.conf"
printf 'XKB_LAYOUT=de\nMOUSE_SPEED=$(reboot)\n' > "$a/etc/winlux/input.conf"
printf 'LANG=es\n' > "$a/etc/winlux/locale.conf"
printf 'wallpaper=bloom\naccent=3\ntaskbar_position=left\n' > "$HOME/.local/share/winlux/personnalisation.conf"
printf 'pins\001\002' > "$HOME/.local/share/winlux/taskbar.dat"
printf '[Default Applications]\ntext/plain=geany.desktop;\n' > "$HOME/.config/mimeapps.list"
printf 'drive=N|sftp|serveur|22|alice|/|NAS|1|1\n' > "$HOME/.config/winlux/netdrives.conf"
printf 'N:secret-scelle\n' > "$HOME/.config/winlux/netdrives.secrets"
transfer status
grep -q '^target=removable|Z:\\media\\alice\\CLE$' "$a/run/winlux/transfer.txt" || fail "clé USB non proposée"
transfer export 1
profile=$(ls "$a/media/alice/CLE/"*.wlxprofile)
members=$(tar -tzf "$profile")
printf '%s\n' "$members" | grep -qx 'user/.config/winlux/netdrives.conf' || fail "lecteurs réseau absents"
! printf '%s\n' "$members" | grep -q 'secrets\|key' || fail "mot de passe exporté"
! tar -xzOf "$profile" system/preferences.conf | grep -q WELCOME_DONE || fail "clé hors liste exportée"

# Machine B
b="$work/b"; export WINLUX_CONTROL_ROOT="$b" HOME="$b/home/bob"
mkdir -p "$b/etc/winlux" "$HOME/Téléchargements"
printf 'THEME=dark\nWELCOME_DONE=0\n' > "$b/etc/winlux/preferences.conf"
cp "$profile" "$HOME/Téléchargements/"
transfer status
grep -q '^found=C:\\Téléchargements\\Reglages Winlux .*\.wlxprofile|' "$b/run/winlux/transfer.txt" || fail "fichier de réglages non trouvé"
transfer import 0
grep -qx 'THEME=light' "$b/etc/winlux/preferences.conf" || fail "thème non importé"
grep -qx 'WELCOME_DONE=0' "$b/etc/winlux/preferences.conf" || fail "clé locale écrasée"
grep -qx 'XKB_LAYOUT=de' "$b/etc/winlux/input.conf" || fail "clavier non importé"
! grep -q 'MOUSE_SPEED' "$b/etc/winlux/input.conf" || fail "valeur dangereuse importée"
grep -qx 'taskbar_position=left' "$HOME/.local/share/winlux/personnalisation.conf" || fail "barre des tâches non importée"
test -f "$HOME/.config/winlux/netdrives.conf" || fail "lecteurs réseau non importés"
grep -qx 'backup=1' "$b/run/winlux/transfer.txt" || fail "pas de sauvegarde avant import"

transfer undo
grep -qx 'THEME=dark' "$b/etc/winlux/preferences.conf" || fail "annulation du thème"
! grep -q 'XKB_LAYOUT' "$b/etc/winlux/input.conf" 2>/dev/null || fail "annulation du clavier"
! test -e "$HOME/.local/share/winlux/personnalisation.conf" || fail "annulation de la personnalisation"

# Archive piégée : chemin hors du dossier, lien symbolique, fichier inconnu.
evil="$work/evil"; mkdir -p "$evil/user/.config"
printf 'format=1\n' > "$evil/winlux-profile.txt"
ln -s /etc/shadow "$evil/user/.config/mimeapps.list"
printf 'x\n' > "$evil/bashrc"
(cd "$evil" && tar -czf "$HOME/Téléchargements/piege.wlxprofile" winlux-profile.txt user bashrc ../evil/bashrc 2>/dev/null) || true
transfer import-file "$HOME/Téléchargements/piege.wlxprofile"
! test -L "$HOME/.config/mimeapps.list" || fail "lien symbolique importé"
! test -e "$HOME/bashrc" || fail "fichier hors liste importé"
printf 'pas une archive' > "$HOME/Téléchargements/faux.wlxprofile"
if transfer import-file "$HOME/Téléchargements/faux.wlxprofile" 2>/dev/null; then fail "fichier invalide accepté"; fi
echo "Transfert des réglages : OK"
