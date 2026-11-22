# #2895 : une étape sautée (programme que Winlux ne lance pas) reste hors de
# la ligne d'état de l'assistant, est résumée en clair en fin d'installation
# et gardée entière au journal (vraie fenêtre GTK sous Xvfb).
if command -v xvfb-run >/dev/null 2>&1 && command -v xwininfo >/dev/null 2>&1; then
    step "Assistant : étapes sautées résumées en fin, hors de la ligne d'état, sous Xvfb (#2895)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-setup-wizard-etapes-sautees.py
else
    skip "Assistant : étapes sautées résumées en fin, hors de la ligne d'état, sous Xvfb (#2895)" "xvfb-run ou xwininfo absent"
fi
