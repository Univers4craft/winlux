# winlux-open : une sortie volontaire (ExitProcess, exit) en code non nul n'affiche
# pas « arrêté de manière inattendue » ; une exception non gérée, si (#2272).
_t = (SCRIPTS / "test-sortie-volontaire.sh").read_text()
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("WINLUX_CRASH_MARK_FILE" in (ROOT / "integration/winlux-open").read_text()
   and "fn record_crash" in _k and "fn marque_de_plantage_distincte_du_code" in _k
   and "test-sortie-volontaire.sh" in (ROOT / "scripts/test-all.d/2272-sortie-volontaire.sh").read_text(),
   "winlux-open : sortie volontaire silencieuse, plantage signalé (#2272)")
