#!/usr/bin/env python3
"""Fige la surface d'exports de kernel32 (#521).

GetProcAddress(kernel32, nom) ne doit rendre que ce que kernel32.dll exporte
sous Windows : un export en trop fait prendre à une appli un chemin que
Windows ne lui fait jamais prendre (VLC, #495).

Vérifie, à partir de `winlux --exports-kernel32` et `winlux --exports` :
  1. chaque nom visible est dans docs/compat/kernel32-reference.txt ou
     commence par « Winlux » (API privées) ;
  2. la surface est exactement docs/compat/kernel32-surface.txt (toute
     évolution est donc délibérée) ;
  3. les fonctions cachées restent disponibles là où Windows les met.

Mettre à jour la surface après un ajout justifié :
  target/release/winlux --exports-kernel32 > docs/compat/kernel32-surface.txt
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REFERENCE = os.path.join(ROOT, "docs/compat/kernel32-reference.txt")
SURFACE = os.path.join(ROOT, "docs/compat/kernel32-surface.txt")

# Fonctions retirées de kernel32 et la DLL qui doit les fournir.
ELSEWHERE = {
    "WaitOnAddress": "kernel32",  # par KernelBase / api-ms-win-core-synch
    "EnumProcessModules": "psapi",
    "EnumProcessModulesEx": "psapi",
    "GetModuleInformation": "psapi",
    "GetPerformanceInfo": "psapi",
    "GetProcessMemoryInfo": "psapi",
    "QueryWorkingSetEx": "psapi",
    "GetModuleBaseNameW": "psapi",
    "GetModuleFileNameExW": "psapi",
    "GetUserNameA": "advapi32",
    "GetUserNameW": "advapi32",
    "LoadStringA": "user32",
    "LoadStringW": "user32",
    "RtlAddVectoredExceptionHandler": "ntdll",
    "RtlAddGrowableFunctionTable": "ntdll",
}
# Échantillon de noms que kernel32.dll n'exporte pas (repli ntdll / msvcrt / advapi32).
NEVER = ["NtQueryInformationProcess", "RtlGetVersion", "memcpy", "strlen", "RegOpenKeyA", "RegQueryValueA"]


def winlux():
    for kind in ("release", "debug"):
        path = os.path.join(ROOT, "target", kind, "winlux")
        if os.access(path, os.X_OK):
            return path
    subprocess.run(["cargo", "build", "-q", "-p", "winlux"], cwd=ROOT, check=True)
    return os.path.join(ROOT, "target", "debug", "winlux")


def names(path):
    out = set()
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if line:
                out.add(line)
    return out


def main():
    exe = winlux()
    surface = subprocess.run([exe, "--exports-kernel32"], capture_output=True, text=True, check=True).stdout.split()
    exports = {}
    for line in subprocess.run([exe, "--exports"], capture_output=True, text=True, check=True).stdout.splitlines():
        dll, _, name = line.partition("\t")
        exports.setdefault(dll, set()).add(name)
    errors = []
    reference = names(REFERENCE)
    foreign = sorted(n for n in surface if n not in reference and not n.startswith("Winlux"))
    if foreign:
        errors.append(
            "kernel32 exporte des noms absents de la référence Windows "
            "(les mettre dans la bonne DLL, ou sourcer l'ajout dans kernel32-reference.txt) :\n    "
            + "\n    ".join(foreign)
        )
    for name in NEVER + [n for n, dll in ELSEWHERE.items() if dll != "kernel32"] + ["WaitOnAddress"]:
        if name in surface:
            errors.append(f"GetProcAddress(kernel32, {name!r}) doit rendre NULL")
    for name, dll in ELSEWHERE.items():
        if name not in exports.get(dll, set()):
            errors.append(f"{name} doit rester disponible dans {dll}")
    frozen = names(SURFACE)
    added, removed = sorted(set(surface) - frozen), sorted(frozen - set(surface))
    if added or removed:
        errors.append(
            "la surface de kernel32 a changé ; si c'est voulu : "
            "target/release/winlux --exports-kernel32 > docs/compat/kernel32-surface.txt\n"
            + "".join(f"    + {n}\n" for n in added)
            + "".join(f"    - {n}\n" for n in removed)
        )
    if errors:
        print("ÉCHEC surface kernel32 :\n- " + "\n- ".join(errors))
        return 1
    print(f"OK : {len(surface)} exports kernel32, tous dans la référence ou privés Winlux.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
