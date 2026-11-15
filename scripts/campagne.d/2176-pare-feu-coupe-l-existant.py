# Pare-feu : bloquer une application coupe aussi ses connexions déjà ouvertes (#2176).
_fw = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
ok("fn cut_connections(" in _fw and "fn cut_blocked_groups(" in _fw and "SOCK_DESTROY" in _fw, "connexions ouvertes coupées au blocage (#2176)")
ok("fn blocked_app_cuts_established_but_not_loopback(" in _fw, "test : le blocage passe avant les connexions établies, la boucle locale reste ouverte (#2176)")
ok("fn open_connection_is_cut(" in _fw, "test réel : une connexion ouverte est coupée (#2176)")
