# GetTempFileNameW ne garde que les trois premiers caractères du préfixe,
# sans refuser un préfixe plus long (Learn, #2988).
_k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("prefix.chars().take(3)" in _k and "fn prefixe_de_fichier_temporaire_tronque_2988" in _k,
   "GetTempFileNameW tronque le préfixe à trois caractères au lieu de le refuser (#2988)")
