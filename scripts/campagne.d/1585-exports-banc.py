# #1585 : exports manquants relevés par le banc sur Steam et pygame32 (cfgmgr32, kernel32, crypt32, advapi32).
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
dev = (runtime_dlls / "devnotify.rs").read_text(encoding="utf-8")
fold = (runtime_dlls / "kernel32_fold.rs").read_text(encoding="utf-8")
svc = (runtime_dlls / "advapi32_services.rs").read_text(encoding="utf-8")
c32 = (runtime_dlls / "crypt32.rs").read_text(encoding="utf-8")
enc = (runtime_dlls / "crypt32_encode.rs").read_text(encoding="utf-8")
ok('"CM_Register_Notification"' in dev and '"CM_Unregister_Notification"' in dev
   and "devnotify::cfgmgr32_exports()" in mod_rs, "cfgmgr32 : CM_Register_Notification et CM_Unregister_Notification (#1585)")
ok('"FoldStringW" => fn fold_string_w' in fold and "kernel32_fold::exports()" in mod_rs,
   "kernel32 : FoldStringW (#1585)")
ok('"QueryServiceConfigW"' in svc, "advapi32 : QueryServiceConfigW (#1585)")
ok("crypt32_encode::exports()" in c32 and '"CertStrToNameA" => fn CertStrToNameA' in enc
   and '"CryptEncodeObjectEx" => fn CryptEncodeObjectEx' in enc and '"CryptHashCertificate2"' in enc
   and '"CertCreateSelfSignCertificate"' in enc, "crypt32 : noms X.500, encodage DER, condensat, certificat auto-signé (#1585)")
