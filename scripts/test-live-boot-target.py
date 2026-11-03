#!/usr/bin/env python3
"""La clé Winlux démarre toujours son propre live, même si Winlux est installé.

En v0.2.17, l'entrée par défaut « Winlux Desktop » de la clé chargeait le noyau
et la racine du disque installé (étiquette WINLUX_ROOT) : impossible de réparer
ou de réinstaller depuis la clé. Contrôle statique du grub.cfg de la clé, puis,
si QEMU, grub-mkrescue et mke2fs sont là, démarrage réel du menu avec un disque
« installé » branché : l'entrée par défaut doit charger le noyau de la clé, et
une entrée à part doit proposer le système installé.
"""
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

project = Path(__file__).resolve().parent.parent
build = (project / "scripts/build-live-iso.sh").read_text()
cfg = re.search(r"cat >\"\$iso_root/boot/grub/grub.cfg\" <<'GRUB'\n(.*?)\nGRUB\n", build, re.S).group(1)


def entry(title):
    start = cfg.index(f'menuentry "{title}"')
    depth, i = 0, cfg.index("{", start)
    for j in range(i, len(cfg)):
        depth += {"{": 1, "}": -1}.get(cfg[j], 0)
        if depth == 0:
            return cfg[i:j + 1]
    raise AssertionError(title)


assert re.search(r"^set default=1$", cfg, re.M)
titles = re.findall(r'^menuentry "([^"]+)"', cfg, re.M)
assert titles[1] == "Winlux Desktop", titles  # l'entrée par défaut
live = cfg[cfg.index("function winlux_live {"):cfg.index("}", cfg.index("function winlux_live {"))]
assert "winlux_root" not in live and "rootfstype=ramfs" in live
for title in ("Winlux Desktop", "Installer Winlux sur le disque", "Vérifier l'intégrité du média puis démarrer"):
    body = entry(title)
    assert "winlux_root" not in body and "WINLUX_ROOT" not in body, (title, body)
assert 'menuentry "Démarrer le Winlux installé sur ce PC"' in cfg

tools = all(shutil.which(t) for t in ("grub-mkrescue", "xorriso", "qemu-system-x86_64", "mke2fs")) and Path("/usr/lib/grub/i386-pc").is_dir()
if not tools:
    print("test-live-boot-target : OK (vérifications statiques ; QEMU, grub-mkrescue ou mke2fs absent)")
    raise SystemExit(0)


def boot(temp, disk, keys=b""):
    serial = Path(temp) / f"serial-{time.monotonic_ns()}.txt"
    args = ["qemu-system-x86_64", "-m", "256", "-cdrom", str(Path(temp) / "live.iso"), "-boot", "d",
            "-display", "none", "-serial", f"file:{serial}", "-monitor", "none"]
    if disk:
        args += ["-drive", f"file={disk},format=raw,if=ide,index=0"]
    proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.time() + 90
    text = b""
    while time.time() < deadline:
        text = serial.read_bytes() if serial.exists() else b""
        if b"WINLUX-NOYAU-" in text:
            break
        time.sleep(1)
    proc.kill()
    proc.wait()
    return text


with tempfile.TemporaryDirectory() as temp:
    tree = Path(temp) / "iso"
    (tree / "boot/grub").mkdir(parents=True)
    # Faux noyaux : GRUB refuse de les charger, et son message dit lequel il a pris.
    (tree / "boot/vmlinuz").write_bytes(b"WINLUX-NOYAU-CLE" * 64)
    (tree / "boot/winlux-initramfs.cpio.gz").write_bytes(b"x")
    # Les erreurs de « linux » s'affichent ; un echo marque le noyau visé.
    test_cfg = cfg.replace("    linux /boot/vmlinuz console=tty1 console=ttyS0,115200n8 rootfstype=ramfs",
                           "    echo WINLUX-NOYAU-CLE\n    linux /boot/vmlinuz console=tty1 console=ttyS0,115200n8 rootfstype=ramfs")
    test_cfg = test_cfg.replace('            linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=tty1',
                                '            echo WINLUX-NOYAU-DISQUE\n            linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=tty1')
    test_cfg = test_cfg.replace("set timeout=5", "set timeout=1")
    (tree / "boot/grub/grub.cfg").write_text(test_cfg + "\n")
    subprocess.run(["grub-mkrescue", "-o", str(Path(temp) / "live.iso"), str(tree)], check=True, capture_output=True)

    installed = Path(temp) / "installed"
    (installed / "boot").mkdir(parents=True)
    (installed / "boot/vmlinuz").write_bytes(b"disque" * 64)
    (installed / "boot/winlux-split-root").write_text("1\n")
    disk = Path(temp) / "disk.img"
    subprocess.run(["mke2fs", "-q", "-t", "ext4", "-L", "WINLUX_ROOT", "-d", str(installed), str(disk), "32M"], check=True)

    with_disk = boot(temp, disk)
    assert b"WINLUX-NOYAU-CLE" in with_disk and b"WINLUX-NOYAU-DISQUE" not in with_disk, with_disk[-600:]
    # Le port série remplace les accents : « Winlux install? sur ce PC ».
    assert b"Winlux install" in with_disk, "entrée du système installé absente"
    alone = boot(temp, None)
    assert b"WINLUX-NOYAU-CLE" in alone, alone[-600:]
    assert b"Winlux install" not in alone, "entrée du système installé sans disque"
print("test-live-boot-target : OK (la clé démarre son live, le Winlux installé a son entrée à part ; vérifié sous QEMU)")
