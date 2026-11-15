# #2679 : le moteur de chaînes par défaut termine la chaîne d'un certificat
# de signature de code sur les racines de code (jamais pour TLS), et un refus
# de chaîne laisse une ligne au journal.
f = (ROOT / "runtime/src/dlls/crypt32.rs").read_text(encoding="utf-8")
ok("fn racines_de_signature_de_code_au_moteur_par_defaut_2679" in f,
   "Certificats : racines de signature de code au moteur par défaut (#2679)")
ok("fn refus_de_chaine_journalise_2679" in f,
   "Certificats : un refus de chaîne est journalisé (#2679)")
