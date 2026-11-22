# #3063 : l'ISO n'a pas 7-Zip mais livre bsdtar (libarchive-tools), qui écrit
# le 7z. « Compresser en fichier 7z » et la modification d'un .7z ouvert
# passent par lui ; un mot de passe, que libarchive ne sait pas chiffrer, est
# refusé. Comportement gardé par le test unitaire nommé.
_mod = (ROOT / "runtime/src/archive/mod.rs").read_text(encoding="utf-8")
ok("fn sept_z_ecrit_par_bsdtar_3063" in _mod and "fn create_7z_bsdtar(" in _mod,
   "7z créé par bsdtar quand 7-Zip manque, mot de passe refusé (#3063)")
_rel = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
_stage = (SCRIPTS / "stage-storage-tools.sh").read_text(encoding="utf-8")
ok("libarchive-tools" in _rel and " bsdtar " in _stage,
   "l'ISO embarque bsdtar, qui écrit le 7z (#3063)")
