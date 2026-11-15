# #2117 : GetLongPathName et GetShortPathName échouent sur un fichier absent (comportement couvert par un test unitaire).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn chemin_long_ou_court_d_un_fichier_absent_echoue" in k, "GetLongPathName/GetShortPathName d'un fichier absent rendent 0 et ERROR_FILE_NOT_FOUND (#2117)")
