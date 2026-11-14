# #2350 : nom tapé dans « Choisir une application » du contrôle parental.
step "Contrôle parental : nom tapé inconnu refusé ou corrigé (#2350)" xvfb-run -a -s "-screen 0 1024x768x24" /usr/bin/python3 -B scripts/test-parental-saisie.py
