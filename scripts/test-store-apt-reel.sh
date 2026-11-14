#!/bin/sh
# Store : installe puis désinstalle réellement un paquet Ubuntu (lynx) avec
# winlux-store, comme le bouton du Store. Chaque appel à apt-get passe d'abord
# par la règle du relais winlux-root (`winlux-root policy`) : une commande que
# le relais refuserait (cas de « Désinstaller » sans effet en v0.2.17, option
# --autoremove refusée) fait échouer le test.
# Demande root et le réseau vers les dépôts Ubuntu (CI : sudo).
set -eu
project=$(cd "$(dirname "$0")/.." && pwd)
pkg=${WINLUX_TEST_PACKAGE:-lynx}
if test "$(id -u)" != 0; then
    echo "test-store-apt-reel : à lancer en root (sudo)" >&2
    exit 2
fi
if dpkg-query -W -f '${db:Status-Abbrev}' "$pkg" 2>/dev/null | grep -q '^ii'; then
    echo "test-store-apt-reel : $pkg est déjà installé, choisir un autre paquet (WINLUX_TEST_PACKAGE)" >&2
    exit 2
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/home" "$work/run"
real=$(command -v apt-get)
cat > "$work/bin/apt-get" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$work/apt.log"
# Simulation (-s, #2234) : la Boutique la lance sans droits, jamais par le relais.
if test "\$1" = -s; then exec "$real" "\$@"; fi
if ! python3 -I -B "$project/scripts/winlux-root.py" policy apt-get "\$@" >/dev/null; then
    echo "refusé par winlux-root : apt-get \$*" >> "$work/refused.log"
    exit 1
fi
exec "$real" "\$@"
EOF
chmod +x "$work/bin/apt-get"
printf 'lynx|navigateurs|Lynx|%s|Test|Paquet de test.\n' "$pkg" > "$work/catalog.txt"
store() {
    env PATH="$work/bin:$PATH" HOME="$work/home" WINLUX_STORE_RUN="$work/run" \
        WINLUX_STORE_CATALOG="$work/catalog.txt" WINLUX_STORE_OFFLINE=1 LANG=fr_FR.UTF-8 \
        python3 -I -B "$project/scripts/winlux-store.py" "$@"
}
fail() {
    echo "ÉCHEC : $1" >&2
    cat "$work/refused.log" "$work/run/store-progress.txt" 2>/dev/null >&2 || true
    tail -20 "$work/run/store.log" >&2 2>/dev/null || true
    exit 1
}
installed() { dpkg-query -W -f '${db:Status-Abbrev}' "$pkg" 2>/dev/null | grep -q '^ii'; }

store refresh || fail "index du Store"
grep -q "^app=apt:$pkg|" "$work/run/store.txt" || fail "$pkg absent de l'index"
store install "apt:$pkg" || fail "installation"
installed || fail "$pkg non installé"
grep -q "state=done" "$work/run/store-progress.txt" || fail "installation non signalée comme terminée"
grep -q "^app=apt:$pkg|[^|]*|[^|]*|[^|]*|[^|]*|i|" "$work/run/store.txt" || fail "index non mis à jour après installation"
store remove "apt:$pkg" || fail "désinstallation"
installed && fail "$pkg toujours installé après « Désinstaller »"
grep -q "state=done" "$work/run/store-progress.txt" || fail "désinstallation non signalée comme terminée"
grep -q "^app=apt:$pkg|[^|]*|[^|]*|[^|]*|[^|]*|a|" "$work/run/store.txt" || fail "index non mis à jour après désinstallation"
test ! -s "$work/refused.log" || fail "commande refusée par le relais"
grep -q -- "purge --autoremove -- $pkg" "$work/apt.log" || fail "désinstallation sans purge"
echo "OK : Store, $pkg réellement installé puis désinstallé (commandes acceptées par winlux-root)"
