# DirectSound de PortAudio (Audacity) n'avance que par la routine de complétion
# de SetWaitableTimer ; ignorée, lecture et enregistrement restaient muets
# (#3117). Le test unitaire nommé ici vérifie le comportement : appel sur le fil
# qui a armé le minuteur, en attente alertable seulement, sans accumulation.
_k32 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
ok("fn minuteur_appelle_sa_routine_en_attente_alertable_3117" in _k32,
   "SetWaitableTimer appelle sa routine de complétion en attente alertable (#3117)")
