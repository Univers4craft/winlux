# #2593 : la racine DigiCert High Assurance EV est reconnue pour la
# signature de code (greffons signés par DigiCert EV Code Signing CA).
# Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/setup/appx_signature.rs").read_text(encoding="utf-8")
ok("fn racine_digicert_ev_pour_la_signature_de_code" in t, "signature de code : racine DigiCert EV reconnue (#2593)")
