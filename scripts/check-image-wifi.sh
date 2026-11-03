#!/bin/sh
# Réseau sans fil et pare-feu dans l'image : vérifié à la construction
# (build-live-iso.sh). v0.2.17 : sans le module ccm, mac80211 ne posait pas la
# clé CCMP, la poignée de main WPA2 échouait et NetworkManager redemandait le
# mot de passe ; sans regulatory.db le pays restait « monde » (canaux 12-13
# invisibles) ; sans netfilter le pare-feu ne faisait rien.
#   check-image-wifi.sh <stage> <version du noyau>
set -eu
stage=$1
release=$2
modules="$stage/lib/modules/$release"
host=${WINLUX_HOST_MODULES:-/lib/modules}
missing=""
# Modules demandés par leur nom (aucune dépendance déclarée) ou attendus par
# type de réseau : s'ils sont modules dans ce noyau, ils doivent être là.
#   WEP : libarc4 ; WPA (TKIP) : michael_mic ; WPA2 : ccm ; WPA3/GCMP : gcm,
#   ghash ; trames de gestion protégées : cmac ; accélération : aesni_intel ;
#   pile 802.11 : cfg80211, mac80211 ; tablettes : brcmfmac_cyw, r8723bs,
#   mwifiex_sdio, rtw88_8723cs ; pare-feu et NAT : nf_tables, nft_chain_nat,
#   nf_conntrack ; partage de connexion : cdc_ether, rndis_host, ipheth, bnep,
#   bridge, veth.
for name in libarc4 michael_mic ccm gcm ghash_generic cmac aesni_intel cfg80211 mac80211 \
            brcmfmac_cyw r8723bs mwifiex_sdio rtw88_8723cs \
            nf_tables nft_chain_nat nf_conntrack nf_nat \
            cdc_ether rndis_host ipheth bnep bridge veth; do
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
    echo "Wi-Fi/réseau : modules du noyau absents de l'image :$missing" >&2
    exit 1
fi
for name in ccm nf_tables; do
    if ! grep -q "/$name\.ko" "$modules/modules.dep" 2>/dev/null && ! grep -q "/$name\.ko" "$modules/modules.builtin" 2>/dev/null; then
        echo "Wi-Fi/réseau : $name inconnu de modules.dep (depmod) : modprobe $name échouerait" >&2
        exit 1
    fi
done
if ! test -f "$stage/lib/firmware/regulatory.db"; then
    echo "Wi-Fi : regulatory.db absent de /lib/firmware (paquet wireless-regdb)" >&2
    exit 1
fi
# wpa_supplicant avec WPA3 (SAE), Entreprise (PEAP, TTLS, MSCHAPv2) et WPS ; NetworkManager.
supplicant=""
for candidate in "$stage/usr/sbin/wpa_supplicant" "$stage/sbin/wpa_supplicant"; do
    test -f "$candidate" && supplicant=$candidate && break
done
if test -z "$supplicant"; then
    echo "Wi-Fi : wpa_supplicant absent (paquet wpasupplicant)" >&2
    exit 1
fi
for feature in SAE PEAP TTLS MSCHAPV2 WPS; do
    if ! grep -a -q "$feature" "$supplicant"; then
        echo "Wi-Fi : wpa_supplicant sans $feature" >&2
        exit 1
    fi
done
for binary in usr/sbin/NetworkManager usr/bin/nmcli; do
    test -f "$stage/$binary" || { echo "Wi-Fi : $binary absent" >&2; exit 1; }
done
# Certificats : magasin complet (Wi-Fi d'entreprise, sites, mises à jour,
# Boutique). Un magasin vide ou rachitique fait refuser des connexions.
test -s "$stage/etc/ssl/certs/ca-certificates.crt" || { echo "Certificats : magasin racine absent" >&2; exit 1; }
roots=$(grep -c "BEGIN CERTIFICATE" "$stage/etc/ssl/certs/ca-certificates.crt") || true
if test "${roots:-0}" -lt "${WINLUX_MIN_ROOTS:-100}"; then
    echo "Certificats : magasin rachitique ($roots autorités, au moins ${WINLUX_MIN_ROOTS:-100} attendues)" >&2
    exit 1
fi
mozilla=$(ls "$stage/usr/share/ca-certificates/mozilla" 2>/dev/null | grep -c '\.crt$') || true
if test "${mozilla:-0}" -lt "${WINLUX_MIN_ROOTS:-100}"; then
    echo "Certificats : /usr/share/ca-certificates/mozilla incomplet ($mozilla) : update-ca-certificates viderait le magasin" >&2
    exit 1
fi
enabled=$(grep -c '^mozilla/' "$stage/etc/ca-certificates.conf" 2>/dev/null) || true
if test "${enabled:-0}" -lt "${WINLUX_MIN_ROOTS:-100}"; then
    echo "Certificats : /etc/ca-certificates.conf absent ou incomplet : update-ca-certificates viderait le magasin" >&2
    exit 1
fi
hashed=$(find "$stage/etc/ssl/certs" -maxdepth 1 -name '*.0' 2>/dev/null | wc -l)
if test "${hashed:-0}" -lt "${WINLUX_MIN_ROOTS:-100}"; then
    echo "Certificats : /etc/ssl/certs sans liens par empreinte ($hashed) : le Wi-Fi d'entreprise ne vérifierait aucun serveur" >&2
    exit 1
fi
for binary in usr/sbin/update-ca-certificates usr/bin/openssl usr/bin/certutil usr/bin/winlux-certificates; do
    test -f "$stage/$binary" || { echo "Certificats : $binary absent (ajout d'une autorité d'entreprise impossible)" >&2; exit 1; }
done
# Heure : le noyau doit lire l'horloge matérielle au démarrage (sinon 1970 et
# tous les certificats refusés jusqu'à la synchronisation SNTP).
config="$stage/boot/config-$release"
test -f "$config" || config="/boot/config-$release"
if test -f "$config" && ! grep -q '^CONFIG_RTC_HCTOSYS=y' "$config"; then
    echo "Heure : noyau sans CONFIG_RTC_HCTOSYS (horloge matérielle non lue au démarrage)" >&2
    exit 1
fi
echo "Wi-Fi : sécurités WEP à WPA3 et Entreprise, pilotes, regulatory.db, pare-feu et partage de connexion présents"
echo "Certificats : $roots autorités racine, ajout d'autorités et heure automatique présents"
