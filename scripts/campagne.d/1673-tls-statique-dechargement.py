# #1673 : un thread neuf copie le TLS statique sous le verrou de MODULES et ignore un module en déchargement.
p = (runtime_dlls.parent / "process.rs").read_text(encoding="utf-8")
ok("fn fill_static_tls(mods: &[Module], array: usize)" in p and "*i == 0 || m.refcount != 0" in p,
   "TLS statique : module en cours de déchargement ignoré, copie sous le verrou (#1673)")
