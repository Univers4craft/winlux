#!/usr/bin/python3
"""Paramètres > Comptes > Options de connexion (#659), dans les 4 langues.

Vrai relais winlux-root sur une racine factice, lecteur d'empreintes simulé :
la carte « Code PIN » apparaît, la carte empreinte seulement avec un lecteur,
et rien du tout sans relais. Une capture par langue est écrite si
WINLUX_SIGNIN_SHOTS=DOSSIER. À lancer avec xvfb-run.
"""
import json
import os
from pathlib import Path
import runpy
import socket
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-signin-page."))
(work / "etc/winlux").mkdir(parents=True)
(work / "run/winlux").mkdir(parents=True)
fake = work / "lecteur"
fake.mkdir()
root_lib = runpy.run_path(str(HERE / "winlux-root.py"), run_name="signin_page_root")
crypt = root_lib["_crypt"]
(work / "etc/shadow").write_text(f"alice:{crypt('motdepasse', '$6$sel$')}:19000::::::\n")
(work / "etc/winlux/users.conf").write_text("alice:root:shadow\n")
sock = work / "root.sock"
os.environ["WINLUX_ROOT_SOCKET"] = str(sock)
os.environ["WINLUX_RUN_DIR"] = str(work / "run/winlux")
env = {**os.environ, "WINLUX_ROOT_PREFIX": str(work), "WINLUX_ROOT_SIGNIN": str(HERE / "winlux-signin.py"),
       "WINLUX_FPRINT_FAKE": str(fake), "WINLUX_ROOT_TEST_PEER": "1000:alice"}
server = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-root.py"), "serve"], env=env)
for _ in range(100):
    if sock.exists():
        break
    time.sleep(0.05)
failures = []


def check(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


def ask(payload):
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(sock))
        client.sendall((json.dumps({"op": "signin", **payload}) + "\n").encode())
        for line in client.makefile("r", encoding="utf-8"):
            answer = json.loads(line)
            if "ok" in answer:
                return answer
    return {}


shots = os.environ.get("WINLUX_SIGNIN_SHOTS")
EXPECTED = {"fr": ("Options de connexion", "Reconnaissance d’empreinte digitale", "Modifier le code PIN"),
            "en": ("Sign-in options", "Fingerprint recognition", "Change PIN"),
            "es": ("Opciones de inicio de sesión", "Reconocimiento de huella digital", "Cambiar el PIN"),
            "de": ("Anmeldeoptionen", "Fingerabdruckerkennung", "PIN ändern")}
try:
    check(ask({"action": "pin-set", "user": "alice", "password": "motdepasse", "pin": "2580"}).get("ok"), "code PIN créé")
    (fake / "alice.fingers").write_text("right-index-finger\n")
    for lang, (section, finger, change) in EXPECTED.items():
        os.environ["WINLUX_LANG"] = lang
        settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name=f"signin_page_{lang}")
        Gtk = settings["Gtk"]

        def walk(widget):
            yield widget
            if isinstance(widget, Gtk.Container):
                for child in widget.get_children():
                    yield from walk(child)

        def texts(window):
            widgets = list(walk(window.body))
            return ([w.get_text() for w in widgets if isinstance(w, Gtk.Label)],
                    [w.get_label() for w in widgets if isinstance(w, Gtk.Button) and w.get_label()])

        window = settings["Settings"]()
        window.set_default_size(1100, 760)
        window.show_all()
        for child in window.body.get_children():
            child.destroy()
        window.sections_signin("alice")
        window.body.show_all()
        labels, buttons = texts(window)
        check(section in labels and finger in labels and change in buttons,
              f"{lang} : options de connexion, code PIN et empreinte traduits")
        if shots:
            for _ in range(60):
                while Gtk.events_pending():
                    Gtk.main_iteration_do(False)
                time.sleep(0.01)
            Path(shots).mkdir(parents=True, exist_ok=True)
            subprocess.run(["import", "-window", "root", str(Path(shots) / f"options-connexion-{lang}.png")], check=False)
        if lang == "fr":
            def pump(seconds, until=None):
                end = time.monotonic() + seconds
                while time.monotonic() < end and not (until and until()):
                    while Gtk.events_pending():
                        Gtk.main_iteration_do(False)
                    time.sleep(0.01)

            # Boîte « Configurer un code PIN » : une erreur ne la fait pas grandir (#1501).
            dialog, fields, error = window.signin_dialog(
                "Configurer un code PIN", [("password", "Mot de passe actuel", False), ("pin", "Nouveau code PIN", True)],
                "De 4 à 32 chiffres. Évitez les chiffres répétés ou qui se suivent (1111, 1234).")
            pump(0.5)
            before = dialog.get_allocated_height(), dialog.get_allocated_width()
            for message in ("Le mot de passe est incorrect.",
                            "Le code PIN est bloqué après trop d’essais. Connectez-vous avec votre mot de passe."):
                error.set_text(message)
                pump(0.5)
                after = dialog.get_allocated_height(), dialog.get_allocated_width()
                check(after == before, f"message d’erreur : la boîte garde sa taille {before} → {after} (#1501)")
            dialog.destroy()
            # Sans lecteur : seule la carte du code PIN, puis lecteur branché
            # page ouverte : la carte arrive seule (#1500).
            page_globals = window.watch_signin_reader.__func__.__globals__
            window.page = page_globals["PAGE_ACCOUNTS"]
            window.SIGNIN_WATCH_MS = 200
            if getattr(window, "_signin_watch", None):
                settings["GLib"].source_remove(window._signin_watch)
            window._signin_watch = None
            refreshed = []
            window.extra_refresh = lambda: refreshed.append(True)
            (fake / "alice.fingers").unlink()
            fake.rmdir()
            for child in window.body.get_children():
                child.destroy()
            window.sections_signin("alice")
            labels, buttons = texts(window)
            check("Code PIN" in labels and finger not in labels, "sans lecteur : carte empreinte masquée")
            pump(1.0)
            check(not refreshed, "sans changement de lecteur : page non reconstruite")
            fake.mkdir()
            (fake / "alice.fingers").write_text("right-index-finger\n")
            pump(5.0, lambda: refreshed)
            check(bool(refreshed), "lecteur branché page ouverte : page reconstruite avec la carte empreinte (#1500)")
            window.page = None
            pump(0.5)
            check(window._signin_watch is None, "page quittée : la surveillance du lecteur s’arrête")
            # Sans relais : rien.
            os.environ["WINLUX_ROOT_SOCKET"] = str(work / "absent.sock")
            for child in window.body.get_children():
                child.destroy()
            window.sections_signin("alice")
            labels, _ = texts(window)
            check(section not in labels, "sans relais root : section masquée")
            os.environ["WINLUX_ROOT_SOCKET"] = str(sock)
        window.destroy()
finally:
    server.kill()

if failures:
    sys.exit(1)
print("OK : Paramètres > Comptes > Options de connexion")
