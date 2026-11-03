#!/usr/bin/python3
"""su et sudo sur un système installé (signalé par damien sur la v0.2.17 :
« su ne fonctionne pas » en voulant faire un apt-get install).

1. Droits de la base Ubuntu : la construction de l'image efface les bits
   setuid/setgid de passwd, chfn, unix_chkpwd… et le groupe shadow de
   /etc/shadow. winlux-update-apt --boot les rétablit (BASE_MODES), y compris
   sur un système déjà installé (marqueur version 3).
2. Root est verrouillé : /usr/bin/su devient une bascule (dpkg-divert) ; « su »
   d'un administrateur passe par sudo, qui demande son propre mot de passe.
"""
import os
from pathlib import Path
import runpy
import stat
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


# ── 1. Droits spéciaux de la base Ubuntu ────────────────────────────────────
update = runpy.run_path(str(ROOT / "scripts/winlux-update-apt.py"), run_name="su_sudo_test")
modes = {relative: (user, group, mode) for relative, user, group, mode in update["BASE_MODES"]}
# Relevé de `tar tvzf ubuntu-base-26.04.1-base-amd64.tar.gz` (fichiers non root/root ou à bits spéciaux).
for relative, expected in {
        "usr/bin/passwd": ("root", "root", 0o4755), "usr/bin/su": ("root", "root", 0o4755),
        "usr/bin/chfn": ("root", "root", 0o4755), "usr/bin/chsh": ("root", "root", 0o4755),
        "usr/bin/gpasswd": ("root", "root", 0o4755), "usr/bin/mount": ("root", "root", 0o4755),
        "usr/bin/umount": ("root", "root", 0o4755), "usr/bin/chage": ("root", "shadow", 0o2755),
        "usr/bin/expiry": ("root", "shadow", 0o2755), "usr/sbin/unix_chkpwd": ("root", "shadow", 0o2755),
        "usr/sbin/pam_extrausers_chkpwd": ("root", "shadow", 0o2755),
        "etc/shadow": ("root", "shadow", 0o640), "etc/gshadow": ("root", "shadow", 0o640),
        "usr/bin/sudo.ws": ("root", "root", 0o4755), "usr/bin/pkexec": ("root", "root", 0o4755)}.items():
    ok(modes.get(relative) == expected, f"droits rétablis : {relative} {oct(expected[2])}")

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    (root / "etc").mkdir()
    me, my_group = os.getuid(), os.getgid()
    # Noms de la base Ubuntu, numéros de l'utilisateur du test (chown sans privilège).
    (root / "etc/passwd").write_text(f"root:x:{me}:{my_group}:root:/root:/bin/bash\n")
    (root / "etc/group").write_text(f"root:x:{my_group}:\nshadow:x:{my_group}:\nutmp:x:{my_group}:\n")
    for relative in ("usr/bin/passwd", "usr/bin/sudo.ws", "usr/sbin/unix_chkpwd", "etc/shadow"):
        (root / relative).parent.mkdir(parents=True, exist_ok=True)
        (root / relative).write_text("")
        (root / relative).chmod(0o755 if "bin" in relative else 0o600)
    (root / "usr/bin/su").symlink_to("passwd")
    fixed = update["restore_base_modes"](root)
    ok(stat.S_IMODE((root / "usr/bin/passwd").stat().st_mode) == 0o4755, "passwd retrouve son setuid")
    ok(stat.S_IMODE((root / "usr/bin/sudo.ws").stat().st_mode) == 0o4755, "sudo retrouve son setuid")
    ok(stat.S_IMODE((root / "usr/sbin/unix_chkpwd").stat().st_mode) == 0o2755, "unix_chkpwd retrouve son setgid")
    ok(stat.S_IMODE((root / "etc/shadow").stat().st_mode) == 0o640, "/etc/shadow lisible par le groupe shadow")
    ok("usr/bin/su" not in fixed, "un lien symbolique n'est jamais modifié")
    ok(update["restore_base_modes"](root) == [], "deuxième passage : rien à corriger")
    # Groupe absent de /etc/group : fichier laissé tel quel, pas d'exception.
    (root / "etc/group").write_text(f"root:x:{my_group}:\n")
    (root / "etc/shadow").chmod(0o600)
    update["restore_base_modes"](root)
    ok(stat.S_IMODE((root / "etc/shadow").stat().st_mode) == 0o600, "groupe shadow absent : fichier inchangé")

apt = (ROOT / "scripts/winlux-update-apt.py").read_text()
ok('BASE_MODES_VERSION = "3"' in apt and "if read(restored) != BASE_MODES_VERSION:" in apt,
   "un système déjà installé reçoit les droits au démarrage suivant la mise à jour")
ok("except (OSError, subprocess.CalledProcessError)" in apt[apt.index("if read(restored) != BASE_MODES_VERSION:"):][:600],
   "une erreur de droits ne bloque jamais le démarrage")
ok('test "$(cat /var/lib/winlux/apt-base-permissions-restored 2>/dev/null)" = 3' in
   (ROOT / "scripts/winlux-update.sh").read_text(),
   "le démarrage rapide ne saute pas la correction (marqueur version 3)")

# ── 2. « su » quand root est verrouillé : /usr/bin/su lui-même ─────────────
# Retour VM sur la v0.2.18 : une fonction bash du Terminal Winlux ne suffit
# pas, /usr/bin/su restait le vrai binaire (« Échec de l'authentification »).
wrapper = ROOT / "integration/native/shell/su"
ok(wrapper.read_bytes()[:10] == b"#!/bin/sh\n" and os.access(wrapper, os.X_OK), "bascule su : script sh exécutable")
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    log = work / "log"
    for name in ("sudo", "su.util-linux"):
        tool = work / name
        tool.write_text(f'#!/bin/sh\nprintf "%s" "{name}" >> "{log}"; for a; do printf " [%s]" "$a" >> "{log}"; done; echo >> "{log}"\n')
        tool.chmod(0o755)
    (work / "id").write_text('#!/bin/sh\ncase "$1" in -u) echo "${FAKE_UID:-1000}";; -Gn) echo "${FAKE_GROUPS:-damien sudo audio}";; esac\n')
    (work / "id").chmod(0o755)

    def run_su(arguments, lang="fr_FR.UTF-8", **env):
        log.write_text("")
        result = subprocess.run(
            ["sh", "-c", f'"{wrapper}" {arguments}'],
            env={"PATH": f"{work}:/usr/bin:/bin", "LANG": lang, "HOME": directory,
                 "WINLUX_REAL_SU": str(work / "su.util-linux"),
                 "WINLUX_ROOT_ENABLED_FILE": str(work / "root-enabled"), **env},
            capture_output=True, text=True, timeout=20)
        return log.read_text().strip(), result.stderr

    called, message = run_su("")
    ok(called == "sudo [-s]", f"su → sudo -s (obtenu : {called})")
    ok("verrouillé" in message and "votre propre mot de passe" in message, "message clair en français")
    ok(run_su("-")[0] == "sudo [-i]", "su - → sudo -i")
    ok(run_su("-l root")[0] == "sudo [-i]", "su -l root → sudo -i")
    ok(run_su("root")[0] == "sudo [-s]", "su root → sudo -s")
    ok(run_su("-c 'apt-get install hello'")[0] == "sudo [sh] [-c] [apt-get install hello]",
       "su -c 'apt-get install …' → sudo sh -c")
    ok(run_su("- -c 'apt-get update'")[0] == "sudo [-i] [sh] [-c] [apt-get update]", "su - -c … → sudo -i sh -c")
    ok(run_su("--command='id'")[0] == "sudo [sh] [-c] [id]", "su --command=… → sudo sh -c")
    ok(run_su("bob")[0] == "su.util-linux [bob]", "su bob : vrai su")
    ok(run_su("- bob -c id")[0] == "su.util-linux [-] [bob] [-c] [id]", "su - bob -c id : vrai su, arguments intacts")
    ok(run_su("-s /bin/sh")[0] == "su.util-linux [-s] [/bin/sh]", "option inconnue : vrai su")
    ok(run_su("-c")[0] == "su.util-linux [-c]", "-c sans commande : vrai su (qui signale l'erreur)")
    called, message = run_su("", FAKE_GROUPS="invite audio")
    ok(called == "su.util-linux", "compte non administrateur : vrai su")
    ok("administrateur peut utiliser sudo" in message, "compte non administrateur : il sait pourquoi su échoue")
    ok(run_su("", FAKE_UID="0")[0] == "su.util-linux", "déjà root : vrai su, sans message")
    (work / "root-enabled").write_text("")
    ok(run_su("")[0] == "su.util-linux", "mot de passe root défini (root-enabled) : vrai su")
    (work / "root-enabled").unlink()
    for lang, word in (("en_US.UTF-8", "locked"), ("es_ES.UTF-8", "bloqueada"), ("de_DE.UTF-8", "gesperrt")):
        ok(word in run_su("", lang=lang)[1], f"message traduit ({lang[:2]})")

# Pose de la bascule : dpkg-divert garde le vrai su (setuid) en su.util-linux.
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    (root / "usr/bin").mkdir(parents=True)
    (root / "usr/share/winlux/native/shell").mkdir(parents=True)
    (root / "usr/share/winlux/native/shell/su").write_bytes(wrapper.read_bytes())
    (root / "usr/bin/su").write_bytes(b"\x7fELF binaire util-linux")
    (root / "usr/bin/su").chmod(0o4755)
    diverted = []

    def fake_divert():
        diverted.append(1)
        os.rename(root / "usr/bin/su", root / "usr/bin/su.util-linux")

    ok(update["install_su_wrapper"](root, fake_divert) is True, "bascule posée")
    ok(diverted == [1], "le vrai su est détourné par dpkg-divert (survit aux mises à jour)")
    ok((root / "usr/bin/su").read_bytes() == wrapper.read_bytes(), "/usr/bin/su est la bascule Winlux")
    ok(stat.S_IMODE((root / "usr/bin/su").stat().st_mode) == 0o755, "bascule sans setuid")
    ok(stat.S_IMODE((root / "usr/bin/su.util-linux").stat().st_mode) == 0o4755, "vrai su toujours setuid")
    ok(update["install_su_wrapper"](root, fake_divert) is False and diverted == [1], "deuxième passage : rien")
    (root / "etc").mkdir()
    (root / "etc/passwd").write_text(f"root:x:{os.getuid()}:{os.getgid()}:root:/root:/bin/bash\n")
    (root / "etc/group").write_text(f"root:x:{os.getgid()}:\n")
    update["restore_base_modes"](root)
    ok(stat.S_IMODE((root / "usr/bin/su").stat().st_mode) == 0o755, "jamais de setuid remis sur le script su")
    (root / "usr/share/winlux/native/shell/su").unlink()
    ok(update["install_su_wrapper"](root, fake_divert) is False, "bascule absente du paquet : su intact")

ok('cp -a "$project/integration/native/." "$stage/usr/share/winlux/native/"' in
   (ROOT / "scripts/stage-desktop-components.sh").read_text(), "integration/native copié dans l'image")
first_boot = (ROOT / "scripts/winlux-first-boot.sh").read_text()
ok(": >/etc/winlux/root-enabled" in first_boot and "rm -f /etc/winlux/root-enabled" in first_boot,
   "premier démarrage : marqueur root-enabled selon le choix du mot de passe root")
guest = (ROOT / "scripts/test-apt-native-guest.py").read_text()
ok('"/usr/bin/passwd"' in guest and '"/usr/sbin/unix_chkpwd"' in guest and "su.util-linux" in guest,
   "contrôle en VM des droits rétablis et de la bascule su")
ok("install_su_wrapper()" in apt, "la bascule su est posée au démarrage")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: su et sudo (droits de la base Ubuntu, su par sudo quand root est verrouillé)")
