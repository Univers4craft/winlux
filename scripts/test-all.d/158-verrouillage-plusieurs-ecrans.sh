# #158 : écran branché ou débranché pendant le verrouillage, chaque écran couvert.
step "Écran verrouillé sur plusieurs écrans (#158)" xvfb-run -a -s "-screen 0 2732x768x24" /usr/bin/python3 -B scripts/test-lock-monitors.py
