#!/usr/bin/python3
"""Relais root du bureau (winlux-root) : politique, préférences, élévation.

Le serveur tourne sur une racine factice (WINLUX_ROOT_PREFIX) avec un appelant
simulé ; winlux-control est remplacé par un script qui note ses arguments.
"""
import os
import runpy
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-root.py"
lib = runpy.run_path(str(TOOL), run_name="winlux_root_lib")


def check(condition, message):
    if not condition:
        print("ERREUR :", message, file=sys.stderr)
        sys.exit(1)


# ── Politique ───────────────────────────────────────────────────────────────
control = lib["control_level"]
check(control(["theme-dark"]) == "user", "préférence relayée sans confirmation")
check(control(["share-add", "/home/a/Documents"]) == "admin", "partage = administrateur")
check(control(["update-apply"]) == "user", "mise à jour = utilisateur (comme Windows Update)")
check(control(["restore-delete", "1"]) == "admin", "point de restauration = administrateur")
check(control(["volume-up"]) is None, "le son reste dans la session")
check(control(["keyboard-fr;reboot"]) is None, "action composée refusée")
check(control(["tz-Europe/Paris\nx"]) is None, "retour à la ligne refusé")
check(control(["anything"]) is None, "action inconnue refusée")
apt = lib["exec_level"]
check(apt(["apt-get", "-y", "-q", "-o", "DPkg::Lock::Timeout=120", "-o", "APT::Status-Fd=5",
           "install", "--no-install-recommends", "--", "gimp"]) == "admin", "installation APT")
check(apt(["apt-get", "-q", "-o", "DPkg::Lock::Timeout=30", "update"]) == "user", "listes APT")
check(apt(["apt-get", "-o", "APT::Update::Pre-Invoke::=sh", "update"]) is None, "option APT dangereuse refusée")
check(apt(["apt-get", "install", "--", "/tmp/x.deb"]) == "admin", "paquet local")
check(apt(["apt-get", "install", "--", "../x"]) is None, "chemin relatif refusé")
check(apt(["sh", "-c", "id"]) is None, "commande quelconque refusée")
check(apt(["dpkg", "-i", "x.deb"]) is None, "dpkg direct refusé")

# ── Serveur sur une racine factice ─────────────────────────────────────────
work = Path(tempfile.mkdtemp(prefix="winlux-root-test."))
(work / "etc/winlux").mkdir(parents=True)
(work / "run/winlux").mkdir(parents=True)
crypt = lib["_crypt"]
alice_hash = crypt("motdepasse", "$6$sel$")
check(alice_hash.startswith("$6$"), "crypt() disponible")
(work / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{alice_hash}:19000::::::\nbob:{crypt('bob', '$6$sel$')}:19000::::::\n")
(work / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")
(work / "etc/winlux/preferences.conf").write_text("THEME=dark\n")
log = work / "control.log"
mock = work / "control-mock"
mock.write_text(f"#!/bin/sh\nprintf '%s|%s|%s\\n' \"$WINLUX_CONTROL_RELAYED\" \"$(pwd)\" \"$*\" >> {log}\necho sortie-relais\nexit 3\n")
mock.chmod(0o755)
env = {**os.environ, "WINLUX_ROOT_PREFIX": str(work), "WINLUX_ROOT_SOCKET": str(work / "root.sock"),
       "WINLUX_ROOT_CONTROL": str(mock), "WINLUX_ROOT_TEST_PEER": "1002:bob"}


def start(peer):
    proc = subprocess.Popen([sys.executable, "-B", str(TOOL), "serve"], env={**env, "WINLUX_ROOT_TEST_PEER": peer})
    for _ in range(100):
        try:
            with socket.socket(socket.AF_UNIX) as probe:
                probe.connect(str(work / "root.sock"))
            return proc
        except OSError:
            time.sleep(0.05)
    proc.kill()
    check(False, "le relais ne démarre pas")


def client(*args, stdin=None, extra_env=None):
    return subprocess.run([sys.executable, "-B", str(TOOL), *args], env={**env, **(extra_env or {})},
                          input=stdin, capture_output=True, text=True, timeout=30, cwd=str(work))


server = start("1002:bob")
try:
    # Préférence : écrite par le relais, lisible par tous.
    check(client("conf", "preferences.conf", "THEME", "light").returncode == 0, "préférence écrite")
    text = (work / "etc/winlux/preferences.conf").read_text()
    check("THEME=light" in text and "THEME=dark" not in text, "préférence remplacée")
    check((work / "etc/winlux/preferences.conf").stat().st_mode & 0o777 == 0o644, "préférence lisible (644)")
    check(client("conf", "shadow", "X", "1").returncode != 0, "fichier hors liste refusé")
    check(client("conf", "lock.conf", "LOCK_AFTER", "1;rm").returncode != 0, "valeur douteuse refusée")
    check(client("conf", "cpu.conf", "CPU_PROFILE", "performance").returncode != 0, "clé système refusée")
    # Action utilisateur : exécutée, sortie et code transmis, dossier courant gardé.
    run = client("control", "theme-dark")
    check(run.returncode == 3 and "sortie-relais" in run.stdout, f"action relayée : {run}")
    check(log.read_text().splitlines()[-1] == f"1|{work}|theme-dark", "arguments et dossier courant")
    # Action administrateur sans élévation (pas d'écran) : refusée.
    run = client("control", "share-add", str(work), extra_env={"DISPLAY": ""})
    check(run.returncode != 0 and "administrateur" in run.stderr, "partage sans confirmation refusé")
    check(len(log.read_text().splitlines()) == 1, "rien exécuté sans élévation")
    # Mot de passe : bob vérifie le sien, pas celui d'alice.
    check(client("check", "bob", stdin="bob\n").returncode == 0, "bon mot de passe")
    check(client("check", "bob", stdin="faux\n").returncode != 0, "mauvais mot de passe")
    check(client("check", "alice", stdin="motdepasse\n").returncode != 0, "mot de passe d'un autre compte via check")
    # Élévation : un compte standard ne s'élève pas avec son propre mot de passe…
    check(client("elevate", "bob", stdin="bob\n").returncode != 0, "compte standard non administrateur")
    # … mais avec celui d'un administrateur, comme le Contrôle de compte Windows.
    check(client("elevate", "alice", stdin="motdepasse\n").returncode == 0, "élévation par un administrateur")
    run = client("control", "share-add", str(work), extra_env={"DISPLAY": ""})
    check(run.returncode == 3, f"partage après élévation : {run}")
    check(log.read_text().splitlines()[-1].endswith(f"share-add {work}"), "partage exécuté")
    # La Boutique : refus d'une commande hors liste, même élevé.
    check(client("exec", "sh", "-c", "id").returncode != 0, "commande quelconque refusée par le serveur")
finally:
    server.terminate()
    server.wait(timeout=10)

# Compte inconnu du bureau (navigateur isolé) : tout est refusé.
server = start("1005:winlux-web")
try:
    run = client("control", "theme-dark")
    check(run.returncode != 0 and "refusé" in run.stderr, "compte hors bureau refusé")
finally:
    server.terminate()
    server.wait(timeout=10)

import shutil
shutil.rmtree(work, ignore_errors=True)
print("OK: winlux-root (politique, préférences, actions relayées, mots de passe, élévation, comptes)")
