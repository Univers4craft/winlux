# Paquets MSI de la fiche de banc 833 construits depuis le dépôt (wixl, msitools, nsis, mingw-w64), publiés par la CI.
ok("WINLUX_MSI_OUT" in (ROOT / "scripts/test-msi-actions.sh").read_text()
   and "WINLUX_MSI_OUT" in (ROOT / "scripts/test-msi-scripts.sh").read_text()
   and (ROOT / "scripts/construire-paquets-833.sh").exists()
   and "construire-paquets-833.sh" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "paquets MSI du banc 833 reproductibles depuis le dépôt (#833)")
