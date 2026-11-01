#!/bin/sh
# Hook APT Post-Invoke : détecte les nouveaux services systemd installés par
# un paquet et les enregistre automatiquement dans le système d'init Winlux.
# Un service dont l'unité contient WantedBy=multi-user.target (ou default.target)
# est activé et démarré.
set -eu

UNIT_DIRS="/usr/lib/systemd/system /lib/systemd/system"
ENABLED_DIR="/etc/winlux/services-enabled"
SYSTEMCTL=/usr/bin/systemctl

test -x "$SYSTEMCTL" || exit 0
mkdir -p "$ENABLED_DIR"

# Rien n'est démarré hors du système en marche : dans un chroot (construction
# de l'image, DPKG_ROOT, /proc/1 absent ou racine différente de la nôtre) ou
# quand systemd est le vrai init (il gère lui-même ses unités).
can_start=1
if test -n "${DPKG_ROOT:-}" || ! test -r /proc/1/comm; then
    can_start=
elif test "$(stat -Lc %d:%i / 2>/dev/null)" != "$(stat -Lc %d:%i /proc/1/root/. 2>/dev/null)"; then
    can_start=
elif test "$(cat /proc/1/comm 2>/dev/null)" = systemd; then
    can_start=
fi

for dir in $UNIT_DIRS; do
    test -d "$dir" || continue
    for unit in "$dir"/*.service; do
        test -f "$unit" || continue
        name=${unit##*/}
        # Already managed or masked.
        test -f "$ENABLED_DIR/$name" && continue
        test -L "/etc/systemd/system/$name" && continue
        wanted=
        section=
        while IFS= read -r line; do
            case "$line" in '#'*|';'*|'') continue ;; '['*']') section=$line; continue ;; esac
            test "$section" = "[Install]" || continue
            case "$line" in
                WantedBy=*multi-user*|WantedBy=*default*|WantedBy=*graphical*)
                    wanted=1 ;;
            esac
        done < "$unit"
        test -n "$wanted" || continue
        "$SYSTEMCTL" enable "$name" 2>/dev/null || true
        test -n "$can_start" || continue
        "$SYSTEMCTL" start "$name" 2>/dev/null || true
    done
done
