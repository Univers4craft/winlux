# Une demande AFD_POLL est retirée de la liste avant sa complétion : plus annulable ensuite (#1836).
_src = (ROOT / "runtime/src/dlls/afd.rs").read_text(encoding="utf-8")
ok("STATUS_NOT_FOUND (#1836)" in _src, "AFD_POLL retirée de la liste avant la complétion (#1836)")
