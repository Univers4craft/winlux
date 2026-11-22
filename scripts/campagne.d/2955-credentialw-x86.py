# Le gestionnaire d'identifiants lit et écrit CREDENTIALW avec la
# disposition du programme appelant : 52 octets et pointeurs de 4 octets
# pour un programme 32 bits, 80 octets en x64 (#2955).
_c = (runtime_dlls / "credentials.rs").read_text(encoding="utf-8")
_inv = (ROOT / "docs" / "compat" / "wow32-structures.tsv").read_text(encoding="utf-8")
ok("struct CredentialW32" in _c and "fn caller_layout" in _c,
   "CREDENTIALW a une disposition x86 choisie selon le programme appelant (#2955)")
ok("credentials.rs\tCredentialW64\tx86\t" in _inv,
   "CREDENTIALW est inventoriée comme convertie x86 dans wow32-structures.tsv (#2955)")
ok(all(f"fn {t}" in _c for t in ("dispositions_credentialw_2955", "credentialw_x86_2955",
                                  "credentialw_x64_2955", "tableau_credentialw_x86_2955")),
   "tests : décalages x86/x64, aller-retour x86 et x64, tableau de CredEnumerateW (#2955)")
