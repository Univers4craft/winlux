# Boutique : la désinstallation retire ce que l'installation a apporté, même
# retenu par une suggestion, et jamais les compagnons d'un autre logiciel (#2189).
_s = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
ok('state.setdefault("apt", {})[ref] = sorted(set(self.installed()) - before - {ref})' in _s,
   "Boutique : paquets apportés par chaque installation notés (#2189)")
ok("self.autoremovable(suggests=False) & brought" in _s and "unneeded -= protected" in _s,
   "Boutique : désinstallation sans reste, compagnons des logiciels restants protégés (#2189)")
