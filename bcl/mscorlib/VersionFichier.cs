// System.Diagnostics.FileVersionInfo de la mscorlib Winlux (#2820, suivi
// #2752) : la ressource de version d'un fichier, lue par notre version.dll.
// Licence GPL 3. Écrit d'après la documentation publique : Learn
// (« FileVersionInfo Class », « FileVersionInfo.GetVersionInfo ») et celle de
// l'API Win32 (« GetFileVersionInfoW », « VerQueryValueW »,
// « VS_FIXEDFILEINFO », « StringFileInfo », « VerLanguageNameW ») ; aucun
// code de Microsoft, de Mono ou de Wine.
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace System.Diagnostics
{
    public sealed class FileVersionInfo
    {
        [DllImport("version.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern int GetFileVersionInfoSizeW(string fileName, IntPtr handle);

        [DllImport("version.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool GetFileVersionInfoW(string fileName, int handle, int len, IntPtr data);

        [DllImport("version.dll", CharSet = CharSet.Unicode)]
        private static extern bool VerQueryValueW(IntPtr block, string subBlock, IntPtr buffer, IntPtr len);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
        private static extern int VerLanguageNameW(int lang, IntPtr name, int size);

        private readonly string _fichier;
        private string _comments, _company, _description, _fileVersion, _internalName, _copyright,
            _trademarks, _originalName, _privateBuild, _productName, _productVersion, _specialBuild, _language;
        private int _fMaj, _fMin, _fBuild, _fPriv, _pMaj, _pMin, _pBuild, _pPriv;
        private int _drapeaux;

        private FileVersionInfo(string fichier) { _fichier = fichier; }

        public static FileVersionInfo GetVersionInfo(string fileName)
        {
            if (fileName == null) throw new ArgumentNullException("fileName");
            string complet = Path.GetFullPath(fileName);
            if (!File.Exists(complet)) throw new FileNotFoundException(complet, complet);
            FileVersionInfo v = new FileVersionInfo(complet);
            v.Lire();
            return v;
        }

        private void Lire()
        {
            int taille = GetFileVersionInfoSizeW(_fichier, IntPtr.Zero);
            if (taille <= 0) return; // Learn : pas de ressource, tout reste vide
            IntPtr bloc = Marshal.AllocHGlobal(taille);
            IntPtr sortie = Marshal.AllocHGlobal(IntPtr.Size + 4);
            try
            {
                if (!GetFileVersionInfoW(_fichier, 0, taille, bloc)) return;
                IntPtr p;
                int n;
                if (Requete(bloc, "\\", sortie, out p, out n) && n >= 52 && (uint)Marshal.ReadInt32(p) == 0xFEEF04BD)
                {
                    // VS_FIXEDFILEINFO : versions en deux mots de 32 bits, poids fort d'abord.
                    int fMs = Marshal.ReadInt32(p, 8), fLs = Marshal.ReadInt32(p, 12);
                    int pMs = Marshal.ReadInt32(p, 16), pLs = Marshal.ReadInt32(p, 20);
                    _fMaj = (fMs >> 16) & 0xFFFF; _fMin = fMs & 0xFFFF; _fBuild = (fLs >> 16) & 0xFFFF; _fPriv = fLs & 0xFFFF;
                    _pMaj = (pMs >> 16) & 0xFFFF; _pMin = pMs & 0xFFFF; _pBuild = (pLs >> 16) & 0xFFFF; _pPriv = pLs & 0xFFFF;
                    _drapeaux = Marshal.ReadInt32(p, 28) & Marshal.ReadInt32(p, 24); // dwFileFlags & dwFileFlagsMask
                }
                // Table de chaînes : la première traduction annoncée, puis, par
                // inférence (Learn ne fixe pas l'ordre), anglais des États-Unis
                // en Unicode, en 1252, puis sans page de code.
                string[] essais = new string[4];
                int langue = -1;
                if (Requete(bloc, "\\VarFileInfo\\Translation", sortie, out p, out n) && n >= 4)
                {
                    langue = Marshal.ReadInt16(p) & 0xFFFF;
                    int page = Marshal.ReadInt16(p, 2) & 0xFFFF;
                    essais[0] = langue.ToString("X4") + page.ToString("X4");
                }
                essais[1] = "040904B0";
                essais[2] = "040904E4";
                essais[3] = "04090000";
                foreach (string cle in essais)
                {
                    if (cle == null) continue;
                    if (!Requete(bloc, "\\StringFileInfo\\" + cle + "\\FileVersion", sortie, out p, out n)
                        && !Requete(bloc, "\\StringFileInfo\\" + cle + "\\ProductName", sortie, out p, out n)
                        && !Requete(bloc, "\\StringFileInfo\\" + cle + "\\CompanyName", sortie, out p, out n)) continue;
                    string b = "\\StringFileInfo\\" + cle + "\\";
                    _comments = Chaine(bloc, b + "Comments", sortie);
                    _company = Chaine(bloc, b + "CompanyName", sortie);
                    _description = Chaine(bloc, b + "FileDescription", sortie);
                    _fileVersion = Chaine(bloc, b + "FileVersion", sortie);
                    _internalName = Chaine(bloc, b + "InternalName", sortie);
                    _copyright = Chaine(bloc, b + "LegalCopyright", sortie);
                    _trademarks = Chaine(bloc, b + "LegalTrademarks", sortie);
                    _originalName = Chaine(bloc, b + "OriginalFilename", sortie);
                    _privateBuild = Chaine(bloc, b + "PrivateBuild", sortie);
                    _productName = Chaine(bloc, b + "ProductName", sortie);
                    _productVersion = Chaine(bloc, b + "ProductVersion", sortie);
                    _specialBuild = Chaine(bloc, b + "SpecialBuild", sortie);
                    if (langue < 0) langue = Convert.ToInt32(cle.Substring(0, 4), 16);
                    break;
                }
                if (langue >= 0) _language = NomLangue(langue);
            }
            finally
            {
                Marshal.FreeHGlobal(sortie);
                Marshal.FreeHGlobal(bloc);
            }
        }

        // VerQueryValueW écrit un pointeur puis une longueur : deux cases du même tampon.
        private static bool Requete(IntPtr bloc, string sous, IntPtr sortie, out IntPtr p, out int n)
        {
            Marshal.WriteIntPtr(sortie, IntPtr.Zero);
            Marshal.WriteInt32(sortie, IntPtr.Size, 0);
            p = IntPtr.Zero;
            n = 0;
            if (!VerQueryValueW(bloc, sous, sortie, new IntPtr(sortie.ToInt64() + IntPtr.Size))) return false;
            p = Marshal.ReadIntPtr(sortie);
            n = Marshal.ReadInt32(sortie, IntPtr.Size);
            return p != IntPtr.Zero;
        }

        // Une chaîne absente de la table vaut null ; la longueur rendue
        // compte le nul final.
        private static string Chaine(IntPtr bloc, string sous, IntPtr sortie)
        {
            IntPtr p;
            int n;
            if (!Requete(bloc, sous, sortie, out p, out n)) return null;
            string s = Marshal.PtrToStringUni(p);
            return s;
        }

        private static string NomLangue(int langue)
        {
            IntPtr t = Marshal.AllocHGlobal(256 * 2);
            try
            {
                int n = VerLanguageNameW(langue, t, 256);
                return n > 0 ? Marshal.PtrToStringUni(t, n) : null;
            }
            finally { Marshal.FreeHGlobal(t); }
        }

        public string FileName { get { return _fichier; } }
        public string Comments { get { return _comments; } }
        public string CompanyName { get { return _company; } }
        public string FileDescription { get { return _description; } }
        public string FileVersion { get { return _fileVersion; } }
        public string InternalName { get { return _internalName; } }
        public string Language { get { return _language; } }
        public string LegalCopyright { get { return _copyright; } }
        public string LegalTrademarks { get { return _trademarks; } }
        public string OriginalFilename { get { return _originalName; } }
        public string PrivateBuild { get { return _privateBuild; } }
        public string ProductName { get { return _productName; } }
        public string ProductVersion { get { return _productVersion; } }
        public string SpecialBuild { get { return _specialBuild; } }

        public int FileMajorPart { get { return _fMaj; } }
        public int FileMinorPart { get { return _fMin; } }
        public int FileBuildPart { get { return _fBuild; } }
        public int FilePrivatePart { get { return _fPriv; } }
        public int ProductMajorPart { get { return _pMaj; } }
        public int ProductMinorPart { get { return _pMin; } }
        public int ProductBuildPart { get { return _pBuild; } }
        public int ProductPrivatePart { get { return _pPriv; } }

        // VS_FF_DEBUG, _PRERELEASE, _PATCHED, _PRIVATEBUILD, _SPECIALBUILD.
        public bool IsDebug { get { return (_drapeaux & 0x01) != 0; } }
        public bool IsPreRelease { get { return (_drapeaux & 0x02) != 0; } }
        public bool IsPatched { get { return (_drapeaux & 0x04) != 0; } }
        public bool IsPrivateBuild { get { return (_drapeaux & 0x08) != 0; } }
        public bool IsSpecialBuild { get { return (_drapeaux & 0x20) != 0; } }

        public override string ToString()
        {
            StringBuilder s = new StringBuilder();
            s.Append("File:             ").Append(FileName).Append(Environment.NewLine);
            s.Append("InternalName:     ").Append(InternalName).Append(Environment.NewLine);
            s.Append("OriginalFilename: ").Append(OriginalFilename).Append(Environment.NewLine);
            s.Append("FileVersion:      ").Append(FileVersion).Append(Environment.NewLine);
            s.Append("FileDescription:  ").Append(FileDescription).Append(Environment.NewLine);
            s.Append("Product:          ").Append(ProductName).Append(Environment.NewLine);
            s.Append("ProductVersion:   ").Append(ProductVersion).Append(Environment.NewLine);
            s.Append("Debug:            ").Append(IsDebug.ToString()).Append(Environment.NewLine);
            s.Append("Patched:          ").Append(IsPatched.ToString()).Append(Environment.NewLine);
            s.Append("PreRelease:       ").Append(IsPreRelease.ToString()).Append(Environment.NewLine);
            s.Append("PrivateBuild:     ").Append(IsPrivateBuild.ToString()).Append(Environment.NewLine);
            s.Append("SpecialBuild:     ").Append(IsSpecialBuild.ToString()).Append(Environment.NewLine);
            s.Append("Language:         ").Append(Language).Append(Environment.NewLine);
            return s.ToString();
        }
    }
}
