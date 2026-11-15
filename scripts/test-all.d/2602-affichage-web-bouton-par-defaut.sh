# #2602 : dialogue « Affichage web », « Plus tard » est le bouton par défaut
# visible (accent, focus et défaut dès l'ouverture).
if command -v xvfb-run >/dev/null 2>&1; then
    step "Affichage web : « Plus tard » bouton par défaut visible, sous Xvfb (#2602)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-webview-dialogue.py
else
    skip "Affichage web : « Plus tard » bouton par défaut visible, sous Xvfb (#2602)" "xvfb-run absent"
fi
