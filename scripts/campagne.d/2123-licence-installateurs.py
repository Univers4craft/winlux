# Contrat de licence affiché et accepté avant toute installation (#2123 à #2126).
_lic = (ROOT / "runtime/src/setup/license.rs").read_text(encoding="utf-8")
_wiz = (ROOT / "scripts/winlux-setup-wizard.py").read_text(encoding="utf-8")
_open = (ROOT / "integration/winlux-open").read_text(encoding="utf-8")
ok("crate::msi::install::license_text" in _lic and "nsis::license" in _lic
   and "inno::license" in _lic and "burn::license" in _lic,
   "Contrat de licence lu pour .msi, NSIS, Inno et Burn (#2123)")
ok("refuse.set_active(True)" in _wiz and "if license and not accept.get_active():" in _wiz,
   "Assistant : « Je n'accepte pas » par défaut, rien installé sans acceptation (#2123)")
ok("--setup --license" in _open and "exit 1602" in _open,
   "winlux-open : pas d'installation silencieuse d'un paquet à contrat sans assistant (#2123)")
