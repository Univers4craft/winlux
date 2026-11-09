#!/bin/sh
# Installation sur un Linux ordinaire (#27), sur l'hôte réel (root) :
# integration/install.sh, lancement direct de ./hello.exe par binfmt_misc,
# association par défaut des .exe, puis désinstallation sans reste.
# Chaque méthode d'enregistrement binfmt est essayée : systemd (si présent),
# binfmt-support (update-binfmts, si présent) et écriture directe dans
# /proc/sys/fs/binfmt_misc/register (systèmes sans systemd).
#
#   sudo sh scripts/test-host-install.sh
# Préfixe /usr/local : rien de /usr n'est remplacé sur la machine de test.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
testdir=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}
prefix=/usr/local
binfmt=/proc/sys/fs/binfmt_misc
fail() { echo "ÉCHEC : $*" >&2; exit 1; }
ok() { echo "  OK     $*"; }

test "$(id -u)" = 0 || fail "à lancer en root (sudo)"
test -x "$root/target/release/winlux" || fail "target/release/winlux absent (cargo build --release -p winlux)"
test -f "$testdir/hello.exe" || fail "hello.exe absent ($testdir)"
test ! -e "$binfmt/winlux" || fail "une entrée binfmt winlux existe déjà : machine non vierge"
test ! -e "$prefix/bin/winlux" || fail "$prefix/bin/winlux existe déjà : machine non vierge"

work=$(mktemp -d)
# Le runtime lancé en root crée /run/winlux (root, 0755) : les tests suivants
# du même runner, en compte ordinaire, ne pouvaient plus y créer leurs
# sockets de service. ./hello.exe est lancé par le compte qui a appelé sudo,
# et tout dossier d'état apparu pendant le test est retiré à la fin.
had_run=0; test -e /run/winlux && had_run=1
as_user=
if test -n "${SUDO_USER:-}" && test "$SUDO_USER" != root && command -v runuser >/dev/null 2>&1; then
    as_user="runuser -u $SUDO_USER --"
fi
mimeapps=/etc/xdg/mimeapps.list
had_mimeapps=0
if test -f "$mimeapps"; then had_mimeapps=1; cp -p "$mimeapps" "$work/mimeapps.avant"; fi
cleanup() {
    sh "$root/integration/uninstall.sh" "$prefix" >/dev/null 2>&1 || true
    if test "$had_mimeapps" = 1; then cp -p "$work/mimeapps.avant" "$mimeapps"; fi
    if test "$had_run" = 0; then rm -rf /run/winlux; fi
    rm -rf "$work"
}
trap cleanup EXIT

cp "$testdir/hello.exe" "$work/hello.exe"
chmod 755 "$work/hello.exe"
chmod 755 "$work"

methods=proc
if test -d /run/systemd/system && command -v systemctl >/dev/null 2>&1; then methods="systemd $methods"; fi
if command -v update-binfmts >/dev/null 2>&1; then methods="$methods update-binfmts"; fi
echo "Méthodes binfmt essayées : $methods${as_user:+ (.exe lancé par $SUDO_USER)}"

for method in $methods; do
    WINLUX_BINFMT=$method WINLUX_VERSION=test sh "$root/integration/install.sh" "$prefix" >"$work/install.log" 2>&1 \
        || { cat "$work/install.log" >&2; fail "$method : install.sh a échoué"; }
    test -r "$binfmt/winlux" || { cat "$work/install.log" >&2; fail "$method : $binfmt/winlux absent"; }
    grep -qx enabled "$binfmt/winlux" || fail "$method : entrée binfmt non activée"
    grep -qx "interpreter $prefix/bin/winlux" "$binfmt/winlux" || fail "$method : interpréteur incorrect ($(grep interpreter "$binfmt/winlux"))"
    grep -q '^flags: .*F' "$binfmt/winlux" || fail "$method : drapeau F (fix-binary) absent"
    ok "$method : binfmt enregistré et actif ($prefix/bin/winlux)"

    out=$(cd "$work" && $as_user ./hello.exe | head -n 1)
    case "$out" in *"Bonjour depuis un .exe Windows"*) ;; *) fail "$method : ./hello.exe direct : « $out »" ;; esac
    ok "$method : ./hello.exe lancé directement par le noyau"

    if command -v xdg-mime >/dev/null 2>&1; then
        default=$(XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" xdg-mime query default application/x-ms-dos-executable)
        test "$default" = winlux.desktop || fail "$method : application par défaut des .exe « $default »"
        ok "$method : xdg-mime rend winlux.desktop pour les .exe"
    else
        grep -q '^application/x-ms-dos-executable=winlux.desktop;' "$mimeapps" || fail "$method : association absente"
        ok "$method : association des .exe écrite (xdg-mime absent)"
    fi

    sh "$prefix/share/winlux/uninstall.sh" "$prefix" >"$work/uninstall.log" 2>&1 \
        || { cat "$work/uninstall.log" >&2; fail "$method : uninstall.sh a échoué"; }
    test ! -e "$binfmt/winlux" || fail "$method : entrée binfmt restée après désinstallation"
    if command -v update-binfmts >/dev/null 2>&1; then
        ! update-binfmts --display winlux 2>/dev/null | grep -q 'winlux' || fail "$method : binfmt-support garde winlux"
    fi
    for path in /etc/binfmt.d/winlux.conf "$prefix/bin/winlux" "$prefix/bin/winlux-open" \
            "$prefix/share/applications/winlux.desktop" "$prefix/share/mime/packages/winlux.xml" "$prefix/share/winlux"; do
        test ! -e "$path" || fail "$method : $path reste après désinstallation"
    done
    if test "$had_mimeapps" = 1; then
        cmp -s "$mimeapps" "$work/mimeapps.avant" || fail "$method : $mimeapps non restauré"
    else
        test ! -e "$mimeapps" || fail "$method : $mimeapps créé et laissé"
    fi
    if (cd "$work" && $as_user ./hello.exe >/dev/null 2>&1); then fail "$method : ./hello.exe se lance encore"; fi
    ok "$method : désinstallation propre (binfmt, fichiers, mimeapps.list)"
done
echo "OK: installation sur un Linux ordinaire ($methods)"
