#!/bin/sh
# Small initramfs: load storage drivers, then enter the real disk/live root.
export PATH=/bin
B=/bin/busybox
fail() { echo "Winlux boot: $*" >&2; exec "$B" sh; }
"$B" mount -t proc proc /proc
"$B" mount -t sysfs sysfs /sys
"$B" mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
while read -r module; do "$B" insmod "/$module" 2>/dev/null || true; done </modules-to-load
"$B" mdev -s
root_device=
for argument in $("$B" cat /proc/cmdline); do
    case "$argument" in root=*) root_device=${argument#root=} ;; esac
done
if test -n "$root_device"; then
    for attempt in $("$B" seq 1 60); do
        device=$root_device
        case "$device" in UUID=*|LABEL=*) device=$("$B" findfs "$device" 2>/dev/null || true) ;; esac
        if test -b "$device" && "$B" mount -t ext4 -o rw "$device" /newroot; then break; fi
        "$B" sleep 0.5
    done
    "$B" awk '$2 == "/newroot" {mounted=1} END {exit !mounted}' /proc/mounts || fail "persistent root not found: $root_device"
else
    found=0
    for attempt in $("$B" seq 1 60); do
        for device in /dev/sr* /dev/sd* /dev/vd* /dev/nvme*n*p*; do
            test -b "$device" || continue
            # Never mount unrelated ext4 disks or replay their journals.
            if "$B" mount -t iso9660 -o ro "$device" /media 2>/dev/null ||
               "$B" mount -t vfat -o ro "$device" /media 2>/dev/null; then
                if test -f /media/boot/winlux-rootfs.squashfs; then found=1; break; fi
                "$B" umount /media
            fi
        done
        test "$found" = 0 || break
        "$B" sleep 0.5
    done
    test "$found" = 1 || fail 'live medium not found'
    "$B" mount -t squashfs -o ro,loop /media/boot/winlux-rootfs.squashfs /lower || fail 'cannot mount live root'
    "$B" mount -t tmpfs -o mode=0755 tmpfs /cow || fail 'cannot create writable layer'
    "$B" mkdir -p /cow/upper /cow/work
    "$B" mount -t overlay overlay -o lowerdir=/lower,upperdir=/cow/upper,workdir=/cow/work /newroot || fail 'cannot create live overlay'
    "$B" mkdir -p /newroot/run
    "$B" mount -t tmpfs -o mode=0755 tmpfs /newroot/run
    "$B" mkdir -p /newroot/run/winlux/live/media /newroot/run/winlux/live/lower /newroot/run/winlux/live/cow
    "$B" chmod 700 /newroot/run/winlux
    "$B" mount --move /media /newroot/run/winlux/live/media
    "$B" mount --move /lower /newroot/run/winlux/live/lower
    "$B" mount --move /cow /newroot/run/winlux/live/cow
fi
test -x /newroot/init || fail 'init missing from target root'
"$B" mkdir -p /newroot/dev /newroot/proc /newroot/sys
for directory in dev proc sys; do "$B" mount --move "/$directory" "/newroot/$directory"; done
exec "$B" switch_root /newroot /init
