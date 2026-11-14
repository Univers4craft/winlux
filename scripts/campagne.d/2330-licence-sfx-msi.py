# Contrat du MSI porté par un auto-extractible 7-Zip (Acrobat Reader), montré avant d'installer (#2330).
_s = (ROOT / "runtime/src/setup/sevenz.rs").read_text(encoding="utf-8")
ok("license::require_acceptance" in _s and "fn contrat_du_msi_porte_par_l_auto_extractible" in _s,
   "auto-extractible 7-Zip : contrat du MSI porté accepté avant l'installation, refus = 1602 (#2330)")
