#!/usr/bin/python3
"""Pages de code asiatiques (#183) : les modules gconv de la glibc sont là.

Le runtime convertit 932, 936, 949 et 950 par iconv (runtime/src/intl/dbcs.rs) ;
sans module gconv, IsValidCodePage répond FALSE. Ce test vérifie :
1. que les quatre pages se convertissent sur cette machine, avec les octets
   attendus de Windows ;
2. qu'aucune exclusion dpkg de la construction de l'ISO ne retire les
   modules gconv (paquet libc6) ;
3. que le test de la VM (test-native-session-guest.py) le revérifie sur
   l'ISO installée.
"""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


samples = {
    "CP932": ("日本語", bytes([0x93, 0xfa, 0x96, 0x7b, 0x8c, 0xea])),
    "CP936": ("中文", bytes([0xd6, 0xd0, 0xce, 0xc4])),
    "CP949": ("한국어", bytes([0xc7, 0xd1, 0xb1, 0xb9, 0xbe, 0xee])),
    "CP950": ("中文", bytes([0xa4, 0xa4, 0xa4, 0xe5])),
}
for page, (text, raw) in samples.items():
    out = subprocess.run(["iconv", "-f", page, "-t", "UTF-8"], input=raw, capture_output=True)
    check(out.returncode == 0 and out.stdout.decode() == text, f"{page} : {out.stdout!r} {out.stderr!r}")

for script in list((ROOT / "scripts").glob("*.py")) + list((ROOT / "scripts").glob("*.sh")):
    if script.name.startswith("test-"):
        continue
    for line in script.read_text(errors="replace").splitlines():
        if re.search(r"path-exclude=[^\s\"']*(gconv|/usr/lib/\*|/usr/lib/x86_64-linux-gnu/\*)", line):
            check(False, f"{script.name} exclut les modules gconv : {line.strip()}")

guest = (ROOT / "scripts/test-native-session-guest.py").read_text()
check('"iconv", "-f", "CP932"' in guest, "la VM ne revérifie pas les modules gconv sur l'ISO")

if failures:
    sys.exit(1)
print("OK: pages 932, 936, 949 et 950 convertibles (gconv), modules gardés dans l'ISO (#183)")
