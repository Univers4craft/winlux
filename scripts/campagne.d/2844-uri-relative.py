# Uri : TryCreate, références relatives et résolution RFC 3986 §5.2 servies
# par notre mscorlib ; témoin UriRelative (exemples de la §5.4.1, code 200,
# #2844).
_r = (ROOT / "bcl/mscorlib/Reseau.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/UriRelative.cs").read_text(encoding="utf-8")
ok("public static bool TryCreate(string uriString, UriKind uriKind, out Uri result)" in _r
   and "public Uri(Uri baseUri, string relativeUri)" in _r and "private static string SansPoints(string entree)" in _r
   and "Code de sortie attendu : 200." in _t,
   "Uri : TryCreate et références relatives résolues (RFC 3986), témoin (#2844)")
