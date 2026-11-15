# Les tests qui remplacent tout PATH tournent dans un processus enfant, pour
# ne pas priver les autres tests de sleep, stty ou sed (#2658).
_l = (ROOT / "runtime/src/launch.rs").read_text()
ok("fn path_isole(" in _l, "aide path_isole des tests (#2658)")
_s = (ROOT / "runtime/src/dlls/shell32.rs").read_text()
ok(_s.count("crate::launch::path_isole(") == 2,
   "tests shell32 qui remplacent PATH isolés (#2658)")
_a = (ROOT / "runtime/src/dlls/apps_win32.rs").read_text()
ok("crate::launch::path_isole(" in _a,
   "test apps_win32 qui remplace PATH isolé (#2658)")
