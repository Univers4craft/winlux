#!/bin/sh
# Lance d'un coup les vérifications rapides de Winlux, sans droits root, sans
# machine virtuelle et sans compiler le runtime : les mêmes que la CI fait en
# Python ou en shell, plus le site web.
#
#   sh scripts/test-all.sh            tout ce qui suit
#   sh scripts/test-all.sh site       seulement les tests dont le nom contient « site »
#
# Les tests longs ou qui demandent du matériel restent à part : cargo test,
# scripts/test-runtime.sh (PE de test), test-*-vm.sh, test-live-iso-qemu.sh.
# Variables : WINLUX_SITE_RENDER=required rend obligatoire le rendu du site
# dans Chromium (sinon il est sauté quand Node.js ou Playwright manque).
set -u

cd "$(dirname "$0")/.." || exit 1
filter=${1:-}
passed=0
failed=0
failures=""

run() {
    name=$1
    shift
    case "$name" in
        *"$filter"*) ;;
        *) return 0 ;;
    esac
    printf '%-32s ' "$name"
    log=$(mktemp)
    start=$(date +%s)
    if "$@" >"$log" 2>&1; then
        passed=$((passed + 1))
        echo "OK ($(( $(date +%s) - start )) s)"
    else
        failed=$((failed + 1))
        failures="$failures $name"
        echo "ÉCHEC"
        sed 's/^/    /' "$log" | tail -n 40
    fi
    rm -f "$log"
}

run site                 python3 -B scripts/test-site.py --render
run wiki-liens           python3 -B - <<'EOF'
import pathlib, re, sys
pages = {p.stem for p in pathlib.Path("wiki").glob("*.md")}
bad = 0
for page in sorted(pathlib.Path("wiki").glob("*.md")):
    text = re.sub(r"`[^`]*`", "", page.read_text(encoding="utf-8"))
    for target in re.findall(r"\]\(([A-Za-z0-9_-]+)(?:#[^)]*)?\)", text):
        if target not in pages:
            print(f"{page}: lien vers une page absente : {target}")
            bad = 1
sys.exit(bad)
EOF
run catalogue-natif      python3 -B scripts/test-native-catalogue.py
run installateur-oobe    python3 -B scripts/test-installer-oobe.py
run installation-auto    python3 -B scripts/test-autoinstall.py
run partitionnement      python3 -B scripts/test-installer-partitioning.py
run signalement          python3 -B scripts/test-report.py
run verrouillage         python3 -B scripts/test-lock.py
run autorisations        python3 -B scripts/test-permission.py
run uac                  python3 -B scripts/test-uac.py
run relais-root          python3 -B scripts/test-root.py
run coller-bureau        python3 -B scripts/test-desktop-paste.py
run modes-alimentation   python3 -B scripts/test-power-modes.py
run redemarrage-garde    python3 -B scripts/test-power-reboot-guard.py
run claude-code          python3 -B scripts/test-claude-code.py
run reinitialisation    python3 -B scripts/test-reset.py
run assistant            python3 -B scripts/test-assistant.py
run eclairage-nocturne   python3 -B scripts/test-ambiance.py
run emojis               python3 -B scripts/test-emoji.py
run presse-papiers       python3 -B scripts/test-clipboard.py
run ancrage-vue-taches   python3 -B scripts/test-snap-taskview.py
run horloge              python3 -B scripts/test-alarms.py
run petits-outils        python3 -B scripts/test-small-tools.py
run copier-texte-image   python3 -B scripts/test-ocr.py
run theme-automatique    python3 -B scripts/test-appearance-auto.py
run createur-cle-usb     python3 -B installation/tests/test_winlux_usb.py

# Lisibilité du texte dans chaque thème : il faut un affichage (xvfb-run sinon).
display() {
    if [ -n "${DISPLAY:-}" ]; then "$@"
    elif command -v xvfb-run >/dev/null 2>&1; then xvfb-run -a -s '-screen 0 1920x1080x24' "$@"
    else echo "ni DISPLAY ni xvfb-run : test sauté"; fi
}
run contraste-gtk        display python3 -B scripts/test-text-contrast.py
# Shell PE : seulement si le runtime et gui_demo.exe sont déjà compilés.
if [ -x target/release/winlux ] && [ -f target/x86_64-pc-windows-msvc/release/gui_demo.exe ]; then
    run contraste-shell-pe   display python3 -B scripts/test-pe-text-contrast.py --seconds 5
fi

echo
echo "$passed réussi(s), $failed échec(s)."
if [ "$failed" -ne 0 ]; then
    echo "En échec :$failures"
    exit 1
fi
