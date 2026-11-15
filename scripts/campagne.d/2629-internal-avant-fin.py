# Internal et InternalHigh d'un OVERLAPPED sont écrits avant que l'opération
# soit vue terminée par GetOverlappedResult (#2629) : un test le vérifie.
_s = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn internal_ecrit_avant_la_fin_2629" in _s,
   "test Internal écrit avant la fin de l'opération (#2629)")
ok("fn signal_overlapped" in _s,
   "signalement séparé de l'écriture d'Internal (#2629)")
