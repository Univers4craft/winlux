#!/bin/sh
# Fixed offline browser catalogue. DEB payloads are isolated, no maintainer scripts.
set -eu
# Prefer full tools over the limited BusyBox applets in /bin.
PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH
root=${WINLUX_BROWSER_ROOT:-}
cache="$root/usr/share/winlux/browsers"
installed="$root/opt/winlux/browsers"
config="$root/etc/winlux/browser.conf"
run="$root/run/winlux"
browser=${2:-edge}
case "${1:---status}" in
    --install|--default|--launch) case "$browser" in edge|chrome|firefox) ;; *) echo 'Navigateur inconnu' >&2; exit 2 ;; esac ;;
    *) browser=edge ;;
esac
binary() {
    case "$1" in
        edge) printf '%s/edge/root/opt/microsoft/msedge/microsoft-edge\n' "$installed" ;;
        chrome) printf '%s/chrome/root/opt/google/chrome/google-chrome\n' "$installed" ;;
        firefox) printf '%s/firefox/root/usr/lib/firefox/firefox\n' "$installed" ;;
    esac
}
default_browser() {
    selected=$(sed -n 's/^default=//p' "$config" 2>/dev/null | head -n 1)
    case "$selected" in edge|chrome|firefox) printf '%s\n' "$selected" ;; *) printf 'edge\n' ;; esac
}
status() {
    printf 'browser_default=%s\n' "$(default_browser)"
    for id in edge chrome firefox; do
        state=unavailable
        test ! -f "$cache/$id.deb" || state=available
        if test -x "$(binary "$id")"; then
            state=installed
            if test -f "$cache/$id.version" && ! cmp -s "$cache/$id.version" "$installed/$id/version"; then state=update; fi
        fi
        printf 'browser_%s=%s\n' "$id" "$state"
    done
}
install_browser() {
    id=$1
    test -r "$cache/$id.deb" && test -r "$cache/$id.sha256" || { echo "Paquet hors ligne absent : $id" >&2; return 1; }
    if test -x "$(binary "$id")" && cmp -s "$cache/$id.version" "$installed/$id/version"; then return 0; fi
    if command -v winlux-update >/dev/null 2>&1 && winlux-update --busy 2>/dev/null; then echo 'Attendez la fin de la mise à jour système.' >&2; return 1; fi
    mkdir -p "$installed" "$run"
    chmod 755 "$installed"
    mkdir "$run/browser-install.lock" 2>/dev/null || { echo 'Installation navigateur déjà en cours' >&2; return 1; }
    work=$(mktemp -d "$installed/.install-$id.XXXXXX")
    previous="$installed/.$id.previous"
    trap 'if test -d "$previous" && ! test -d "$installed/$id"; then mv "$previous" "$installed/$id"; fi; rm -rf "$work"; rmdir "$run/browser-install.lock" 2>/dev/null || true' EXIT HUP INT TERM
    cp "$cache/$id.deb" "$work/package.deb"
    hash=$(cat "$cache/$id.sha256")
    printf '%s  %s\n' "$hash" "$work/package.deb" | sha256sum -c - || return 1
    expected=$id
    case "$id" in edge) expected=microsoft-edge-stable ;; chrome) expected=google-chrome-stable ;; esac
    test "$(/usr/bin/dpkg-deb -f "$work/package.deb" Package)" = "$expected" || { echo 'Paquet inattendu' >&2; return 1; }
    test "$(/usr/bin/dpkg-deb -f "$work/package.deb" Architecture)" = amd64 || return 1
    mkdir "$work/root"
    chmod 755 "$work" "$work/root"
    /usr/bin/dpkg-deb -x "$work/package.deb" "$work/root"
    chmod 755 "$work/root"
    target=$(binary "$id")
    relative=${target#"$installed/$id/"}
    test -x "$work/$relative" || { echo 'Programme navigateur absent du paquet' >&2; return 1; }
    /usr/bin/dpkg-deb -f "$work/package.deb" Version > "$work/version"
    rm "$work/package.deb"
    # Refuse replacement while any process still has the previous browser mapped.
    if test -d "$installed/$id"; then
        for mapping in /proc/[0-9]*/maps; do
            if grep -Fq "$installed/$id/root/" "$mapping" 2>/dev/null; then echo 'Fermez ce navigateur avant sa mise à jour.' >&2; return 1; fi
        done
        test ! -e "$previous" || { echo 'Ancienne transaction navigateur à récupérer.' >&2; return 1; }
        mv "$installed/$id" "$previous"
    fi
    mv "$work" "$installed/$id"
    rm -rf "$previous"
    trap - EXIT HUP INT TERM
    rmdir "$run/browser-install.lock"
    echo "Navigateur installé : $id"
    if command -v winlux-applications >/dev/null 2>&1; then winlux-applications refresh >/dev/null 2>&1 || true; fi
}
launch() {
    id=$1; shift
    program=$(binary "$id")
    if ! test -x "$program"; then
        mkdir -p "$run"
        printf '1\n' > "$run/browser-chooser"
        echo 'Choisissez un navigateur dans Paramètres > Navigateurs.' >&2
        return 0
    fi
    if test "$(id -u)" = 0 && test -z "$root"; then
        # Vendor sandbox stays enabled: the browser runs as its own user.
        home=/home/winlux-web
        test -d "$home" || { echo 'Compte navigateur non initialisé' >&2; return 1; }
        case "$id" in edge|chrome)
            size=$(/usr/bin/xrandr --current 2>/dev/null | awk '/\*/ {print $1;exit}')
            width=${size%x*}; height=${size#*x}
            case "$width:$height" in *[!0-9:]*|:|*:) width=1920; height=1080 ;; esac
            width=$((width * 80 / 100)); height=$((height * 75 / 100))
            test "$width" -le 1120 || width=1120
            test "$height" -le 740 || height=740
            set -- --no-first-run --no-default-browser-check "--window-size=$width,$height" "$@"
            ;;
        esac
        xhost +SI:localuser:winlux-web >/dev/null 2>&1 || true
        for helper in "${program%/*}/chrome-sandbox" "${program%/*}/msedge-sandbox"; do
            if test -f "$helper" && ! test -L "$helper"; then chown root:root "$helper"; chmod 4755 "$helper"; fi
        done
        mkdir -p "$run"
        # Téléchargements : le dossier de l'utilisateur (C:\Users\<nom>\Downloads),
        # et « Ouvrir le dossier » ouvre l'Explorateur de la session.
        downloads=/usr/share/winlux/native/browser/winlux-browser-downloads.py
        bus_config=
        if test -f "$downloads"; then
            /usr/bin/python3 "$downloads" prepare >>"$run/browser-downloads.log" 2>&1 || true
            /usr/bin/python3 "$downloads" serve "$$" >>"$run/browser-downloads.log" 2>&1 &
            test ! -f "${downloads%/*}/session-bus.conf" || bus_config="--config-file=${downloads%/*}/session-bus.conf"
        fi
        if /usr/bin/setpriv --reuid=winlux-web --regid=winlux-web --init-groups --reset-env \
            env HOME="$home" USER=winlux-web LOGNAME=winlux-web DISPLAY="${DISPLAY:-:0}" \
            /usr/bin/dbus-run-session $bus_config -- "$program" "$@"; then return 0; else
            printf 'Le navigateur a échoué. Consultez le journal navigateur.\n' > "$run/action.txt"
            printf '1\n' > "$run/browser-chooser"
            return 1
        fi
    fi
    exec "$program" "$@"
}
case "${1:---status}" in
    --status) status ;;
    --install) install_browser "$browser" ;;
    --default)
        install_browser "$browser"
        mkdir -p "${config%/*}"; printf 'default=%s\n' "$browser" > "$config.new"; mv "$config.new" "$config" ;;
    --launch) shift 2; launch "$browser" "$@" ;;
    --open) shift; launch "$(default_browser)" "$@" ;;
    *) echo 'usage: winlux-browsers --status|--install ID|--default ID|--launch ID|--open [URL]' >&2; exit 2 ;;
esac
