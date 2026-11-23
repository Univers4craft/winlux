# #3282 : les outils d'archive (7-Zip, unrar, unar, bsdtar) convertissent les
# noms selon la locale héritée ; sous C, bsdtar abîmait « été » sans erreur à
# la compression et ne relisait plus rien. Chaque outil passe désormais par
# tools::archiver (LC_ALL=C.UTF-8). Comportement gardé par le test unitaire
# nommé, rejoué sous LC_ALL=C par scripts/test-all.d/3282-archives-locale-c.sh.
import re as _re_3282
_mod_3282 = (ROOT / "runtime/src/archive/mod.rs").read_text(encoding="utf-8")
_code_3282 = _mod_3282.split("#[cfg(test)]")[0]
_tools_3282 = (ROOT / "runtime/src/archive/tools.rs").read_text(encoding="utf-8")
ok("fn sept_z_nom_accentue_locale_heritee_3282" in _mod_3282
   and "pub fn archiver(" in _tools_3282 and '"LC_ALL", "C.UTF-8"' in _tools_3282,
   "outils d'archive sous locale UTF-8 fixe, test du nom accentué en 7z (#3282)")
ok(not _re_3282.search(r"Command::new\(", _code_3282)
   and len(_re_3282.findall(r"tools::archiver\(", _code_3282)) == 6,
   "les six appels d'outils d'archive passent par tools::archiver (#3282)")
_step_3282 = (SCRIPTS / "test-all.d/3282-archives-locale-c.sh").read_text(encoding="utf-8")
ok("LC_ALL=C " in _step_3282 and "sept_z_nom_accentue_locale_heritee_3282" in _step_3282,
   "le test du nom accentué est rejoué sous LC_ALL=C (#3282)")
