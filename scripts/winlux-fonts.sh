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

is_user_font() {
    case "$1" in
        "$user_dir"/*|"$home"/.fonts/*) return 0 ;;
    esac
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
            for dir in "$root/usr/share/fonts" "$root/usr/local/share/fonts" "$user_dir" "$home/.fonts"; do
                test -d "$dir" || continue
                find "$dir" -type f \( -iname '*.ttf' -o -iname '*.otf' -o -iname '*.ttc' -o -iname '*.otc' \) 2>/dev/null \
                    | while IFS= read -r f; do
                        name=$(basename "$f"); name=${name%.*}
                        printf '%s|Regular|%s\n' "$name" "$f"
                    done
            done
        fi
    } | awk -F'|' -v user="$user_dir/" -v legacy="$home/.fonts/" '
        NF >= 3 && $3 ~ /\.([Tt][Tt][FfCc]|[Oo][Tt][FfCc])$/ {
            scope = (index($3, user) == 1 || index($3, legacy) == 1) ? "user" : "system"
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
        if ! test -f "$file"; then result error "Fichier introuvable : $file"; exit 2; fi
        if ! valid_font "$file"; then result error "$(basename "$file") n'est pas une police TrueType ou OpenType."; exit 2; fi
        mkdir -p "$user_dir"
        dest="$user_dir/$(basename "$file")"
        if test -e "$dest" && cmp -s "$file" "$dest"; then
            result done "$(basename "$file") est déjà installée."
        else
            cp "$file" "$dest.tmp.$$"
            chmod 644 "$dest.tmp.$$"
            mv "$dest.tmp.$$" "$dest"
            refresh_cache
            result done "$(basename "$file") est installée."
        fi
        list ;;
    remove)
        file=$(linux_path "${2:-}")
        if ! is_user_font "$file"; then
            result error 'Les polices système sont protégées : seules vos polices peuvent être désinstallées.'
            exit 2
        fi
        case "$file" in *..*) result error 'Chemin refusé.'; exit 2 ;; esac
        if ! test -f "$file"; then result error "Police introuvable : $file"; exit 2; fi
        rm -f "$file"
        refresh_cache
        result done "$(basename "$file") est désinstallée."
        list ;;
    *) result error "Action inconnue : $action"; exit 2 ;;
esac
