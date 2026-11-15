# Runtime VC++ intégré : flux et locales de msvcp140 (basic_ios, iostream, _Locinfo…) pour OBS sans redistribuable (#2195).
_m = (ROOT / "runtime/src/dlls/mod.rs").read_text()
ok("m.extend(msvcp140_flux::exports());" in _m,
   "msvcp140 sert aussi les flux et les locales (#2195)")
_p = (ROOT / "runtime/src/dlls/msvcp140_flux.rs").read_text()
_e = _p[_p.index("pub fn exports()"):]
ok('"??0?$basic_ios@DU?$char_traits@D@std@@@std@@IEAA@XZ"' in _e,
   "msvcp140 : le constructeur basic_ios<char> qui arrêtait OBS est exporté (#2195)")
ok(_e.count('" => ') >= 200 and '"?cout@std@@3V?$basic_ostream@DU?$char_traits@D@std@@@1@A"' in _e,
   "msvcp140 : flux, facettes, cout et cerr exportés (#2195)")
