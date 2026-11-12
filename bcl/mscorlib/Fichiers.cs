// System.IO de la mscorlib Winlux : Path, File, Directory, FileInfo,
// DirectoryInfo et les exceptions d'entrée-sortie (#2012, #2013).
// Licence GPL 3. Écrit d'après la documentation publique de .NET Framework
// 4.x (MS Learn : « Path Class », « File Class », « Directory Class »,
// « File path formats on Windows systems ») ; aucun code de Microsoft, de
// Mono ou de Wine.
//
// Les chemins suivent les règles de Windows. L'accès au disque passe par
// les primitives de System.IO.Natif (runtime/src/clr/fichiers.rs), qui
// appellent kernel32 : un programme .NET voit les mêmes chemins, codes
// d'erreur et règles de partage qu'un programme natif.
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Text;

namespace System.IO
{
    public class DirectoryNotFoundException : IOException
    {
        public DirectoryNotFoundException() { }
        public DirectoryNotFoundException(string message) : base(message) { }
        public DirectoryNotFoundException(string message, Exception innerException) : base(message, innerException) { }
    }

    public class PathTooLongException : IOException
    {
        public PathTooLongException() { }
        public PathTooLongException(string message) : base(message) { }
        public PathTooLongException(string message, Exception innerException) : base(message, innerException) { }
    }

    public class EndOfStreamException : IOException
    {
        public EndOfStreamException() : base("Unable to read beyond the end of the stream.") { }
        public EndOfStreamException(string message) : base(message) { }
        public EndOfStreamException(string message, Exception innerException) : base(message, innerException) { }
    }

    public class DriveNotFoundException : IOException
    {
        public DriveNotFoundException() { }
        public DriveNotFoundException(string message) : base(message) { }
    }

    public enum FileMode { CreateNew = 1, Create = 2, Open = 3, OpenOrCreate = 4, Truncate = 5, Append = 6 }

    [Flags]
    public enum FileAccess { Read = 1, Write = 2, ReadWrite = 3 }

    [Flags]
    public enum FileShare { None = 0, Read = 1, Write = 2, ReadWrite = 3, Delete = 4, Inheritable = 16 }

    [Flags]
    public enum FileOptions
    {
        None = 0, Encrypted = 0x4000, DeleteOnClose = 0x04000000, SequentialScan = 0x08000000,
        RandomAccess = 0x10000000, Asynchronous = 0x40000000, WriteThrough = unchecked((int)0x80000000),
    }

    public enum SeekOrigin { Begin = 0, Current = 1, End = 2 }

    public enum SearchOption { TopDirectoryOnly = 0, AllDirectories = 1 }

    [Flags]
    public enum FileAttributes
    {
        ReadOnly = 0x1, Hidden = 0x2, System = 0x4, Directory = 0x10, Archive = 0x20, Device = 0x40,
        Normal = 0x80, Temporary = 0x100, SparseFile = 0x200, ReparsePoint = 0x400, Compressed = 0x800,
        Offline = 0x1000, NotContentIndexed = 0x2000, Encrypted = 0x4000, IntegrityStream = 0x8000,
        NoScrubData = 0x20000,
    }

    // Primitives du moteur (fichiers.rs). Un échec rend -code Win32.
    internal static class Natif
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long Ouvrir(string chemin, int disposition, int acces, int partage, int options);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Lire(long h, byte[] tampon, int debut, int nombre);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Ecrire(long h, byte[] tampon, int debut, int nombre);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long Positionner(long h, long deplacement, int origine);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long Taille(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int FixerTaille(long h, long longueur);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Vider(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Fermer(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long[] Infos(string chemin);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int FixerAttributs(string chemin, int attributs);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int FixerDateEcriture(string chemin, long fileTime);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int CreerDossier(string chemin);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int SupprimerDossier(string chemin);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int SupprimerFichier(string chemin);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Deplacer(string source, string destination, bool remplacer);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Copier(string source, string destination, bool remplacer);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string[] Lister(string motif, int quoi);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string CheminComplet(string chemin);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string DossierTemp();
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string DossierCourant();
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int FixerDossierCourant(string chemin);

        // Code Win32 → exception .NET, avec les messages de .NET Framework.
        internal static Exception Erreur(int code, string chemin)
        {
            if (code < 0) code = -code;
            string p = chemin ?? "";
            switch (code)
            {
                case 2: return new FileNotFoundException("Could not find file '" + p + "'.", chemin);
                case 3: return new DirectoryNotFoundException("Could not find a part of the path '" + p + "'.");
                case 5: return new UnauthorizedAccessException("Access to the path '" + p + "' is denied.");
                case 15: return new DriveNotFoundException("Could not find the drive '" + p + "'.");
                case 32:
                case 33: return new IOException("The process cannot access the file '" + p + "' because it is being used by another process.");
                case 80:
                case 183: return new IOException("The file '" + p + "' already exists.");
                case 87:
                case 123: return new ArgumentException("The path is not of a legal form.");
                case 145: return new IOException("The directory is not empty.");
                case 206: return new PathTooLongException("The specified path, file name, or both are too long.");
                default: return new IOException("I/O error " + code + " on '" + p + "'.");
            }
        }

        internal static void Verifier(int resultat, string chemin)
        {
            if (resultat < 0) throw Erreur(resultat, chemin);
        }
    }

    public static class Path
    {
        public static readonly char DirectorySeparatorChar = '\\';
        public static readonly char AltDirectorySeparatorChar = '/';
        public static readonly char VolumeSeparatorChar = ':';
        public static readonly char PathSeparator = ';';

        private static readonly char[] _invalidesChemin = Construire(false);
        private static readonly char[] _invalidesNom = Construire(true);

        [Obsolete("Please use GetInvalidPathChars or GetInvalidFileNameChars instead.")]
        public static readonly char[] InvalidPathChars = Construire(false);

        private static char[] Construire(bool nom)
        {
            List<char> l = new List<char> { '"', '<', '>', '|', '\0' };
            for (int i = 1; i < 32; i++) l.Add((char)i);
            if (nom) { l.Add(':'); l.Add('*'); l.Add('?'); l.Add('\\'); l.Add('/'); }
            return l.ToArray();
        }

        public static char[] GetInvalidPathChars() { return (char[])_invalidesChemin.Clone(); }
        public static char[] GetInvalidFileNameChars() { return (char[])_invalidesNom.Clone(); }

        internal static bool EstSeparateur(char c) { return c == '\\' || c == '/'; }

        private static void VerifierCaracteres(string path)
        {
            // Une boucle « for » : « foreach » sur une chaîne fait échouer
            // l'émission de notre mscorlib (CS7038).
            for (int k = 0; k < path.Length; k++)
            {
                char c = path[k];
                if (c == '"' || c == '<' || c == '>' || c == '|' || c < 32)
                    throw new ArgumentException("Illegal characters in path.");
            }
        }

        // Longueur de la racine : « C:\ » 3, « C: » 2, « \ » 1,
        // « \\serveur\partage » jusqu'au séparateur après le partage.
        internal static int LongueurRacine(string path)
        {
            int n = path.Length;
            if (n >= 2 && EstSeparateur(path[0]) && EstSeparateur(path[1]))
            {
                int i = 2, parties = 2;
                while (i < n && (!EstSeparateur(path[i]) || --parties > 0)) i++;
                return i;
            }
            if (n >= 2 && path[1] == ':')
                return n >= 3 && EstSeparateur(path[2]) ? 3 : 2;
            if (n >= 1 && EstSeparateur(path[0])) return 1;
            return 0;
        }

        public static bool IsPathRooted(string path)
        {
            if (path == null) return false;
            VerifierCaracteres(path);
            return LongueurRacine(path) > 0;
        }

        public static string GetPathRoot(string path)
        {
            if (path == null) return null;
            VerifierCaracteres(path);
            return path.Substring(0, LongueurRacine(path)).Replace('/', '\\');
        }

        public static string Combine(string path1, string path2)
        {
            if (path1 == null) throw new ArgumentNullException("path1");
            if (path2 == null) throw new ArgumentNullException("path2");
            VerifierCaracteres(path1);
            VerifierCaracteres(path2);
            return Joindre(path1, path2);
        }

        private static string Joindre(string a, string b)
        {
            if (b.Length == 0) return a;
            if (a.Length == 0 || LongueurRacine(b) > 0) return b;
            char fin = a[a.Length - 1];
            if (EstSeparateur(fin) || fin == ':') return a + b;
            return a + "\\" + b;
        }

        public static string Combine(string path1, string path2, string path3)
        {
            return Combine(Combine(path1, path2), path3);
        }

        public static string Combine(string path1, string path2, string path3, string path4)
        {
            return Combine(Combine(Combine(path1, path2), path3), path4);
        }

        public static string Combine(params string[] paths)
        {
            if (paths == null) throw new ArgumentNullException("paths");
            string r = "";
            for (int i = 0; i < paths.Length; i++)
            {
                if (paths[i] == null) throw new ArgumentNullException("paths");
                VerifierCaracteres(paths[i]);
                r = Joindre(r, paths[i]);
            }
            return r;
        }

        public static string GetDirectoryName(string path)
        {
            if (path == null) return null;
            VerifierCaracteres(path);
            if (path.Length == 0) throw new ArgumentException("The path is not of a legal form.");
            int racine = LongueurRacine(path);
            int i = path.Length;
            if (i <= racine) return null;
            while (i > racine && !EstSeparateur(path[--i])) { }
            return path.Substring(0, i).Replace('/', '\\');
        }

        private static int DebutNom(string path)
        {
            int i = path.Length;
            while (i > 0)
            {
                char c = path[i - 1];
                if (EstSeparateur(c) || c == ':') break;
                i--;
            }
            return i;
        }

        public static string GetFileName(string path)
        {
            if (path == null) return null;
            VerifierCaracteres(path);
            return path.Substring(DebutNom(path));
        }

        private static int PointExtension(string path)
        {
            int debut = DebutNom(path);
            for (int i = path.Length - 1; i >= debut; i--)
                if (path[i] == '.') return i;
            return -1;
        }

        public static string GetFileNameWithoutExtension(string path)
        {
            string nom = GetFileName(path);
            if (nom == null) return null;
            int point = nom.LastIndexOf('.');
            return point < 0 ? nom : nom.Substring(0, point);
        }

        public static string GetExtension(string path)
        {
            if (path == null) return null;
            VerifierCaracteres(path);
            int point = PointExtension(path);
            if (point < 0 || point == path.Length - 1) return "";
            return path.Substring(point);
        }

        public static bool HasExtension(string path)
        {
            if (path == null) return false;
            VerifierCaracteres(path);
            int point = PointExtension(path);
            return point >= 0 && point < path.Length - 1;
        }

        public static string ChangeExtension(string path, string extension)
        {
            if (path == null) return null;
            VerifierCaracteres(path);
            int point = PointExtension(path);
            string sans = point < 0 ? path : path.Substring(0, point);
            if (extension == null) return sans;
            if (extension.Length == 0) return sans + ".";
            return extension[0] == '.' ? sans + extension : sans + "." + extension;
        }

        public static string GetFullPath(string path)
        {
            if (path == null) throw new ArgumentNullException("path");
            if (path.Length == 0 || path.Trim().Length == 0) throw new ArgumentException("The path is not of a legal form.");
            VerifierCaracteres(path);
            string r = Natif.CheminComplet(path);
            if (r == null) throw new ArgumentException("The path is not of a legal form.");
            return r;
        }

        public static string GetTempPath() { return Natif.DossierTemp(); }

        private static readonly Random _hasard = new Random();

        public static string GetRandomFileName()
        {
            const string alphabet = "abcdefghijklmnopqrstuvwxyz0123456789";
            char[] c = new char[12];
            lock (_hasard)
            {
                for (int i = 0; i < 12; i++) c[i] = alphabet[_hasard.Next(alphabet.Length)];
            }
            c[8] = '.';
            return new string(c);
        }

        // GetTempFileName crée un fichier vide « tmpXXXX.tmp » unique.
        public static string GetTempFileName()
        {
            string dossier = GetTempPath();
            for (int essai = 0; essai < 65535; essai++)
            {
                int n;
                lock (_hasard) { n = _hasard.Next(1, 65536); }
                string chemin = Combine(dossier, "tmp" + n.ToString("X4") + ".tmp");
                long h = Natif.Ouvrir(chemin, 1, unchecked((int)0xC0000000), 0, 0x80);
                if (h >= 0) { Natif.Fermer(h); return chemin; }
                if (h != -80 && h != -183) throw Natif.Erreur((int)h, chemin);
            }
            throw new IOException("The file exists.");
        }
    }

    public static class File
    {
        internal static FileAttributes? Attributs(string path)
        {
            if (string.IsNullOrEmpty(path)) return null;
            long[] i = Natif.Infos(path);
            return i == null ? (FileAttributes?)null : (FileAttributes)(int)i[0];
        }

        private static long[] InfosOuErreur(string path)
        {
            Verifier(path, "path");
            long[] i = Natif.Infos(path);
            if (i == null) throw new FileNotFoundException("Could not find file '" + path + "'.", path);
            return i;
        }

        internal static void Verifier(string path, string nom)
        {
            if (path == null) throw new ArgumentNullException(nom);
            if (path.Length == 0 || path.Trim().Length == 0) throw new ArgumentException("Empty path name is not legal.", nom);
            // Une boucle « for » : « foreach » sur une chaîne fait échouer
            // l'émission de notre mscorlib (CS7038).
            for (int k = 0; k < path.Length; k++)
            {
                char c = path[k];
                if (c == '"' || c == '<' || c == '>' || c == '|' || c < 32)
                    throw new ArgumentException("Illegal characters in path.", nom);
            }
        }

        public static bool Exists(string path)
        {
            try
            {
                FileAttributes? a = Attributs(path);
                return a.HasValue && (a.Value & FileAttributes.Directory) == 0;
            }
            catch (Exception) { return false; }
        }

        public static FileStream Open(string path, FileMode mode) { return new FileStream(path, mode, mode == FileMode.Append ? FileAccess.Write : FileAccess.ReadWrite, FileShare.None); }
        public static FileStream Open(string path, FileMode mode, FileAccess access) { return new FileStream(path, mode, access, FileShare.None); }
        public static FileStream Open(string path, FileMode mode, FileAccess access, FileShare share) { return new FileStream(path, mode, access, share); }
        public static FileStream OpenRead(string path) { return new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read); }
        public static FileStream OpenWrite(string path) { return new FileStream(path, FileMode.OpenOrCreate, FileAccess.Write, FileShare.None); }
        public static FileStream Create(string path) { return new FileStream(path, FileMode.Create, FileAccess.ReadWrite, FileShare.None); }
        public static FileStream Create(string path, int bufferSize) { return Create(path); }
        public static FileStream Create(string path, int bufferSize, FileOptions options) { return new FileStream(path, FileMode.Create, FileAccess.ReadWrite, FileShare.None, bufferSize, options); }
        public static StreamReader OpenText(string path) { return new StreamReader(path); }
        public static StreamWriter CreateText(string path) { return new StreamWriter(path, false); }
        public static StreamWriter AppendText(string path) { return new StreamWriter(path, true); }

        public static byte[] ReadAllBytes(string path)
        {
            using (FileStream f = OpenRead(path))
            {
                long taille = f.Length;
                if (taille > int.MaxValue) throw new IOException("The file is too long. This operation is currently limited to supporting files less than 2 gigabytes in size.");
                byte[] octets = new byte[(int)taille];
                int lus = 0;
                while (lus < octets.Length)
                {
                    int n = f.Read(octets, lus, octets.Length - lus);
                    if (n == 0) break;
                    lus += n;
                }
                if (lus < octets.Length) Array.Resize(ref octets, lus);
                return octets;
            }
        }

        public static void WriteAllBytes(string path, byte[] bytes)
        {
            if (bytes == null) throw new ArgumentNullException("bytes");
            using (FileStream f = Create(path)) f.Write(bytes, 0, bytes.Length);
        }

        public static string ReadAllText(string path) { using (StreamReader r = new StreamReader(path)) return r.ReadToEnd(); }
        public static string ReadAllText(string path, Encoding encoding) { using (StreamReader r = new StreamReader(path, encoding)) return r.ReadToEnd(); }

        public static string[] ReadAllLines(string path) { return ReadAllLines(path, Encoding.UTF8); }

        public static string[] ReadAllLines(string path, Encoding encoding)
        {
            List<string> lignes = new List<string>();
            using (StreamReader r = new StreamReader(path, encoding))
            {
                string l;
                while ((l = r.ReadLine()) != null) lignes.Add(l);
            }
            return lignes.ToArray();
        }

        public static IEnumerable<string> ReadLines(string path) { return ReadAllLines(path); }
        public static IEnumerable<string> ReadLines(string path, Encoding encoding) { return ReadAllLines(path, encoding); }

        public static void WriteAllText(string path, string contents) { using (StreamWriter w = new StreamWriter(path, false)) w.Write(contents); }
        public static void WriteAllText(string path, string contents, Encoding encoding) { using (StreamWriter w = new StreamWriter(path, false, encoding)) w.Write(contents); }
        public static void AppendAllText(string path, string contents) { using (StreamWriter w = new StreamWriter(path, true)) w.Write(contents); }
        public static void AppendAllText(string path, string contents, Encoding encoding) { using (StreamWriter w = new StreamWriter(path, true, encoding)) w.Write(contents); }

        public static void WriteAllLines(string path, string[] contents) { WriteAllLines(path, (IEnumerable<string>)contents); }
        public static void WriteAllLines(string path, string[] contents, Encoding encoding) { WriteAllLines(path, (IEnumerable<string>)contents, encoding); }
        public static void WriteAllLines(string path, IEnumerable<string> contents) { Lignes(new StreamWriter(path, false), contents); }
        public static void WriteAllLines(string path, IEnumerable<string> contents, Encoding encoding) { Lignes(new StreamWriter(path, false, encoding), contents); }
        public static void AppendAllLines(string path, IEnumerable<string> contents) { Lignes(new StreamWriter(path, true), contents); }
        public static void AppendAllLines(string path, IEnumerable<string> contents, Encoding encoding) { Lignes(new StreamWriter(path, true, encoding), contents); }

        private static void Lignes(StreamWriter w, IEnumerable<string> contents)
        {
            using (w)
            {
                if (contents == null) throw new ArgumentNullException("contents");
                foreach (string l in contents) w.WriteLine(l);
            }
        }

        // Supprimer un fichier absent n'est pas une erreur (.NET).
        public static void Delete(string path)
        {
            Verifier(path, "path");
            int r = Natif.SupprimerFichier(path);
            if (r < 0 && r != -2) throw Natif.Erreur(r, path);
        }

        public static void Copy(string sourceFileName, string destFileName) { Copy(sourceFileName, destFileName, false); }

        public static void Copy(string sourceFileName, string destFileName, bool overwrite)
        {
            Verifier(sourceFileName, "sourceFileName");
            Verifier(destFileName, "destFileName");
            int r = Natif.Copier(sourceFileName, destFileName, overwrite);
            if (r < 0) throw Natif.Erreur(r, r == -80 || r == -183 ? destFileName : sourceFileName);
        }

        public static void Move(string sourceFileName, string destFileName)
        {
            Verifier(sourceFileName, "sourceFileName");
            Verifier(destFileName, "destFileName");
            if (!Exists(sourceFileName)) throw new FileNotFoundException("Could not find file '" + sourceFileName + "'.", sourceFileName);
            int r = Natif.Deplacer(sourceFileName, destFileName, false);
            if (r == -80 || r == -183) throw new IOException("Cannot create a file when that file already exists.");
            Natif.Verifier(r, sourceFileName);
        }

        public static void Replace(string sourceFileName, string destinationFileName, string destinationBackupFileName)
        {
            if (destinationBackupFileName != null) Copy(destinationFileName, destinationBackupFileName, true);
            Natif.Verifier(Natif.Deplacer(sourceFileName, destinationFileName, true), sourceFileName);
        }

        public static FileAttributes GetAttributes(string path) { return (FileAttributes)(int)InfosOuErreur(path)[0]; }
        public static void SetAttributes(string path, FileAttributes fileAttributes) { Verifier(path, "path"); Natif.Verifier(Natif.FixerAttributs(path, (int)fileAttributes), path); }

        public static DateTime GetCreationTimeUtc(string path) { return DateTime.FromFileTimeUtc(InfosOuErreur(path)[1]); }
        public static DateTime GetCreationTime(string path) { return GetCreationTimeUtc(path).ToLocalTime(); }
        public static DateTime GetLastAccessTimeUtc(string path) { return DateTime.FromFileTimeUtc(InfosOuErreur(path)[2]); }
        public static DateTime GetLastAccessTime(string path) { return GetLastAccessTimeUtc(path).ToLocalTime(); }
        public static DateTime GetLastWriteTimeUtc(string path) { return DateTime.FromFileTimeUtc(InfosOuErreur(path)[3]); }
        public static DateTime GetLastWriteTime(string path) { return GetLastWriteTimeUtc(path).ToLocalTime(); }

        public static void SetLastWriteTimeUtc(string path, DateTime lastWriteTimeUtc)
        {
            Verifier(path, "path");
            Natif.Verifier(Natif.FixerDateEcriture(path, lastWriteTimeUtc.ToFileTimeUtc()), path);
        }

        public static void SetLastWriteTime(string path, DateTime lastWriteTime) { SetLastWriteTimeUtc(path, lastWriteTime.ToUniversalTime()); }
    }

    public static class Directory
    {
        public static bool Exists(string path)
        {
            try
            {
                FileAttributes? a = File.Attributs(path);
                return a.HasValue && (a.Value & FileAttributes.Directory) != 0;
            }
            catch (Exception) { return false; }
        }

        // Crée aussi les dossiers parents ; un dossier déjà là n'est pas
        // une erreur, un fichier du même nom en est une.
        public static DirectoryInfo CreateDirectory(string path)
        {
            File.Verifier(path, "path");
            Creer(path);
            return new DirectoryInfo(path);
        }

        private static void Creer(string path)
        {
            if (Exists(path)) return;
            string parent = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(parent) && parent != path) Creer(parent);
            int r = Natif.CreerDossier(path);
            if (r == -80 || r == -183)
            {
                if (Exists(path)) return;
                throw new IOException("Cannot create \"" + path + "\" because a file or directory with the same name already exists.");
            }
            Natif.Verifier(r, path);
        }

        public static void Delete(string path) { Delete(path, false); }

        public static void Delete(string path, bool recursive)
        {
            File.Verifier(path, "path");
            if (!Exists(path)) throw new DirectoryNotFoundException("Could not find a part of the path '" + path + "'.");
            if (recursive)
            {
                foreach (string f in Lister(path, "*", 1)) File.Delete(f);
                foreach (string d in Lister(path, "*", 2)) Delete(d, true);
            }
            Natif.Verifier(Natif.SupprimerDossier(path), path);
        }

        public static void Move(string sourceDirName, string destDirName)
        {
            File.Verifier(sourceDirName, "sourceDirName");
            File.Verifier(destDirName, "destDirName");
            if (!Exists(sourceDirName)) throw new DirectoryNotFoundException("Could not find a part of the path '" + sourceDirName + "'.");
            Natif.Verifier(Natif.Deplacer(sourceDirName, destDirName, false), sourceDirName);
        }

        internal static List<string> Lister(string path, string searchPattern, int quoi)
        {
            File.Verifier(path, "path");
            if (searchPattern == null) throw new ArgumentNullException("searchPattern");
            if (!Exists(path)) throw new DirectoryNotFoundException("Could not find a part of the path '" + Path.GetFullPath(path) + "'.");
            string[] noms = Natif.Lister(Path.Combine(path, searchPattern), quoi);
            List<string> r = new List<string>();
            if (noms == null) return r;
            foreach (string n in noms) r.Add(Path.Combine(path, n));
            return r;
        }

        private static string[] Chercher(string path, string searchPattern, SearchOption option, int quoi)
        {
            List<string> r = Lister(path, searchPattern, quoi);
            if (option == SearchOption.AllDirectories)
                foreach (string d in Lister(path, "*", 2))
                    r.AddRange(Chercher(d, searchPattern, option, quoi));
            return r.ToArray();
        }

        public static string[] GetFiles(string path) { return Chercher(path, "*", SearchOption.TopDirectoryOnly, 1); }
        public static string[] GetFiles(string path, string searchPattern) { return Chercher(path, searchPattern, SearchOption.TopDirectoryOnly, 1); }
        public static string[] GetFiles(string path, string searchPattern, SearchOption searchOption) { return Chercher(path, searchPattern, searchOption, 1); }
        public static string[] GetDirectories(string path) { return Chercher(path, "*", SearchOption.TopDirectoryOnly, 2); }
        public static string[] GetDirectories(string path, string searchPattern) { return Chercher(path, searchPattern, SearchOption.TopDirectoryOnly, 2); }
        public static string[] GetDirectories(string path, string searchPattern, SearchOption searchOption) { return Chercher(path, searchPattern, searchOption, 2); }
        public static string[] GetFileSystemEntries(string path) { return Chercher(path, "*", SearchOption.TopDirectoryOnly, 3); }
        public static string[] GetFileSystemEntries(string path, string searchPattern) { return Chercher(path, searchPattern, SearchOption.TopDirectoryOnly, 3); }
        public static string[] GetFileSystemEntries(string path, string searchPattern, SearchOption searchOption) { return Chercher(path, searchPattern, searchOption, 3); }

        public static IEnumerable<string> EnumerateFiles(string path) { return GetFiles(path); }
        public static IEnumerable<string> EnumerateFiles(string path, string searchPattern) { return GetFiles(path, searchPattern); }
        public static IEnumerable<string> EnumerateFiles(string path, string searchPattern, SearchOption searchOption) { return GetFiles(path, searchPattern, searchOption); }
        public static IEnumerable<string> EnumerateDirectories(string path) { return GetDirectories(path); }
        public static IEnumerable<string> EnumerateDirectories(string path, string searchPattern) { return GetDirectories(path, searchPattern); }
        public static IEnumerable<string> EnumerateDirectories(string path, string searchPattern, SearchOption searchOption) { return GetDirectories(path, searchPattern, searchOption); }
        public static IEnumerable<string> EnumerateFileSystemEntries(string path) { return GetFileSystemEntries(path); }
        public static IEnumerable<string> EnumerateFileSystemEntries(string path, string searchPattern) { return GetFileSystemEntries(path, searchPattern); }

        public static string GetCurrentDirectory() { return Natif.DossierCourant(); }

        public static void SetCurrentDirectory(string path)
        {
            File.Verifier(path, "path");
            Natif.Verifier(Natif.FixerDossierCourant(path), path);
        }

        public static DirectoryInfo GetParent(string path)
        {
            File.Verifier(path, "path");
            string parent = Path.GetDirectoryName(Path.GetFullPath(path));
            return parent == null ? null : new DirectoryInfo(parent);
        }

        public static string GetDirectoryRoot(string path) { return Path.GetPathRoot(Path.GetFullPath(path)); }

        public static DateTime GetLastWriteTime(string path) { return File.GetLastWriteTime(path); }
        public static DateTime GetLastWriteTimeUtc(string path) { return File.GetLastWriteTimeUtc(path); }
        public static DateTime GetCreationTime(string path) { return File.GetCreationTime(path); }
        public static DateTime GetCreationTimeUtc(string path) { return File.GetCreationTimeUtc(path); }
        public static string[] GetLogicalDrives() { return new string[] { "C:\\" }; }
    }

    public abstract class FileSystemInfo
    {
        protected string FullPath;
        protected string OriginalPath;
        private long[] _infos;
        private bool _lu;

        protected FileSystemInfo() { }

        internal void Initialiser(string path)
        {
            File.Verifier(path, "path");
            OriginalPath = path;
            FullPath = Path.GetFullPath(path);
            _lu = false;
        }

        internal long[] Infos
        {
            get
            {
                if (!_lu) { _infos = Natif.Infos(FullPath); _lu = true; }
                return _infos;
            }
        }

        internal long[] InfosOuErreur
        {
            get
            {
                long[] i = Infos;
                if (i == null) throw new FileNotFoundException("Could not find file '" + FullPath + "'.", FullPath);
                return i;
            }
        }

        public virtual string FullName { get { return FullPath; } }
        public abstract string Name { get; }
        public abstract bool Exists { get; }
        public abstract void Delete();

        public string Extension { get { return Path.GetExtension(FullPath); } }

        public FileAttributes Attributes
        {
            get { long[] i = Infos; return i == null ? (FileAttributes)(-1) : (FileAttributes)(int)i[0]; }
            set { Natif.Verifier(Natif.FixerAttributs(FullPath, (int)value), FullPath); _lu = false; }
        }

        public DateTime CreationTimeUtc { get { return DateTime.FromFileTimeUtc(InfosOuErreur[1]); } }
        public DateTime CreationTime { get { return CreationTimeUtc.ToLocalTime(); } }
        public DateTime LastAccessTimeUtc { get { return DateTime.FromFileTimeUtc(InfosOuErreur[2]); } }
        public DateTime LastAccessTime { get { return LastAccessTimeUtc.ToLocalTime(); } }

        public DateTime LastWriteTimeUtc
        {
            get { return DateTime.FromFileTimeUtc(InfosOuErreur[3]); }
            set { File.SetLastWriteTimeUtc(FullPath, value); _lu = false; }
        }

        public DateTime LastWriteTime
        {
            get { return LastWriteTimeUtc.ToLocalTime(); }
            set { LastWriteTimeUtc = value.ToUniversalTime(); }
        }

        public void Refresh() { _lu = false; }

        public override string ToString() { return OriginalPath; }
    }

    public sealed class FileInfo : FileSystemInfo
    {
        public FileInfo(string fileName)
        {
            if (fileName == null) throw new ArgumentNullException("fileName");
            Initialiser(fileName);
        }

        public override string Name { get { return Path.GetFileName(FullPath); } }

        public override bool Exists
        {
            get { long[] i = Infos; return i != null && (i[0] & 0x10) == 0; }
        }

        public long Length
        {
            get
            {
                long[] i = Infos;
                if (i == null || (i[0] & 0x10) != 0) throw new FileNotFoundException("Could not find file '" + FullPath + "'.", FullPath);
                return i[4];
            }
        }

        public string DirectoryName { get { return Path.GetDirectoryName(FullPath); } }

        public DirectoryInfo Directory
        {
            get { string d = DirectoryName; return d == null ? null : new DirectoryInfo(d); }
        }

        public bool IsReadOnly
        {
            get { return (Attributes & FileAttributes.ReadOnly) != 0; }
            set
            {
                FileAttributes a = Attributes;
                Attributes = value ? a | FileAttributes.ReadOnly : a & ~FileAttributes.ReadOnly;
            }
        }

        public FileStream Open(FileMode mode) { return File.Open(FullPath, mode); }
        public FileStream Open(FileMode mode, FileAccess access) { return File.Open(FullPath, mode, access); }
        public FileStream Open(FileMode mode, FileAccess access, FileShare share) { return File.Open(FullPath, mode, access, share); }
        public FileStream OpenRead() { return File.OpenRead(FullPath); }
        public FileStream OpenWrite() { return File.OpenWrite(FullPath); }
        public FileStream Create() { return File.Create(FullPath); }
        public StreamReader OpenText() { return File.OpenText(FullPath); }
        public StreamWriter CreateText() { return File.CreateText(FullPath); }
        public StreamWriter AppendText() { return File.AppendText(FullPath); }

        public FileInfo CopyTo(string destFileName) { return CopyTo(destFileName, false); }

        public FileInfo CopyTo(string destFileName, bool overwrite)
        {
            File.Copy(FullPath, destFileName, overwrite);
            return new FileInfo(destFileName);
        }

        public void MoveTo(string destFileName)
        {
            File.Move(FullPath, destFileName);
            Initialiser(destFileName);
        }

        public override void Delete() { File.Delete(FullPath); Refresh(); }
    }

    public sealed class DirectoryInfo : FileSystemInfo
    {
        public DirectoryInfo(string path)
        {
            if (path == null) throw new ArgumentNullException("path");
            Initialiser(path);
        }

        public override string Name
        {
            get
            {
                string p = FullPath;
                if (p.Length > Path.LongueurRacine(p) && Path.EstSeparateur(p[p.Length - 1])) p = p.Substring(0, p.Length - 1);
                string n = Path.GetFileName(p);
                return n.Length == 0 ? p : n;
            }
        }

        public override bool Exists
        {
            get { long[] i = Infos; return i != null && (i[0] & 0x10) != 0; }
        }

        public DirectoryInfo Parent
        {
            get
            {
                string p = FullPath;
                if (p.Length > Path.LongueurRacine(p) && Path.EstSeparateur(p[p.Length - 1])) p = p.Substring(0, p.Length - 1);
                string d = Path.GetDirectoryName(p);
                return d == null ? null : new DirectoryInfo(d);
            }
        }

        public DirectoryInfo Root { get { return new DirectoryInfo(Path.GetPathRoot(FullPath)); } }

        public void Create() { System.IO.Directory.CreateDirectory(FullPath); Refresh(); }

        public DirectoryInfo CreateSubdirectory(string path)
        {
            return System.IO.Directory.CreateDirectory(Path.Combine(FullPath, path));
        }

        public override void Delete() { System.IO.Directory.Delete(FullPath, false); Refresh(); }
        public void Delete(bool recursive) { System.IO.Directory.Delete(FullPath, recursive); Refresh(); }

        public void MoveTo(string destDirName)
        {
            System.IO.Directory.Move(FullPath, destDirName);
            Initialiser(destDirName);
        }

        public FileInfo[] GetFiles() { return GetFiles("*", SearchOption.TopDirectoryOnly); }
        public FileInfo[] GetFiles(string searchPattern) { return GetFiles(searchPattern, SearchOption.TopDirectoryOnly); }

        public FileInfo[] GetFiles(string searchPattern, SearchOption searchOption)
        {
            string[] noms = System.IO.Directory.GetFiles(FullPath, searchPattern, searchOption);
            FileInfo[] r = new FileInfo[noms.Length];
            for (int i = 0; i < noms.Length; i++) r[i] = new FileInfo(noms[i]);
            return r;
        }

        public DirectoryInfo[] GetDirectories() { return GetDirectories("*", SearchOption.TopDirectoryOnly); }
        public DirectoryInfo[] GetDirectories(string searchPattern) { return GetDirectories(searchPattern, SearchOption.TopDirectoryOnly); }

        public DirectoryInfo[] GetDirectories(string searchPattern, SearchOption searchOption)
        {
            string[] noms = System.IO.Directory.GetDirectories(FullPath, searchPattern, searchOption);
            DirectoryInfo[] r = new DirectoryInfo[noms.Length];
            for (int i = 0; i < noms.Length; i++) r[i] = new DirectoryInfo(noms[i]);
            return r;
        }

        public FileSystemInfo[] GetFileSystemInfos() { return GetFileSystemInfos("*"); }

        public FileSystemInfo[] GetFileSystemInfos(string searchPattern)
        {
            List<FileSystemInfo> r = new List<FileSystemInfo>();
            foreach (DirectoryInfo d in GetDirectories(searchPattern)) r.Add(d);
            foreach (FileInfo f in GetFiles(searchPattern)) r.Add(f);
            return r.ToArray();
        }

        public IEnumerable<FileInfo> EnumerateFiles() { return GetFiles(); }
        public IEnumerable<FileInfo> EnumerateFiles(string searchPattern) { return GetFiles(searchPattern); }
        public IEnumerable<FileInfo> EnumerateFiles(string searchPattern, SearchOption searchOption) { return GetFiles(searchPattern, searchOption); }
        public IEnumerable<DirectoryInfo> EnumerateDirectories() { return GetDirectories(); }
        public IEnumerable<DirectoryInfo> EnumerateDirectories(string searchPattern) { return GetDirectories(searchPattern); }
        public IEnumerable<FileSystemInfo> EnumerateFileSystemInfos() { return GetFileSystemInfos(); }
    }
}

namespace System
{
    // System.Random (#2015). Générateur xorshift64* : la documentation de
    // .NET ne garantit pas la suite produite pour une graine donnée, seulement
    // qu'une même graine redonne la même suite dans le même runtime.
    public class Random
    {
        private static int _compteur;
        private ulong _etat;

        public Random() : this(Environment.TickCount ^ Threading.Interlocked.Increment(ref _compteur) * 0x2545F491) { }

        public Random(int Seed)
        {
            _etat = 0x9E3779B97F4A7C15UL ^ (ulong)(uint)Seed * 0xBF58476D1CE4E5B9UL;
            if (_etat == 0) _etat = 0x9E3779B97F4A7C15UL;
        }

        private ulong Suivant()
        {
            ulong x = _etat;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            _etat = x;
            return x * 0x2545F4914F6CDD1DUL;
        }

        protected virtual double Sample() { return (Suivant() >> 11) * (1.0 / 9007199254740992.0); }

        public virtual int Next() { return (int)(Suivant() >> 33); }

        public virtual int Next(int maxValue)
        {
            if (maxValue < 0) throw new ArgumentOutOfRangeException("maxValue", "'maxValue' must be greater than zero.");
            return (int)(Sample() * maxValue);
        }

        public virtual int Next(int minValue, int maxValue)
        {
            if (minValue > maxValue) throw new ArgumentOutOfRangeException("minValue", "'minValue' cannot be greater than maxValue.");
            long etendue = (long)maxValue - minValue;
            return (int)(minValue + (long)(Sample() * etendue));
        }

        public virtual double NextDouble() { return Sample(); }

        public virtual void NextBytes(byte[] buffer)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            for (int i = 0; i < buffer.Length; i++) buffer[i] = (byte)(Suivant() >> 56);
        }
    }
}
