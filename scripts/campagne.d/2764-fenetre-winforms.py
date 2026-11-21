# WinForms, mur 1 : Application et Form de base dans notre mscorlib, témoin
# FenetreBase (code 200) et vraie fenêtre fermée par l'utilisateur sous Xvfb (#2764).
_f = (ROOT / "bcl/mscorlib/Fenetres.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/FenetreBase.cs").read_text(encoding="utf-8")
ok("public sealed class Application" in _f and "public class Form : ContainerControl" in _f
   and "Code de sortie attendu : 200." in _t
   and (ROOT / "scripts/test-all.d/2764-fenetre-winforms.sh").exists(),
   "WinForms : Application et Form de base, témoin et test Xvfb (#2764)")
