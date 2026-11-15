# Le faux xdg-open du test « afficher dans le dossier » écrit son journal
# puis le renomme : le test ne lit jamais un journal vide (#2661).
_s = (ROOT / "runtime/src/dlls/apps_win32.rs").read_text()
ok("/bin/mv '{0}.tmp' '{0}'" in _s,
   "journal du faux xdg-open écrit puis renommé (#2661)")
