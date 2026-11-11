# NSIS : $SYSDIR d'un installateur x86 vise SysWOW64, sauf redirection coupée (#1800).
_n = (ROOT / "runtime/src/setup/nsis.rs").read_text(encoding="utf-8")
ok("0x25 if is_x86_stub(self.stub) && !self.fs_redirect_off" in _n
   and "fn fs_redirection_call(" in _n and "fn sysdir_d_un_installateur_x86" in _n,
   "NSIS : $SYSDIR redirigé vers SysWOW64 pour un installateur x86 (#1800)")
