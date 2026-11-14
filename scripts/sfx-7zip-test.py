"""Fabrique un auto-extractible 7-Zip de test (#2390) : souche MZ/PE x64,
bloc de configuration du module SFX, archive 7z « copie » (sans
compression). Usage : sfx-7zip-test.py SORTIE CONFIG NOM=FICHIER...

Format 7z : 7zFormat.txt du SDK LZMA (domaine public), comme le
générateur des tests unitaires de runtime/src/setup/sevenz.rs."""
import struct
import sys
import zlib

SIGNATURE = b"7z\xbc\xaf\x27\x1c"


def num(v):
    for k in range(9):
        if k == 8 or v < (1 << (8 * k + 7 - k)):
            first = ((0xFF << (8 - k)) & 0xFF) | (v >> (8 * k) if k < 8 else 0)
            return bytes([first]) + v.to_bytes(8, "little")[:k]
    raise ValueError(v)


def archive(files):
    packed = b"".join(d for _, d in files)
    h = bytearray([0x01, 0x04, 0x06]) + num(0) + num(1) + b"\x09" + num(len(packed)) + b"\x00"
    h += b"\x07\x0b" + num(1) + b"\x00" + b"\x01\x01\x00" + b"\x0c" + num(len(packed)) + b"\x00"
    h += b"\x08\x0d" + num(len(files)) + b"\x09"
    for _, d in files[:-1]:
        h += num(len(d))
    h += b"\x0a\x01"
    for _, d in files:
        h += struct.pack("<I", zlib.crc32(d))
    h += b"\x00\x00"
    h += b"\x05" + num(len(files))
    names = bytearray(b"\x00")
    for name, _ in files:
        names += (name + "\0").encode("utf-16-le")
    h += b"\x11" + num(len(names)) + names + b"\x00\x00"
    start = struct.pack("<QQI", len(packed), len(h), zlib.crc32(h))
    return SIGNATURE + b"\x00\x04" + struct.pack("<I", zlib.crc32(start)) + start + packed + bytes(h)


def main(out, config, *pairs):
    # detect_file ignore un fichier de moins de 4096 octets.
    stub = bytearray(0x1000)
    stub[0:2] = b"MZ"
    stub[0x3C:0x40] = struct.pack("<I", 0x80)
    stub[0x80:0x84] = b"PE\0\0"
    stub[0x84:0x86] = struct.pack("<H", 0x8664)
    files = []
    for p in pairs:
        name, path = p.split("=", 1)
        with open(path, "rb") as f:
            files.append((name, f.read()))
    block = b";!@Install@!UTF-8!\r\n" + config.encode().replace(b"\\n", b"\r\n") + b"\r\n;!@InstallEnd@!"
    with open(out, "wb") as f:
        f.write(bytes(stub) + block + archive(files))


if __name__ == "__main__":
    main(*sys.argv[1:])
