# #2448 : QueryDosDevice connaît toutes les lettres de GetLogicalDrives,
# dont Z:. Le comportement est gardé par le test unitaire nommé.
se = (ROOT / "runtime" / "src" / "dlls" / "serial.rs").read_text(encoding="utf-8")
ok("fn chaque_lecteur_annonce_a_une_cible_dos" in se, "test des cibles DOS de chaque lecteur annoncé (#2448)")
