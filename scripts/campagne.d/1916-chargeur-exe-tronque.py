# Chargeur : un .exe tronqué (section au-delà de la fin du fichier) est refusé proprement, sans panique (#1916).
_l = (ROOT / "runtime/src/loader.rs").read_text(encoding="utf-8")
ok("fichier tronqué ({} octets), la section {} commence à {start} (image invalide)" in _l
   and "s.raw_ptr as usize >= file.len()" in _l,
   "chargeur : .exe tronqué refusé proprement, sans panique (#1916)")
