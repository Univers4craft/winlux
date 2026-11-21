# Un .exe ou une DLL en cours d'exécution ne peut être ni réécrit ni tronqué (#1917).
# Vérifie les noms du verrou d'image et du test qui refuse l'écriture.
_i = (ROOT / "runtime/src/image_lock.rs").read_text(encoding="utf-8")
ok("fn hold(" in _i and "fn refuse_write(" in _i
   and "fn image_chargee_refuse_l_ecriture(" in _i,
   "image en cours d'exécution : CreateFile et CopyFile en écriture refusés (ERROR_SHARING_VIOLATION) (#1917)")
