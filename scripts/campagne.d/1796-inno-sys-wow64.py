# Inno Setup : {sys} en mode 32 bits vise SysWOW64, pas System32 (#1796).
_inno = (ROOT / "runtime/src/setup/inno.rs").read_text(encoding="utf-8")
ok('"sys" if !self.x64 =>' in _inno and "fn sys_en_mode_32_bits_va_dans_syswow64" in _inno,
   "Inno : {sys} redirigé vers SysWOW64 en mode 32 bits (#1796)")
