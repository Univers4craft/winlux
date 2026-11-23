# LCMapStringW/Ex : drapeaux vérifiés, tampon trop petit refusé avec
# ERROR_INSUFFICIENT_BUFFER, clé de tri en octets (#3257).
nls_3257 = (ROOT / "runtime/src/dlls/nls.rs").read_text(encoding="utf-8")
ok("fn lcmapstring_suit_learn_3257" in nls_3257 and "pub fn sort_key_bytes" in nls_3257,
   "LCMapString : drapeaux, tampon trop petit et clé de tri en octets (#3257)")
