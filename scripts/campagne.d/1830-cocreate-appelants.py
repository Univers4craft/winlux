# L'échec de CoCreateInstance nomme l'interface et les appelants (WinRAR, CLSID mal formé, #1830).
_src = (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
ok("appelants {}" in _src and "fn caller_text" in _src, "CoCreateInstance journalise interface et appelants en cas d'échec (#1830)")
