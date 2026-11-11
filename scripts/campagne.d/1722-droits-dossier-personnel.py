# Un installeur ne rend jamais un dossier (ni le dossier personnel) inscriptible par tous (#1722, #1723).
setup = ROOT / "runtime/src/setup"
src = {p.name: p.read_text(encoding="utf-8") for p in setup.glob("*.rs")}
m = src["mod.rs"]
ok(not any("set_readonly(false)" in t for t in src.values()),
   "installeur : pas de set_readonly(false), qui ajoute l'écriture pour tous (#1722)")
ok("ErrorKind::IsADirectory" in m and "mode | 0o200" in m,
   "installeur : un dossier n'est jamais ouvert comme fichier, écriture rendue au seul propriétaire (#1722)")
ok("fn droits_preserves_et_echec_nettoye" in m and "inst.rollback()" in m,
   "installeur : droits du dossier personnel testés, installation en échec nettoyée (#1723)")
ok("fn dest(" in src["inno.rs"],
   "Inno Setup : destination {code:…} sans racine rattachée à {app} (#1723)")
ok('"localappdata" => local("")' in src["inno.rs"]
   and 'crate::paths::profile_path("AppData\\\\Local")' in src["nsis.rs"]
   and 'profile("AppData\\\\Local")' in src["burn.rs"],
   "Inno, NSIS, Burn : dossiers du profil sous C:\\Users\\<compte> (#1724)")
