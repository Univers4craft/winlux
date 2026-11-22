# WinForms, mur 5b : System.Drawing.Font sur de vraies polices GDI,
# Control.Font ambiante et mise à l'échelle automatique, vérifiées par le
# témoin Polices (code 200, #2801).
_d = (ROOT / "bcl/mscorlib/Dessin.cs").read_text(encoding="utf-8")
_f = (ROOT / "bcl/mscorlib/Fenetres.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/Polices.cs").read_text(encoding="utf-8")
ok("public sealed class Font" in _d and "public virtual Font Font" in _f
   and "public SizeF CurrentAutoScaleDimensions" in _f and "Code de sortie attendu : 200." in _t,
   "WinForms : Font, Control.Font et AutoScale servis par notre mscorlib, témoin (#2801)")
