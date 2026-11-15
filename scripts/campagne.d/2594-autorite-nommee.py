# #2594 : un refus de signature pour autorité non reconnue nomme le dernier
# certificat de la chaîne et son émetteur. Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/setup/appx_signature.rs").read_text(encoding="utf-8")
ok("fn racine_absente_nommee_dans_la_raison" in t, "signature de code : autorité manquante nommée (#2594)")
