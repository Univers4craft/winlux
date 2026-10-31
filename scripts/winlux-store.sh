#!/bin/sh
# Service du Store Winlux : catalogue, installation et mises à jour.
#
#   winlux-store refresh            état du catalogue -> $run/store.txt
#   winlux-store install <paquet>   installe un paquet apt du catalogue
#   winlux-store remove <paquet>    désinstalle un paquet apt
#   winlux-store upgrade <paquet>   met à jour un paquet installé
#   winlux-store upgrade-all        met à jour tous les paquets du catalogue
#   winlux-store install-file <f>   installe un .exe/.msi (runtime Winlux) ou .deb
#
# Les opérations longues écrivent leur avancement dans $run/store-progress.txt
# (op, cible, state=running|done|error, percent, message), relu par le shell.
set -eu
umask 022
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
catalog=${WINLUX_STORE_CATALOG:-"$root/usr/share/winlux/store-catalog.txt"}
mkdir -p "$run"
export DEBIAN_FRONTEND=noninteractive
have() { command -v "$1" >/dev/null 2>&1; }

progress() {
    # progress <state> <percent> <message>
    tmp="$run/store-progress.txt.$$"
    printf 'op=%s\ntarget=%s\nstate=%s\npercent=%s\nmessage=%s\n' \
        "$op" "$target" "$1" "$2" "$3" > "$tmp"
    mv "$tmp" "$run/store-progress.txt"
}

# Chemin Windows du shell (C:\..., Z:\...) -> chemin Linux.
linux_path() {
    case "$1" in
        [Zz]:*) printf '/%s\n' "$(printf '%s' "${1#??}" | tr '\\' '/' | sed 's#^/*##')" ;;
        [Cc]:*) printf '%s/%s\n' "${WINLUX_DRIVE_C:-${HOME:-/root}}" "$(printf '%s' "${1#??}" | tr '\\' '/' | sed 's#^/*##')" ;;
        *) printf '%s\n' "$1" ;;
    esac
}

valid_package() {
    case "$1" in
        ''|-*|*[!a-z0-9.+-]*) return 1 ;;
    esac
    # Seuls les paquets du catalogue sont gérés par le Store.
    grep -v '^#' "$catalog" 2>/dev/null | cut -d'|' -f4 | grep -qx -- "$1"
}

# Préfixe pour les commandes administrateur (apt).
admin() {
    if test "$(id -u)" = 0; then "$@"
    elif have sudo && sudo -n true 2>/dev/null; then sudo -n "$@"
    elif have pkexec; then pkexec "$@"
    else return 126
    fi
}

refresh() {
    out="$run/store.txt.$$"
    packages=$(grep -v '^#' "$catalog" 2>/dev/null | cut -d'|' -f4 | tr '\n' ' ')
    apt_ok=0
    if have apt-get && have dpkg-query; then apt_ok=1; fi
    admin_ok=0
    if test "$(id -u)" = 0 || { have sudo && sudo -n true 2>/dev/null; } || have pkexec; then admin_ok=1; fi
    states="$run/store-states.$$"
    : > "$states"
    if test "$apt_ok" = 1 && test -n "$packages"; then
        # paquet|i ou vide|version installée
        # shellcheck disable=SC2086
        dpkg-query -W -f '${Package}|${db:Status-Abbrev}|${Version}|${Installed-Size}\n' $packages 2>/dev/null \
            | awk -F'|' '{ s = ($2 ~ /^ii/) ? "i" : ""; print "inst|" $1 "|" s "|" $3 "|" $4 }' >> "$states" || true
        if have apt-cache; then
            # shellcheck disable=SC2086
            apt-cache policy $packages 2>/dev/null | awk '
                /^[^ ].*:$/ { sub(/:$/, ""); pkg = $0; next }
                /^  Candidate:/ { print "cand|" pkg "|" $2 }' >> "$states" || true
            # shellcheck disable=SC2086
            apt-cache show --no-all-versions $packages 2>/dev/null | awk '
                /^Package:/ { pkg = $2 }
                /^Installed-Size:/ { print "size|" pkg "|" $2 }' >> "$states" || true
        fi
    fi
    {
        printf 'apt=%s\nadmin=%s\n' "$apt_ok" "$admin_ok"
        grep -v '^#' "$catalog" 2>/dev/null | grep '|' | awk -F'|' -v states="$states" -v apt="$apt_ok" '
            BEGIN {
                while ((getline line < states) > 0) {
                    n = split(line, f, "|")
                    if (f[1] == "inst") { inst[f[2]] = f[3]; ver[f[2]] = f[4]; if (f[5] != "") size[f[2]] = f[5] }
                    else if (f[1] == "cand") cand[f[2]] = f[3]
                    else if (f[1] == "size" && !(f[2] in size)) size[f[2]] = f[3]
                }
            }
            {
                pkg = $4; c = cand[pkg]; if (c == "(none)") c = ""
                if (inst[pkg] == "i") state = (c != "" && c != ver[pkg]) ? "u" : "i"
                else state = (c != "" || apt == 0) ? "a" : "x"
                if (apt == 0) state = "x"
                v = (inst[pkg] == "i") ? ver[pkg] : ""
                if (state == "u") updates++
                printf "app=%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n", $1, $2, $3, pkg, state, v, c, size[pkg] + 0, $5, $6
            }
            END { printf "updates=%d\n", updates + 0 }'
        # Programmes Windows installés (lanceurs générés par winlux-applications).
        apps=${XDG_DATA_HOME:-${HOME:-/root}/.local/share}/applications
        for desktop in "$apps"/winlux-pe-*.desktop; do
            test -f "$desktop" || continue
            name=$(sed -n 's/^Name=//p' "$desktop" | head -n 1)
            exe=$(sed -n 's/^X-Winlux-Executable=//p' "$desktop" | head -n 1)
            test -n "$name" && printf 'win=%s|%s\n' "$name" "$exe"
        done
    } > "$out"
    rm -f "$states"
    mv "$out" "$run/store.txt"
}

# Lance apt-get en suivant son avancement (APT::Status-Fd).
apt_run() {
    status="$run/store-apt.$$"
    rc=0
    { admin apt-get -y -q -o APT::Status-Fd=3 -o Dpkg::Use-Pty=0 "$@" 3>&1 1>"$run/store.log" 2>&1 || echo "rc:$?"; } \
        | while IFS= read -r line; do
            case "$line" in
                dlstatus:*|pmstatus:*)
                    kind=${line%%:*}; rest=${line#*:}; rest=${rest#*:}
                    pct=${rest%%:*}; pct=${pct%%.*}; msg=${rest#*:}
                    case "$pct" in ''|*[!0-9]*) pct=0 ;; esac
                    if test "$kind" = dlstatus; then pct=$((pct / 2)); else pct=$((50 + pct / 2)); fi
                    progress running "$pct" "$msg" ;;
                rc:*) echo "${line#rc:}" > "$status" ;;
            esac
        done
    if test -s "$status"; then rc=$(cat "$status"); fi
    rm -f "$status"
    return "$rc"
}

finish() {
    # finish <code> <message réussite> <message échec>
    if test "$1" = 0; then progress done 100 "$2"
    elif test "$1" = 126; then progress error 0 'Droits administrateur requis pour installer des logiciels.'
    else progress error 0 "$3 (voir $run/store.log)"
    fi
    refresh || true
}

action=${1:-refresh}
target=${2:-}
op=$action
case "$action" in
    refresh) refresh; exit 0 ;;
esac
if ! mkdir "$run/store.lock" 2>/dev/null; then
    progress error 0 'Une installation est déjà en cours.'
    exit 1
fi
trap 'rmdir "$run/store.lock" 2>/dev/null || true' EXIT HUP INT TERM
case "$action" in
    install|remove|upgrade)
        if ! have apt-get; then progress error 0 'apt-get est indisponible sur ce système.'; exit 1; fi
        if ! valid_package "$target"; then progress error 0 "Paquet inconnu du Store : $target"; exit 2; fi
        progress running 0 'Préparation...'
        rc=0
        case "$action" in
            install) apt_run install --no-install-recommends -- "$target" || rc=$?
                finish "$rc" "$target est installé." "Échec de l'installation de $target" ;;
            remove) apt_run remove -- "$target" || rc=$?
                finish "$rc" "$target est désinstallé." "Échec de la désinstallation de $target" ;;
            upgrade) apt_run install --only-upgrade -- "$target" || rc=$?
                finish "$rc" "$target est à jour." "Échec de la mise à jour de $target" ;;
        esac ;;
    upgrade-all)
        if ! have apt-get; then progress error 0 'apt-get est indisponible sur ce système.'; exit 1; fi
        target=$(sed -n 's/^app=[^|]*|[^|]*|[^|]*|\([^|]*\)|u|.*/\1/p' "$run/store.txt" 2>/dev/null | tr '\n' ' ')
        if test -z "$target"; then progress done 100 'Tous les logiciels sont à jour.'; exit 0; fi
        progress running 0 'Préparation des mises à jour...'
        rc=0
        # shellcheck disable=SC2086
        apt_run install --only-upgrade -- $target || rc=$?
        finish "$rc" 'Mises à jour installées.' 'Échec des mises à jour' ;;
    install-file)
        file=$(linux_path "$target")
        target=$file
        if ! test -f "$file"; then progress error 0 "Fichier introuvable : $file"; exit 2; fi
        case "$(printf '%s' "$file" | tr 'A-Z' 'a-z')" in
            *.exe)
                progress running 10 "Installation de $(basename "$file") avec le runtime Winlux..."
                rc=0
                (cd "$(dirname "$file")" && winlux "$file") >"$run/store.log" 2>&1 || rc=$?
                if have winlux-applications; then winlux-applications refresh >/dev/null 2>&1 || true; fi
                finish "$rc" "$(basename "$file") est terminé." "L'installateur s'est arrêté avec le code $rc" ;;
            *.msi)
                # msiexec doit exister dans C:\Windows\System32 : le runtime le
                # lance comme n'importe quel .exe.
                msiexec="${WINLUX_DRIVE_C:-${HOME:-/root}}/windows/system32/msiexec.exe"
                if ! test -f "$msiexec"; then
                    progress error 0 'Les paquets .msi demandent msiexec, pas encore fourni par le runtime Winlux.'
                    exit 3
                fi
                progress running 10 "Installation de $(basename "$file")..."
                rc=0
                winlux "$msiexec" /i "$file" >"$run/store.log" 2>&1 || rc=$?
                if have winlux-applications; then winlux-applications refresh >/dev/null 2>&1 || true; fi
                finish "$rc" "$(basename "$file") est installé." "msiexec s'est arrêté avec le code $rc" ;;
            *.deb)
                if ! have apt-get; then progress error 0 'apt-get est indisponible sur ce système.'; exit 1; fi
                progress running 0 'Préparation...'
                rc=0
                apt_run install -- "$file" || rc=$?
                finish "$rc" "$(basename "$file") est installé." "Échec de l'installation de $(basename "$file")" ;;
            *) progress error 0 'Format non pris en charge (.exe, .msi ou .deb attendu).'; exit 2 ;;
        esac ;;
    *) progress error 0 "Action inconnue : $action"; exit 2 ;;
esac
