# #1499, #1500, #1501 : finitions du code PIN et de l'empreinte (banc de la fiche 659).
lock_py = (SCRIPTS / "winlux-lock.py").read_text(encoding="utf-8")
pages_py = (SCRIPTS / "winlux-settings-pages.py").read_text(encoding="utf-8")
ok("self.method_hint" in lock_py and "lock-method-hint" in lock_py,
   "Écran verrouillé : « Code PIN » reste écrit au-dessus du champ au focus (#1499)")
ok("def watch_signin_reader" in pages_py,
   "Paramètres > Comptes : la carte empreinte suit le lecteur branché ou retiré (#1500)")
ok('create_pango_layout("\\n\\n")' in pages_py,
   "Configurer un code PIN : place réservée au message d’erreur, Valider ne bouge pas (#1501)")
