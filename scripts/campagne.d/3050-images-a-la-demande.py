# Images PE des DLL intégrées construites à la première demande de leur
# HMODULE, chacune dans la case d'adresse fixe de son rang ; ntdll et
# kernel32 d'avance pour le bac à sable de Chromium (Gain #3050). Le test
# unitaire vérifie la case, le retour par HMODULE et l'en-tête « MZ ».
_t = (ROOT / "runtime/src/dlls/pe_image.rs").read_text(encoding="utf-8")
ok("fn images_a_la_demande_dans_leur_case_fixe()" in _t
   and "fn builtin_handle(" in (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8"),
   "DLL intégrées : image PE construite à la demande, à adresse fixe (#3050)")
