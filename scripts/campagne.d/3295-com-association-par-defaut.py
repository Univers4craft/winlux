# CoCreateInstance de CLSID_ApplicationAssociationRegistration rend le même
# objet que SHCreateAssociationRegistration (Acrobat, passe « PDF mutant »,
# #3295).
ole_3295 = (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
tw_3295 = (ROOT / "runtime/src/dlls/teams_win32.rs").read_text(encoding="utf-8")
ok("super::teams_win32::create_builtin(&guid, &iid, out)" in ole_3295
   and "CLSID_APPLICATION_ASSOCIATION_REGISTRATION" in tw_3295
   and "fn association_par_co_create_instance_3295" in tw_3295,
   "COM : ApplicationAssociationRegistration créée par CoCreateInstance (#3295)")
