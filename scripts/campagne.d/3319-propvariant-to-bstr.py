# propsys n'exportait pas PropVariantToBSTR : le renderer de Discord mourait
# en 0xC0000139 en lisant les propriétés d'un périphérique audio (#3319).
# Le test unitaire nommé ici vérifie la sémantique de Learn pour
# PropVariantToBSTR, PropVariantToString et PropVariantToBuffer.
_ps = (ROOT / "runtime" / "src" / "dlls" / "lot_f_propsys.rs").read_text(encoding="utf-8")
ok("fn propvariant_vers_bstr_texte_et_tampon_3319" in _ps,
   "PropVariantToBSTR, PropVariantToString et PropVariantToBuffer servis selon Learn (#3319)")
