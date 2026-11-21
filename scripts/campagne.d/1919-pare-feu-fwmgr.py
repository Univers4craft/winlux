# HNetCfg.FwMgr existe : l'autorisation de pare-feu de SteamService est acceptée, en x64 comme en x86 (#1919).
# Vérifie la fabrique, le test d'autorisation et la table x86 des interfaces.
_n = (ROOT / "runtime/src/dlls/netfw.rs").read_text(encoding="utf-8")
ok("fn create_builtin(" in _n
   and "fn autorisation_de_programme_acceptee_et_retrouvee(" in _n
   and (ROOT / "runtime/src/wow32/com_netfw.rs").is_file(),
   "pare-feu : CLSID_NetFwMgr et NetFwAuthorizedApplication servis, x86 compris (#1919)")
