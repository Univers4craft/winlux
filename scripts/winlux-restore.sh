#!/bin/sh
# winlux-restore : gestion des points de restauration système.
# Usage :
#   winlux-restore create [NOM]   — créer un point de restauration
#   winlux-restore list           — lister les points disponibles
#   winlux-restore restore ID     — restaurer un point
#   winlux-restore delete ID      — supprimer un point
#   winlux-restore status         — état du système de restauration
set -eu

RESTORE_DIR=/var/lib/winlux/restore
# Les comptes : passwd/group sans leurs mots de passe (shadow/gshadow) seraient
# incohérents après restauration ; cp -a conserve leurs droits (0640 root:shadow).
CONF_DIRS="/etc/winlux /etc/passwd /etc/group /etc/shadow /etc/gshadow"

mkdir -p "$RESTORE_DIR"

timestamp() {
    date '+%Y%m%d-%H%M%S' 2>/dev/null || printf '%s' "$(cat /proc/uptime | cut -d' ' -f1)"
}

human_date() {
    date '+%Y-%m-%d %H:%M:%S' 2>/dev/null || echo "inconnu"
}

case "${1:-status}" in
    create)
        name=${2:-"auto-$(timestamp)"}
        id="rp-$(timestamp)"
        point_dir="$RESTORE_DIR/$id"
        mkdir -p "$point_dir/config" "$point_dir/meta"

        # Métadonnées
        printf '%s\n' "$name" > "$point_dir/meta/name"
        human_date > "$point_dir/meta/date"
        printf '%s\n' "$id" > "$point_dir/meta/id"

        # Sauvegarder les fichiers de configuration
        for path in $CONF_DIRS; do
            if test -e "$path"; then
                dest="$point_dir/config$path"
                mkdir -p "$(dirname "$dest")"
                cp -a "$path" "$dest" 2>/dev/null || true
            fi
        done

        # Liste des paquets installés (dpkg si dispo, sinon opkg/busybox)
        if command -v dpkg >/dev/null 2>&1; then
            dpkg --get-selections > "$point_dir/meta/packages.list" 2>/dev/null || true
        fi

        # Version du système
        if test -f /usr/share/winlux/version; then
            cp /usr/share/winlux/version "$point_dir/meta/version"
        fi
        if test -f /usr/share/winlux/BUILD-INFO; then
            cp /usr/share/winlux/BUILD-INFO "$point_dir/meta/build-info"
        fi

        # Taille du point
        du -sh "$point_dir" 2>/dev/null | cut -f1 > "$point_dir/meta/size"

        printf 'RESTORE_CREATED\n'
        printf 'id=%s\n' "$id"
        printf 'name=%s\n' "$name"
        ;;

    list)
        if ! test -d "$RESTORE_DIR"; then
            printf 'RESTORE_EMPTY\n'
            exit 0
        fi
        count=0
        for point in "$RESTORE_DIR"/rp-*; do
            test -d "$point" || continue
            id=$(basename "$point")
            name=$(cat "$point/meta/name" 2>/dev/null || echo "$id")
            date=$(cat "$point/meta/date" 2>/dev/null || echo "inconnue")
            size=$(cat "$point/meta/size" 2>/dev/null || echo "?")
            printf '%s|%s|%s|%s\n' "$id" "$name" "$date" "$size"
            count=$((count + 1))
        done
        if test "$count" = 0; then
            printf 'RESTORE_EMPTY\n'
        fi
        ;;

    restore)
        id=${2:?'id du point requis'}
        point_dir="$RESTORE_DIR/$id"
        if ! test -d "$point_dir"; then
            printf 'RESTORE_NOT_FOUND\n' >&2
            exit 1
        fi

        # Restaurer les fichiers de configuration
        if test -d "$point_dir/config"; then
            for src in $(find "$point_dir/config" -type f 2>/dev/null); do
                dest=${src#"$point_dir/config"}
                mkdir -p "$(dirname "$dest")"
                cp -a "$src" "$dest" 2>/dev/null || printf 'SKIP %s\n' "$dest" >&2
            done
        fi

        # Restaurer les paquets si dpkg est disponible
        if test -f "$point_dir/meta/packages.list" && command -v dpkg >/dev/null 2>&1; then
            dpkg --set-selections < "$point_dir/meta/packages.list" 2>/dev/null || true
            if command -v apt-get >/dev/null 2>&1; then
                apt-get -y dselect-upgrade 2>/dev/null || true
            fi
        fi

        printf 'RESTORE_DONE\n'
        printf 'id=%s\n' "$id"
        ;;

    delete)
        id=${2:?'id du point requis'}
        point_dir="$RESTORE_DIR/$id"
        if test -d "$point_dir"; then
            rm -rf "$point_dir"
            printf 'RESTORE_DELETED\n'
        else
            printf 'RESTORE_NOT_FOUND\n' >&2
            exit 1
        fi
        ;;

    status)
        count=0
        total_size=0
        if test -d "$RESTORE_DIR"; then
            for point in "$RESTORE_DIR"/rp-*; do
                test -d "$point" || continue
                count=$((count + 1))
            done
            total_size=$(du -sh "$RESTORE_DIR" 2>/dev/null | cut -f1 || echo "0K")
        fi
        printf 'RESTORE_STATUS\n'
        printf 'count=%d\n' "$count"
        printf 'size=%s\n' "$total_size"
        ;;

    *)
        printf 'Usage: winlux-restore {create|list|restore|delete|status}\n' >&2
        exit 1
        ;;
esac
