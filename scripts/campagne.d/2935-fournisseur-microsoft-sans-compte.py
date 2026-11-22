# Teams trouve le fournisseur Microsoft, sans compte ni jeton (#2935).
o = (ROOT / "runtime" / "src" / "dlls" / "winrt_wam_objets.rs").read_text(encoding="utf-8")
ok("fn requete_de_jeton_et_fournisseur_2511" in o, "WebAccountProvider et WebTokenRequest servis (#2935)")
ok("fn resultats_sans_compte_ni_jeton_2511" in o, "aucun compte, interaction requise, sans jeton (#2935)")
