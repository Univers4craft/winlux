# #2889 : winlux-open lance le programme sans attendre winlux-applications
# register et ne l'inspecte qu'une fois ; latence de la sonde Win32 mesurée
# sous Xvfb (premier pixel, touche, clic).
step "winlux-open : lancement sans attente, une seule inspection (#2889)" sh scripts/test-winlux-open-rapide.sh
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Latence : sonde Win32 x64/x86, premier pixel, touche et clic, sous Xvfb (#2889)" xvfb-run -a -s '-screen 0 1280x800x24' sh scripts/test-latence-sonde.sh
else
    skip "Latence : sonde Win32 x64/x86, premier pixel, touche et clic, sous Xvfb (#2889)" "mode rapide ou xvfb-run absent"
fi
