# #2836 : l'icône posée par WM_SETICON (Form.Icon du témoin Icones) est
# publiée dans _NET_WM_ICON, sous Xvfb.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Fenêtres : icône WM_SETICON publiée dans _NET_WM_ICON, sous Xvfb (#2836)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-icone-fenetre.sh
else
    skip "Fenêtres : icône WM_SETICON publiée dans _NET_WM_ICON, sous Xvfb (#2836)" "mode rapide ou xvfb-run absent"
fi
