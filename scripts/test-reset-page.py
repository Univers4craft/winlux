#!/usr/bin/python3
"""Page Paramètres > Récupération (#283) sous Xvfb, avec un winlux-reset factice.

Vérifie que la page affiche les options d'un PC installé, masque celles d'un
Live, parcourt les assistants « Réinitialiser ce PC » et « Créer une clé de
secours » jusqu'à la commande envoyée à winlux-reset, dans les 4 langues.
WINLUX_RESET_CAPTURES=DOSSIER enregistre une capture de chaque écran.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
CAPTURES = os.environ.get("WINLUX_RESET_CAPTURES")


def fake_tool(directory, installed=True, pending=""):
    log = directory / "calls.log"
    tool = directory / "winlux-reset"
    tool.write_text(f"""#!/bin/sh
echo "$*" >>{log}
case "$1" in
  status) printf 'installed={1 if installed else 0}\\nlive={0 if installed else 1}\\npending={pending}\\ngrub_install=1\\nkey_tools=1\\n' ;;
  sources) echo 'media:/run/winlux/live/media|Clé de secours ou support Winlux (v0.2.15)'; echo 'cloud|Télécharger depuis Internet (environ 2 Go)' ;;
  list-keys) echo '/dev/sdb|16008609792|SanDisk Ultra' ;;
  prepare) read -r password; echo "password=$password" >>{log}; echo 'PROGRESS 50 Décompression du système d’origine…'; echo 'DONE Prêt : redémarrez pour terminer la réinitialisation.' ;;
  make-key) echo 'PROGRESS 60 Copie de Winlux sur la clé…'; echo 'DONE Clé de secours prête : vous pouvez la retirer.' ;;
  *) echo 'DONE ok' ;;
esac
""")
    tool.chmod(0o755)
    return tool, log


def load(lang):
    os.environ["WINLUX_LANG"] = lang
    loader = SourceFileLoader(f"reset_page_{lang}", str(HERE / "winlux-reset-page.py"))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def pump(Gtk, seconds=0.4):
    end = time.time() + seconds
    while time.time() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.02)


def capture(Gdk, widget, name):
    if not CAPTURES:
        return
    Path(CAPTURES).mkdir(parents=True, exist_ok=True)
    window = widget.get_window()
    width, height = window.get_width(), window.get_height()
    pixbuf = Gdk.pixbuf_get_from_window(window, 0, 0, width, height)
    if pixbuf:
        pixbuf.savev(str(Path(CAPTURES) / f"{name}.png"), "png", [], [])


def labels(widget):
    from gi.repository import Gtk
    out = []
    if isinstance(widget, (Gtk.Label,)):
        out.append(widget.get_text())
    if isinstance(widget, Gtk.Button) and widget.get_label():
        out.append(widget.get_label())
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children():
            out += labels(child)
    return out


def click(widget, text):
    from gi.repository import Gtk
    if isinstance(widget, Gtk.Button) and (widget.get_label() == text or text in labels(widget)):
        widget.clicked()
        return True
    if isinstance(widget, Gtk.Container):
        return any(click(child, text) for child in widget.get_children())
    return False


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def main():
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        tool, log = fake_tool(directory)
        os.environ["WINLUX_RESET_TOOL"] = str(tool)
        for lang, title, keep, reset in (("fr", "Récupération", "Conserver mes fichiers", "Réinitialiser"),
                                         ("en", "Recovery", "Keep my files", "Reset"),
                                         ("es", "Recuperación", "Conservar mis archivos", "Restablecer"),
                                         ("de", "Wiederherstellung", "Eigene Dateien behalten", "Zurücksetzen")):
            module = load(lang)
            import gi
            gi.require_version("Gtk", "3.0")
            gi.require_version("Gdk", "3.0")
            from gi.repository import Gdk, Gtk
            window = module.StandaloneWindow()
            window.show_all()
            pump(Gtk)
            text = labels(window)
            check(title in text and module._t("Réinitialiser ce PC") in text, f"{lang} : page incomplète {text}")
            check(module._t("Créer une clé de secours") in text and "SanDisk" in (window.panel.keys.get_active_text() or ""), f"{lang} : clé absente")
            check(module._t("Démarrage avancé") in text and module._t("Réparer le démarrage") in text, f"{lang} : options absentes")
            capture(Gdk, window, f"{lang}-1-page")
            # Assistant « Tout supprimer » jusqu'à la commande envoyée.
            wizard = module.ResetWizard(window, lambda: None)
            wizard.show_all()
            pump(Gtk)
            check(keep in labels(wizard), f"{lang} : choix absents")
            capture(Gdk, wizard, f"{lang}-2-choix")
            wizard.picked("wipe")
            wizard.fullname.set_text("Élodie Martin")
            wizard.password.set_text("Secret-123")
            wizard.again.set_text("Secret-123")
            pump(Gtk)
            check(wizard.user.get_text() == "elodie", f"{lang} : nom de compte proposé {wizard.user.get_text()}")
            capture(Gdk, wizard, f"{lang}-3-compte")
            wizard.account_next()
            pump(Gtk)
            capture(Gdk, wizard, f"{lang}-4-source")
            wizard.source_next()
            pump(Gtk)
            check(not wizard.go.get_sensitive(), f"{lang} : « Tout supprimer » sans confirmation")
            wizard.understood.set_active(True)
            check(wizard.go.get_sensitive(), f"{lang} : confirmation sans effet")
            capture(Gdk, wizard, f"{lang}-5-resume")
            check(click(wizard.footer, reset), f"{lang} : bouton {reset} absent")
            for _ in range(50):
                pump(Gtk, 0.1)
                if not wizard.running:
                    break
            capture(Gdk, wizard, f"{lang}-6-pret")
            calls = log.read_text()
            check("prepare --mode wipe --source media:/run/winlux/live/media --user elodie --fullname Élodie Martin" in calls,
                  f"{lang} : commande {calls}")
            check("password=Secret-123" in calls, f"{lang} : mot de passe non transmis")
            check(module._t("Redémarrer maintenant") in labels(wizard), f"{lang} : pas de redémarrage proposé")
            wizard.destroy()
            # Clé de secours.
            key = module.KeyWizard(window, "/dev/sdb", "SanDisk Ultra (16,0 Go) — /dev/sdb", lambda: None)
            key.show_all()
            pump(Gtk)
            capture(Gdk, key, f"{lang}-7-cle")
            key.start()
            for _ in range(50):
                pump(Gtk, 0.1)
                if not key.running:
                    break
            check("make-key --device /dev/sdb --source media:/run/winlux/live/media" in log.read_text(), f"{lang} : clé")
            capture(Gdk, key, f"{lang}-8-cle-prete")
            key.destroy()
            window.destroy()
            log.write_text("")
        # Live : ni réinitialisation ni démarrage avancé, mais la clé de secours.
        tool, log = fake_tool(directory, installed=False)
        module = load("fr")
        from gi.repository import Gtk
        window = module.StandaloneWindow()
        window.show_all()
        pump(Gtk)
        text = labels(window)
        check("Réinitialiser le PC" not in text and "Démarrage avancé" not in text, "Live : réinitialisation proposée")
        check("Créer une clé de secours" in text, "Live : clé de secours absente")
        window.destroy()
        # Réinitialisation prête : redémarrer ou annuler.
        tool, log = fake_tool(directory, pending="keep")
        module = load("fr")
        window = module.StandaloneWindow()
        window.show_all()
        pump(Gtk)
        check("Annuler la réinitialisation" in labels(window), "réinitialisation prête non signalée")
        capture(Gdk, window, "fr-9-prete")
        window.destroy()
    print("Paramètres > Récupération : OK (4 langues)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
