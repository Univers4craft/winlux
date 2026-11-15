# #2708 : CertGetEnhancedKeyUsage rend les OID de l'extension EKU du
# certificat au lieu de répondre toujours « tous usages ».
f = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text(encoding="utf-8")
ok("fn usages_etendus_du_certificat_2708" in f,
   "Certificats : usages étendus du certificat rendus (#2708)")
