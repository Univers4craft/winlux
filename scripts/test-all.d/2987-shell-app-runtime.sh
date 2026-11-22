# #2987 : winlux-shell-app lance le runtime de WINLUX_RUNTIME quand il est
# posé (passes « par chemin »), et /usr/bin/winlux sinon.
shell_app_runtime() {
    d=$(mktemp -d "${TMPDIR:-/tmp}/winlux-2987.XXXXXX") || return 1
    # Faux runtime : note ses arguments et WINLUX_SHELL_APP.
    printf '#!/bin/sh\nprintf "%%s|%%s\\n" "${WINLUX_SHELL_APP:-}" "$*" >> "%s/appels"\n' "$d" > "$d/winlux"
    chmod +x "$d/winlux"
    rc=0
    for app in explorer store notepad; do
        env WINLUX_RUNTIME="$d/winlux" WINLUX_LEGACY_SETTINGS=0 sh scripts/winlux-shell-app.sh "$app" </dev/null >"$d/$app.log" 2>&1 || {
            echo "$app : le script a échoué :"; cat "$d/$app.log"; rc=1
        }
    done
    printf 'appels du faux runtime :\n'; cat "$d/appels" 2>/dev/null
    for app in explorer store notepad; do
        grep -qx "$app|/opt/winlux/tests/gui_demo.exe" "$d/appels" 2>/dev/null || { echo "$app : faux runtime non appelé"; rc=1; }
    done
    grep -q 'runtime=${WINLUX_RUNTIME:-/usr/bin/winlux}' scripts/winlux-shell-app.sh || { echo "runtime installé par défaut absent"; rc=1; }
    case $d in "${TMPDIR:-/tmp}"/winlux-2987.*) rm -r -- "$d" ;; esac
    return $rc
}
step "winlux-shell-app : WINLUX_RUNTIME respecté pour l'Explorateur, la Boutique et le Bloc-notes (#2987)" shell_app_runtime
