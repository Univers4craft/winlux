# Test du chemin d'un autre processus : lu seulement après l'exec de l'enfant (#2109).
_w = (ROOT / "runtime/src/dlls/win32_basics.rs").read_text(encoding="utf-8")
ok("sleep n'a pas été exécuté en 10 s" in _w, "chemin_d_un_autre_processus attend l'exec de l'enfant (#2109)")
