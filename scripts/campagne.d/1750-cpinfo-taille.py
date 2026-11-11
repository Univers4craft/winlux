# #1750 : GetCPInfo(Ex) n'écrit pas au-delà de CPINFO (18) / CPINFOEXW (544) / CPINFOEXA (284).
a = (ROOT / "runtime/src/intl/api.rs").read_text(encoding="utf-8")
ok("const CPINFO_SIZE: usize = 18;" in a and "const CPINFOEXW_SIZE: usize = 544;" in a
   and "const CPINFOEXA_SIZE: usize = 284;" in a and "548 - 20" not in a,
   "GetCPInfo et GetCPInfoEx respectent les tailles de winnls.h (#1750)")
