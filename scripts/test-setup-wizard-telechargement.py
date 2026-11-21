#!/usr/bin/python3
"""Assistant d'installation, installeur web (#2750) : l'accord de téléchargement.

Sous Xvfb, deux parties :
1. La vraie question GTK : elle nomme le paquet, la version, l'éditeur, le
   fichier, la source et l'empreinte SHA-256 ; « Annuler » a le focus, le
   défaut, la couleur d'accent et son cadre de focus dès l'ouverture ;
   « Annuler » rend un refus. Dialog.run est remplacé par une sonde.
2. L'assistant entier, devant un faux runtime qui rend 4 (accord requis) sans
   `--allow-download` : refus → 1602 et une seule installation tentée ;
   accord → relance avec `--allow-download` et réussite.
"""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk

HERE = Path(__file__).resolve().parent
WIZARD = HERE / "winlux-setup-wizard.py"
os.environ["WINLUX_LANG"] = "fr"
spec = importlib.util.spec_from_file_location("winlux_setup_wizard", WIZARD)
wizard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wizard)

SHA = "3cd861b5af3f85bd28666d4a9017d03de237c08ad0103ee9d92b2cd921f8c867"
DOWNLOAD = ("download.name=Paint.NET\ndownload.version=5.1.12\ndownload.publisher=dotPDN LLC\n"
            "download.source=github.com/paintdotnet/release\n"
            "download.file=paint.net.5.1.12.install.x64.zip\ndownload.size_mb=81\n"
            f"download.sha256={SHA}\n")
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


# 1. La question elle-même.
fields = wizard.download_fields("setup : bla\n" + DOWNLOAD)
check(fields is not None, "lignes download.* non lues")
check(wizard.download_fields("download.name=X\n") is None, "question posée avec des champs manquants")
check(wizard.with_download(["w", "--setup", "/a.exe", "--desktop"])
      == ["w", "--setup", "--allow-download", "/a.exe", "--desktop"], "relance sans --allow-download")
seen = {}


def probe(dialog):
    dialog.show_all()
    while Gtk.events_pending():
        Gtk.main_iteration()
    cancel = dialog.get_widget_for_response(Gtk.ResponseType.CANCEL)
    ok = dialog.get_widget_for_response(Gtk.ResponseType.OK)
    seen.update(
        text=dialog.get_property("secondary-text"), cancel=cancel.get_label(), ok=ok.get_label(),
        focus=dialog.get_focus() is cancel, focus_visible=dialog.get_focus_visible(),
        default=dialog.get_default_widget() is cancel,
        cancel_accent=cancel.get_style_context().has_class("suggested-action"),
        ok_accent=ok.get_style_context().has_class("suggested-action"),
        ok_focus=ok.has_focus(), ok_default=ok.has_default(),
    )
    return Gtk.ResponseType.CANCEL


real_run = Gtk.Dialog.run
Gtk.Dialog.run = probe
if fields:
    check(wizard.confirm_download(None, fields) is False, "« Annuler » n'a pas rendu un refus")
Gtk.Dialog.run = real_run
if seen:
    for part in ("Paint.NET 5.1.12", "dotPDN LLC", "github.com/paintdotnet/release",
                 "paint.net.5.1.12.install.x64.zip", "81 Mo", SHA):
        check(part in seen["text"], f"la question ne nomme pas « {part} »")
    check(seen["cancel"] == "Annuler" and seen["ok"] == "Télécharger et installer",
          f"boutons {seen['cancel']} / {seen['ok']}")
    check(seen["focus"], "« Annuler » n'a pas le focus à l'ouverture")
    check(seen["focus_visible"], "le cadre de focus n'est pas visible à l'ouverture")
    check(seen["default"], "« Annuler » n'est pas le bouton par défaut")
    check(seen["cancel_accent"] and not seen["ok_accent"], "la couleur d'accent n'est pas sur « Annuler »")
    check(not seen["ok_focus"] and not seen["ok_default"], "« Télécharger et installer » a le focus ou le défaut")
else:
    check(False, "la question ne s'est pas ouverte")

# 2. L'assistant entier, faux runtime.
with tempfile.TemporaryDirectory() as work:
    work = Path(work)
    calls = work / "appels.txt"
    fake = work / "winlux"
    fake.write_text(f"""#!/bin/sh
case "$1 $2" in
    "--setup --info") printf '7z\\tpaint.net\\t5.112.9563.32325\\tdotPDN LLC\\t\\n'; exit 0 ;;
    "--setup --license") exit 1 ;;
esac
echo "$*" >>"{calls}"
case " $* " in
    *" --allow-download "*) echo "setup : Paint.NET est installé."; exit 0 ;;
esac
printf '%s' '{DOWNLOAD}'
echo "setup : cet installeur télécharge Paint.NET 5.1.12 ; après votre accord." >&2
exit 4
""")
    fake.chmod(0o755)
    exe = work / "paint.net.5.1.12.install.anycpu.web.exe"
    exe.write_bytes(b"MZ")
    home = work / "home"
    home.mkdir()

    def run(answer):
        calls.unlink(missing_ok=True)
        env = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home), WINLUX_LANG="fr",
                   WINLUX_SETUP_WIZARD_AUTO="1", WINLUX_SETUP_WIZARD_AUTO_TELECHARGEMENT=answer,
                   WINLUX_BIN=str(fake), NO_AT_BRIDGE="1")
        res = subprocess.run([sys.executable, str(WIZARD), str(exe)], env=env, capture_output=True,
                             text=True, timeout=120)
        lines = calls.read_text().splitlines() if calls.exists() else []
        return res, lines

    res, lines = run("refuse")
    check(res.returncode == wizard.CANCELLED & 0xFF or res.returncode == wizard.CANCELLED,
          f"refus : code {res.returncode} au lieu de 1602")
    check(len(lines) == 1 and "--allow-download" not in lines[0], f"refus : appels {lines}")
    check("code 4" not in res.stdout, "refus : « code 4 » affiché")
    res, lines = run("accepte")
    check(res.returncode == 0, f"accord : code {res.returncode}\n{res.stdout}{res.stderr}")
    check(len(lines) == 2 and lines[1].startswith("--setup --allow-download "), f"accord : appels {lines}")
    check("code 4" not in res.stdout, "accord : « code 4 » affiché")

if failures:
    sys.exit(1)
print("Assistant : l'installeur web demande l'accord de téléchargement, Annuler par défaut (#2750)")
