# Assistant d'installation : pas de « sans exécuter le code » (#1464), menu Démarrer annoncé seulement si un lanceur a été posé (#1465).
_wizard = (ROOT / "scripts/winlux-setup-wizard.py").read_text()
ok("sans exécuter le code" not in _wizard and "Winlux installe ce programme lui-même." in _wizard,
   "assistant d'installation : n'affirme plus ne pas exécuter le code de l'installateur (#1464)")
ok("menu_entries() != state[\"menu\"]" in _wizard,
   "assistant d'installation : menu Démarrer annoncé seulement si un lanceur a été posé (#1465)")
ok("let undo_edits = super::extra::undo(m);" in (ROOT / "runtime/src/msi/install.rs").read_text()
   and "test ! -e \"$pf/Winlux Actions\"" in (ROOT / "scripts/test-msi-actions.sh").read_text(),
   "msiexec : RemoveFile avant les dossiers, dossier vidé retiré (#1466)")
