# Bundles WiX Burn (#1555) : variables du manifeste et de la ligne de commande, TargetDir choisi, aucun paquet installé = échec, --setup --info.
_burn = (ROOT / "runtime/src/setup/burn.rs").read_text()
ok("pub fn initial_vars" in _burn and "aucun paquet installé" in _burn
   and "burn::info(&host)" in (ROOT / "runtime/src/setup/mod.rs").read_text()
   and 'info["kind"] == "burn"' in (ROOT / "scripts/winlux-setup-wizard.py").read_text(),
   "bundle Burn : variables par défaut et en ligne de commande, jamais de faux succès (#1555)")
