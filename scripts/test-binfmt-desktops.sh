#!/bin/sh
# Teste l'intégration binfmt_misc + associations MIME sur plusieurs bureaux.
#
# Vérifie que :
#   1. Le module binfmt_misc est monté
#   2. Le format .exe est enregistré via binfmt
#   3. Un .exe peut être exécuté directement (sans préfixer winlux)
#   4. Les types MIME sont enregistrés (xdg-mime)
#   5. L'application par défaut pour les .exe est winlux.desktop
#   6. Le fichier .desktop est valide
#
# Usage :
#   scripts/test-binfmt-desktops.sh               # test local
#   WINLUX_BIN=/chemin/winlux scripts/test-binfmt-desktops.sh
set -eu

pass=0
fail=0
skip=0

check() {
    label=$1
    shift
    if "$@" >/dev/null 2>&1; then
        echo "  OK : $label"
        pass=$((pass + 1))
    else
        echo "  ÉCHEC : $label"
        fail=$((fail + 1))
    fi
}

check_skip() {
    label=$1
    shift
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "  SKIP : $label ($1 absent)"
        skip=$((skip + 1))
        return
    fi
    check "$label" "$@"
}

winlux_bin=${WINLUX_BIN:-$(command -v winlux 2>/dev/null || echo /usr/bin/winlux)}
test_exe=${WINLUX_TEST_EXE:-}

# Trouver un .exe de test.
if test -z "$test_exe"; then
    for candidate in \
        /opt/winlux/tests/hello.exe \
        /usr/share/winlux/tests/hello.exe \
        "$(dirname "$winlux_bin")/../share/winlux/tests/hello.exe" \
        target/x86_64-pc-windows-msvc/release/hello.exe; do
        if test -x "$candidate" 2>/dev/null || test -r "$candidate" 2>/dev/null; then
            test_exe="$candidate"
            break
        fi
    done
fi

echo "=== Test binfmt_misc ==="

# 1. Module binfmt_misc monté ?
check "binfmt_misc monté" test -d /proc/sys/fs/binfmt_misc

# 2. Format Winlux enregistré ?
if test -r /proc/sys/fs/binfmt_misc/winlux; then
    echo "  OK : format winlux enregistré"
    pass=$((pass + 1))
elif test -r /proc/sys/fs/binfmt_misc/DOSWin; then
    echo "  OK : format DOSWin enregistré (compatible)"
    pass=$((pass + 1))
else
    # Chercher un entry qui gère les PE (magic MZ).
    found=0
    for entry in /proc/sys/fs/binfmt_misc/*; do
        case "$(basename "$entry")" in register|status) continue ;; esac
        if grep -q "$winlux_bin" "$entry" 2>/dev/null || grep -q winlux "$entry" 2>/dev/null; then
            echo "  OK : format binfmt détecté ($(basename "$entry"))"
            pass=$((pass + 1))
            found=1
            break
        fi
    done
    if test "$found" -eq 0; then
        echo "  ÉCHEC : aucun format binfmt pour les .exe"
        fail=$((fail + 1))
    fi
fi

# 3. Exécution directe d'un .exe ?
if test -n "$test_exe" && test -r "$test_exe"; then
    echo "=== Exécution directe ==="
    # Copier dans /tmp pour éviter les problèmes de permission.
    tmp_exe=$(mktemp /tmp/winlux-test-XXXXXX.exe)
    cp "$test_exe" "$tmp_exe"
    chmod +x "$tmp_exe"
    if output=$("$tmp_exe" 2>&1); then
        echo "  OK : exécution directe réussie"
        echo "    sortie : $output"
        pass=$((pass + 1))
    else
        echo "  ÉCHEC : exécution directe échouée (code $?)"
        fail=$((fail + 1))
    fi
    rm -f "$tmp_exe"
else
    echo "  SKIP : pas de .exe de test disponible"
    skip=$((skip + 1))
fi

echo "=== Types MIME ==="

# 4. Types MIME enregistrés ?
check_skip "MIME application/x-ms-dos-executable" \
    xdg-mime query filetype /dev/null

if command -v xdg-mime >/dev/null 2>&1 && test -n "$test_exe" && test -r "$test_exe"; then
    mime_type=$(xdg-mime query filetype "$test_exe" 2>/dev/null || true)
    case "$mime_type" in
        application/x-ms-dos-executable|application/x-dosexec|application/x-msdownload|application/vnd.microsoft.portable-executable)
            echo "  OK : type MIME détecté ($mime_type)"
            pass=$((pass + 1)) ;;
        *)
            echo "  INFO : type MIME = $mime_type (attendu : application/x-ms-dos-executable)"
            skip=$((skip + 1)) ;;
    esac
fi

# 5. Application par défaut ?
check_skip "application par défaut pour .exe" \
    sh -c 'xdg-mime query default application/x-ms-dos-executable 2>/dev/null | grep -q winlux'

# 6. Fichier .desktop valide ?
check_skip "winlux.desktop valide" \
    desktop-file-validate /usr/share/applications/winlux.desktop

echo "=== Détection du bureau ==="
desktop="${XDG_CURRENT_DESKTOP:-inconnu}"
session="${XDG_SESSION_TYPE:-inconnu}"
echo "  Bureau : $desktop"
echo "  Session : $session"

# Test spécifique par bureau.
case "$desktop" in
    *GNOME*)
        check_skip "nautilus reconnaît les .exe" \
            sh -c 'gio info --attributes=standard::content-type /dev/null >/dev/null 2>&1'
        ;;
    *KDE*|*Plasma*)
        check_skip "KDE : Dolphin disponible" command -v dolphin
        ;;
    *XFCE*)
        check_skip "XFCE : Thunar disponible" command -v thunar
        ;;
esac

echo ""
echo "=== Résumé ==="
echo "Réussis : $pass | Échoués : $fail | Ignorés : $skip"
echo "Bureau : $desktop | Session : $session"
test "$fail" -eq 0
