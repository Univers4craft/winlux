# WINLUX_TRACE suit les mémoires partagées nommées : nom, taille demandée et réelle, vues, VirtualQuery et NtQuerySection (#1694).
_m = (ROOT / "runtime/src/dlls/mmap.rs").read_text(encoding="utf-8")
ok('"CreateFileMapping({n}' in _m and '"OpenFileMapping({}' in _m and '"MapViewOfFile({n}' in _m,
   "trace des sections nommées et de leurs vues (#1694)")
