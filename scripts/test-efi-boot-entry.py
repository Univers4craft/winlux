#!/usr/bin/env python3
"""Entrée de démarrage UEFI « Winlux » (NVRAM) en plus du chemin de secours.

Testeur VM : système installé en UEFI parti sur le shell UEFI, seul
\\EFI\\BOOT présent sur l'ESP et aucune entrée NVRAM. Le test vérifie que
l'installateur (graphique et install.sh) crée l'entrée « Winlux » vers le
chargeur de EFI/BOOT (shim ou GRUB), retire les anciennes, garde le chemin
de secours, et ne fait rien sans variables UEFI ni efibootmgr.
"""
import ast
import builtins
import io
import os
import pathlib
import re
import subprocess
import types

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/winlux-installer.py").read_text(encoding="utf-8")
tree = ast.parse(source)
node = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "efi_boot_entry")

calls = []
listing = "BootCurrent: 0001\nBoot0000* UiApp\tFvVol\nBoot0003* Winlux\tHD(2,GPT)\nBoot0004 Winlux\tHD(2,GPT)\nBoot0005* Winlux old\tx\n"


def fake_run(command, **_):
    calls.append(command)
    return types.SimpleNamespace(returncode=0, stdout=listing if command == ["efibootmgr"] else "")


real_open = builtins.open


def fake_open(path, *args, **kwargs):
    if path == "/sys/class/block/nvme0n1p2/partition":
        return io.StringIO("2\n")
    return real_open(path, *args, **kwargs)


fake_os = types.SimpleNamespace(path=types.SimpleNamespace(
    basename=os.path.basename, dirname=os.path.dirname,
    realpath=lambda p: {"/dev/nvme0n1p2": "/dev/nvme0n1p2",
                        "/sys/class/block/nvme0n1p2": "/sys/devices/pci0000:00/nvme/nvme0/nvme0n1/nvme0n1p2"}.get(p, p)))
writable = {"value": True}
ns = {"os": fake_os, "re": re, "subprocess": subprocess, "open": fake_open,
      "shutil": types.SimpleNamespace(which=lambda name: "/usr/bin/efibootmgr"),
      "efi_variables_writable": lambda: writable["value"]}
exec(compile(ast.Module(body=[node], type_ignores=[]), "installer", "exec"), ns)

assert ns["efi_boot_entry"]("/dev/nvme0n1p2", "\\EFI\\BOOT\\BOOTX64.EFI", run=fake_run) is True
deleted = [c[c.index("--bootnum") + 1] for c in calls if "--delete-bootnum" in c]
assert deleted == ["0003", "0004"], deleted
create = next(c for c in calls if "--create" in c)
assert create[create.index("--disk") + 1] == "/dev/nvme0n1" and create[create.index("--part") + 1] == "2", create
assert create[create.index("--loader") + 1] == "\\EFI\\BOOT\\BOOTX64.EFI" and create[create.index("--label") + 1] == "Winlux"
print("entrée « Winlux » créée vers \\EFI\\BOOT\\BOOTX64.EFI (NVMe, partition 2), anciennes retirées")

calls.clear()
writable["value"] = False
assert ns["efi_boot_entry"]("/dev/nvme0n1p2", "\\EFI\\BOOT\\BOOTIA32.EFI", run=fake_run) is False and not calls
print("sans variables UEFI (efi=noruntime) : rien, le chemin de secours suffit")

grub = source[source.index("        if layout[\"esp\"]:\n            if layout[\"own_esp\"]:"):source.index("        if split_root:\n            with open(f\"{target}/boot/winlux-split-root\"")]
assert '["--removable", "--no-nvram"]' in grub, "chemin de secours EFI/BOOT perdu"
assert grub.index("install-esp") < grub.index("efi_boot_entry(layout[\"esp\"], loader)")
assert "BOOTIA32.EFI" in grub and "BOOTX64.EFI" in grub
assert '"--bootloader-id=winlux"' in grub, "ESP partagé avec Windows : entrée propre conservée"
install = (ROOT / "scripts/install.sh").read_text(encoding="utf-8")
assert install.index("install-esp") < install.index("efibootmgr --quiet --create --disk \"$device\" --part 2 --label Winlux")
print("installateur graphique et install.sh : EFI/BOOT gardé, entrée créée après la chaîne Secure Boot")
