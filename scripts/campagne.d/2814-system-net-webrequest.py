# System.Net pour le téléchargeur de Paint.NET : WebRequest, HttpWebRequest
# (AddRange, Proxy) et HttpWebResponse servis par notre mscorlib sur WinHTTP,
# témoin Reseau lancé contre un serveur local (code 200, #2814).
_r = (ROOT / "bcl/mscorlib/Reseau.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/Reseau.cs").read_text(encoding="utf-8")
_s = (ROOT / "scripts/test-webrequest.sh").read_text(encoding="utf-8")
ok("public class HttpWebRequest : WebRequest" in _r and "public void AddRange(long from, long to)" in _r
   and "public class WebProxy : IWebProxy" in _r and "Code de sortie attendu : 200." in _t
   and "WINLUX_TEMOIN_URL" in _s,
   "System.Net : HttpWebRequest, AddRange et WebProxy sur WinHTTP, témoin réseau (#2814)")
