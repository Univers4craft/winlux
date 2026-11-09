# 7-Zip x86 avec accès refusé : FormatMessage écrit un pointeur de 4 octets,
# `throw;` relance l'exception SEH prise par catch(...) (#1413).
ok("crate::wow32::write_ptr(buf as usize, p as usize)" in (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text(encoding="utf-8")
   and "seh: Option<(u32, u32, Vec<u32>)>" in (ROOT / "runtime/src/wow32/cxx.rs").read_text(encoding="utf-8")
   and (ROOT / "testc/wow32/cxx32_seh.cpp").is_file(),
   "7-Zip x86 : accès refusé sans plantage (#1413)")
