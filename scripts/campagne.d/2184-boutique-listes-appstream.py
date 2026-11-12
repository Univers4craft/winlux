# Boutique sur système neuf : listes apt et fiches AppStream téléchargées
# sans action (#2184, #2185) ; fin d'installation expliquée (#2186).
_s = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
_iso = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
_conf = (ROOT / "integration/apt/50winlux-appstream").read_text(encoding="utf-8")
_rs = (ROOT / "testexe/src/shell/store.rs").read_text(encoding="utf-8")
ok('not any(lists.glob("*_Packages*"))' in _s,
   "Boutique : listes apt absentes, apt-get update lancé à l'ouverture (#2184)")
ok("deb::DEP-11" in _conf and "50winlux-appstream" in _iso and 'glob("*_dep11_Components-*.yml*")' in _s,
   "Boutique : fiches AppStream des dépôts téléchargées par apt et lues par la Boutique (#2185)")
ok('"apt_triggers"' in _s and "BUSY_AT = GetTickCount() | 1;" in _rs,
   "Boutique : déclencheurs de dpkg expliqués, clic pendant une opération expliqué dans le bandeau (#2186)")
