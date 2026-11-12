# NetFwPolicy2 et NetFwRule existent : la règle de pare-feu de SteamService /install est acceptée, x86 compris (#1919).
_n = (ROOT / "runtime/src/dlls/netfw2.rs").read_text(encoding="utf-8")
_x = (ROOT / "runtime/src/wow32/com_netfw.rs").read_text(encoding="utf-8")
ok("fn regle_de_pare_feu_acceptee_et_retrouvee" in _n
   and "&INETFWPOLICY2," in _x and "&INETFWRULE," in _x,
   "pare-feu : CLSID_NetFwPolicy2 et NetFwRule servis, x86 compris (#1919)")
