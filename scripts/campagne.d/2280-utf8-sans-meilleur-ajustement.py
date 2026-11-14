# #2280 : WideCharToMultiByte(CP_UTF8) accepte WC_NO_BEST_FIT_CHARS (installeur de Logitech G HUB).
intl_api = (ROOT / "runtime" / "src" / "intl" / "api.rs").read_text(encoding="utf-8")
ok("flags & !(WC_ERR_INVALID_CHARS | WC_NO_BEST_FIT_CHARS) != 0" in intl_api
   and "0x480," in intl_api,
   "WideCharToMultiByte : WC_NO_BEST_FIT_CHARS accepté en UTF-8 (#2280)")
