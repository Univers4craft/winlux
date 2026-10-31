#!/bin/sh
# Gestion du firmware pour le matériel détecté.
# Vérifie la présence des firmwares nécessaires et signale les manquants.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

WINLUX_HW_LOG=${WINLUX_HW_LOG:-/run/winlux/hardware.log}
log() { printf '[%s] firmware: %s\n' "$(date '+%H:%M:%S' 2>/dev/null || echo '??:??:??')" "$*" >>"$WINLUX_HW_LOG"; }

firmware_dir=/lib/firmware
missing_log=/run/winlux/hw/firmware-missing.txt
: >"$missing_log"

check_firmware() {
    name=$1
    path=$2
    if test -f "$firmware_dir/$path" || test -f "$firmware_dir/${path}.zst" || test -f "$firmware_dir/${path}.xz"; then
        log "Firmware présent : $name ($path)"
        return 0
    fi
    log "Firmware MANQUANT : $name ($path)"
    printf '%s %s\n' "$name" "$path" >>"$missing_log"
    return 1
}

# Wi-Fi Intel (iwlwifi)
if test "$(cat /run/winlux/hw/has-wifi 2>/dev/null)" = 1; then
    for fw in /lib/firmware/iwlwifi-*.ucode; do
        test -f "$fw" && { log "Firmware Intel Wi-Fi présent"; break; } || true
    done
fi

# NVIDIA (si driver propriétaire)
if test "$(cat /run/winlux/hw/nvidia-driver 2>/dev/null)" = "nvidia"; then
    for fw in /lib/firmware/nvidia; do
        test -d "$fw" && { log "Firmware NVIDIA présent"; break; } || true
    done
fi

# AMD GPU
if test "$(cat /run/winlux/hw/has-amd 2>/dev/null)" = 1; then
    for fw in /lib/firmware/amdgpu; do
        test -d "$fw" && { log "Firmware AMD GPU présent"; break; } || true
    done
fi

# Bluetooth
if test "$(cat /run/winlux/hw/has-bluetooth 2>/dev/null)" = 1; then
    for fw_dir in intel rtl_bt qca mediatek; do
        test -d "$firmware_dir/$fw_dir" && log "Firmware Bluetooth ($fw_dir) présent" || true
    done
fi

# Résumé
missing_count=$(wc -l <"$missing_log" 2>/dev/null || echo 0)
if test "$missing_count" -gt 0; then
    log "ATTENTION : $missing_count firmware(s) manquant(s). Voir $missing_log"
else
    log "Tous les firmwares nécessaires sont présents"
fi
