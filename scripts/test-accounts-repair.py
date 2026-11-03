#!/usr/bin/env python3
"""Écran noir après une coupure juste après l'installation : plus jamais.

Après l'installation, une remise à zéro immédiate laissait /etc/passwd avec
des octets nuls à la place du compte : le bureau ne trouvait plus
l'utilisateur et winlux-native-config empilait des comptes système à moitié
créés (useradd --uid 1002, puis « le groupe 1004 n'existe pas »…).

Le test vérifie :
- winlux-accounts-repair : octets nuls, doublons, verrous orphelins,
  comptes repris de la sauvegarde passwd-/shadow-, compte de l'installation
  recréé sans sauvegarde, fichiers sains jamais réécrits ;
- winlux-native-config : groupe déjà présent réutilisé, useradd en échec
  remplacé par un ajout direct, compte annexe en échec non bloquant ;
- l'installateur : comptes écrits par temporaire + fsync + rename avec leur
  sauvegarde, « terminée » seulement après sync et démontage vérifié ;
- l'init : réparation avant le choix du compte, écran de secours au lieu
  d'un écran noir, les deux outils copiés dans l'image.
"""
import ast
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import types

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("repair", ROOT / "scripts/winlux-accounts-repair.py")
repair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair)

SYSTEM = "root:x:0:0:root:/root:/bin/bash\npolkitd:x:980:980:PolicyKit:/nonexistent:/bin/false\n"
USER = "damien:x:1000:1000:Damien:/home/damien:/bin/bash\n"
WEB = "winlux-web:!:1001:1001:Winlux web:/nonexistent:/bin/false\n"


def make_root(tmp, passwd, shadow, group, backups=None, config=True):
    etc = tmp / "etc"
    (etc / "winlux").mkdir(parents=True)
    (tmp / "home/damien").mkdir(parents=True)
    os.chown(tmp / "home/damien", 1000, 1000) if os.geteuid() == 0 else None
    (etc / "passwd").write_bytes(passwd)
    (etc / "shadow").write_bytes(shadow)
    (etc / "group").write_bytes(group)
    for name, content in (backups or {}).items():
        (etc / name).write_bytes(content)
    if config:
        (etc / "winlux/install-config.json").write_text(json.dumps({"username": "damien", "fullname": "Damien"}))
    return etc


def lines(path):
    return [line for line in path.read_text().splitlines() if line]


with tempfile.TemporaryDirectory() as tmp:
    tmp = pathlib.Path(tmp)
    # Disque de la VM du testeur : nuls à la place du compte, winlux-web en double.
    corrupted = (SYSTEM + "\0" * 100 + WEB + "winlux-web:!:1003:1001:Winlux web:/nonexistent:/bin/false\n").encode()
    etc = make_root(tmp, corrupted, b"root:!:19000:0:99999:7:::\n" + b"\0" * 80,
                    b"root:x:0:\ndamien:x:1000:\nsudo:x:27:damien\n",
                    {"passwd-": (SYSTEM + USER).encode(),
                     "shadow-": b"root:!:19000:0:99999:7:::\ndamien:$6$sel$hash:19000:0:99999:7:::\n"})
    for lock in ("passwd.lock", ".pwd.lock", "group.lock", "passwd+"):
        (etc / lock).write_text("123")
    fixes = repair.repair(str(etc), str(tmp / "home"))
    passwd = lines(etc / "passwd")
    assert "\0" not in (etc / "passwd").read_text()
    assert [line.split(":")[0] for line in passwd].count("winlux-web") == 1, passwd
    assert USER.strip() in passwd, passwd
    assert "damien:$6$sel$hash:19000:0:99999:7:::" in lines(etc / "shadow"), lines(etc / "shadow")
    for lock in ("passwd.lock", ".pwd.lock", "group.lock", "passwd+"):
        assert not (etc / lock).exists(), lock
    assert not list(etc.glob(".*winlux-*")), "fichier temporaire laissé"
    print("passwd abîmé : nuls et doublon retirés, compte et mot de passe repris de passwd-/shadow-, verrous supprimés")

with tempfile.TemporaryDirectory() as tmp:
    tmp = pathlib.Path(tmp)
    # Sans sauvegarde : le compte de l'installation est recréé, verrouillé.
    etc = make_root(tmp, (SYSTEM + "\0" * 60 + WEB).encode(), b"root:!:19000:0:99999:7:::\n",
                    b"root:x:0:\nwinlux-web:x:1001:\ndamien:x:1000:\n")
    repair.repair(str(etc), str(tmp / "home"))
    entry = next(line for line in lines(etc / "passwd") if line.startswith("damien:"))
    fields = entry.split(":")
    assert fields[2] == "1000" and fields[3] == "1000" and fields[5] == "/home/damien" and fields[6] == "/bin/bash", entry
    assert any(line.startswith("damien:!:") for line in lines(etc / "shadow"))
    print("sans sauvegarde : compte de l'installation recréé (UID 1000), mot de passe verrouillé")

with tempfile.TemporaryDirectory() as tmp:
    tmp = pathlib.Path(tmp)
    # Fichiers sains : rien n'est réécrit, et un compte supprimé exprès
    # (encore présent dans passwd-) ne revient pas.
    etc = make_root(tmp, (SYSTEM + USER).encode(), b"root:!:19000:0:99999:7:::\ndamien:x:19000:0:99999:7:::\n",
                    b"root:x:0:\ndamien:x:1000:\n",
                    {"passwd-": (SYSTEM + USER + "ancien:x:1005:1005::/home/ancien:/bin/bash\n").encode()})
    before = {name: (etc / name).stat().st_ino for name in ("passwd", "shadow", "group")}
    assert repair.repair(str(etc), str(tmp / "home")) == []
    assert before == {name: (etc / name).stat().st_ino for name in ("passwd", "shadow", "group")}
    assert "ancien" not in (etc / "passwd").read_text()
    print("fichiers sains : jamais réécrits, compte supprimé non ressuscité")

# ── winlux-native-config : ensure_system_account idempotent ──────────────
source = (ROOT / "scripts/winlux-native-config.py").read_text(encoding="utf-8")
tree = ast.parse(source)
wanted = {"_append_account_line", "ensure_system_account", "ensure_account_or_log"}
code = ast.Module(body=[node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in wanted],
                  type_ignores=[])
assert len(code.body) == 3, "fonctions de comptes introuvables dans winlux-native-config"


class Entry(types.SimpleNamespace):
    pass


def fake_world(users, groups, useradd_fails):
    calls = []

    def getpwnam(name):
        for user in users:
            if user.pw_name == name:
                return user
        raise KeyError(name)

    def getgrnam(name):
        for group in groups:
            if group.gr_name == name:
                return group
        raise KeyError(name)

    def run(command, check=False, **_):
        calls.append(command)
        if command[0] == "useradd" and useradd_fails:
            raise subprocess.CalledProcessError(1, command)
        if command[0] == "groupadd":
            groups.append(Entry(gr_name=command[-1], gr_gid=int(command[2])))
        if command[0] == "useradd":
            users.append(Entry(pw_name=command[-1], pw_uid=int(command[3]), pw_gid=int(command[5]), pw_dir=command[9]))
        return types.SimpleNamespace(returncode=0)

    def append(path, line, mode):
        calls.append(("append", path, line))
        fields = line.strip().split(":")
        if path == "/etc/group":
            groups.append(Entry(gr_name=fields[0], gr_gid=int(fields[2])))
        else:
            users.append(Entry(pw_name=fields[0], pw_uid=int(fields[2]), pw_gid=int(fields[3]), pw_dir=fields[5]))

    namespace = {
        "pwd": types.SimpleNamespace(getpwnam=getpwnam, getpwall=lambda: list(users)),
        "grp": types.SimpleNamespace(getgrnam=getgrnam, getgrall=lambda: list(groups)),
        "subprocess": types.SimpleNamespace(run=run, CalledProcessError=subprocess.CalledProcessError),
        "shutil": types.SimpleNamespace(which=lambda name: f"/usr/sbin/{name}"),
        "os": os, "sys": sys, "Path": pathlib.Path,
    }
    exec(compile(code, "winlux-native-config", "exec"), namespace)
    namespace["_append_account_line"] = append
    return namespace, calls


users = [Entry(pw_name="damien", pw_uid=1000, pw_gid=1000, pw_dir="/home/damien"),
         Entry(pw_name="winlux-web", pw_uid=1001, pw_gid=1001, pw_dir="/nonexistent")]
groups = [Entry(gr_name="damien", gr_gid=1000), Entry(gr_name="winlux-web", gr_gid=1001),
          Entry(gr_name="winlux-saver", gr_gid=1002)]
world, calls = fake_world(users, groups, useradd_fails=False)
saver = world["ensure_system_account"]("winlux-saver", 1002, "Winlux screensaver", "/home/winlux-saver")
assert saver.pw_uid == 1002 and saver.pw_gid == 1002, saver
assert not any(call[0] == "groupadd" for call in calls), calls
assert world["ensure_system_account"]("winlux-saver", 1002, "x", "/x") is saver and len(calls) == 1
print("groupe winlux-saver déjà là (création interrompue) : réutilisé, UID 1002, pas de 1003/1004")

users = users[:2]
groups = groups[:2]
world, calls = fake_world(users, groups, useradd_fails=True)
saver = world["ensure_system_account"]("winlux-saver", 1002, "Winlux screensaver", "/home/winlux-saver")
assert saver.pw_uid == 1002 and saver.pw_gid == 1002, saver
assert any(call[0] == "append" and call[1] == "/etc/passwd" for call in calls), calls
print("useradd en échec : compte ajouté directement, sans exception")


def broken(*_args):
    raise OSError("passwd illisible")


world["ensure_system_account"] = broken
assert world["ensure_account_or_log"]("winlux-saver", 1002, "x", "/x") is None
for name in ("messagebus", "polkitd", "winlux-saver"):
    assert f'ensure_account_or_log("{name}"' in source, f"compte {name} encore bloquant"
assert "winlux-accounts-repair" in source
print("compte annexe impossible : journalisé, la session continue")

# ── Installateur ────────────────────────────────────────────────────────
installer = (ROOT / "scripts/winlux-installer.py").read_text(encoding="utf-8")
tree = ast.parse(installer)
helper = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "write_account_file")
namespace = {"os": os}
exec(compile(ast.Module(body=[helper], type_ignores=[]), "installer", "exec"), namespace)
with tempfile.TemporaryDirectory() as tmp:
    path = pathlib.Path(tmp) / "shadow"
    path.write_text("root:!:1:0:99999:7:::\n")
    path.chmod(0o640)
    namespace["write_account_file"](str(path), ["root:!:1:0:99999:7:::", "damien:$6$x:1:0:99999:7:::"], 0o640)
    assert path.read_text() == pathlib.Path(tmp, "shadow-").read_text() == "root:!:1:0:99999:7:::\ndamien:$6$x:1:0:99999:7:::\n"
    assert path.stat().st_mode & 0o777 == 0o640
    assert sorted(p.name for p in pathlib.Path(tmp).iterdir()) == ["shadow", "shadow-"]
configure = installer[installer.index("    def configure_user("):installer.index("    @staticmethod\n    def write_winlux_conf")]
assert 'open(passwd_path, "a")' not in configure and 'open(shadow_path, "a")' not in configure
for name in ("passwd_path", "group_path", "shadow_path"):
    assert f"write_account_file({name}" in configure, name
print("installateur : comptes écrits par temporaire + fsync + rename, sauvegarde passwd-/shadow- comprise")

run = installer[installer.index("    def run_installation("):installer.index("    def extract_boot_files(")]
assert run.index("self.finalize_target()") < run.index("self.post_install()")
assert 'subprocess.run(["sync"], timeout=60)' not in run
final = installer[installer.index("    def finalize_target("):installer.index("    def release_target(")]
assert "unmount_verified" in final and "raise RuntimeError" in final
verify = installer[installer.index("    def unmount_verified("):installer.index("    def finalize_target(")]
assert "stop_processes_under" in verify and "remount,ro" in verify and "fsfreeze" in verify
post = installer[installer.index("    def post_install("):installer.index("    def show_done(")]
assert 'tr("progress_done")' in post
assert installer.count('"err_unmount":') >= 4
print("installateur : « terminée » seulement après sync et démontage vérifié (ESP comprise)")

# ── Init ─────────────────────────────────────────────────────────────────
build = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = build[build.index('cat >"$stage/init" <<'):]
assert init.index("/usr/bin/winlux-accounts-repair") < init.index("desktop_user=$(")
assert "/usr/bin/winlux-rescue-screen" in init
assert 'install -m 755 "$root/scripts/winlux-accounts-repair.py" "$stage/usr/bin/winlux-accounts-repair"' in build
assert 'install -m 755 "$root/scripts/winlux-rescue-screen.py" "$stage/usr/bin/winlux-rescue-screen"' in build
for lang in ("fr", "en", "de", "es"):
    out = subprocess.run([sys.executable, str(ROOT / "scripts/winlux-rescue-screen.py"), "--print"],
                         env={**os.environ, "LANG": lang}, capture_output=True, text=True, check=True).stdout
    assert "Winlux" in out and "S" in out, out
print("init : réparation avant le choix du compte, écran de secours en 4 langues au lieu d'un écran noir")
