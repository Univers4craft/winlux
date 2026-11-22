# #3021 : les pages Imprimantes et scanners et Partage et transfert des
# Paramètres s'affichent sans attendre lpstat ni winlux-control.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Paramètres : état lent lu hors du fil GTK (#3021)" xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-settings-etat-lent.py
else
    skip "Paramètres : état lent lu hors du fil GTK (#3021)" "xvfb-run absent"
fi
