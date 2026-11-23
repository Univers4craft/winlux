#!/bin/sh
# Lancement d'un installateur par un compte administrateur (#3172) :
# l'élévation est accordée sans question (#1739), donc rien de
# l'installateur n'est lu avant son démarrage. testc/msvc/uacbig_msvc.c
# (manifeste requireAdministrator) reçoit 256 Mo de données à la fin, comme
# un vrai installateur ; le runtime, lancé par un compte administrateur non
# élevé (membre de sudo/wheel/admin/adm, comme le compte de damien ; root
# est toujours élevé et ne passe pas par la règle) sans réponse forcée ni
# écran, doit le démarrer (code 42) avec un pic de mémoire
# de moins de 128 Mo et en moins de 2 s, détection des installateurs
# écartée (WINLUX_SETUP_NATIVE=1). Avant le correctif, tout le
# fichier était lu (pic de plus de 256 Mo) pour une ligne de journal.
# Sauté (code 0) sans runtime, sans clang, ou sans compte administrateur à
# emprunter.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
src="$root/testc/msvc/uacbig_msvc.c"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
# Un membre d'un groupe administrateur (lu par security::admin_capable).
admin=$(awk -F: '$1 ~ /^(sudo|wheel|admin|adm)$/ && $4 != "" { split($4, m, ","); print m[1]; exit }' /etc/group)
if [ "$(id -u)" -eq 0 ]; then
    command -v setpriv >/dev/null || { echo "root sans setpriv : test sauté"; exit 0; }
    test -n "$admin" || { echo "aucun compte administrateur dans /etc/group : test sauté"; exit 0; }
    # Processus non root (non élevé) au nom d'un administrateur.
    as_admin="setpriv --reuid=65534 --regid=65534 --clear-groups"
else
    admin=$(id -un)
    id -Gn | tr ' ' '\n' | grep -qxE 'sudo|wheel|admin|adm' || { echo "compte non administrateur : test sauté"; exit 0; }
    as_admin=
fi
for d in clang lld-link llvm-dlltool python3; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\n' >"$work/kernel32.def"
llvm-dlltool -m i386:x86-64 -d "$work/kernel32.def" -l "$work/kernel32.lib"
clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -c "$src" -o "$work/uacbig.obj"
lld-link /nologo /entry:entry /subsystem:console /nodefaultlib /manifest:embed \
    "/manifestuac:level='requireAdministrator' uiAccess='false'" \
    "$work/uacbig.obj" "$work/kernel32.lib" /out:"$work/Setup.exe"
grep -q requireAdministrator "$work/Setup.exe" || { echo "manifeste requireAdministrator absent du PE" >&2; exit 1; }
# Données d'installateur : de vrais octets (pas un fichier creux).
python3 -I -c 'import sys
with open(sys.argv[1], "ab") as f:
    block = bytes(range(256)) * 4096
    for _ in range(256): f.write(block)' "$work/Setup.exe"
chmod 755 "$work"
chmod 644 "$work/Setup.exe"
mkdir -p "$work/c"
chmod 777 "$work/c"
python3 -I - "$runtime" "$work/Setup.exe" "$work/c" "$admin" $as_admin <<'PY'
import os, resource, subprocess, sys, time
runtime, exe, drive, admin = sys.argv[1:5]
prefix = sys.argv[5:]
env = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY", "WINLUX_UAC")}
# WINLUX_SETUP_NATIVE : la détection des installateurs (NSIS, Inno, 7z…)
# parcourt aussi le fichier, mais dans sa projection (pages du cache, pas
# de copie) ; elle est mesurée à part. Ici, seule la règle UAC compte.
env.update(HOME=drive, WINLUX_HOME=drive, WINLUX_DRIVE_C=drive, WINLUX_GUI="headless",
           WINLUX_SETUP_NATIVE="1", USER=admin, LOGNAME=admin)
env.pop("WINLUX_ELEVATED", None)
size = os.path.getsize(exe) >> 20
start = time.monotonic()
rc = subprocess.run(prefix + [runtime, exe], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=60)
elapsed = time.monotonic() - start
peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss >> 10
print(f"installateur de {size} Mo, administrateur {admin} non élevé : code {rc.returncode}, pic {peak} Mo, {elapsed:.2f} s")
if rc.returncode != 42:
    print(rc.stderr.decode(errors="replace")[-2000:], file=sys.stderr)
    sys.exit("l'installateur n'a pas démarré")
if peak >= 128:
    sys.exit(f"pic de {peak} Mo : l'installateur a été lu avant son lancement")
if elapsed >= 2:
    sys.exit(f"{elapsed:.2f} s avant la fin d'un programme qui sort aussitôt")
print("OK : installateur lancé sans être lu")
PY
