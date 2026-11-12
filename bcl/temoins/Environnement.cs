// Témoin de System.Environment (#1991) et des attributs d'assembly (#1992)
// dans la mscorlib Winlux, et de String.Empty (#1997).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 7) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) s'arrêtait sur Environment.UserInteractive
// et référençait GetFolderPath, OSVersion, SystemDirectory, Version et les
// attributs d'assembly (AssemblyTitle, FileVersion, Guid, TargetFramework…).
using System;
using System.Diagnostics;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;

[assembly: AssemblyTitle("Témoin")]
[assembly: AssemblyFileVersion("1.2.3.4")]
[assembly: ComVisible(false)]
[assembly: Guid("6a3df000-0000-4000-8000-000000001992")]

namespace Temoins
{
    static class Environnement
    {
        static int Main()
        {
            // String.Empty et Boolean.TrueString étaient nuls (#1997).
            if (string.Empty == null || string.Empty.Length != 0 || bool.TrueString != "True" || bool.FalseString != "False") return 1;
            // Lancé hors du gestionnaire de services : interactif.
            if (!Environment.UserInteractive || Environment.HasShutdownStarted) return 1;
            OperatingSystem os = Environment.OSVersion;
            if (os.Platform != PlatformID.Win32NT || os.Version.Major != 10 || !os.VersionString.StartsWith("Microsoft Windows NT 10.0")) return 2;
            if (Environment.Version.Major != 4 || Environment.ProcessorCount < 1 || Environment.NewLine != "\r\n") return 3;
            string appdata = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string systeme = Environment.SystemDirectory;
            if (appdata.Length == 0 || !systeme.ToLowerInvariant().EndsWith("system32")) return 4;
            if (Environment.MachineName == null || Environment.TickCount == 0 && Environment.TickCount64 == 0) return 5;

            AssemblyFileVersionAttribute fv = new AssemblyFileVersionAttribute("1.2.3.4");
            TargetFrameworkAttribute tf = new TargetFrameworkAttribute(".NETFramework,Version=v4.7.2") { FrameworkDisplayName = ".NET Framework 4.7.2" };
            if (fv.Version != "1.2.3.4" || tf.FrameworkName != ".NETFramework,Version=v4.7.2" || tf.FrameworkDisplayName != ".NET Framework 4.7.2") return 6;
            DebuggableAttribute d = new DebuggableAttribute(true, false);
            if (!d.IsJITTrackingEnabled || d.IsJITOptimizerDisabled || new GuidAttribute("x").Value != "x" || new ComVisibleAttribute(true).Value != true) return 7;
            return 200;
        }
    }
}
