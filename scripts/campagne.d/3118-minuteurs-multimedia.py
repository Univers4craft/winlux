# Chaque timeSetEvent créait un fil Windows jamais libéré à la fin d'un
# minuteur à coup unique ; Qt en réarme ~40/s, Audacity perdait ~8 Mo/s dès son
# lancement (#3118). Le test unitaire nommé ici vérifie qu'un minuteur à coup
# unique ne laisse ni entrée ni fil, et que tous partagent un seul fil.
_aw = (ROOT / "runtime" / "src" / "dlls" / "apps_win32.rs").read_text(encoding="utf-8")
ok("fn minuteurs_a_coup_unique_ne_laissent_rien_3118" in _aw,
   "les minuteurs multimédia à coup unique ne laissent ni entrée ni fil (#3118)")
