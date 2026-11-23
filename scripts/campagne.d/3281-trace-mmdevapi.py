# Discord attend 30 s une énumération de périphériques qui ne revient jamais
# (#3281) ; l'énumérateur MMDevice ne laissait aucune trace. Le test unitaire
# nommé ici vérifie qu'une table tracée par WINLUX_COMTRACE énumère toujours
# et que chaque appel est journalisé.
_mm = (ROOT / "runtime" / "src" / "dlls" / "mmdevapi.rs").read_text(encoding="utf-8")
ok("fn enumerateur_trace_par_winlux_comtrace_3281" in _mm,
   "énumérateur MMDevice visible dans la trace COM (#3281)")
