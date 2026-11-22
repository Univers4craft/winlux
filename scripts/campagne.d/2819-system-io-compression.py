# System.IO.Compression pour le téléchargeur de Paint.NET : ZipArchive en
# lecture et DeflateStream (RFC 1951) servis par notre mscorlib, témoin
# Compression (code 200, #2819).
_c = (ROOT / "bcl/mscorlib/Compression.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/Compression.cs").read_text(encoding="utf-8")
ok("public class ZipArchive : IDisposable" in _c and "public class DeflateStream : Stream" in _c
   and "public sealed class InvalidDataException" in _c and "Code de sortie attendu : 200." in _t,
   "System.IO.Compression : ZipArchive et DeflateStream servis par notre mscorlib, témoin (#2819)")
