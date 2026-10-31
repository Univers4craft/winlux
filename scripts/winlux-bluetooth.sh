#!/bin/sh
# Bluetooth du bureau Winlux : pilote BlueZ (bluetoothd) via son client
# D-Bus bluetoothctl. L'état est écrit (UTF-8) dans
# /run/winlux/bluetooth.txt pour la page « Bluetooth et appareils » et
# l'indicateur de la barre des tâches.
set -eu
root=${WINLUX_CONTROL_ROOT:-}
run="$root/run/winlux"
mkdir -p "$run"
ctl=${WINLUX_BLUETOOTHCTL:-bluetoothctl}

have() { command -v "$1" >/dev/null 2>&1; }
valid_mac() {
    case "$1" in
        [0-9A-Fa-f][0-9A-Fa-f]:[0-9A-Fa-f][0-9A-Fa-f]:[0-9A-Fa-f][0-9A-Fa-f]:[0-9A-Fa-f][0-9A-Fa-f]:[0-9A-Fa-f][0-9A-Fa-f]:[0-9A-Fa-f][0-9A-Fa-f]) return 0 ;;
        *) return 1 ;;
    esac
}
# Noms d'appareils : une ligne, sans séparateur. Le fichier reste en UTF-8,
# converti en Windows-1252 par le bureau à la lecture.
clean() { tr -d '|\r\n' | tr '\t' ' '; }
kind_of() {
    case "$1" in
        audio-*) echo audio ;;
        input-keyboard) echo clavier ;;
        input-mouse|input-tablet) echo souris ;;
        input-gaming) echo manette ;;
        phone) echo telephone ;;
        computer) echo ordinateur ;;
        *) echo appareil ;;
    esac
}

status() {
    out="$run/bluetooth.txt"
    tmp="$out.tmp.$$"
    {
        if ! have "$ctl" || ! show=$("$ctl" show 2>/dev/null) || ! printf '%s\n' "$show" | grep -q '^Controller '; then
            printf 'present=0\npowered=0\ndiscovering=0\nconnected=0\n'
        else
            printf 'present=1\n'
            printf 'adapter=%s\n' "$(printf '%s\n' "$show" | sed -n 's/^[[:space:]]*Alias: //p' | head -n 1 | clean)"
            powered=0; printf '%s\n' "$show" | grep -q 'Powered: yes' && powered=1
            discovering=0; printf '%s\n' "$show" | grep -q 'Discovering: yes' && discovering=1
            printf 'powered=%s\ndiscovering=%s\n' "$powered" "$discovering"
            connected=0
            for mac in $("$ctl" devices 2>/dev/null | awk '$1 == "Device" { print $2 }' | head -n 12); do
                valid_mac "$mac" || continue
                info=$("$ctl" info "$mac" 2>/dev/null) || continue
                name=$(printf '%s\n' "$info" | sed -n 's/^[[:space:]]*Name: //p' | head -n 1 | clean)
                test -n "$name" || name=$mac
                icon=$(printf '%s\n' "$info" | sed -n 's/^[[:space:]]*Icon: //p' | head -n 1)
                paired=0; printf '%s\n' "$info" | grep -q 'Paired: yes' && paired=1
                conn=0; printf '%s\n' "$info" | grep -q 'Connected: yes' && conn=1
                connected=$((connected + conn))
                printf 'device=%s|%s|%s|%s|%s\n' "$mac" "$name" "$(kind_of "$icon")" "$paired" "$conn"
            done
            printf 'connected=%s\n' "$connected"
        fi
    } > "$tmp"
    mv "$tmp" "$out"
}

action=${1:-status}
mac=${2:-}
case "$action" in
    status) status; exit 0 ;;
    pair|connect|disconnect|remove)
        valid_mac "$mac" || { echo "Adresse Bluetooth invalide : $mac" >&2; exit 1; } ;;
esac
have "$ctl" || { echo 'BlueZ (bluetoothctl) non installe' >&2; exit 1; }
rc=0
case "$action" in
    on)
        if have rfkill; then rfkill unblock bluetooth 2>/dev/null || true; fi
        "$ctl" power on || rc=$? ;;
    off) "$ctl" power off || rc=$? ;;
    scan)
        "$ctl" power on >/dev/null 2>&1 || true
        "$ctl" --timeout "${WINLUX_BLUETOOTH_SCAN_SECONDS:-8}" scan on || rc=$? ;;
    pair)
        # Agent « NoInputNoOutput » : appairage simple (casques, manettes,
        # souris). Un clavier qui exige un code reste à appairer à la main.
        "$ctl" --agent NoInputNoOutput --timeout 25 pair "$mac" || rc=$?
        if test "$rc" = 0; then
            "$ctl" trust "$mac" || true
            "$ctl" --timeout 15 connect "$mac" || rc=$?
        fi ;;
    connect) "$ctl" --timeout 15 connect "$mac" || rc=$? ;;
    disconnect) "$ctl" disconnect "$mac" || rc=$? ;;
    remove) "$ctl" remove "$mac" || rc=$? ;;
    *) echo "Action Bluetooth inconnue : $action" >&2; exit 1 ;;
esac
status
exit "$rc"
