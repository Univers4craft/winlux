# #2118 : MoveFile vers une destination existante rend ERROR_ALREADY_EXISTS, comme MoveFileEx (comportement couvert par un test unitaire).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn deplacement_vers_une_destination_existante_rend_183" in k, "MoveFileA/W vers une destination existante rendent 183 (#2118)")
