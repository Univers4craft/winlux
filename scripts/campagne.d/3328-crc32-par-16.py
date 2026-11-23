# #3328 (Gain) : le CRC-32 des archives traitait un octet par tour
# (304 Mo/s, un tiers du temps d'extraction). Découpage par 16 octets,
# résultat identique vérifié par le test unitaire nommé.
_crc_3328 = (ROOT / "runtime/src/archive/crc32.rs").read_text(encoding="utf-8")
ok("fn decoupage_identique_octet_par_octet" in _crc_3328 and "static SLICES" in _crc_3328,
   "CRC-32 des archives découpé par 16 octets, identique à l'octet par octet (#3328)")
