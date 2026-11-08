#!/usr/bin/env python3
"""Mises à jour A/B (#31) : winlux-ab prepare installe la nouvelle version dans
l'emplacement B sans toucher au système actif (liens physiques pour ce qui ne
change pas), refuse un paquet qui demande un paquet Ubuntu absent, puis
`winlux --ab-boot` échange les emplacements et la réserve revient sur demande."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
tool = project / "scripts/winlux-ab.py"
failures = []


def ok(condition, message):
    print(("ok   " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


def deb(work, version, depends=None, extra=b"meme contenu\n"):
    tree = work / f"pkg-{version}-{len(list(work.glob('pkg-*')))}"
    (tree / "DEBIAN").mkdir(parents=True)
    control = f"Package: winlux-desktop\nVersion: {version}\nArchitecture: all\nMaintainer: Winlux <winlux@localhost>\nDescription: test\n"
    if depends:
        control += f"Depends: {depends}\n"
    (tree / "DEBIAN/control").write_text(control)
    (tree / "DEBIAN/conffiles").write_text("/etc/apt/apt.conf.d/90winlux-desktop\n")
    files = {"usr/share/winlux/version": f"v{version}\n", "usr/share/winlux/stable": extra.decode(),
             "opt/winlux/tests/lisezmoi": f"{version}\n", "init": f"#!/bin/sh\n# {version}\n",
             "etc/apt/apt.conf.d/90winlux-desktop": f"// {version}\n"}
    for name, text in files.items():
        (tree / name).parent.mkdir(parents=True, exist_ok=True)
        (tree / name).write_text(text)
    output = tree.with_suffix(".deb")
    subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(tree), str(output)], check=True, capture_output=True)
    return output


def ab(root, *args):
    return subprocess.run([sys.executable, "-B", str(tool), *map(str, args)], capture_output=True, text=True,
                          env=dict(os.environ, WINLUX_AB_ROOT=str(root)))


def read(path):
    return path.read_text().strip()


with tempfile.TemporaryDirectory() as temp:
    work = Path(temp)
    root = work / "racine"
    admin = root / "var/lib/dpkg"
    for folder in ("info", "updates"):
        (admin / folder).mkdir(parents=True)
    (admin / "status").write_text("")
    (admin / "available").write_text("")
    (root / "var/log").mkdir(parents=True)
    os.symlink("usr/bin", root / "bin")
    force = [] if os.geteuid() == 0 else ["--force-not-root"]
    subprocess.run(["dpkg", "--root=" + str(root), *force, "--force-script-chrootless", "--log=/dev/null",
                    "--install", str(deb(work, "0.3.0"))], check=True, capture_output=True)
    ok(read(root / "usr/share/winlux/version") == "v0.3.0", "système de test installé en 0.3.0")
    stable_inode = (root / "usr/share/winlux/stable").stat().st_ino
    usr_inode = (root / "usr").stat().st_ino
    (root / "usr/lib").mkdir(parents=True, exist_ok=True)
    (root / "usr/lib/fichier-ubuntu").write_text("hors paquet Winlux\n")

    missing = ab(root, "prepare", deb(work, "0.3.1", depends="paquet-ubuntu-absent"), "v0.3.1")
    ok(missing.returncode == 3 and not (root / ".winlux-ab/b").exists(),
       f"dépendance Ubuntu absente : code 3, emplacement B retiré ({missing.returncode})")

    prepared = ab(root, "prepare", deb(work, "0.3.1"), "v0.3.1")
    ok(prepared.returncode == 0, f"mise à jour préparée ({prepared.stderr.strip()[-300:]})")
    slot = root / ".winlux-ab/b"
    ok(read(root / "usr/share/winlux/version") == "v0.3.0" and (root / "usr").stat().st_ino == usr_inode,
       "système actif intact pendant la préparation")
    ok(read(slot / "usr/share/winlux/version") == "v0.3.1" and read(slot / "init").endswith("0.3.1"),
       "emplacement B en 0.3.1 (/usr, /init)")
    ok(read(root / "etc/apt/apt.conf.d/90winlux-desktop") == "// 0.3.0", "/etc actif intact")
    ok(read(slot / "etc-next/apt/apt.conf.d/90winlux-desktop") == "// 0.3.1"
       and read(slot / "etc-current/apt/apt.conf.d/90winlux-desktop") == "// 0.3.0",
       "fichiers /etc du paquet préparés dans les deux sens")
    shared = (slot / "usr/share/winlux/stable").stat().st_ino
    ok(shared != stable_inode, "fichier réécrit par dpkg : nouvel inode, l'ancien reste à l'actif")
    ok((slot / "usr/lib/fichier-ubuntu").stat().st_ino == (root / "usr/lib/fichier-ubuntu").stat().st_ino,
       "fichier hors paquet partagé par lien physique (aucune place prise)")
    status = dict(line.split("=", 1) for line in ab(root, "status").stdout.splitlines())
    ok(status.get("state") == "prepared" and status.get("version") == "v0.3.1"
       and status.get("previous_version") == "v0.3.0", f"état préparé ({status})")
    ok(oct((root / ".winlux-ab").stat().st_mode & 0o777) == "0o700", "dossier A/B réservé à root")

    binary = next((p for p in (project / "target/release/winlux", project / "target/debug/winlux") if p.is_file()), None)
    if binary is None:
        print("saut  bascule : binaire winlux absent (cargo build)")
    else:
        def boot(cmdline_interactive=True):
            return subprocess.run([str(binary), "--ab-boot", str(root)], capture_output=True, text=True).returncode
        ok(boot() == 10, "au démarrage : bascule, /init doit se relancer")
        ok(read(root / "usr/share/winlux/version") == "v0.3.1" and read(root / "init").endswith("0.3.1")
           and read(root / "etc/apt/apt.conf.d/90winlux-desktop") == "// 0.3.1", "système actif en 0.3.1")
        query = subprocess.run(["dpkg-query", "--admindir=" + str(admin), "-W", "-f=${Version}", "winlux-desktop"],
                               capture_output=True, text=True).stdout
        ok(query == "0.3.1", f"base dpkg active en 0.3.1 ({query})")
        ok(read(slot / "usr/share/winlux/version") == "v0.3.0", "0.3.0 gardée en réserve")
        ok(ab(root, "confirm").returncode == 0 and "state=confirmed" in ab(root, "status").stdout, "version validée")
        ok(ab(root, "rollback").returncode == 0, "retour demandé")
        ok(boot() == 10 and read(root / "usr/share/winlux/version") == "v0.3.0"
           and read(root / "etc/apt/apt.conf.d/90winlux-desktop") == "// 0.3.0", "retour sur la réserve 0.3.0")

client = (project / "scripts/winlux-update-apt.py").read_text()
ok("prepared = ab_prepare(package" in client and "if not prepared:\n                        write_fields(TRIAL" in client,
   "client : A/B d'abord, APT sur place en repli, pas de second compteur d'essai")
ok('ab("rollback")' in client and "ab_rolled_back()" in client and 'ab("confirm")' in client,
   "client : retour, échec du bureau et validation passent par winlux-ab")
for name, needle in ((project / "scripts/build-live-iso.sh", "--ab-boot"),
                     (project / "scripts/stage-desktop-components.sh", "winlux-ab")):
    ok(needle in name.read_text(), f"{name.name} appelle {needle}")

if failures:
    raise SystemExit(f"\n{len(failures)} échec(s)")
print("\nMises à jour A/B : tout est bon")
