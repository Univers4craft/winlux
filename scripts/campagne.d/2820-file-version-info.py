# System.Diagnostics.FileVersionInfo pour le téléchargeur de Paint.NET : la
# ressource de version lue par notre version.dll, témoin VersionFichier qui
# lit sa propre version (code 200, #2820).
_v = (ROOT / "bcl/mscorlib/VersionFichier.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/VersionFichier.cs").read_text(encoding="utf-8")
ok("public static FileVersionInfo GetVersionInfo(string fileName)" in _v and "public int FileMajorPart" in _v
   and "Code de sortie attendu : 200." in _t,
   "FileVersionInfo : GetVersionInfo et parties de version servis par notre mscorlib, témoin (#2820)")
