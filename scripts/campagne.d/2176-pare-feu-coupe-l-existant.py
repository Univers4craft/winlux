# Pare-feu : bloquer une application coupe aussi ses connexions déjà ouvertes (#2176).
_fw = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
ok("fn cut_connections(" in _fw and "SOCK_DESTROY" in _fw and "cut_blocked_groups(&cfg)" in _fw, "connexions ouvertes coupées au blocage (#2176)")
ok("head.append(&mut app_blocks)" in _fw and _fw.index("head.append(&mut app_blocks)") < _fw.index('head.push("ct state established,related accept"'), "blocages par application avant les connexions établies (#2176)")
