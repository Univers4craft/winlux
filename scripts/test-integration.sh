#!/bin/sh
# Teste l'intégration bureau : installation/désinstallation dans une racine
# temporaire et comportement du lanceur de double-clic winlux-open.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
fail() { echo "ÉCHEC : $*" >&2; exit 1; }
ok() { echo "  OK     $*"; }

# --- --inspect -------------------------------------------------------------
apps="$work/Mes programmes"
mkdir -p "$apps"
cp "$testdir/hello.exe" "$apps/Bonjour monde.exe"
cp "$testdir/exitcode.exe" "$apps/code.exe"
cp "$testdir/hello.exe" "$apps/fenetre.exe"
# Marque la copie comme programme graphique (IMAGE_SUBSYSTEM_WINDOWS_GUI).
pe=$(od -An -tu4 -j60 -N4 "$apps/fenetre.exe" | tr -d ' ')
printf '\002' | dd of="$apps/fenetre.exe" bs=1 seek=$((pe + 24 + 68)) conv=notrunc 2>/dev/null
cp "$root/README.md" "$apps/faux.exe"

test "$("$runtime" --inspect "$apps/Bonjour monde.exe")" = console || fail "hello.exe devrait être console"
test "$("$runtime" --inspect "$apps/fenetre.exe")" = gui || fail "fenetre.exe devrait être gui"
if "$runtime" --inspect "$apps/faux.exe" 2>/dev/null; then fail "faux.exe accepté"; fi
if "$runtime" --inspect "$testdir/testdll.dll" 2>/dev/null; then fail "DLL acceptée"; fi
ok "--inspect console/gui/invalide/DLL"

# --- Installation dans une racine temporaire -------------------------------
dest="$work/racine"
mkdir -p "$dest/etc/xdg"
printf '[Default Applications]\napplication/x-msdownload=autre.desktop;\ntext/plain=kate.desktop;\n' >"$dest/etc/xdg/mimeapps.list"
cp "$dest/etc/xdg/mimeapps.list" "$work/mimeapps.avant"
DESTDIR="$dest" WINLUX_VERSION=test sh "$root/integration/install.sh" /usr >/dev/null
for f in usr/bin/winlux usr/bin/winlux-open usr/bin/winlux-update usr/share/applications/winlux.desktop \
         usr/share/mime/packages/winlux.xml usr/share/winlux/uninstall.sh etc/binfmt.d/winlux.conf; do
    test -e "$dest/$f" || fail "$f non installé"
done
grep -q '^Exec=/usr/bin/winlux-open %f$' "$dest/usr/share/applications/winlux.desktop" || fail "Exec incorrect"
grep -q '^Terminal=false$' "$dest/usr/share/applications/winlux.desktop" || fail "Terminal doit être false"
grep -q ':/usr/bin/winlux:F$' "$dest/etc/binfmt.d/winlux.conf" || fail "binfmt incorrect"
grep -q '^application/x-msdownload=winlux.desktop;autre.desktop;$' "$dest/etc/xdg/mimeapps.list" || fail "association existante non prioritaire"
grep -q '^application/x-ms-dos-executable=winlux.desktop;$' "$dest/etc/xdg/mimeapps.list" || fail "association absente"
test -x "$dest/usr/bin/winlux-browser" || fail "winlux-browser non installé"
grep -q '^Exec=/usr/bin/winlux-browser %U$' "$dest/usr/share/applications/winlux-browser.desktop" || fail "Exec du navigateur incorrect"
for type in x-scheme-handler/http x-scheme-handler/https text/html; do
    grep -q "^$type=winlux-browser.desktop;$" "$dest/etc/xdg/mimeapps.list" || fail "$type n'ouvre pas le navigateur par défaut"
done
DESTDIR="$dest" WINLUX_VERSION=test sh "$root/integration/install.sh" /usr >/dev/null
test "$(grep -c 'winlux.desktop' "$dest/etc/xdg/mimeapps.list")" -eq 6 || fail "réinstallation non idempotente"
test "$(grep -c 'winlux-browser.desktop' "$dest/etc/xdg/mimeapps.list")" -eq 4 || fail "réinstallation du navigateur non idempotente"
ok "installation DESTDIR et associations MIME"

DESTDIR="$dest" sh "$dest/usr/share/winlux/uninstall.sh" /usr >/dev/null
test ! -e "$dest/usr/bin/winlux" || fail "winlux non désinstallé"
test ! -e "$dest/etc/binfmt.d/winlux.conf" || fail "binfmt non désinstallé"
cmp -s "$dest/etc/xdg/mimeapps.list" "$work/mimeapps.avant" || fail "mimeapps.list non restauré"
ok "désinstallation et restauration de mimeapps.list"

# --- Navigateur par défaut : celui qui est installé, sans marque imposée ----
edgebin="$work/edge-bin"
mkdir -p "$edgebin"
printf '#!/bin/sh\nprintf "edge %%s\\n" "$@" >"%s/edge.log"\n' "$work" >"$edgebin/microsoft-edge-stable"
printf '#!/bin/sh\nprintf "firefox %%s\\n" "$@" >"%s/edge.log"\n' "$work" >"$edgebin/firefox"
chmod +x "$edgebin/microsoft-edge-stable" "$edgebin/firefox"
# Only fixture browsers may be discovered. Hosted runners can have Chrome,
# Chromium or Edge installed; launching one here is neither deterministic nor
# safe on a headless machine. Preserve the launcher's real exec/PATH behavior
# while excluding its absolute /opt fallback from this isolated fixture.
for tool in id dirname sed grep; do ln -s "$(command -v "$tool")" "$edgebin/$tool"; done
printf '#!/bin/sh\nexit 0\n' >"$edgebin/notify-send"
chmod +x "$edgebin/notify-send"
browser_launch() (
    export PATH="$edgebin"
    command() {
        test "$1" = -v || exit 99
        case "$2" in /*) return 1 ;; esac
        /bin/sh -c 'command -v "$1"' sh "$2"
    }
    . "$root/integration/winlux-browser"
)
browser_launch https://example.org
grep -q '^firefox https://example.org$' "$work/edge.log" || fail "winlux-browser n'ouvre pas le navigateur présent"
rm "$edgebin/firefox"
browser_launch https://example.com 2>/dev/null
grep -q '^edge https://example.com$' "$work/edge.log" || fail "pas de repli sur l'autre navigateur présent"
# Navigateur choisi dans Paramètres > Navigateurs : il passe en premier.
printf '#!/bin/sh\ncase "$1" in --status) printf "browser_default=chrome\\nbrowser_chrome=installed\\n" ;; --open) shift; printf "catalogue %%s\\n" "$@" >"%s/edge.log" ;; esac\n' "$work" >"$edgebin/winlux-browsers"
chmod +x "$edgebin/winlux-browsers"
browser_launch https://example.net
grep -q '^catalogue https://example.net$' "$work/edge.log" || fail "le navigateur choisi n'est pas utilisé"
rm "$edgebin/winlux-browsers"
# Aucune marque Microsoft dans l'entrée générique du navigateur.
! grep -qi 'microsoft\|edge' "$root/integration/winlux-browser.desktop" || fail "marque Microsoft dans winlux-browser.desktop"
for lang in en de es; do grep -q "^Name\[$lang\]=" "$root/integration/winlux-browser.desktop" || fail "Name[$lang] absent"; done
ok "winlux-browser ouvre le navigateur choisi ou présent, nom et icône neutres"

# --- Lanceur de double-clic ------------------------------------------------
fake="$work/bin"
mkdir -p "$fake"
cat >"$fake/kdialog" <<EOF
#!/bin/sh
printf '%s\n' "\$@" >>"$work/dialogue.log"
EOF
cat >"$fake/faux-terminal" <<EOF
#!/bin/sh
printf 'terminal\n' >>"$work/terminal.log"
test "\$1" = -e || exit 99
shift
exec "\$@" </dev/null
EOF
chmod +x "$fake/kdialog" "$fake/faux-terminal"
cp "$runtime" "$fake/winlux"
open="$root/integration/winlux-open"
launch() {
    env PATH="$fake:$PATH" DISPLAY=:99 XDG_STATE_HOME="$work/state" WINLUX_GUI=headless \
        WINLUX_TERMINAL=faux-terminal "$open" "$@" </dev/null >"$work/sortie.log" 2>&1
}

launch "$apps/Bonjour monde.exe" || fail "programme console : code $?"
grep -q 'terminal' "$work/terminal.log" || fail "aucun terminal ouvert pour un programme console"
grep -q 'terminé avec le code 0' "$work/sortie.log" || fail "résumé de fin absent"
ok "programme console dans un terminal (chemin avec espaces)"

# No terminal in this isolated PATH: the program must still be executed.
no_terminal="$work/no-terminal"
mkdir -p "$no_terminal"
for tool in dirname basename mkdir tail grep sed; do ln -s "$(command -v "$tool")" "$no_terminal/$tool"; done
cp "$fake/kdialog" "$no_terminal/kdialog"
env -u WINLUX_TERMINAL PATH="$no_terminal" DISPLAY=:99 XDG_STATE_HOME="$work/no-terminal-state" \
    WINLUX_RUNTIME="$fake/winlux" WINLUX_GUI=headless "$open" "$apps/Bonjour monde.exe" </dev/null || fail "programme refuse sans terminal"
test -s "$work/no-terminal-state/winlux/last-launch.log" || fail "sortie console perdue sans terminal"
ok "double-clic sans terminal et sans profil : execution et sortie conservees"

# A graphical desktop can inherit /dev/console: isatty(stdout) is insufficient.
python3 - "$open" "$apps/Bonjour monde.exe" "$fake/winlux" "$no_terminal" "$work/pty-state" <<'PY'
import os, pathlib, pty, subprocess, sys
launcher, exe, runtime, tools, state = sys.argv[1:]
env = os.environ.copy()
env.pop("WINLUX_TERMINAL", None)
env.pop("WINLUX_NO_TERMINAL", None)
env.update(PATH=tools, DISPLAY=":99", WINLUX_GUI="headless", WINLUX_RUNTIME=runtime,
           WINLUX_DESKTOP_LAUNCH="1", XDG_STATE_HOME=state)
master, slave = pty.openpty()
try:
    assert os.isatty(slave)
    result = subprocess.run([launcher, exe], env=env, stdin=subprocess.DEVNULL,
                            stdout=slave, stderr=slave, timeout=10)
    assert result.returncode == 0, result.returncode
    assert (pathlib.Path(state)/"winlux/last-launch.log").stat().st_size > 0
finally:
    os.close(master)
    os.close(slave)
PY
ok "console heritee du bureau : le double-clic conserve une sortie visible"

set +e
launch "$apps/code.exe"; status=$?
set -e
test "$status" -eq 42 || fail "code de sortie $status au lieu de 42"
ok "code de sortie conservé"

rm -f "$work/terminal.log"
launch "file://$(printf '%s' "$apps" | sed 's/ /%20/g')/fenetre.exe" || fail "programme graphique : code $?"
test ! -e "$work/terminal.log" || fail "un terminal a été ouvert pour un programme graphique"
ok "programme graphique lancé sans terminal (URI file://)"

set +e
launch "$apps/faux.exe"; status=$?
set -e
test "$status" -eq 1 || fail "faux.exe : code $status"
grep -q 'MZ' "$work/dialogue.log" || fail "aucune erreur graphique pour faux.exe"
launch "$apps/absent.exe" && fail "fichier absent accepté"
grep -q 'n’existe pas' "$work/dialogue.log" || fail "aucune erreur pour un fichier absent"
ok "erreurs affichées dans une boîte de dialogue"

# Un .exe graphique qui plante, lancé depuis l'Explorateur de la session
# native (ShellExecute : WINLUX_DESKTOP_LAUNCH=1) : la boîte d'erreur doit
# s'afficher. Seul le bureau PE (WINLUX_DESKTOP_DEMO), qui montre l'écran
# bleu, la remplace. Avant, la fenêtre ne s'ouvrait pas et rien ne le disait.
crashbin="$work/crash-bin"
journal="$work/journal"
mkdir -p "$crashbin" "$journal"
cat >"$crashbin/winlux" <<EOF
#!/bin/sh
if [ "\$1" = --inspect ]; then echo gui; exit 0; fi
printf '[CRASH] ts=%s sig=11 stop=IRQL_NOT_LESS_OR_EQUAL exe=plante.exe pid=%s\n' "\$(date +%s)" "\$\$" >>"$journal/crash.log"
exit 5
EOF
chmod +x "$crashbin/winlux"
cp "$apps/fenetre.exe" "$apps/plante.exe"
crash_launch() {
    : >"$work/dialogue.log"
    env PATH="$fake:$PATH" DISPLAY=:99 XDG_STATE_HOME="$work/state" WINLUX_RUNTIME="$crashbin/winlux" \
        WINLUX_JOURNAL_DIR="$journal" WINLUX_DESKTOP_LAUNCH=1 "$@" "$open" "$apps/plante.exe" \
        </dev/null >"$work/sortie.log" 2>&1
}
set +e
crash_launch WINLUX_NATIVE_SHELL=1; status=$?
set -e
test "$status" -eq 5 || fail "plantage : code $status au lieu de 5"
grep -q 'plante.exe' "$work/dialogue.log" || fail "plantage depuis l'Explorateur natif : aucune boîte"
set +e
crash_launch WINLUX_DESKTOP_DEMO=1
set -e
test ! -s "$work/dialogue.log" || fail "bureau PE : boîte en double avec l'écran bleu"
ok "plantage d'un .exe graphique ouvert depuis l'Explorateur : message affiché"

# Installateur 32 bits que le runtime déroule lui-même (WiX Burn comme
# vc_redist.x64.exe, NSIS, Inno Setup) : --inspect répond « setup » et le
# double-clic l'installe au lieu de refuser un programme 32 bits.
setupbin="$work/setup-bin"
mkdir -p "$setupbin"
cat >"$setupbin/winlux" <<EOF
#!/bin/sh
if [ "\$1" = --inspect ]; then echo setup; exit 0; fi
echo "\$@" >"$work/setup-args.log"
exit 0
EOF
chmod +x "$setupbin/winlux"
cp "$apps/fenetre.exe" "$apps/vc_redist.x64.exe"
: >"$work/dialogue.log"
env PATH="$fake:$PATH" DISPLAY=:99 XDG_STATE_HOME="$work/state" WINLUX_RUNTIME="$setupbin/winlux" \
    "$open" "$apps/vc_redist.x64.exe" </dev/null >"$work/sortie.log" 2>&1 || fail "installateur : échec"
grep -q 'vc_redist.x64.exe' "$work/setup-args.log" || fail "installateur : runtime non appelé"
grep -q 'est installé' "$work/dialogue.log" || fail "installateur : pas de message de fin"
if grep -q '32 bits' "$work/dialogue.log"; then fail "installateur refusé comme programme 32 bits"; fi
ok "installateur 32 bits (vc_redist) : installé par double-clic"

echo "Intégration bureau validée."
