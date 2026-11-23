# ntdll exporte _wcsnicmp dans la table de son image : le courtier du bac à
# sable de Chromium (BSD) le cherche juste après memcpy et tue le moteur de
# rendu d'Acrobat s'il est absent (i20, #3367).
nt_3367 = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok('"_wcsnicmp" => fn' in nt_3367
   and "fn wcsnicmp_dans_l_image_ntdll_3367" in nt_3367,
   "ntdll : _wcsnicmp dans la table d'exports de l'image (#3367)")
