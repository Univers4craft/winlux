# #2116 : GetFileSize et SetFilePointer réussis qui rendent 0xFFFFFFFF remettent GetLastError à 0 (comportement couvert par un test unitaire).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn taille_et_position_0xffffffff_remettent_le_code_a_zero" in k, "GetFileSize/SetFilePointer à 0xFFFFFFFF réussis posent NO_ERROR (#2116)")
