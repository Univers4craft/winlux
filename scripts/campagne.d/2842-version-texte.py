# System.Version lue en texte : Version(string), Parse, TryParse, ToString(n)
# servis par notre mscorlib ; témoin VersionTexte (code 200, #2842).
_r = (ROOT / "bcl/mscorlib/Reflexion.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/VersionTexte.cs").read_text(encoding="utf-8")
ok("public static bool TryParse(string input, out Version result)" in _r and "public Version(string version)" in _r
   and "public string ToString(int fieldCount)" in _r and "Code de sortie attendu : 200." in _t,
   "Version : Parse, TryParse et ToString(n) servis par notre mscorlib, témoin (#2842)")
