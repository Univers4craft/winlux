# #1357 : le processus GPU de Chromium peint les surgissants en direct ;
# la marge transparente est découpée par une forme X.
step "Forme des surgissants d'un autre processus (#1357)" cargo test -q -p winlux -- surgissant_etranger_decoupe_selon_l_alpha
if [ "$quick" != 1 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Forme X sur une fenêtre étrangère (#1357)" xvfb-run -a cargo test -q -p winlux -- --ignored forme_posee_sur_une_fenetre_etrangere_x11
else
    skip "Forme X sur une fenêtre étrangère (#1357)" "xvfb-run absent ou mode rapide"
fi
