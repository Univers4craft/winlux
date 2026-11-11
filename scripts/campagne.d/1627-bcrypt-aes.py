# #1627 : AES de CNG (clés symétriques, KeyDataBlob, CBC/ECB/CFB) et BCryptDuplicateHash.
bc = (runtime_dlls / "bcrypt.rs").read_text(encoding="utf-8")
ok(all(f'"{n}"=>fn' in bc.replace(" ", "") for n in ("BCryptImportKey", "BCryptSetProperty", "BCryptDuplicateHash",
   "BCryptGenerateSymmetricKey", "BCryptEncrypt", "BCryptDecrypt")) and "bcrypt_stub" not in bc,
   "bcrypt sert AES (import de clé, mode de chaînage, chiffrement) et BCryptDuplicateHash (#1627)")
