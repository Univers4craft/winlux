# #3079 : fond « Winlux Aurore » animé, coupé en plein écran, fenêtre agrandie,
# économiseur d'écran, batterie faible, autre fond ou sans compositeur.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Fond Aurore animé coupé dès qu'on ne le voit pas, sous Xvfb (#3079)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-fond-anime.py
else
    skip "Fond Aurore animé coupé dès qu'on ne le voit pas, sous Xvfb (#3079)" "mode rapide ou xvfb-run absent"
fi
