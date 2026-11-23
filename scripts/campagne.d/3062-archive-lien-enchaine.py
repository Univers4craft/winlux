# #3062 : une archive non fiable ne doit laisser aucun lien symbolique qui
# mène hors du dossier d'extraction, même en passant par un autre lien de
# l'archive (`a → .` puis `x → a/..`). Le comportement est gardé par les
# tests unitaires nommés (tar, zip, et la résolution sur le disque).
_arch = ROOT / "runtime/src/archive"
_tar = (_arch / "tar.rs").read_text(encoding="utf-8")
_zip = (_arch / "zip.rs").read_text(encoding="utf-8")
_util = (_arch / "util.rs").read_text(encoding="utf-8")
ok("fn lien_enchaine_hors_du_dossier_retire_3062" in _tar,
   "tar : un lien enchaîné qui sort du dossier d'extraction est retiré (#3062)")
ok("fn lien_enchaine_hors_du_dossier_retire_3062" in _zip,
   "zip : un lien enchaîné qui sort du dossier d'extraction est retiré (#3062)")
ok("fn lien_resolu_sur_le_disque_3062" in _util and "pub fn link_escapes(" in _util,
   "un lien d'archive est jugé résolu sur le disque, liens traversés compris (#3062)")
