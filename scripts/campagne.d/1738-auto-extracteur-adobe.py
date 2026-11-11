# Archive d'installateur (setup.exe / setup.ini, auto-extracteur d'Adobe) suivie, jamais copiée ni annoncée installée (#1738).
sz = (ROOT / "runtime/src/setup/sevenz.rs").read_text(encoding="utf-8")
ok("pub fn is_setup_bundle" in sz and "fn run_bundle_setup" in sz
   and "à lancer, pas à copier" in sz,
   "auto-extracteur : setup.ini/setup.exe suivis, jamais copiés dans Program Files (#1738)")
