# #1708 : chaînes et paramètres de stratégie crypt32 en disposition x86.
c = (runtime_dlls / "crypt32.rs").read_text(encoding="utf-8")
ok("fn fill_chain_view(" in c and "simple32: [u32; 8]" in c,
   "CERT_CHAIN_CONTEXT rendu en disposition x86 aux programmes 32 bits (#1708)")
# Noms : la fonction CertVerifyCertificateChainPolicy et la structure
# SSL_EXTRA_CERT_CHAIN_POLICY_PARA qu'elle lit (#2711).
ok("fn CertVerifyCertificateChainPolicy(" in c and "SSL_EXTRA_CERT_CHAIN_POLICY_PARA" in c
   and "wow32::pointer_size" in c,
   "CertVerifyCertificateChainPolicy : paramètres SSL lus à la taille d'un pointeur (#1708)")
ok("fn engine_config_layout(ps: usize)" in c,
   "CERT_CHAIN_ENGINE_CONFIG lu selon la taille d'un pointeur (#1708)")
