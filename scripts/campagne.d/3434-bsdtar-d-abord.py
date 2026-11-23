# #3434 : l'extraction préférait 7-Zip dès qu'il était installé, alors que
# l'ISO ne livre que bsdtar : deux comportements selon la machine (#3405 sur
# les runners, #3411 sur le PC). bsdtar d'abord partout, 7-Zip, unrar et
# unar seulement en repli ; un 7z chiffré seul passe par 7-Zip. Ordre gardé
# par le test unitaire nommé, qui simule les installations.
_mod_3434 = (ROOT / "runtime/src/archive/mod.rs").read_text(encoding="utf-8")
ok("fn bsdtar_d_abord_7zip_en_repli_3434" in _mod_3434
   and "fn extract_order(" in _mod_3434 and "fn create_7z_tool(" in _mod_3434,
   "bsdtar d'abord, 7-Zip seulement en repli, extraction et création (#3434)")
