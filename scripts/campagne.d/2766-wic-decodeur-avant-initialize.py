# #2766 : un décodeur WIC créé par CreateDecoder(format) connaît son format
# avant Initialize ; GetDecoderInfo répond (Paint.NET, WicHelpers.GetLoadExtensions).
# Comportement gardé par le test unitaire
# dlls::windowscodecs::tests::informations_d_un_decodeur_avant_initialize.
wic_2766 = (ROOT / "runtime" / "src" / "dlls" / "windowscodecs.rs").read_text(encoding="utf-8")
ok("struct DecoderCell" in wic_2766 and "fn informations_d_un_decodeur_avant_initialize" in wic_2766,
   "WIC : GetDecoderInfo d'un décodeur avant Initialize (#2766)")
