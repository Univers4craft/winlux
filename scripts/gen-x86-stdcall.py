#!/usr/bin/env python3
"""Table des arguments stdcall des API x86 (octets dépilés par l'appelé).

Source : les bibliothèques d'import i686 de MinGW-w64 (fichiers .def du
dossier lib32 de mingw-w64-crt, placés dans le domaine public), où chaque
fonction stdcall porte sa taille d'arguments dans son nom décoré
(« _RegSetKeySecurity@12 »). Aucune donnée Microsoft.

La passerelle wow32 en tire le `ret N` des fonctions x86 : sans elle, un
bouchon Rust sans argument posé sur une API à N arguments laisserait la
pile x86 décalée de N octets au retour (#232).

Usage : scripts/gen-x86-stdcall.py > runtime/src/wow32/stdcall.tsv
"""
import re
import subprocess
import sys

LIBDIR = "/usr/i686-w64-mingw32/lib"
DLLS = """advapi32 avrt bcrypt comctl32 comdlg32 crypt32 d2d1 d3d11 d3d9 dbghelp
dhcpcsvc dinput8 dnsapi dsound dwmapi dwrite dxgi dxva2 gdi32 gdiplus hid imm32
iphlpapi kernel32 mpr msi msimg32 ncrypt netapi32 ntdll ole32 oleacc oleaut32
opengl32 powrprof propsys psapi rpcrt4 secur32 sensapi setupapi shcore shell32
shfolder shlwapi urlmon user32 userenv usp10 uxtheme version windowscodecs
winhttp wininet winmm winspool wintrust winusb ws2_32 wtsapi32 xinput""".split()

SYM = re.compile(r" I __imp__([A-Za-z_][A-Za-z0-9_]*)@(\d+)$")


def main():
    print("# Octets d'arguments des fonctions stdcall x86, par DLL.")
    print("# Généré par scripts/gen-x86-stdcall.py depuis les .def i686 de MinGW-w64 (domaine public).")
    print("# Format : « [dll] » puis une ligne « fonction octets » par fonction.")
    for dll in DLLS:
        try:
            out = subprocess.run(
                ["nm", f"{LIBDIR}/lib{dll}.a"], capture_output=True, text=True, check=True
            ).stdout
        except (OSError, subprocess.CalledProcessError):
            print(f"lib{dll}.a absente", file=sys.stderr)
            continue
        seen = {}
        for line in out.splitlines():
            m = SYM.search(line)
            if m:
                seen.setdefault(m.group(1), int(m.group(2)))
        print("[xinput1_4]" if dll == "xinput" else f"[{dll}]")
        for func in sorted(seen):
            print(f"{func} {seen[func]}")


main()
