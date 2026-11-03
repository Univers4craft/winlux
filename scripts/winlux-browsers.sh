#!/bin/sh
# Fixed browser catalogue. DEB payloads are isolated, no maintainer scripts.
# No browser ships in the image: the user chooses one (Paramètres >
# Navigateurs), downloaded from its publisher on this PC (index over HTTPS,
# SHA-256 from the index). Edge's licence forbids redistributing it anyway.
set -eu
# Prefer full tools over the limited BusyBox applets in /bin.
PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH
root=${WINLUX_BROWSER_ROOT:-}
cache="$root/usr/share/winlux/browsers"
downloads="$root/var/cache/winlux/browsers"
fetch=${WINLUX_BROWSER_FETCH:-"$root/usr/lib/winlux/winlux-browser-fetch"}
installed="$root/opt/winlux/browsers"
config="$root/etc/winlux/browser.conf"
run="$root/run/winlux"
browser=${2:-none}
mode=${1:---status}
case "${1:---status}" in
    --install|--default|--launch) case "$browser" in edge|chrome|firefox) ;; *) echo 'Navigateur inconnu' >&2; exit 2 ;; esac ;;
    *) browser=none ;;
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
    case "$selected" in edge|chrome|firefox) printf '%s\n' "$selected" ;; *) printf 'none\n' ;; esac
}
status() {
    printf 'browser_default=%s\n' "$(default_browser)"
    for id in chrome edge firefox; do
        state=unavailable
        # Téléchargeable chez l'éditeur : proposé, jamais fourni par Winlux.
        test ! -x "$fetch" || state=downloadable
        test ! -f "$cache/$id.deb" || state=available
        if test -x "$(binary "$id")"; then
            state=installed
            if test -f "$cache/$id.version" && ! cmp -s "$cache/$id.version" "$installed/$id/version"; then state=update; fi
        fi
        printf 'browser_%s=%s\n' "$id" "$state"
    done
}
# Route par défaut présente (comme l'init) : sinon inutile d'attendre le délai réseau.
network_up() {
    awk 'NR > 1 && $2 == "00000000" { found = 1 } END { exit !found }' "${WINLUX_NETWORK_ROUTES:-/proc/net/route}" 2>/dev/null
}
# Sans réseau, le navigateur demandé est noté et l'init le télécharge au
# prochain démarrage connecté (choix par défaut : first-boot-browser).
remember() {
    mkdir -p "$root/etc/winlux"
    if test "$mode" = --default; then
        printf '%s\n' "$1" > "$root/etc/winlux/first-boot-browser"
    else
        pending="$root/etc/winlux/browser-install-pending"
        grep -qx "$1" "$pending" 2>/dev/null || printf '%s\n' "$1" >> "$pending"
    fi
}
forget() {
    pending="$root/etc/winlux/browser-install-pending"
    if test -f "$pending"; then
        grep -vx "$1" "$pending" > "$pending.new" || true
        mv "$pending.new" "$pending"
        test -s "$pending" || rm -f "$pending"
    fi
    if test "$mode" = --default && test "$(sed -n '1p' "$root/etc/winlux/first-boot-browser" 2>/dev/null)" = "$1"; then
        mv "$root/etc/winlux/first-boot-browser" "$root/etc/winlux/first-boot-browser.done"
    fi
}
offline() {
    remember "$1"
    # Code 75 : reporté. Paramètres l'explique avec un message dans la langue de l'interface.
    echo "Pas de connexion Internet : $1 sera téléchargé chez son éditeur et installé automatiquement au prochain démarrage connecté." >&2
    return 75
}
# Heure : synchronisée avant un téléchargement tant qu'aucune synchro n'a
# réussi depuis le démarrage (« force » : même après une synchro).
clock_first() {
    grep -q '^AUTO=0' "$root/etc/winlux/time.conf" 2>/dev/null && return 0
    if test "${1:-}" != force && grep -q '"last"' "$run/time-sync.json" 2>/dev/null; then return 0; fi
    command -v winlux-certificates >/dev/null 2>&1 || return 0
    timeout 30 winlux-certificates time sync >/dev/null 2>&1 || true
}
# Dernier échec dû à la date (certificat « pas encore valide » ou expiré).
clock_wrong() {
    grep -qiE 'certificate is not yet valid|certificate has expired' "$run/browser-fetch.last" 2>/dev/null
}
fetch_logged() {
    status=0
    "$fetch" --only "$1" "$downloads" 2>"$run/browser-fetch.last" || status=$?
    cat "$run/browser-fetch.last" >>"$run/browser-fetch.log" 2>/dev/null || true
    return "$status"
}
install_browser() {
    id=$1
    if ! test -r "$cache/$id.deb"; then
        case "$id" in
            edge|chrome|firefox)
                # Paquet officiel téléchargé chez l'éditeur, jamais fourni par Winlux.
                test -x "$fetch" || { echo "Téléchargement indisponible : $id" >&2; return 1; }
                network_up || { offline "$id"; return 75; }
                mkdir -p "$downloads" "$run"
                chmod 755 "$downloads"
                # Horloge jamais synchronisée depuis le démarrage (pile de la
                # carte mère vide : 2015) : tous les certificats seraient
                # « pas encore valides ». Synchronisée d'abord.
                clock_first
                echo "Téléchargement de $id depuis le site de son éditeur…"
                # Détails techniques (erreurs Python, HTTP) dans un journal, pas à l'écran.
                if ! fetch_logged "$id" && ! { clock_wrong && clock_first force && fetch_logged "$id"; }; then
                    network_up || { offline "$id"; return 75; }
                    if clock_wrong; then
                        echo "La date de ce PC est fausse ($(date +%d/%m/%Y)) : les sites sécurisés refusent la connexion. Activez « Régler l'heure automatiquement » dans Paramètres > Heure et langue, puis réessayez." >&2
                    else
                        echo "Le site de l'éditeur de $id ne répond pas. Réessayez plus tard (détails : /run/winlux/browser-fetch.log)." >&2
                    fi
                    return 1
                fi
                cache=$downloads
                ;;
        esac
    fi
    test -r "$cache/$id.deb" && test -r "$cache/$id.sha256" || { echo "Paquet hors ligne absent : $id" >&2; return 1; }
    if test -x "$(binary "$id")" && cmp -s "$cache/$id.version" "$installed/$id/version"; then forget "$id"; return 0; fi
    if command -v winlux-update >/dev/null 2>&1 && winlux-update --busy 2>/dev/null; then echo 'Attendez la fin de la mise à jour système.' >&2; return 1; fi
    mkdir -p "$installed" "$run"
    chmod 755 "$installed"
    # Le trap est posé avant la prise du verrou : une interruption entre les
    # deux ne laisse pas de verrou orphelin ; lock_held dit s'il nous appartient.
    work=; lock_held=
    previous="$installed/.$id.previous"
    trap 'if test -d "$previous" && ! test -d "$installed/$id"; then mv "$previous" "$installed/$id"; fi; test -z "$work" || rm -rf "$work"; test -z "$lock_held" || rmdir "$run/browser-install.lock" 2>/dev/null || true' EXIT HUP INT TERM
    mkdir "$run/browser-install.lock" 2>/dev/null || { echo 'Installation navigateur déjà en cours' >&2; trap - EXIT HUP INT TERM; return 1; }
    lock_held=1
    work=$(mktemp -d "$installed/.install-$id.XXXXXX")
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
    forget "$id"
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
        # Déjà installé (choix fait depuis une fenêtre « Ouvrir avec ») : pas de
        # nouveau téléchargement, seul le choix change.
        test -x "$(binary "$browser")" || install_browser "$browser"
        mkdir -p "${config%/*}"; printf 'default=%s\n' "$browser" > "$config.new"; mv "$config.new" "$config" ;;
    --launch) shift 2; launch "$browser" "$@" ;;
    --open) shift; launch "$(default_browser)" "$@" ;;
    *) echo 'usage: winlux-browsers --status|--install ID|--default ID|--launch ID|--open [URL]' >&2; exit 2 ;;
esac
