# #2893 : la fenêtre de l'assistant d'installation garde sa largeur quand une
# ligne d'état est très longue (vraie fenêtre GTK sous Xvfb).
if command -v xvfb-run >/dev/null 2>&1 && command -v xwininfo >/dev/null 2>&1; then
    step "Assistant : largeur fixe malgré une longue ligne d'état, sous Xvfb (#2893)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-setup-wizard-largeur.py
else
    skip "Assistant : largeur fixe malgré une longue ligne d'état, sous Xvfb (#2893)" "xvfb-run ou xwininfo absent"
fi
