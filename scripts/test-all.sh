#!/bin/sh
# Suite complète de tests locaux en une seule commande :
#   sh scripts/test-all.sh            tout ce que la CI lance sur l'hôte
#   sh scripts/test-all.sh --rapide   sans compilation ni tests PE (quelques minutes)
#   sh scripts/test-all.sh site       seulement les étapes dont le nom contient « site »
#                                     (filtre combinable avec --rapide)
#
# Les étapes sont lues dans .github/workflows/ci.yml : une vérification ajoutée
# à la CI est aussi lancée ici, sans liste à tenir à jour. Les étapes qui
# demandent sudo ou un outil absent (xvfb-run, mingw) sont sautées et listées
# à la fin. Les tests en VM (ISO complète) restent ceux de release.yml.
set -u
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root" || exit 1
quick=0 filter=
for argument in "$@"; do
    case "$argument" in --rapide) quick=1 ;; *) filter=$argument ;; esac
done
logs=$(mktemp -d "${TMPDIR:-/tmp}/winlux-tests.XXXXXX")
passed=0 failed=0 skipped=0
failures= skips=

step() {
    name=$1; shift
    case "$name $*" in *"$filter"*) ;; *) return 0 ;; esac
    log="$logs/$(printf '%s' "$name" | tr -c 'A-Za-z0-9._-' '_').log"
    printf '%-60s' "$name"
    start=$(date +%s)
    if "$@" >"$log" 2>&1; then
        passed=$((passed + 1)); printf ' OK (%ss)\n' "$(( $(date +%s) - start ))"
    else
        failed=$((failed + 1)); failures="$failures
  - $name (journal : $log)"
        printf ' ÉCHEC\n'; tail -n 15 "$log" | sed 's/^/      /'
    fi
}
skip() { case "$1" in *"$filter"*) ;; *) return 0 ;; esac; skipped=$((skipped + 1)); skips="$skips
  - $1 ($2)"; }

if test "$quick" = 0 && test -z "$filter"; then
    step "Compilation du runtime" cargo build --release -p winlux
    step "Tests unitaires Rust (cargo test)" cargo test -p winlux
    if command -v cargo >/dev/null && test -f scripts/build-tests.sh; then
        step "Programmes Windows de test (build-tests)" bash scripts/build-tests.sh
    fi
fi

# Noms courts acceptés par le filtre (« contraste », « site »…) : le filtre
# porte sur le nom de l'étape et sa commande.
# Commandes « scripts/test-… » de la CI (ci.yml, et les tests d'hôte de
# release.yml), une par ligne ; la boucle des fenêtres surgissantes est dépliée.
grep -hE '^\s*(run: )?(sudo |xvfb-run [^/]*)?(/usr/bin/)?(python3|sh|bash|dbus-run-session)[^#]*(scripts/test-|installation/tests/test)[A-Za-z0-9_.-]+' \
    .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/installation.yml |
    sed -E 's/^\s*(run: )?//' | awk '!seen[$0]++' |
    while IFS= read -r line; do
        case "$line" in
            *'"$popup"'*) for popup in start volume power calendar; do printf '%s\n' "$line" | sed "s/\"\$popup\"/$popup/"; done ;;
            *) printf '%s\n' "$line" ;;
        esac
    done >"$logs/steps.txt"
while IFS= read -r command; do
    script=$(printf '%s\n' "$command" | grep -oE '(scripts/test-|installation/tests/test)[A-Za-z0-9_-]+\.(py|sh)( [a-z]+$)?' | tail -n 1)
    case "$command" in
        sudo*) skip "$script" "demande sudo"; continue ;;
        *-vm.sh*|*qemu*|*vm-guest*) skip "$script" "VM et ISO complète (release.yml)"; continue ;;
        *xvfb-run*) command -v xvfb-run >/dev/null 2>&1 || { skip "$script" "xvfb-run absent"; continue; } ;;
    esac
    case "$script" in
        scripts/test-runtime.sh|scripts/test-runtime-parallel.sh|scripts/test-integration.sh|scripts/test-mingw.sh)
            if test "$quick" = 1; then skip "$script" "mode rapide"; continue; fi
            test -d target/x86_64-pc-windows-msvc/release || { skip "$script" "programmes PE non compilés"; continue; } ;;
    esac
    if test "$script" = scripts/test-mingw.sh; then
        command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || { skip "$script" "MinGW-w64 absent"; continue; }
        step "scripts/build-mingw-tests.sh" sh scripts/build-mingw-tests.sh
    fi
    step "$script" sh -c "$command"
done <"$logs/steps.txt"

# Fautes matérielles en exceptions SEH et suspension de threads (#269) :
# lancés en CI par test-mingw.sh ;
# ici aussi sans MinGW, dès que clang et le runtime compilé sont là.
if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 && command -v clang >/dev/null 2>&1 && test -x target/release/winlux; then
    step "scripts/test-hw-exceptions.sh" sh scripts/test-hw-exceptions.sh
    step "scripts/test-thread-control.sh" sh scripts/test-thread-control.sh
fi

# Accent violet Prisme par défaut (#343), pas encore dans ci.yml.
step "scripts/test-avahi-printers.py" python3 -B scripts/test-avahi-printers.py
step "scripts/test-control-status-cost.py" python3 -B scripts/test-control-status-cost.py

# Applis WinForms et WPF .NET autonomes (#664–#666, #268) : télécharge une fois le runtime
# .NET 9 de Paint.NET portable (132 Mo), donc pas en mode rapide.
if test "$quick" = 0; then
    step "scripts/test-winforms-bonjour.sh" sh scripts/test-winforms-bonjour.sh
    step "scripts/test-wpf-bonjour.sh" sh scripts/test-wpf-bonjour.sh
fi

# Site web et wiki (site.yml) : rendu Chromium sauté sans Node.js ni
# Playwright, sauf WINLUX_SITE_RENDER=required.
step "scripts/test-site.py (site web)" python3 -B scripts/test-site.py --render
step "wiki : liens entre pages" python3 -B -c '
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
'

total=$((passed + failed))
echo
echo "Résultat : $passed réussis, $failed échoués sur $total, $skipped sautés."
test -z "$skips" || printf 'Sautés :%s\n' "$skips"
test -z "$failures" || printf 'Échecs :%s\n' "$failures"
test "$failed" = 0
