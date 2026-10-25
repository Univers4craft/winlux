#!/bin/sh
# PID 1 exclusivement dans la VM jetable de test-install-vm-guest.sh.
test "$$" -eq 1 || { echo 'réservé au PID 1 de la VM de test' >&2; exit 2; }
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
busybox mount -t proc proc /proc
busybox grep -qw 'winlux.install-test=1' /proc/cmdline || exit 2
busybox mount -t sysfs sysfs /sys
busybox mount -t devtmpfs devtmpfs /dev
busybox mount -t tmpfs -o size=1536m tmpfs /tmp
mkdir -p /dev/pts
busybox mount -t devpts devpts /dev/pts
echo WINLUX_INSTALL_TEST_BEGIN
# Un vrai terminal de contrôle permet de tester la confirmation interactive.
busybox setsid busybox cttyhack /bin/bash /opt/winlux-test/scripts/install.sh /dev/vda --iso /opt/winlux-test/source.iso </dev/console >/dev/console 2>&1
status=$?
echo "WINLUX_INSTALL_EXIT=$status"
busybox sync
busybox poweroff -f
