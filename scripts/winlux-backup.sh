#!/bin/sh
# winlux-backup : historique des fichiers Winlux (sauvegardes incrémentales).
#
# Chaque utilisateur choisit ses dossiers, une destination (disque externe,
# dossier ou partage réseau SMB/NFS), une fréquence et une durée de
# conservation. Chaque sauvegarde est un instantané complet et navigable :
# les fichiers inchangés depuis l'instantané précédent sont des liens durs,
# seuls les fichiers nouveaux ou modifiés occupent de la place.
#
# Les points de restauration système (winlux-restore) protègent la
# configuration ; l'historique des fichiers protège les données personnelles.
# Les deux ne s'exécutent jamais pendant une mise à jour Winlux.
#
# Usage :
#   winlux-backup status UTILISATEUR
#   winlux-backup set UTILISATEUR CLE VALEUR      (ENABLED, DESTINATION, FREQUENCY, KEEP_DAYS)
#   winlux-backup add UTILISATEUR DOSSIER | remove UTILISATEUR DOSSIER
#   winlux-backup share UTILISATEUR URL [COMPTE [MOT_DE_PASSE|-]]   (- : lu sur l'entrée standard)
#   winlux-backup run UTILISATEUR | auto-run | prune UTILISATEUR
#   winlux-backup list UTILISATEUR
#   winlux-backup browse UTILISATEUR INSTANTANE DOSSIER
#   winlux-backup versions UTILISATEUR CHEMIN
#   winlux-backup restore UTILISATEUR INSTANTANE CHEMIN [replace|safe|keep-both|DOSSIER_CIBLE]
#       CHEMIN « / » : tout l'instantané (replace, safe ou DOSSIER_CIBLE). safe ne remplace
#       jamais un fichier modifié depuis : la copie sauvegardée arrive à côté.
#   winlux-backup request UTILISATEUR   (lit la commande dans .../request, une valeur par ligne)
#   winlux-backup drives
set -eu
umask 077
root=${WINLUX_CONTROL_ROOT:-}
config="$root/etc/winlux/backup"
run="$root/run/winlux/backup"
mkdir -p "$config" "$run"
# Le bureau, sous le compte, lit drives.txt et son propre dossier d'état.
if test -z "$root" && test "$(id -u)" = 0; then chmod 755 "$run"; fi

fail() { printf 'winlux-backup : %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
now() { date +%s; }

check_user() {
    case "$1" in ''|*[!A-Za-z0-9._-]*|.*) fail "nom d'utilisateur invalide : $1" ;; esac
}
user_home() {
    if test -n "${WINLUX_BACKUP_HOME:-}"; then printf '%s\n' "$WINLUX_BACKUP_HOME"; return; fi
    home=
    if test -r "$root/etc/passwd"; then
        home=$(awk -F: -v u="$1" '$1 == u { print $6; exit }' "$root/etc/passwd")
    fi
    if test -z "$home"; then
        if test "$1" = root; then home=/root; else home="/home/$1"; fi
    fi
    printf '%s%s\n' "$root" "$home"
}
# Root lit les dossiers à sauvegarder : un compte ne protège que ce qui est
# à lui (dossier personnel, ses disques sous /media/<compte>), jamais /etc
# ni le dossier d'un autre, même par un lien symbolique.
in_scope() {
    test "$1" != root || return 0
    is_home=$(user_home "$1")
    case "$2/" in "${is_home#"$root"}"/*|/media/"$1"/*|/run/media/"$1"/*) return 0 ;; esac
    return 1
}
real_in_scope() {
    test "$1" != root || return 0
    ris_real=$(readlink -f "$root$2" 2>/dev/null) || return 1
    ris_home=$(readlink -f "$(user_home "$1")" 2>/dev/null) || ris_home=$(user_home "$1")
    case "$ris_real/" in "$ris_home"/*|"$root"/media/"$1"/*|"$root"/run/media/"$1"/*) return 0 ;; esac
    return 1
}
conf_file() { printf '%s/%s.conf\n' "$config" "$1"; }
cred_file() { printf '%s/%s.cred\n' "$config" "$1"; }
# Dossier d'état d'un compte : le bureau tourne sous ce compte et passe par
# le relais root (winlux-root) ; il doit lire les résultats et déposer ses
# requêtes. Dossier root:<groupe du compte> 0770, fichiers 0640.
account_gid() {
    test -r "$root/etc/passwd" || return 0
    awk -F: -v u="$1" '$1 == u { print $4; exit }' "$root/etc/passwd"
}
out_dir() {
    d="$run/$1"
    mkdir -p "$d"
    if test -z "$root" && test "$(id -u)" = 0 && test "$1" != root; then
        gid=$(account_gid "$1")
        if test -n "$gid"; then chgrp "$gid" "$d" 2>/dev/null && chmod 770 "$d"; fi
    fi
    printf '%s\n' "$d"
}
get() { test -r "$1" && sed -n "s/^$2=//p" "$1" | head -n 1 || true; }
put() {
    file=$1 key=$2 new_value=$3
    tmp="$file.$$"
    if test -r "$file"; then grep -v "^$key=" "$file" > "$tmp" || true; else : > "$tmp"; fi
    printf '%s=%s\n' "$key" "$new_value" >> "$tmp"
    mv "$tmp" "$file"
}
# Fichier temporaire créé par mktemp (jamais un nom prévisible : le compte
# peut écrire dans son dossier d'état), puis renommé d'un coup.
staging() { mktemp "$1.XXXXXX"; }
publish() {
    chmod 640 "$1"
    chgrp "$(stat -c %g "${2%/*}")" "$1" 2>/dev/null || true
    mv -f "$1" "$2"
}

# Configuration par défaut : comme l'historique des fichiers de Windows,
# les bibliothèques personnelles sont protégées dès l'activation.
ensure_conf() {
    f=$(conf_file "$1")
    test -f "$f" && return 0
    home=$(user_home "$1")
    {
        printf 'ENABLED=0\nDESTINATION=\nFREQUENCY=60\nKEEP_DAYS=0\n'
        for sub in Documents Images Bureau Musique Vidéos; do printf 'FOLDER=%s/%s\n' "${home#"$root"}" "$sub"; done
    } > "$f.$$"
    mv "$f.$$" "$f"
}
folders() { sed -n 's/^FOLDER=//p' "$(conf_file "$1")"; }

is_network() {
    case "$1" in smb://*|//*|nfs://*|*:/*) return 0 ;; *) return 1 ;; esac
}

# Répertoire racine de l'historique sur la destination ; monte le partage
# réseau si nécessaire. Rien n'est créé sur un disque absent.
mount_point() { printf '%s/mnt-%s\n' "$run" "$1"; }
open_destination() {
    user=$1
    dest=$(get "$(conf_file "$user")" DESTINATION)
    test -n "$dest" || return 1
    if is_network "$dest"; then
        mp=$(mount_point "$user")
        mkdir -p "$mp"
        if ! grep -q " $mp " "$root/proc/mounts" 2>/dev/null; then
            have mount || return 1
            case "$dest" in
                smb://*|//*)
                    share="//${dest#smb://}"; share=$(printf '%s' "$share" | sed 's|^///*|//|')
                    opts="vers=3.0"
                    if test -r "$(cred_file "$user")"; then opts="$opts,credentials=$(cred_file "$user")"; else opts="$opts,guest"; fi
                    mount -t cifs "$share" "$mp" -o "$opts" 2>/dev/null || return 1 ;;
                nfs://*) host=${dest#nfs://}; mount -t nfs "${host%%/*}:/${host#*/}" "$mp" 2>/dev/null || return 1 ;;
                *) mount -t nfs "$dest" "$mp" 2>/dev/null || return 1 ;;
            esac
        fi
        base=$mp
    else
        base="$root$dest"
        test -d "$base" || return 1
    fi
    test -w "$base" || return 1
    host=$(cat "$root/etc/hostname" 2>/dev/null || hostname 2>/dev/null || printf 'winlux')
    printf '%s/WinluxFileHistory/%s/%s\n' "$base" "$host" "$user"
}
close_destination() {
    mp=$(mount_point "$1")
    if grep -q " $mp " "$root/proc/mounts" 2>/dev/null; then umount "$mp" 2>/dev/null || true; fi
}

snapshots() {
    # Instantanés complets, du plus ancien au plus récent.
    test -d "$1" || return 0
    for s in "$1"/*; do
        test -f "$s/.winlux-snapshot" && basename "$s"
    done | sort
}
latest() { snapshots "$1" | tail -n 1; }

copy_entry() {
    # copy_entry SOURCE CIBLE PRECEDENT : lien dur si le fichier est identique
    # (taille et date) dans l'instantané précédent, copie sinon.
    ce_src=$1 ce_dst=$2 ce_prev=$3
    if test -n "$ce_prev" && test -f "$ce_prev" && test ! -L "$ce_prev" &&
        test "$(stat -c '%s %Y' "$ce_src")" = "$(stat -c '%s %Y' "$ce_prev")"; then
        ln "$ce_prev" "$ce_dst" 2>/dev/null && return 0
    fi
    cp -p "$ce_src" "$ce_dst"
}

backup_folder() {
    folder=$1 snap=$2 prev=$3
    src="$root$folder"
    test -d "$src" || { printf 'Dossier absent ignoré : %s\n' "$folder"; return 0; }
    real_in_scope "$user" "$folder" || { printf 'Dossier hors du compte ignoré : %s\n' "$folder"; return 0; }
    mkdir -p "$snap$folder"
    if have rsync && test -z "${WINLUX_BACKUP_NO_RSYNC:-}"; then
        if test -n "$prev" && test -d "$prev$folder"; then
            rsync -a --link-dest="$prev$folder/" "$src/" "$snap$folder/"
        else
            rsync -a "$src/" "$snap$folder/"
        fi
        return
    fi
    # Repli portable (BusyBox) : parcours fichier par fichier.
    find "$src" -mindepth 1 | while IFS= read -r path; do
        rel=${path#"$src"}
        case "$rel" in *'
'*) continue ;; esac
        if test -L "$path"; then cp -P "$path" "$snap$folder$rel" 2>/dev/null || true
        elif test -d "$path"; then mkdir -p "$snap$folder$rel"
        elif test -f "$path"; then
            p=
            test -z "$prev" || p="$prev$folder$rel"
            copy_entry "$path" "$snap$folder$rel" "$p" || printf 'Copie impossible : %s\n' "$folder$rel"
        fi
    done
}

do_run() {
    user=$1
    ensure_conf "$user"
    if have winlux-update && winlux-update --busy 2>/dev/null; then fail "mise à jour en cours : sauvegarde reportée"; fi
    lock="$run/$user.lock"
    mkdir "$lock" 2>/dev/null || { printf 'winlux-backup : une sauvegarde est déjà en cours\n' >&2; exit 75; }
    trap 'rmdir "$lock" 2>/dev/null || true; close_destination "$user"' EXIT HUP INT TERM
    put "$(conf_file "$user")" LAST_ATTEMPT "$(now)"
    history=$(open_destination "$user") || { put "$(conf_file "$user")" LAST_RESULT unavailable; fail "destination indisponible : branchez le disque ou vérifiez le partage"; }
    mkdir -p "$history"
    prev_name=$(latest "$history")
    prev=
    test -z "$prev_name" || prev="$history/$prev_name"
    name=$(date +%Y%m%d-%H%M%S)
    if test -e "$history/$name"; then sleep 1; name=$(date +%Y%m%d-%H%M%S); fi
    partial="$history/.$name.partial"
    rm -rf "$partial"
    mkdir -p "$partial"
    folders "$user" > "$run/$user.folders.$$"
    while IFS= read -r folder; do
        test -n "$folder" || continue
        backup_folder "$folder" "$partial" "$prev"
    done < "$run/$user.folders.$$"
    rm -f "$run/$user.folders.$$"
    files=$(find "$partial" -type f | wc -l | tr -d ' ')
    {
        printf 'epoch=%s\n' "$(now)"
        printf 'date=%s\n' "$(date '+%d/%m/%Y %H:%M')"
        printf 'files=%s\n' "$files"
        printf 'user=%s\n' "$user"
    } > "$partial/.winlux-snapshot"
    # Un instantané n'apparaît qu'une fois complet : une coupure laisse
    # seulement un dossier .partial ignoré et nettoyé au passage suivant.
    mv "$partial" "$history/$name"
    put "$(conf_file "$user")" LAST_RUN "$(now)"
    put "$(conf_file "$user")" LAST_RESULT ok
    # Mesuré une fois par sauvegarde : du sur tout l'historique est coûteux.
    used=$(du -sk "$history" 2>/dev/null | awk '{ print $1 }')
    test -z "$used" || put "$(conf_file "$user")" USED_KIB "$used"
    printf 'Sauvegarde %s terminée : %s fichiers.\n' "$name" "$files"
    do_prune "$user" "$history"
}

do_prune() {
    user=$1 history=$2
    keep=$(get "$(conf_file "$user")" KEEP_DAYS)
    case "$keep" in ''|0|*[!0-9]*) return 0 ;; esac
    limit=$(( $(now) - keep * 86400 ))
    newest=$(latest "$history")
    for s in $(snapshots "$history"); do
        test "$s" != "$newest" || continue
        epoch=$(get "$history/$s/.winlux-snapshot" epoch)
        case "$epoch" in ''|*[!0-9]*) continue ;; esac
        if test "$epoch" -lt "$limit"; then rm -rf "${history:?}/$s"; printf 'Instantané expiré supprimé : %s\n' "$s"; fi
    done
    for p in "$history"/.*.partial; do if test -d "$p"; then rm -rf "$p"; fi; done
    return 0
}

human_size() {
    awk -v k="$1" 'BEGIN { if (k >= 1048576) printf "%.1f Go", k/1048576; else if (k >= 1024) printf "%.1f Mo", k/1024; else printf "%d Ko", k }'
}

do_status() {
    user=$1
    ensure_conf "$user"
    f=$(conf_file "$user")
    out="$(out_dir "$user")/status.txt"
    tmp=$(staging "$out")
    {
        printf 'enabled=%s\n' "$(get "$f" ENABLED)"
        printf 'destination=%s\n' "$(get "$f" DESTINATION)"
        printf 'frequency=%s\n' "$(get "$f" FREQUENCY)"
        printf 'keep_days=%s\n' "$(get "$f" KEEP_DAYS)"
        printf 'last_run=%s\n' "$(get "$f" LAST_RUN)"
        last=$(get "$f" LAST_RUN)
        if test -n "$last"; then printf 'last_run_text=%s\n' "$(date -d "@$last" '+%d/%m/%Y %H:%M' 2>/dev/null || printf '%s' "$last")"; fi
        printf 'last_result=%s\n' "$(get "$f" LAST_RESULT)"
        used=$(get "$f" USED_KIB)
        test -z "$used" || printf 'used=%s\n' "$(human_size "$used")"
        printf 'running=%s\n' "$(test -d "$run/$user.lock" && echo 1 || echo 0)"
        dest=$(get "$f" DESTINATION)
        network=0; ! is_network "$dest" || network=1
        printf 'network=%s\n' "$network"
        available=0
        if test -n "$dest"; then
            if test "$network" = 1; then
                # Ne pas monter un partage à chaque rafraîchissement.
                available=$(test "$(get "$f" LAST_RESULT)" != unavailable && echo 1 || echo 0)
            elif test -d "$root$dest" && test -w "$root$dest"; then
                available=1
                kib=$(df -Pk "$root$dest" 2>/dev/null | awk 'NR == 2 { print $4 }')
                test -z "$kib" || printf 'free=%s\n' "$(human_size "$kib")"
                home=$(user_home "$user")
                if test "$(stat -c %d "$root$dest")" = "$(stat -c %d "$home" 2>/dev/null || echo x)"; then printf 'same_disk=1\n'; fi
            fi
        fi
        printf 'available=%s\n' "$available"
        count=0
        while IFS= read -r folder; do
            test -n "$folder" || continue
            count=$((count + 1))
            present=1; test -d "$root$folder" || present=0
            printf 'folder=%s|%s\n' "$present" "$folder"
        done <<EOF
$(folders "$user")
EOF
        printf 'folders=%s\n' "$count"
        printf 'home=%s\n' "$(user_home "$user" | sed "s|^$root||")"
    } > "$tmp"
    publish "$tmp" "$out"
}

do_list() {
    user=$1
    out="$(out_dir "$user")/snapshots.txt"
    tmp=$(staging "$out")
    history=$(open_destination "$user") || { publish "$tmp" "$out"; fail "destination indisponible"; }
    for s in $(snapshots "$history"); do
        printf '%s|%s|%s\n' "$s" "$(get "$history/$s/.winlux-snapshot" date)" "$(get "$history/$s/.winlux-snapshot" files)"
    done > "$tmp"
    publish "$tmp" "$out"
}

check_snapshot() {
    case "$1" in [0-9]*-[0-9]*) ;; *) fail "instantané invalide : $1" ;; esac
    case "$1" in *[!0-9-]*) fail "instantané invalide : $1" ;; esac
}
check_path() {
    case "$1" in /*) ;; *) fail "chemin absolu attendu : $1" ;; esac
    case "/$1/" in */../*|*/./*) fail "chemin invalide : $1" ;; esac
}

do_browse() {
    user=$1 snap=$2 dir=$3
    check_snapshot "$snap"; check_path "$dir"
    out="$(out_dir "$user")/browse.txt"
    tmp=$(staging "$out")
    history=$(open_destination "$user") || fail "destination indisponible"
    base="$history/$snap"
    test -d "$base" || fail "instantané introuvable : $snap"
    {
        printf 'snapshot=%s\n' "$snap"
        printf 'date=%s\n' "$(get "$base/.winlux-snapshot" date)"
        printf 'dir=%s\n' "$dir"
        if test "$dir" = /; then
            # Racine : les dossiers protégés de cet instantané.
            folders "$user" | while IFS= read -r folder; do
                if test -d "$base$folder"; then printf 'entry=d|%s|0|%s\n' "$folder" "$(stat -c %Y "$base$folder")"; fi
            done
        elif test -d "$base$dir"; then
            for e in "$base$dir"/* "$base$dir"/.[!.]*; do
                test -e "$e" || test -L "$e" || continue
                name=${e##*/}
                test "$name" != .winlux-snapshot || continue
                kind=f; test ! -d "$e" || kind=d
                printf 'entry=%s|%s|%s|%s\n' "$kind" "$name" "$(stat -c %s "$e")" "$(stat -c %Y "$e")"
            done
        fi
    } > "$tmp"
    publish "$tmp" "$out"
}

do_versions() {
    user=$1 path=$2
    check_path "$path"
    out="$(out_dir "$user")/versions.txt"
    tmp=$(staging "$out")
    history=$(open_destination "$user") || fail "destination indisponible"
    last=
    {
        printf 'path=%s\n' "$path"
        for s in $(snapshots "$history"); do
            f="$history/$s$path"
            test -e "$f" || continue
            sig=$(stat -c '%s %Y' "$f")
            # Une version n'est listée que si le contenu a changé.
            test "$sig" != "$last" || continue
            last=$sig
            printf 'version=%s|%s|%s\n' "$s" "$(get "$history/$s/.winlux-snapshot" date)" "$(stat -c %s "$f")"
        done
    } > "$tmp"
    publish "$tmp" "$out"
}

# Nom « fichier (restauré AAAAMMJJ-HHMMSS).ext » à côté de l'original.
restored_name() {
    rn_dir=${1%/*} rn_name=${1##*/}
    case "$rn_name" in ?*.*) rn_stem=${rn_name%.*} rn_ext=".${rn_name##*.}" ;; *) rn_stem=$rn_name rn_ext= ;; esac
    printf '%s/%s (restauré %s)%s\n' "$rn_dir" "$rn_stem" "$2" "$rn_ext"
}

# Écritures de la restauration sous le compte, jamais en root : un lien
# symbolique planté dans le dossier personnel ne mène nulle part ailleurs
# que là où le compte peut déjà écrire. Root lit seulement la sauvegarde.
as_user() {
    if test -n "${AS_UID:-}"; then setpriv --reuid="$AS_UID" --regid="$AS_GID" --init-groups "$@"; else "$@"; fi
}
set_restore_identity() {
    AS_UID= AS_GID=
    test -z "$root" && test "$(id -u)" = 0 && test "$1" != root || return 0
    AS_UID=$(awk -F: -v u="$1" '$1 == u { print $3; exit }' /etc/passwd)
    AS_GID=$(account_gid "$1")
    test -n "$AS_UID" && test -n "$AS_GID" || fail "compte inconnu : $1"
    have setpriv || fail "setpriv absent : restauration impossible sans changer d'identité"
}

# Copie d'un élément de l'instantané vers la cible selon le mode :
#   replace : la version sauvegardée remplace l'actuelle ;
#   safe    : ce qui manque est remis, ce qui est identique est laissé, et un
#             fichier modifié depuis garde sa version actuelle : la version
#             sauvegardée arrive à côté (« … (restauré …) »). Rien n'est écrasé.
restore_file() {
    rf_src=$1 rf_dst=$2 rf_mode=$3 rf_snap=$4
    if test "$rf_mode" = safe && as_user test -e "$rf_dst" -o -L "$rf_dst"; then
        if test ! -L "$rf_src" && as_user test -f "$rf_dst" -a ! -L "$rf_dst" && as_user cmp -s - "$rf_dst" < "$rf_src"; then
            return 0
        fi
        rf_dst=$(restored_name "$rf_dst" "$rf_snap")
    fi
    # Un dossier à la place du fichier n'est jamais supprimé.
    if as_user test -d "$rf_dst" -a ! -L "$rf_dst"; then printf 'Dossier conservé : %s\n' "${rf_dst#"$root"}" >&2; return 0; fi
    rf_tmp="${rf_dst%/*}/.winlux-restore.$$"
    as_user rm -f "$rf_tmp"
    if test -L "$rf_src"; then
        as_user ln -s "$(readlink "$rf_src")" "$rf_tmp" || { printf 'Copie impossible : %s\n' "${rf_dst#"$root"}" >&2; return 0; }
    elif test -f "$rf_src"; then
        rf_when=$(date -d "@$(stat -c %Y "$rf_src")" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || true)
        if ! as_user sh -c 'cat > "$1" && chmod "$2" "$1" && { test -z "$3" || touch -d "$3" "$1" || true; }' sh "$rf_tmp" \
            "$(stat -c %a "$rf_src")" "$rf_when" < "$rf_src"; then
            as_user rm -f "$rf_tmp"; printf 'Copie impossible : %s\n' "${rf_dst#"$root"}" >&2; return 0
        fi
    else
        return 0
    fi
    as_user mv -f "$rf_tmp" "$rf_dst"
}

restore_tree() {
    rt_src=$1 rt_dst=$2 rt_mode=$3 rt_snap=$4
    if test -d "$rt_src" && test ! -L "$rt_src"; then
        as_user mkdir -p "$rt_dst" || { printf 'Dossier impossible à créer : %s\n' "${rt_dst#"$root"}" >&2; return 0; }
        # Copie fusionnée : les fichiers récents absents de l'instantané restent.
        (cd "$rt_src" && find . -mindepth 1) | while IFS= read -r rel; do
            rel=${rel#./}
            test "$rel" != .winlux-snapshot || continue
            if test -d "$rt_src/$rel" && test ! -L "$rt_src/$rel"; then as_user mkdir -p "$rt_dst/$rel" || true
            else restore_file "$rt_src/$rel" "$rt_dst/$rel" "$rt_mode" "$rt_snap"; fi
        done
    else
        as_user mkdir -p "$(dirname "$rt_dst")" || true
        restore_file "$rt_src" "$rt_dst" "$rt_mode" "$rt_snap"
    fi
}

do_restore() {
    user=$1 snap=$2 path=$3 mode=${4:-replace}
    check_snapshot "$snap"; check_path "$path"
    history=$(open_destination "$user") || fail "destination indisponible"
    test -d "$history/$snap" || fail "instantané introuvable : $snap"
    set_restore_identity "$user"
    if test "$path" = /; then
        # Tout l'instantané : chaque dossier protégé, à sa place.
        # Vers un autre dossier : un sous-dossier par dossier sauvegardé.
        elsewhere=
        case "$mode" in replace|safe) ;; /*) check_path "$mode"; elsewhere=$mode; mode=safe ;; *) fail "mode de restauration inconnu : $mode" ;; esac
        folders "$user" > "$run/$user.restore.$$"
        while IFS= read -r folder; do
            test -n "$folder" && test -d "$history/$snap$folder" || continue
            if test -n "$elsewhere"; then dest="$root$elsewhere/${folder##*/}"; else dest="$root$folder"; fi
            restore_tree "$history/$snap$folder" "$dest" "$mode" "$snap"
        done < "$run/$user.restore.$$"
        rm -f "$run/$user.restore.$$"
        printf 'Restauré : instantané %s\n' "$snap"
        return 0
    fi
    src="$history/$snap$path"
    test -e "$src" || fail "élément absent de l'instantané : $path"
    # On ne restaure que dans les dossiers protégés de l'utilisateur.
    allowed=0
    while IFS= read -r folder; do
        case "$path/" in "$folder"/*) allowed=1 ;; esac
    done <<EOF
$(folders "$user")
EOF
    test "$allowed" = 1 || fail "chemin hors des dossiers sauvegardés : $path"
    case "$mode" in
        replace|safe) target="$root$path" ;;
        keep-both) target="$root$(restored_name "$path" "$snap")"; mode=replace ;;
        /*) check_path "$mode"; target="$root$mode/${path##*/}"; mode=safe ;;
        *) fail "mode de restauration inconnu : $mode" ;;
    esac
    restore_tree "$src" "$target" "$mode" "$snap"
    printf 'Restauré : %s (%s)\n' "${target#"$root"}" "$snap"
}

do_drives() {
    out="$run/drives.txt"
    tmp=$(staging "$out")
    {
        awk '$2 ~ "^/(media|mnt|run/media)/" && $2 !~ "/winlux/" { print $2 }' "$root/proc/mounts" 2>/dev/null | while IFS= read -r mp; do
            mp=$(printf '%b' "$mp")
            kib=$(df -Pk "$root$mp" 2>/dev/null | awk 'NR == 2 { print $4 }')
            printf 'drive=%s|%s\n' "$mp" "$(human_size "${kib:-0}")"
        done
    } > "$tmp"
    publish "$tmp" "$out"
    chmod 644 "$out"
}

do_set() {
    user=$1 key=$2 val=$3
    ensure_conf "$user"
    case "$key" in
        ENABLED) case "$val" in 0|1) ;; *) fail "valeur ENABLED invalide" ;; esac ;;
        FREQUENCY) case "$val" in 10|15|20|30|60|180|360|720|1440) ;; *) fail "fréquence invalide (minutes)" ;; esac ;;
        KEEP_DAYS) case "$val" in 0|1|7|30|90|180|365|730) ;; *) fail "durée de conservation invalide (jours)" ;; esac ;;
        DESTINATION)
            if test -n "$val" && ! is_network "$val"; then
                check_path "$val"
                test -d "$root$val" || fail "dossier de destination introuvable : $val"
                # Root y écrit : un disque monté ou le dossier du compte, pas /etc.
                case "$val/" in /media/*|/mnt/*|/run/media/*) ;; *) in_scope "$user" "$val" || fail "destination refusée : choisissez un disque branché ($val)" ;; esac
            fi ;;
        *) fail "réglage inconnu : $key" ;;
    esac
    put "$(conf_file "$user")" "$key" "$val"
    test "$key" != DESTINATION || put "$(conf_file "$user")" LAST_RESULT ""
}

do_folder() {
    op=$1 user=$2 path=$3
    ensure_conf "$user"
    check_path "$path"
    path=${path%/}
    test -n "$path" || fail "la racine ne peut pas être sauvegardée"
    test "$op" = remove || in_scope "$user" "$path" || fail "seuls les dossiers du compte (dossier personnel, disques /media/$user) se sauvegardent : $path"
    f=$(conf_file "$user")
    grep -vxF "FOLDER=$path" "$f" > "$f.$$" || true
    test "$op" = remove || printf 'FOLDER=%s\n' "$path" >> "$f.$$"
    mv "$f.$$" "$f"
}

do_share() {
    user=$1 url=$2 account=${3:-} password=${4:-}
    # « - » : le mot de passe arrive sur l'entrée standard (jamais dans la
    # liste des processus). Paramètres l'envoie ainsi.
    if test "$password" = -; then IFS= read -r password || true; fi
    case "$account$password" in *'
'*) fail "compte ou mot de passe invalide" ;; esac
    is_network "$url" || fail "adresse de partage invalide : smb://serveur/partage ou serveur:/chemin"
    ensure_conf "$user"
    if test -n "$account"; then
        printf 'username=%s\npassword=%s\n' "$account" "$password" > "$(cred_file "$user").$$"
        mv "$(cred_file "$user").$$" "$(cred_file "$user")"
    else rm -f "$(cred_file "$user")"; fi
    put "$(conf_file "$user")" DESTINATION "$url"
    history=$(open_destination "$user") || { put "$(conf_file "$user")" LAST_RESULT unavailable; close_destination "$user"; fail "connexion au partage impossible"; }
    close_destination "$user"
    put "$(conf_file "$user")" LAST_RESULT ""
    printf 'Partage prêt : %s\n' "$history"
}

do_auto() {
    for f in "$config"/*.conf; do
        test -f "$f" || continue
        user=$(basename "$f" .conf)
        test "$(get "$f" ENABLED)" = 1 || continue
        freq=$(get "$f" FREQUENCY); case "$freq" in ''|*[!0-9]*) freq=60 ;; esac
        last=$(get "$f" LAST_ATTEMPT); case "$last" in ''|*[!0-9]*) last=0 ;; esac
        test $(( $(now) - last )) -ge $(( freq * 60 )) || continue
        (sh "$0" run "$user" >> "$run/$user.log" 2>&1) || true
        do_status "$user"
    done
}

cmd=${1:-}
test $# -gt 0 && shift
case "$cmd" in
    status) check_user "${1:-}"; do_status "$1" ;;
    set) check_user "${1:-}"; test $# -ge 2 || fail "usage : set UTILISATEUR CLE [VALEUR]"; do_set "$1" "$2" "${3:-}"; do_status "$1" ;;
    add|remove) check_user "${1:-}"; test $# -eq 2 || fail "usage : $cmd UTILISATEUR DOSSIER"; do_folder "$cmd" "$1" "$2"; do_status "$1" ;;
    share) check_user "${1:-}"; test $# -ge 2 || fail "usage : share UTILISATEUR URL [COMPTE [MOT_DE_PASSE]]"; do_share "$@"; do_status "$1" ;;
    run)
        check_user "${1:-}"
        if ( do_run "$1" ); then rc=0; else rc=$?; fi
        # BusyBox n'exécute pas toujours le piège EXIT d'un sous-shell.
        if test "$rc" != 75; then rmdir "$run/$1.lock" 2>/dev/null || true; close_destination "$1"; fi
        do_status "$1"; exit "$rc" ;;
    auto-run) do_auto ;;
    prune) check_user "${1:-}"; history=$(open_destination "$1") || fail "destination indisponible"; do_prune "$1" "$history"; close_destination "$1" ;;
    list) check_user "${1:-}"; do_list "$1"; close_destination "$1" ;;
    browse) check_user "${1:-}"; test $# -eq 3 || fail "usage : browse UTILISATEUR INSTANTANE DOSSIER"; do_browse "$@"; close_destination "$1" ;;
    versions) check_user "${1:-}"; test $# -eq 2 || fail "usage : versions UTILISATEUR CHEMIN"; do_versions "$@"; close_destination "$1" ;;
    restore) check_user "${1:-}"; test $# -ge 3 || fail "usage : restore UTILISATEUR INSTANTANE CHEMIN [MODE]"; do_restore "$@"; close_destination "$1" ;;
    drives) do_drives ;;
    request)
        # Le bureau écrit une commande par ligne (UTF-8) : les chemins avec
        # espaces, guillemets ou accents ne passent pas par la ligne de commande.
        check_user "${1:-}"
        req="$run/$1/request"
        # Déposée par le compte dans son dossier d'état : un fichier ordinaire.
        test -f "$req" && test ! -L "$req" || { rm -f "$req"; fail "aucune requête"; }
        set -- $(sed -n '1p' "$req" | tr -cd 'a-z-') "$1"
        a1=$(sed -n '2p' "$req"); a2=$(sed -n '3p' "$req"); a3=$(sed -n '4p' "$req"); a4=$(sed -n '5p' "$req")
        rm -f "$req"
        case "${1:-}" in
            set|add|remove|browse|versions|restore|share)
                n=0
                for v in "$a1" "$a2" "$a3" "$a4"; do test -z "$v" || n=$((n + 1)); done
                case "$n" in
                    0) exec sh "$0" "$1" "$2" ;;
                    1) exec sh "$0" "$1" "$2" "$a1" ;;
                    2) exec sh "$0" "$1" "$2" "$a1" "$a2" ;;
                    3) exec sh "$0" "$1" "$2" "$a1" "$a2" "$a3" ;;
                    *) exec sh "$0" "$1" "$2" "$a1" "$a2" "$a3" "$a4" ;;
                esac ;;
            *) fail "requête inconnue" ;;
        esac ;;
    *) sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
