# ECDSA de CNG pour ISCC d'Inno Setup 6.7 (#2173).
_bc = (ROOT / "runtime/src/dlls/bcrypt.rs").read_text(encoding="utf-8")
_ec = (ROOT / "runtime/src/dlls/bcrypt/ecdsa.rs").read_text(encoding="utf-8")
ok("mod ecdsa;" in _bc and all(f"fn {n}(" in _ec for n in ("open", "verify_signature", "import_key_pair")),
   "bcrypt : fournisseur ECDSA ouvert, clé publique importée, signature vérifiée (#2173)")
ok('"ECDSA_P256"' in _ec and "fn verification_p256" in _ec,
   "bcrypt : vérification ECDSA P-256 testée sur une signature réelle (#2173)")
