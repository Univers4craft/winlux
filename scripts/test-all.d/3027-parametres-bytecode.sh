# Gain #3027 : le bytecode des pages des Paramètres est gardé dans le cache
# du compte (installées dans /usr/bin, elles étaient recompilées à chaque
# ouverture) ; une page modifiée est relue.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Paramètres : bytecode des pages dans le cache du compte (#3027)" xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-settings-pycache.py
else
    skip "Paramètres : bytecode des pages dans le cache du compte (#3027)" "xvfb-run absent"
fi
