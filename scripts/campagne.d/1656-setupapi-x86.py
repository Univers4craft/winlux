# #1656 : setupapi remplit ses structures à la taille x86 (pack(1), ULONG_PTR de 4 octets).
dev = (runtime_dlls / "devnodes.rs").read_text(encoding="utf-8")
inf = (runtime_dlls / "setupinf.rs").read_text(encoding="utf-8")
ok("fn write_reserved" in dev and "(data.add(24) as *mut usize).write_unaligned(index + 1);" in dev
   and dev.count("as *mut usize).write_unaligned(") == 1,
   "setupapi : Reserved de SP_DEVINFO_DATA/SP_DEVICE_INTERFACE_DATA écrit sur 4 octets en x86 (#1656)")
ok("LAYOUT_32" in inf and "drvinfo_v2: 1564" in inf and "install_params: 556" in inf and "ctx_section: 8" in inf,
   "setupapi : dispositions x86 pack(1) des structures de pilotes et d'INFCONTEXT (#1656)")
ok((ROOT / "scripts" / "test-setupapi32.sh").exists() and (ROOT / "testc" / "wow32" / "setupapi32.c").exists(),
   "setupapi : test x86 avec gardes après chaque structure (#1656)")
