# #2758 : un code de sortie Windows non nul dont l'octet bas vaut 0
# (0x80131500) ne sort plus en statut Linux 0. Comportement gardé par le
# test dlls::kernel32::tests_code_enfant::exitprocess_0x80131500_vu_comme_un_echec.
k32_2758 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
ok("pub const fn linux_exit_status(code: u32) -> i32" in k32_2758
   and "fn exitprocess_0x80131500_vu_comme_un_echec" in k32_2758,
   "code de sortie 0x80131500 vu comme un échec par le shell Linux (#2758)")
