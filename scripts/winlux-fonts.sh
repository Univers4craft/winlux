#!/bin/sh
# Service du gestionnaire de polices Winlux (fontconfig).
#
#   winlux-fonts list             polices installées -> $run/fonts.txt
#   winlux-fonts install <fich>   installe une police .ttf/.otf/.ttc/.otc
#                                 pour l'utilisateur (dossier fontconfig)
#   winlux-fonts remove <fich>    désinstalle une police de l'utilisateur
#
# fonts.txt : font=famille|style|portée (system ou user)|chemin
# fonts-action.txt : state=done|error et message, relus par le shell.
# Les .exe voient les mêmes polices : le runtime lit ces dossiers pour
# EnumFontFamilies, et Xft les dessine via fontconfig.
set -eu
umask 022
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
mkdir -p "$run"
home=${HOME:-/root}
user_dir=${XDG_DATA_HOME:-$home/.local/share}/fonts
have() { command -v "$1" >/dev/null 2>&1; }

# Langue de l'interface (Paramètres > Langue) : fr, en, es ou de.
lang=${WINLUX_LANG:-}
if test -z "$lang" && test -r "$root/etc/winlux/locale.conf"; then
    lang=$(sed -n 's/^LANG=//p' "$root/etc/winlux/locale.conf" | head -n 1)
fi
lang=$(printf '%s' "${lang:-${LANG:-fr}}" | cut -c1-2 | tr 'A-Z' 'a-z')
case "$lang" in fr|en|es|de) ;; *) lang=fr ;; esac

# msg clé [valeur] : message traduit, relu et affiché par l'application Polices.
msg() {
    case "$lang:$1" in
        fr:not_found) printf 'Fichier introuvable : %s' "$2" ;;
        en:not_found) printf 'File not found: %s' "$2" ;;
        es:not_found) printf 'Archivo no encontrado: %s' "$2" ;;
        de:not_found) printf 'Datei nicht gefunden: %s' "$2" ;;
        fr:invalid) printf "%s n'est pas une police TrueType ou OpenType." "$2" ;;
        en:invalid) printf '%s is not a TrueType or OpenType font.' "$2" ;;
        es:invalid) printf '%s no es una fuente TrueType u OpenType.' "$2" ;;
        de:invalid) printf '%s ist keine TrueType- oder OpenType-Schriftart.' "$2" ;;
        fr:already) printf '%s est déjà installée.' "$2" ;;
        en:already) printf '%s is already installed.' "$2" ;;
        es:already) printf '%s ya está instalada.' "$2" ;;
        de:already) printf '%s ist bereits installiert.' "$2" ;;
        fr:installed) printf '%s est installée.' "$2" ;;
        en:installed) printf '%s is installed.' "$2" ;;
        es:installed) printf '%s está instalada.' "$2" ;;
        de:installed) printf '%s ist installiert.' "$2" ;;
        fr:protected) printf 'Les polices système sont protégées : seules vos polices peuvent être désinstallées.' ;;
        en:protected) printf 'System fonts are protected: only your own fonts can be uninstalled.' ;;
        es:protected) printf 'Las fuentes del sistema están protegidas: solo se pueden desinstalar sus fuentes.' ;;
        de:protected) printf 'Systemschriftarten sind geschützt: Nur Ihre eigenen Schriftarten können deinstalliert werden.' ;;
        fr:refused) printf 'Chemin refusé.' ;;
        en:refused) printf 'Path refused.' ;;
        es:refused) printf 'Ruta rechazada.' ;;
        de:refused) printf 'Pfad abgelehnt.' ;;
        fr:font_missing) printf 'Police introuvable : %s' "$2" ;;
        en:font_missing) printf 'Font not found: %s' "$2" ;;
        es:font_missing) printf 'Fuente no encontrada: %s' "$2" ;;
        de:font_missing) printf 'Schriftart nicht gefunden: %s' "$2" ;;
        fr:removed) printf '%s est désinstallée.' "$2" ;;
        en:removed) printf '%s is uninstalled.' "$2" ;;
        es:removed) printf '%s está desinstalada.' "$2" ;;
        de:removed) printf '%s ist deinstalliert.' "$2" ;;
        fr:unknown) printf 'Action inconnue : %s' "$2" ;;
        en:unknown) printf 'Unknown action: %s' "$2" ;;
        es:unknown) printf 'Acción desconocida: %s' "$2" ;;
        de:unknown) printf 'Unbekannte Aktion: %s' "$2" ;;
    esac
}

result() {
    tmp="$run/fonts-action.txt.$$"
    printf 'state=%s\nmessage=%s\n' "$1" "$2" > "$tmp"
    mv "$tmp" "$run/fonts-action.txt"
}

linux_path() {
    case "$1" in
        [Zz]:*) printf '/%s\n' "$(printf '%s' "${1#??}" | tr '\\' '/' | sed 's#^/*##')" ;;
        [Cc]:*) printf '%s/%s\n' "${WINLUX_DRIVE_C:-$home}" "$(printf '%s' "${1#??}" | tr '\\' '/' | sed 's#^/*##')" ;;
        *) printf '%s\n' "$1" ;;
    esac
}

# Dossiers de polices des applications Windows (C:\Windows\Fonts et le
# dossier par utilisateur de Windows 10+) : visibles de Linux, et
# désinstallables comme les polices de l'utilisateur.
drive_c=${WINLUX_DRIVE_C:-$home}
windows_dirs="$drive_c/Windows/Fonts $drive_c/windows/Fonts $drive_c/WINDOWS/Fonts $drive_c/AppData/Local/Microsoft/Windows/Fonts"

is_user_font() {
    case "$1" in
        "$user_dir"/*|"$home"/.fonts/*) return 0 ;;
    esac
    for dir in $windows_dirs; do
        case "$1" in "$dir"/*) return 0 ;; esac
    done
    return 1
}

# Vérifie la signature sfnt (TrueType, OpenType/CFF, collection).
valid_font() {
    case "$(printf '%s' "$1" | tr 'A-Z' 'a-z')" in
        *.ttf|*.otf|*.ttc|*.otc) ;;
        *) return 1 ;;
    esac
    magic=$(od -An -N4 -tx1 "$1" 2>/dev/null | tr -d ' \n')
    case "$magic" in
        00010000|4f54544f|74746366|74727565) return 0 ;;
    esac
    return 1
}

list() {
    out="$run/fonts.txt.$$"
    {
        if have fc-list; then
            fc-list --format '%{family[0]}|%{style[0]}|%{file}\n' 2>/dev/null
        else
            for dir in "$root/usr/share/fonts" "$root/usr/local/share/fonts" "$user_dir" "$home/.fonts" $windows_dirs; do
                test -d "$dir" || continue
                find "$dir" -type f \( -iname '*.ttf' -o -iname '*.otf' -o -iname '*.ttc' -o -iname '*.otc' \) 2>/dev/null \
                    | while IFS= read -r f; do
                        name=$(basename "$f"); name=${name%.*}
                        printf '%s|Regular|%s\n' "$name" "$f"
                    done
            done
        fi
    } | awk -F'|' -v user="$user_dir/" -v legacy="$home/.fonts/" -v drive="$drive_c/" '
        NF >= 3 && $3 ~ /\.([Tt][Tt][FfCc]|[Oo][Tt][FfCc])$/ {
            rest = (index($3, drive) == 1) ? tolower(substr($3, length(drive) + 1)) : ""
            windows = (index(rest, "windows/fonts/") == 1 || index(rest, "appdata/local/microsoft/windows/fonts/") == 1)
            scope = (index($3, user) == 1 || index($3, legacy) == 1 || windows) ? "user" : "system"
            print "font=" $1 "|" $2 "|" scope "|" $3
        }' | sort -f -u > "$out"
    mv "$out" "$run/fonts.txt"
}

refresh_cache() {
    if have fc-cache; then fc-cache -f "$user_dir" >/dev/null 2>&1 || true; fi
}

action=${1:-list}
case "$action" in
    list) list ;;
    install)
        file=$(linux_path "${2:-}")
        if ! test -f "$file"; then result error "$(msg not_found "$file")"; exit 2; fi
        if ! valid_font "$file"; then result error "$(msg invalid "$(basename "$file")")"; exit 2; fi
        mkdir -p "$user_dir"
        dest="$user_dir/$(basename "$file")"
        if test -e "$dest" && cmp -s "$file" "$dest"; then
            result done "$(msg already "$(basename "$file")")"
        else
            cp "$file" "$dest.tmp.$$"
            chmod 644 "$dest.tmp.$$"
            mv "$dest.tmp.$$" "$dest"
            refresh_cache
            result done "$(msg installed "$(basename "$file")")"
        fi
        list ;;
    remove)
        file=$(linux_path "${2:-}")
        if ! is_user_font "$file"; then
            result error "$(msg protected)"
            exit 2
        fi
        case "$file" in *..*) result error "$(msg refused)"; exit 2 ;; esac
        if ! test -f "$file"; then result error "$(msg font_missing "$file")"; exit 2; fi
        rm -f "$file"
        refresh_cache
        result done "$(msg removed "$(basename "$file")")"
        list ;;
    *) result error "$(msg unknown "$action")"; exit 2 ;;
esac
