"""Clés USB sous Windows : API Win32 directes (ctypes), sans outil externe.

L'écriture suit la méthode habituelle des outils de création de clé :
verrouiller puis démonter chaque volume de la clé, effacer sa table de
partitions, écrire l'image secteur par secteur sur \\\\.\\PhysicalDriveN sans
cache, puis demander à Windows de relire le disque.
"""

import ctypes
import os
import struct
import time
from ctypes import wintypes

from .disks import Disk, DiskError

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
FILE_SHARE_READ = 0x1
FILE_SHARE_WRITE = 0x2
OPEN_EXISTING = 3
FILE_FLAG_NO_BUFFERING = 0x20000000
FILE_FLAG_WRITE_THROUGH = 0x80000000
INVALID_HANDLE_VALUE = wintypes.HANDLE(-1).value
MEM_COMMIT = 0x1000
MEM_RESERVE = 0x2000
MEM_RELEASE = 0x8000
PAGE_READWRITE = 0x04

IOCTL_STORAGE_QUERY_PROPERTY = 0x002D1400
IOCTL_STORAGE_GET_DEVICE_NUMBER = 0x002D1080
IOCTL_DISK_GET_LENGTH_INFO = 0x0007405C
IOCTL_DISK_GET_DRIVE_GEOMETRY_EX = 0x000700A0
IOCTL_DISK_UPDATE_PROPERTIES = 0x00070140
IOCTL_DISK_DELETE_DRIVE_LAYOUT = 0x0007C100
IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS = 0x00560000
FSCTL_LOCK_VOLUME = 0x00090018
FSCTL_DISMOUNT_VOLUME = 0x00090020

BUS_USB = 0x07
BUS_SD = 0x0C
BUS_MMC = 0x0D

kernel32.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID,
                                 wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
kernel32.CreateFileW.restype = wintypes.HANDLE
kernel32.DeviceIoControl.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD,
                                     wintypes.LPVOID, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD),
                                     wintypes.LPVOID]
kernel32.DeviceIoControl.restype = wintypes.BOOL
kernel32.WriteFile.argtypes = [wintypes.HANDLE, wintypes.LPCVOID, wintypes.DWORD,
                               ctypes.POINTER(wintypes.DWORD), wintypes.LPVOID]
kernel32.WriteFile.restype = wintypes.BOOL
kernel32.ReadFile.argtypes = [wintypes.HANDLE, wintypes.LPVOID, wintypes.DWORD,
                              ctypes.POINTER(wintypes.DWORD), wintypes.LPVOID]
kernel32.ReadFile.restype = wintypes.BOOL
kernel32.SetFilePointerEx.argtypes = [wintypes.HANDLE, ctypes.c_longlong, ctypes.POINTER(ctypes.c_longlong),
                                      wintypes.DWORD]
kernel32.SetFilePointerEx.restype = wintypes.BOOL
kernel32.FlushFileBuffers.argtypes = [wintypes.HANDLE]
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
kernel32.VirtualAlloc.argtypes = [wintypes.LPVOID, ctypes.c_size_t, wintypes.DWORD, wintypes.DWORD]
kernel32.VirtualAlloc.restype = ctypes.c_void_p
kernel32.VirtualFree.argtypes = [ctypes.c_void_p, ctypes.c_size_t, wintypes.DWORD]
kernel32.GetLogicalDrives.restype = wintypes.DWORD


def _open(path, access, flags=0):
    handle = kernel32.CreateFileW(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE, None,
                                  OPEN_EXISTING, flags, None)
    if handle == INVALID_HANDLE_VALUE or handle is None:
        return None
    return handle


def _ioctl(handle, code, inbuf=None, outsize=0):
    out = ctypes.create_string_buffer(outsize) if outsize else None
    returned = wintypes.DWORD(0)
    inp = ctypes.create_string_buffer(inbuf, len(inbuf)) if inbuf else None
    ok = kernel32.DeviceIoControl(handle, code, inp, len(inbuf) if inbuf else 0,
                                  out, outsize, ctypes.byref(returned), None)
    if not ok:
        return None
    return out.raw[:returned.value] if out is not None else b""


def _string_at(raw, offset):
    if not offset or offset >= len(raw):
        return ""
    end = raw.find(b"\0", offset)
    return raw[offset:end if end >= 0 else None].decode("ascii", "replace").strip()


def _volume_disks(letter):
    """Numéros de disque sur lesquels repose le volume X: (plusieurs si étendu)."""
    handle = _open(f"\\\\.\\{letter}:", 0)
    if handle is None:
        return []
    try:
        raw = _ioctl(handle, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, outsize=8 + 24 * 32)
    finally:
        kernel32.CloseHandle(handle)
    if not raw or len(raw) < 8:
        return []
    count = struct.unpack_from("<I", raw, 0)[0]
    return [struct.unpack_from("<I", raw, 8 + 24 * i)[0] for i in range(count) if 8 + 24 * i + 4 <= len(raw)]


def _letters():
    mask = kernel32.GetLogicalDrives()
    return [chr(65 + i) for i in range(26) if mask & (1 << i)]


def _volumes_by_disk():
    table = {}
    for letter in _letters():
        for number in _volume_disks(letter):
            table.setdefault(number, []).append(f"{letter}:")
    return table


def _system_disks(volumes):
    system_letters = {(os.environ.get("SystemDrive") or "C:").upper()[:2]}
    windir = os.environ.get("WINDIR") or os.environ.get("SystemRoot")
    if windir:
        system_letters.add(windir[:2].upper())
    return {n for n, letters in volumes.items() if system_letters & set(letters)}


def list_disks():
    volumes = _volumes_by_disk()
    system = _system_disks(volumes)
    disks = []
    for number in range(64):
        path = f"\\\\.\\PhysicalDrive{number}"
        handle = _open(path, 0)
        if handle is None:
            continue
        try:
            query = struct.pack("<II4x", 0, 0)  # StorageDeviceProperty, PropertyStandardQuery
            raw = _ioctl(handle, IOCTL_STORAGE_QUERY_PROPERTY, query, 1024) or b""
            length = _ioctl(handle, IOCTL_DISK_GET_LENGTH_INFO, outsize=8)
        finally:
            kernel32.CloseHandle(handle)
        size = struct.unpack("<q", length)[0] if length and len(length) == 8 else 0
        removable, bus, model = False, 0, ""
        if len(raw) >= 32:
            (_ver, _size, _type, _mod, rem, _cq, vend, prod, _rev, _ser, bus) = struct.unpack_from("<IIBBBBIIIII", raw, 0)
            removable = bool(rem)
            model = " ".join(p for p in (_string_at(raw, vend), _string_at(raw, prod)) if p)
        disks.append(Disk(path=path, model=model or f"Disque {number}", size=size,
                          usb=bus in (BUS_USB, BUS_SD, BUS_MMC), removable=removable,
                          system=number in system, mountpoints=volumes.get(number, []), number=number))
    return disks


def image_on_disk(image, disk):
    drive = os.path.splitdrive(os.path.abspath(image))[0]
    if len(drive) != 2 or drive[1] != ":":
        return False  # chemin réseau \\serveur\partage : jamais sur la clé
    return disk.number in _volume_disks(drive[0].upper())


class _Locked:
    """Volumes de la clé verrouillés et démontés tant que l'objet est ouvert."""

    def __init__(self, handles):
        self.handles = handles

    def close(self):
        for handle in self.handles:
            kernel32.CloseHandle(handle)  # la fermeture lève aussi le verrou
        self.handles = []


def prepare(disk):
    handles = []
    try:
        for letter in disk.mountpoints:
            handle = _open(f"\\\\.\\{letter}", GENERIC_READ | GENERIC_WRITE)
            if handle is None:
                raise DiskError("err_unmount", mount=letter, error=str(ctypes.get_last_error()))
            handles.append(handle)
            for _attempt in range(20):
                if _ioctl(handle, FSCTL_LOCK_VOLUME) is not None:
                    break
                time.sleep(0.5)
            else:
                raise DiskError("err_unmount", mount=letter, error=str(ctypes.get_last_error()))
            if _ioctl(handle, FSCTL_DISMOUNT_VOLUME) is None:
                raise DiskError("err_unmount", mount=letter, error=str(ctypes.get_last_error()))
    except BaseException:
        _Locked(handles).close()
        raise
    return _Locked(handles)


class Target:
    """Disque physique ouvert sans cache ; tampon aligné alloué par VirtualAlloc."""

    BUF = 4 << 20

    def __init__(self, path):
        self.path = path
        physical = path.lower().startswith("\\\\.\\physicaldrive")
        flags = FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH if physical else 0
        self.handle = _open(path, GENERIC_READ | GENERIC_WRITE, flags)
        if self.handle is None:
            err = ctypes.get_last_error()
            raise DiskError("err_admin" if err == 5 else "err_open", error=str(ctypes.WinError(err)))
        self.size = None
        self.sector_size = 512
        if physical:
            length = _ioctl(self.handle, IOCTL_DISK_GET_LENGTH_INFO, outsize=8)
            if length:
                self.size = struct.unpack("<q", length)[0]
            geo = _ioctl(self.handle, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, outsize=256)
            if geo and len(geo) >= 24:
                self.sector_size = struct.unpack_from("<I", geo, 20)[0] or 512
            # Sans table de partitions, Windows ne remonte rien pendant l'écriture.
            _ioctl(self.handle, IOCTL_DISK_DELETE_DRIVE_LAYOUT)
        self.buf = kernel32.VirtualAlloc(None, self.BUF, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
        if not self.buf:
            raise DiskError("err_open", error="VirtualAlloc")
        self.physical = physical

    def write(self, data):
        view = memoryview(data)
        while view:
            n = min(len(view), self.BUF)
            ctypes.memmove(self.buf, bytes(view[:n]), n)
            written = wintypes.DWORD(0)
            if not kernel32.WriteFile(self.handle, self.buf, n, ctypes.byref(written), None) or written.value != n:
                raise DiskError("err_write", error=str(ctypes.WinError(ctypes.get_last_error())))
            view = view[n:]

    def sync(self):
        kernel32.FlushFileBuffers(self.handle)

    def rewind(self):
        if not kernel32.SetFilePointerEx(self.handle, 0, None, 0):
            raise DiskError("err_read", error=str(ctypes.WinError(ctypes.get_last_error())))

    def read(self, n):
        n = min(n, self.BUF)
        got = wintypes.DWORD(0)
        if not kernel32.ReadFile(self.handle, self.buf, n, ctypes.byref(got), None):
            raise DiskError("err_read", error=str(ctypes.WinError(ctypes.get_last_error())))
        return ctypes.string_at(self.buf, got.value)

    def close(self):
        if self.handle is None:
            return
        if self.physical:
            _ioctl(self.handle, IOCTL_DISK_UPDATE_PROPERTIES)  # Windows relit la nouvelle table
        kernel32.CloseHandle(self.handle)
        kernel32.VirtualFree(self.buf, 0, MEM_RELEASE)
        self.handle = None


def open_target(path):
    return Target(path)
