#!/usr/bin/python3
"""Signature Ed25519 des manifestes de mise à jour Winlux (#31).

    winlux-signature verify CLE.pub FICHIER FICHIER.sig
    winlux-signature sign GRAINE FICHIER > FICHIER.sig
    winlux-signature public GRAINE > CLE.pub
    winlux-signature new GRAINE           # crée une graine secrète (0600)

Implémentation d'Ed25519 écrite d'après la RFC 8032 (section 5.1), sans
dépendance : Python seul, pour que le client de mise à jour puisse vérifier
sans bibliothèque supplémentaire. La graine (clé privée) fait 32 octets et
s'écrit en hexadécimal ; la clé publique et la signature aussi (64 et 128
caractères hexadécimaux, suivis d'un saut de ligne).

La graine ne vit que dans le secret GitHub WINLUX_UPDATE_SIGNING_KEY et sur
le PC qui l'a créée ; seule la clé publique est dans le dépôt
(integration/native/keys/update-signing.pub) et dans l'image.
"""
from __future__ import annotations

import hashlib
import os
import sys
from pathlib import Path

P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def _sha512(data: bytes) -> int:
    return int.from_bytes(hashlib.sha512(data).digest(), "little")


# Points en coordonnées étendues (X, Y, Z, T), x = X/Z, y = Y/Z, x*y = T/Z.
def _add(p, q):
    a = (p[1] - p[0]) * (q[1] - q[0]) % P
    b = (p[1] + p[0]) * (q[1] + q[0]) % P
    c = 2 * p[3] * q[3] * D % P
    d = 2 * p[2] * q[2] % P
    e, f, g, h = b - a, d - c, d + c, b + a
    return (e * f % P, g * h % P, f * g % P, e * h % P)


def _mul(s: int, p):
    q = (0, 1, 1, 0)
    while s > 0:
        if s & 1:
            q = _add(q, p)
        p = _add(p, p)
        s >>= 1
    return q


def _equal(p, q) -> bool:
    return (p[0] * q[2] - q[0] * p[2]) % P == 0 and (p[1] * q[2] - q[1] * p[2]) % P == 0


def _recover_x(y: int, sign: int):
    if y >= P:
        return None
    x2 = (y * y - 1) * pow(D * y * y + 1, P - 2, P)
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P != 0:
        x = x * SQRT_M1 % P
    if (x * x - x2) % P != 0:
        return None
    if (x & 1) != sign:
        x = P - x
    return x


_GY = 4 * pow(5, P - 2, P) % P
_GX = _recover_x(_GY, 0)
G = (_GX, _GY, 1, _GX * _GY % P)


def _compress(p) -> bytes:
    zinv = pow(p[2], P - 2, P)
    x, y = p[0] * zinv % P, p[1] * zinv % P
    return int.to_bytes(y | ((x & 1) << 255), 32, "little")


def _decompress(data: bytes):
    if len(data) != 32:
        return None
    y = int.from_bytes(data, "little")
    sign = y >> 255
    y &= (1 << 255) - 1
    x = _recover_x(y, sign)
    return None if x is None else (x, y, 1, x * y % P)


def _expand(seed: bytes):
    if len(seed) != 32:
        raise ValueError("graine Ed25519 de 32 octets attendue")
    h = hashlib.sha512(seed).digest()
    a = int.from_bytes(h[:32], "little")
    a &= (1 << 254) - 8
    a |= 1 << 254
    return a, h[32:]


def public_key(seed: bytes) -> bytes:
    a, _ = _expand(seed)
    return _compress(_mul(a, G))


def sign(seed: bytes, message: bytes) -> bytes:
    a, prefix = _expand(seed)
    public = _compress(_mul(a, G))
    r = _sha512(prefix + message) % L
    big_r = _compress(_mul(r, G))
    h = _sha512(big_r + public + message) % L
    s = (r + h * a) % L
    return big_r + int.to_bytes(s, 32, "little")


def verify(public: bytes, message: bytes, signature: bytes) -> bool:
    if len(public) != 32 or len(signature) != 64:
        return False
    a = _decompress(public)
    if a is None:
        return False
    big_r = _decompress(signature[:32])
    if big_r is None:
        return False
    s = int.from_bytes(signature[32:], "little")
    if s >= L:
        return False
    h = _sha512(signature[:32] + public + message) % L
    return _equal(_mul(s, G), _add(big_r, _mul(h, a)))


def read_hex(path: Path, size: int, what: str) -> bytes:
    text = path.read_text(encoding="ascii", errors="replace").strip()
    try:
        value = bytes.fromhex(text)
    except ValueError:
        value = b""
    if len(value) != size:
        raise ValueError(f"{what} invalide dans {path} ({size} octets en hexadécimal attendus)")
    return value


def verify_file(public_path: Path, path: Path, signature_path: Path) -> bool:
    return verify(read_hex(public_path, 32, "clé publique"), path.read_bytes(),
                  read_hex(signature_path, 64, "signature"))


def main(argv: list[str]) -> int:
    try:
        if len(argv) == 5 and argv[1] == "verify":
            good = verify_file(Path(argv[2]), Path(argv[3]), Path(argv[4]))
            print("signature valide" if good else "signature INVALIDE", file=sys.stderr)
            return 0 if good else 1
        if len(argv) == 4 and argv[1] == "sign":
            seed = read_hex(Path(argv[2]), 32, "graine")
            print(sign(seed, Path(argv[3]).read_bytes()).hex())
            return 0
        if len(argv) == 3 and argv[1] == "public":
            print(public_key(read_hex(Path(argv[2]), 32, "graine")).hex())
            return 0
        if len(argv) == 3 and argv[1] == "new":
            target = Path(argv[2])
            fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "w") as handle:
                handle.write(os.urandom(32).hex() + "\n")
            print(public_key(read_hex(target, 32, "graine")).hex())
            return 0
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
