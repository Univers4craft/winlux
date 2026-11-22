# winlux --inspect projette le programme (ImageFile) au lieu de le copier en
# mémoire : un .exe de 300 Mo s'inspecte sous 150 Mo de données (#2918).
_t = (SCRIPTS / "test-inspect-projete.sh").read_text(encoding="utf-8")
ok("inspecté sous 150 Mo de données" in _t
   and "test-inspect-projete.sh" in (SCRIPTS / "test-all.d/2918-inspection-projetee.sh").read_text(encoding="utf-8"),
   "winlux --inspect : un .exe de 300 Mo s'inspecte sous 150 Mo de données (#2918)")
