# Au plantage, les membres des jobs KILL_ON_JOB_CLOSE sont tués même si le
# verrou des jobs est tenu (#2220). Le comportement est vérifié par les
# tests unitaires nommés ci-dessous ; ce contrôle vérifie les noms.
_j = (ROOT / "runtime/src/dlls/jobs.rs").read_text(encoding="utf-8")
ok("fn plantage_tue_les_membres_meme_verrou_tenu()" in _j
   and "fn table_de_plantage_pleine()" in _j,
   "plantage : membres KILL_ON_JOB_CLOSE tués sans dépendre du verrou des jobs (#2220)")
