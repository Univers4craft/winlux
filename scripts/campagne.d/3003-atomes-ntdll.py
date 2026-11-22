# #3003 : ntdll exporte NtAddAtom, NtAddAtomEx, NtFindAtom et NtDeleteAtom en
# refus franc (STATUS_NOT_IMPLEMENTED), sans lire leurs arguments : le bac à
# sable d'Acrobat trouve sa cible d'interception. Le comportement est gardé
# par les tests unitaires nommés.
t = (runtime_dlls / "ntdll_atoms.rs").read_text(encoding="utf-8")
ok("fn atomes_ntdll_exportes_en_refus_franc_3003" in t, "ntdll exporte les fonctions d'atomes en refus franc (#3003)")
ok("fn atomes_ntdll_tailles_x86_3003" in t, "les fonctions d'atomes de ntdll dépilent la taille x86 de MinGW-w64 (#3003)")
h = (runtime_dlls / "handle_origin.rs").read_text(encoding="utf-8")
ok("fn origine_d_un_handle_nomme_l_appelant_3004" in h, "WINLUX_DIAG_HANDLES nomme l'appel créateur d'un handle (#3003)")
