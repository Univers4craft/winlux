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
recovery=0
luks_device=
boot_device=
for argument in $("$B" cat /proc/cmdline); do
    case "$argument" in
        root=*) root_device=${argument#root=} ;;
        winlux.recovery=1) recovery=1 ;;
        winlux.luks=*) luks_device=${argument#winlux.luks=} ;;
        winlux.boot=*) boot_device=${argument#winlux.boot=} ;;
    esac
done
resolve_device() {
    case "$1" in UUID=*|LABEL=*) "$B" findfs "$1" 2>/dev/null || true ;; *) echo "$1" ;; esac
}
if test -n "$luks_device"; then
    test -x /bin/cryptsetup || fail 'disque chiffré mais cryptsetup absent de l initramfs'
    test -f /etc/boot.kmap && "$B" loadkmap </etc/boot.kmap 2>/dev/null
    "$B" mkdir -p /run/cryptsetup
    export DM_DISABLE_UDEV=1
    for attempt in $("$B" seq 1 60); do
        device=$(resolve_device "$luks_device")
        test -b "$device" && break
        "$B" sleep 0.5
    done
    test -b "$device" || fail "disque chiffré introuvable : $luks_device"
    # /dev/console is the serial port when console=ttyS0 comes last; ask on the screen.
    tty=/dev/console
    test -c /dev/tty1 && tty=/dev/tty1
    unlocked=0
    for attempt in 1 2 3 4 5; do
        echo 'Winlux : saisissez la phrase secrète du disque chiffré' >"$tty"
        if /bin/cryptsetup open --type luks --tries 1 "$device" winlux_crypt <"$tty" >"$tty" 2>&1; then
            unlocked=1
            break
        fi
    done
    test "$unlocked" = 1 || fail 'déverrouillage du disque chiffré impossible'
fi
if test -n "$root_device"; then
    for attempt in $("$B" seq 1 60); do
        device=$root_device
        case "$device" in UUID=*|LABEL=*) device=$("$B" findfs "$device" 2>/dev/null || true) ;; esac
        if test -b "$device" && "$B" mount -t ext4 -o rw "$device" /newroot; then break; fi
        "$B" sleep 0.5
    done
    "$B" awk '$2 == "/newroot" {mounted=1} END {exit !mounted}' /proc/mounts || fail "persistent root not found: $root_device"
    if test -n "$boot_device"; then
        device=$(resolve_device "$boot_device")
        "$B" mkdir -p /newroot/boot
        test -b "$device" && "$B" mount -t ext4 -o rw "$device" /newroot/boot || fail "partition /boot introuvable : $boot_device"
    fi
    if test "$recovery" = 1; then
        "$B" mkdir -p /newroot/dev /newroot/proc /newroot/sys /newroot/run
        for directory in dev proc sys; do
            "$B" mount --bind "/$directory" "/newroot/$directory" || fail "recovery mount failed: $directory"
        done
        "$B" mount -t tmpfs -o mode=0755 tmpfs /newroot/run || fail 'recovery runtime mount failed'
        while :; do
            echo
            echo 'Winlux - secours : 1 restaurer la mise à jour précédente, 2 console, 3 redémarrer'
            echo 'La restauration nécessite une sauvegarde valide. Une clé USB Winlux permet aussi de démarrer.'
            printf '> '
            read -r choice || choice=2
            case "$choice" in
                1) "$B" chroot /newroot /bin/busybox sh /usr/bin/winlux-update --rollback && echo 'Restauration terminée. Redémarrez avec 3.' || echo 'Restauration impossible. Utilisez la console ou une clé USB de secours.' ;;
                2) echo 'Racine installée : /newroot. Tapez exit pour revenir au menu.'; "$B" sh ;;
                3) "$B" sync; "$B" reboot -f ;;
            esac
        done
    fi
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
