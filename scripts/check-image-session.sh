#!/bin/sh
# Plomberie de la session native dans l'image, vérifiée à la construction
# (build-live-iso.sh). Relevé sur un vrai PC en v0.2.19 :
#   #604 : /dev/rfkill réservé à root (sans logind) → blueman-applet meurt,
#          plus d'agent d'appairage ; commande rfkill absente ;
#   #610 : glxinfo absent → ligne OpenGL du rapport toujours « ? » ;
#   #615 : at-spi2-core absent → pas de bus org.a11y.Bus, aucune technologie
#          d'assistance possible.
#   check-image-session.sh <stage>
set -eu
stage=$1
missing=""
for path in usr/bin/rfkill usr/bin/glxinfo usr/libexec/at-spi-bus-launcher \
            usr/share/dbus-1/services/org.a11y.Bus.service; do
    test -e "$stage/$path" || missing="$missing /$path"
done
rule="$stage/usr/lib/udev/rules.d/60-winlux-rfkill.rules"
if ! test -f "$rule"; then
    missing="$missing /usr/lib/udev/rules.d/60-winlux-rfkill.rules"
elif ! grep -q '^KERNEL=="rfkill".*GROUP="netdev".*MODE="0664"' "$rule"; then
    echo "Session native : 60-winlux-rfkill.rules n'ouvre pas /dev/rfkill au groupe netdev" >&2
    exit 1
fi
if test -n "$missing"; then
    echo "Session native : absent de l'image :$missing" >&2
    exit 1
fi
echo "Session native : rfkill, glxinfo, bus d'accessibilité et règle udev présents"
