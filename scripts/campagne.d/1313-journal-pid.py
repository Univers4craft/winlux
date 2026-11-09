# #1313 : les lignes de WINLUX_LOG_FILE portent le programme et le pid.
ok("journal_origin()" in (ROOT / "runtime/src/log.rs").read_text(encoding="utf-8").split("fn file_line", 1)[-1][:400]
   and "journal_fichier_porte_le_programme_et_le_pid" in (SCRIPTS / "test-all.d/1313-journal-pid.sh").read_text(encoding="utf-8"),
   "WINLUX_LOG_FILE : programme et pid sur chaque ligne, tous processus confondus (#1313)")
