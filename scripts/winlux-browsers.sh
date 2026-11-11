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
# Catalogue, ordre alphabétique des noms affichés (Brave, Google Chrome,
# Microsoft Edge, Mozilla Firefox, Opera, Opera GX, Tor Browser, Vivaldi) :
# tous ont une version Linux officielle (relevé du 11 novembre 2026, #1716).
ids='brave chrome edge firefox opera operagx tor vivaldi'
known() {
    case "$1" in brave|chrome|edge|firefox|opera|operagx|tor|vivaldi) return 0 ;; *) return 1 ;; esac
}
# Clé des développeurs de Tor Browser, épinglée ici aussi : l'archive est
# revérifiée avant extraction, pas seulement au téléchargement.
tor_fingerprint=EF6E286DDA85EA2A4BA7DE684E2C6E8793298290
if test -n "$root" && test -n "${WINLUX_BROWSER_TOR_FINGERPRINT:-}"; then tor_fingerprint=$WINLUX_BROWSER_TOR_FINGERPRINT; fi
case "${1:---status}" in
    --install|--default|--launch) known "$browser" || { echo 'Navigateur inconnu' >&2; exit 2; } ;;
    *) browser=none ;;
esac
# Programme habituel dans le paquet de l'éditeur. Le chemin réellement trouvé
# à l'installation est noté dans <id>/program et prime.
default_program() {
    case "$1" in
        brave) printf 'root/opt/brave.com/brave/brave-browser\n' ;;
        chrome) printf 'root/opt/google/chrome/google-chrome\n' ;;
        edge) printf 'root/opt/microsoft/msedge/microsoft-edge\n' ;;
        firefox) printf 'root/usr/lib/firefox/firefox\n' ;;
        opera) printf 'root/usr/lib/x86_64-linux-gnu/opera/opera\n' ;;
        operagx) printf 'root/usr/lib/x86_64-linux-gnu/opera-gx/opera-gx\n' ;;
        tor) printf 'root/tor-browser/Browser/start-tor-browser\n' ;;
        vivaldi) printf 'root/opt/vivaldi/vivaldi\n' ;;
    esac
}
safe_relative() {
    case "$1" in root/*) ;; *) return 1 ;; esac
    case "/$1/" in */../*|*/./*|*//*) return 1 ;; esac
    return 0
}
binary() {
    noted=$(sed -n '1p' "$installed/$1/program" 2>/dev/null || true)
    if test -n "$noted" && safe_relative "$noted"; then
        printf '%s/%s/%s\n' "$installed" "$1" "$noted"
    else
        printf '%s/%s/%s\n' "$installed" "$1" "$(default_program "$1")"
    fi
}
# Programme dans l'arbre extrait : le chemin habituel, sinon le lien de
# /usr/bin que pose le paquet (un éditeur peut changer de dossier).
find_program() {
    tree=$1; id=$2
    relative=$(default_program "$id")
    if test -x "$tree/$relative" && ! test -d "$tree/$relative"; then printf '%s\n' "$relative"; return 0; fi
    test "$id" != tor || return 1
    for link in "$tree"/root/usr/bin/*; do
        test -L "$link" || continue
        target=$(readlink "$link")
        case "$target" in /opt/*|/usr/lib/*) ;; *) continue ;; esac
        candidate="root$target"
        safe_relative "$candidate" || continue
        if test -x "$tree/$candidate" && ! test -d "$tree/$candidate"; then printf '%s\n' "$candidate"; return 0; fi
    done
    return 1
}
# Nom du paquet attendu dans le .deb de l'éditeur.
expected_package() {
    case "$1:$2" in
        brave:brave-browser|chrome:google-chrome-stable|edge:microsoft-edge-stable|firefox:firefox) return 0 ;;
        opera:opera-stable|operagx:opera-gx-stable|operagx:opera-gx|vivaldi:vivaldi-stable) return 0 ;;
    esac
    return 1
}
default_browser() {
    selected=$(sed -n 's/^default=//p' "$config" 2>/dev/null | head -n 1)
    if known "$selected"; then printf '%s\n' "$selected"; else printf 'none\n'; fi
}
status() {
    printf 'browser_default=%s\n' "$(default_browser)"
    for id in $ids; do
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
# Firefox dans la langue du système (fr, de, es ; l'anglais est intégré) :
# paquets de langue officiels de Mozilla, vérifiés comme le navigateur.
# Facultatifs : un paquet absent ou altéré est ignoré, Firefox reste en anglais.
# Chaque langpack-*.xpi est placé à deux endroits : distribution/extensions
# (installé dans un profil neuf) et browser/extensions (relu à chaque
# démarrage : un profil déjà créé en anglais le reçoit aussi).
add_langpacks() {
    firefox_dir="$1/usr/lib/firefox"
    for lang in fr de es; do
        pack="$cache/firefox-l10n-$lang"
        if ! test -r "$pack.deb" || ! test -r "$pack.sha256"; then
            # Trace pour le diagnostic : sans lui, Firefox reste en anglais dans cette langue.
            echo "Paquet de langue de Firefox absent : $lang" >&2
            continue
        fi
        printf '%s  %s\n' "$(cat "$pack.sha256")" "$pack.deb" | sha256sum -c - >/dev/null 2>&1 || { echo "Paquet de langue altéré ignoré : $lang" >&2; continue; }
        # Espagnol : paquet par pays chez Mozilla (firefox-l10n-es-es, -es-ar…, #432).
        case "$(/usr/bin/dpkg-deb -f "$pack.deb" Package)" in
            "firefox-l10n-$lang"|"firefox-l10n-$lang"-[a-z][a-z]) ;;
            *) echo "Paquet de langue inattendu ignoré : $lang" >&2; continue ;;
        esac
        /usr/bin/dpkg-deb -x "$pack.deb" "$1"
    done
    mkdir -p "$firefox_dir/distribution/extensions" "$firefox_dir/browser/extensions" "$firefox_dir/defaults/pref"
    find "$1" -name 'langpack-*.xpi' -type f | while IFS= read -r xpi; do
        for target in distribution/extensions browser/extensions; do
            test "$xpi" -ef "$firefox_dir/$target/${xpi##*/}" || cp "$xpi" "$firefox_dir/$target/"
        done
    done
    # Préférence vide = langue du système (LANG, que winlux-browsers donne au
    # lancement) ; les paquets de langue du dossier de l'application (portée 4)
    # sont activés sans demande, comme ceux d'une distribution.
    printf '%s\n' 'pref("intl.locale.requested", "");' 'pref("extensions.autoDisableScopes", 11);' \
        > "$firefox_dir/defaults/pref/winlux-langue.js"
    chmod -R a+rX "$firefox_dir"
}
# Tor Browser : signature des développeurs de Tor revérifiée avec la clé que
# winlux-browser-fetch a contrôlée (empreinte principale épinglée ci-dessus).
tor_signature_ok() {
    archive=$1; signature=$2; keyring=$3
    test -r "$signature" && test -r "$keyring" || { echo 'Signature de Tor Browser absente' >&2; return 1; }
    command -v gpgv >/dev/null 2>&1 || { echo 'gpgv absent : signature de Tor Browser invérifiable' >&2; return 1; }
    gnupg_home=$(mktemp -d)
    statuses=$(gpgv --homedir "$gnupg_home" --keyring "$keyring" --status-fd 1 "$signature" "$archive" 2>/dev/null) || statuses=
    rm -r -f "${gnupg_home:?}"
    printf '%s\n' "$statuses" | grep -q '^\[GNUPG:\] GOODSIG ' || { echo 'Signature de Tor Browser invalide' >&2; return 1; }
    printf '%s\n' "$statuses" | grep -qi "^\[GNUPG:\] VALIDSIG .* $tor_fingerprint\$" || { echo 'Tor Browser signé par une clé inattendue' >&2; return 1; }
}
# Archive de Tor Browser : aucun chemin absolu ni « .. », un seul dossier tor-browser.
tor_extract() {
    archive=$1; target=$2
    listing=$(tar -tJf "$archive") || { echo 'Archive de Tor Browser illisible' >&2; return 1; }
    if printf '%s\n' "$listing" | grep -qE '^/|(^|/)\.\.(/|$)'; then echo 'Archive de Tor Browser refusée (chemin hors du dossier)' >&2; return 1; fi
    if printf '%s\n' "$listing" | grep -qvE '^(\./)?(tor-browser(/|$)|$)'; then echo 'Archive de Tor Browser inattendue' >&2; return 1; fi
    tar -xJf "$archive" --no-same-owner -C "$target"
}
install_browser() {
    id=$1
    format=deb
    test "$id" != tor || format=tar.xz
    if ! test -r "$cache/$id.$format"; then
        case "$id" in
            brave|chrome|edge|firefox|opera|operagx|tor|vivaldi)
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
    test -r "$cache/$id.$format" && test -r "$cache/$id.sha256" || { echo "Paquet hors ligne absent : $id" >&2; return 1; }
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
    cp "$cache/$id.$format" "$work/package.$format"
    hash=$(cat "$cache/$id.sha256")
    printf '%s  %s\n' "$hash" "$work/package.$format" | sha256sum -c - || return 1
    mkdir "$work/root"
    chmod 755 "$work" "$work/root"
    if test "$id" = tor; then
        tor_signature_ok "$work/package.tar.xz" "$cache/tor.tar.xz.asc" "$cache/tor.keyring" || return 1
        tor_extract "$work/package.tar.xz" "$work/root" || return 1
        sed -n '1p' "$cache/tor.version" > "$work/version"
    else
        expected_package "$id" "$(/usr/bin/dpkg-deb -f "$work/package.deb" Package)" || { echo 'Paquet inattendu' >&2; return 1; }
        test "$(/usr/bin/dpkg-deb -f "$work/package.deb" Architecture)" = amd64 || return 1
        /usr/bin/dpkg-deb -x "$work/package.deb" "$work/root"
        /usr/bin/dpkg-deb -f "$work/package.deb" Version > "$work/version"
    fi
    chmod 755 "$work/root"
    relative=$(find_program "$work" "$id") || { echo 'Programme navigateur absent du paquet' >&2; return 1; }
    printf '%s\n' "$relative" > "$work/program"
    rm -f "$work/package.deb" "$work/package.tar.xz"
    if test "$id" = firefox; then add_langpacks "$work/root"; fi
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
# Langue de l'interface Winlux (Paramètres > Langue : fr, en, de, es), donnée
# au navigateur : Firefox (avec son paquet de langue), Chrome et Edge suivent
# LANG/LANGUAGE. La session met LANG=fr_FR.UTF-8 quelle que soit la langue
# choisie, et un LANG absent ou « C » laissait Firefox en anglais.
browser_locale() {
    code=$(sed -n 's/^LANG=//p' "$root/etc/winlux/locale.conf" 2>/dev/null | head -n 1 | cut -c1-2)
    test -n "$code" || code=$(printf '%s' "${LANG:-fr}" | cut -c1-2)
    case "$code" in
        en) printf 'en_US.UTF-8\n' ;;
        de) printf 'de_DE.UTF-8\n' ;;
        es) printf 'es_ES.UTF-8\n' ;;
        *) printf 'fr_FR.UTF-8\n' ;;
    esac
}
launch() {
    id=$1; shift
    locale=$(browser_locale)
    LANG=$locale; LANGUAGE=${locale%%_*}; LC_MESSAGES=$locale
    export LANG LANGUAGE LC_MESSAGES
    unset LC_ALL
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
        case "$id" in brave|chrome|edge|opera|operagx|vivaldi)
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
        if test "$id" = tor; then
            # Copie personnelle du compte du navigateur (voir tor_copy).
            program=$(/usr/bin/setpriv --reuid=winlux-web --regid=winlux-web --init-groups --reset-env \
                env HOME="$home" /bin/sh "$0" --tor-copy) || { echo 'Copie de Tor Browser impossible' >&2; return 1; }
        fi
        for helper in "${program%/*}/chrome-sandbox" "${program%/*}/msedge-sandbox" "${program%/*}/opera_sandbox" "${program%/*}/vivaldi-sandbox"; do
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
            env HOME="$home" USER=winlux-web LOGNAME=winlux-web DISPLAY="${DISPLAY:-:0}" LANG="$LANG" LANGUAGE="$LANGUAGE" LC_MESSAGES="$LC_MESSAGES" \
            /usr/bin/dbus-run-session $bus_config -- "$program" "$@"; then return 0; else
            notice="$run/action.txt"
            if test "$(id -u)" != 0; then mkdir -p "$run/control-$(id -u)" 2>/dev/null && notice="$run/control-$(id -u)/action.txt"; fi
            printf 'Le navigateur a échoué. Consultez le journal navigateur.\n' > "$notice" || true
            printf '1\n' > "$run/browser-chooser"
            return 1
        fi
    fi
    if test "$id" = tor; then program=$(tor_copy) || return 1; fi
    exec "$program" "$@"
}
# Tor Browser écrit son profil dans son propre dossier : il tourne depuis une
# copie personnelle (~/.local/share/winlux/tor-browser), refaite à chaque
# nouvelle version, jamais depuis /opt/winlux/browsers (lecture seule). Le
# profil (Browser/TorBrowser/Data) est gardé d'une version à l'autre.
tor_copy() {
    source_dir="$installed/tor/root/tor-browser"
    version=$(sed -n '1p' "$installed/tor/version" 2>/dev/null)
    test -d "$source_dir" && test -n "$version" || return 1
    copy="${HOME:?}/.local/share/winlux/tor-browser"
    if test "$(sed -n '1p' "$copy/.winlux-version" 2>/dev/null)" != "$version"; then
        mkdir -p "${copy%/*}"
        rm -r -f "${copy:?}.new"
        cp -R "$source_dir" "$copy.new"
        if test -d "$copy/Browser/TorBrowser/Data"; then
            rm -r -f "${copy:?}.new/Browser/TorBrowser/Data"
            mv "$copy/Browser/TorBrowser/Data" "$copy.new/Browser/TorBrowser/Data"
        fi
        rm -r -f "${copy:?}.old"
        if test -d "$copy"; then mv "$copy" "$copy.old"; fi
        mv "$copy.new" "$copy"
        rm -r -f "${copy:?}.old"
        printf '%s\n' "$version" > "$copy/.winlux-version"
    fi
    printf '%s/Browser/start-tor-browser\n' "$copy"
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
    --tor-copy) tor_copy ;;
    *) echo 'usage: winlux-browsers --status|--install ID|--default ID|--launch ID|--open [URL] (ID : brave chrome edge firefox opera operagx tor vivaldi)' >&2; exit 2 ;;
esac
