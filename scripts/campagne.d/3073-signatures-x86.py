# Pont x86 : l'index des signatures (SIGS de build.rs) est rangé par nom
# de fonction, sans un format! par ligne à chaque lancement d'un programme
# 32 bits ; le test unitaire le compare à l'ancien index (Gain #3073).
_t = (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
ok("fn index_des_signatures_identique_a_l_ancien()" in _t and "by_name_module" in _t,
   "pont x86 : index des signatures sans allocation, réponses identiques (#3073)")
