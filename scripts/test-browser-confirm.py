#!/usr/bin/python3
"""Paramètres > Navigateurs : éditeur, version et source avant l'installation (#923).

1. winlux-browser-fetch --info lit seulement l'index d'un miroir local et
   rend version, adresse exacte, empreinte et taille, sans télécharger le
   paquet.
2. Les domaines montrés par Paramètres sont ceux des dépôts téléchargés
   (SOURCES de winlux-browser-fetch).
3. Le texte de confirmation dans les quatre langues, avec et sans réseau.
4. Vraie page 11 (Xvfb) : « Installer Mozilla Firefox » ouvre la fenêtre de
   confirmation au lieu d'installer ; Annuler ne lance rien, Installer lance
   winlux-control browser-install-firefox.
"""
import ast
import hashlib
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
from urllib.parse import urlparse

HERE = Path(__file__).resolve().parent
FETCH = HERE / "prepare-browser-packages.py"
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


tmp = Path(tempfile.mkdtemp(prefix="winlux-browser-confirm-"))
mirror = tmp / "mirror"
index = mirror / "dists/mozilla/main/binary-amd64/Packages"
index.parent.mkdir(parents=True)
digest = hashlib.sha256(b"firefox").hexdigest()
index.write_text("Package: firefox\nVersion: 150.0~build1\nArchitecture: amd64\n"
                 f"Filename: pool/mozilla/firefox_150.0_amd64.deb\nSize: 84000000\nSHA256: {digest}\n\n")

# ── 1. --info : index seul ──
env = {**os.environ, "WINLUX_BROWSER_MIRROR": mirror.as_uri() + "/"}
result = subprocess.run([sys.executable, str(FETCH), "--info", "firefox"], capture_output=True, text=True, env=env, cwd=tmp)
info = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
check(result.returncode == 0, f"--info a échoué : {result.stderr}")
check(info.get("version") == "150.0~build1" and info.get("sha256") == digest and info.get("size") == "84000000"
      and info.get("source", "").endswith("/pool/mozilla/firefox_150.0_amd64.deb"), f"--info : {info}")
check(not any(tmp.rglob("*.deb")), "--info a téléchargé un paquet")

# ── 2. Domaines identiques à ceux des téléchargements ──
tree = ast.parse(FETCH.read_text())
sources = next(ast.literal_eval(node.value) for node in tree.body
               if isinstance(node, ast.Assign) and getattr(node.targets[0], "id", "") == "SOURCES")
settings_source = (HERE / "winlux-settings.py").read_text()
hosts = ast.literal_eval(settings_source.split("BROWSER_HOSTS = ", 1)[1].split("\n", 1)[0])
check(hosts == {name: urlparse(base).hostname for name, (_, base, _) in sources.items()},
      f"BROWSER_HOSTS {hosts} différent des dépôts téléchargés")

# ── 3. Texte dans les quatre langues (sous-processus : _t lit WINLUX_LANG au chargement) ──
probe = """
import runpy, sys, json
s = runpy.run_path(sys.argv[1], run_name="browser_confirm_probe")
online = s["browser_confirmation"]("firefox", json.loads(sys.argv[2]))
offline = s["browser_confirmation"]("chrome", {})
print(json.dumps([online, offline]))
"""
import json
expected = {
    "fr": ("Installer Mozilla Firefox ?", "Éditeur : Mozilla Corporation", "Version : 150.0~build1", "Source : https://packages.mozilla.org", "Taille : 84 Mo", "Version : connue au téléchargement"),
    "en": ("Install Mozilla Firefox?", "Publisher: Mozilla Corporation", "Version: 150.0~build1", "Source: https://packages.mozilla.org", "Size: 84 MB", "Version: known when downloading"),
    "es": ("¿Instalar Mozilla Firefox?", "Editor: Mozilla Corporation", "Versión: 150.0~build1", "Origen: https://packages.mozilla.org", "Tamaño: 84 MB", "Versión: se conocerá al descargar"),
    "de": ("Mozilla Firefox installieren?", "Herausgeber: Mozilla Corporation", "Version: 150.0~build1", "Quelle: https://packages.mozilla.org", "Größe: 84 MB", "Version: wird beim Herunterladen bekannt"),
}
for lang, (title, publisher, version, source, size, unknown) in expected.items():
    out = subprocess.run([sys.executable, "-c", probe, str(HERE / "winlux-settings.py"), json.dumps(info)],
                         capture_output=True, text=True, env={**os.environ, "WINLUX_LANG": lang})
    try:
        (online_title, online_text), (offline_title, offline_text) = json.loads(out.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        check(False, f"{lang} : texte illisible ({out.stderr[-400:]})")
        continue
    check(online_title == title, f"{lang} : titre {online_title!r}")
    for part in (publisher, version, source, size, digest[:16]):
        check(part in online_text, f"{lang} : « {part} » absent de {online_text!r}")
    check(unknown in offline_text and "https://dl.google.com" in offline_text and "Google LLC" in offline_text,
          f"{lang} : sans réseau {offline_text!r}")

# ── 4. Vraie page 11 ──
fake_fetch = tmp / "fetch"
fake_fetch.write_text(f"#!/bin/sh\nexec {sys.executable} {FETCH} \"$@\"\n")
fake_fetch.chmod(0o755)
os.environ["WINLUX_BROWSER_FETCH"] = str(fake_fetch)
os.environ["WINLUX_BROWSER_MIRROR"] = mirror.as_uri() + "/"
os.environ["WINLUX_LANG"] = "fr"
settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="browser_confirm_test")
Gtk, GLib = settings["Gtk"], settings["GLib"]
window = settings["Settings"]()
launched = []
real_popen = settings["subprocess"].Popen


class FakeProcess:
    returncode = 0
    def poll(self): return 0


def fake_popen(command, *args, **kwargs):
    if command[:1] == ["winlux-control"] and command[1:2] and command[1].startswith("browser-"):
        launched.append(command[1:])
        return FakeProcess()
    return real_popen(command, *args, **kwargs)


settings["subprocess"].Popen = fake_popen


def pump(seconds):
    loop = GLib.MainLoop()
    GLib.timeout_add(int(seconds * 1000), loop.quit)
    loop.run()


def wait_dialog_filled(dialog, seconds=15):
    for _ in range(int(seconds * 10)):
        if "Éditeur" in (dialog.get_property("secondary-text") or ""): return True
        pump(0.1)
    return False


window.open_page(11)
pump(0.3)
window.execute("browser-install-firefox")
dialog = getattr(window, "browser_dialog", None)
check(dialog is not None and not launched, "« Installer » doit d'abord demander confirmation")
if dialog is not None:
    install = dialog.get_widget_for_response(Gtk.ResponseType.OK)
    check(not install.get_sensitive(), "« Installer » actif avant de connaître la version")
    check(wait_dialog_filled(dialog), f"détails jamais affichés : {dialog.get_property('secondary-text')!r}")
    text = dialog.get_property("secondary-text") or ""
    check("Version : 150.0~build1" in text and "packages.mozilla.org" in text, f"détails : {text!r}")
    check(install.get_sensitive(), "« Installer » reste grisé une fois la version connue")
    dialog.response(Gtk.ResponseType.CANCEL)
    pump(0.2)
    check(not launched, f"Annuler a lancé l'installation : {launched}")
window.execute("browser-install-firefox")
dialog = getattr(window, "browser_dialog", None)
if dialog is not None and wait_dialog_filled(dialog):
    dialog.response(Gtk.ResponseType.OK)
    pump(0.3)
check(launched == [["browser-install-firefox"]], f"Installer n'a pas lancé winlux-control browser-install-firefox : {launched}")

if failures:
    sys.exit(1)
print("OK: Navigateurs, éditeur, version, source et taille montrés avant l'installation, 4 langues, confirmer ou refuser (#923)")
