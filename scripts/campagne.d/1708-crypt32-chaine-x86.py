# #1708 : chaînes et paramètres de stratégie crypt32 en disposition x86.
c = (runtime_dlls / "crypt32.rs").read_text(encoding="utf-8")
ok("fn fill_chain_view(" in c and "simple32: [u32; 8]" in c,
   "CERT_CHAIN_CONTEXT rendu en disposition x86 aux programmes 32 bits (#1708)")
ok("crate::wow32::read_ptr(para + 8)" in c and "12usize.next_multiple_of(ps)" in c,
   "CertVerifyCertificateChainPolicy : paramètres SSL lus à la taille d'un pointeur (#1708)")
ok("fn engine_config_layout(ps: usize)" in c,
   "CERT_CHAIN_ENGINE_CONFIG lu selon la taille d'un pointeur (#1708)")
