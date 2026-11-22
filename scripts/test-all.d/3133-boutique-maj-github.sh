# #3133 : le Store se met à jour depuis GitHub, dans le compte et sans mot
# de passe. Pose acceptée ; refus d'une signature fausse, d'un SHA-256 faux
# et d'un numéro ancien ; hors ligne ; retours en arrière ; root ignoré ;
# coût du lanceur.
step "Store : mise à jour signée depuis GitHub (#3133)" python3 -B scripts/test-boutique-maj.py
if command -v xvfb-run >/dev/null 2>&1; then
    step "Paramètres : mises à jour du Store visibles (#3133)" xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-settings-maj-store.py
else
    skip "Paramètres : mises à jour du Store visibles (#3133)" "xvfb-run absent"
fi
