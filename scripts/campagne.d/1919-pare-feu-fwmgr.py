# HNetCfg.FwMgr existe : l'autorisation de pare-feu de SteamService est acceptée, en x64 comme en x86 (#1919).
_o = (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
_n = (ROOT / "runtime/src/dlls/netfw.rs").read_text(encoding="utf-8")
_c = (ROOT / "runtime/src/wow32/com.rs").read_text(encoding="utf-8")
ok("super::netfw::create_builtin(&guid, &iid, out)" in _o
   and "fn autorisation_de_programme_acceptee_et_retrouvee" in _n
   and ".chain(super::com_netfw::ALL.iter())" in _c,
   "pare-feu : CLSID_NetFwMgr et NetFwAuthorizedApplication servis, x86 compris (#1919)")
