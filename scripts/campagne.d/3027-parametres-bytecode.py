# Paramètres : le bytecode des pages (installées dans /usr/bin, non
# inscriptible) est gardé dans le cache du compte au lieu d'être recompilé à
# chaque ouverture ; page modifiée relue, chargement sans cache (Gain #3027).
_t = (SCRIPTS / "test-settings-pycache.py").read_text(encoding="utf-8")
ok("sys.pycache_prefix" in (SCRIPTS / "winlux-settings.py").read_text(encoding="utf-8")
   and "page modifiée relue malgré le cache" in _t and "chargement sans cache inscriptible" in _t
   and "test-settings-pycache.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
   and "test-settings-pycache.py" in (SCRIPTS / "test-all.d/3027-parametres-bytecode.sh").read_text(encoding="utf-8"),
   "Paramètres : bytecode des pages dans le cache du compte, jamais périmé (#3027)")
