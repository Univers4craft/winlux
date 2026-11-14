# msvcp140 : membres protégés de codecvt<wchar_t> et wcerr pour l'installeur de Logitech G HUB (#2229).
_f = (ROOT / "runtime/src/dlls/msvcp140_flux.rs").read_text()
ok('"?do_in@?$codecvt@_WDU_Mbstatet@@@std@@' in _f and '"?wcerr@std@@3V' in _f,
   "msvcp140 exporte do_in/do_out de codecvt<wchar_t> et wcerr (#2229)")
