# Moteur .NET maison : System.Windows.Forms.MessageBox.Show est servi (témoin
# BoiteMessage, code 200) et sa boîte s'affiche sous Xvfb (#2753).
_b = (ROOT / "runtime/src/clr/bcl.rs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/BoiteMessage.cs").read_text(encoding="utf-8")
ok("fn message_box_show(" in _b and "Code de sortie attendu : 200." in _t
   and (ROOT / "scripts/test-all.d/2753-boite-message-winforms.sh").exists(),
   "WinForms : MessageBox.Show servi par le moteur .NET, témoin et test Xvfb (#2753)")
