#!/usr/bin/python3
"""Réseau utilisable depuis le compte du bureau sur le système installé.

Régression v0.2.17 (PC de damien) : « connecté, mais rien ne marche » et
Wi-Fi « non autorisé ». Les services root démarraient en umask 077 :
/etc/resolv.conf écrit en 0600 par NetworkManager (aucun nom résolu depuis
le compte) et règle polkit du réseau absente ou illisible du disque installé.
"""
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SCRIPT = HERE / "winlux-network-perms.sh"
RULE = ROOT / "integration/native/network/50-winlux-network.rules"
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


def mode(path):
    return stat.S_IMODE(path.stat().st_mode)


def repair(root):
    return subprocess.run(["sh", str(SCRIPT)], env={**os.environ, "WINLUX_SYSTEM_ROOT": str(root)},
                          capture_output=True, text=True, timeout=30)


work = Path(tempfile.mkdtemp())
try:
    # Disque installé tel que trouvé sur le portable : resolv.conf 0600, pas de règle.
    (work / "etc").mkdir()
    resolv = work / "etc/resolv.conf"
    resolv.write_text("nameserver 192.168.1.1\n")
    resolv.chmod(0o600)
    # /etc/timezone écrit en 0600 au premier démarrage (umask 077) : le compte
    # lisait « Permission denied » (testeur VM, #419).
    timezone = work / "etc/timezone"
    timezone.write_text("Europe/Paris\n")
    timezone.chmod(0o600)
    (work / "etc/default").mkdir()
    locale = work / "etc/default/locale"
    locale.write_text("LANG=fr_FR.UTF-8\n")
    locale.chmod(0o600)
    (work / "usr/share/winlux/native/network").mkdir(parents=True)
    shutil.copy(RULE, work / "usr/share/winlux/native/network/")
    result = repair(work)
    check(result.returncode == 0, f"winlux-network-perms : {result.stderr}")
    rule = work / "etc/polkit-1/rules.d/50-winlux-network.rules"
    check(mode(resolv) == 0o644, f"resolv.conf en {oct(mode(resolv))}, attendu 0644")
    check(mode(timezone) == 0o644, f"/etc/timezone en {oct(mode(timezone))}, attendu 0644")
    check(mode(locale) == 0o644, f"/etc/default/locale en {oct(mode(locale))}, attendu 0644")
    check(rule.is_file() and rule.read_text() == RULE.read_text(), "règle polkit du réseau absente du disque installé")
    check(rule.is_file() and mode(rule) == 0o644, "règle polkit illisible par polkitd")
    check(mode(work / "etc/polkit-1/rules.d") == 0o755, "dossier rules.d illisible par polkitd")
    # Dossier d'Ubuntu en 0700 et ancienne règle : remis lisibles, règle à jour.
    (work / "etc/polkit-1/rules.d").chmod(0o700)
    rule.write_text("// ancienne\n")
    other = work / "etc/polkit-1/rules.d/49-autre.rules"
    other.write_text("// autre\n")
    other.chmod(0o600)
    repair(work)
    check(rule.read_text() == RULE.read_text(), "règle obsolète non remplacée")
    check(mode(work / "etc/polkit-1/rules.d") == 0o755 and mode(other) == 0o644, "rules.d d'Ubuntu reste illisible")
    # resolv.conf en lien vers /run (NetworkManager) : la cible est réparée.
    (work / "run/NetworkManager").mkdir(parents=True)
    real = work / "run/NetworkManager/resolv.conf"
    real.write_text("nameserver 1.1.1.1\n")
    real.chmod(0o600)
    resolv.unlink()
    resolv.symlink_to(real)
    repair(work)
    check(mode(real) == 0o644, "cible du lien resolv.conf illisible")
    # Rien à faire : aucune sortie, aucune erreur.
    quiet = repair(work)
    check(quiet.returncode == 0 and quiet.stdout == "", f"deuxième passage bavard : {quiet.stdout!r}")
finally:
    shutil.rmtree(work, ignore_errors=True)

# Branchement : au démarrage avant polkitd, puis à chaque bail.
services = (HERE / "winlux-system-services.sh").read_text()
check("\numask 022\n" in services and services.index("\numask 022\n") < services.index("NetworkManager --no-daemon"),
      "services root lancés en umask 077")
check("/usr/bin/winlux-network-perms" in services
      and services.index("/usr/bin/winlux-network-perms") < services.index("/usr/lib/polkit-1/polkitd >"),
      "règle polkit non posée avant polkitd")
watch = (HERE / "winlux-network-watch.sh").read_text()
check("perms\n" in watch and "/usr/bin/winlux-network-perms" in watch, "resolv.conf non surveillé après un renouvellement")
stage = (HERE / "stage-desktop-components.sh").read_text()
check("'winlux-network-perms.sh:winlux-network-perms'" in stage, "winlux-network-perms absent de l'image")
check('cp -a "$project/integration/native/." "$stage/usr/share/winlux/native/"' in stage, "source de la règle absente de l'image")
check('"$stage/etc/polkit-1/rules.d/50-winlux-network.rules"' in stage, "règle polkit absente de l'image")
guest = (HERE / "test-native-session-guest.py").read_text()
check("50-winlux-network.rules" in guest and "resolv.conf" in guest, "contrôle en VM absent")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("OK: resolv.conf lisible et règle polkit du réseau posée (système installé et Live)")
