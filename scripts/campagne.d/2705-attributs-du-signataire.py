# #2705 : CryptMsgGetParam rend les attributs (authentifiés et non
# authentifiés) du signataire au lieu de refuser ces paramètres.
f = (ROOT / "runtime/src/dlls/crypt32_signer/tests.rs").read_text(encoding="utf-8")
ok("fn attributs_du_signataire_rendus_2705" in f,
   "Signature : attributs du signataire rendus par CryptMsgGetParam (#2705)")
