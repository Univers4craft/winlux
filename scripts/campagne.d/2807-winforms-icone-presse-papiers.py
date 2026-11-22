# WinForms, mur 5c : System.Drawing.Icon (.ico en HICON), Form.Icon,
# Application.ProductVersion et Clipboard, vérifiés par le témoin Icones
# (code 200, #2807).
_i = (ROOT / "bcl/mscorlib/Icones.cs").read_text(encoding="utf-8")
_f = (ROOT / "bcl/mscorlib/Fenetres.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/Icones.cs").read_text(encoding="utf-8")
ok("public sealed class Icon" in _i and "public sealed class Clipboard" in _i
   and "public static string ProductVersion" in _f and "public Icon Icon" in _f
   and "Code de sortie attendu : 200." in _t,
   "WinForms : Icon, Form.Icon, ProductVersion et Clipboard servis par notre mscorlib, témoin (#2807)")
