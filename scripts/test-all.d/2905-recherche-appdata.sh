# #2905 : la recherche du menu Démarrer n'affiche ni chemin AppData ni marque
# Microsoft (groupes du menu exclus, dossiers des programmes nommés), sous Xvfb.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Menu Démarrer : recherche sans chemin AppData ni marque Microsoft, sous Xvfb (#2905)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-start-search-appdata.py
else
    skip "Menu Démarrer : recherche sans chemin AppData ni marque Microsoft, sous Xvfb (#2905)" "mode rapide ou xvfb-run absent"
fi
