# CoCreateInstance sert CLSID_NetworkListManager : le module réseau de Qt 5 (iCUE 4.18) ne lit plus un objet absent (#1844).
ok("fn gestionnaire_points_de_connexion_et_abonnes" in (ROOT / "runtime/src/dlls/netlistmgr.rs").read_text(encoding="utf-8")
   and "netlistmgr::create_builtin" in (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8"),
   "gestionnaire de listes de réseaux servi par CoCreateInstance (#1844)")
