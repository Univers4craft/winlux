# #2992 : contrôle enfant en hauteur 0 sous un vrai serveur X.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Contrôle enfant en hauteur 0 : zone client vide, aucun WM_PAINT, sous X (#2992)" xvfb-run -a -s '-screen 0 1280x800x24' sh scripts/test-libelle-vide.sh
else
    skip "Contrôle enfant en hauteur 0 : zone client vide, aucun WM_PAINT, sous X (#2992)" "xvfb-run absent"
fi
