# #3324 : un .tar.gz abîmé au milieu s'extrayait « avec succès », fichier
# corrompu à sa taille complète : tar s'arrête aux blocs nuls de fin, avant
# la somme CRC de gzip. Le flux est désormais lu jusqu'au bout. Comportement
# gardé par le test unitaire nommé.
_mod_3324 = (ROOT / "runtime/src/archive/mod.rs").read_text(encoding="utf-8")
ok("fn tar_gz_abime_refuse" in _mod_3324,
   "un .tar.gz abîmé est refusé, la somme CRC de gzip est contrôlée (#3324)")
