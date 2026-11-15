# #2605 : confirmation « Installer … ? » de la Boutique, « Annuler » est le
# bouton actif visible (accent, focus, défaut et cadre de focus dès l'ouverture).
if command -v xvfb-run >/dev/null 2>&1; then
    step "Boutique : « Annuler » bouton actif visible de la confirmation, sous Xvfb (#2605)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-store-confirm-dialogue.py
else
    skip "Boutique : « Annuler » bouton actif visible de la confirmation, sous Xvfb (#2605)" "xvfb-run absent"
fi
