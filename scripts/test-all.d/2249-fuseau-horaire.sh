# #2249 : fuseau horaire inconnu tapé dans Heure et langue.
step "Heure et langue : fuseau horaire inconnu signalé (#2249)" xvfb-run -a -s "-screen 0 1366x768x24" /usr/bin/python3 -B scripts/test-fuseau-horaire.py
