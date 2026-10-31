#!/usr/bin/python3
"""Tests hors ligne de winlux-lock : compte verrouillé, vérification du mot de
passe (empreinte crypt de /etc/shadow et fichier Live), réglages et décision
du service d'inactivité. Aucun affichage n'est ouvert."""
import contextlib
import io
import os
from pathlib import Path
import runpy
import tempfile
import time

tmp = Path(tempfile.mkdtemp())
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ.pop("WINLUX_SESSION_USER", None)
os.environ["WINLUX_LANG"] = "fr"
(tmp / "etc/winlux").mkdir(parents=True)
(tmp / "run/winlux").mkdir(parents=True)
lock = runpy.run_path(str(Path(__file__).with_name("winlux-lock.py")), run_name="lock_test")

# Empreinte SHA-512 produite par la libcrypt de la machine de test.
digest = lock["_crypt"]("Secret-42", "$6$winluxtest$")
assert digest.startswith("$6$winluxtest$") and len(digest) > 40, digest

(tmp / "etc/passwd").write_text(
    "root:x:0:0:root:/root:/bin/bash\n"
    "daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n"
    "nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n"
    "alice:x:1000:1000:Alice:/home/alice:/bin/bash\n")
(tmp / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{digest}:19000:0:99999:7:::\n")

# Session administrateur : le compte personnel créé à l'installation est protégé.
if os.geteuid() == 0:
    assert lock["session_account"]() == "alice"
os.environ["WINLUX_SESSION_USER"] = "alice"
assert lock["session_account"]() == "alice"
assert lock["account_home"]("alice") == "/home/alice"
assert lock["requires_password"]("alice")
assert lock["check_password"]("alice", "Secret-42")
assert not lock["check_password"]("alice", "secret-42")
assert not lock["check_password"]("alice", "")

# Session Live : root est verrouillé dans shadow, son mot de passe vient de users.conf.
(tmp / "etc/winlux/users.conf").write_text("root:root:winlux\ninvite:user:nopass\n")
assert lock["requires_password"]("root")
assert lock["check_password"]("root", "winlux")
assert not lock["check_password"]("root", "Winlux")
# Compte sans mot de passe : un clic suffit et rien ne se verrouille tout seul.
assert not lock["requires_password"]("invite")
assert lock["check_password"]("invite", "")
# Compte inconnu partout : jamais enfermé dehors.
assert not lock["requires_password"]("fantome")
# Empreinte shadow vide : pas de mot de passe.
(tmp / "etc/shadow").write_text("vide::19000::::::\n")
assert not lock["requires_password"]("vide")

# Réglages par défaut puis lus depuis lock.conf.
assert lock["lock_after"]() == 300 and lock["lock_on_resume"]()
(tmp / "etc/winlux/lock.conf").write_text("LOCK_AFTER=600\nLOCK_ON_RESUME=0\n")
assert lock["lock_after"]() == 600 and not lock["lock_on_resume"]()
(tmp / "etc/winlux/lock.conf").write_text("LOCK_AFTER=abc\n")
assert lock["lock_after"]() == 300

# Décision du service.
should = lock["should_lock"]
assert should(400, 300, 0, True, True) == (True, "idle")
assert should(100, 300, 0, True, True) == (False, "")
assert should(100, 300, 60, True, True) == (True, "resume")
assert should(100, 300, 60, False, True) == (False, "")
assert should(9999, 0, 0, True, True) == (False, "")
assert should(9999, 300, 60, True, False) == (False, "")
assert should(None, 300, 0, True, True) == (False, "")

# Verrouillage en cours : le PID doit vivre.
assert not lock["is_locked"]()
(tmp / "run/winlux/lock").mkdir()
(tmp / "run/winlux/lock/active").write_text(f"{os.getpid()}\n")
assert lock["is_locked"]()
(tmp / "run/winlux/lock/active").write_text("999999999\n")
assert not lock["is_locked"]()

# État lu par winlux-control.
(tmp / "etc/shadow").write_text(f"alice:{digest}:19000:0:99999:7:::\n")
(tmp / "etc/winlux/lock.conf").write_text("LOCK_AFTER=900\n")
out = io.StringIO()
with contextlib.redirect_stdout(out):
    lock["status"]()
fields = dict(line.split("=", 1) for line in out.getvalue().splitlines())
assert fields == {"lock_after": "900", "lock_on_resume": "1", "lock_account": "alice",
                  "lock_password": "1", "lock_active": "0"}, fields

# Date longue dans les quatre langues.
day = time.struct_time((2026, 10, 31, 16, 0, 0, 5, 304, 0))
assert lock["long_date"](day, "fr") == "samedi 31 octobre"
assert lock["long_date"](day, "en") == "Saturday, October 31"
assert lock["long_date"](day, "es") == "sábado, 31 de octubre"
assert lock["long_date"](day, "de") == "Samstag, 31. Oktober"
# Chaque texte de l'écran a ses trois traductions.
assert all(len(row) == 3 and all(row) for row in lock["_TR"].values())
print("OK: winlux-lock (compte, mot de passe crypt/Live, réglages, inactivité, réveil, état, langues)")
