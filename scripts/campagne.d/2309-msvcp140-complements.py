# msvcp140 : flux, locale::id, call_once, _Dtest et lanceurs importés par iCUE (#2309).
_f = (ROOT / "runtime/src/dlls/msvcp140_flux.rs").read_text()
ok("m.extend(complements::exports());" in _f,
   "msvcp140 sert les compléments de flux importés par iCUE (#2309)")
_c = (ROOT / "runtime/src/dlls/msvcp140_flux/complements.rs").read_text()
ok('"?_Execute_once@std@@YAHAEAUonce_flag@1@P6AHPEAX1PEAPEAX@Z1@Z"' in _c
   and '"?peek@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAHXZ"' in _c
   and '"_Dtest"' in _c,
   "msvcp140 : _Execute_once, istream::peek et _Dtest exportés (#2309)")
