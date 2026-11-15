# #2683 : la racine DigiCert Assured ID est reconnue pour la signature de
# code (horodatage « DigiCert SHA2 Assured ID Timestamping CA »).
# Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/setup/appx_signature.rs").read_text(encoding="utf-8")
ok("fn racine_d_horodatage_digicert_assured_id" in t, "signature de code : racine d'horodatage DigiCert Assured ID reconnue (#2683)")
