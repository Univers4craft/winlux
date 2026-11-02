#!/usr/bin/env python3
"""Réinitialiser ce PC (#283) : rien de ce qui doit être gardé ne disparaît.

Système « installé » factice avec des fichiers témoins, image d'origine en
SquashFS, puis les deux étapes réelles : `winlux-reset prepare` (Python) et
l'échange de l'initramfs (scripts/winlux-reset-apply.sh, BusyBox si présent).
Vérifie octet par octet les fichiers gardés, les comptes, le noyau, la reprise
après une coupure au milieu de l'échange, l'annulation et les refus.
"""
import gzip
import hashlib
import importlib.util
from importlib.machinery import SourceFileLoader
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
RESET = HERE / "winlux-reset.py"
APPLY = HERE / "winlux-reset-apply.sh"
BUSYBOX = shutil.which("busybox")


def load(path, name):
    loader = SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def write(path, text, mode=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text) if isinstance(text, str) else path.write_bytes(text)
    if mode is not None:
        os.chmod(path, mode)


def digest_tree(root):
    out = {}
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            out[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def old_system(base):
    root = base / "installed"
    write(root / "etc/passwd", "root:x:0:0:root:/root:/bin/bash\nmessagebus:x:100:101::/nonexistent:/usr/sbin/nologin\n"
          "alice:x:1000:1000:Alice Martin:/home/alice:/bin/bash\nbob:x:1001:1001:Bob:/home/bob:/bin/bash\n"
          "winlux-web:!:1002:1002:Winlux browser:/home/winlux-web:/bin/sh\n")
    write(root / "etc/group", "root:x:0:\nsudo:x:27:alice\naudio:x:29:alice,bob\nshadow:x:42:\nmessagebus:x:101:\n"
          "alice:x:1000:\nbob:x:1001:\nwinlux-web:x:1002:\n")
    write(root / "etc/shadow", "root:!:19000:0:99999:7:::\nalice:$6$abc$ALICEHASH:19000:0:99999:7:::\n"
          "bob:$6$def$BOBHASH:19000:0:99999:7:::\n", 0o640)
    write(root / "etc/gshadow", "root:*::\nsudo:*::alice\naudio:*::alice,bob\nalice:!::\nbob:!::\n", 0o640)
    write(root / "etc/sudoers.d/alice", "alice ALL=(ALL:ALL) ALL\n", 0o440)
    write(root / "etc/hostname", "pc-de-test\n")
    write(root / "etc/fstab", "# fstab Winlux\n")
    write(root / "etc/winlux/install-config.json", json.dumps({"username": "alice", "autologin": True}))
    write(root / "etc/winlux/locale.conf", "LANG=fr\n")
    write(root / "etc/winlux/preferences.conf", "THEME=dark\n")
    write(root / "etc/NetworkManager/system-connections/Maison.nmconnection", "[wifi]\nssid=Maison\n", 0o600)
    write(root / "usr/bin/ancienne-appli", "#!/bin/sh\n", 0o755)
    write(root / "usr/share/winlux/version", "v0.2.14\n")
    write(root / "var/lib/dpkg/status", "Package: bash\nStatus: install ok installed\n\n"
          "Package: gimp\nStatus: install ok installed\n\n")
    write(root / "opt/restes/fichier", "vieux\n")
    write(root / "init", "#!/bin/sh\necho ancien\n", 0o755)
    write(root / "swapfile", b"\0" * 4096, 0o600)
    write(root / "boot/vmlinuz", "ANCIEN NOYAU\n")
    write(root / "boot/winlux-initramfs.cpio.gz", gzip.compress(b"ancien initramfs"))
    write(root / "boot/winlux-split-root", "1\n")
    write(root / "boot/grub/grub.cfg", "menuentry \"Winlux Desktop\" {\n}\n")
    write(root / "boot/grub/i386-pc/core.img", b"GRUB")
    (root / "lost+found").mkdir()
    for user in ("alice", "bob"):
        for index in range(5):
            write(root / f"home/{user}/Documents/témoin-{index}.bin", os.urandom(4096 + index))
        write(root / f"home/{user}/.config/réglage.ini", f"{user}\n")
        (root / f"home/{user}/Bureau").mkdir(parents=True, exist_ok=True)
        os.symlink("Documents/témoin-0.bin", root / f"home/{user}/lien")
    write(root / "home/alice/.wine-like/drive_c/Program Files/Notepad++/notepad++.exe", b"MZ" + os.urandom(2000))
    return root


def new_tree(base):
    tree = base / "factory"
    write(tree / "etc/passwd", "root:x:0:0:Winlux:/root:/bin/sh\nmessagebus:x:100:101::/nonexistent:/usr/sbin/nologin\n"
          "systemd-network:x:998:998::/:/usr/sbin/nologin\n")
    write(tree / "etc/group", "root:x:0:\nsudo:x:27:\naudio:x:29:root\nvideo:x:44:root\nshadow:x:42:\n"
          "messagebus:x:101:\nsystemd-network:x:998:\n")
    write(tree / "etc/shadow", "root:!:19000:0:99999:7:::\n", 0o640)
    write(tree / "etc/gshadow", "root:*::\nsudo:*::\naudio:*::root\nvideo:*::root\n", 0o640)
    write(tree / "etc/hostname", "winlux\n")
    write(tree / "etc/winlux/preferences.conf", "THEME=light\n")
    write(tree / "usr/bin/nouvelle-appli", "#!/bin/sh\n", 0o755)
    write(tree / "usr/share/winlux/version", "v0.2.15\n")
    write(tree / "usr/share/applications/winlux-installer.desktop", "[Desktop Entry]\n")
    write(tree / "var/lib/dpkg/status", "Package: bash\nStatus: install ok installed\n\n")
    write(tree / "init", "#!/bin/sh\necho nouveau\n", 0o755)
    for name in ("dev", "proc", "sys", "run", "tmp", "home", "boot"):
        (tree / name).mkdir(exist_ok=True)
    medium = base / "medium"
    write(medium / "boot/vmlinuz", "NOUVEAU NOYAU\n")
    write(medium / "boot/winlux-initramfs.cpio.gz", gzip.compress(b"070701 init . /reset-apply.sh\nwinlux_reset_apply"))
    subprocess.run(["mksquashfs", str(tree), str(medium / "boot/winlux-rootfs.squashfs"), "-noappend", "-quiet",
                    "-all-root"], check=True, stdout=subprocess.DEVNULL)
    return medium


def reset(root, work, *args, password=None):
    env = dict(os.environ, WINLUX_RESET_ROOT=str(root), WINLUX_RESET_WORK=str(work), WINLUX_LANG="fr")
    return subprocess.run([sys.executable, "-B", str(RESET), *args], input=(password or "") + "\n",
                          capture_output=True, text=True, env=env, timeout=600)


def apply(root, busybox=None):
    tool = busybox or BUSYBOX or "env"
    return subprocess.run(["sh", "-c", f'B="{tool}"; . "{APPLY}"; winlux_reset_apply "$1"', "apply", str(root)],
                          capture_output=True, text=True, timeout=300)


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def expect_ok(result, what):
    check(result.returncode == 0 and "ERROR" not in result.stdout,
          f"{what} : code {result.returncode}\n{result.stdout}\n{result.stderr}")


def test_keep(base, medium):
    root = old_system(base / "keep")
    homes = digest_tree(root / "home")
    old_hash = (root / "etc/shadow").read_text()
    result = reset(root, base / "work-keep", "prepare", "--mode", "keep", "--source", f"media:{medium}")
    expect_ok(result, "préparation (conserver)")
    check("DONE" in result.stdout, result.stdout)
    # Rien n'est effacé avant le redémarrage : seuls /boot et /.winlux-reset ont changé.
    check(digest_tree(root / "home") == homes, "préparation : /home modifié")
    check((root / "usr/bin/ancienne-appli").exists(), "préparation : système modifié trop tôt")
    check((root / "boot/vmlinuz").read_text() == "NOUVEAU NOYAU\n", "noyau de la nouvelle version non posé")
    check((root / "boot/vmlinuz.before-reset").read_text() == "ANCIEN NOYAU\n", "ancien noyau non gardé")
    result = apply(root)
    check(result.returncode == 0, f"échange : {result.stdout}{result.stderr}")
    check(digest_tree(root / "home") == {**homes, **{k: v for k, v in digest_tree(root / "home").items() if k not in homes}},
          "un fichier personnel a changé")
    for name, value in homes.items():
        check(digest_tree(root / "home").get(name) == value, f"fichier personnel perdu ou modifié : {name}")
    check(os.readlink(root / "home/alice/lien") == "Documents/témoin-0.bin", "lien personnel perdu")
    check((root / "usr/bin/nouvelle-appli").exists() and not (root / "usr/bin/ancienne-appli").exists(), "système non remplacé")
    check(not (root / "opt").exists(), "ancien dossier système resté")
    check((root / "init").read_text() == "#!/bin/sh\necho nouveau\n", "init non remplacé")
    check(not (root / "usr/share/applications/winlux-installer.desktop").exists(), "lanceur de l'installateur resté")
    passwd = (root / "etc/passwd").read_text()
    check("alice:x:1000:1000:Alice Martin:/home/alice:/bin/bash" in passwd and "bob:x:1001:" in passwd, "comptes perdus")
    check("systemd-network:x:998" in passwd and "winlux-web" not in passwd, "comptes de service incohérents")
    shadow = (root / "etc/shadow").read_text()
    check("alice:$6$abc$ALICEHASH:" in shadow and "bob:$6$def$BOBHASH:" in shadow and old_hash, "mots de passe perdus")
    groups = {line.split(":")[0]: line.split(":")[3] for line in (root / "etc/group").read_text().splitlines()}
    check(groups["sudo"] == "alice" and set(groups["audio"].split(",")) == {"root", "alice", "bob"}, f"groupes : {groups}")
    check("video" in groups and "alice" in groups, "groupes perdus")
    check((root / "etc/sudoers.d/alice").exists(), "droits administrateur perdus")
    check((root / "etc/hostname").read_text() == "pc-de-test\n", "nom du PC perdu")
    check((root / "etc/winlux/preferences.conf").read_text() == "THEME=light\n", "réglages non remis d'origine")
    check(json.loads((root / "etc/winlux/install-config.json").read_text())["username"] == "alice", "compte du bureau perdu")
    check((root / "etc/NetworkManager/system-connections/Maison.nmconnection").exists(), "Wi-Fi perdu")
    check((root / "swapfile").stat().st_size == 4096 and (root / "lost+found").is_dir(), "fichier d'échange perdu")
    check((root / "boot/grub/i386-pc/core.img").read_bytes() == b"GRUB", "GRUB touché")
    check(not (root / "boot/vmlinuz.before-reset").exists() and not (root / ".winlux-reset").exists(), "restes de l'échange")
    report = root / "home/alice/Bureau/Applications supprimées.txt"
    check(report.is_file() and "gimp" in report.read_text(), "liste des applications supprimées absente")
    print("conserver mes fichiers : OK")


def test_wipe(base, medium):
    root = old_system(base / "wipe")
    result = reset(root, base / "work-wipe", "prepare", "--mode", "wipe", "--source", f"media:{medium}",
                   "--user", "claire", "--fullname", "Claire Dupont", password="Secret-123")
    expect_ok(result, "préparation (tout supprimer)")
    check((root / "home/alice/Documents").is_dir(), "tout supprimer : effacé avant le redémarrage")
    result = apply(root)
    check(result.returncode == 0, f"échange : {result.stdout}{result.stderr}")
    check(sorted(p.name for p in (root / "home").iterdir()) == ["claire"], "anciens dossiers personnels restés")
    leftovers = [str(p) for p in root.rglob("*") if "témoin" in p.name or p.name == "ancienne-appli" or p.name == "Maison.nmconnection"]
    check(not leftovers, f"fichiers non supprimés : {leftovers}")
    passwd = (root / "etc/passwd").read_text()
    check("alice" not in passwd and "bob" not in passwd and "claire:x:1000:1000:Claire Dupont:/home/claire:/bin/bash" in passwd,
          f"comptes : {passwd}")
    shadow = (root / "etc/shadow").read_text()
    check("claire:$6$" in shadow and "Secret-123" not in shadow and "ALICEHASH" not in shadow, "mot de passe du nouveau compte")
    check("claire" in [line for line in (root / "etc/group").read_text().splitlines() if line.startswith("sudo:")][0],
          "nouveau compte non administrateur")
    for folder in ("Bureau", "Documents", "Téléchargements"):
        check((root / "home/claire" / folder).is_dir(), f"dossier {folder} absent")
    check(json.loads((root / "etc/winlux/install-config.json").read_text())["username"] == "claire", "compte du bureau")
    check((root / "etc/hostname").read_text() == "pc-de-test\n", "nom du PC")
    check((root / "boot/grub/i386-pc/core.img").exists() and (root / "swapfile").exists(), "démarrage ou échange touchés")
    print("tout supprimer : OK")


def test_interrupted(base, medium):
    root = old_system(base / "coupure")
    homes = digest_tree(root / "home")
    expect_ok(reset(root, base / "work-cut", "prepare", "--mode", "keep", "--source", f"media:{medium}"), "préparation")
    # Coupure de courant simulée : le 3e renommage échoue (comme si le PC s'éteignait).
    tool = BUSYBOX or "env"
    wrapper = base / "coupure-busybox"
    write(wrapper, f'#!/bin/sh\nif [ "$1" = mv ]; then n=$(cat {base}/mv-count 2>/dev/null || echo 0); n=$((n+1)); '
          f'echo $n >{base}/mv-count; [ $n -ge 3 ] && exit 1; fi\nexec {tool} "$@"\n', 0o755)
    result = apply(root, str(wrapper))
    check(result.returncode != 0 and (root / ".winlux-reset/ready").exists(), "la coupure n'a pas été simulée")
    check(digest_tree(root / "home") == homes, "coupure : fichiers personnels touchés")
    result = apply(root)
    check(result.returncode == 0, f"reprise : {result.stdout}{result.stderr}")
    check(digest_tree(root / "home") == {**homes, **{k: v for k, v in digest_tree(root / "home").items() if k not in homes}},
          "reprise : fichiers personnels")
    check((root / "usr/bin/nouvelle-appli").exists() and not (root / "opt").exists(), "reprise : système incomplet")
    check(not (root / ".winlux-reset").exists(), "reprise : état resté")
    print("coupure pendant l'échange puis reprise : OK")


def test_cancel_and_refusals(base, medium):
    root = old_system(base / "annuler")
    expect_ok(reset(root, base / "work-cancel", "prepare", "--mode", "keep", "--source", f"media:{medium}"), "préparation")
    result = reset(root, base / "work-cancel", "cancel")
    expect_ok(result, "annulation")
    check((root / "boot/vmlinuz").read_text() == "ANCIEN NOYAU\n" and not (root / ".winlux-reset").exists(), "annulation incomplète")
    check(apply(root).returncode == 0 and (root / "usr/bin/ancienne-appli").exists(), "annulé mais appliqué")
    write(root / "etc/fstab", "UUID=1234 /var ext4 defaults 0 2\n")
    result = reset(root, base / "work-cancel", "prepare", "--mode", "keep", "--source", f"media:{medium}")
    check(result.returncode == 1 and "Partition séparée pour /var" in result.stdout, f"/var séparé accepté : {result.stdout}")
    write(root / "etc/fstab", "UUID=1234 /home ext4 defaults 0 2\n")
    result = reset(root, base / "work-cancel", "prepare", "--mode", "wipe", "--source", f"media:{medium}",
                   "--user", "Root!", password="x")
    check(result.returncode == 1 and "Nom de compte invalide" in result.stdout, "nom de compte invalide accepté")
    old = base / "ancien-support"
    shutil.copytree(medium, old)
    write(old / "boot/winlux-initramfs.cpio.gz", gzip.compress(b"init sans reinitialisation"))
    result = reset(root, base / "work-cancel", "prepare", "--mode", "keep", "--source", f"media:{old}")
    check(result.returncode == 1 and "trop ancienne" in result.stdout, f"initramfs ancien accepté : {result.stdout}")
    os.environ["WINLUX_LANG"] = "de"
    result = reset(root, base / "work-cancel", "prepare", "--mode", "keep", "--source", "media:/nulle/part")
    check(result.returncode == 1 and "ERROR" in result.stdout, "source absente acceptée")
    del os.environ["WINLUX_LANG"]
    print("annulation et refus : OK")


def test_boot_menus(base):
    module = load(RESET, "winlux_reset")
    cfg = base / "grub.cfg"
    cfg.write_text(module.grub_template("1111-2222", "UUID=1111-2222", ""))
    module.enable_next_recovery(cfg)
    module.enable_next_recovery(cfg)
    text = cfg.read_text()
    check(text.count(module.NEXT_RECOVERY_BLOCK) == 1, "bloc du menu de secours dupliqué")
    check('set default="Winlux secours (restauration ou console)"' in text, "entrée de secours non choisie")
    key = base / "key.cfg"
    module.LANG = "es"
    key.write_text(module.key_grub_cfg("UUID=1111-2222", " winlux.luks=UUID=abcd", True))
    check("Menú de recuperación de este PC" in key.read_text() and "winlux.recovery=1" in key.read_text(), "menu de la clé")
    check("winlux.luks=UUID=abcd" in key.read_text(), "disque chiffré oublié par la clé")
    if shutil.which("grub-script-check"):
        for path in (cfg, key):
            subprocess.run(["grub-script-check", str(path)], check=True)
    for text, row in module.TEXT.items():
        check(len(row) == 3 and all(row), f"traduction manquante : {text}")
    page = (HERE / "winlux-reset-page.py").read_text()
    namespace = {}
    start = page.index("_TR = {")
    exec(page[start:page.index("\n}\n", start) + 3], namespace)
    for text, row in namespace["_TR"].items():
        check(len(row) == 3 and all(row), f"traduction manquante (Paramètres) : {text}")
    recovery = (HERE / "winlux-recovery.sh").read_text()
    for key_name in ("m7", "keep", "wipe", "confirm", "restart_now"):
        for lang in ("en", "es", "de", "*"):
            check(f"{lang}:{key_name})" in recovery, f"menu de secours : {lang}:{key_name} manquant")
    subprocess.run(["sh", "-n", str(HERE / "winlux-recovery.sh")], check=True)
    subprocess.run(["sh", "-n", str(APPLY)], check=True)
    subprocess.run(["sh", "-n", str(HERE / "winlux-live-init.sh")], check=True)
    check("reset-apply.sh" in (HERE / "build-live-root.sh").read_text(), "initramfs sans reset-apply.sh")
    root_tool = load(HERE / "winlux-root.py", "winlux_root")
    check(root_tool.exec_level(["winlux-reset", "status"]) == "user", "état réservé à l'administrateur")
    check(root_tool.exec_level(["winlux-reset", "prepare", "--mode", "wipe", "--source", "cloud", "--user", "bob"]) == "admin",
          "réinitialisation sans administrateur")
    check(root_tool.exec_level(["winlux-reset", "make-key", "--device", "/dev/sdb", "--source", "cloud"]) == "admin", "clé")
    check(root_tool.exec_level(["winlux-reset", "rm", "-rf"]) is None, "commande inconnue relayée")
    print("menus de démarrage, traductions, relais root : OK")


def main():
    if not shutil.which("mksquashfs") or not shutil.which("unsquashfs"):
        print("squashfs-tools absent : test ignoré", file=sys.stderr)
        return 0
    with tempfile.TemporaryDirectory(prefix="winlux-reset-") as temporary:
        base = Path(temporary)
        medium = new_tree(base)
        test_keep(base, medium)
        test_wipe(base, medium)
        test_interrupted(base, medium)
        test_cancel_and_refusals(base, medium)
        test_boot_menus(base)
    print("winlux-reset : tous les tests sont passés")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
