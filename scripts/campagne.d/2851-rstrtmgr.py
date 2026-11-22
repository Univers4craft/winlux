# rstrtmgr.dll (Gestionnaire de redémarrage) servie : OneDriveSetup.exe
# l'importe statiquement (#2851).
_r = (runtime_dlls / "rstrtmgr.rs").read_text(encoding="utf-8")
_m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('name: "rstrtmgr"' in _m and all(f'"{n}"' in _r for n in ("RmStartSession", "RmRegisterResources", "RmGetList", "RmEndSession")),
   "rstrtmgr.dll intégrée, avec les quatre fonctions importées par OneDriveSetup.exe (#2851)")
ok("fn session_du_gestionnaire_de_redemarrage_2851" in _r and "fn arguments_refuses_2851" in _r,
   "tests de la session du Gestionnaire de redémarrage et des arguments refusés (#2851)")
ok("[rstrtmgr]" in (ROOT / "runtime/src/wow32/stdcall.tsv").read_text(encoding="utf-8"),
   "tailles d'arguments x86 de rstrtmgr.dll dans la table stdcall (#2851)")
