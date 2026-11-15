# #2698 : un CRYPT_*_BLOB d'un programme 32 bits (CertNameToStrW, CryptQueryObject)
# est lu avec pbData à +4, et non à la disposition x64.
f = (ROOT / "runtime/src/dlls/crypt32.rs").read_text(encoding="utf-8")
ok("fn blob_x86_et_x64_2698" in f,
   "Certificats : CERT_NAME_BLOB x86 lu à la bonne disposition (#2698)")
