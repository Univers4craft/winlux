# Pare-feu : « apply » rend la main une fois la liste des programmes filtrés armée par le service (#2446).
_fw = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
ok("fn wait_armed(" in _fw and "const ARMED:" in _fw, "apply attend que la liste filtrée soit armée (#2446)")
ok("fn apply_attend_la_liste_armee(" in _fw, "test : apply attend la liste armée, avec une limite (#2446)")
