# msvcp140 : std::exception_ptr (__ExceptionPtr*), iCUE ouvre sa fenêtre (#2307).
_m = (ROOT / "runtime/src/dlls/mod.rs").read_text()
ok("m.extend(msvcp140_eptr::exports());" in _m,
   "msvcp140 sert les fonctions de std::exception_ptr (#2307)")
_e = (ROOT / "runtime/src/dlls/msvcp140_eptr.rs").read_text()
ok(_e.count('"?__ExceptionPtr') == 10 and '"?_Rethrow_future_exception@std@@YAXVexception_ptr@1@@Z"' in _e,
   "msvcp140 : les dix __ExceptionPtr* et _Rethrow_future_exception exportées (#2307)")
_c = (ROOT / "runtime/src/dlls/cxx_eh.rs").read_text()
ok("msvcp140_eptr::liberer_copie_relancee(obj)" in _c,
   "cxx_eh rend la copie levée par rethrow_exception à la fin du catch (#2307)")
