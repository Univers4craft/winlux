#!/bin/sh
# Boots a minimal ISO built by winlux-secureboot in QEMU with OVMF Secure Boot
# enforced (Microsoft keys enrolled): shim -> signed GRUB -> Winlux grub.cfg -> signed kernel.
# Paquets : qemu-system-x86 ovmf shim-signed grub-efi-amd64-signed grub-pc-bin mtools dosfstools xorriso busybox-static
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
kernel=${WINLUX_KERNEL:-/boot/vmlinuz}
code=${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.secboot.fd}
vars_template=${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.ms.fd}
for path in "$kernel" "$code" "$vars_template" /usr/bin/busybox; do
    test -r "$path" || { echo "requis : $path" >&2; exit 1; }
done
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
iso_root="$work/iso"
mkdir -p "$iso_root/boot/grub" "$work/initrd/bin" "$work/initrd/proc" "$work/initrd/sys"
cp -L "$kernel" "$iso_root/boot/vmlinuz"
: >"$iso_root/boot/winlux-rootfs.squashfs"
cp /usr/bin/busybox "$work/initrd/bin/busybox"
cat >"$work/initrd/init" <<'EOF'
#!/bin/busybox sh
/bin/busybox mount -t proc proc /proc
/bin/busybox mount -t sysfs sysfs /sys
/bin/busybox mount -t efivarfs efivarfs /sys/firmware/efi/efivars 2>/dev/null
state=$(/bin/busybox dd if=/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c bs=1 skip=4 count=1 2>/dev/null | /bin/busybox od -An -t u1 | /bin/busybox tr -d ' ')
echo "WINLUX_SB_KERNEL_OK secureboot=${state:-absent} cmdline=$(/bin/busybox cat /proc/cmdline)"
/bin/busybox poweroff -f
EOF
chmod 755 "$work/initrd/init" "$work/initrd/bin/busybox"
(cd "$work/initrd" && find . | cpio --create --format=newc --quiet | gzip -9) >"$iso_root/boot/winlux-initramfs.cpio.gz"
awk "/<<'GRUB'/{f=1;next} /^GRUB\$/{f=0} f" "$root/scripts/build-live-iso.sh" >"$iso_root/boot/grub/grub.cfg"
grub-script-check "$iso_root/boot/grub/grub.cfg"
sh "$root/scripts/winlux-secureboot.sh" prepare-iso "$iso_root" "Winlux test"
sh "$root/scripts/winlux-secureboot.sh" make-iso "$iso_root" "$work/winlux-sb.iso" 2>"$work/xorriso.log" || { cat "$work/xorriso.log" >&2; exit 1; }
cp "$vars_template" "$work/vars.fd"
status=0
expected='secureboot=1'
if test "${WINLUX_TEST_BIOS:-0}" = 1; then
    # Same hybrid image, legacy BIOS El Torito path.
    expected='secureboot=absent'
    timeout 180 qemu-system-x86_64 -m 1024 -nographic -no-reboot \
        -cdrom "$work/winlux-sb.iso" -serial mon:stdio >"$work/serial.log" 2>&1 || status=$?
else
    timeout 180 qemu-system-x86_64 -machine q35,smm=on -m 1024 -nographic -no-reboot \
        -global driver=cfi.pflash01,property=secure,value=on \
        -drive if=pflash,format=raw,unit=0,readonly=on,file="$code" \
        -drive if=pflash,format=raw,unit=1,file="$work/vars.fd" \
        -cdrom "$work/winlux-sb.iso" -serial mon:stdio >"$work/serial.log" 2>&1 || status=$?
fi
if grep -q "WINLUX_SB_KERNEL_OK $expected" "$work/serial.log"; then
    grep 'WINLUX_SB_KERNEL_OK' "$work/serial.log" | tr -d '\r'
    echo 'WINLUX_SECUREBOOT_RESULT=0'
else
    tr -d '\r' <"$work/serial.log" | tail -40
    echo "WINLUX_SECUREBOOT_RESULT=1 (qemu=$status)"
    exit 1
fi
