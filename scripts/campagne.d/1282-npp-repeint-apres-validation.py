# Une invalidation qui suit une ValidateRect n'est plus perdue : l'éditeur
# Scintilla de Notepad++ se repeint après l'auto-complétion et la bulle d'appel.
ok("invalidation_apres_validation_repeint" in (runtime_dlls / "gui.rs").read_text()
   and "invalidation_apres_validation_repeint" in (SCRIPTS / "test-all.d" / "1279-enum-child-windows.sh").read_text(),
   "Notepad++ se repeint après l'auto-complétion : invalidation conservée après ValidateRect (#1282)")
