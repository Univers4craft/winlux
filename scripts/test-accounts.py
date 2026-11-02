#!/usr/bin/python3
"""Comptes locaux : Paramètres › Comptes, winlux-accounts, « Changer
d'utilisateur » (écran de verrouillage + relais root) et « Mettre en veille ».

Racines factices partout : aucun compte réel n'est touché, les commandes
système (useradd, chpasswd…) sont notées dans un journal.
"""
import os
from pathlib import Path
import runpy
import socket
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-accounts-"))


def check(condition, message):
    if not condition:
        print("ERREUR :", message, file=sys.stderr)
        sys.exit(1)


# ── winlux-accounts sur une racine factice ─────────────────────────────────
root = work / "root"
(root / "etc/winlux").mkdir(parents=True)
(root / "run/winlux/sessions").mkdir(parents=True)
(root / "etc/passwd").write_text(
    "root:x:0:0:root:/root:/bin/bash\n"
    "alice:x:1000:1000:Alice Martin,,,:/home/alice:/bin/bash\n"
    "bob:x:1001:1001:Bob:/home/bob:/bin/bash\n"
    "winlux-web:x:1500:1500::/var/lib/winlux-web:/usr/sbin/nologin\n")
(root / "etc/group").write_text("sudo:x:27:alice\naudio:x:29:alice,bob\nvideo:x:44:\n")
(root / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")
log = work / "commands.log"
env = {**os.environ, "WINLUX_ACCOUNTS_ROOT": str(root), "WINLUX_ACCOUNTS_LOG": str(log)}
TOOL = HERE / "winlux-accounts.py"


def accounts(*args, stdin=""):
    return subprocess.run([sys.executable, "-B", str(TOOL), *args], env=env, input=stdin,
                          capture_output=True, text=True, timeout=30)


listed = accounts("list")
check(listed.returncode == 0 and listed.stdout.splitlines() == ["alice:admin:Alice Martin", "bob:user:Bob"],
      f"liste des comptes : {listed.stdout!r} {listed.stderr!r}")

# Ajout : groupes existants seulement, sudo pour un administrateur, mot de passe par stdin.
result = accounts("add", "chloe", "admin", "Chloé Durand", stdin="Secret-1\n")
check(result.returncode == 0, f"ajout refusé : {result.stderr}")
text = log.read_text()
check("useradd -m -s /bin/bash -c Chloé Durand -G audio,video,sudo chloe" in text, f"useradd : {text}")
check("chpasswd <mot de passe>" in text and "Secret-1" not in text, "mot de passe passé par l'entrée standard, jamais noté")
check("chloe:root:shadow" in (root / "etc/winlux/users.conf").read_text(), "users.conf suit le nouveau compte")
check(oct((root / "etc/winlux/users.conf").stat().st_mode)[-3:] == "644", "users.conf lisible par l'écran de verrouillage")
log.write_text("")
check(accounts("add", "dan", "user", stdin="\n").returncode == 0, "compte sans mot de passe")
check("passwd -d dan" in log.read_text(), "mot de passe vide = compte ouvert d'un clic")
for bad in ("Root", "root", "../x", "a b", "x" * 40, "administrateur"):
    check(accounts("add", bad, "user", stdin="x\n").returncode == 1, f"nom refusé : {bad}")
check(accounts("add", "alice", "user", stdin="x\n").returncode == 1, "compte existant refusé")
check(accounts("add", "eve", "superuser", stdin="x\n").returncode == 1, "type inconnu refusé")

# Type de compte : jamais sans administrateur.
(root / "etc/passwd").write_text((root / "etc/passwd").read_text() + "chloe:x:1002:1002:Chloé Durand:/home/chloe:/bin/bash\n")
(root / "etc/group").write_text("sudo:x:27:alice,chloe\naudio:x:29:alice,bob\nvideo:x:44:\n")
log.write_text("")
check(accounts("role", "bob", "admin").returncode == 0 and "gpasswd -a bob sudo" in log.read_text(), "bob administrateur")
check("bob:root:shadow" in (root / "etc/winlux/users.conf").read_text(), "rôle noté dans users.conf")
(root / "etc/group").write_text("sudo:x:27:alice\naudio:x:29:alice,bob\nvideo:x:44:\n")
(root / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\nchloe:user:shadow\n")
refused = accounts("role", "alice", "user")
check(refused.returncode == 1 and "administrateur" in refused.stderr, "le dernier administrateur reste administrateur")
check(accounts("remove", "alice", "keep").returncode == 1, "le dernier administrateur n'est pas supprimé")

# Suppression : session ouverte refusée, dossier gardé ou effacé.
(root / "run/winlux/sessions/bob").write_text("display=:1\n")
check(accounts("remove", "bob", "delete").returncode == 1, "session ouverte : suppression refusée")
(root / "run/winlux/sessions/bob").unlink()
log.write_text("")
check(accounts("remove", "bob", "delete").returncode == 0 and "userdel -r bob" in log.read_text(), "suppression avec fichiers")
check("bob:" not in (root / "etc/winlux/users.conf").read_text(), "users.conf sans le compte supprimé")
log.write_text("")
check(accounts("remove", "chloe", "keep").returncode == 0 and "userdel chloe" in log.read_text(), "suppression en gardant les fichiers")
check(accounts("password", "nobody-here", stdin="x\n").returncode == 1, "compte inconnu")

# ── Relais root : politique et « switch » ─────────────────────────────────
lib = runpy.run_path(str(HERE / "winlux-root.py"), run_name="winlux_root_lib")
for action in ("account-add", "account-password", "account-role", "account-remove"):
    check(lib["control_level"]([action, "x"]) == "admin", f"{action} demande un administrateur")
control_sh = (HERE / "winlux-control.sh").read_text()
check('account-add|account-password|account-role|account-remove) shift; winlux-accounts' in control_sh,
      "winlux-control transmet les actions de compte")

relay = work / "relay"
(relay / "etc/winlux").mkdir(parents=True)
(relay / "run/winlux").mkdir(parents=True)
crypt = lib["_crypt"]
(relay / "etc/shadow").write_text(f"alice:{crypt('a', '$6$sel$')}:19000::::::\nbob:{crypt('b-pass', '$6$sel$')}:19000::::::\n")
(relay / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")
calls = relay / "control.log"
mock = relay / "control-mock"
mock.write_text(f"#!/bin/sh\nprintf '%s\\n' \"$*\" >> {calls}\nexit 0\n")
mock.chmod(0o755)
renv = {**os.environ, "WINLUX_ROOT_PREFIX": str(relay), "WINLUX_ROOT_SOCKET": str(relay / "root.sock"),
        "WINLUX_ROOT_CONTROL": str(mock), "WINLUX_ROOT_TEST_PEER": "1000:alice"}
server = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-root.py"), "serve"], env=renv)
try:
    for _ in range(100):
        if (relay / "root.sock").exists():
            break
        time.sleep(0.05)

    def ask(payload):
        import json
        with socket.socket(socket.AF_UNIX) as client:
            client.connect(str(relay / "root.sock"))
            client.sendall((json.dumps(payload) + "\n").encode())
            return json.loads(client.makefile("r").readline() or "{}")
    check(ask({"op": "switch", "user": "bob", "password": "faux"}) == {"ok": False}, "mauvais mot de passe : pas de bascule")
    check(not calls.exists(), "aucune session ouverte sans le bon mot de passe")
    check(ask({"op": "switch", "user": "bob", "password": "b-pass"}) == {"ok": True}, "bon mot de passe : bascule")
    check(calls.read_text().strip() == "session-switch bob", f"winlux-sessions switch bob : {calls.read_text()!r}")
    check(ask({"op": "switch", "user": "mallory", "password": ""}) == {"ok": False}, "compte inconnu refusé")
    # Sans mot de passe, la session d'un autre demande un administrateur.
    answer = ask({"op": "control", "args": ["session-switch", "bob"]})
    check(answer.get("auth") == "required" or "error" in answer, f"session-switch d'un autre : {answer}")
finally:
    server.kill()

# ── Écran de verrouillage : autres comptes ─────────────────────────────────
lock_root = work / "lock"
(lock_root / "etc/winlux").mkdir(parents=True)
(lock_root / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\nroot:root:x\n")
os.environ["WINLUX_LOCK_ROOT"] = str(lock_root)
lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_test")
check(lock["other_accounts"]("alice") == ["bob"], "autres comptes, sans root")
(lock_root / "etc/winlux/users.conf").write_text("root:root:winlux\n")
check(lock["other_accounts"]("alice") == ["root"], "session Live : root seul")
check("switch" in lock["__doc__"] and 'command == "switch"' in (HERE / "winlux-lock.py").read_text(),
      "winlux-lock switch")

# ── Menu Démarrer : veille et changement d'utilisateur ────────────────────
os.environ["WINLUX_LANG"] = "fr"
start = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="start_test")
state = work / "power-state"
os.environ["WINLUX_POWER_STATE"] = str(state)
state.write_text("freeze mem disk\n")
check(start["power_menu_has_sleep"](), "veille proposée (mem)")
state.write_text("disk\n")
check(not start["power_menu_has_sleep"](), "veille cachée si le noyau ne la sait pas faire")
users = work / "users.conf"
os.environ["WINLUX_USERS_CONF"] = str(users)
users.write_text("alice:root:shadow\n")
check(not start["power_menu_has_switch"](), "un seul compte : pas de « Changer d'utilisateur »")
users.write_text("alice:root:shadow\nbob:user:shadow\n")
check(start["power_menu_has_switch"](), "deux comptes : « Changer d'utilisateur »")
source = (HERE / "winlux-start-menu.py").read_text()
check('"winlux-control", "power-suspend"' in source and '"switch"' in source, "actions du menu d'alimentation")
for lang, words in (("en", ("Switch user", "Sleep")), ("es", ("Cambiar de usuario", "Suspender")),
                    ("de", ("Benutzer wechseln", "Energie sparen"))):
    out = subprocess.run([sys.executable, "-c", "import runpy,sys;m=runpy.run_path(sys.argv[1],run_name='t');"
                          "print(m['_t']('Changer d’utilisateur'));print(m['_t']('Mettre en veille'))",
                          str(HERE / "winlux-start-menu.py")], env={**os.environ, "WINLUX_LANG": lang},
                         capture_output=True, text=True)
    check(tuple(out.stdout.split("\n")[:2]) == words, f"{lang} : {out.stdout!r} {out.stderr[-300:]!r}")

# ── Paramètres : page 33 « Comptes » ──────────────────────────────────────
settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="settings_test")
check(len(settings["PAGES"]) >= 34 and settings["PAGES"][33] == "Comptes", "page 33 = Comptes")
check(any(33 in pages for _, pages in settings["CATEGORIES"]), "catégorie Comptes")
pages = runpy.run_path(str(HERE / "winlux-settings-pages.py"), run_name="pages_test")
check(pages["PAGE_ACCOUNTS"] == 33 and pages["ExtraPages"].EXTRA_PAGES[33] == "page_accounts", "page branchée")
check(pages["accounts_module"]() is not None, "winlux-accounts trouvé à côté des Paramètres")
os.environ["WINLUX_LIVE_FLAG"] = str(work / "live-session")
check(pages["page_available"](33, {}), "page visible sur un système installé")
(work / "live-session").write_text("")
check(not pages["page_available"](33, {}), "page cachée en session Live")
for key in ("Votre compte", "Autres utilisateurs", "Ajouter un compte", "Changer le type de compte",
            "Administrateur", "Standard", "Les deux mots de passe sont différents."):
    row = pages["_TR"].get(key)
    check(row is not None and len(row) == 3 and all(row), f"traduction : {key}")

print("OK: comptes, changement d'utilisateur et veille")
