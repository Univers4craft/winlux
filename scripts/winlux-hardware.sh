#!/bin/sh
# Détection et configuration automatique du matériel au démarrage.
# Appelé par init après le montage des systèmes de fichiers virtuels.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

WINLUX_HW_LOG=${WINLUX_HW_LOG:-/run/winlux/hardware.log}
mkdir -p "$(dirname "$WINLUX_HW_LOG")" /run/winlux/hw

log() { printf '[%s] %s\n' "$(date '+%H:%M:%S' 2>/dev/null || echo '??:??:??')" "$*" >>"$WINLUX_HW_LOG"; }
# Le winlux-prime-run livré avec Winlux choisit le GPU via le runtime
# (adresse PCI exacte, NVIDIA ou Mesa) : ne pas le remplacer.
prime_run_is_runtime() { grep -q -- "--gpu-env" /usr/bin/winlux-prime-run 2>/dev/null; }
log "Début de la détection matérielle"

# --- Chargement des modules de base ---
load_module() {
    modprobe "$1" 2>/dev/null && log "Module chargé : $1" || true
}

# PCI et USB (nécessaires pour énumérer le matériel)
for m in pci_hotplug shpchp pciehp; do load_module "$m"; done

# --- Énumération PCI ---
enumerate_pci() {
    if command -v lspci >/dev/null 2>&1; then
        lspci -nn 2>/dev/null >/run/winlux/hw/lspci.txt || true
        log "Énumération PCI terminée (lspci)"
    elif test -d /sys/bus/pci/devices; then
        for dev in /sys/bus/pci/devices/*; do
            test -d "$dev" || continue
            vendor=$(cat "$dev/vendor" 2>/dev/null || echo "????")
            device=$(cat "$dev/device" 2>/dev/null || echo "????")
            class=$(cat "$dev/class" 2>/dev/null || echo "??????")
            printf '%s %s %s %s\n' "$(basename "$dev")" "$vendor" "$device" "$class"
        done >/run/winlux/hw/pci-devices.txt
        log "Énumération PCI terminée (sysfs)"
    fi
}

# --- Détection GPU ---
detect_gpu() {
    gpu_count=0
    has_nvidia=0
    has_amd=0
    has_intel=0
    nvidia_ids=""
    amd_ids=""
    intel_ids=""

    for dev in /sys/bus/pci/devices/*; do
        test -d "$dev" || continue
        class=$(cat "$dev/class" 2>/dev/null || echo "0x000000")
        case "$class" in
            0x030000|0x030200|0x038000)
                vendor=$(cat "$dev/vendor" 2>/dev/null || echo "0x0000")
                device_id=$(cat "$dev/device" 2>/dev/null || echo "0x0000")
                gpu_count=$((gpu_count + 1))
                case "$vendor" in
                    0x10de)
                        has_nvidia=1
                        nvidia_ids="$nvidia_ids ${vendor}:${device_id}"
                        log "GPU NVIDIA détecté : $(basename "$dev") [$vendor:$device_id]"
                        ;;
                    0x1002)
                        has_amd=1
                        amd_ids="$amd_ids ${vendor}:${device_id}"
                        log "GPU AMD détecté : $(basename "$dev") [$vendor:$device_id]"
                        ;;
                    0x8086)
                        has_intel=1
                        intel_ids="$intel_ids ${vendor}:${device_id}"
                        log "GPU Intel détecté : $(basename "$dev") [$vendor:$device_id]"
                        ;;
                    *)
                        log "GPU inconnu détecté : $(basename "$dev") [$vendor:$device_id]"
                        ;;
                esac
                ;;
        esac
    done

    printf '%s\n' "$gpu_count" >/run/winlux/hw/gpu-count
    printf '%s\n' "$has_nvidia" >/run/winlux/hw/has-nvidia
    printf '%s\n' "$has_amd" >/run/winlux/hw/has-amd
    printf '%s\n' "$has_intel" >/run/winlux/hw/has-intel
    log "Total GPU détectés : $gpu_count (NVIDIA=$has_nvidia, AMD=$has_amd, Intel=$has_intel)"
}

# --- Configuration GPU ---
configure_gpu() {
    has_nvidia=$(cat /run/winlux/hw/has-nvidia 2>/dev/null || echo 0)
    has_amd=$(cat /run/winlux/hw/has-amd 2>/dev/null || echo 0)
    has_intel=$(cat /run/winlux/hw/has-intel 2>/dev/null || echo 0)
    gpu_count=$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)

    # NVIDIA : tenter nouveau (libre) puis nvidia (propriétaire)
    if test "$has_nvidia" = 1; then
        if test -d /lib/modules/"$(uname -r)"/kernel/drivers/gpu/drm/nouveau; then
            load_module nouveau
            log "Driver nouveau chargé pour GPU NVIDIA"
        fi
        if test -f /usr/bin/nvidia-smi || test -d /lib/modules/"$(uname -r)"/updates/dkms; then
            for m in nvidia nvidia_modeset nvidia_uvm nvidia_drm; do
                load_module "$m"
            done
            if test -c /dev/nvidiactl; then
                log "Driver NVIDIA propriétaire actif"
                printf 'nvidia\n' >/run/winlux/hw/nvidia-driver
            else
                log "Driver NVIDIA propriétaire absent, nouveau utilisé"
                printf 'nouveau\n' >/run/winlux/hw/nvidia-driver
            fi
        else
            printf 'nouveau\n' >/run/winlux/hw/nvidia-driver
        fi
    fi

    # AMD : amdgpu (moderne) puis radeon (ancien)
    if test "$has_amd" = 1; then
        load_module amdgpu
        if test -d /sys/module/amdgpu; then
            log "Driver amdgpu chargé pour GPU AMD"
            printf 'amdgpu\n' >/run/winlux/hw/amd-driver
        else
            load_module radeon
            log "Driver radeon chargé pour GPU AMD (ancien)"
            printf 'radeon\n' >/run/winlux/hw/amd-driver
        fi
    fi

    # Intel : i915
    if test "$has_intel" = 1; then
        load_module i915
        log "Driver i915 chargé pour GPU Intel"
        printf 'i915\n' >/run/winlux/hw/intel-driver
    fi

    # Fallback modesetting si aucun GPU dédié
    if test "$gpu_count" = 0; then
        log "Aucun GPU PCI détecté, mode logiciel (modesetting/swrast)"
        printf 'modesetting\n' >/run/winlux/hw/gpu-driver
    fi

    # Configuration multi-GPU / switchable graphics
    if test "$gpu_count" -gt 1; then
        configure_multi_gpu
    fi
}

# --- Multi-GPU (Optimus/PRIME, switchable) ---
configure_multi_gpu() {
    has_nvidia=$(cat /run/winlux/hw/has-nvidia 2>/dev/null || echo 0)
    has_intel=$(cat /run/winlux/hw/has-intel 2>/dev/null || echo 0)
    has_amd=$(cat /run/winlux/hw/has-amd 2>/dev/null || echo 0)
    gpu_count=$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)

    log "Configuration multi-GPU ($gpu_count GPU)"
    printf '%s\n' "$gpu_count" >/run/winlux/hw/multi-gpu

    # NVIDIA Optimus (Intel + NVIDIA laptop)
    if test "$has_nvidia" = 1 && test "$has_intel" = 1; then
        log "Configuration Optimus détectée (Intel + NVIDIA)"
        printf 'optimus\n' >/run/winlux/hw/gpu-mode

        # PRIME offload : Intel pour le bureau, NVIDIA à la demande
        if test -d /sys/module/nvidia_drm; then
            load_module nvidia_drm
            printf 'prime-offload\n' >/run/winlux/hw/prime-mode
            log "PRIME offload activé (nvidia_drm)"
        else
            printf 'prime-nouveau\n' >/run/winlux/hw/prime-mode
            log "PRIME via nouveau (pas de nvidia_drm)"
        fi

        # Créer le script d'offload GPU pour les applications, sauf si la
        # version Winlux (qui interroge le runtime) est déjà installée.
        mkdir -p /usr/bin
        if prime_run_is_runtime; then
            log "winlux-prime-run du runtime conservé"
        else
        cat >/usr/bin/winlux-prime-run <<'PRIME'
#!/bin/sh
# Lance une application sur le GPU dédié (PRIME offload).
export __NV_PRIME_RENDER_OFFLOAD=1
export __NV_PRIME_RENDER_OFFLOAD_PROVIDER=NVIDIA-G0
export __GLX_VENDOR_LIBRARY_NAME=nvidia
export __VK_LAYER_NV_optimus=NVIDIA_only
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/nvidia_icd.json
exec "$@"
PRIME
        chmod 755 /usr/bin/winlux-prime-run
        log "Script winlux-prime-run créé"
        fi
    fi

    # AMD switchable (Intel + AMD ou AMD + AMD)
    if test "$has_amd" = 1 && test "$has_intel" = 1; then
        log "Configuration switchable AMD détectée (Intel + AMD)"
        printf 'switchable-amd\n' >/run/winlux/hw/gpu-mode

        mkdir -p /usr/bin
        if prime_run_is_runtime; then
            log "winlux-prime-run du runtime conservé"
        else
        cat >/usr/bin/winlux-prime-run <<'PRIME'
#!/bin/sh
# Lance une application sur le GPU AMD dédié (DRI_PRIME).
export DRI_PRIME=1
exec "$@"
PRIME
        chmod 755 /usr/bin/winlux-prime-run
        log "Script winlux-prime-run créé (DRI_PRIME AMD)"
        fi
    fi

    # Multi-GPU identique (SLI/CrossFire style)
    if test "$has_nvidia" = 1 && test "$has_intel" = 0 && test "$has_amd" = 0; then
        nvidia_count=0
        for dev in /sys/bus/pci/devices/*; do
            test -d "$dev" || continue
            vendor=$(cat "$dev/vendor" 2>/dev/null || echo "0x0000")
            class=$(cat "$dev/class" 2>/dev/null || echo "0x000000")
            case "$class" in 0x030000|0x030200) test "$vendor" = "0x10de" && nvidia_count=$((nvidia_count + 1)) ;; esac
        done
        if test "$nvidia_count" -gt 1; then
            log "Multi-GPU NVIDIA détecté ($nvidia_count cartes)"
            printf 'multi-nvidia\n' >/run/winlux/hw/gpu-mode
        fi
    fi
    if test "$has_amd" = 1 && test "$has_intel" = 0 && test "$has_nvidia" = 0; then
        amd_count=0
        for dev in /sys/bus/pci/devices/*; do
            test -d "$dev" || continue
            vendor=$(cat "$dev/vendor" 2>/dev/null || echo "0x0000")
            class=$(cat "$dev/class" 2>/dev/null || echo "0x000000")
            case "$class" in 0x030000|0x030200) test "$vendor" = "0x1002" && amd_count=$((amd_count + 1)) ;; esac
        done
        if test "$amd_count" -gt 1; then
            log "Multi-GPU AMD détecté ($amd_count cartes)"
            printf 'multi-amd\n' >/run/winlux/hw/gpu-mode
        fi
    fi
}

# --- Génération de xorg.conf dynamique ---
generate_xorg_config() {
    has_nvidia=$(cat /run/winlux/hw/has-nvidia 2>/dev/null || echo 0)
    has_amd=$(cat /run/winlux/hw/has-amd 2>/dev/null || echo 0)
    has_intel=$(cat /run/winlux/hw/has-intel 2>/dev/null || echo 0)
    gpu_mode=$(cat /run/winlux/hw/gpu-mode 2>/dev/null || echo "")
    nvidia_driver=$(cat /run/winlux/hw/nvidia-driver 2>/dev/null || echo "")

    conf_dir=/usr/share/X11/xorg.conf.d
    mkdir -p "$conf_dir"

    # Supprimer l'ancien config modesetting statique (uniquement si on a un vrai GPU)
    gpu_count=$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)
    if test "$gpu_count" -gt 0; then
        rm -f "$conf_dir/20-winlux-display.conf"
    fi

    # Configuration NVIDIA
    if test "$has_nvidia" = 1; then
        if test "$nvidia_driver" = "nvidia"; then
            cat >"$conf_dir/20-winlux-nvidia.conf" <<'EOF'
Section "Device"
    Identifier "Winlux NVIDIA"
    Driver "nvidia"
    Option "NoLogo" "true"
EndSection
EOF
            log "Xorg configuré avec driver nvidia"
        else
            cat >"$conf_dir/20-winlux-nouveau.conf" <<'EOF'
Section "Device"
    Identifier "Winlux Nouveau"
    Driver "modesetting"
EndSection
EOF
            log "Xorg configuré avec modesetting (nouveau)"
        fi
    fi

    # PRIME offload Xorg (Intel + NVIDIA)
    if test "$gpu_mode" = "optimus" && test "$nvidia_driver" = "nvidia"; then
        cat >"$conf_dir/21-winlux-prime.conf" <<'EOF'
Section "ServerLayout"
    Identifier "Winlux PRIME"
    Screen 0 "Intel"
    Option "AllowNVIDIAGPUScreens"
EndSection

Section "Device"
    Identifier "Intel GPU"
    Driver "modesetting"
    BusID "PCI:0:2:0"
EndSection

Section "Screen"
    Identifier "Intel"
    Device "Intel GPU"
EndSection

Section "Device"
    Identifier "NVIDIA GPU"
    Driver "nvidia"
EndSection
EOF
        log "Xorg PRIME offload configuré"
    fi

    # Configuration AMD
    if test "$has_amd" = 1 && test "$has_nvidia" = 0; then
        cat >"$conf_dir/20-winlux-amd.conf" <<'EOF'
Section "Device"
    Identifier "Winlux AMD"
    Driver "modesetting"
    Option "TearFree" "true"
EndSection
EOF
        log "Xorg configuré pour AMD (modesetting + TearFree)"
    fi

    # Configuration Intel seul
    if test "$has_intel" = 1 && test "$has_nvidia" = 0 && test "$has_amd" = 0; then
        cat >"$conf_dir/20-winlux-intel.conf" <<'EOF'
Section "Device"
    Identifier "Winlux Intel"
    Driver "modesetting"
    Option "TearFree" "true"
EndSection
EOF
        log "Xorg configuré pour Intel (modesetting + TearFree)"
    fi
}

# --- Détection CPU / multi-CPU ---
detect_cpu() {
    cpu_count=$(grep -c '^processor' /proc/cpuinfo 2>/dev/null || echo 1)
    physical_count=$(grep 'physical id' /proc/cpuinfo 2>/dev/null | sort -u | wc -l)
    test "$physical_count" -gt 0 || physical_count=1
    cores_per_socket=$(grep 'cpu cores' /proc/cpuinfo 2>/dev/null | head -1 | awk -F: '{gsub(/[[:space:]]/, "", $2); print $2}')
    test -n "$cores_per_socket" || cores_per_socket=1
    threads_per_core=$((cpu_count / (physical_count * cores_per_socket)))
    test "$threads_per_core" -gt 0 || threads_per_core=1

    printf '%s\n' "$cpu_count" >/run/winlux/hw/cpu-threads
    printf '%s\n' "$physical_count" >/run/winlux/hw/cpu-sockets
    printf '%s\n' "$cores_per_socket" >/run/winlux/hw/cpu-cores-per-socket

    log "CPU : $physical_count socket(s), $cores_per_socket coeur(s)/socket, $cpu_count thread(s) total"

    # NUMA
    if test -d /sys/devices/system/node; then
        numa_nodes=$(ls -d /sys/devices/system/node/node* 2>/dev/null | wc -l)
        printf '%s\n' "$numa_nodes" >/run/winlux/hw/numa-nodes
        log "NUMA : $numa_nodes noeud(s)"

        for node_dir in /sys/devices/system/node/node*; do
            test -d "$node_dir" || continue
            node_id=$(basename "$node_dir" | sed 's/node//')
            cpulist=$(cat "$node_dir/cpulist" 2>/dev/null || echo "?")
            meminfo=$(awk '/MemTotal/ {print $4 " " $5}' "$node_dir/meminfo" 2>/dev/null || echo "? kB")
            log "  Noeud $node_id : CPUs=$cpulist, Mémoire=$meminfo"
        done
    fi

    # Fonctionnalités CPU
    flags=$(grep -m1 '^flags' /proc/cpuinfo 2>/dev/null | cut -d: -f2)
    features=""
    for f in sse3 ssse3 sse4_1 sse4_2 avx avx2 avx512f aes rdrand; do
        case " $flags " in *" $f "*) features="$features $f" ;; esac
    done
    printf '%s\n' "$features" >/run/winlux/hw/cpu-features
    log "Fonctionnalités CPU :$features"
}

# --- Configuration multi-CPU / SMP ---
configure_smp() {
    physical_count=$(cat /run/winlux/hw/cpu-sockets 2>/dev/null || echo 1)
    cpu_count=$(cat /run/winlux/hw/cpu-threads 2>/dev/null || echo 1)

    if test "$physical_count" -gt 1; then
        log "Système multi-socket détecté ($physical_count sockets)"

        # Activer irqbalance si disponible
        if command -v irqbalance >/dev/null 2>&1; then
            irqbalance --oneshot 2>/dev/null || true
            irqbalance --daemon 2>/dev/null || true
            log "irqbalance démarré pour multi-CPU"
        fi

        # Configurer le scheduler pour multi-socket
        if test -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor; then
            for gov in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
                test -w "$gov" && echo performance >"$gov" 2>/dev/null || true
            done
            log "Governor CPU réglé sur performance (multi-socket)"
        fi
    fi

    # Toujours activer le scheduler bandwidth pour les systèmes SMP
    if test "$cpu_count" -gt 1; then
        if test -w /proc/sys/kernel/sched_autogroup_enabled; then
            echo 1 >/proc/sys/kernel/sched_autogroup_enabled
            log "Autogroup scheduler activé"
        fi
    fi
}

# --- Réseau : Wi-Fi ---
detect_wifi() {
    wifi_found=0
    for dev in /sys/class/net/*; do
        test -d "$dev/wireless" || test -d "$dev/phy80211" || continue
        wifi_found=1
        iface=$(basename "$dev")
        log "Interface Wi-Fi détectée : $iface"
    done

    # Charger les pilotes Wi-Fi courants
    for m in iwlwifi iwlmvm ath9k ath10k_pci ath11k_pci rtw88_pci rtw89_pci \
             brcmfmac mt7921e rtl8xxxu rtl8192ce rtl8188ee; do
        load_module "$m"
    done

    # Re-scanner après chargement des modules
    if test "$wifi_found" = 0; then
        for dev in /sys/class/net/*; do
            test -d "$dev/wireless" || test -d "$dev/phy80211" || continue
            wifi_found=1
            iface=$(basename "$dev")
            log "Interface Wi-Fi détectée après chargement modules : $iface"
        done
    fi

    printf '%s\n' "$wifi_found" >/run/winlux/hw/has-wifi
    if test "$wifi_found" = 1; then
        # Firmware : demander à udev de charger les firmwares manquants
        if command -v udevadm >/dev/null 2>&1; then
            udevadm trigger --subsystem-match=net --action=add 2>/dev/null || true
            udevadm settle --timeout=5 2>/dev/null || true
        fi
    fi
}

# --- Réseau : Ethernet ---
detect_ethernet() {
    for m in e1000e igb igc ixgbe i40e r8169 r8125 tg3 bnxt_en mlx5_core \
             atlantic virtio_net; do
        load_module "$m"
    done
    log "Pilotes Ethernet chargés"
}

# --- Bluetooth ---
detect_bluetooth() {
    for m in bluetooth btusb btintel btrtl btbcm bnep rfcomm; do
        load_module "$m"
    done

    bt_found=0
    if test -d /sys/class/bluetooth && test -n "$(ls -A /sys/class/bluetooth/ 2>/dev/null)"; then
        bt_found=1
        for adapter in /sys/class/bluetooth/*; do
            log "Adaptateur Bluetooth détecté : $(basename "$adapter")"
        done
    fi
    printf '%s\n' "$bt_found" >/run/winlux/hw/has-bluetooth
}

# --- Audio ---
detect_audio() {
    for m in snd_hda_intel snd_hda_codec_realtek snd_hda_codec_hdmi \
             snd_usb_audio snd_intel8x0 snd_sof_pci_intel_tgl \
             snd_sof_pci_intel_cnl snd_sof_pci_intel_icl; do
        load_module "$m"
    done

    audio_found=0
    if test -d /proc/asound && test -n "$(ls /proc/asound/card* 2>/dev/null)"; then
        audio_found=1
        for card in /proc/asound/card*; do
            test -d "$card" || continue
            name=$(cat "$card/id" 2>/dev/null || echo "inconnu")
            log "Carte audio détectée : $name"
        done
    fi
    printf '%s\n' "$audio_found" >/run/winlux/hw/has-audio

    # PipeWire ou PulseAudio
    if command -v pipewire >/dev/null 2>&1; then
        printf 'pipewire\n' >/run/winlux/hw/audio-server
        log "Serveur audio : PipeWire"
    elif command -v pulseaudio >/dev/null 2>&1; then
        printf 'pulseaudio\n' >/run/winlux/hw/audio-server
        log "Serveur audio : PulseAudio"
    else
        printf 'alsa\n' >/run/winlux/hw/audio-server
        log "Serveur audio : ALSA direct"
    fi
}

# --- Périphériques d'entrée ---
detect_input() {
    # Touchpad
    for m in i2c_hid_acpi i2c_hid hid_multitouch; do
        load_module "$m"
    done

    touchpad_found=0
    for dev in /sys/class/input/event*; do
        test -d "$dev/device" || continue
        name=$(cat "$dev/device/name" 2>/dev/null || echo "")
        case "$name" in
            *[Tt]ouchpad*|*[Tt]rackpad*|*Synaptics*|*ELAN*|*Cypress*|*ALPS*)
                touchpad_found=1
                log "Touchpad détecté : $name"
                ;;
        esac
    done
    printf '%s\n' "$touchpad_found" >/run/winlux/hw/has-touchpad

    # Écran tactile
    touch_found=0
    for dev in /sys/class/input/event*; do
        test -d "$dev/device" || continue
        caps=$(cat "$dev/device/capabilities/abs" 2>/dev/null || echo "0")
        name=$(cat "$dev/device/name" 2>/dev/null || echo "")
        case "$name" in *[Tt]ouch[Ss]creen*|*[Dd]igitizer*)
            touch_found=1
            log "Écran tactile détecté : $name"
            ;;
        esac
    done
    printf '%s\n' "$touch_found" >/run/winlux/hw/has-touchscreen
}

# --- Webcam ---
detect_webcam() {
    load_module uvcvideo
    webcam_found=0
    for dev in /sys/class/video4linux/video*; do
        test -d "$dev" || continue
        name=$(cat "$dev/name" 2>/dev/null || echo "")
        case "$name" in ""|*[Mm]etadata*) continue ;; esac
        webcam_found=1
        log "Webcam détectée : $name"
    done
    printf '%s\n' "$webcam_found" >/run/winlux/hw/has-webcam
}

# --- Lecteur d'empreintes ---
detect_fingerprint() {
    fp_found=0
    if command -v fprintd-list >/dev/null 2>&1; then
        fp_found=1
        log "Service d'empreintes digitales disponible (fprintd)"
    fi
    # Vérifier les périphériques USB connus
    for dev in /sys/bus/usb/devices/*/idVendor; do
        test -r "$dev" || continue
        vendor=$(cat "$dev")
        case "$vendor" in
            138a|06cb|04f3|1c7a|27c6|2808)
                fp_found=1
                log "Lecteur d'empreintes USB détecté (vendor=$vendor)"
                ;;
        esac
    done
    printf '%s\n' "$fp_found" >/run/winlux/hw/has-fingerprint
}

# --- Batterie et alimentation ---
detect_power() {
    battery_found=0
    for bat in /sys/class/power_supply/BAT*; do
        test -d "$bat" || continue
        battery_found=1
        status=$(cat "$bat/status" 2>/dev/null || echo "inconnu")
        capacity=$(cat "$bat/capacity" 2>/dev/null || echo "?")
        log "Batterie détectée : $(basename "$bat"), état=$status, capacité=${capacity}%"
    done
    printf '%s\n' "$battery_found" >/run/winlux/hw/has-battery

    # Charger les modules ACPI pour la gestion de l'alimentation
    for m in ac battery button thermal fan processor; do
        load_module "$m"
    done

    # Gestion thermique
    if test -d /sys/class/thermal; then
        for zone in /sys/class/thermal/thermal_zone*; do
            test -d "$zone" || continue
            type=$(cat "$zone/type" 2>/dev/null || echo "inconnu")
            temp=$(cat "$zone/temp" 2>/dev/null || echo 0)
            temp_c=$((temp / 1000))
            log "Zone thermique : $type = ${temp_c}°C"
        done
    fi
}

# --- Stockage ---
detect_storage() {
    for m in nvme ahci sd_mod uas usb_storage mmc_core sdhci sdhci_pci; do
        load_module "$m"
    done

    for disk in /sys/block/sd* /sys/block/nvme* /sys/block/mmcblk*; do
        test -d "$disk" || continue
        name=$(basename "$disk")
        model=$(cat "$disk/device/model" 2>/dev/null | tr -d '[:space:]' || echo "inconnu")
        size_sectors=$(cat "$disk/size" 2>/dev/null || echo 0)
        size_gb=$((size_sectors * 512 / 1073741824))
        rotational=$(cat "$disk/queue/rotational" 2>/dev/null || echo "?")
        case "$rotational" in 0) type="SSD" ;; 1) type="HDD" ;; *) type="inconnu" ;; esac
        log "Stockage : $name ($model), ${size_gb} Gio, type=$type"
    done
}

# --- Firmware ---
load_firmware() {
    if test -d /lib/firmware; then
        log "Répertoire firmware disponible : /lib/firmware"
        if command -v udevadm >/dev/null 2>&1; then
            udevadm trigger --action=add 2>/dev/null || true
            udevadm settle --timeout=10 2>/dev/null || true
            log "Firmware rechargé via udev"
        fi
    else
        log "ATTENTION : /lib/firmware absent — certains périphériques peuvent ne pas fonctionner"
    fi
}

# --- Rapport final ---
hardware_summary() {
    log "=== Résumé matériel ==="
    log "GPU : $(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0) carte(s)"
    test "$(cat /run/winlux/hw/has-nvidia 2>/dev/null)" = 1 && log "  NVIDIA (driver=$(cat /run/winlux/hw/nvidia-driver 2>/dev/null || echo ?))"
    test "$(cat /run/winlux/hw/has-amd 2>/dev/null)" = 1 && log "  AMD (driver=$(cat /run/winlux/hw/amd-driver 2>/dev/null || echo ?))"
    test "$(cat /run/winlux/hw/has-intel 2>/dev/null)" = 1 && log "  Intel (driver=$(cat /run/winlux/hw/intel-driver 2>/dev/null || echo ?))"
    test "$(cat /run/winlux/hw/gpu-count 2>/dev/null || echo 0)" -gt 1 && log "  Mode GPU : $(cat /run/winlux/hw/gpu-mode 2>/dev/null || echo standard)"
    log "CPU : $(cat /run/winlux/hw/cpu-sockets 2>/dev/null || echo 1) socket(s), $(cat /run/winlux/hw/cpu-threads 2>/dev/null || echo ?) thread(s)"
    test "$(cat /run/winlux/hw/numa-nodes 2>/dev/null || echo 0)" -gt 1 && log "  NUMA : $(cat /run/winlux/hw/numa-nodes) noeud(s)"
    log "Wi-Fi : $(test "$(cat /run/winlux/hw/has-wifi 2>/dev/null)" = 1 && echo oui || echo non)"
    log "Bluetooth : $(test "$(cat /run/winlux/hw/has-bluetooth 2>/dev/null)" = 1 && echo oui || echo non)"
    log "Audio : $(test "$(cat /run/winlux/hw/has-audio 2>/dev/null)" = 1 && echo oui || echo non) ($(cat /run/winlux/hw/audio-server 2>/dev/null || echo ?))"
    log "Touchpad : $(test "$(cat /run/winlux/hw/has-touchpad 2>/dev/null)" = 1 && echo oui || echo non)"
    log "Batterie : $(test "$(cat /run/winlux/hw/has-battery 2>/dev/null)" = 1 && echo oui || echo non)"
    log "=== Fin du résumé ==="
}

# --- Exécution principale ---
main() {
    enumerate_pci
    detect_gpu
    configure_gpu
    detect_cpu
    configure_smp
    detect_wifi
    detect_ethernet
    detect_bluetooth
    detect_audio
    detect_input
    detect_webcam
    detect_fingerprint
    detect_power
    detect_storage
    load_firmware
    generate_xorg_config
    hardware_summary
    log "Détection matérielle terminée"
}

main "$@"
