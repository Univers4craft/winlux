# #3067 : le démon des alarmes tourne sans GTK et sonne à l'échéance exacte ;
# seule la fenêtre de sonnerie, processus à part, charge GTK.
step "Alarmes : démon sans GTK au repos, réveil à l'échéance (#3067)" /usr/bin/python3 -B scripts/test-alarms-demon-leger.py
