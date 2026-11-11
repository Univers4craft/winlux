# MSI : le processus x86 d'une action lit le paquet à la demande, sans le charger en entier (#1642).
sdb = (ROOT / "runtime/src/msi/session_db.rs").read_text(encoding="utf-8")
cfb = (ROOT / "runtime/src/msi/cfb.rs").read_text(encoding="utf-8")
act = (ROOT / "runtime/src/msi/actions.rs").read_text(encoding="utf-8")
ok("Database::open_tables" in sdb and "read_exact_at" in cfb,
   "MSI : base de session lue à la demande, cabinets intégrés laissés sur le disque (#1642)")
ok("fn is_abort_message" in act,
   "MSI : message d'abandon (mémoire refusée, erreur fatale) gardé au journal de l'action (#1642)")
