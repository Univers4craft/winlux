# #2764 : WinForms mur 1, Application.Run ouvre une vraie Form (témoin FenetreBase,
# bâti par l'étape BCL C#), fermée comme par l'utilisateur, sous Xvfb.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "WinForms : Application.Run ouvre une vraie Form, fermée par l'utilisateur, sous Xvfb (#2764)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-fenetre-winforms.sh
else
    skip "WinForms : Application.Run ouvre une vraie Form, fermée par l'utilisateur, sous Xvfb (#2764)" "mode rapide ou xvfb-run absent"
fi
