#!/usr/bin/env python3
"""Installateur : pilotes propres à CET ordinateur dès l'installation.

Hors ligne, l'installation reste complète avec ce que contient l'image ; en
ligne, `winlux-drivers auto` tourne dans la cible (chroot) pendant
l'installation, avec le vrai matériel du live, et le récapitulatif final
montre ce qui a été ajouté. Ce qui échoue est refait au premier démarrage
connecté (auto.done absent).

Le test vérifie, sans GTK :
- l'ordre : après la configuration du premier démarrage, avant le démontage ;
- hors ligne : rien n'est lancé, récapitulatif « au premier démarrage » ;
- en ligne : chroot avec /run/winlux/hw et resolv.conf, puis resolv.conf et
  policy-rc.d de la cible remis comme avant ;
- les paquets ajoutés regroupés en lignes lisibles, traduites en 6 langues ;
- le premier démarrage relance toujours `winlux-drivers auto` sans auto.done.
"""
import ast
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import types

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/winlux-installer.py").read_text(encoding="utf-8")
tree = ast.parse(source)
installer_class = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "InstallerApp")
methods = {node.name: node for node in installer_class.body if isinstance(node, ast.FunctionDef)}
functions = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.Assign))
             and (getattr(node, "name", "") in ("installed_packages", "hardware_categories")
                  or any(getattr(t, "id", "") == "HARDWARE_CATEGORIES" for t in getattr(node, "targets", [])))]
assert len(functions) == 3, "aides de l'installateur introuvables"

# ── Ordre dans run_installation ─────────────────────────────────────────
run = ast.get_source_segment(source, methods["run_installation"])
assert run.index("self.configure_first_boot(target)") < run.index("self.install_hardware_support(target)") \
    < run.index("self.finalize_target()")
print("ordre : premier démarrage configuré → pilotes de cet ordinateur → sync et démontage")

calls = []


def fake_run(command, **kwargs):
    calls.append(command)
    if command[:2] == ["chroot", fake_run.root] and command[2:3] == ["dpkg-query"]:
        packages = fake_run.after if fake_run.done else fake_run.before
        return types.SimpleNamespace(returncode=0, stdout="".join(p + "\n" for p in packages), stderr="")
    if command[:3] == ["chroot", fake_run.root, "/usr/bin/winlux-hardware-check"]:
        assert command[3:] == ["--items"], command
        return types.SimpleNamespace(returncode=0, stdout="microcode\nrgb\n", stderr="")
    if command[:4] == ["chroot", fake_run.root, "/usr/bin/winlux-drivers", "install"]:
        fake_run.after.append({"rgb": "openrgb", "microcode": "intel-microcode"}[command[4]])
        return types.SimpleNamespace(returncode=0, stdout="", stderr="")
    if command[:3] == ["chroot", fake_run.root, "/usr/bin/winlux-drivers"]:
        # Le chroot voit l'inventaire du live et le DNS du live.
        assert (pathlib.Path(fake_run.root) / "etc/resolv.conf").read_text() == open("/etc/resolv.conf").read()
        assert os.access(pathlib.Path(fake_run.root) / "usr/sbin/policy-rc.d", os.X_OK)
        fake_run.done = True
        return types.SimpleNamespace(returncode=fake_run.code, stdout="Pilote NVIDIA 580 installé.\n", stderr="")
    return types.SimpleNamespace(returncode=0, stdout="", stderr="")


namespace = {"os": os, "re": re, "sys": sys, "time": __import__("time"), "shutil": __import__("shutil"),
             "contextlib": __import__("contextlib"),
             "subprocess": types.SimpleNamespace(run=fake_run, SubprocessError=subprocess.SubprocessError),
             "tr": lambda key, **_: f"<{key}>"}
exec(compile(ast.Module(body=functions, type_ignores=[]), "installer", "exec"), namespace)
body = [methods[name] for name in ("target_chroot", "online", "install_hardware_support", "hardware_lines")]
klass = ast.ClassDef(name="Fake", bases=[], keywords=[], body=body, decorator_list=[], type_params=[]) \
    if sys.version_info >= (3, 12) else ast.ClassDef(name="Fake", bases=[], keywords=[], body=body, decorator_list=[])
exec(compile(ast.fix_missing_locations(ast.Module(body=[klass], type_ignores=[])), "installer", "exec"), namespace)
Fake = namespace["Fake"]
Fake.set_progress = lambda self, *_: None

assert namespace["hardware_categories"]({"nvidia-driver-580", "nvidia-utils-580", "linux-modules-extra-7.0.0-12-generic",
                                         "intel-microcode", "intel-media-va-driver", "fprintd", "libfoo"}) == \
    ["hw_nvidia", "hw_modules", "hw_microcode", "hw_vaapi", "hw_fingerprint"]
print("paquets ajoutés regroupés : NVIDIA, pilotes du noyau, microcode, VA-API, empreintes")

with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp)
    (root / "usr/bin").mkdir(parents=True)
    (root / "usr/bin/winlux-drivers").write_text("#!/bin/sh\n")
    (root / "etc").mkdir()
    (root / "etc/resolv.conf").write_text("nameserver 127.0.0.53\n")
    fake_run.root = str(root)
    fake_run.before = ["bash", "dpkg"]
    fake_run.after = ["bash", "dpkg", "nvidia-driver-580", "intel-microcode"]
    fake_run.done = False
    fake_run.code = 0

    app = Fake()
    app.online = lambda: False
    report = app.install_hardware_support(str(root))
    assert report["state"] == "offline" and not calls, calls
    assert app.hardware_lines() == ["<hw_offline>"]
    print("hors ligne : rien lancé, « ajouté au premier démarrage connecté »")

    app.online = lambda: True
    have_resolv = os.path.isfile("/etc/resolv.conf")
    if not have_resolv:
        print("(pas de /etc/resolv.conf sur cet hôte : vérification DNS sautée)")
        fake_run.root = "/nonexistent"
    report = app.install_hardware_support(str(root))
    if have_resolv:
        assert report["state"] == "done", report
        assert report["added"] == ["hw_nvidia", "hw_microcode"], report
        assert app.hardware_lines() == ["<hw_nvidia>", "<hw_microcode>"]
        binds = [c[-1] for c in calls if c[:2] == ["mount", "--bind"]]
        if os.path.isdir("/run/winlux/hw"):
            assert f"{root}/run/winlux/hw" in binds, binds
        assert {f"{root}/proc", f"{root}/sys", f"{root}/dev"} <= set(binds), binds
        assert (root / "etc/resolv.conf").read_text() == "nameserver 127.0.0.53\n"
        assert not (root / "usr/sbin/policy-rc.d").exists()
        assert (root / "var/log/winlux-drivers.log").is_file()
        print("en ligne : winlux-drivers auto dans la cible, DNS et policy-rc.d remis, journal gardé")

        fake_run.done = False
        fake_run.code = 1
        fake_run.after = ["bash", "dpkg"]
        report = app.install_hardware_support(str(root))
        assert report["state"] == "partial" and app.hardware_lines() == ["<hw_partial>"], report
        fake_run.code = 0
        fake_run.done = False
        report = app.install_hardware_support(str(root))
        assert app.hardware_lines() == ["<hw_none>"], report
        print("échec partiel : « le reste au premier démarrage » ; rien à ajouter : dit clairement")

        # Inventaire du fil matériel : éléments en plus (RGB du MSI GE73).
        (root / "usr/bin/winlux-hardware-check").write_text("#!/bin/sh\n")
        fake_run.done = False
        fake_run.after = ["bash", "dpkg"]
        calls.clear()
        report = app.install_hardware_support(str(root))
        assert ["chroot", str(root), "/usr/bin/winlux-drivers", "install", "rgb"] in calls, calls
        assert report["added"] == ["hw_microcode", "hw_rgb"], report
        print("winlux-hardware-check --items : éléments en plus installés (RGB, microcode)")

# ── Traductions et premier démarrage ───────────────────────────────────
keys = ["progress_drivers", "hw_show", "hw_none", "hw_offline", "hw_partial", "hw_nvidia", "hw_modules",
        "hw_microcode", "hw_vaapi", "hw_fwupd", "hw_fingerprint", "hw_rgb", "hw_printer"]
for key in keys:
    count = len(re.findall(rf'^\s+"{key}":', source, re.M))
    assert count == 6, f"{key} : {count} langue(s) sur 6"
print("textes en 6 langues")
build = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
assert "/var/lib/winlux/drivers/auto.done" in build and "winlux-drivers auto" in build
drivers = (ROOT / "scripts/winlux-drivers.py").read_text(encoding="utf-8")
auto = drivers[drivers.index("def auto():"):drivers.index("def boot():")]
assert "if not failed:" in auto and "auto.done" in auto
print("premier démarrage connecté : winlux-drivers auto refait tant que tout n'est pas installé")
