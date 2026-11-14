# #2351 : application bloquée avant la mise à jour, affichée sous le nom du menu.
step "Contrôle parental : ancien nom d'une application bloquée remplacé (#2351)" xvfb-run -a -s "-screen 0 1024x768x24" /usr/bin/python3 -B scripts/test-parental-ancien-nom.py
