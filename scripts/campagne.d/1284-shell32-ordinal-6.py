# SHDefExtractIconW exporté aussi par son ordinal 6 (import de wxWidgets).
ok('"#6" => fn sh_def_extract_icon_w' in (runtime_dlls / "ftp_apps.rs").read_text()
   and "shell32_ordinal_6_est_sh_def_extract_icon_w" in (SCRIPTS / "test-all.d" / "1284-shell32-ordinal-6.sh").read_text(),
   "shell32 ordinal 6 (SHDefExtractIconW) : fermeture d'une seconde fenêtre d'Audacity (#1284)")
