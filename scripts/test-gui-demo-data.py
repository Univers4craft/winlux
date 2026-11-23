#!/usr/bin/env python3
"""#3432 : gui_demo.exe ne porte plus ses grands tableaux statiques en .data.

Lit l'en-tête PE de l'exe construit et vérifie que les sections initialisées
(.data et compagnie, données brutes sur disque) restent sous 2 Mio. Avant le
correctif : 7,5 Mo (écrans du Terminal remplis d'espaces, entrées de
l'Observateur d'événements).
"""
import struct
import sys

LIMITE = 2 * 1024 * 1024
IMAGE_SCN_CNT_INITIALIZED_DATA = 0x40
IMAGE_SCN_MEM_WRITE = 0x80000000

path = sys.argv[1] if len(sys.argv) > 1 else "target/x86_64-pc-windows-msvc/release/gui_demo.exe"
data = open(path, "rb").read()
pe = struct.unpack_from("<I", data, 0x3C)[0]
assert data[pe:pe + 4] == b"PE\0\0", "en-tête PE absent"
nsec = struct.unpack_from("<H", data, pe + 6)[0]
opt = struct.unpack_from("<H", data, pe + 20)[0]
off = pe + 24 + opt
total = 0
for i in range(nsec):
    name, _vsize, _va, rawsize, _rawptr = struct.unpack_from("<8sIIII", data, off + 40 * i)
    flags = struct.unpack_from("<I", data, off + 40 * i + 36)[0]
    if flags & IMAGE_SCN_CNT_INITIALIZED_DATA and flags & IMAGE_SCN_MEM_WRITE:
        n = name.rstrip(b"\0").decode(errors="replace")
        print(f"  section {n:8} {rawsize / 1048576:6.2f} Mio sur disque")
        total += rawsize
print(f"données inscriptibles initialisées : {total / 1048576:.2f} Mio (limite {LIMITE / 1048576:.0f} Mio)")
if total > LIMITE:
    sys.exit("ÉCHEC : gui_demo.exe repart avec de grands tableaux statiques en .data (#3432)")
print("OK")
