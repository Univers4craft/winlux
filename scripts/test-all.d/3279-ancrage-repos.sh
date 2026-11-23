# #3279 : l'ancrage (winlux-snap watch) ne se réveille plus souris immobile ;
# il suit les événements XInput2 bruts de la souris, repli sur 100 ms sans eux.
if [ "$quick" != 1 ] && command -v xvfb-run >/dev/null 2>&1 && command -v openbox >/dev/null 2>&1; then
    step "Ancrage au repos : aucun réveil souris immobile (#3279)" xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-ancrage-repos.py
else
    skip "Ancrage au repos : aucun réveil souris immobile (#3279)" "xvfb-run ou openbox absent, ou mode rapide"
fi
