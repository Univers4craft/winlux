# #1195 : SSE 4.2 et voisins annoncés comme CPUID les voit (Photoshop).
ok('38 => f("sse4.2")' in (runtime_dlls / "kernel32_misc.rs").read_text(encoding="utf-8")
   and "instructions_sse4_et_avx512_suivent_cpuid" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "IsProcessorFeaturePresent : SSSE3, SSE4.1, SSE4.2 et AVX-512F selon CPUID (#1195)")
