#!/usr/bin/python3
"""Tests hors ligne de winlux-uac : comptes administrateurs, choix du compte,
vérification du mot de passe et traductions. Aucun affichage n'est ouvert."""
import os
from pathlib import Path
import runpy
import tempfile

tmp = Path(tempfile.mkdtemp())
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ["WINLUX_LANG"] = "fr"
(tmp / "etc/winlux").mkdir(parents=True)
(tmp / "run/winlux").mkdir(parents=True)
uac = runpy.run_path(str(Path(__file__).with_name("winlux-uac.py")), run_name="uac_test")
lock = uac["LOCK"]

digest = lock["_crypt"]("Admin-7", "$6$winluxuac$")
(tmp / "etc/passwd").write_text(
    "root:x:0:0:root:/root:/bin/bash\n"
    "alice:x:1000:1000:Alice:/home/alice:/bin/bash\n"
    "bob:x:1001:1001:Bob:/home/bob:/bin/bash\n")
(tmp / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{digest}:19000:0:99999:7:::\nbob:{digest}:19000::::::\n")
(tmp / "etc/group").write_text("root:x:0:\nsudo:x:27:alice\nusers:x:100:bob\n")

assert uac["admin_accounts"]() == ["alice", "root"]

# Administratrice : son propre mot de passe.
os.environ["WINLUX_SESSION_USER"] = "alice"
assert uac["target_account"]() == ("alice", True)
assert uac["needs_password"]("alice")
assert uac["verify"]("alice", "Admin-7")
assert not uac["verify"]("alice", "admin-7")

# Compte standard : on demande le mot de passe d'un administrateur.
os.environ["WINLUX_SESSION_USER"] = "bob"
assert uac["target_account"]() == ("alice", False)

# Rôle administrateur de la session Live.
(tmp / "etc/winlux/users.conf").write_text("carla:admin:live-pass\n")
assert "carla" in uac["admin_accounts"]()
assert uac["verify"]("carla", "live-pass")
assert not uac["verify"]("carla", "faux")

# Aucun compte inconnu n'est accepté.
assert not uac["verify"]("personne", "x")

# Traductions : chaque texte existe dans les quatre langues.
for fr, row in uac["_TR"].items():
    assert len(row) == 3 and all(row), fr
assert "{name}" in uac["_TR"]["Pour continuer, entrez le mot de passe de {name}."][2]

print("OK: winlux-uac (comptes administrateurs, mot de passe, langues)")
