# #1728 : WinVerifyTrust vérifie vraiment et garde son état pour les WTHelper*.
w = (runtime_dlls / "wintrust.rs").read_text(encoding="utf-8")
ok("examine_pe_with_certs(&path)" in w and "TRUST_E_NOSIGNATURE" in w,
   "WinVerifyTrust vérifie la signature Authenticode du fichier, sans bouchon de confiance (#1728)")
ok("fn build_state(" in w and "WTD_STATEACTION_CLOSE" in w,
   "WinVerifyTrust garde un CRYPT_PROVIDER_DATA jusqu'à WTD_STATEACTION_CLOSE (#1728)")
ok('"WTHelperGetProvCertFromChain" => fn WTHelperGetProvCertFromChain' in w,
   "WTHelper* rendent le signataire et ses certificats (#1728)")
