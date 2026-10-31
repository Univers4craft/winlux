#!/bin/sh
# Versioned system updates. Personal data and /etc are never in the payload.
set -eu

repository=${WINLUX_UPDATE_REPOSITORY:-Univers4craft/winlux}
target_root=${WINLUX_UPDATE_ROOT:-/}
test -d "$target_root" || { echo "racine de mise à jour absente" >&2; exit 1; }
target_root=$(CDPATH= cd -- "$target_root" && pwd -P)
if test -f "$target_root/usr/share/winlux/update-backend" && grep -qx apt "$target_root/usr/share/winlux/update-backend"; then
    test "$target_root" = / || { echo 'Exécutez le moteur APT dans le système cible.' >&2; exit 1; }
    exec /usr/bin/winlux-update-apt "$@"
fi
manifest_source=${WINLUX_UPDATE_MANIFEST:-https://github.com/$repository/releases/latest/download/winlux-update-manifest.txt}
state_dir="$target_root/var/lib/winlux"
config="$target_root/etc/winlux/update.conf"
work=
lock_owned=0
transaction=0
transaction_backup=
updater_tools=
recovered=0
download_cache=

# Progress follows actual phases/files, not a synthetic timer. The inhibitor
# is independent of the GUI so automatic updates are protected as well.
progress() {
    mkdir -p "$target_root/run/winlux" "$state_dir"
    printf 'phase=%s\npercent=%s\ndone=%s\ntotal=%s\nitem=%s\n' "$1" "$2" "$3" "$4" "$5" >"$target_root/run/winlux/update-progress.new.$$"
    mv -fT "$target_root/run/winlux/update-progress.new.$$" "$target_root/run/winlux/update-progress"
}
inhibit() { mkdir -p "$target_root/run/winlux"; printf '%s\n' "$$" >"$target_root/run/winlux/update-inhibit"; }
boot_id() { sed -n '1p' "$target_root/proc/sys/kernel/random/boot_id" 2>/dev/null || echo unknown; }

# State/configuration paths must not redirect a confined update onto the host.
for internal in var var/lib var/lib/winlux run run/winlux etc etc/winlux; do
    if test -L "$target_root/$internal"; then echo "répertoire d'état symbolique refusé : $internal" >&2; exit 1; fi
    if test -e "$target_root/$internal" && ! test -d "$target_root/$internal"; then echo "répertoire d'état invalide : $internal" >&2; exit 1; fi
done
if test -L "$config"; then echo "configuration de mise à jour symbolique refusée" >&2; exit 1; fi

fail() { echo "$*" >&2; exit 1; }
tool() {
    applet=$1; shift
    if test -n "$updater_tools"; then "$updater_tools" "$applet" "$@";
    elif test -x /bin/busybox; then /bin/busybox "$applet" "$@";
    else command "$applet" "$@"; fi
}
# Keep the running installer independent of files/libraries being replaced.
# The Winlux image supplies a statically linked BusyBox. Its private copy also
# remains usable for rollback if /bin/busybox is one of the updated files.
cp() { tool cp "$@"; }
cut() { tool cut "$@"; }
grep() { tool grep "$@"; }
mkdir() { tool mkdir "$@"; }
mktemp() { tool mktemp "$@"; }
mv() { tool mv "$@"; }
rm() { tool rm "$@"; }
sed() { tool sed "$@"; }
sha256sum() { tool sha256sum "$@"; }
tar() { tool tar "$@"; }
tr() { tool tr "$@"; }
wc() { tool wc "$@"; }
prepare_work() {
    # Le paquet compressé et sa racine décompressée dépassent facilement le
    # tmpfs /tmp d'une VM de 4 Gio. Préparer sur le disque de l'installation.
    if test -z "$work"; then
        mkdir -p "$state_dir"
        work=$(mktemp -d "$state_dir/update-work.XXXXXX") || fail "espace de préparation indisponible sur le disque : $state_dir"
    fi
}
require_space() {
    space_path=$1; required_kib=$2
    available_kib=$(tool df -Pk "$space_path" | tool awk 'END {print $4}')
    case "$available_kib" in ''|*[!0-9]*) fail "impossible de mesurer l'espace libre : $space_path" ;; esac
    test "$available_kib" -ge "$required_kib" || fail "Espace insuffisant sur $space_path : $required_kib Kio requis, $available_kib Kio disponibles. Libérez de l'espace puis réessayez."
}
prepare_tools() {
    prepare_work
    if test -x /bin/busybox && test -z "$updater_tools"; then
        cp /bin/busybox "$work/busybox"
        tool chmod 755 "$work/busybox"
        "$work/busybox" true || fail "outils privés de mise à jour inutilisables"
        updater_tools="$work/busybox"
    fi
}
value() {
    result=$(sed -n "s/^$1=//p" "$2")
    test -n "$result" && test "$(printf '%s\n' "$result" | wc -l)" -eq 1 || return 1
    printf '%s' "$result"
}
safe_name() { case "$1" in ''|*[!A-Za-z0-9._+-]*) return 1 ;; esac; }
safe_path() {
    case "$1" in ''|/*|*/|*//*|*[!A-Za-z0-9._+@%=/:-]*) return 1 ;; esac
    case "/$1/" in */../*|*/./*) return 1 ;; esac
    case "$1" in usr/*|lib/*|lib64/*|bin/*|opt/winlux/*|boot/*|init) return 0 ;; *) return 1 ;; esac
}
safe_link() {
    link_path=$1; link_target=$2
    case "$link_target" in ''|*[!A-Za-z0-9._+@%=/:-]*) return 1 ;; esac
    # Ubuntu masks obsolete systemd units with this harmless exact target.
    # Other links into /dev remain forbidden, as do masks outside unit dirs.
    if test "$link_target" = /dev/null; then
        case "${link_path%/*}:${link_path##*/}" in
            lib/systemd/system:*.service|usr/lib/systemd/system:*.service) return 0 ;;
            *) return 1 ;;
        esac
    fi
    case "$link_target" in /*) link_absolute=$link_target ;; *) link_absolute="${link_path%/*}/$link_target" ;; esac
    resolved=$(printf '%s\n' "$link_absolute" | tool awk -F/ '
        { n=0; for(i=1;i<=NF;i++) { if($i=="" || $i==".") continue;
          if($i=="..") { if(n==0) exit 1; n--; } else parts[++n]=$i; }
          for(i=1;i<=n;i++) printf "%s%s", i==1 ? "" : "/", parts[i]; print ""; }') || return 1
    safe_path "$resolved"
}
# Existing parent links may point within the managed tree (e.g. /lib -> usr/lib),
# but must not redirect installation into user data or outside a test root.
safe_parent() {
    relative=$1; prefix=; parent=${relative%/*}
    test "$parent" != "$relative" || return 0
    old_ifs=$IFS; IFS=/; set -- $parent; IFS=$old_ifs
    for part do
        prefix=${prefix:+$prefix/}$part
        existing="$target_root/$prefix"
        if test -e "$existing" || test -L "$existing"; then
            test -d "$existing" || fail "parent non répertoire : $prefix"
            real=$(tool readlink -f "$existing") || fail "parent illisible : $prefix"
            if test "$target_root" = /; then rel=${real#/};
            else case "$real" in "$target_root"/*) rel=${real#"$target_root"/} ;; *) fail "parent hors racine : $prefix" ;; esac; fi
            case "$rel" in
                bin|bin/*|usr|usr/*|lib|lib/*|lib64|lib64/*|opt|opt/winlux|opt/winlux/*|boot|boot/*) ;;
                etc|etc/winlux) test "$relative" = etc/winlux/update.conf || fail "parent hors système : $prefix" ;;
                *) fail "parent hors système : $prefix" ;;
            esac
        fi
    done
}
current_version() {
    if test -f "$target_root/usr/share/winlux/version"; then sed -n '1p' "$target_root/usr/share/winlux/version"; else echo none; fi
}
current_scope() {
    if test -f "$target_root/usr/share/winlux/update-scope"; then sed -n '1p' "$target_root/usr/share/winlux/update-scope"; else echo runtime; fi
}
up_to_date() { test "$(current_version)" = "$version" && { test "$scope" = runtime || test "$(current_scope)" = system; }; }
write_action() {
    mkdir -p "$target_root/run/winlux"
    requested_action=$1
    # A later runtime update must not hide a pending kernel reboot.
    if { test -f "$target_root/run/winlux/update-action" && grep -qx reboot "$target_root/run/winlux/update-action"; } || { test -f "$state_dir/pending-reboot"; }; then requested_action=reboot; fi
    if test "$requested_action" = reboot; then
        printf '%s\n' "$(boot_id)" >"$state_dir/pending-reboot.new"
        mv -fT "$state_dir/pending-reboot.new" "$state_dir/pending-reboot"
    fi
    printf '%s\n' "$requested_action" >"$target_root/run/winlux/update-action.new"
    mv -fT "$target_root/run/winlux/update-action.new" "$target_root/run/winlux/update-action"
}
restore_backup() {
    restoring=$1
    test -f "$restoring/restore.list" || return 1
    # Validate the entire backup before restoring its first file.
    while read -r kind digest mode path; do
        safe_path "$path" || return 1
        safe_parent "$path"
        case "$kind" in
            F) test -f "$restoring/files/$path" && ! test -L "$restoring/files/$path" || return 1
               test "$(sha256sum "$restoring/files/$path" | cut -d' ' -f1)" = "$digest" || return 1 ;;
            L) test -L "$restoring/files/$path" || return 1
               test "$(tool readlink "$restoring/files/$path")" = "$digest" || return 1
               safe_link "$path" "$digest" || return 1 ;;
            A) ;;
            *) return 1 ;;
        esac
    done <"$restoring/restore.list"
    while read -r kind digest mode path; do
        destination="$target_root/$path"
        if test "$kind" = A; then rm -f "$destination" || return 1;
        else
            mkdir -p "${destination%/*}" || return 1
            rm -f "$destination.winlux-restore.$$" || return 1
            cp -a "$restoring/files/$path" "$destination.winlux-restore.$$" || return 1
            mv -fT "$destination.winlux-restore.$$" "$destination" || return 1
        fi
    done <"$restoring/restore.list"
    if test -f "$restoring/installed-files.before"; then
        cp "$restoring/installed-files.before" "$state_dir/installed-files.new" || return 1
        mv -fT "$state_dir/installed-files.new" "$state_dir/installed-files" || return 1
    else rm -f "$state_dir/installed-files" || return 1; fi
}
cleanup() {
    status=$?; trap - EXIT HUP INT TERM
    if test "$status" -ne 0 && test "$lock_owned" -eq 1 && test -f "$target_root/run/winlux/update-inhibit"; then
        failure_phase=$(sed -n 's/^phase=//p' "$target_root/run/winlux/update-progress" 2>/dev/null || true)
        printf '%s\n' "$failure_phase" >"$target_root/run/winlux/update-error-phase" || true
    fi
    if test "$transaction" -eq 1; then
        progress restoring 0 0 0 'Restauration après interruption'
        echo "Installation interrompue : restauration des fichiers précédents." >&2
        if restore_backup "$transaction_backup"; then rm -f "$state_dir/update-transaction"; write_action reboot; tool sync;
        else echo "RESTAURATION INCOMPLÈTE : sauvegarde conservée dans $transaction_backup" >&2; fi
        status=1
    fi
    if test "$lock_owned" -eq 1; then
        if test "$status" -ne 0 && test -f "$target_root/run/winlux/update-inhibit"; then progress error 0 0 0 'Échec : consulter le journal et la sauvegarde'; fi
        rm -f "$target_root/run/winlux/update-inhibit" "$state_dir/update.lock/pid" "$state_dir/update.lock/boot-id"
        tool rmdir "$state_dir/update.lock" || true
    fi
    if test "$status" -eq 0 && test -n "$download_cache"; then rm -f "$download_cache"; fi
    if test -n "$work"; then rm -rf "$work"; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' HUP TERM
lock() {
    mkdir -p "$state_dir"
    if ! mkdir "$state_dir/update.lock" 2>/dev/null; then
        if test -d "$state_dir/update.lock" && ! test -L "$state_dir/update.lock" && test -f "$state_dir/update.lock/pid"; then
            previous_pid=$(sed -n '1p' "$state_dir/update.lock/pid")
            case "$previous_pid" in ''|*[!0-9]*) fail "verrou de mise à jour invalide" ;; esac
            previous_boot=$(sed -n '1p' "$state_dir/update.lock/boot-id" 2>/dev/null || true)
            if ! kill -0 "$previous_pid" 2>/dev/null || { test -n "$previous_boot" && test "$previous_boot" != unknown && test "$previous_boot" != "$(boot_id)"; }; then
                rm -f "$state_dir/update.lock/pid" "$state_dir/update.lock/boot-id"
                tool rmdir "$state_dir/update.lock" || fail "verrou périmé non récupérable"
                mkdir "$state_dir/update.lock" || fail "une mise à jour a démarré simultanément"
            else fail "une mise à jour est déjà en cours (PID $previous_pid)"; fi
        else fail "une mise à jour est déjà en cours (verrou $state_dir/update.lock)"; fi
    fi
    lock_owned=1; printf '%s\n' "$$" >"$state_dir/update.lock/pid"
    boot_id >"$state_dir/update.lock/boot-id"
    prepare_tools
    if test -f "$state_dir/update-transaction"; then
        interrupted=$(sed -n '1p' "$state_dir/update-transaction")
        case "$interrupted" in update-backup.*) safe_name "$interrupted" || fail "journal invalide" ;; *) fail "journal invalide" ;; esac
        echo "Récupération de la dernière installation interrompue."
        inhibit
        progress restoring 0 0 0 'Récupération de la transaction interrompue'
        restore_backup "$state_dir/$interrupted" || fail "récupération impossible ; sauvegarde $state_dir/$interrupted conservée"
        rm -f "$state_dir/update-transaction"
        recovered=1
        write_action reboot
        tool sync
    fi
}
download() {
    source=$1; destination=$2; accept=${3:-application/octet-stream}
    case "$source" in
        file://*) cp "${source#file://}" "$destination" ;;
        https://github.com/*|https://objects.githubusercontent.com/*)
            command -v curl >/dev/null 2>&1 || fail "curl est requis"
            set -- curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
                --connect-timeout 15 --max-time 300 --header "Accept: $accept" --output "$destination"
            "$@" "$source" || fail "téléchargement de la release publique GitHub impossible ; vérifiez le réseau" ;;
        *) fail "source de mise à jour refusée : $source" ;;
    esac
}
validate_manifest() {
    manifest=$1
    format=$(value format "$manifest") || fail "format absent du manifeste"
    case "$format" in
        1|2) ;;
        3) fail "Cette version utilise APT natif : installez la nouvelle image Winlux. La migration automatique de l'ancien système n'est pas prise en charge." ;;
        *) fail "format de manifeste inconnu" ;;
    esac
    version=$(value version "$manifest") || fail "version absente"
    asset=$(value asset "$manifest") || fail "actif absent"
    checksum=$(value sha256 "$manifest") || fail "SHA-256 absent"
    safe_name "$version" && safe_name "$asset" || fail "version ou nom d'actif invalide"
    case "$checksum" in *[!0-9a-f]*|'') fail "somme SHA-256 invalide" ;; esac
    test "${#checksum}" -eq 64 || fail "somme SHA-256 invalide"
    scope=runtime; action=restart-session; components=runtime,updater
    if test "$format" = 2; then
        scope=$(value scope "$manifest") || fail "portée absente"
        action=$(value action "$manifest") || fail "action absente"
        components=$(value components "$manifest") || fail "composants absents"
        case "$scope:$action" in system:reboot|runtime:restart-session) ;; *) fail "portée/action incohérente" ;; esac
        case "$components" in *[!A-Za-z0-9,._+-]*|'') fail "composants invalides" ;; esac
    fi
    notes_asset=$(value notes_asset "$manifest" || true)
    notes_checksum=$(value notes_sha256 "$manifest" || true)
    if test -n "$notes_asset$notes_checksum"; then
        safe_name "$notes_asset" || fail "nom des notes invalide"
        case "$notes_checksum" in *[!0-9a-f]*|'') fail "SHA-256 des notes invalide" ;; esac
        test "${#notes_checksum}" -eq 64 || fail "SHA-256 des notes invalide"
    fi
}
fetch_manifest() {
    manifest=$1
    download "$manifest_source" "$manifest"
    validate_manifest "$manifest"
    # Plain text only, bounded and bound to this manifest by its checksum.
    # Old releases without a notes asset remain supported.
    if test -n "$notes_asset"; then
        if test -n "${WINLUX_UPDATE_ASSET_DIR:-}"; then notes_source="file://$WINLUX_UPDATE_ASSET_DIR/$notes_asset";
        else notes_source="https://github.com/$repository/releases/download/$version/$notes_asset"; fi
        download "$notes_source" "$work/notes.txt"
        test "$(wc -c <"$work/notes.txt")" -le 24576 || fail "notes de version trop longues"
        test "$(sha256sum "$work/notes.txt" | cut -d' ' -f1)" = "$notes_checksum" || fail "SHA-256 des notes incorrect"
        cp "$work/notes.txt" "$state_dir/available-notes.new"
        mv -fT "$state_dir/available-notes.new" "$state_dir/available-notes"
    else
        printf 'Cette release ne fournit pas encore de notes détaillées.\n' >"$state_dir/available-notes"
    fi
    printf '%s\n' "$version" >"$state_dir/available-notes-version"
    cp "$manifest" "$state_dir/available-manifest.new"
    mv -fT "$state_dir/available-manifest.new" "$state_dir/available-manifest"
}
list_updates() {
    echo "Version installée : $(current_version)"
    if test -f "$target_root/run/winlux/update-action"; then echo "Action en attente : $(sed -n '1p' "$target_root/run/winlux/update-action")"; fi
    if ! test -f "$state_dir/available-manifest"; then echo "Aucune vérification disponible. Lancez une recherche de mises à jour (--check)."; return; fi
    validate_manifest "$state_dir/available-manifest"
    echo "Version disponible : $version"
    if up_to_date; then echo "Winlux est à jour."; fi
    echo "Portée : $scope"
    echo "Composants : $components"
    shown_bytes=$(value download_bytes "$state_dir/available-manifest" || true)
    case "$shown_bytes" in ''|*[!0-9]*) ;; *) echo "Taille à télécharger : $shown_bytes octets" ;; esac
    shown_kib=$(value unpacked_kib "$state_dir/available-manifest" || true)
    case "$shown_kib" in ''|*[!0-9]*) ;; *) echo "Taille décompressée : $shown_kib Kio (hors sauvegarde et marge)" ;; esac
    if test "$action" = reboot; then echo "Action après installation : redémarrage du système";
    else echo "Action après installation : redémarrage de la session graphique"; fi
    echo "Les données personnelles et la configuration sont conservées."
    printf '\nModifications détaillées — %s\n\n' "$version"
    if test "$(sed -n '1p' "$state_dir/available-notes-version" 2>/dev/null || true)" = "$version" && test -r "$state_dir/available-notes"; then
        cat "$state_dir/available-notes"
    else echo "Notes non disponibles. Lancez Rechercher pour les récupérer."; fi
}

mode=${1:-}
case "$mode" in
    --busy)
        pid=$(sed -n '1p' "$target_root/run/winlux/update-inhibit" 2>/dev/null || true)
        case "$pid" in ''|0|*[!0-9]*) exit 1 ;; esac
        kill -0 "$pid" 2>/dev/null; exit $? ;;
    --boot) test "$#" -eq 1 || fail 'arguments inattendus' ;;
    # Validation du démarrage : gérée seulement par le moteur APT.
    --confirm-boot) exit 0 ;;
    --boot-failed) exit 1 ;;
    --list) test "$#" -eq 1 || fail "arguments inattendus"; list_updates; exit 0 ;;
    --auto)
        test "$#" -le 2 || fail "arguments inattendus"
        case "${2:-status}" in
            status) if test -f "$config" && grep -qx 'auto=enabled' "$config"; then echo enabled; else echo disabled; fi; exit 0 ;;
            enable|disable)
                lock; safe_parent etc/winlux/update.conf; mkdir -p "${config%/*}"
                if test -f "$config"; then sed '/^auto=/d' "$config" >"$config.new"; else : >"$config.new"; fi
                if test "$2" = enable; then printf 'auto=enabled\n' >>"$config.new"; else printf 'auto=disabled\n' >>"$config.new"; fi
                tool chmod 644 "$config.new"; mv -fT "$config.new" "$config"
                echo "Mises à jour automatiques : $2"; exit 0 ;;
            *) fail "usage : winlux-update --auto enable|disable|status" ;;
        esac ;;
    --auto-run)
        test "$#" -eq 1 || fail "arguments inattendus"
        if ! test -f "$config" || ! grep -qx 'auto=enabled' "$config"; then exit 0; fi
        mode=--apply ;;
    --check|--apply|--rollback) test "$#" -eq 1 || fail "arguments inattendus" ;;
    *) echo "usage : winlux-update --check | --list | --apply | --rollback | --auto enable|disable|status | --auto-run" >&2; exit 2 ;;
esac
lock
if test "$mode" = --boot; then
    if test -f "$state_dir/pending-reboot"; then
        previous_boot=$(sed -n '1p' "$state_dir/pending-reboot")
        this_boot=$(boot_id)
        if test "$previous_boot" != unknown && test "$this_boot" != unknown && test "$previous_boot" != "$this_boot" && test "$recovered" = 0; then
            rm -f "$state_dir/pending-reboot" "$target_root/run/winlux/update-action"
            progress idle 100 0 0 'Mise à jour finalisée au démarrage'
            tool sync
        else printf 'reboot\n' >"$target_root/run/winlux/update-action"; fi
    fi
    if test "$recovered" = 1; then progress ready 100 0 0 'Système restauré : redémarrage conseillé';
    else progress idle 100 0 0 'Démarrage du système terminé'; fi
    exit 0
fi
if test "$mode" = --apply || test "$mode" = --rollback; then inhibit; progress checking 0 0 0 'Recherche et préparation'; fi
if test "$mode" = --rollback; then
    test -f "$state_dir/rollback-current" || fail "aucune sauvegarde Winlux"
    backup_name=$(sed -n '1p' "$state_dir/rollback-current")
    case "$backup_name" in update-backup.*) safe_name "$backup_name" || fail "nom de sauvegarde invalide" ;; *) fail "nom de sauvegarde invalide" ;; esac
    backup="$state_dir/$backup_name"
    action=$(value action "$backup/metadata") || fail "action de restauration absente"
    case "$action" in reboot|restart-session) ;; *) fail "action de restauration invalide" ;; esac
    progress restoring 0 0 0 'Restauration de la sauvegarde'
    restore_backup "$backup" || fail "sauvegarde incomplète ou restauration impossible"
    write_action "$action"
    progress ready 100 0 0 'Restauration terminée'
    echo "Retour à Winlux $(current_version) effectué. Action requise : $(sed -n '1p' "$target_root/run/winlux/update-action")."
    exit 0
fi

prepare_work
fetch_manifest "$work/manifest.txt"
list_updates
test "$mode" = --apply || exit 0
if up_to_date; then progress idle 100 0 0 'Winlux est déjà à jour'; exit 0; fi
download_bytes=$(value download_bytes "$work/manifest.txt" || true)
unpacked_kib=$(value unpacked_kib "$work/manifest.txt" || true)
if test -n "$download_bytes$unpacked_kib"; then
    for size in "$download_bytes" "$unpacked_kib"; do
        case "$size" in ''|*[!0-9]*) fail "taille invalide dans le manifeste" ;; esac
        test "${#size}" -le 12 || fail "taille excessive dans le manifeste"
    done
    # Prévoir archive, décompression, copie de restauration et marge.
    # L'estimation inclut les arbres gérés, jamais les documents personnels.
    backup_kib=0
    for tree in usr bin lib lib64 opt/winlux boot; do
        if test -e "$target_root/$tree"; then
            tree_kib=$(tool du -sk "$target_root/$tree" | tool cut -f1)
            backup_kib=$((backup_kib + tree_kib))
        fi
    done
    required_kib=$(( (download_bytes + 1023) / 1024 + unpacked_kib * 2 + backup_kib + 65536 ))
    require_space "$state_dir" "$required_kib"
    require_space "$target_root" "$required_kib"
    echo "Prévalidation espace : $required_kib Kio requis (archive, décompression, restauration et marge)."
else
    echo "Ancien manifeste sans tailles : contrôle avant décompression uniquement."
fi
if test -n "${WINLUX_UPDATE_ASSET_DIR:-}"; then asset_source="file://$WINLUX_UPDATE_ASSET_DIR/$asset";
else asset_source="https://github.com/$repository/releases/download/$version/$asset"; fi
bundle="$work/bundle.tar.gz"
progress downloading 0 0 0 "$asset"
progress_helper="$target_root/usr/bin/winlux-update-progress"
if test -x "$progress_helper" && command -v python3 >/dev/null 2>&1; then
    download_cache="$state_dir/download-$checksum.part"
    test ! -L "$download_cache" || fail 'cache de téléchargement symbolique refusé'
    test ! -e "$download_cache" || test -f "$download_cache" || fail 'cache de téléchargement invalide'
    bundle="$download_cache"
    WINLUX_PROGRESS_FILE="$target_root/run/winlux/update-progress" "$progress_helper" download "$asset_source" "${download_bytes:-0}" "$bundle" || fail 'échec du téléchargement'
else
    download "$asset_source" "$bundle"
fi
progress verifying 10 0 0 'Vérification SHA-256 et inventaire'
if test -x "$progress_helper" && command -v python3 >/dev/null 2>&1; then
    actual_checksum=$(WINLUX_PROGRESS_FILE="$target_root/run/winlux/update-progress" "$progress_helper" hash "$bundle" 0) || fail 'échec du calcul SHA-256'
else actual_checksum=$(sha256sum "$bundle" | cut -d' ' -f1); fi
if test "$actual_checksum" != "$checksum"; then
    if test -n "$download_cache"; then rm -f "$download_cache"; download_cache=; fi
    fail "échec de la vérification SHA-256 : le paquet téléchargé est incorrect"
fi
progress verifying -1 0 0 'Contrôle du contenu de l’archive'
tar -tzf "$bundle" >"$work/archive.list"
tar -tvzf "$bundle" >"$work/archive.types"
while IFS= read -r entry; do case "$entry" in -*|d*) ;; *) fail "liens ou fichiers spéciaux interdits dans l'archive" ;; esac; done <"$work/archive.types"
sed -e 's#^\./##' -e 's#/$##' -e '/^$/d' "$work/archive.list" >"$work/archive.names"
while IFS= read -r entry; do
    case "$entry" in ''|/*|*//*|*[!A-Za-z0-9._+@%=/:-]*) fail "chemin dangereux dans le paquet : $entry" ;; esac
    case "/$entry/" in */../*|*/./*) fail "chemin dangereux dans le paquet : $entry" ;; esac
    if test "$format" = 2; then case "$entry" in FILES|LINKS|META|payload|payload/*) ;; *) fail "entrée inattendue : $entry" ;; esac
    else case "$entry" in usr|usr/bin|usr/share|usr/share/winlux|usr/bin/winlux|usr/bin/winlux-update|usr/share/winlux/version) ;; *) fail "entrée runtime inattendue : $entry" ;; esac; fi
done <"$work/archive.names"
test -z "$(tool sort "$work/archive.names" | tool uniq -d)" || fail "entrées dupliquées dans le paquet"
mkdir -p "$work/extracted"
archive_kib=$(tool awk '$3 ~ /^[0-9]+$/ {bytes += $3} END {printf "%.0f", bytes/1024+1}' "$work/archive.types")
require_space "$state_dir" "$((archive_kib + 65536))"
progress extracting -1 0 0 'Décompression du paquet sur disque'
tar -xzf "$bundle" -C "$work/extracted"
progress verifying -1 0 0 'Vérification des fichiers et de leur emplacement'
if test "$format" = 2; then
    for key in format version scope action components; do
        test "$(value "$key" "$work/extracted/META")" = "$(value "$key" "$work/manifest.txt")" || fail "métadonnées du paquet incohérentes : $key"
    done
    payload="$work/extracted/payload"; files="$work/extracted/FILES"; links="$work/extracted/LINKS"
    test -f "$files" && test -f "$links" || fail "inventaire du paquet absent"
else
    payload="$work/extracted"; files="$work/legacy-files"; links="$work/legacy-links"
    : >"$files"; : >"$links"
    for path in usr/bin/winlux usr/bin/winlux-update usr/share/winlux/version; do
        test -f "$payload/$path" || fail "fichier runtime absent : $path"
        file_mode=644; case "$path" in usr/bin/*) file_mode=755 ;; esac
        printf '%s %s %s\n' "$(sha256sum "$payload/$path" | cut -d' ' -f1)" "$file_mode" "$path" >>"$files"
    done
fi
: >"$work/new-files"
: >"$work/file-checksums"
while read -r digest file_mode path extra; do
    test -z "${extra:-}" && safe_path "$path" || fail "inventaire fichiers invalide"
    case "$file_mode" in 644|755) ;; *) fail "droits de fichier refusés : $path" ;; esac
    test -f "$payload/$path" && ! test -L "$payload/$path" || fail "fichier absent : $path"
    printf '%s  %s\n' "$digest" "$path" >>"$work/file-checksums"
    printf '%s\n' "$path" >>"$work/new-files"
done <"$files"
# Une seule invocation pour tous les fichiers, au lieu de plusieurs processus
# par fichier. Les chemins ont été validés avant de changer de répertoire.
if ! (cd "$payload" && sha256sum -c "$work/file-checksums" >"$work/checksum-results"); then
    tool tail -n 20 "$work/checksum-results" >&2
    fail "un ou plusieurs fichiers du paquet sont altérés"
fi
tool find "$payload" -type f | sed "s#^$payload/##" | tool sort >"$work/actual-files"
tool sort "$work/new-files" >"$work/listed-files"
tool cmp -s "$work/actual-files" "$work/listed-files" || fail "l'inventaire ne correspond pas aux fichiers du paquet"
while read -r path link_target extra; do
    test -z "${extra:-}" && safe_path "$path" && safe_link "$path" "$link_target" || fail "lien dangereux dans l'inventaire : $path"
    printf '%s\n' "$path" >>"$work/new-files"
done <"$links"
test -z "$(tool sort "$work/new-files" | tool uniq -d)" || fail "fichiers/liens dupliqués"
test -x "$payload/usr/bin/winlux" && test -x "$payload/usr/bin/winlux-update" || fail "runtime ou outil de mise à jour absent"
test "$(sed -n '1p' "$payload/usr/share/winlux/version")" = "$version" || fail "version du paquet incohérente"
include_boot=0
if test "$scope" = system; then
    test -x "$payload/init" && test -f "$payload/opt/winlux/tests/gui_demo.exe" || fail "paquet système incomplet"
    case ",$components," in *,boot,*|*,kernel,*|*,drivers,*) include_boot=1 ;; esac
    if test "$include_boot" = 1; then
        test -f "$payload/boot/vmlinuz" && test -f "$payload/boot/winlux-initramfs.cpio.gz" || fail "démarrage système incomplet"
        tool find "$payload/lib/modules" -type f | grep -q . || fail "modules noyau absents"
    else
        if grep -Eq '^(boot|lib/modules)(/|$)' "$work/new-files"; then fail 'fichiers de démarrage non annoncés'; fi
    fi
fi
cp "$work/new-files" "$work/targets"
cp "$work/new-files" "$work/managed-files"
if test -f "$state_dir/installed-files"; then
    while IFS= read -r path; do
        safe_path "$path" || fail "inventaire installé invalide"
        if test "$scope" = system && test "$include_boot" = 0; then
            case "$path" in boot/*|lib/modules/*)
                printf '%s\n' "$path" >>"$work/managed-files"
                continue ;;
            esac
        fi
        if test "$scope" = system; then printf '%s\n' "$path" >>"$work/targets";
        else printf '%s\n' "$path" >>"$work/managed-files"; fi
    done <"$state_dir/installed-files"
fi
tool sort -u "$work/targets" >"$work/targets.sorted"
tool sort -u "$work/managed-files" >"$work/managed-files.sorted"
# A payload must never replace a parent directory with a file or link.
progress verifying -1 0 0 'Contrôle des chemins avant installation'
tool awk 'NR == FNR { files[$0] = 1; next }
    { path = $0; while (sub(/\/[^\/]*$/, "", path))
        if (path in files) { print "un fichier masque un parent : " path > "/dev/stderr"; exit 1 } }' \
    "$work/new-files" "$work/targets.sorted" || fail 'conflit de chemins dans le paquet'
while IFS= read -r path; do
    safe_parent "$path"
    if test -d "$target_root/$path" && ! test -L "$target_root/$path"; then fail "répertoire à la place d'un fichier : $path"; fi
done <"$work/targets.sorted"
# The manifest announces the maximum restart requirement. Decide the actual
# action from changed files; replacing only the desktop does not reboot Linux.
action=restart-session
if test "$scope" = system; then
    while IFS= read -r path; do
        case "$path" in
            boot/*|init|bin/*|lib/*|lib64/*|usr/lib/*|usr/bin/Xorg|usr/bin/udevadm|usr/share/X11/xorg.conf.d/*)
                unchanged=0
                if test -f "$payload/$path" && ! test -L "$target_root/$path" && test -f "$target_root/$path"; then
                    if tool cmp -s "$payload/$path" "$target_root/$path"; then unchanged=1; fi
                elif test -L "$target_root/$path"; then
                    new_target=$(tool awk -v wanted="$path" '$1 == wanted {print $2}' "$links")
                    if test -n "$new_target" && test "$(tool readlink "$target_root/$path")" = "$new_target"; then unchanged=1; fi
                elif ! test -e "$target_root/$path" && ! grep -Fxq "$path" "$work/new-files"; then unchanged=1;
                fi
                if test "$unchanged" -eq 0; then action=reboot; break; fi ;;
        esac
    done <"$work/targets.sorted"
fi
# Revérifier après téléchargement et validation : d'autres applications
# peuvent avoir consommé de l'espace depuis le premier contrôle.
remaining_kib=$(tool du -sk "$payload" | tool cut -f1)
backup_kib=0
for tree in usr bin lib lib64 opt/winlux boot; do
    if test -e "$target_root/$tree"; then
        tree_kib=$(tool du -sk "$target_root/$tree" | tool cut -f1)
        backup_kib=$((backup_kib + tree_kib))
    fi
done
require_space "$state_dir" "$((remaining_kib + backup_kib + 65536))"
require_space "$target_root" "$((remaining_kib + 65536))"
transaction_backup=$(mktemp -d "$state_dir/update-backup.XXXXXX")
tool chmod 700 "$transaction_backup"
mkdir -p "$transaction_backup/files"
: >"$transaction_backup/restore.list"
printf 'action=%s\n' "$action" >"$transaction_backup/metadata"
if test -f "$state_dir/installed-files"; then cp "$state_dir/installed-files" "$transaction_backup/installed-files.before"; fi
progress_total=$(wc -l <"$work/targets.sorted"); progress_done=0
while IFS= read -r path; do
    progress backing-up "$((20 + progress_done * 20 / (progress_total + 1)))" "$progress_done" "$progress_total" "$path"
    original="$target_root/$path"
    if test -L "$original"; then
        link_target=$(tool readlink "$original")
        safe_link "$path" "$link_target" || fail "lien installé non restaurable : $path"
        mkdir -p "$transaction_backup/files/$(tool dirname "$path")"
        cp -a "$original" "$transaction_backup/files/$path"
        printf 'L %s - %s\n' "$link_target" "$path" >>"$transaction_backup/restore.list"
    elif test -f "$original"; then
        mkdir -p "$transaction_backup/files/$(tool dirname "$path")"
        cp -a "$original" "$transaction_backup/files/$path"
        printf 'F %s - %s\n' "$(sha256sum "$original" | cut -d' ' -f1)" "$path" >>"$transaction_backup/restore.list"
    elif test -e "$original"; then fail "type installé non pris en charge : $path";
    else printf 'A - - %s\n' "$path" >>"$transaction_backup/restore.list"; fi
    progress_done=$((progress_done + 1))
done <"$work/targets.sorted"
printf '%s\n' "${transaction_backup##*/}" >"$state_dir/update-transaction.new"
mv -fT "$state_dir/update-transaction.new" "$state_dir/update-transaction"
tool sync
transaction=1
progress_total=$(( $(wc -l <"$files") + $(wc -l <"$links") )); progress_done=0
while read -r digest file_mode path; do
    progress installing "$((40 + progress_done * 55 / (progress_total + 1)))" "$progress_done" "$progress_total" "$path"
    destination="$target_root/$path"
    mkdir -p "${destination%/*}"
    rm -f "$destination.winlux-new.$$"
    cp "$payload/$path" "$destination.winlux-new.$$"
    tool chmod "$file_mode" "$destination.winlux-new.$$"
    mv -fT "$destination.winlux-new.$$" "$destination"
    progress_done=$((progress_done + 1))
done <"$files"
while read -r path link_target; do
    progress installing "$((40 + progress_done * 55 / (progress_total + 1)))" "$progress_done" "$progress_total" "$path"
    destination="$target_root/$path"
    mkdir -p "${destination%/*}"
    rm -f "$destination.winlux-new.$$"
    tool ln -s "$link_target" "$destination.winlux-new.$$"
    mv -fT "$destination.winlux-new.$$" "$destination"
    progress_done=$((progress_done + 1))
done <"$links"
if test "$scope" = system; then
    while IFS= read -r path; do if ! grep -Fxq "$path" "$work/new-files"; then rm -f "$target_root/$path"; fi; done <"$work/targets.sorted"
fi
cp "$work/managed-files.sorted" "$state_dir/installed-files.new"
mv -fT "$state_dir/installed-files.new" "$state_dir/installed-files"
printf '%s\n' "${transaction_backup##*/}" >"$state_dir/rollback-current.new"
mv -fT "$state_dir/rollback-current.new" "$state_dir/rollback-current"
write_action "$action"
progress finalizing 98 "$progress_done" "$progress_total" 'Synchronisation des fichiers sur disque'
tool sync
rm -f "$state_dir/update-transaction"
transaction=0
tool sync
progress ready 100 "$progress_done" "$progress_total" 'Installation terminée : redémarrage conseillé'
echo "Winlux $version installé ($scope). Sauvegarde complète disponible avec --rollback."
echo "Action requise : $(sed -n '1p' "$target_root/run/winlux/update-action"). Les redémarrages du système restent soumis à votre accord."
