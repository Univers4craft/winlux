#!/usr/bin/python3
"""Trois chemins d'amorçage : BIOS, UEFI 64 bits et UEFI 32 bits.

Un processeur 64 bits peut avoir un micrologiciel UEFI 32 bits (tablettes
Atom Bay Trail / Cherry Trail). Le système reste 64 bits ; seul le chargeur
existe aussi en 32 bits (EFI/BOOT/BOOTIA32.EFI). Vérifie :
  - le choix des chargeurs par l'installateur et la réinitialisation
    (les deux sur une partition EFI à nous, celui du micrologiciel sur une
    partition partagée avec Windows) ;
  - le câblage : paquets GRUB 32 bits dans l'image et sur les runners, contrôle
    de l'ISO à la construction, démarrage uefi32 dans la validation ;
  - si grub-mkrescue et les modules sont là, une petite ISO porte bien les trois.
Le démarrage réel sous QEMU (OVMF 32 bits) est fait par
installation/tests/test-block-device.sh et par la validation de release.yml.
"""
import ast
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

project = Path(__file__).resolve().parent.parent

with tempfile.TemporaryDirectory() as temp:
    grub = Path(temp) / "grub"
    (grub / "x86_64-efi").mkdir(parents=True)
    for script in ("scripts/winlux-installer.py", "scripts/winlux-reset.py"):
        tree_ast = ast.parse((project / script).read_text())
        functions = [node for node in tree_ast.body if isinstance(node, ast.FunctionDef)
                     and node.name in ("firmware_efi_bits", "efi_grub_targets")]
        namespace = {"os": __import__("os"), "Path": Path}
        exec(compile(ast.Module(body=functions, type_ignores=[]), script, "exec"), namespace)
        targets = namespace["efi_grub_targets"]
        # Sans modules 32 bits, rien ne change.
        assert targets(True, str(grub)) == ["x86_64-efi"], script
        (grub / "i386-efi").mkdir(exist_ok=True)
        assert targets(True, str(grub)) == ["x86_64-efi", "i386-efi"], script
        # Partition partagée : chargeur du micrologiciel seulement (ici 64 bits ou inconnu).
        bits = Path("/sys/firmware/efi/fw_platform_size")
        expected = ["i386-efi"] if bits.exists() and bits.read_text().strip() == "32" else ["x86_64-efi"]
        assert targets(False, str(grub)) == expected, script
        (grub / "i386-efi").rmdir()

installer = (project / "scripts/winlux-installer.py").read_text()
assert 'for efi_target in efi_grub_targets(layout["own_esp"])' in installer
reset = (project / "scripts/winlux-reset.py").read_text()
assert reset.count("efi_grub_targets(") == 3, "réinitialisation ou clé de secours sans BOOTIA32"
install_sh = (project / "scripts/install.sh").read_text()
assert "grub-install --target=i386-efi" in install_sh
assert '"grub-efi-ia32-bin"' in (project / "scripts/prepare-apt-base.py").read_text()
release = (project / ".github/workflows/release.yml").read_text()
assert "grub-efi-ia32-bin" in release and "fetch-ovmf-ia32.sh" in release and 'RELEASE_NAME.iso" uefi32' in release
usb = (project / ".github/workflows/installation.yml").read_text()
assert "grub-efi-ia32-bin" in usb and "fetch-ovmf-ia32.sh" in usb and 'WINLUX_OVMF32="$WINLUX_OVMF32"' in usb
assert "uefi32" in (project / "installation/tests/test-block-device.sh").read_text()
assert "uefi32" in (project / "scripts/test-install-vm-guest.sh").read_text()
assert "uefi32" in (project / "scripts/test-live-iso-qemu.sh").read_text()
build = (project / "scripts/build-live-iso.sh").read_text()
assert "for platform in i386-pc x86_64-efi i386-efi" in build
assert 'check-iso-boot-paths.sh" "$out"' in build
# Terminal graphique de GRUB + UEFI 32 bits = noyau 64 bits planté (mode mixte) :
# chaque grub.cfg généré reste en texte sur efi-i386.
for script in ("scripts/build-live-iso.sh", "scripts/install.sh"):
    assert '= "efi-i386" ]; then\n    set winlux_term=console\nelif loadfont' in (project / script).read_text(), script
assert reset.count('efi-i386\\" ]; then\\n    set winlux_term=console\\nelif loadfont') == 2
assert 'ia32_loader "$iso_root"' in (project / "scripts/winlux-secureboot.sh").read_text()

if all(shutil.which(tool) for tool in ("grub-mkrescue", "xorriso", "mdir")) and all(
        Path(f"/usr/lib/grub/{p}").is_dir() for p in ("i386-pc", "x86_64-efi", "i386-efi")):
    with tempfile.TemporaryDirectory() as temp:
        tree = Path(temp) / "iso"
        (tree / "boot/grub").mkdir(parents=True)
        (tree / "boot/grub/grub.cfg").write_text("echo winlux\n")
        iso = Path(temp) / "test.iso"
        subprocess.run(["grub-mkrescue", "-o", str(iso), str(tree)], check=True, capture_output=True)
        subprocess.run(["sh", str(project / "scripts/check-iso-boot-paths.sh"), str(iso)], check=True,
                       stdout=subprocess.DEVNULL)
    print("test-boot-paths : OK (BIOS, UEFI 64 et UEFI 32 bits, ISO de test vérifiée)")
else:
    print("test-boot-paths : OK (BIOS, UEFI 64 et UEFI 32 bits ; ISO de test non construite, GRUB absent)")
