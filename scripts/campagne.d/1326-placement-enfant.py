# GetWindowPlacement d'une fenêtre enfant : rcNormalPosition relatif au
# parent ; 7-Zip en déduit la place de sa zone d'adresse (#1326).
ok("tombait 200 pixels trop bas (#1326)" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "GetWindowPlacement d'un enfant relatif au parent" in (ROOT / "testc/wow32/dlg32.c").read_text(encoding="utf-8"),
   "GetWindowPlacement d'un enfant relatif au parent (#1326)")
