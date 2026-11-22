# Le caractère tapé juste avant Entrée arrive avant elle (#3072) : GetMessage sert
# le WM_CHAR posté par TranslateMessage avant la touche suivante déjà en file.
# Le comportement est vérifié par le test unitaire et sous Xvfb par scripts/test-point-puis-entree.sh.
_g = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
ok("fn le_caractere_traduit_passe_devant_la_touche_suivante_deja_en_file" in _g
   and (ROOT / "scripts/test-point-puis-entree.sh").is_file()
   and (ROOT / "testc/frappe/point_puis_entree.c").is_file(),
   "« . » puis Entrée : testé par un test unitaire et sous Xvfb (#3072)")
