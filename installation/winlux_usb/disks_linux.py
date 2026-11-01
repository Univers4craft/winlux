"""Clés USB sous Linux/Winlux, lues dans /sys sans dépendance externe."""

import fcntl
import os
import stat
import subprocess
from pathlib import Path

from .disks import Disk, DiskError

SYS = os.environ.get("WINLUX_USB_SYSFS", "/sys")
MOUNTS = os.environ.get("WINLUX_USB_MOUNTS", "/proc/self/mounts")
SWAPS = os.environ.get("WINLUX_USB_SWAPS", "/proc/swaps")
SKIP_PREFIXES = ("ram", "zram", "dm-", "md", "sr", "nbd", "fd", "drbd")
# Points de montage qui désignent le système en cours, ou le média Live dont
# Winlux tourne : ces disques ne sont jamais proposés.
SYSTEM_MOUNTS = {"/", "/boot", "/boot/efi", "/efi", "/usr", "/var", "/home",
                 "/run/winlux/live/media", "/run/winlux/live/lower"}

BLKRRPART = 0x125F
BLKFLSBUF = 0x1261


def _read(path, default=""):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace").strip()
    except OSError:
        return default


def _unescape(field):
    return field.replace("\\040", " ").replace("\\011", "\t").replace("\\012", "\n").replace("\\134", "\\")


def _mount_table():
    """{nom de périphérique: [points de montage]} d'après /proc/self/mounts et /proc/swaps."""
    table = {}
    for line in _read(MOUNTS).splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0].startswith("/dev/"):
            dev = os.path.realpath(parts[0])
            table.setdefault(os.path.basename(dev), []).append(_unescape(parts[1]))
    for line in _read(SWAPS).splitlines()[1:]:
        parts = line.split()
        if parts and parts[0].startswith("/dev/"):
            table.setdefault(os.path.basename(os.path.realpath(parts[0])), []).append("[SWAP]")
    return table


def _members(sys_block, name):
    """Le disque et ses partitions, ainsi que les volumes construits dessus (LUKS, LVM)."""
    names = [name]
    base = Path(sys_block) / name
    try:
        for child in sorted(base.iterdir()):
            if child.name.startswith(name) and (child / "partition").exists():
                names.append(child.name)
    except OSError:
        pass
    for member in list(names):
        holders = base / member / "holders" if member != name else base / "holders"
        try:
            names.extend(h.name for h in holders.iterdir())
        except OSError:
            pass
    return names


def list_disks(include_loops=None):
    if include_loops is None:
        # Tests : un périphérique loop joue le rôle de la clé.
        include_loops = os.environ.get("WINLUX_USB_INCLUDE_LOOPS") == "1"
    sys_block = Path(SYS) / "block"
    mounts = _mount_table()
    disks = []
    try:
        names = sorted(os.listdir(sys_block))
    except OSError:
        return disks
    for name in names:
        if name.startswith(SKIP_PREFIXES) or (name.startswith("loop") and not include_loops):
            continue
        base = sys_block / name
        size = int(_read(base / "size", "0") or 0) * 512
        if size <= 0:
            continue
        real = os.path.realpath(base)
        usb = "/usb" in real or name.startswith("mmcblk")
        removable = _read(base / "removable") == "1"
        vendor = _read(base / "device/vendor")
        if vendor.startswith("0x"):
            vendor = ""  # identifiant PCI (virtio), pas un nom de fabricant
        model = _read(base / "device/model") or _read(base / "device/name")
        label = " ".join(p for p in (vendor, model) if p) or name
        points = []
        for member in _members(sys_block, name):
            points.extend(mounts.get(member, []))
        system = any(p in SYSTEM_MOUNTS or p == "[SWAP]" for p in points)
        disks.append(Disk(path=f"/dev/{name}", model=label, size=size, usb=usb,
                          removable=removable, system=system, mountpoints=points))
    return disks


def _disk_name_of_dev(dev):
    """Nom du disque entier qui contient le périphérique bloc major:minor `dev`."""
    link = Path(SYS) / "dev/block" / f"{os.major(dev)}:{os.minor(dev)}"
    try:
        real = Path(os.path.realpath(link))
    except OSError:
        return None
    if (real / "partition").exists():
        return real.parent.name
    return real.name


def image_on_disk(image, disk):
    try:
        st = os.stat(image)
    except OSError:
        return False
    name = _disk_name_of_dev(st.st_dev)
    if name and name == os.path.basename(disk.path):
        return True
    # Fichier sur un volume chiffré ou LVM posé sur la clé.
    holders_dev = Path(SYS) / "dev/block" / f"{os.major(st.st_dev)}:{os.minor(st.st_dev)}"
    try:
        holder = os.path.realpath(holders_dev)
        slaves = Path(holder) / "slaves"
        for slave in slaves.iterdir():
            parent = _disk_name_of_dev(os.stat(f"/dev/{slave.name}").st_rdev)
            if parent == os.path.basename(disk.path):
                return True
    except OSError:
        pass
    return False


class _Prepared:
    def close(self):
        pass


def prepare(disk):
    """Démonte toutes les partitions de la clé (comme « Éjecter » sans la couper)."""
    for point in sorted(set(disk.mountpoints), key=len, reverse=True):
        if point == "[SWAP]":
            raise DiskError("err_unmount", mount=point)
        result = subprocess.run(["umount", point], capture_output=True, text=True)
        if result.returncode != 0:
            # Démontage paresseux en dernier recours : plus rien ne s'écrira dessus.
            result = subprocess.run(["umount", "-l", point], capture_output=True, text=True)
            if result.returncode != 0:
                raise DiskError("err_unmount", mount=point, error=result.stderr.strip())
    return _Prepared()


class Target:
    """Cible d'écriture brute : périphérique bloc (accès exclusif) ou fichier."""

    def __init__(self, path):
        self.path = path
        try:
            mode = os.stat(path).st_mode
        except OSError as exc:
            raise DiskError("err_open", error=str(exc)) from exc
        self.block = stat.S_ISBLK(mode)
        flags = os.O_RDWR | getattr(os, "O_CLOEXEC", 0)
        if self.block:
            flags |= os.O_EXCL  # refusé par le noyau si une partition est encore montée
        try:
            self.fd = os.open(path, flags)
        except OSError as exc:
            key = "err_busy" if exc.errno == 16 else ("err_admin" if exc.errno in (1, 13) else "err_open")
            raise DiskError(key, error=str(exc)) from exc
        self.size = os.lseek(self.fd, 0, os.SEEK_END) if self.block else None
        os.lseek(self.fd, 0, os.SEEK_SET)
        self.sector_size = 512

    def write(self, data):
        view = memoryview(data)
        while view:
            n = os.write(self.fd, view)
            view = view[n:]

    def sync(self):
        os.fsync(self.fd)
        if self.block:
            try:
                fcntl.ioctl(self.fd, BLKFLSBUF)  # relire le support, pas le cache
            except OSError:
                pass
        try:
            os.posix_fadvise(self.fd, 0, 0, os.POSIX_FADV_DONTNEED)
        except (AttributeError, OSError):
            pass

    def rewind(self):
        os.lseek(self.fd, 0, os.SEEK_SET)

    def read(self, n):
        return os.read(self.fd, n)

    def close(self):
        if self.fd is None:
            return
        if self.block:
            try:
                fcntl.ioctl(self.fd, BLKRRPART)  # le noyau relit la nouvelle table
            except OSError:
                pass
        os.close(self.fd)
        self.fd = None


def open_target(path):
    return Target(path)
