#!/bin/sh
# Wi-Fi chiffré dans l'image : vérifié à la construction (build-live-iso.sh).
# v0.2.17 : sans le module ccm, mac80211 ne posait pas la clé CCMP, la
# poignée de main WPA2 échouait et NetworkManager redemandait le mot de passe ;
# sans regulatory.db le pays restait « monde » (canaux 12-13 invisibles).
#   check-image-wifi.sh <stage> <version du noyau>
set -eu
stage=$1
release=$2
modules="$stage/lib/modules/$release"
host=${WINLUX_HOST_MODULES:-/lib/modules}
missing=""
# Modules que mac80211 et les pilotes demandent par leur nom (aucune
# dépendance déclarée) : s'ils sont modules dans ce noyau, ils doivent être là.
for name in ccm cmac michael_mic gcm ghash_generic aesni_intel cfg80211 mac80211 brcmfmac_cyw r8723bs mwifiex_sdio rtw88_8723cs; do
    file=$(find "$host/$release/kernel" -name "$name.ko*" -print -quit 2>/dev/null || true)
    if test -z "$file"; then
        alt=$(echo "$name" | tr _ -)
        file=$(find "$host/$release/kernel" -name "$alt.ko*" -print -quit 2>/dev/null || true)
    fi
    # Intégré au noyau ou absent de cette version : rien à embarquer.
    test -n "$file" || continue
    relative=${file#$host/$release/}
    test -f "$modules/${relative%.zst}" || missing="$missing $name"
done
if test -n "$missing"; then
    echo "Wi-Fi : modules du noyau absents de l'image :$missing" >&2
    exit 1
fi
if ! grep -q '/ccm\.ko' "$modules/modules.dep" 2>/dev/null && ! grep -q '/ccm\.ko' "$modules/modules.builtin" 2>/dev/null; then
    echo "Wi-Fi : ccm inconnu de modules.dep (depmod) : modprobe ccm échouerait" >&2
    exit 1
fi
if ! test -f "$stage/lib/firmware/regulatory.db"; then
    echo "Wi-Fi : regulatory.db absent de /lib/firmware (paquet wireless-regdb)" >&2
    exit 1
fi
echo "Wi-Fi : crypto du noyau, pilotes et regulatory.db présents"
