# #2012 : System.IO dans notre mscorlib (Stream, FileStream, TextWriter,
# StreamReader, BinaryWriter, Directory…) au-dessus de kernel32 ; témoin
# Fichiers.
# #2013 : File et Path suivent les règles de Windows, plus celles de Linux.
# #2015 : System.Random dans notre mscorlib.
bcl = ROOT / "bcl" / "mscorlib"
fichiers = (bcl / "Fichiers.cs").read_text(encoding="utf-8")
ok("public class FileStream : Stream" in (bcl / "Flux.cs").read_text(encoding="utf-8")
   and "public class StreamReader : TextReader" in (bcl / "Lecteurs.cs").read_text(encoding="utf-8")
   and "kernel32::create_file_w" in (runtime_dlls.parent / "clr" / "fichiers.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Fichiers.cs").read_text(encoding="utf-8"),
   "CLR : System.IO dans notre mscorlib au-dessus de kernel32, témoin Fichiers (#2012)")
ok("internal static int LongueurRacine(string path)" in fichiers
   and "fn path_dispatch" not in (runtime_dlls.parent / "clr" / "bcl.rs").read_text(encoding="utf-8"),
   "CLR : File et Path suivent les règles de chemin de Windows (#2013)")
ok("public class Random" in fichiers, "CLR : System.Random dans notre mscorlib (#2015)")
