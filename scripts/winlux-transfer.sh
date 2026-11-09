#!/bin/sh
# winlux-transfer : emporter ses réglages d'une installation de Winlux à une
# autre, par simple fichier (clé USB, partage réseau…), sans aucun compte en
# ligne.
#
# Le fichier « .wlxprofile » est une archive tar.gz qui contient :
#   - le thème clair ou sombre, la couleur d'accent, la taille de la barre et
#     le fond d'écran de la session native (user/apparence.conf ; une image
#     personnelle de 8 Mio au plus voyage dans user/fond.<ext>, un fond livré
#     avec Winlux par son seul chemin) (#1101) ;
#   - pour l'ancien bureau PE : personnalisation, épingles, icônes du bureau,
#     accessibilité ;
#   - les lecteurs réseau, sans aucun mot de passe (ils restent scellés sur
#     la machine d'origine : Winlux les redemande à la première connexion) ;
#   - les applications épinglées au menu Démarrer (clé « pins » de
#     ~/.config/winlux/start.json, dans user/epingles.json) (#1458) ;
#   - les applications par défaut (mimeapps.list) ;
#   - la langue, la disposition du clavier, la souris, le thème et
#     l'alignement de la barre, les choix de confidentialité.
# L'archive et tout ce que l'import écrit dans le dossier personnel
# appartiennent au compte, même quand le script tourne en root (#1459).
# Rien d'autre n'est lu à l'import : liste blanche de fichiers, de clés et de
# valeurs. Avant chaque import, les réglages actuels sont sauvegardés et
# « undo » les remet.
#
# Tourne sous le compte (action locale de winlux-control, #1100) : les
# réglages et le dossier personnel sont ceux du compte, l'état va dans son
# dossier d'exécution (WINLUX_TRANSFER_RUN) et les clés de /etc/winlux
# passent par le relais winlux-root (liste blanche de son côté aussi).
#
# Usage (via winlux-control transfer-<action> ...) :
#   winlux-transfer status          écrit /run/winlux/transfer.txt
#   winlux-transfer export [N]      enregistre dans la destination N (0 = Documents)
#   winlux-transfer import N        importe le fichier N de la liste « found »
#   winlux-transfer import-file F   importe un fichier précis
#   winlux-transfer undo            remet les réglages d'avant le dernier import
set -eu
umask 077
root=${WINLUX_CONTROL_ROOT:-}
etc="$root/etc/winlux"
run=${WINLUX_TRANSFER_RUN:-$root/run/winlux}
out="$run/transfer.txt"
home=${HOME:-/root}
share="$home/.local/share/winlux"
backup="$share/reglages-avant-import.wlxprofile"
mkdir -p "$run"
MAX_BYTES=262144
WALL_MAX=8388608
PROFILE_MAX=12582912
PERSONALIZE=${WINLUX_PERSONALIZE:-winlux-personalize}
# Bureau sous le compte : /etc/winlux reste à root, le relais écrit les clés.
relayed() { test -z "$root" && test "$(id -u)" != 0 && test -S "${WINLUX_ROOT_SOCKET:-/run/winlux-root.sock}"; }

# Propriétaire du dossier personnel : l'archive porte son nom, et ce que le
# script crée chez lui lui est rendu s'il tourne en root (#1459).
home_uid=$(stat -c %u "$home" 2>/dev/null || id -u)
home_gid=$(stat -c %g "$home" 2>/dev/null || id -g)
own() {
    test "$(id -u)" = 0 && test "$home_uid" != 0 || return 0
    for path in "$@"; do
        # Le fichier, puis chaque dossier parent créé sous le dossier personnel.
        while case "$path" in "$home"/*) true ;; *) false ;; esac; do
            if test -e "$path" || test -L "$path"; then
                chown -h "$home_uid:$home_gid" "$path" 2>/dev/null || true
            fi
            path=$(dirname "$path")
        done
    done
}
fail() { printf 'winlux-transfer : %s\n' "$*" >&2; note "error" "$*"; status; exit 1; }
note() { printf '%s|%s\n' "$1" "$2" > "$run/transfer-last.txt"; chmod 644 "$run/transfer-last.txt" 2>/dev/null || true; }

# Fichiers personnels emportés (chemins relatifs au dossier personnel).
USER_FILES=".local/share/winlux/personnalisation.conf
.local/share/winlux/taskbar.dat
.local/share/winlux/desktop-icons.dat
.local/share/winlux/accessibility.ini
.config/winlux/netdrives.conf
.config/mimeapps.list"
# Réglages système : fichier et clés autorisées.
SYSTEM_KEYS="preferences.conf THEME TASKBAR_ALIGN
input.conf XKB_LAYOUT TAP_TO_CLICK MOUSE_SPEED
locale.conf LANG
privacy.conf TELEMETRY MICROPHONE CAMERA LOCATION LOCATION_HISTORY"

# ---------------------------------------------------------------- dossiers

xdg_dir() {
    # Dossier XDG (DOCUMENTS, DESKTOP, DOWNLOAD) ou son nom par défaut.
    d=$(sed -n "s/^XDG_$1_DIR=\"\(.*\)\"/\1/p" "$home/.config/user-dirs.dirs" 2>/dev/null | head -n 1)
    d=$(printf '%s' "$d" | sed "s|^\$HOME|$home|")
    if test -n "$d" && test -d "$d"; then printf '%s\n' "$d"; return; fi
    for name in $2; do
        if test -d "$home/$name"; then printf '%s\n' "$home/$name"; return; fi
    done
    test "$1" != DOCUMENTS || printf '%s\n' "$home"
}

# Destinations d'export : Documents, puis chaque clé USB ou disque amovible.
targets() {
    printf 'documents|%s\n' "$(xdg_dir DOCUMENTS Documents)"
    for d in "$root"/run/media/*/* "$root"/media/*/* "$root"/media/*; do
        test -d "$d" && test -w "$d" || continue
        case "$d" in "$root"/media/*/*) ;; "$root"/media/*) test -z "$(ls -A "$d" 2>/dev/null | head -n 1)" && continue ;; esac
        printf 'removable|%s\n' "$d"
    done | awk -F '|' '!seen[$2]++'
}

# Fichiers de réglages trouvés aux endroits habituels (premier niveau seulement).
found() {
    {
        for d in "$(xdg_dir DOCUMENTS Documents)" "$(xdg_dir DESKTOP 'Bureau Desktop')" \
                 "$(xdg_dir DOWNLOAD 'Téléchargements Downloads')" "$home"; do
            test -n "$d" && printf '%s\n' "$d"
        done
        targets | cut -d '|' -f 2
    } | awk '!seen[$0]++' | while IFS= read -r d; do
        for f in "$d"/*.wlxprofile; do
            test -f "$f" && ! test -L "$f" && printf '%s\n' "$f"
        done
    done | awk '!seen[$0]++'
}

windows_path() {
    p=${1#"$root"}
    h=${home#"$root"}
    case "$p" in
        "$h") printf 'C:\\\n' ;;
        "$h"/*) printf 'C:\\%s\n' "${p#"$h"/}" | tr '/' '\\' ;;
        *) printf 'Z:\\%s\n' "${p#/}" | tr '/' '\\' ;;
    esac
}

manifest_value() { tar -xzOf "$1" winlux-profile.txt 2>/dev/null | sed -n "s/^$2=//p" | head -n 1 | tr -d '|\r' | cut -c 1-60; }

status() {
    {
        printf 'tar=%s\n' "$(command -v tar >/dev/null 2>&1 && echo 1 || echo 0)"
        targets | while IFS='|' read -r kind path; do
            printf 'target=%s|%s\n' "$kind" "$(windows_path "$path")"
        done
        found | while IFS= read -r f; do
            printf 'found=%s|%s|%s\n' "$(windows_path "$f")" "$(manifest_value "$f" host)" "$(manifest_value "$f" created)"
        done
        test ! -f "$backup" || printf 'backup=1\n'
        if test -r "$run/transfer-last.txt"; then printf 'last=%s\n' "$(head -n 1 "$run/transfer-last.txt")"; fi
    } > "$out.tmp.$$"
    chmod 644 "$out.tmp.$$"
    mv "$out.tmp.$$" "$out"
}

# ---------------------------------------------------------------- apparence

# Thème, accent, taille de la barre et fond de la session native (#1101) :
# ~/.config/winlux/appearance.json, et le fond d'xfdesktop s'il n'y est pas.
appearance_export() {
    dir=$1
    py=$(command -v python3 2>/dev/null || true)
    test -n "$py" || return 0
    "$py" -I - "$home/.config/winlux/appearance.json" > "$dir/apparence.conf" <<'PY' || true
import json, re, sys
try:
    with open(sys.argv[1], encoding="utf-8") as source:
        state = json.load(source)
except (OSError, ValueError):
    state = {}
state = state if isinstance(state, dict) else {}
# Valeurs par défaut de winlux-personalize : l'annulation d'un import les remet.
state = {"theme": "dark", "accent": "#6a3df0", "panel-size": "40", **state}
checks = (("THEME", "theme", r"dark|light"), ("ACCENT", "accent", r"#[0-9A-Fa-f]{6}"),
          ("PANEL_SIZE", "panel-size", r"32|40|48|56|64"), ("WALLPAPER", "wallpaper", r"/[^\x00-\x1f\x7f]{1,1024}"))
for key, name, pattern in checks:
    value = state.get(name)
    if isinstance(value, (str, int)) and re.fullmatch(pattern, str(value)):
        print(f"{key}={value}")
PY
    if ! grep -q '^WALLPAPER=' "$dir/apparence.conf" 2>/dev/null && command -v xfconf-query >/dev/null 2>&1; then
        prop=$(xfconf-query -c xfce4-desktop -l 2>/dev/null | grep '/last-image$' | head -n 1)
        wall=$(test -z "$prop" || xfconf-query -c xfce4-desktop -p "$prop" 2>/dev/null | head -n 1)
        case "$wall" in /*) printf 'WALLPAPER=%s\n' "$wall" | tr -d '\r' >> "$dir/apparence.conf" ;; esac
    fi
    wall=$(sed -n 's/^WALLPAPER=//p' "$dir/apparence.conf" | head -n 1)
    case "$wall" in
        # Fond livré avec Winlux : présent sur l'autre PC, le chemin suffit.
        /usr/share/winlux/*) ;;
        ?*)
            sed -i '/^WALLPAPER=/d' "$dir/apparence.conf"
            ext=$(printf '%s' "${wall##*.}" | tr 'A-Z' 'a-z')
            case "$ext" in png|jpg|jpeg|webp|bmp|gif) ;; *) ext= ;; esac
            if test -n "$ext" && test -f "$wall" && ! test -L "$wall" && test "$(wc -c < "$wall")" -le "$WALL_MAX"; then
                cp "$wall" "$dir/fond.$ext"
            fi ;;
    esac
    test -s "$dir/apparence.conf" || rm -f "$dir/apparence.conf"
}

# Remet l'apparence par winlux-personalize, qui revérifie chaque valeur
# (thème, couleur, taille, image lisible par GdkPixbuf).
appearance_import() {
    dir=$1
    command -v "$PERSONALIZE" >/dev/null 2>&1 || return 0
    f="$dir/apparence.conf"
    if test -f "$f" && ! test -L "$f" && test "$(wc -c < "$f")" -le 4096; then
        theme=$(sed -n 's/^THEME=//p' "$f" | head -n 1)
        case "$theme" in dark|light) "$PERSONALIZE" theme "$theme" >/dev/null 2>&1 || true ;; esac
        accent=$(sed -n 's/^ACCENT=//p' "$f" | head -n 1)
        if printf '%s' "$accent" | grep -qE '^#[0-9A-Fa-f]{6}$'; then "$PERSONALIZE" accent "$accent" >/dev/null 2>&1 || true; fi
        size=$(sed -n 's/^PANEL_SIZE=//p' "$f" | head -n 1)
        case "$size" in 32|40|48|56|64) "$PERSONALIZE" panel-size "$size" >/dev/null 2>&1 || true ;; esac
        wall=$(sed -n 's/^WALLPAPER=//p' "$f" | head -n 1)
    else
        wall=
    fi
    for picture in "$dir"/fond.png "$dir"/fond.jpg "$dir"/fond.jpeg "$dir"/fond.webp "$dir"/fond.bmp "$dir"/fond.gif; do
        test -f "$picture" && ! test -L "$picture" || continue
        test "$(wc -c < "$picture")" -le "$WALL_MAX" || continue
        mkdir -p "$share/fonds"
        wall="$share/fonds/fond-importe.${picture##*.}"
        cp "$picture" "$wall.$$" && mv "$wall.$$" "$wall"
        own "$wall"
        break
    done
    case "$wall" in
        *..*) ;;
        /usr/share/winlux/*|"$share"/fonds/*) test -f "$wall" && "$PERSONALIZE" wallpaper "$wall" >/dev/null 2>&1 || true ;;
    esac
}

# ---------------------------------------------------------------- épingles

# Applications épinglées au menu Démarrer (#1458) : la seule clé « pins » de
# ~/.config/winlux/start.json, des identifiants .desktop, 100 au plus.
PINS_CHECK='
import json, re, sys
def valid(pins):
    return (isinstance(pins, list) and len(pins) <= 100
            and all(isinstance(p, str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]{0,200}\.desktop", p) for p in pins))
def load(path):
    try:
        with open(path, encoding="utf-8") as source:
            data = json.load(source)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}
'
pins_export() {
    py=$(command -v python3 2>/dev/null || true)
    test -n "$py" || return 0
    "$py" -I -c "$PINS_CHECK
state = load(sys.argv[1])
if valid(state.get('pins')):
    json.dump({'pins': state['pins']}, sys.stdout)
" "$home/.config/winlux/start.json" > "$1/epingles.json" || true
    test -s "$1/epingles.json" || rm -f "$1/epingles.json"
}

# Fusionne les épingles dans start.json sans toucher aux autres clés.
# Annulation (« undo ») sans épingles sauvegardées : le menu reprend ses
# épingles par défaut.
pins_import() {
    py=$(command -v python3 2>/dev/null || true)
    test -n "$py" || return 0
    f="$1/epingles.json"
    if ! test -f "$f" || test -L "$f"; then
        test "${2:-}" = undo || return 0
        f=
    fi
    test -z "$f" || test "$(wc -c < "$f")" -le 65536 || return 0
    state="$home/.config/winlux/start.json"
    mkdir -p "$home/.config/winlux"
    "$py" -I -c "$PINS_CHECK
import os
target, source = sys.argv[1], sys.argv[2]
state = load(target)
if source:
    pins = load(source).get('pins')
    if not valid(pins):
        sys.exit(0)
    state['pins'] = pins
elif 'pins' in state:
    del state['pins']
else:
    sys.exit(0)
temporary = target + '.import.' + str(os.getpid())
with open(temporary, 'w', encoding='utf-8') as out:
    json.dump(state, out)
os.replace(temporary, target)
" "$state" "$f" || true
    own "$state"
}

# ---------------------------------------------------------------- export

build() {
    dest=$1
    bwork=$(mktemp -d "${TMPDIR:-/tmp}/winlux-transfer.XXXXXX")
    trap 'rm -rf "$bwork"' EXIT HUP INT TERM
    mkdir -p "$bwork/user" "$bwork/system"
    {
        printf 'format=1\n'
        printf 'created=%s\n' "$(date '+%Y-%m-%d %H:%M')"
        printf 'host=%s\n' "$(cat "$root/etc/hostname" 2>/dev/null || hostname 2>/dev/null || printf 'winlux')"
        printf 'version=%s\n' "$(head -n 1 "$root/usr/share/winlux/version" 2>/dev/null || printf 'developpement')"
    } > "$bwork/winlux-profile.txt"
    members="winlux-profile.txt"
    for rel in $USER_FILES; do
        f="$home/$rel"
        test -f "$f" && ! test -L "$f" || continue
        mkdir -p "$bwork/user/$(dirname "$rel")"
        cp "$f" "$bwork/user/$rel"
        members="$members user/$rel"
    done
    printf '%s\n' "$SYSTEM_KEYS" | while read -r file keys; do
        test -r "$etc/$file" || continue
        for key in $keys; do
            grep "^$key=" "$etc/$file" | tail -n 1 || true
        done > "$bwork/system/$file"
        test -s "$bwork/system/$file" || rm -f "$bwork/system/$file"
    done
    for f in "$bwork"/system/*; do test -f "$f" && members="$members system/${f##*/}"; done
    appearance_export "$bwork/user"
    pins_export "$bwork/user"
    for f in "$bwork"/user/apparence.conf "$bwork"/user/epingles.json "$bwork"/user/fond.*; do test -f "$f" && members="$members user/${f##*/}"; done
    # Membres au nom du compte, jamais root (#1459).
    # shellcheck disable=SC2086
    (cd "$bwork" && tar -czf "$dest.part" --owner="$home_uid" --group="$home_gid" --numeric-owner $members)
    mv "$dest.part" "$dest"
    own "$dest"
    rm -rf "$bwork"
    trap - EXIT HUP INT TERM
}

export_to() {
    command -v tar >/dev/null 2>&1 || fail "tar est introuvable"
    n=${1:-0}
    case "$n" in *[!0-9]*|'') fail "destination invalide : $n" ;; esac
    dir=$(targets | sed -n "$((n + 1))p" | cut -d '|' -f 2-)
    test -n "$dir" && test -d "$dir" || fail "destination introuvable"
    host=$(cat "$root/etc/hostname" 2>/dev/null || hostname 2>/dev/null || printf 'winlux')
    host=$(printf '%s' "$host" | tr -cd 'A-Za-z0-9._-' | cut -c 1-30)
    dest="$dir/Reglages Winlux ${host:-winlux} $(date '+%Y-%m-%d').wlxprofile"
    umask 022
    build "$dest"
    note "exported" "$(windows_path "$dest")"
}

# ---------------------------------------------------------------- import

# Copie un fichier clé=valeur en ne gardant que les clés permises et des
# valeurs sans caractère de contrôle ni de shell.
filter_keys() {
    src=$1 file=$2 keys=$3
    for key in $keys; do
        line=$(grep "^$key=" "$src" | tail -n 1 || true)
        value=${line#*=}
        test -n "$line" || continue
        printf '%s' "$value" | grep -qE '^[A-Za-z0-9_.,:/+@ -]{0,64}$' || continue
        if relayed; then
            "${WINLUX_ROOT_TOOL:-winlux-root}" conf "$file" "$key" "$value" >/dev/null 2>&1 || true
            continue
        fi
        target="$etc/$file"
        tmp="$target.$$"
        if test -r "$target"; then grep -v "^$key=" "$target" > "$tmp" || true; else : > "$tmp"; fi
        printf '%s=%s\n' "$key" "$value" >> "$tmp"
        mv "$tmp" "$target"
        chmod 644 "$target"
    done
}

# Fichiers personnels : texte « clé=valeur » ou sections .ini, ou petits
# fichiers binaires du bureau (lus avec contrôle de taille par le shell).
user_file_ok() {
    rel=$1 f=$2
    case "$rel" in
        *.dat) return 0 ;;
        .config/mimeapps.list)
            ! grep -vqE '^(\[[A-Za-z ]+\]|[A-Za-z0-9.+-]+/[A-Za-z0-9.+_-]+=[A-Za-z0-9._;-]*|#.*|)$' "$f" ;;
        *) ! grep -vqE '^(\[[^]]*\]|[A-Za-z0-9_. -]+=[^`$]*|#.*|;.*|)$' "$f" ;;
    esac
}

import_file() {
    file=$1
    command -v tar >/dev/null 2>&1 || fail "tar est introuvable"
    test -f "$file" && ! test -L "$file" || fail "fichier introuvable"
    test "$(wc -c < "$file")" -le "$PROFILE_MAX" || fail "fichier trop gros pour des réglages"
    list=$(tar -tzf "$file" 2>/dev/null) || fail "ce fichier n'est pas un fichier de réglages Winlux"
    printf '%s\n' "$list" | grep -qx 'winlux-profile.txt' || fail "ce fichier n'est pas un fichier de réglages Winlux"
    test "$(tar -xzOf "$file" winlux-profile.txt | sed -n 's/^format=//p' | head -n 1)" = 1 || fail "format de réglages inconnu"
    allowed="winlux-profile.txt"
    for rel in $USER_FILES; do allowed="$allowed user/$rel"; done
    for file_keys in $(printf '%s\n' "$SYSTEM_KEYS" | cut -d ' ' -f 1); do allowed="$allowed system/$file_keys"; done
    allowed="$allowed user/apparence.conf user/epingles.json user/fond.png user/fond.jpg user/fond.jpeg user/fond.webp user/fond.bmp user/fond.gif"
    wanted=
    for m in $list; do
        case " $wanted " in *" $m "*) continue ;; esac
        case " $allowed " in *" $m "*) wanted="$wanted $m" ;; esac
    done
    work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-transfer.XXXXXX")
    trap 'rm -rf "$work"' EXIT HUP INT TERM
    # Seuls les membres attendus sont extraits, sans propriétaire ni droits.
    # shellcheck disable=SC2086
    tar -xzf "$file" -C "$work" --no-same-owner --no-same-permissions $wanted 2>/dev/null || fail "archive illisible"
    # Sauvegarde des réglages actuels avant d'écraser quoi que ce soit.
    if test "${2:-}" != undo; then
        mkdir -p "$share"
        build "$backup"
        trap 'rm -rf "$work"' EXIT HUP INT TERM
    fi
    count=0
    for rel in $USER_FILES; do
        f="$work/user/$rel"
        # Annulation : ce qui n'existait pas avant l'import disparaît.
        if test "${2:-}" = undo && ! test -e "$f"; then rm -f "$home/$rel"; continue; fi
        test -f "$f" && ! test -L "$f" || continue
        test "$(wc -c < "$f")" -le "$MAX_BYTES" || continue
        user_file_ok "$rel" "$f" || continue
        mkdir -p "$home/$(dirname "$rel")"
        cp "$f" "$home/$rel.import.$$"
        mv "$home/$rel.import.$$" "$home/$rel"
        own "$home/$rel"
        count=$((count + 1))
    done
    printf '%s\n' "$SYSTEM_KEYS" | while read -r name keys; do
        f="$work/system/$name"
        if test "${2:-}" = undo; then
            if ! test -f "$f"; then : > "$work/empty"; f="$work/empty"; fi
            for key in $keys; do
                # Sous le compte, le relais ne sait qu'écrire : la clé reste.
                grep -q "^$key=" "$f" || relayed || ! test -r "$etc/$name" || sed -i "/^$key=/d" "$etc/$name"
            done
        fi
        test -f "$f" && ! test -L "$f" || continue
        test "$(wc -c < "$f")" -le 4096 || continue
        relayed || mkdir -p "$etc"
        filter_keys "$f" "$name" "$keys"
    done
    appearance_import "$work/user"
    pins_import "$work/user" "${2:-}"
    rm -rf "$work"
    trap - EXIT HUP INT TERM
    note "imported" "$(windows_path "$file")"
}

action=${1:-status}
case "$action" in
    status) ;;
    export) export_to "${2:-0}" ;;
    import)
        n=${2:-}
        case "$n" in *[!0-9]*|'') fail "fichier invalide : $n" ;; esac
        f=$(found | sed -n "$((n + 1))p")
        test -n "$f" || fail "fichier introuvable"
        import_file "$f" ;;
    import-file) import_file "${2:-}" ;;
    undo)
        test -f "$backup" || fail "aucune sauvegarde d'avant import"
        import_file "$backup" undo
        rm -f "$backup"
        note "restored" "" ;;
    *) sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
status
