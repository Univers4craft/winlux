#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
iso=${1:-"$root/build/winlux-live-test.iso"}
mode=${2:-bios}
timeout_seconds=${WINLUX_QEMU_TIMEOUT:-180}
log=${WINLUX_QEMU_LOG:-"$root/build/qemu-$mode.log"}

command -v qemu-system-x86_64 >/dev/null 2>&1 || {
    echo "qemu-system-x86_64 est requis" >&2
    exit 1
}
command -v timeout >/dev/null 2>&1 || { echo "timeout est requis" >&2; exit 1; }
test -f "$iso" || { echo "ISO absente : $iso" >&2; exit 1; }
mkdir -p "$(dirname -- "$log")"

# UEFI 32 bits : processeur sans pagination à 5 niveaux (LA57), comme les
# Atom concernés. Avec LA57, le noyau 64 bits plante dans les services UEFI en
# mode mixte (page NX), un cas qui n'existe que sous QEMU « -cpu max ».
cpu=${WINLUX_QEMU_CPU:-max}
if test "$mode" = uefi32 && test -z "${WINLUX_QEMU_CPU:-}"; then cpu=max,la57=off; fi
set -- qemu-system-x86_64 \
    -m "${WINLUX_QEMU_MEMORY:-2048}" \
    -smp "${WINLUX_QEMU_CPUS:-2}" -cpu "$cpu" \
    -accel tcg \
    -display none \
    -nic none \
    -monitor none \
    -serial stdio \
    -no-reboot \
    -cdrom "$iso"

case "$mode" in
    bios) ;;
    uefi|uefi32)
        if test "$mode" = uefi32; then
            # Processeur 64 bits, micrologiciel UEFI 32 bits (tablettes Atom) : BOOTIA32.EFI.
            ovmf=${WINLUX_OVMF32:-/usr/share/OVMF/OVMF32_CODE_4M.fd}
            ovmf_vars=${WINLUX_OVMF32_VARS:-/usr/share/OVMF/OVMF32_VARS_4M.fd}
        else
            ovmf=${WINLUX_OVMF:-/usr/share/OVMF/OVMF_CODE_4M.fd}
            ovmf_vars=${WINLUX_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
        fi
        test -r "$ovmf" || {
            echo "firmware UEFI absent ou illisible : $ovmf" >&2
            exit 1
        }
        test -r "$ovmf_vars" || {
            echo "variables UEFI absentes ou illisibles : $ovmf_vars" >&2
            exit 1
        }
        vars_copy="$root/build/OVMF_VARS_4M.$mode.test.fd"
        cp "$ovmf_vars" "$vars_copy"
        set -- "$@" \
            -machine q35 \
            -drive "if=pflash,format=raw,unit=0,readonly=on,file=$ovmf" \
            -drive "if=pflash,format=raw,unit=1,file=$vars_copy"
        ;;
    *)
        echo "mode inconnu : $mode (attendu : bios, uefi ou uefi32)" >&2
        exit 1
        ;;
esac

echo "Test QEMU $mode de $iso"
set +e
# Select GRUB's diagnostic hotkey while the hidden menu is waiting. Repeating
# covers BIOS and slower UEFI firmware without changing the delivered ISO;
# every 0.1 s so a key always lands inside GRUB's short countdown.
(
    for attempt in $(seq 1 400); do printf d || exit 0; sleep 0.1; done
) | timeout "$timeout_seconds" "$@" >"$log" 2>&1
qemu_status=$?
set -e

if [ "$qemu_status" -ne 0 ]; then
    echo "QEMU a échoué (code $qemu_status), journal : $log" >&2
    tail -100 "$log" >&2
    exit 1
fi

for marker in \
    WINLUX_LIVE_SEQUENTIAL_EXIT=0 \
    WINLUX_LIVE_PARALLEL_EXIT=0 \
    WINLUX_LIVE_X11_EXIT=0 \
    WINLUX_LIVE_RESULT=0
do
    if ! grep -q "$marker" "$log"; then
        echo "marqueur absent : $marker (journal : $log)" >&2
        grep -A20 'WINLUX_LIVE_TERMINAL_FAILED' "$log" >&2 || true
        tail -100 "$log" >&2
        exit 1
    fi
done

echo "ISO validée sous QEMU ($mode), journal : $log"
