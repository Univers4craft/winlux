# #1667 : les contextes de certificat suivent la disposition x86 (CERT_CONTEXT 20 octets, CERT_INFO 112).
c = (runtime_dlls / "crypt32.rs").read_text(encoding="utf-8")
e = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("fn fill_view_ps" in c and "info32: [u32; 28]" in c and "view: [u64; 5]" in c,
   "CERT_CONTEXT et CERT_INFO rendus en disposition x86 aux programmes 32 bits (#1667)")
ok("let size = 2 * ps as u32;" in e,
   "CERT_ENHKEY_USAGE de la taille d'un pointeur x 2 (#1667)")
