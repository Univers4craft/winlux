# #2753 : MessageBox.Show de WinForms servi par le moteur .NET maison ; vraie
# boîte sous Xvfb, Entrée rendu à l'appli (témoin BoiteMessage, bâti par l'étape BCL C#).
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "WinForms : MessageBox.Show affiche la boîte et rend le bouton choisi, sous Xvfb (#2753)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-boite-message-winforms.sh
else
    skip "WinForms : MessageBox.Show affiche la boîte et rend le bouton choisi, sous Xvfb (#2753)" "mode rapide ou xvfb-run absent"
fi
