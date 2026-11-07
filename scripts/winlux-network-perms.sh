#!/bin/sh
# Accès au réseau pour les comptes du bureau (système installé et Live).
#
# Winlux démarre ses services en root avec un umask 077. Deux effets sur un
# système installé, où le bureau tourne sous le compte de l'utilisateur :
# - NetworkManager écrivait /etc/resolv.conf en 0600 : le compte ne
#   résolvait plus aucun nom. « Connecté à Internet », mais navigateur,
#   Boutique et mises à jour échouaient tous.
# - la règle polkit 50-winlux-network.rules (comptes du groupe netdev) était
#   absente ou illisible par polkitd : nmcli et le Wi-Fi répondaient
#   « non autorisé ».
# Ce script, lancé en root au démarrage puis par winlux-network-watch,
# remet les deux en place, ainsi que les droits de /etc/timezone,
# /etc/hostname et /etc/default/locale, et /dev/rfkill (groupe netdev).
# WINLUX_SYSTEM_ROOT sert aux tests.
set -u
root=${WINLUX_SYSTEM_ROOT:-}
rule_source="$root/usr/share/winlux/native/network/50-winlux-network.rules"
rules="$root/etc/polkit-1/rules.d"
rule="$rules/50-winlux-network.rules"

# resolv.conf lisible par tous (y compris un lien vers un fichier de /run).
resolv="$root/etc/resolv.conf"
if test -e "$resolv"; then
    target=$resolv
    if test -L "$resolv"; then target=$(readlink -f "$resolv" 2>/dev/null || echo "$resolv"); fi
    case "$(stat -c %a "$target" 2>/dev/null)" in
        644|664|666|444) ;;
        *) chmod 644 "$target" 2>/dev/null && echo "resolv.conf rendu lisible : $target" ;;
    esac
fi

# Réglages système que le bureau lit sous le compte de l'utilisateur :
# fuseau, nom de la machine, langue. Écrits en 0600 sous umask 077
# (/etc/timezone illisible : Java, scripts et Paramètres voyaient UTC).
for file in "$root/etc/timezone" "$root/etc/hostname" "$root/etc/default/locale"; do
    test -f "$file" && ! test -L "$file" || continue
    case "$(stat -c %a "$file" 2>/dev/null)" in
        644|664|666|444) ;;
        *) chmod 644 "$file" 2>/dev/null && echo "rendu lisible : $file" ;;
    esac
done

# Interrupteurs radio (#604) : /dev/rfkill au groupe netdev, même quand udev
# a créé le nœud avant de lire 60-winlux-rfkill.rules (module chargé tôt) ou
# ne tourne pas. Sans cela blueman-applet, l'agent d'appairage, meurt.
rfkill="$root/dev/rfkill"
if test -c "$rfkill" || { test -n "$root" && test -f "$rfkill"; }; then
    if getent group netdev >/dev/null 2>&1 && test "$(stat -c %G "$rfkill" 2>/dev/null)" != netdev; then
        chgrp netdev "$rfkill" 2>/dev/null && echo "/dev/rfkill rendu au groupe netdev"
    fi
    case "$(stat -c %a "$rfkill" 2>/dev/null)" in
        664|666) ;;
        *) chmod 664 "$rfkill" 2>/dev/null ;;
    esac
fi

# Règle polkit du réseau, posée et lisible par polkitd.
if test -f "$rule_source"; then
    mkdir -p "$rules"
    if ! test -f "$rule" || test "$(cat "$rule_source")" != "$(cat "$rule")"; then
        cp "$rule_source" "$rule.new" && mv -f "$rule.new" "$rule" && echo "règle polkit du réseau installée"
    fi
fi
if test -d "$rules"; then
    chmod 755 "$root/etc/polkit-1" "$rules" 2>/dev/null || true
    for file in "$rules"/*.rules; do
        test -f "$file" && chmod 644 "$file" 2>/dev/null
    done
fi
exit 0
