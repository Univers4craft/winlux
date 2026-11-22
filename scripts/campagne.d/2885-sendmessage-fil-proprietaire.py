# SendMessage vers la fenêtre d'un autre fil : la procédure s'exécute sur le fil qui l'a créée (#2885).
_u = (ROOT / "runtime/src/dlls/user32.rs").read_text()
ok("fn send_message_execute_sur_le_fil_de_la_fenetre" in _u,
   "SendMessage inter-fils exécuté par le fil propriétaire de la fenêtre (#2885)")
