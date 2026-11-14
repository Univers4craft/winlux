# Élévation demandée par les installateurs que Winlux déroule lui-même (#2180).
# Le comportement (refus = 1223 sans rien installer, accord, compte administrateur
# sans question) est vérifié par scripts/test-elevation-installateurs.sh en CI.
_el = (ROOT / "runtime/src/setup/elevation.rs").read_text(encoding="utf-8")
_mi = (ROOT / "runtime/src/msi/install.rs").read_text(encoding="utf-8")
_bu = (ROOT / "runtime/src/setup/burn.rs").read_text(encoding="utf-8")
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok("pub fn require(" in _el and "pub fn allusers(" in _mi and "pub fn per_machine(" in _bu,
   "élévation : niveau demandé lu dans le MSI (ALLUSERS), le bundle Burn et le manifeste (#2180)")
ok("scripts/test-elevation-installateurs.sh" in _ci,
   "élévation : MSI et NSIS refusés sans élévation, installés avec, vérifiés en CI (#2180)")
