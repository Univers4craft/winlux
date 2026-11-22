# Les lanceurs winlux-lnk-*.desktop du Bureau sont cachés et système pour les programmes Windows (#3000).
_w = (ROOT / "runtime/src/dlls/winfile.rs").read_text()
ok("fn lanceur_de_session_cache_et_systeme" in _w,
   "l'Explorateur ne montre pas les lanceurs winlux-lnk du Bureau (#3000)")
