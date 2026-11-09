# python312.dll x86 : variables de fuseau horaire sous 4 Go, fonctions
# mathématiques C99 et _wstat64i32 écrites hors macro (#1407).
ok('"__timezone" | "__p__timezone" => raw(ret_tz, 0)' in (ROOT / "runtime/src/wow32/crt.rs").read_text(encoding="utf-8")
   and 'pub extern "win64" fn acosh(x: f64) -> f64' in (ROOT / "runtime/src/dlls/crt_ucrt.rs").read_text(encoding="utf-8")
   and 'unsafe extern "win64" fn wstat64i32(p: *const u16, out: *mut u8) -> i32' in (ROOT / "runtime/src/dlls/crt_extra.rs").read_text(encoding="utf-8"),
   "python312.dll x86 : __timezone, maths C99, _wstat64i32 (#1407)")
