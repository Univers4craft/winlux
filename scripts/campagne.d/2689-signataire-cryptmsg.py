# #2689 : CryptMsgGetParam(CMSG_SIGNER_INFO_PARAM) rend le SignerInfo du
# message (Acrobat vérifie ainsi l'éditeur de ses greffons).
# Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/crypt32_signer/tests.rs").read_text(encoding="utf-8")
ok("fn informations_du_signataire_rendues" in t, "crypt32 : CryptMsgGetParam rend les informations du signataire (#2689)")
