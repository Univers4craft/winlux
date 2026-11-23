# kernel32 : RtlZeroMemory, RtlSecureZeroMemory, RtlCopyMemory,
# RtlMoveMemory et RtlCompareMemory se résolvent par GetProcAddress (#3349).
_k32_3349 = (ROOT / "runtime/src/dlls/kernel32_node.rs").read_text(encoding="utf-8")
ok("fn rtl_memoire_sur_kernel32_3349" in _k32_3349,
   "kernel32 : les cinq Rtl*Memory sont servies (#3349)")
