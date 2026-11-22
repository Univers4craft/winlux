# Une lettre tapée après un raccourci pris par TranslateAccelerator (Ctrl+N de Notepad++) n'est plus perdue (#2870).
# Le comportement est vérifié par le test unitaire et sous Xvfb par scripts/test-raccourci-puis-lettre.sh.
_g = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
ok("fn une_pression_non_traduite_ne_vole_pas_le_caractere_de_la_suivante" in _g
   and (ROOT / "scripts/test-raccourci-puis-lettre.sh").is_file()
   and (ROOT / "testc/frappe/raccourci_puis_lettre.c").is_file(),
   "lettre après un raccourci Ctrl+N : testée par un test unitaire et sous Xvfb (#2870)")
