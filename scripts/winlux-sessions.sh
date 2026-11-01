#!/bin/sh
# winlux-sessions : sessions simultanées et changement rapide d'utilisateur.
#
# Chaque utilisateur connecté garde son propre bureau Winlux sur son propre
# serveur X et son propre terminal virtuel : changer d'utilisateur ne ferme
# rien, on bascule seulement de terminal (comme le « Changer d'utilisateur »
# de Windows). Le registre vit dans /run/winlux/sessions/<compte>.
#
# Usage :
#   winlux-sessions register COMPTE     (appelé par le bureau après connexion)
#   winlux-sessions unregister COMPTE
#   winlux-sessions switch COMPTE       (active la session, la crée au besoin)
#   winlux-sessions logoff COMPTE       (ferme la session de ce compte)
#   winlux-sessions list                (écrit /run/winlux/sessions.txt)
#   winlux-sessions display COMPTE      (affiche l'écran X de la session)
set -eu
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
registry="$run/sessions"
xsock=${WINLUX_X11_SOCKETS:-/tmp/.X11-unix}
mkdir -p "$registry"

fail() { printf 'winlux-sessions : %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
get() { test -r "$1" && sed -n "s/^$2=//p" "$1" | head -n 1 || true; }

check_user() {
    case "$1" in ''|*[!A-Za-z0-9._-]*|.*) fail "nom d'utilisateur invalide : $1" ;; esac
    # Seuls les comptes de l'écran de connexion Winlux ont un bureau.
    users="$root/etc/winlux/users.conf"
    if test -r "$users" && ! cut -d: -f1 "$users" | grep -qxF "$1"; then fail "compte inconnu : $1"; fi
}
current_vt() {
    if test -n "${WINLUX_TEST_VT:-}"; then printf '%s\n' "$WINLUX_TEST_VT"; return; fi
    if have fgconsole; then fgconsole 2>/dev/null && return; fi
    cat /sys/class/tty/tty0/active 2>/dev/null | sed 's/^tty//' || true
}
activate_vt() {
    test -n "$1" || return 0
    if test -n "${WINLUX_TEST_LOG:-}"; then printf 'chvt %s\n' "$1" >> "$WINLUX_TEST_LOG"; return 0; fi
    have chvt || fail "chvt indisponible : impossible de changer de terminal"
    chvt "$1"
}
alive() {
    # Une session vit tant que son écran X répond et que son bureau tourne.
    f="$registry/$1"
    test -f "$f" || return 1
    display=$(get "$f" display)
    test -S "$xsock/X${display#:}" || return 1
    pid=$(get "$f" pid)
    test -z "$pid" || kill -0 "$pid" 2>/dev/null
}
write() {
    user=$1; shift
    tmp="$registry/.$user.$$"
    printf '%s\n' "$@" > "$tmp"
    mv "$tmp" "$registry/$user"
}
home_of() {
    home=
    if test -r "$root/etc/passwd"; then home=$(awk -F: -v u="$1" '$1 == u { print $6; exit }' "$root/etc/passwd"); fi
    if test -z "$home"; then if test "$1" = root; then home=/root; else home="/home/$1"; fi; fi
    printf '%s\n' "$home"
}

do_list() {
    out="$run/sessions.txt"
    active=$(current_vt)
    for f in "$registry"/*; do
        test -f "$f" || continue
        user=${f##*/}
        if ! alive "$user"; then rm -f "$f"; continue; fi
        vt=$(get "$f" vt)
        state=background; test "$vt" != "$active" || state=active
        printf 'session=%s|%s|%s|%s\n' "$user" "$(get "$f" display)" "$vt" "$state"
    done > "$out.tmp.$$"
    mv "$out.tmp.$$" "$out"
}

do_register() {
    user=$1
    display=${DISPLAY:-:0}
    # Un écran n'a qu'un propriétaire : l'ancien est déconnecté de cet écran.
    for f in "$registry"/*; do
        test -f "$f" || continue
        if test "$(get "$f" display)" = "$display" && test "${f##*/}" != "$user"; then rm -f "$f"; fi
    done
    # Session déjà créée par « switch » sur cet écran : garder son suivi.
    if test -f "$registry/$user" && test "$(get "$registry/$user" display)" = "$display"; then do_list; return 0; fi
    # Le bureau principal n'a pas de PID suivi : il vit tant que son écran X vit.
    pid=${WINLUX_SESSION_PID:-}
    write "$user" "display=$display" "vt=$(current_vt)" "pid=$pid" "started=$(date +%s)"
    do_list
}

free_display() {
    n=1
    while test "$n" -lt 16; do
        if ! test -e "$xsock/X$n" && ! test -e "/tmp/.X$n-lock"; then printf '%s\n' "$n"; return; fi
        n=$((n + 1))
    done
    fail "trop de sessions ouvertes"
}

start_session() {
    user=$1
    n=$(free_display)
    vt=$((n + 1))
    test "$vt" -lt 7 || vt=$((n + 7))
    home=$(home_of "$user")
    mkdir -p "$root$home"
    for sub in Bureau Documents Images Musique Vidéos Téléchargements; do mkdir -p "$root$home/$sub"; done
    log="$run/session-$user.log"
    xorg=${WINLUX_XORG:-/usr/bin/Xorg}
    test -x "$xorg" || fail "serveur X introuvable : $xorg"
    "$xorg" ":$n" "vt$vt" -noreset -nolisten tcp -logfile "$run/Xorg.$n.log" > "$log" 2>&1 &
    xpid=$!
    tries=0
    while ! test -S "$xsock/X$n"; do
        tries=$((tries + 1))
        kill -0 "$xpid" 2>/dev/null || fail "le serveur X :$n s'est arrêté (journal $run/Xorg.$n.log)"
        test "$tries" -lt 100 || { kill "$xpid" 2>/dev/null || true; fail "le serveur X :$n ne répond pas"; }
        sleep 0.1
    done
    # Bureau natif (XFCE + Openbox) comme pour le premier compte ; l'ancien
    # bureau de démonstration ne sert que s'il est le seul installé.
    shell=${WINLUX_SESSION_SHELL:-}
    if test -z "$shell"; then
        shell=/usr/share/winlux/xsession-winlux
        if test -x /usr/bin/winlux-user-session && test -x /usr/bin/openbox; then shell=/usr/bin/winlux-user-session; fi
    fi
    uid=
    if test "$user" != root && have id; then uid=$(id -u "$user" 2>/dev/null || true); fi
    runtime=
    if test -n "$uid"; then
        # Dossiers créés ici par root : ils doivent appartenir au compte.
        gid=$(id -g "$user")
        chown "$uid:$gid" "$root$home" 2>/dev/null || true
        for sub in Bureau Documents Images Musique Vidéos Téléchargements; do chown "$uid:$gid" "$root$home/$sub" 2>/dev/null || true; done
        runtime="$root/run/user/$uid"
        mkdir -p "$runtime"
        chown "$uid:$gid" "$runtime" 2>/dev/null || true
        chmod 700 "$runtime"
    fi
    (
        export DISPLAY=":$n" HOME="$home" USER="$user" LOGNAME="$user"
        # Ouverte par un compte déjà authentifié sur l'écran de verrouillage.
        export WINLUX_SESSION_USER="$user" WINLUX_NO_SPLASH=1
        if test -n "$runtime"; then export XDG_RUNTIME_DIR="/run/user/$uid"; fi
        if test -n "$uid" && have setpriv && have xhost; then
            xhost "+si:localuser:$user" >/dev/null 2>&1 || true
            exec setpriv --reuid="$uid" --regid="$(id -g "$user")" --init-groups "$shell"
        fi
        exec "$shell"
    ) >> "$log" 2>&1 &
    spid=$!
    write "$user" "display=:$n" "vt=$vt" "pid=$spid" "xorg_pid=$xpid" "started=$(date +%s)"
    # Fin du bureau : son serveur X s'arrête avec lui.
    ( while kill -0 "$spid" 2>/dev/null; do sleep 1; done; kill "$xpid" 2>/dev/null || true; rm -f "$registry/$user" ) >/dev/null 2>&1 &
    printf '%s\n' "$vt"
}

do_switch() {
    user=$1
    if alive "$user"; then
        vt=$(get "$registry/$user" vt)
    else
        rm -f "$registry/$user"
        vt=$(start_session "$user")
    fi
    activate_vt "$vt"
    do_list
}

do_logoff() {
    user=$1
    f="$registry/$user"
    test -f "$f" || { do_list; return 0; }
    display=$(get "$f" display)
    vt=$(get "$f" vt)
    pid=$(get "$f" pid)
    xpid=$(get "$f" xorg_pid)
    rm -f "$f"
    if test -n "$pid"; then kill "$pid" 2>/dev/null || true; fi
    if test -n "$xpid"; then kill "$xpid" 2>/dev/null || true; fi
    # Revenir sur une autre session ouverte, ou sur l'écran principal.
    if test "$vt" = "$(current_vt)"; then
        next=
        for g in "$registry"/*; do
            test -f "$g" || continue
            if alive "${g##*/}"; then next=$(get "$g" vt); break; fi
        done
        if test -n "$next"; then activate_vt "$next"; elif test "$display" != ":0"; then activate_vt "${WINLUX_PRIMARY_VT:-1}"; fi
    fi
    do_list
}

cmd=${1:-list}
case "$cmd" in
    register) check_user "${2:-}"; do_register "$2" ;;
    unregister) check_user "${2:-}"; rm -f "$registry/$2"; do_list ;;
    switch) check_user "${2:-}"; do_switch "$2" ;;
    logoff) check_user "${2:-}"; do_logoff "$2" ;;
    list) do_list ;;
    display) check_user "${2:-}"; alive "$2" || fail "aucune session ouverte pour $2"; get "$registry/$2" display ;;
    *) sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
