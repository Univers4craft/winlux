# Les tests squirrel qui écrivent dans le dossier temporaire du profil
# prennent le verrou d'environnement des tests (#2485, même motif que #2466).
_s = (ROOT / "runtime/src/setup/squirrel.rs").read_text()
_t = _s[_s.index("mod tests"):]
ok(_t.count("crate::mounts::TEST_ENV_LOCK") >= 2,
   "tests squirrel sous le verrou d'environnement (#2485)")
