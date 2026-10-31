#!/bin/sh
# Winlux hardware detection — outputs machine-readable inventory for the installer.
# Works in minimal live environments (busybox, no lspci/lsusb guaranteed).
set -u
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

has() { command -v "$1" >/dev/null 2>&1; }
read_sys() { test -r "$1" && cat "$1" 2>/dev/null | tr '\0' ' ' | sed 's/[[:space:]]*$//'; }
quote() { printf '%s\n' "$1" | sed "s/'/'\\\\''/g; s/.*/'&'/"; }

# ── CPU ─────────────────────────────────────────────────────────────────
echo '[cpu]'

cpu_model=$(read_sys /proc/cpuinfo | awk -F': ' '/^model name/ {print $2; exit}')
test -n "$cpu_model" || cpu_model="unknown"
printf 'CPU_MODEL=%s\n' "$cpu_model"

cpu_arch=$(uname -m 2>/dev/null || echo unknown)
printf 'CPU_ARCH=%s\n' "$cpu_arch"

# Physical sockets
sockets=0
if test -d /sys/devices/system/cpu; then
    sockets=$(awk -F: '/^physical id/ {print $2}' /proc/cpuinfo | sort -u | wc -l)
    test "$sockets" -gt 0 2>/dev/null || sockets=1
fi
printf 'CPU_SOCKETS=%d\n' "$sockets"

# Physical cores (total across sockets)
physical_cores=$(awk -F: '/^core id/ {print $2}' /proc/cpuinfo | sort -u | wc -l)
test "$physical_cores" -gt 0 2>/dev/null || physical_cores=1
# Account for multi-socket: each socket has its own core-id space
if test "$sockets" -gt 1; then
    physical_cores=$(awk -F': ' '/^physical id|^core id/ {print $2}' /proc/cpuinfo |
        paste - - | sort -u | wc -l)
    test "$physical_cores" -gt 0 2>/dev/null || physical_cores=1
fi
printf 'CPU_CORES_PHYSICAL=%d\n' "$physical_cores"

logical_cores=$(nproc 2>/dev/null || grep -c '^processor' /proc/cpuinfo 2>/dev/null || echo 1)
printf 'CPU_CORES_LOGICAL=%d\n' "$logical_cores"

if test "$logical_cores" -gt "$physical_cores" 2>/dev/null && test "$physical_cores" -gt 0; then
    printf 'CPU_HYPERTHREADING=yes\n'
else
    printf 'CPU_HYPERTHREADING=no\n'
fi

flags=$(awk -F': ' '/^flags/ {print $2; exit}' /proc/cpuinfo)
for feature in sse sse2 sse3 ssse3 sse4_1 sse4_2 avx avx2 avx512f aes nx; do
    if echo " $flags " | grep -qw "$feature"; then
        printf 'CPU_FEATURE_%s=yes\n' "$(echo "$feature" | tr '[:lower:]' '[:upper:]')"
    fi
done

# ── RAM ─────────────────────────────────────────────────────────────────
echo ''
echo '[ram]'

mem_total_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo)
mem_available_kb=$(awk '/^MemAvailable:/ {print $2}' /proc/meminfo)
test -n "$mem_total_kb" || mem_total_kb=0
test -n "$mem_available_kb" || mem_available_kb=0
mem_total_mb=$((mem_total_kb / 1024))
mem_available_mb=$((mem_available_kb / 1024))
printf 'RAM_TOTAL_MB=%d\n' "$mem_total_mb"
printf 'RAM_AVAILABLE_MB=%d\n' "$mem_available_mb"

# DIMM slot count (requires dmidecode, often absent in live)
if has dmidecode; then
    slots=$(dmidecode -t 17 2>/dev/null | grep -c 'Memory Device' || echo 0)
    populated=$(dmidecode -t 17 2>/dev/null | grep 'Size:' | grep -cv 'No Module Installed' || echo 0)
    printf 'RAM_SLOTS_TOTAL=%d\n' "$slots"
    printf 'RAM_SLOTS_POPULATED=%d\n' "$populated"
fi

# ── GPU ─────────────────────────────────────────────────────────────────
echo ''
echo '[gpu]'

gpu_index=0
# Walk PCI devices in sysfs — works without lspci
for dev in /sys/bus/pci/devices/*; do
    test -r "$dev/class" || continue
    class=$(read_sys "$dev/class")
    # VGA (0x030000) or 3D controller (0x030200)
    case "$class" in
        0x0300*|0x0302*) ;;
        *) continue ;;
    esac
    vendor_id=$(read_sys "$dev/vendor")
    device_id=$(read_sys "$dev/device")
    case "$vendor_id" in
        0x10de) vendor_name=NVIDIA ;;
        0x1002) vendor_name=AMD ;;
        0x8086) vendor_name=Intel ;;
        *) vendor_name="unknown($vendor_id)" ;;
    esac
    driver=
    if test -L "$dev/driver"; then
        driver=$(basename "$(readlink "$dev/driver")")
    fi
    # Try to get a human-readable name from lspci
    model=
    if has lspci; then
        pci_slot=$(basename "$dev")
        model=$(lspci -s "$pci_slot" 2>/dev/null | sed 's/^[^ ]* //')
    fi
    test -n "$model" || model="$vendor_name $device_id"

    # VRAM: NVIDIA exposes via resource sizes, DRM exposes mem_info_vram_total
    vram_mb=
    drm_dir=
    for card in "$dev"/drm/card*; do
        test -d "$card" && drm_dir="$card" && break
    done
    if test -n "$drm_dir"; then
        vram_file="$dev/mem_info_vram_total"
        if test -r "$vram_file"; then
            vram_bytes=$(read_sys "$vram_file")
            test -n "$vram_bytes" && vram_mb=$((vram_bytes / 1048576))
        fi
    fi
    # Fallback: parse BAR sizes from the resource file
    if test -z "$vram_mb" && test -r "$dev/resource"; then
        max_bar=0
        while IFS=' ' read -r start end flags; do
            start_dec=$((start))
            end_dec=$((end))
            test "$end_dec" -gt "$start_dec" 2>/dev/null || continue
            bar_size=$((end_dec - start_dec + 1))
            test "$bar_size" -gt "$max_bar" && max_bar=$bar_size
        done <"$dev/resource"
        test "$max_bar" -gt 0 && vram_mb=$((max_bar / 1048576))
    fi

    printf 'GPU_%d_VENDOR=%s\n' "$gpu_index" "$vendor_name"
    printf 'GPU_%d_VENDOR_ID=%s\n' "$gpu_index" "$vendor_id"
    printf 'GPU_%d_DEVICE_ID=%s\n' "$gpu_index" "$device_id"
    printf 'GPU_%d_MODEL=%s\n' "$gpu_index" "$model"
    printf 'GPU_%d_DRIVER=%s\n' "$gpu_index" "${driver:-none}"
    test -n "$vram_mb" && printf 'GPU_%d_VRAM_MB=%d\n' "$gpu_index" "$vram_mb"
    gpu_index=$((gpu_index + 1))
done
printf 'GPU_COUNT=%d\n' "$gpu_index"

# ── Storage ─────────────────────────────────────────────────────────────
echo ''
echo '[disk]'

disk_index=0
if has lsblk; then
    lsblk -bdnro NAME,SIZE,MODEL,TRAN,TYPE,ROTA,RM 2>/dev/null | while IFS= read -r line; do
        set -- $line
        name=$1 size=${2:-0} model=${3:-} tran=${4:-} dtype=${5:-} rota=${6:-1} removable=${7:-0}
        case "$dtype" in disk|loop) ;; *) continue ;; esac
        # Skip tiny loop devices (likely snap mounts)
        test "$dtype" = loop && test "$size" -lt 1073741824 2>/dev/null && continue
        size_gb=$((size / 1073741824))
        case "$rota" in
            0) media=SSD ;;
            1) media=HDD ;;
            *) media=unknown ;;
        esac
        test "$tran" = "nvme" && media=NVMe
        printf 'DISK_%d_NAME=%s\n' "$disk_index" "$name"
        printf 'DISK_%d_SIZE_GB=%d\n' "$disk_index" "$size_gb"
        printf 'DISK_%d_SIZE_BYTES=%s\n' "$disk_index" "$size"
        printf 'DISK_%d_MODEL=%s\n' "$disk_index" "$model"
        printf 'DISK_%d_TRANSPORT=%s\n' "$disk_index" "${tran:-unknown}"
        printf 'DISK_%d_MEDIA=%s\n' "$disk_index" "$media"
        printf 'DISK_%d_REMOVABLE=%s\n' "$disk_index" "$removable"
        # List partitions
        part_idx=0
        lsblk -bnro NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT "/dev/$name" 2>/dev/null | while IFS= read -r pline; do
            set -- $pline
            pname=$1
            test "$pname" = "$name" && continue
            psize=${2:-0} pfs=${3:-} plabel=${4:-} pmount=${5:-}
            printf 'DISK_%d_PART_%d_NAME=%s\n' "$disk_index" "$part_idx" "$pname"
            printf 'DISK_%d_PART_%d_SIZE_BYTES=%s\n' "$disk_index" "$part_idx" "$psize"
            printf 'DISK_%d_PART_%d_FSTYPE=%s\n' "$disk_index" "$part_idx" "$pfs"
            printf 'DISK_%d_PART_%d_LABEL=%s\n' "$disk_index" "$part_idx" "$plabel"
            printf 'DISK_%d_PART_%d_MOUNT=%s\n' "$disk_index" "$part_idx" "$pmount"
            part_idx=$((part_idx + 1))
        done
        disk_index=$((disk_index + 1))
    done
else
    for dev in /sys/block/*; do
        name=$(basename "$dev")
        case "$name" in loop*|ram*|dm-*) continue ;; esac
        size_sectors=$(read_sys "$dev/size")
        test -n "$size_sectors" || continue
        size_bytes=$((size_sectors * 512))
        test "$size_bytes" -gt 0 || continue
        size_gb=$((size_bytes / 1073741824))
        model=$(read_sys "$dev/device/model")
        rota=$(read_sys "$dev/queue/rotational")
        case "$rota" in 0) media=SSD ;; *) media=HDD ;; esac
        case "$name" in nvme*) media=NVMe ;; esac
        printf 'DISK_%d_NAME=%s\n' "$disk_index" "$name"
        printf 'DISK_%d_SIZE_GB=%d\n' "$disk_index" "$size_gb"
        printf 'DISK_%d_MODEL=%s\n' "$disk_index" "${model:-unknown}"
        printf 'DISK_%d_MEDIA=%s\n' "$disk_index" "$media"
        disk_index=$((disk_index + 1))
    done
fi
printf 'DISK_COUNT=%d\n' "$disk_index"

# ── Network ─────────────────────────────────────────────────────────────
echo ''
echo '[network]'

net_index=0
for iface_path in /sys/class/net/*; do
    iface=$(basename "$iface_path")
    case "$iface" in lo) continue ;; esac
    iface_type=$(read_sys "$iface_path/type")
    # type 1 = Ethernet/Wi-Fi, skip others (tunnels, bridges internal)
    case "$iface_type" in 1) ;; *) continue ;; esac
    mac=$(read_sys "$iface_path/address")
    driver=
    if test -L "$iface_path/device/driver"; then
        driver=$(basename "$(readlink "$iface_path/device/driver")")
    fi
    operstate=$(read_sys "$iface_path/operstate")
    speed=$(read_sys "$iface_path/speed" 2>/dev/null)
    # Detect Wi-Fi: presence of wireless/ subdirectory
    if test -d "$iface_path/wireless" || test -d "$iface_path/phy80211"; then
        kind=wifi
    else
        kind=ethernet
    fi
    printf 'NET_%d_NAME=%s\n' "$net_index" "$iface"
    printf 'NET_%d_TYPE=%s\n' "$net_index" "$kind"
    printf 'NET_%d_MAC=%s\n' "$net_index" "${mac:-unknown}"
    printf 'NET_%d_DRIVER=%s\n' "$net_index" "${driver:-unknown}"
    printf 'NET_%d_STATE=%s\n' "$net_index" "${operstate:-unknown}"
    test -n "$speed" && test "$speed" != "-1" && printf 'NET_%d_SPEED_MBPS=%s\n' "$net_index" "$speed"
    net_index=$((net_index + 1))
done
printf 'NET_COUNT=%d\n' "$net_index"

# ── Audio ───────────────────────────────────────────────────────────────
echo ''
echo '[audio]'

audio_index=0
if test -r /proc/asound/cards; then
    while IFS= read -r line; do
        case "$line" in
            [[:space:]]*[0-9]*)
                card_num=$(echo "$line" | awk '{print $1}')
                card_name=$(echo "$line" | sed 's/^[[:space:]]*[0-9]*[[:space:]]*\[//;s/\].*$//')
                desc_line=
                IFS= read -r desc_line || true
                desc=$(echo "$desc_line" | sed 's/^[[:space:]]*//')
                printf 'AUDIO_%d_NAME=%s\n' "$audio_index" "$card_name"
                printf 'AUDIO_%d_DESCRIPTION=%s\n' "$audio_index" "$desc"
                audio_index=$((audio_index + 1))
                ;;
        esac
    done </proc/asound/cards
fi
printf 'AUDIO_COUNT=%d\n' "$audio_index"

# ── Battery ─────────────────────────────────────────────────────────────
echo ''
echo '[battery]'

bat_found=0
for bat in /sys/class/power_supply/BAT*; do
    test -d "$bat" || continue
    bat_found=1
    bat_name=$(basename "$bat")
    status=$(read_sys "$bat/status")
    capacity=$(read_sys "$bat/capacity")
    energy_full=$(read_sys "$bat/energy_full")
    energy_design=$(read_sys "$bat/energy_full_design")
    printf 'BATTERY_%s_STATUS=%s\n' "$bat_name" "${status:-unknown}"
    printf 'BATTERY_%s_CAPACITY_PCT=%s\n' "$bat_name" "${capacity:-unknown}"
    test -n "$energy_full" && printf 'BATTERY_%s_ENERGY_FULL_UWH=%s\n' "$bat_name" "$energy_full"
    test -n "$energy_design" && printf 'BATTERY_%s_ENERGY_DESIGN_UWH=%s\n' "$bat_name" "$energy_design"
done
if test "$bat_found" = 0; then
    printf 'BATTERY_PRESENT=no\n'
else
    printf 'BATTERY_PRESENT=yes\n'
fi

# AC adapter
for ac in /sys/class/power_supply/AC* /sys/class/power_supply/ADP*; do
    test -d "$ac" || continue
    online=$(read_sys "$ac/online")
    printf 'AC_ONLINE=%s\n' "${online:-unknown}"
    break
done

# ── Firmware / Boot mode ────────────────────────────────────────────────
echo ''
echo '[firmware]'

if test -d /sys/firmware/efi; then
    printf 'BOOT_MODE=UEFI\n'
    # Secure Boot
    sb_file=/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c
    if test -r "$sb_file"; then
        # Last byte: 1 = enabled, 0 = disabled
        sb_val=$(od -An -tx1 -j4 -N1 "$sb_file" 2>/dev/null | tr -d ' ')
        case "$sb_val" in
            01) printf 'SECURE_BOOT=enabled\n' ;;
            00) printf 'SECURE_BOOT=disabled\n' ;;
            *) printf 'SECURE_BOOT=unknown\n' ;;
        esac
    else
        printf 'SECURE_BOOT=unknown\n'
    fi
    # EFI bitness
    efi_fw_platform=/sys/firmware/efi/fw_platform_size
    if test -r "$efi_fw_platform"; then
        printf 'EFI_BITNESS=%s\n' "$(read_sys "$efi_fw_platform")"
    fi
else
    printf 'BOOT_MODE=BIOS\n'
    printf 'SECURE_BOOT=not_applicable\n'
fi

# ── Display ─────────────────────────────────────────────────────────────
echo ''
echo '[display]'

disp_index=0
# DRM connectors in sysfs
for conn in /sys/class/drm/card*-*; do
    test -d "$conn" || continue
    conn_name=$(basename "$conn")
    status=$(read_sys "$conn/status")
    case "$status" in connected) ;; *) continue ;; esac
    # Read EDID-derived modes
    modes_file="$conn/modes"
    preferred_mode=
    if test -r "$modes_file"; then
        preferred_mode=$(head -n1 "$modes_file")
    fi
    printf 'DISPLAY_%d_CONNECTOR=%s\n' "$disp_index" "$conn_name"
    printf 'DISPLAY_%d_STATUS=%s\n' "$disp_index" "$status"
    test -n "$preferred_mode" && printf 'DISPLAY_%d_PREFERRED_MODE=%s\n' "$disp_index" "$preferred_mode"
    disp_index=$((disp_index + 1))
done

# Fallback: xrandr if DRM sysfs gave nothing and X is running
if test "$disp_index" = 0 && has xrandr && test -n "${DISPLAY:-}"; then
    xrandr --current 2>/dev/null | while IFS= read -r line; do
        case "$line" in
            *" connected "*)
                conn_name=$(echo "$line" | awk '{print $1}')
                resolution=$(echo "$line" | grep -o '[0-9]*x[0-9]*' | head -n1)
                printf 'DISPLAY_%d_CONNECTOR=%s\n' "$disp_index" "$conn_name"
                printf 'DISPLAY_%d_STATUS=connected\n' "$disp_index"
                test -n "$resolution" && printf 'DISPLAY_%d_PREFERRED_MODE=%s\n' "$disp_index" "$resolution"
                disp_index=$((disp_index + 1))
                ;;
        esac
    done
fi
printf 'DISPLAY_COUNT=%d\n' "$disp_index"

# ── System summary ──────────────────────────────────────────────────────
echo ''
echo '[system]'

hostname=$(hostname 2>/dev/null || read_sys /proc/sys/kernel/hostname)
printf 'HOSTNAME=%s\n' "${hostname:-unknown}"

product=$(read_sys /sys/class/dmi/id/product_name)
test -n "$product" && printf 'PRODUCT_NAME=%s\n' "$product"

manufacturer=$(read_sys /sys/class/dmi/id/sys_vendor)
test -n "$manufacturer" && printf 'MANUFACTURER=%s\n' "$manufacturer"

bios_vendor=$(read_sys /sys/class/dmi/id/bios_vendor)
test -n "$bios_vendor" && printf 'BIOS_VENDOR=%s\n' "$bios_vendor"

bios_version=$(read_sys /sys/class/dmi/id/bios_version)
test -n "$bios_version" && printf 'BIOS_VERSION=%s\n' "$bios_version"

kernel_ver=$(uname -r 2>/dev/null)
printf 'KERNEL=%s\n' "${kernel_ver:-unknown}"
