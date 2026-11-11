# MSI : une action personnalisée arrêtée par un signal le dit dans le journal, avec ses diagnostics (#1628).
run = (ROOT / "runtime/src/setup/run.rs").read_text(encoding="utf-8")
act = (ROOT / "runtime/src/msi/actions.rs").read_text(encoding="utf-8")
ok("fn signal_line" in run and "processus de l'action arrêté sans rendre de code" in act,
   "MSI : signal et diagnostics d'une action en échec écrits au journal (#1628)")
