#!/usr/bin/env python3
"""Fabrique une fausse ISO hybride pour les tests (en-têtes ISO 9660 + MBR, contenu aléatoire)."""
import os
import sys

path, size = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 20 * 1024 * 1024 + 1000
data = bytearray(os.urandom(size))
data[446:510] = bytes(64)          # table de partitions vide : Windows ne monte rien
data[510:512] = b"\x55\xaa"
data[0x8001:0x8006] = b"CD001"
with open(path, "wb") as f:
    f.write(data)
