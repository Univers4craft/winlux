# #2243 : le drapeau /run/winlux/browser-chooser était commun aux comptes ;
# le premier qui l'écrivait bloquait les autres, et seule la session principale l'ouvrait.
nav = (SCRIPTS / "winlux-browsers.sh").read_text(encoding="utf-8")
ok('chooser="$run/browser-chooser-$(id -u)"' in nav and '> "$run/browser-chooser"' not in nav,
   "winlux-browsers : demande du choix du navigateur propre au compte (#2243)")
ses = (SCRIPTS / "winlux-user-session.sh").read_text(encoding="utf-8")
ok('test -O "/run/winlux/browser-chooser-$uid"' in ses and 'test "$primary" = 1 && test -O /run/winlux/browser-chooser' not in ses,
   "session de chaque compte, principale ou non : ouvre son propre choix du navigateur (#2243)")
