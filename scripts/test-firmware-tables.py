#!/usr/bin/env python3
"""#1197 : GetSystemFirmwareTable / EnumSystemFirmwareTables ('RSMB').

Vérifie sans compiler que kernel32 exporte les deux fonctions (64 et
32 bits), que la surface de référence les liste et que le démarrage pose
une copie lisible par tous des tables SMBIOS, que le noyau réserve à root.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
fails = []


def need(cond, msg):
    if not cond:
        fails.append(msg)


apps = (ROOT / "runtime/src/dlls/apps_win32.rs").read_text(encoding="utf-8")
for name, fn in (("EnumSystemFirmwareTables", "enum_system_firmware_tables"),
                 ("GetSystemFirmwareTable", "get_system_firmware_table")):
    need(f'"{name}" => fn {fn}' in apps, f"export {name} absent de kernel32")
need('u32::from_be_bytes(*b"RSMB")' in apps, "fournisseur 'RSMB' absent")
need("/run/winlux/firmware" in apps, "copie de démarrage non lue par le runtime")

surface = (ROOT / "docs/compat/kernel32-surface.txt").read_text().split()
need("EnumSystemFirmwareTables" in surface and "GetSystemFirmwareTable" in surface,
     "surface kernel32 incomplète")

tsv = (ROOT / "runtime/src/wow32/stdcall.tsv").read_text().splitlines()
need("EnumSystemFirmwareTables 12" in tsv and "GetSystemFirmwareTable 16" in tsv,
     "tailles stdcall 32 bits absentes")

boot = (ROOT / "scripts/winlux-system-services.sh").read_text(encoding="utf-8")
need("/sys/firmware/dmi/tables/$table" in boot and "chmod 644 \"/run/winlux/firmware/$table\"" in boot,
     "copie des tables SMBIOS au démarrage absente")
need(boot.index("/run/winlux/firmware") < boot.index("dbus-daemon --system"),
     "copie SMBIOS posée trop tard")

if fails:
    for f in fails:
        print("ÉCHEC :", f)
    raise SystemExit(1)
print("OK : tables du micrologiciel (SMBIOS) pour GetSystemFirmwareTable (#1197).")
