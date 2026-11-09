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
# Session native (#1101) : thème, accent, taille de barre, fond personnel.
mkdir -p "$HOME/Images"
printf '\211PNG\r\n\032\nfond' > "$HOME/Images/mon fond.png"
printf '{"theme": "light", "accent": "#12ab34", "panel-size": "48", "wallpaper": "%s"}\n' "$HOME/Images/mon fond.png" > "$HOME/.config/winlux/appearance.json"
# Épingles du menu Démarrer (#1458) : seule la clé « pins » voyage.
printf '{"pins": ["firefox.desktop", "org.gnome.Calculator.desktop"], "recent": ["/home/alice/secret.txt"]}\n' > "$HOME/.config/winlux/start.json"
transfer status
grep -q '^target=removable|Z:\\media\\alice\\CLE$' "$a/run/winlux/transfer.txt" || fail "clé USB non proposée"
transfer export 1
profile=$(ls "$a/media/alice/CLE/"*.wlxprofile)
members=$(tar -tzf "$profile")
printf '%s\n' "$members" | grep -qx 'user/.config/winlux/netdrives.conf' || fail "lecteurs réseau absents"
! printf '%s\n' "$members" | grep -q 'secrets\|key' || fail "mot de passe exporté"
! tar -xzOf "$profile" system/preferences.conf | grep -q WELCOME_DONE || fail "clé hors liste exportée"
printf '%s\n' "$members" | grep -qx 'user/fond.png' || fail "fond personnel non exporté (#1101)"
tar -xzOf "$profile" user/epingles.json | grep -q '"firefox.desktop", "org.gnome.Calculator.desktop"' || fail "épingles non exportées (#1458)"
! tar -xzOf "$profile" user/epingles.json | grep -q recent || fail "start.json exporté en entier (#1458)"
# Membres au nom du compte, jamais root (#1459).
owners=$(tar -tvzf "$profile" --numeric-owner | awk '{print $2}' | sort -u)
test "$owners" = "$(stat -c %u "$HOME")/$(stat -c %g "$HOME")" || fail "membres de l'archive au nom de $owners, pas du compte (#1459)"
look=$(tar -xzOf "$profile" user/apparence.conf)
printf '%s\n' "$look" | grep -qx 'THEME=light' || fail "thème de la session native non exporté (#1101)"
printf '%s\n' "$look" | grep -qx 'ACCENT=#12ab34' || fail "accent non exporté (#1101)"
printf '%s\n' "$look" | grep -qx 'PANEL_SIZE=48' || fail "taille de la barre non exportée"
! printf '%s\n' "$look" | grep -q '^WALLPAPER=' || fail "chemin d'un fond personnel exporté"

# Fond livré avec Winlux : son chemin suffit, l'image ne voyage pas.
printf '{"theme": "dark", "wallpaper": "/usr/share/winlux/wallpapers/winlux-prisme-nuit.png"}\n' > "$HOME/.config/winlux/appearance.json"
mkdir -p "$work/livre"; mv "$a/media/alice/CLE/"*.wlxprofile "$work/livre/"
transfer export 1
shipped=$(ls "$a/media/alice/CLE/"*.wlxprofile)
tar -xzOf "$shipped" user/apparence.conf | grep -qx 'WALLPAPER=/usr/share/winlux/wallpapers/winlux-prisme-nuit.png' || fail "fond livré non exporté"
! tar -tzf "$shipped" | grep -q '^user/fond' || fail "image d'un fond livré exportée"
rm -f "$shipped"; mv "$work/livre/"*.wlxprofile "$a/media/alice/CLE/"

# État dans le dossier d'exécution du compte (#1102).
mkdir -p "$work/run-alice"
WINLUX_TRANSFER_RUN="$work/run-alice" transfer status
test -f "$work/run-alice/transfer.txt" || fail "état hors du dossier du compte (#1102)"

# Machine B
b="$work/b"; export WINLUX_CONTROL_ROOT="$b" HOME="$b/home/bob"
# winlux-personalize factice : note ce que l'import lui demande.
mkdir -p "$work/bin"
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s"\n' "$work/personalize.log" > "$work/bin/personalize"
chmod 755 "$work/bin/personalize"
export WINLUX_PERSONALIZE="$work/bin/personalize"
mkdir -p "$b/etc/winlux" "$HOME/Téléchargements"
printf 'THEME=dark\nWELCOME_DONE=0\n' > "$b/etc/winlux/preferences.conf"
mkdir -p "$HOME/.config/winlux"
printf '{"pins": ["winlux-explorer.desktop"], "recent": ["x"]}\n' > "$HOME/.config/winlux/start.json"
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
grep -q '"pins": \["firefox.desktop", "org.gnome.Calculator.desktop"\]' "$HOME/.config/winlux/start.json" || fail "épingles non importées (#1458)"
grep -q '"recent": \["x"\]' "$HOME/.config/winlux/start.json" || fail "autres clés de start.json perdues (#1458)"
grep -qx 'theme light' "$work/personalize.log" || fail "thème non appliqué (#1101)"
grep -qx 'accent #12ab34' "$work/personalize.log" || fail "accent non appliqué (#1101)"
grep -qx 'panel-size 48' "$work/personalize.log" || fail "taille de barre non appliquée"
grep -qx "wallpaper $HOME/.local/share/winlux/fonds/fond-importe.png" "$work/personalize.log" || fail "fond non appliqué (#1101)"
cmp -s "$HOME/.local/share/winlux/fonds/fond-importe.png" "$a/home/alice/Images/mon fond.png" || fail "fond importé altéré"

: > "$work/personalize.log"
transfer undo
grep -qx 'theme dark' "$work/personalize.log" || fail "annulation : thème d'avant non remis (#1101)"
grep -qx 'accent #6a3df0' "$work/personalize.log" || fail "annulation : accent d'avant non remis (#1101)"
grep -qx 'THEME=dark' "$b/etc/winlux/preferences.conf" || fail "annulation du thème"
grep -q '"pins": \["winlux-explorer.desktop"\]' "$HOME/.config/winlux/start.json" || fail "annulation : épingles d'avant non remises (#1458)"
! grep -q 'XKB_LAYOUT' "$b/etc/winlux/input.conf" 2>/dev/null || fail "annulation du clavier"
! test -e "$HOME/.local/share/winlux/personnalisation.conf" || fail "annulation de la personnalisation"

# Archive piégée : chemin hors du dossier, lien symbolique, fichier inconnu.
evil="$work/evil"; mkdir -p "$evil/user/.config"
printf 'format=1\n' > "$evil/winlux-profile.txt"
ln -s /etc/shadow "$evil/user/.config/mimeapps.list"
printf 'x\n' > "$evil/bashrc"
printf '{"pins": ["../../.bashrc", "a.desktop"]}' > "$evil/user/epingles.json"
pins_before=$(cat "$HOME/.config/winlux/start.json")
(cd "$evil" && tar -czf "$HOME/Téléchargements/piege.wlxprofile" winlux-profile.txt user bashrc ../evil/bashrc 2>/dev/null) || true
transfer import-file "$HOME/Téléchargements/piege.wlxprofile"
! test -L "$HOME/.config/mimeapps.list" || fail "lien symbolique importé"
! test -e "$HOME/bashrc" || fail "fichier hors liste importé"
test "$(cat "$HOME/.config/winlux/start.json")" = "$pins_before" || fail "épingles invalides importées (#1458)"
printf 'pas une archive' > "$HOME/Téléchargements/faux.wlxprofile"
if transfer import-file "$HOME/Téléchargements/faux.wlxprofile" 2>/dev/null; then fail "fichier invalide accepté"; fi
# Sous le compte (#1100) : action locale de winlux-control, état dans son dossier.
grep -q '^        transfer-\*) return 0 ;;$' "$project/scripts/winlux-control.sh" || fail "transfer-* relayé en root (#1100)"
grep -q 'WINLUX_TRANSFER_RUN="$mine" winlux-transfer' "$project/scripts/winlux-control.sh" || fail "état du transfert hors du dossier du compte (#1102)"
grep -q 'run_file("transfer.txt")' "$project/scripts/winlux-settings-pages.py" || fail "Paramètres ne lit pas l'état du compte (#1102)"
# En root pour un compte (#1459) : rien de ce qui est écrit chez lui n'est à root.
if test "$(id -u)" = 0; then
    c="$work/c"; export WINLUX_CONTROL_ROOT="$c" HOME="$c/home/carole"
    mkdir -p "$c/etc/winlux" "$HOME/Documents"
    chown -R 4321:4321 "$HOME"
    cp "$profile" "$HOME/Documents/"
    transfer import-file "$HOME/Documents/${profile##*/}"
    transfer export 0
    root_owned=$(find "$HOME" -uid 0 -print)
    test -z "$root_owned" || fail "fichiers à root dans le dossier du compte : $root_owned (#1459)"
    test "$(stat -c %u "$HOME/.config/winlux/start.json")" = 4321 || fail "start.json à root (#1459)"
    test "$(stat -c %u "$HOME/.local/share/winlux/reglages-avant-import.wlxprofile")" = 4321 || fail "sauvegarde à root (#1459)"
    exported=$(ls "$HOME/Documents/"Reglages*"$(date '+%Y-%m-%d')".wlxprofile | head -n 1)
    test "$(tar -tvzf "$exported" --numeric-owner | awk '{print $2}' | sort -u)" = 4321/4321 || fail "export en root : membres pas au nom du compte (#1459)"
fi
echo "Transfert des réglages : OK"
