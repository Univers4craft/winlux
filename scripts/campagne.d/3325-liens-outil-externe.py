# #3325 : un 7z ou un RAR extrait par un outil externe (7-Zip, unrar, unar,
# bsdtar) gardait ses liens vers /etc ou « ../../.. ». Comme pour ZIP et TAR
# (#3062), les liens qui sortent du dossier sont retirés après l'outil.
# Comportement gardé par le test unitaire nommé.
_mod_3325 = (ROOT / "runtime/src/archive/mod.rs").read_text(encoding="utf-8")
_util_3325 = (ROOT / "runtime/src/archive/util.rs").read_text(encoding="utf-8")
ok("fn liens_sortants_outil_externe_retires" in _mod_3325
   and "pub fn remove_escaping_links(" in _util_3325,
   "liens sortants d'un 7z ou d'un RAR retirés après l'outil externe (#3325)")
