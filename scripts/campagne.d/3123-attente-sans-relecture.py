# WaitForMultipleObjects bloque jusqu'au signal, sans relire les objets toutes les millisecondes (#3123).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn wait_for_multiple_objects_dort_jusqu_au_signal" in _k,
   "attente sur plusieurs objets sans réveil avant le signal (#3123)")
ok("fn wait_for_multiple_objects_reveille_par_semaphore_et_mutex" in _k,
   "attente sur plusieurs objets réveillée par sémaphore et mutex (#3123)")
