#!/bin/bash
# Wi-Fi WPA3 (SAE) en GCMP-256 : test déterministe sur radios simulées.
#
# mac80211_hwsim crée deux cartes Wi-Fi virtuelles dans le noyau ; hostapd
# joue la box sur la première, wpa_supplicant le PC sur la seconde. Trois cas :
#   1. SAE + CCMP, réseau décrit comme le fait NetworkManager : témoin, doit se connecter ;
#   2. SAE + GCMP-256, chiffrement GCMP-256 demandé explicitement : doit se
#      connecter (le noyau et les modules gcm/ghash savent le faire) ;
#   3. SAE + GCMP-256, réseau décrit comme le fait NetworkManager (pas de
#      « pairwise », NetworkManager n'accepte que tkip et ccmp) : le résultat
#      dit si une box en GCMP-256 seul est joignable avec ce wpa_supplicant.
# Le cas 3 doit concorder avec ce que winlux-wifi annonce (WINLUX_EXPECT_GCMP_NM
# = connect|refuse) ; sans attente fixée, le résultat est seulement affiché.
#
# Demande root, hostapd, wpa_supplicant, iw et le module mac80211_hwsim.
# Sans module chargeable, le test s'arrête avec le code 77 (« non vérifiable »).
set -u
work=$(mktemp -d)
cleanup() {
    pkill -f "hostapd $work" 2>/dev/null
    pkill -f "wpa_supplicant .*$work" 2>/dev/null
    sleep 0.5
    rmmod mac80211_hwsim 2>/dev/null
    rm -rf "$work"
}
trap cleanup EXIT
test "$(id -u)" = 0 || { echo "root requis" >&2; exit 77; }
for tool in hostapd wpa_supplicant wpa_cli iw; do
    command -v "$tool" >/dev/null || { echo "$tool absent" >&2; exit 77; }
done
rmmod mac80211_hwsim 2>/dev/null
modprobe mac80211_hwsim radios=2 || { echo "mac80211_hwsim indisponible sur ce noyau" >&2; exit 77; }
sleep 1
interfaces=$(for phy in /sys/class/ieee80211/*; do ls "$phy/device/net" 2>/dev/null; done | sort)
ap=$(echo "$interfaces" | sed -n 1p)
sta=$(echo "$interfaces" | sed -n 2p)
test -n "$ap" && test -n "$sta" || { echo "radios simulées introuvables" >&2; exit 77; }
command -v nmcli >/dev/null && nmcli device set "$ap" managed no 2>/dev/null; command -v nmcli >/dev/null && nmcli device set "$sta" managed no 2>/dev/null
echo "wpa_supplicant : $(wpa_supplicant -v 2>&1 | sed -n 1p)"

# connect NOM CHIFFREMENT_BOX LIGNES_RESEAU_SUPPLICANT
connect() {
    name=$1 cipher=$2 network=$3
    pkill -f "hostapd $work" 2>/dev/null; pkill -f "wpa_supplicant .*$work" 2>/dev/null; sleep 0.5
    cat >"$work/hostapd.conf" <<EOF
interface=$ap
driver=nl80211
ssid=Box-$name
hw_mode=g
channel=6
wpa=2
wpa_key_mgmt=SAE
rsn_pairwise=$cipher
group_cipher=$cipher
ieee80211w=2
sae_pwe=2
sae_password=motdepasse-winlux
EOF
    hostapd -B "$work/hostapd.conf" -P "$work/hostapd.pid" >"$work/hostapd-$name.log" 2>&1 || { cat "$work/hostapd-$name.log"; return 2; }
    cat >"$work/sta.conf" <<EOF
ctrl_interface=$work/ctrl
pmf=1
sae_pwe=2
network={
    ssid="Box-$name"
    key_mgmt=SAE
    proto=RSN
    ieee80211w=2
    sae_password="motdepasse-winlux"
$network
}
EOF
    wpa_supplicant -B -i "$sta" -D nl80211 -c "$work/sta.conf" -f "$work/sta-$name.log" || return 2
    for _ in $(seq 1 40); do
        if wpa_cli -p "$work/ctrl" -i "$sta" status 2>/dev/null | grep -q '^wpa_state=COMPLETED'; then
            wpa_cli -p "$work/ctrl" -i "$sta" status | grep -E '^(pairwise_cipher|group_cipher|key_mgmt)='
            return 0
        fi
        sleep 0.5
    done
    tail -n 5 "$work/sta-$name.log"
    return 1
}

failed=0
if connect ccmp CCMP ""; then echo "1. SAE + CCMP (comme NetworkManager) : connecté"; else echo "ÉCHEC 1. SAE + CCMP : pas de connexion (témoin)"; failed=1; fi
if connect gcmp-explicite GCMP-256 "    pairwise=GCMP-256
    group=GCMP-256"; then echo "2. SAE + GCMP-256 demandé explicitement : connecté"; else echo "ÉCHEC 2. SAE + GCMP-256 explicite : pas de connexion"; failed=1; fi
if connect gcmp-nm GCMP-256 ""; then result=connect; else result=refuse; fi
echo "3. SAE + GCMP-256 décrit comme NetworkManager : $result"
expected=${WINLUX_EXPECT_GCMP_NM:-}
if test -n "$expected" && test "$expected" != "$result"; then
    echo "ÉCHEC 3. attendu « $expected », obtenu « $result »"
    failed=1
fi
exit "$failed"
