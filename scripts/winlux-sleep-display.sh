#!/bin/sh
# Script de veille (/usr/lib/winlux/sleep.d/10-display) : au réveil, les
# écrans ont pu être branchés ou débranchés pendant la veille.
case "${1:-}" in
    post) DISPLAY=${DISPLAY:-:0} winlux-display sync >/dev/null 2>&1 || true ;;
esac
exit 0
