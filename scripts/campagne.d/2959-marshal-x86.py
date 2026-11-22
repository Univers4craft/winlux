# CredMarshalCredentialW et CredUnmarshalCredentialW lisent et rendent
# leurs structures avec la taille de pointeur du programme ; le type 3
# (BinaryBlobCredential), non documenté, est refusé en x86 (#2959).
_c = (runtime_dlls / "credentials.rs").read_text(encoding="utf-8")
ok("fn marshal_input" in _c and "fn unmarshal_output" in _c,
   "le marshaling des identifiants suit la taille de pointeur du programme (#2959)")
ok(all(f"fn {t}" in _c for t in ("marshal_nom_x86_2959", "unmarshal_nom_x86_2959",
                                  "blob_refuse_en_x86_2959")),
   "tests : nom marshalé et démarshalé en x86, type 3 refusé en x86 (#2959)")
