# WinForms, mur 5a : Label, Button et ProgressBar en vraies fenêtres enfants,
# AutoSize, AcceptButton/CancelButton, vérifiés par le témoin Controles
# (code 200, #2795).
_c = (ROOT / "bcl/mscorlib/Controles.cs").read_text(encoding="utf-8")
_f = (ROOT / "bcl/mscorlib/Fenetres.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/Controles.cs").read_text(encoding="utf-8")
ok("public class Label : Control" in _c and "public class Button : ButtonBase, IButtonControl" in _c
   and "public class ProgressBar : Control" in _c and "public virtual bool AutoSize" in _f
   and "public IButtonControl AcceptButton" in _f and "Code de sortie attendu : 200." in _t,
   "WinForms : Label, Button, ProgressBar, AutoSize et AcceptButton servis par notre mscorlib, témoin (#2795)")
