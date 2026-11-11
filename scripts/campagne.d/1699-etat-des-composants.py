# #1699 : MsiGetComponentState ne déclare pas posé un composant que
# l'installation ne retient pas (WiX programmait alors le réglage d'un
# service jamais inscrit, ExecServiceConfig rendait 1603).
msi_session = (ROOT / "runtime/src/msi/session.rs").read_text(encoding="utf-8")
msi_actions = (ROOT / "runtime/src/msi/actions.rs").read_text(encoding="utf-8")
ok('"MsiGetComponentStateW" => fn msi_get_component_state_w' in msi_session
   and "INSTALLSTATE_UNKNOWN" in msi_session
   and "super::session::COMPONENTS_KEY" in msi_actions,
   "actions personnalisées : état réel des composants retenus (#1699)")
