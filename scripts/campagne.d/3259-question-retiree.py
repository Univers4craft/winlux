# #3259 : la question « … ne répond pas » survivait à la mort du programme, et
# son identifiant X pouvait être réattribué entre-temps. Elle surveille
# désormais le processus visé (PID et date de départ) et sa fenêtre.
fc = (ROOT / "scripts/winlux-force-close.py").read_text(encoding="utf-8")
tf = (ROOT / "scripts/test-force-close.py").read_text(encoding="utf-8")
ok("def reason_to_withdraw" in fc and "WITHDRAWN" in fc,
   "la question « ne répond pas » se retire quand le programme ou sa fenêtre disparaît (#3259)")
ok("def withdrawal_checks" in tf and "XID réattribué" in tf,
   "test-force-close vérifie le retrait de la question, programme mort et XID réattribué (#3259)")
