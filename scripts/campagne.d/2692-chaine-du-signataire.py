# #2692 : WinVerifyTrust range la chaîne du signataire dans
# CRYPT_PROVIDER_SGNR.pChainContext au lieu de la laisser nulle.
f = (ROOT / "runtime/src/dlls/wintrust.rs").read_text(encoding="utf-8")
ok("fn chaine_du_signataire_dans_l_etat_2692" in f,
   "Signature : chaîne du signataire dans l'état de WinVerifyTrust (#2692)")
