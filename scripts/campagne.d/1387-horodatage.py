# Signatures Authenticode (#1387) : horodatage RFC 3161 ou PKCS #9 lu, chaîne du signataire vérifiée à sa date (cas Steam).
ok("fn rfc3161_time" in (ROOT / "runtime/src/setup/appx_signature.rs").read_text()
   and "fn countersignature_time" in (ROOT / "runtime/src/setup/appx_signature.rs").read_text()
   and "X509_STORE_CTX_set_time" in (runtime_dlls / "hostssl.rs").read_text(),
   "signature horodatée au certificat expiré : éditeur vérifié (#1387)")
