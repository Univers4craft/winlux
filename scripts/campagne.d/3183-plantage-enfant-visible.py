# Une exception non gérée écrit toujours son entrée [CRASH] ; sans lanceur
# qui l'affiche (processus enfant, lancement direct), la boîte « a cessé de
# fonctionner » s'ouvre, sauf sans affichage, avec SEM_NOGPFAULTERRORBOX ou
# avec WER_FAULT_REPORTING_NO_UI (#3183).
misc_3183 = (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text(encoding="utf-8")
ok("fn plantage_non_gere_boite_et_journal_3183" in misc_3183,
   "Exception non gérée : entrée [CRASH] et boîte d'un enfant, testées de bout en bout (#3183)")
ok("fn quatre_conditions_de_la_boite_3183" in misc_3183,
   "Boîte de plantage : marque winlux-open, affichage, SetErrorMode, WerSetFlags (#3183)")
ok("fn texte_de_la_boite_3183" in misc_3183,
   "Boîte de plantage : programme, code et chemin du journal en fr/en/de/es (#3183)")
report_3183 = (ROOT / "scripts/winlux-report.py").read_text(encoding="utf-8")
ok("def auto_sendable(crash)" in report_3183,
   "Envoi automatique : exception non gérée, exception C++ et __fastfail jamais mises en file (#3183)")
