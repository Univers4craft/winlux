#!/bin/sh
# Script de veille (/usr/lib/winlux/sleep.d/20-touchpad) : certains pavés
# tactiles reviennent de veille avec les réglages du pilote par défaut.
case "${1:-}" in
    post) DISPLAY=${DISPLAY:-:0} winlux-touchpad apply >/dev/null 2>&1 || true ;;
esac
exit 0
