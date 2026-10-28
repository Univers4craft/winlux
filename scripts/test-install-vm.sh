#!/bin/sh
# Test privilégié facultatif : uniquement un nouveau disque image, jamais un SSD.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
iso="$root/build/winlux-live-test.iso"
keep=0
while test $# -gt 0; do
    case "$1" in
        --iso) test $# -ge 2 || exit 1; iso=$2; shift 2 ;;
        --keep-disk) keep=1; shift ;;
        *) echo "option inconnue : $1" >&2; exit 1 ;;
    esac
done
test "$(id -u)" -eq 0 || { echo 'root requis pour les loop/montages de ce test' >&2; exit 1; }
test -r "$iso" || { echo 'ISO absente' >&2; exit 1; }
for tool in qemu-system-x86_64 truncate losetup timeout; do command -v "$tool" >/dev/null; done
work=$(mktemp -d /tmp/winlux-install-vm.XXXXXX)
disk="$work/test.img"
loop_dev=
cleanup() {
    if test -n "$loop_dev"; then losetup -d "$loop_dev" || return; fi
    if test "$keep" = 0; then rm -rf -- "$work"; else echo "Disque et journaux conservés : $work"; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
truncate -s 4G "$disk"
loop_dev=$(losetup --find --show --partscan "$disk")
# L'installateur lui-même vérifie le backing-file, le swap et les montages.
sh "$root/scripts/install.sh" "$loop_dev" --iso "$iso" --test-image "$disk"
losetup -d "$loop_dev"
loop_dev=
for mode in bios uefi; do
    log="$work/$mode.log"
    set -- qemu-system-x86_64 -m 2048 -smp 2 -accel tcg -display none -monitor none \
        -serial stdio -nic none -no-reboot -drive "file=$disk,format=raw,if=virtio"
    if test "$mode" = uefi; then
        ovmf=${WINLUX_OVMF:-/usr/share/OVMF/OVMF_CODE_4M.fd}
        vars=${WINLUX_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
        cp "$vars" "$work/vars.fd"
        set -- "$@" -machine q35 -drive "if=pflash,format=raw,unit=0,readonly=on,file=$ovmf" \
            -drive "if=pflash,format=raw,unit=1,file=$work/vars.fd"
    fi
    # Touche diagnostic ; le bureau n'est pas lancé et le test s'éteint lui-même.
    set +e
    (for attempt in $(seq 1 80); do printf d || exit 0; sleep 0.5; done) | \
        timeout "${WINLUX_QEMU_TIMEOUT:-180}" "$@" >"$log" 2>&1
    status=$?
    set -e
    test "$status" = 0 || { tail -n 50 "$log"; exit 1; }
    for marker in WINLUX_SYSTEM_VERSION= WINLUX_LIVE_SEQUENTIAL_EXIT=0 WINLUX_LIVE_PARALLEL_EXIT=0 WINLUX_LIVE_X11_EXIT=0 WINLUX_LIVE_RESULT=0; do
        grep -q "$marker" "$log" || { echo "$mode : marqueur absent $marker" >&2; tail -n 50 "$log"; exit 1; }
    done
    echo "Installation disque validée en $mode"
done
