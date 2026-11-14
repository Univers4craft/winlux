# Runtime VC++ intégré : msvcp140_2 (fonctions mathématiques spéciales) pour Qt6Gui / OBS (#2187).
_m = (ROOT / "runtime/src/dlls/mod.rs").read_text()
_r = _m[_m.index("pub fn prefers_real_file("):]
_r = _r[:_r.index("\n}\n")]
ok("pub mod msvcp140_2;" in _m and "msvcp140_2::exports" in _m and '"msvcp140_2"' in _r,
   "msvcp140_2 intégrée, la vraie DLL passe avant quand elle est présente (#2187)")
_p = (ROOT / "runtime/src/dlls/msvcp140_2.rs").read_text()
ok(_p.count('"__std_smf_') == 44 and '"__std_smf_hypot3f"' in _p,
   "msvcp140_2 : les 44 fonctions __std_smf_* exportées, dont hypot3f (#2187)")
