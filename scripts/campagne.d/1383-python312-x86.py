# python312.dll x86 : __fpe_flt_rounds et les autres fonctions du CRT et de
# kernel32 qu'elle importe ; aides mathématiques _CI* à un opérande (#1383).
ok('"__fpe_flt_rounds" => fn fpe_flt_rounds' in (ROOT / "runtime/src/dlls/crt_proc.rs").read_text(encoding="utf-8")
   and '"_wspawnv" => fn wspawnv' in (ROOT / "runtime/src/dlls/crt_proc.rs").read_text(encoding="utf-8")
   and '"PathCchSkipRoot"' in (ROOT / "runtime/src/dlls/kernel32_path.rs").read_text(encoding="utf-8")
   and '"_CIsinh" => ci1_code' in (ROOT / "runtime/src/wow32/crt.rs").read_text(encoding="utf-8"),
   "fonctions importées par python312.dll x86 (#1383)")
