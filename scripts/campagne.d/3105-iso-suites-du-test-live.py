# Chaque exécutable que test-runtime.sh lance depuis $testdir doit être copié
# dans /opt/winlux/tests par build-live-iso.sh, sinon le test live de l'ISO
# échoue et bloque le workflow Release (#3105 : libelle_vide_suite, palette_suite).
import re as _re
_tr = (ROOT / "scripts" / "test-runtime.sh").read_text(encoding="utf-8")
_iso = (ROOT / "scripts" / "build-live-iso.sh").read_text(encoding="utf-8")
_lances = set(_re.findall(r'\$testdir/([A-Za-z0-9_]+)\.exe', _tr))
for _m in _re.finditer(r'for name in ([^;\n]+); do\n\s*exe="\$testdir/\$name\.exe"', _tr):
    _lances |= set(_m.group(1).split())
_lances.discard("")
_copies = set()
for _m in _re.finditer(r'for name in ([^;\n]+); do\n\s*cp "\$root/target/x86_64-pc-windows-msvc/release/\$name\.exe" "\$stage/opt/winlux/tests/\$name\.exe"', _iso):
    _copies |= set(_m.group(1).split())
_manquants = sorted(_lances - _copies)
print("suites lancées par test-runtime.sh :", len(_lances), "copiées dans l'ISO :", len(_copies), "manquantes :", _manquants)
ok(len(_lances) > 30 and len(_copies) > 30 and not _manquants,
   "chaque suite PE lancée par le test live est copiée dans l'image (#3105)")
