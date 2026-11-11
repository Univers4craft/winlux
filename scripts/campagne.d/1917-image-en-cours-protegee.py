# Un .exe ou une DLL en cours d'exécution ne peut être ni réécrit ni tronqué (#1917).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_l = (ROOT / "runtime/src/loader.rs").read_text(encoding="utf-8")
_i = (ROOT / "runtime/src/image_lock.rs").read_text(encoding="utf-8")
ok("crate::image_lock::hold(path);" in _l
   and 'crate::image_lock::refuse_write(&p, "CreateFile")' in _k
   and _k.count('crate::image_lock::refuse_write(&target, "CopyFile")') == 2
   and "fn image_chargee_refuse_l_ecriture" in _i,
   "image en cours d'exécution : CreateFile et CopyFile en écriture refusés (ERROR_SHARING_VIOLATION) (#1917)")
