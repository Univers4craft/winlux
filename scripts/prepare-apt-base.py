#!/usr/bin/env python3
"""Build a real, configured Ubuntu package database inside a disposable VM.

No maintainer script runs on the build host. No release is created here.
"""
import argparse
import hashlib
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tarfile

BASE = "ubuntu-base-26.04.1-base-amd64.tar.gz"
BASE_SHA256 = "a496a960472ce474a59590b8987d3a1135d3cbef1991f3b1abe8cacfea8bf85a"
PROJECT = Path(__file__).resolve().parent.parent
PACKAGES = runpy.run_path(str(PROJECT / "scripts/prepare-desktop-packages.py"))["PACKAGES"]
PACKAGES += ["apt", "dpkg", "bash", "coreutils", "findutils", "grep", "sed", "tar",
             "gzip", "xz-utils", "ca-certificates", "ubuntu-keyring", "curl", "sudo",
             "busybox-static", "xserver-xorg-core", "xserver-xorg-input-libinput",
             "xvfb", "xterm", "x11-xkb-utils", "xinput", "fonts-dejavu-core",
             "ncurses-base", "udev", "kmod", "util-linux", "dbus", "hello",
             "nftables"]
PACKAGES += ["rsync", "openssh-client"]
PACKAGES += ["xfonts-base", "libarchive-tools", "squashfs-tools", "libfaad2", "libmpg123-0",
             "libqt5core5t64", "libqt5gui5t64", "libqt5widgets5t64",
             "libqt6core6t64", "libqt6gui6", "libqt6widgets6"]
# Tools the graphical installer runs from the live session.
PACKAGES += ["parted", "dosfstools", "e2fsprogs", "cryptsetup-bin", "ntfs-3g", "kbd",
             "console-setup", "grub2-common", "grub-pc-bin", "grub-efi-amd64-bin", "efibootmgr",
             "xorriso", "openssl", "python3-gi-cairo"]
# Decoding engine of the media player (winlux_media runs ffmpeg/ffprobe from PATH).
PACKAGES += ["ffmpeg"]
# Ubuntu retire des paquets les traductions de ses applications (nm-applet,
# GTK, GLib, PolicyKit…) pour les mettre dans les language-pack : sans eux,
# le menu réseau et les boîtes de dialogue restent en anglais. Les quatre
# langues de l'interface ; leur postinst génère aussi les locales es/de/en.
LANGUAGE_PACKS = [f"language-pack{kind}-{lang}-base"
                  for lang in ("fr", "en", "es", "de") for kind in ("", "-gnome")]
PACKAGES += LANGUAGE_PACKS
DESKTOP_REQUIRED = ["apt", "dpkg", "bash", "busybox-static", "curl", "ca-certificates",
    "python3", "python3-gi", "gir1.2-gtk-3.0", "openbox", "xfdesktop4", "xfce4-panel",
    "xfce4-genmon-plugin", "xfconf", "xfce4-settings", "dbus-x11", "wmctrl", "xdotool",
    "x11-utils", "x11-xserver-utils", "x11-xkb-utils", "xinput", "xserver-xorg-core",
    "xserver-xorg-input-libinput", "xterm", "fonts-dejavu-core", "util-linux", "thunar",
    "network-manager", "network-manager-gnome", "udisks2", "gvfs", "libfaad2", "libmpg123-0",
    "libqt5core5t64", "libqt5gui5t64", "libqt5widgets5t64",
    "libqt6core6t64", "libqt6gui6", "libqt6widgets6", "libasound2t64", "alsa-utils",
    "libx11-6", "libxft2", "libcairo2", "polkitd", "rsync", "openssh-client", "espeak-ng", "ffmpeg", "winlux-kernel"] + LANGUAGE_PACKS

def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", required=True, type=Path)
    parser.add_argument("--kernel-release", required=True)
    parser.add_argument("--output", type=Path, default=PROJECT / "build/apt-base")
    args = parser.parse_args()
    work = args.output.resolve()
    work.mkdir(parents=True, exist_ok=True)
    base = work / BASE
    if not base.exists():
        run("curl", "-fL", "--retry", "2", "--connect-timeout", "20", "--max-time", "600",
            "https://cdimage.ubuntu.com/ubuntu-base/releases/26.04/release/" + BASE,
            "-o", str(base))
    if hashlib.sha256(base.read_bytes()).hexdigest() != BASE_SHA256:
        raise RuntimeError("Ubuntu Base checksum mismatch")
    root = work / "bootstrap-root"
    if root.exists():
        raise RuntimeError(f"Bootstrap directory already exists: {root}; retain it for diagnostics or move it before rebuilding")
    root.mkdir()
    with tarfile.open(base) as archive:
        archive.extractall(root, filter="tar")
    boot = root / "winlux-bootstrap"
    boot.mkdir()
    cache = PROJECT / "build/desktop-components/cache/archives"
    archives = root / "var/cache/apt/archives"
    archives.mkdir(parents=True, exist_ok=True)
    for package in cache.glob("*.deb"): shutil.copyfile(package, archives / package.name)
    shutil.copyfile("/usr/bin/busybox", boot / "busybox")
    (boot / "busybox").chmod(0o755)
    dependencies = subprocess.check_output(["modprobe", "-S", args.kernel_release,
                                           "--show-depends", "e1000"], text=True)
    modules = []
    for index, line in enumerate(dependencies.splitlines()):
        if not line.startswith("insmod "): continue
        source = Path(line.split()[1]); target = boot / f"module-{index}.ko"
        if source.suffix == ".zst":
            with target.open("wb") as output: run("zstd", "-qdc", str(source), stdout=output)
        else: shutil.copyfile(source, target)
        modules.append(str(target.relative_to(root)))
    # Native Debian scripts may configure services; the build never starts them.
    policy = root / "usr/sbin/policy-rc.d"
    policy.write_text("#!/bin/sh\nexit 101\n"); policy.chmod(0o755)
    # ubuntu-base exclut toutes les traductions (dpkg.cfg.d/excludes) : sans
    # elles le bureau, la Corbeille et l'éditeur restent en anglais. Garder
    # celles des quatre langues de l'interface ; lu après « excludes ». Les
    # pages d'aide des language-pack (help-langpack, la moitié de leur taille)
    # n'ont pas de lecteur dans Winlux : elles ne sont pas installées.
    (root / "etc/dpkg/dpkg.cfg.d/winlux-locales").write_text(
        "".join(f"path-include=/usr/share/locale/{lang}/LC_MESSAGES/*.mo\n" for lang in ("fr", "en", "es", "de"))
        + "path-exclude=/usr/share/help-langpack/*\n")
    (root / "etc/resolv.conf").unlink(missing_ok=True)
    (root / "etc/resolv.conf").write_text("nameserver 10.0.2.3\n")
    # Signed Ubuntu indexes authenticate packages; no trusted=yes or disabled checks.
    for entry in (root / "etc/apt/sources.list.d").glob("*"): entry.unlink()
    (root / "etc/apt/sources.list").write_text(
        "deb http://archive.ubuntu.com/ubuntu resolute main universe multiverse restricted\n"
        "deb http://archive.ubuntu.com/ubuntu resolute-updates main universe multiverse restricted\n"
        "deb http://security.ubuntu.com/ubuntu resolute-security main universe multiverse restricted\n")
    (boot / "dhcp").write_text("#!/winlux-bootstrap/busybox sh\n"
        "B=/winlux-bootstrap/busybox\n"
        'case "$1" in bound|renew) $B ifconfig "$interface" "$ip" netmask "$subnet"; '
        '$B route add default gw "${router%% *}" "$interface" || true;; esac\n')
    (boot / "dhcp").chmod(0o755)
    install = " ".join(sorted(set(PACKAGES)))
    (boot / "install").write_text("#!/bin/bash\nset -e\n"
        "apt-get update\n"
        + f"apt-get install -y --no-install-recommends {install}\n"
        + "localedef --no-archive -i fr_FR -f UTF-8 /usr/lib/locale/fr_FR.utf8\n"
          # Le menu de l'icône réseau est traduit dans chaque langue.
          "for l in fr es de; do test -s /usr/share/locale-langpack/$l/LC_MESSAGES/nm-applet.mo; done\n"
          "test -z \"$(find /usr/share/help-langpack -type f 2>/dev/null)\"\n"
          "dpkg --audit > /winlux-bootstrap/audit\ntest ! -s /winlux-bootstrap/audit\n"
          "apt-get check\nhello\napt-get purge -y hello\napt-get clean\n"
          "rm -rf /var/lib/apt/lists/*\nmkdir -p /usr/share/winlux\n"
          "echo ubuntu-resolute > /usr/share/winlux/apt-base\n"
          "tar --numeric-owner --exclude=./proc --exclude=./sys --exclude=./dev "
          "--exclude=./run --exclude=./tmp --exclude=./winlux-bootstrap "
          "-C / -cpf /winlux-bootstrap/root.tar .\n")
    (boot / "init").write_text("#!/winlux-bootstrap/busybox sh\n"
        "B=/winlux-bootstrap/busybox\n"
        "export PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=/root DEBIAN_FRONTEND=noninteractive\n"
        "$B mount -t proc proc /proc\n$B mount -t sysfs sysfs /sys\n"
        "$B mount -t devtmpfs devtmpfs /dev\n$B mkdir -p /dev/pts /run\n"
        "$B mount -t devpts devpts /dev/pts\n$B mount -t tmpfs tmpfs /run\n"
        "$B chmod 1777 /tmp\n$B chown -R 0:0 /etc /usr /var\n"
        + "".join(f"$B insmod /{module}\n" for module in modules)
        + "$B ip link set lo up\n"
        # Diagnostics land in bootstrap.log: a missing NIC or DHCP lease is
        # otherwise indistinguishable from a package failure.
        + "echo WINLUX_NET_INTERFACES: $($B ls /sys/class/net)\n"
        + 'up=0; for n in /sys/class/net/*; do test "${n##*/}" = lo && continue; '
          '$B ip link set "${n##*/}" up; '
          '$B udhcpc -n -q -t 20 -T 3 -i "${n##*/}" -s /winlux-bootstrap/dhcp && up=1; break; done\n'
        + 'if test "$up" != 1; then echo WINLUX_NET_FAILED; $B ip addr; $B dmesg | $B tail -40; '
          'echo WINLUX_APT_BASE_RESULT=network; $B sync; $B poweroff -f; fi\n'
        + "/bin/bash /winlux-bootstrap/install\nresult=$?\necho WINLUX_APT_BASE_RESULT=$result\n"
          "$B sync\n$B poweroff -f\nwhile :; do $B sleep 1; done\n")
    (boot / "init").chmod(0o755)
    disk = work / "bootstrap.raw"
    run("truncate", "-s", "12G", str(disk))
    run("fakeroot", "mkfs.ext4", "-q", "-F", "-d", str(root), str(disk))
    accel = "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg"
    # Host binaries copied into the VM (BusyBox) may use AVX: QEMU's default
    # qemu64 CPU model lacks it and the guest init dies on an invalid opcode.
    log = work / "bootstrap.log"
    with log.open("w") as output:
        run("timeout", "-k", "5", "2400", "qemu-system-x86_64", "-m", "3072", "-smp", "2",
            "-accel", accel, "-cpu", "max", "-display", "none", "-monitor", "none", "-serial", "stdio",
            "-no-reboot", "-kernel", str(args.kernel.resolve()),
            "-append", "root=/dev/vda rw rootfstype=ext4 init=/winlux-bootstrap/init console=ttyS0 panic=1",
            "-drive", f"file={disk},if=virtio,format=raw", "-nic", "user,model=e1000",
            stdout=output, stderr=subprocess.STDOUT)
    if "WINLUX_APT_BASE_RESULT=0" not in log.read_text():
        tail = log.read_text(errors="replace").splitlines()[-40:]
        raise RuntimeError(f"APT base build failed; see {log}\n" + "\n".join(tail))
    tarball = work / "root.tar"
    run("debugfs", "-R", f"dump /winlux-bootstrap/root.tar {tarball}", str(disk))
    if not tarball.is_file(): raise RuntimeError("Missing configured base archive")
    if os.environ.get("WINLUX_KEEP_TEST_DISK", "0") != "1":
        disk.unlink()
        shutil.rmtree(root)
    print(f"Configured native APT base: {tarball}")

if __name__ == "__main__": main()
