# ECDSA de CNG pour ISCC d'Inno Setup 6.7 (#2173).
_bc = (ROOT / "runtime/src/dlls/bcrypt.rs").read_text(encoding="utf-8")
_ec = (ROOT / "runtime/src/dlls/bcrypt/ecdsa.rs").read_text(encoding="utf-8")
ok("ecdsa::open(&name)" in _bc and "ecdsa::verify_signature(" in _bc and "ecdsa::import_key_pair(" in _bc,
   "bcrypt : fournisseur ECDSA ouvert, clé publique importée, signature vérifiée (#2173)")
ok('"ECDSA_P256"' in _ec and "fn verification_p256" in _ec,
   "bcrypt : vérification ECDSA P-256 testée sur une signature réelle (#2173)")
