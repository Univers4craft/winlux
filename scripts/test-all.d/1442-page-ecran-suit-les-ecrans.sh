# #1442 : Paramètres > Écran suit un écran débranché sans rechargement.
step "Page Écran à jour après débranchement (#1442)" xvfb-run -a -s "-screen 0 1366x768x24" /usr/bin/python3 -B scripts/test-display-page-follow.py
