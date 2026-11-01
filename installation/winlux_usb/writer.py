"""Écriture de l'image sur la clé, puis relecture de contrôle."""

import hashlib
import os
import time

from . import disks
from .disks import DiskError
from .image import Cancelled, check_image

CHUNK = 4 << 20


def _padded(n, sector):
    return (n + sector - 1) // sector * sector


def validate(image, disk_path, allow_fixed=False):
    """Contrôles avant d'effacer quoi que ce soit. Renvoie (taille image, disque ou None)."""
    size = check_image(image)
    disk = disks.find_disk(disk_path)
    if disk is not None:
        if disk.system:
            raise DiskError("err_system_disk")
        if not disk.candidate and not allow_fixed:
            raise DiskError("err_not_removable")
        if disks.image_on_disk(image, disk):
            raise DiskError("err_source_on_disk")
        if disk.size and disk.size < size:
            raise DiskError("err_too_small", need=size, have=disk.size)
    elif not allow_fixed:
        raise DiskError("err_no_disk", path=disk_path)
    return size, disk


def write_image(image, disk_path, progress=None, cancel=None, verify=True, allow_fixed=False):
    """Efface la cible et y copie l'image octet pour octet.

    `progress(phase, done, total)` reçoit les phases « prepare », « write » et
    « verify ». Lève DiskError/ImageError avec une clé de message traduite.
    """
    size, disk = validate(image, disk_path, allow_fixed)
    report = progress or (lambda *_: None)
    report("prepare", 0, size)
    held = disks.prepare(disk) if disk is not None else None
    try:
        target = disks.open_target(disk_path)
        try:
            if target.size is not None and target.size < size:
                raise DiskError("err_too_small", need=size, have=target.size)
            sector = target.sector_size
            source_hash = hashlib.sha256()
            done = 0
            with open(image, "rb") as src:
                while True:
                    if cancel is not None and cancel.is_set():
                        raise Cancelled()
                    block = src.read(CHUNK)
                    if not block:
                        break
                    source_hash.update(block)
                    if len(block) % sector:
                        # Dernier bloc : un disque s'écrit par secteurs entiers.
                        block += b"\0" * (_padded(len(block), sector) - len(block))
                    target.write(block)
                    done = min(done + len(block), size)
                    report("write", done, size)
            target.sync()
            if verify:
                _verify(target, image, size, sector, source_hash.hexdigest(), report, cancel)
        finally:
            target.close()
    finally:
        if held is not None:
            held.close()
    return size


def _verify(target, image, size, sector, expected, report, cancel):
    target.rewind()
    digest = hashlib.sha256()
    done = 0
    while done < size:
        if cancel is not None and cancel.is_set():
            raise Cancelled()
        want = min(CHUNK, _padded(size - done, sector))
        block = target.read(want)
        if not block:
            raise DiskError("err_verify")
        block = block[:size - done]
        digest.update(block)
        done += len(block)
        report("verify", done, size)
    if digest.hexdigest() != expected:
        raise DiskError("err_verify")


class Speed:
    """Débit lissé pour l'affichage (« 24,3 Mo/s »)."""

    def __init__(self):
        self.t0 = time.monotonic()
        self.d0 = 0
        self.rate = 0.0

    def update(self, done):
        now = time.monotonic()
        if now - self.t0 >= 1.0:
            inst = (done - self.d0) / (now - self.t0)
            self.rate = inst if not self.rate else 0.7 * self.rate + 0.3 * inst
            self.t0, self.d0 = now, done
        return self.rate


def is_admin():
    if os.name == "nt":
        import ctypes
        try:
            return bool(ctypes.windll.shell32.IsUserAnAdmin())
        except OSError:
            return False
    return os.geteuid() == 0
