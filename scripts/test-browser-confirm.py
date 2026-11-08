#!/usr/bin/python3
"""Paramètres > Navigateurs : éditeur, version et source avant l'installation (#923).

1. Le texte de confirmation (browser_download_text) dans les quatre langues :
   éditeur, version (lue dans le cache, sinon « dernière version stable »),
   dépôt officiel et vérification SHA-256.
2. Vraie page 11 (Xvfb) : le bouton « Installer Mozilla Firefox » ouvre la
   fenêtre de confirmation au lieu d'installer, Annuler est le bouton par
   défaut et ne lance rien ; « Télécharger et installer » lance
   winlux-control browser-install-firefox.
Les sources affichées = dépôts du téléchargeur : test-settings-redesign.py.
"""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
SETTINGS = HERE / "winlux-settings.py"
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


root = Path(tempfile.mkdtemp(prefix="winlux-browser-confirm-"))
(root / "var/cache/winlux/browsers").mkdir(parents=True)
(root / "var/cache/winlux/browsers/firefox.version").write_text("150.0.1\n")

# ── 1. Quatre langues (sous-processus : _t lit WINLUX_LANG au chargement) ──
probe = """
import runpy, sys, json
s = runpy.run_path(sys.argv[1], run_name="browser_confirm_probe")
t = s["_t"]
print(json.dumps([t("Installer {name} ?").format(name="Mozilla Firefox"), t("Télécharger et installer"), t("Annuler"),
                  s["browser_download_text"]("firefox", sys.argv[2]), s["browser_download_text"]("chrome", sys.argv[2])]))
"""
expected = {
    "fr": ("Installer Mozilla Firefox ?", "Télécharger et installer", "Annuler", "Éditeur : Mozilla", "Version : 150.0.1",
           "Source : https://packages.mozilla.org/apt/", "dernière version stable"),
    "en": ("Install Mozilla Firefox?", "Download and install", "Cancel", "Publisher: Mozilla", "Version: 150.0.1",
           "Source: https://packages.mozilla.org/apt/", "latest stable version"),
    "es": ("¿Instalar Mozilla Firefox?", "Descargar e instalar", "Cancelar", "Editor: Mozilla", "Versión: 150.0.1",
           "Origen: https://packages.mozilla.org/apt/", "última versión estable"),
    "de": ("Mozilla Firefox installieren?", "Herunterladen und installieren", "Abbrechen", "Herausgeber: Mozilla",
           "Version: 150.0.1", "Quelle: https://packages.mozilla.org/apt/", "neueste stabile Version"),
}
for lang, (title, install, cancel, publisher, version, source, unknown) in expected.items():
    out = subprocess.run([sys.executable, "-c", probe, str(SETTINGS), str(root)],
                         capture_output=True, text=True, env={**os.environ, "WINLUX_LANG": lang})
    try:
        got_title, got_install, got_cancel, firefox, chrome = json.loads(out.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        check(False, f"{lang} : texte illisible ({out.stderr[-400:]})")
        continue
    check((got_title, got_install, got_cancel) == (title, install, cancel), f"{lang} : boutons {got_title!r} {got_install!r} {got_cancel!r}")
    for part in (publisher, version, source, "SHA-256"):
        check(part in firefox, f"{lang} : « {part} » absent de {firefox!r}")
    check(unknown in chrome and "https://dl.google.com/" in chrome, f"{lang} : version inconnue {chrome!r}")

# ── 2. Vraie page 11 ──
os.environ["WINLUX_LANG"] = "fr"
settings = runpy.run_path(str(SETTINGS), run_name="browser_confirm_test")
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


def confirmation():
    return next((w for w in Gtk.Window.list_toplevels()
                 if isinstance(w, Gtk.MessageDialog) and w.get_visible()), None)


window.open_page(11)
pump(0.3)
button = getattr(window, "browser_buttons", {}).get("browser-install-firefox")
check(button is not None, "page 11 sans bouton « Installer Mozilla Firefox »")
if button is not None:
    button.clicked()
    pump(0.3)
    dialog = confirmation()
    check(dialog is not None and not launched, f"« Installer » doit d'abord demander confirmation (lancé : {launched})")
    if dialog is not None:
        text = dialog.get_property("secondary-text") or ""
        check("Éditeur : Mozilla" in text and "packages.mozilla.org" in text and "SHA-256" in text, f"détails : {text!r}")
        cancel = dialog.get_widget_for_response(Gtk.ResponseType.CANCEL)
        check(cancel is not None and cancel.has_default(), "Annuler n'est pas le bouton par défaut")
        dialog.response(Gtk.ResponseType.CANCEL)
        pump(0.2)
        check(not launched, f"Annuler a lancé l'installation : {launched}")
    button.clicked()
    pump(0.3)
    dialog = confirmation()
    if dialog is not None:
        dialog.response(Gtk.ResponseType.OK)
        pump(0.3)
    check(launched == [["browser-install-firefox"]], f"confirmer n'a pas lancé winlux-control browser-install-firefox : {launched}")

if failures:
    sys.exit(1)
print("OK: Navigateurs, éditeur, version et source montrés avant l'installation, 4 langues, Annuler par défaut (#923)")
