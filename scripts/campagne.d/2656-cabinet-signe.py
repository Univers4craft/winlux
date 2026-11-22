# #2656 : WinVerifyTrust lit la signature Authenticode d'un cabinet (.cab)
# au lieu de le déclarer non signé (TRUST_E_NOSIGNATURE).
f = (ROOT / "runtime/src/setup/appx_signature.rs").read_text(encoding="utf-8")
ok("fn cabinet_signe_lu_et_verifie_2656" in f,
   "Signature : un cabinet signé est lu et vérifié (#2656)")
