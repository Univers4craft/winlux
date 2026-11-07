#!/usr/bin/python3
"""Imprimantes : « Rechercher » refusé au compte administrateur (#622).

PC de damien, v0.2.19 : lpinfo répondait « Interdit », le groupe lpadmin
(SystemGroup de CUPS) n'existait pas et l'administrateur n'en était pas
membre ; Paramètres accusait à tort l'imprimante. Vérifie, sur une racine
factice (commandes notées, rien de lancé) :
- winlux-accounts sync-printing crée lpadmin s'il manque, y ajoute chaque
  administrateur et en retire les comptes standard (#741 : ISO bba69cc,
  « lpadmin:x:106:camille », le compte standard et pas l'administrateur),
  sans rien refaire la 2e fois ;
- add NOM user ne donne pas lpadmin, add NOM admin oui ;
- role NOM admin ajoute lpadmin, role NOM user le retire ;
- winlux-system-services le lance avant cupsd ;
- Paramètres montre le vrai refus de CUPS, traduit dans les 4 langues.
"""
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-accounts.py"
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


work = Path(tempfile.mkdtemp(prefix="winlux-printers-"))
try:
    root = work / "root"
    (root / "etc/winlux").mkdir(parents=True)
    (root / "etc/passwd").write_text(
        "root:x:0:0:root:/root:/bin/bash\n"
        "damien:x:1000:1000:Damien:/home/damien:/bin/bash\n"
        "invite:x:1001:1001:Invité:/home/invite:/bin/bash\n")
    group = root / "etc/group"
    group.write_text("sudo:x:27:damien\nnetdev:x:110:damien,invite\n")
    log = work / "commands.log"
    env = {**os.environ, "WINLUX_ACCOUNTS_ROOT": str(root), "WINLUX_ACCOUNTS_LOG": str(log)}

    def accounts(*args):
        return subprocess.run([sys.executable, "-B", str(TOOL), *args], env=env, capture_output=True, text=True, timeout=30)

    result = accounts("sync-printing")
    commands = log.read_text().splitlines() if log.exists() else []
    check(result.returncode == 0, f"sync-printing : {result.stderr}")
    check(commands == ["groupadd --system lpadmin", "gpasswd -a damien lpadmin"],
          f"groupe absent : attendu création + damien, obtenu {commands}")

    log.unlink()
    group.write_text("sudo:x:27:damien\nlpadmin:x:120:damien\n")
    result = accounts("sync-printing")
    check(result.returncode == 0 and not log.exists(), "sync-printing refait un travail déjà fait")

    # ISO de validation bba69cc : le compte standard dans lpadmin, pas l'administrateur.
    group.write_text("sudo:x:27:damien\nlpadmin:x:106:invite\n")
    result = accounts("sync-printing")
    commands = log.read_text().splitlines()
    check(result.returncode == 0 and sorted(commands) == ["gpasswd -a damien lpadmin", "gpasswd -d invite lpadmin"],
          f"lpadmin inversé non réparé : {commands} {result.stderr}")

    log.unlink()
    group.write_text("sudo:x:27:damien\nlpadmin:x:120:\n")
    accounts("role", "invite", "admin")
    commands = log.read_text().splitlines()
    check("gpasswd -a invite lpadmin" in commands, f"role admin sans lpadmin : {commands}")
    check("gpasswd -a invite sudo" in commands, f"role admin sans sudo : {commands}")

    log.unlink()
    group.write_text("sudo:x:27:damien,invite\nlpadmin:x:120:damien,invite\n")
    (root / "etc/winlux/users.conf").write_text("damien:root:shadow\ninvite:root:shadow\n")
    accounts("role", "invite", "user")
    commands = log.read_text().splitlines()
    check("gpasswd -d invite lpadmin" in commands, f"role user garde lpadmin : {commands}")

    for name, kind, expect in (("camille", "user", False), ("paul", "admin", True)):
        log.unlink()
        group.write_text("sudo:x:27:damien\nlpadmin:x:120:damien\naudio:x:29:\nscanner:x:120:\n")
        result = subprocess.run([sys.executable, "-B", str(TOOL), "add", name, kind], env=env, input="\n",
                                capture_output=True, text=True, timeout=30)
        useradd = next((line for line in log.read_text().splitlines() if line.startswith("useradd")), "")
        groups_given = useradd.split("-G ", 1)[1].split()[0].split(",") if "-G " in useradd else []
        check(result.returncode == 0, f"add {name} {kind} : {result.stderr}")
        check(("lpadmin" in groups_given) == expect, f"add {name} {kind} : groupes {groups_given}")
        check("scanner" in groups_given and "audio" in groups_given, f"add {name} {kind} : groupes de base perdus {groups_given}")
finally:
    shutil.rmtree(work)

services = (HERE / "winlux-system-services.sh").read_text()
check("winlux-accounts sync-printing" in services, "winlux-system-services ne lance pas sync-printing")
check(services.index("sync-printing") < services.index("/usr/sbin/cupsd -f"), "sync-printing doit passer avant cupsd")

pages = (HERE / "winlux-settings-pages.py").read_text()
search = pages[pages.index("def find_printers"):pages.index("def add_printer")]
check("found.returncode" in search and "Le service d’impression a refusé la recherche :" in search,
      "Paramètres masque encore le refus de CUPS derrière « Aucune imprimante trouvée »")
check("subprocess.run([\"lpinfo\", \"-v\"]" in search, "la recherche ne lit plus la sortie d'erreur de lpinfo")
for lang, expected in (("en", "The print service refused the search:"), ("es", "El servicio de impresión rechazó la búsqueda:"),
                       ("de", "Der Druckdienst hat die Suche abgelehnt:")):
    out = subprocess.run([sys.executable, "-c", "import runpy,sys;m=runpy.run_path(sys.argv[1],run_name='t');"
                          "print(m['_t']('Le service d’impression a refusé la recherche :'))", str(HERE / "winlux-settings-pages.py")],
                         env={**os.environ, "WINLUX_LANG": lang}, capture_output=True, text=True, timeout=60)
    check(out.stdout.strip() == expected, f"traduction {lang} : {out.stdout!r} {out.stderr[-300:]!r}")

if failures:
    raise SystemExit("ÉCHEC test-printer-perms :\n- " + "\n- ".join(failures))
print("OK test-printer-perms : administrateurs dans lpadmin, refus de CUPS affiché")
