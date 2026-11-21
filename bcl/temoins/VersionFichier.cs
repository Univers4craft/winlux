// Témoin de System.Diagnostics.FileVersionInfo (#2820).
// Façade de compilation : System.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 6) de la première vérification qui échoue.
// Le téléchargeur de Paint.NET lit la version d'un fichier par
// FileVersionInfo.GetVersionInfo puis FileMajorPart… : notre mscorlib n'avait
// pas ce type. Le témoin lit sa propre ressource de version, que Roslyn
// écrit d'après les attributs ci-dessous.
extern alias Facade;
using System;
using System.IO;
using System.Reflection;
using FileVersionInfo = Facade::System.Diagnostics.FileVersionInfo;

[assembly: AssemblyFileVersion("7.3.21.4")]
[assembly: AssemblyInformationalVersion("7.3-temoin")]
[assembly: AssemblyCompany("Winlux témoin")]
[assembly: AssemblyProduct("Témoin FileVersionInfo")]
[assembly: AssemblyTitle("Lecture de version")]
[assembly: AssemblyCopyright("GPL 3")]

static class VersionFichier
{
    static int Main()
    {
        string moi = Path.GetFullPath("VersionFichier.exe");
        if (!File.Exists(moi)) moi = Path.GetFullPath(Environment.GetCommandLineArgs()[0]);

        FileVersionInfo v = FileVersionInfo.GetVersionInfo(moi);
        // 1. Le nom complet du fichier.
        if (v.FileName != moi) return 1;

        // 2. Les parties numériques de VS_FIXEDFILEINFO.
        if (v.FileMajorPart != 7 || v.FileMinorPart != 3 || v.FileBuildPart != 21 || v.FilePrivatePart != 4) return 2;
        if (v.ProductMajorPart != 7 || v.IsDebug) return 2;

        // 3. Les chaînes de StringFileInfo.
        if (v.FileVersion != "7.3.21.4" || v.ProductVersion != "7.3-temoin") return 3;
        if (v.CompanyName != "Winlux témoin" || v.ProductName != "Témoin FileVersionInfo") return 3;
        if (v.FileDescription != "Lecture de version" || v.LegalCopyright != "GPL 3") return 3;

        // 4. OriginalFilename : le nom du module.
        if (v.OriginalFilename != "VersionFichier.exe") return 4;

        // 5. Un fichier sans ressource de version : tout reste vide.
        string texte = Path.Combine(Path.GetTempPath(), "winlux-temoin-version.txt");
        File.WriteAllText(texte, "pas un exécutable");
        try
        {
            FileVersionInfo t = FileVersionInfo.GetVersionInfo(texte);
            if (t.FileMajorPart != 0 || t.FileVersion != null || t.CompanyName != null) return 5;
        }
        finally { File.Delete(texte); }

        // 6. Un fichier absent : FileNotFoundException.
        try { FileVersionInfo.GetVersionInfo(moi + ".absent"); return 6; }
        catch (FileNotFoundException) { }

        return 200;
    }
}
