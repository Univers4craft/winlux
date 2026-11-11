# #1490 : un .exe arrêté par SIGTERM (ou SIGINT, SIGHUP) n'est pas un plantage :
# winlux-open ne doit afficher aucun message « arrêté de manière inattendue ».
arret_demande_winlux_open() {
    d=$(mktemp -d "${TMPDIR:-/tmp}/winlux-1490.XXXXXX") || return 1
    # Faux runtime : « --inspect » répond « gui », le lancement s'arrête.
    printf '#!/bin/sh\n[ "$1" = --inspect ] && { echo gui; exit 0; }\nkill -TERM $$\nsleep 5\n' > "$d/winlux-term"
    printf '#!/bin/sh\n[ "$1" = --inspect ] && { echo gui; exit 0; }\nexit 3\n' > "$d/winlux-echec"
    chmod +x "$d/winlux-term" "$d/winlux-echec"
    : > "$d/prog.exe"
    rc=0
    env -u DISPLAY -u WAYLAND_DISPLAY WINLUX_RUNTIME="$d/winlux-term" WINLUX_NO_TERMINAL=1 WINLUX_GUI=headless \
        XDG_STATE_HOME="$d/etat" sh integration/winlux-open "$d/prog.exe" </dev/null >"$d/term.log" 2>&1
    st=$?
    [ "$st" = 143 ] || { echo "SIGTERM : statut $st au lieu de 143"; rc=1; }
    if grep -q 'inattendue' "$d/term.log"; then echo "SIGTERM présenté comme un plantage :"; cat "$d/term.log"; rc=1; fi
    env -u DISPLAY -u WAYLAND_DISPLAY WINLUX_RUNTIME="$d/winlux-echec" WINLUX_NO_TERMINAL=1 WINLUX_GUI=headless \
        XDG_STATE_HOME="$d/etat" sh integration/winlux-open "$d/prog.exe" </dev/null >"$d/echec.log" 2>&1
    grep -q 'inattendue (code 3)' "$d/echec.log" || { echo "un vrai échec n'est plus signalé :"; cat "$d/echec.log"; rc=1; }
    case $d in "${TMPDIR:-/tmp}"/winlux-1490.*) rm -r -- "$d" ;; esac
    return $rc
}
step "winlux-open : arrêt demandé sans message de plantage (#1490)" arret_demande_winlux_open
