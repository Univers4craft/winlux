# #1609 : le journal Direct2D décrit le DC où une image est présentée (fenêtre, bitmap, découpe).
src = (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
d2d = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("pub fn describe_dc" in src and "describe_dc(hdc)" in d2d,
   "Direct2D : le journal décrit le DC de présentation (#1609)")
