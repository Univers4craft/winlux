#!/usr/bin/env python3
"""Mémoire au lancement d'un gros .exe (#997).

claude.exe (300 Mo) montait à 863 Mo rien que pour `--version`. Le runtime
gardait deux copies complètes du fichier (lecture de main.rs, puis du
chargeur) en plus de l'image chargée ; sous 700 Mo, le système le tuait
(« code -9 »). Le fichier est désormais projeté, et une grande section
alignée est projetée depuis le disque.

Le test ajoute à hello.exe une section de données de 192 Mio, alignée sur la
page dans le fichier comme la section .bun de claude.exe, puis mesure le pic
de mémoire anonyme (RssAnon) du lancement. Avant le correctif : environ 3 × 192 Mio de
plus que hello.exe ; après : quelques Mio de plus.
"""
import os
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUNTIME = Path(os.environ.get("WINLUX_BIN") or ROOT / "target/release/winlux")
TESTDIR = Path(os.environ.get("WINLUX_TESTDIR") or ROOT / "target/x86_64-pc-windows-msvc/release")
BIG = 192 << 20
PAGE = 0x1000
# Marge sur le pic de hello.exe seul : bien en dessous d'une copie de la section.
MARGIN = 48 << 20


def align(value, to):
    return (value + to - 1) // to * to


def with_big_section(source: bytes) -> bytes:
    """hello.exe + une section « .big » de BIG octets, projetable depuis le fichier."""
    data = bytearray(source)
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    count, = struct.unpack_from("<H", data, pe + 6)
    optional_size, = struct.unpack_from("<H", data, pe + 20)
    optional = pe + 24
    section_alignment, file_alignment = struct.unpack_from("<II", data, optional + 32)
    size_of_image, size_of_headers = struct.unpack_from("<II", data, optional + 56)
    table = optional + optional_size
    header = table + 40 * count
    if header + 40 > size_of_headers:
        raise SystemExit("pas de place pour une section de plus dans les en-têtes de hello.exe")
    if any(data[header:header + 40]):
        raise SystemExit("en-tête de section suivant déjà occupé")
    raw = align(len(data), PAGE)  # aligné sur la page dans le fichier, comme .bun
    rva = align(size_of_image, section_alignment)
    struct.pack_into("<8sIIIIIIHHI", data, header, b".big", BIG, rva, BIG, raw, 0, 0, 0, 0,
                     0x40000040)  # données initialisées, lecture seule
    struct.pack_into("<H", data, pe + 6, count + 1)
    struct.pack_into("<I", data, optional + 56, align(rva + BIG, section_alignment))
    data.extend(b"\0" * (raw - len(data)))
    pattern = bytes(range(256)) * (PAGE // 256)
    data.extend(pattern * (BIG // PAGE))
    assert file_alignment <= PAGE
    return bytes(data)


def peak(exe: Path) -> int:
    """Pic de mémoire anonyme (RssAnon) du lancement, en octets, relevé toutes
    les millisecondes. Les pages projetées depuis le fichier n'y comptent pas :
    le noyau peut les rendre, comme pour une image sous Windows. Une copie de
    192 Mio dure bien plus d'une milliseconde, elle ne peut pas passer entre
    deux relevés."""
    env = dict(os.environ, WINLUX_GUI="headless")
    process = subprocess.Popen([str(RUNTIME), str(exe)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    status_file = Path(f"/proc/{process.pid}/status")
    high = 0
    deadline = time.monotonic() + 60
    while process.poll() is None and time.monotonic() < deadline:
        try:
            for line in status_file.read_text().splitlines():
                if line.startswith("RssAnon:"):
                    high = max(high, int(line.split()[1]) * 1024)
        except (OSError, ValueError):
            pass
        time.sleep(0.001)
    out, err = process.communicate(timeout=60)
    if process.returncode != 0:
        raise SystemExit(f"{exe.name} : code {process.returncode}\n{(out[-400:] + err[-400:]).decode(errors='replace')}")
    return high


def main():
    hello = TESTDIR / "hello.exe"
    if not RUNTIME.is_file() or not hello.is_file():
        print(f"runtime ou hello.exe absent ({RUNTIME}, {hello})", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory(prefix="winlux-image-memory-") as work:
        big = Path(work) / "gros.exe"
        big.write_bytes(with_big_section(hello.read_bytes()))
        base = peak(hello)
        large = peak(big)
    mib = lambda n: f"{n / (1 << 20):.0f} Mio"
    print(f"hello.exe : {mib(base)} ; avec 192 Mio de section : {mib(large)} (limite {mib(base + MARGIN)})")
    if large > base + MARGIN:
        print("ÉCHEC : le lancement copie le fichier en mémoire (#997)", file=sys.stderr)
        return 1
    print("OK: gros .exe lancé sans copie du fichier en mémoire (#997)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
