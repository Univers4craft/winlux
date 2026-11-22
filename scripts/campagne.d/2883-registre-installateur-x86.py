# Un installateur NSIS x86 voit HKLM\Software sous Wow6432Node : la clé d'une installation x64 ne décide pas de son dossier (#2883).
_n = (ROOT / "runtime/src/setup/nsis.rs").read_text()
ok("fn registre_vu_par_un_installateur_x86" in _n,
   "un installateur x86 lit et écrit le registre dans la vue 32 bits (#2883)")
