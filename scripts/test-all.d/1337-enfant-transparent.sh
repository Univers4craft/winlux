# #1337 : un enfant WS_EX_TRANSPARENT (« Chrome Legacy Window ») ne masque
# plus l'image présentée dans son parent par le processus GPU de Chromium.
step "Enfant transparent sans masque (#1337)" cargo test -q -p winlux -- enfant_transparent_laisse_voir_le_parent
if [ "$quick" != 1 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Enfant transparent en fenêtre X InputOnly (#1337)" xvfb-run -a cargo test -q -p winlux -- --ignored enfant_transparent_en_inputonly_x11
else
    skip "Enfant transparent en fenêtre X InputOnly (#1337)" "xvfb-run absent ou mode rapide"
fi
