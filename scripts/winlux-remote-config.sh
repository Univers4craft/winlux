#!/bin/sh
# winlux-remote-config : réglages du bureau à distance Winlux (page
# « Bureau à distance » des Paramètres) et pilotage du serveur winlux-remote.
#
# Usage :
#   winlux-remote-config status            (écrit /run/winlux/remote.txt)
#   winlux-remote-config enable | disable | restart | ensure
#   winlux-remote-config port N | port-up | port-down | port-default
#   winlux-remote-config allow COMPTE | deny COMPTE
#   winlux-remote-config legacy-on | legacy-off   (mot de passe VNC non chiffré)
#   winlux-remote-config view-only | full-control
set -eu
umask 077
root=${WINLUX_CONTROL_ROOT:-}
conf="$root/etc/winlux/remote.conf"
users_conf="$root/etc/winlux/users.conf"
vnc_passwd="$root/etc/winlux/remote-vnc.passwd"
run="$root/run/winlux"
pidfile="$run/remote.pid"
mkdir -p "$root/etc/winlux" "$run"

fail() { printf 'winlux-remote-config : %s\n' "$*" >&2; exit 1; }
get() { test -r "$conf" && sed -n "s/^$1=//p" "$conf" | tail -n 1 || true; }
put() {
    tmp="$conf.$$"
    if test -r "$conf"; then grep -v "^$1=" "$conf" > "$tmp" || true; else : > "$tmp"; fi
    printf '%s=%s\n' "$1" "$2" >> "$tmp"
    mv "$tmp" "$conf"
}
getd() { v=$(get "$1"); printf '%s\n' "${v:-$2}"; }
port() { p=$(get PORT); case "$p" in ''|*[!0-9]*) echo 5900 ;; *) echo "$p" ;; esac; }
running() {
    test -r "$pidfile" || return 1
    pid=$(cat "$pidfile")
    case "$pid" in ''|*[!0-9]*) return 1 ;; esac
    kill -0 "$pid" 2>/dev/null
}
server=${WINLUX_REMOTE_BIN:-winlux-remote}

start() {
    running && return 0
    command -v "$server" >/dev/null 2>&1 || test -x "$server" || fail "serveur winlux-remote introuvable"
    rm -f "$run/remote-state.txt"
    WINLUX_CONTROL_ROOT="$root" DISPLAY=${DISPLAY:-:0} "$server" --port "$(port)" >> "$run/remote.log" 2>&1 &
    printf '%s\n' "$!" > "$pidfile"
    # Le premier lancement génère la clé RSA : laisser le temps d'écouter.
    tries=0
    while test "$tries" -lt 150; do
        test -r "$run/remote-state.txt" && return 0
        running || { tail -n 3 "$run/remote.log" >&2; fail "le serveur s'est arrêté"; }
        sleep 0.2
        tries=$((tries + 1))
    done
}
stop() {
    if running; then kill "$(cat "$pidfile")" 2>/dev/null || true; fi
    rm -f "$pidfile" "$run/remote-state.txt"
}

check_user() {
    case "$1" in ''|*[!A-Za-z0-9._-]*) fail "nom de compte invalide : $1" ;; esac
    test -r "$users_conf" && cut -d: -f1 "$users_conf" | grep -qxF "$1" || fail "compte inconnu : $1"
}
# Liste effective : USERS, ou les administrateurs tant qu'elle est vide.
allowed() {
    list=$(get USERS)
    if test -n "$list"; then printf '%s\n' "$list" | tr ',' '\n' | sed '/^$/d; /^-$/d'
    elif test -r "$users_conf"; then awk -F: '$2 == "root" { print $1 }' "$users_conf"; fi
}

status() {
    out="$run/remote.txt"
    {
        printf 'enabled=%s\n' "$(getd ENABLED 0)"
        printf 'port=%s\n' "$(port)"
        printf 'legacy=%s\n' "$(getd LEGACY_VNC 0)"
        printf 'view_only=%s\n' "$(getd VIEW_ONLY 0)"
        if test "$(get LEGACY_VNC)" = 1 && test -r "$vnc_passwd"; then printf 'legacy_password=%s\n' "$(head -n 1 "$vnc_passwd")"; fi
        printf 'allowed=%s\n' "$(allowed | tr '\n' ',' | sed 's/,$//')"
        if test -r "$users_conf"; then
            awk -F: 'NF >= 3 { print "account=" $1 "|" $2 "|" ($3 == "nopass" ? 0 : 1) }' "$users_conf"
        fi
        if running && test -r "$run/remote-state.txt"; then
            grep -E '^(fingerprint|clients|client)=' "$run/remote-state.txt" || true
            printf 'running=1\n'
        else printf 'running=0\n'; fi
        addr=
        if command -v ip >/dev/null 2>&1; then
            addr=$(ip -4 -o addr show scope global 2>/dev/null | awk 'NR == 1 { split($4, a, "/"); print a[1] }')
        fi
        printf 'address=%s\n' "${addr:-adresse IP de cet ordinateur}"
    } > "$out.tmp.$$"
    mv "$out.tmp.$$" "$out"
}

action=${1:-status}
case "$action" in
    status) ;;
    enable) put ENABLED 1; start ;;
    disable) put ENABLED 0; stop ;;
    restart) stop; test "$(get ENABLED)" != 1 || start ;;
    ensure) if test "$(get ENABLED)" = 1; then start; fi ;;
    port|port-up|port-down|port-default)
        case "$action" in
            port) new=${2:-} ;;
            port-up) new=$(( $(port) + 1 )) ;;
            port-down) new=$(( $(port) - 1 )) ;;
            *) new=5900 ;;
        esac
        case "$new" in ''|*[!0-9]*) fail "port invalide" ;; esac
        test "$new" -ge 1024 && test "$new" -le 65535 || fail "port hors limites (1024 à 65535)"
        put PORT "$new"
        if running; then stop; start; fi ;;
    allow|deny)
        check_user "${2:-}"
        list=$(allowed | grep -vxF "$2" || true)
        test "$action" = deny || list=$(printf '%s\n%s\n' "$list" "$2")
        list=$(printf '%s\n' "$list" | sed '/^$/d' | tr '\n' ',' | sed 's/,$//')
        # Liste vide après un refus : personne (et non « administrateurs »).
        test -n "$list" || list=-
        put USERS "$list" ;;
    legacy-on)
        # Mot de passe aléatoire de 8 caractères (limite du protocole VNC).
        pw=$(tr -dc 'A-HJ-NP-Za-km-z2-9' < /dev/urandom | head -c 8)
        printf '%s\n' "$pw" > "$vnc_passwd"
        chmod 600 "$vnc_passwd"
        put LEGACY_VNC 1 ;;
    legacy-off) rm -f "$vnc_passwd"; put LEGACY_VNC 0 ;;
    view-only) put VIEW_ONLY 1 ;;
    full-control) put VIEW_ONLY 0 ;;
    *) sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
status
