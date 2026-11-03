#!/usr/bin/env python3
"""Menu GRUB : flèches du clavier utilisables sur un vrai PC.

Sur un PC portable, les flèches ne déplaçaient pas la sélection dans le menu de
la clé Winlux (v0.2.16). Le terminal série de GRUB (port sans rien au bout, ou
port série exposé par l'UEFI) ralentit la lecture du clavier. Il est donc
réservé aux machines virtuelles QEMU/KVM, où les tests pilotent le menu par le
port série. Le menu de la clé laisse aussi 5 s pour choisir.

Si QEMU et grub-mkrescue sont présents, le vrai grub.cfg de la clé est démarré
deux fois : avec le fabricant « QEMU », le menu passe par le port série ; avec
un fabricant de PC réel, plus rien n'est écrit sur ce port.
"""
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

project = Path(__file__).resolve().parent.parent

shipped = {
    "scripts/build-live-iso.sh": 1,   # menu de la clé / de l'ISO
    "scripts/install.sh": 1,          # disque installé par install.sh
    "scripts/winlux-installer.py": 1, # disque installé par l'installateur graphique
    "scripts/winlux-reset.py": 2,     # réinitialisation et clé de secours
}
for script, count in shipped.items():
    text = (project / script).read_text()
    serials = text.count("serial --unit=0")
    assert serials == count, (script, serials)
    gates = text.count("smbios --type 1 --get-string 4 --set winlux_vendor")
    assert gates == count, (script, gates)
    # Chaque « serial --unit » vient après le test du fabricant.
    for match in re.finditer(r"serial --unit=0", text):
        before = text[max(0, match.start() - 400):match.start()]
        assert "QEMU" in before and "winlux_vendor" in before, (script, text[match.start() - 200:match.start() + 40])

build = (project / "scripts/build-live-iso.sh").read_text()
live_cfg = re.search(r"cat >\"\$iso_root/boot/grub/grub.cfg\" <<'GRUB'\n(.*?)\nGRUB\n", build, re.S).group(1)
assert re.search(r"^set timeout=5$", live_cfg, re.M), "délai du menu de la clé"

tools = all(shutil.which(t) for t in ("grub-mkrescue", "xorriso", "qemu-system-x86_64")) and Path("/usr/lib/grub/i386-pc").is_dir()
if tools:
    with tempfile.TemporaryDirectory() as temp:
        tree = Path(temp) / "iso"
        (tree / "boot/grub").mkdir(parents=True)
        (tree / "boot/grub/grub.cfg").write_text(live_cfg + "\n")
        iso = Path(temp) / "menu.iso"
        subprocess.run(["grub-mkrescue", "-o", str(iso), str(tree)], check=True, capture_output=True)
        for vendor, expect_menu in (("QEMU", True), ("LENOVO", False)):
            serial = Path(temp) / f"serial-{vendor}.txt"
            proc = subprocess.Popen(["qemu-system-x86_64", "-m", "256", "-cdrom", str(iso), "-display", "none",
                                     "-smbios", f"type=1,manufacturer={vendor}", "-serial", f"file:{serial}"],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            deadline = time.time() + 60
            seen = False
            while time.time() < deadline:
                if serial.exists() and b"Winlux Desktop" in serial.read_bytes():
                    seen = True
                    break
                if not expect_menu and time.time() > deadline - 40:
                    break  # 20 s sans menu sur le port série suffisent
                time.sleep(1)
            proc.kill()
            proc.wait()
            assert seen == expect_menu, (vendor, serial.read_bytes()[:400] if serial.exists() else b"")
    print("test-grub-keyboard : OK (terminal série réservé aux VM, vérifié sous QEMU ; délai 5 s)")
else:
    print("test-grub-keyboard : OK (vérifications statiques ; QEMU ou grub-mkrescue absent)")
