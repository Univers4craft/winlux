// Témoin de System.IO dans la mscorlib Winlux (#2012, #2013) et de
// System.Random (#2015).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 13) de la première vérification qui échoue.
// La 13 appelle les primitives de System.IO.Natif par réflexion avec des
// bornes qui débordent : elles doivent refuser (-87,
// ERROR_INVALID_PARAMETER) sans toucher la mémoire hors du tableau. Un objet
// qui n'est pas un tableau d'octets est refusé dès Invoke (ArgumentException,
// #2037).
// Corsair.Service.exe (iCUE 4.18) appelait TextWriter::Write (9 fois),
// Path::Combine (9), FileStream::.ctor (8), File::Exists (8),
// BinaryWriter::Write (8), Stream::Read/Write, StringWriter, StringReader,
// MemoryStream, Directory::GetFiles/CreateDirectory et Random::Next.
using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;

namespace Temoins
{
    static class Fichiers
    {
        static int Main()
        {
            // Règles de chemin de Windows, même sur notre noyau Linux.
            if (Path.Combine(@"C:\Users", "x.txt") != @"C:\Users\x.txt" || Path.Combine(@"C:\a\", @"D:\b") != @"D:\b") return 1;
            if (Path.GetDirectoryName(@"C:\a\b.txt") != @"C:\a" || Path.GetDirectoryName(@"C:\a.txt") != @"C:\" || Path.GetDirectoryName(@"C:\") != null) return 1;
            if (Path.GetFileName(@"C:\a\b.txt") != "b.txt" || Path.GetExtension(@"C:\a.b\c") != "" || Path.GetFileNameWithoutExtension("x.tar.gz") != "x.tar") return 2;
            if (Path.ChangeExtension(@"C:\a\b.txt", "log") != @"C:\a\b.log" || !Path.IsPathRooted(@"\\serveur\partage") || Path.GetPathRoot(@"\\serveur\partage\x") != @"\\serveur\partage") return 2;
            if (Path.DirectorySeparatorChar != '\\' || Path.GetInvalidFileNameChars().Length != 41) return 2;

            // Le dossier temporaire est un chemin Windows qui existe.
            string temp = Path.GetTempPath();
            if (temp.Length < 3 || temp[1] != ':' || !Directory.Exists(temp)) return 3;
            string dossier = Path.Combine(temp, "temoin-fichiers-" + Environment.TickCount);
            Directory.CreateDirectory(Path.Combine(dossier, @"sous\profond"));
            if (!Directory.Exists(Path.Combine(dossier, "sous"))) return 3;

            // Texte : UTF-8 sans marque par défaut, accents relus.
            string f = Path.Combine(dossier, "été.txt");
            File.WriteAllText(f, "première ligne\r\nseconde\nfin");
            if (!File.Exists(f) || File.Exists(dossier) || new FileInfo(f).Length != 28) return 4;
            string[] lignes = File.ReadAllLines(f);
            if (lignes.Length != 3 || lignes[0] != "première ligne" || lignes[2] != "fin") return 4;
            File.AppendAllText(f, "€");
            if (File.ReadAllText(f) != "première ligne\r\nseconde\nfin€") return 4;

            // Lecture par petits morceaux : un caractère UTF-8 coupé entre
            // deux lectures doit être recollé.
            StringBuilder long_ = new StringBuilder();
            for (int i = 0; i < 300; i++) long_.Append("é€");
            using (StreamWriter w = new StreamWriter(f, false, new UTF8Encoding(true))) w.Write(long_.ToString());
            byte[] brut = File.ReadAllBytes(f);
            if (brut.Length != 3 + 300 * 5 || brut[0] != 0xEF) return 5;
            using (StreamReader r = new StreamReader(new FileStream(f, FileMode.Open, FileAccess.Read), Encoding.UTF8, true, 128))
                if (r.ReadToEnd() != long_.ToString()) return 5;

            // FileStream : position, déplacement, taille, ajout.
            string bin = Path.Combine(dossier, "données.bin");
            using (FileStream fs = new FileStream(bin, FileMode.Create))
            {
                fs.Write(new byte[] { 1, 2, 3, 4, 5 }, 0, 5);
                if (fs.Position != 5 || fs.Length != 5) return 6;
                fs.Seek(1, SeekOrigin.Begin);
                if (fs.ReadByte() != 2) return 6;
                fs.SetLength(3);
                if (fs.Length != 3) return 6;
            }
            using (FileStream fs = new FileStream(bin, FileMode.Append)) fs.WriteByte(9);
            byte[] lu = File.ReadAllBytes(bin);
            if (lu.Length != 4 || lu[2] != 3 || lu[3] != 9) return 6;

            // Binaire : aller-retour BinaryWriter → BinaryReader.
            MemoryStream ms = new MemoryStream();
            using (BinaryWriter bw = new BinaryWriter(ms, Encoding.UTF8, true))
            {
                bw.Write(0x12345678); bw.Write(-2L); bw.Write(true); bw.Write(1.5); bw.Write("chaîne"); bw.Write((ushort)65535);
            }
            if (ms.Length != 4 + 8 + 1 + 8 + 1 + 7 + 2) return 7;
            ms.Position = 0;
            BinaryReader br = new BinaryReader(ms);
            if (br.ReadInt32() != 0x12345678 || br.ReadInt64() != -2 || !br.ReadBoolean() || br.ReadDouble() != 1.5) return 7;
            if (br.ReadString() != "chaîne" || br.ReadUInt16() != 65535) return 7;
            try { br.ReadByte(); return 7; } catch (EndOfStreamException) { }

            // Texte en mémoire.
            StringWriter sw = new StringWriter();
            sw.Write("a{0}", 1); sw.WriteLine(true); sw.Write('z');
            if (sw.ToString() != "a1True\r\nz") return 8;
            StringReader sr = new StringReader("un\r\ndeux\nfin");
            if (sr.ReadLine() != "un" || sr.Peek() != 'd' || sr.ReadLine() != "deux" || sr.ReadToEnd() != "fin" || sr.ReadLine() != null) return 8;

            // Dossiers : listes, motifs, récursion, suppression.
            File.WriteAllText(Path.Combine(dossier, @"sous\profond\c.log"), "x");
            string[] txt = Directory.GetFiles(dossier, "*.txt");
            if (txt.Length != 1 || Path.GetFileName(txt[0]) != "été.txt") return 9;
            if (Directory.GetFiles(dossier, "*", SearchOption.AllDirectories).Length != 3 || Directory.GetDirectories(dossier).Length != 1) return 9;
            DirectoryInfo di = new DirectoryInfo(dossier);
            if (di.Name != Path.GetFileName(dossier) || di.GetFiles().Length != 2 || !di.Exists) return 9;

            // Erreurs : exceptions de .NET, pas de plantage.
            try { File.ReadAllText(Path.Combine(dossier, "absent.txt")); return 10; }
            catch (FileNotFoundException e) { if (e.FileName == null || !e.FileName.EndsWith("absent.txt")) return 10; }
            try { File.ReadAllText(Path.Combine(dossier, @"pas\là.txt")); return 10; }
            catch (DirectoryNotFoundException) { }
            try { new FileStream(f, FileMode.CreateNew); return 10; }
            catch (IOException) { }
            // Le mode de partage (FileShare.None) n'est pas encore appliqué
            // par CreateFileW : #2014.

            File.Copy(f, f + ".copie");
            File.Move(f + ".copie", f + ".déplacé");
            if (File.Exists(f + ".copie") || !File.Exists(f + ".déplacé")) return 11;
            Directory.Delete(dossier, true);
            if (Directory.Exists(dossier)) return 11;

            Random a = new Random(42), b = new Random(42);
            int x = a.Next(10, 20);
            if (x != b.Next(10, 20) || x < 10 || x >= 20 || a.NextDouble() >= 1.0 || new Random().Next(5) >= 5) return 12;

            Type natif = typeof(FileStream).Assembly.GetType("System.IO.Natif");
            MethodInfo lire = natif == null ? null : natif.GetMethod("Lire", BindingFlags.NonPublic | BindingFlags.Static);
            MethodInfo ecrire = natif == null ? null : natif.GetMethod("Ecrire", BindingFlags.NonPublic | BindingFlags.Static);
            if (lire == null || ecrire == null) return 13;
            string sonde = Path.Combine(temp, "temoin-sonde-" + Environment.TickCount);
            using (FileStream fs = new FileStream(sonde, FileMode.Create))
            {
                fs.Write(new byte[64], 0, 64);
                fs.Flush();
                fs.Position = 0;
                long h = (long)typeof(FileStream).GetField("_h", BindingFlags.NonPublic | BindingFlags.Instance).GetValue(fs);
                object[][] essais = {
                    new object[] { h, new byte[4], int.MaxValue, int.MaxValue },
                    new object[] { h, new byte[4], 2, int.MaxValue },
                    new object[] { h, new byte[4], -1, 2 },
                };
                foreach (object[] e in essais)
                {
                    if ((int)lire.Invoke(null, e) != -87 || (int)ecrire.Invoke(null, e) != -87) return 13;
                }
                // Un autre objet qu'un byte[] : Invoke le refuse avant (#2037).
                object[][] faux = {
                    new object[] { h, "pas un tableau", 0, 8 },
                    new object[] { h, new int[4], 0, 8 },
                };
                foreach (object[] e in faux)
                {
                    try { lire.Invoke(null, e); return 13; } catch (ArgumentException) { }
                    try { ecrire.Invoke(null, e); return 13; } catch (ArgumentException) { }
                }
                if ((int)lire.Invoke(null, new object[] { h, new byte[8], 0, 8 }) != 8) return 13;
            }
            File.Delete(sonde);
            return 200;
        }
    }
}
