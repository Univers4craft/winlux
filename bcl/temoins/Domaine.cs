// Témoin de System.AppDomain dans la mscorlib Winlux (#2054).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait AppDomain::get_CurrentDomain,
// BaseDirectory, UnhandledException et AssemblyResolve.
using System;
using System.IO;
using System.Reflection;

namespace Temoins
{
    static class Domaine
    {
        static int Main()
        {
            AppDomain d = AppDomain.CurrentDomain;
            if (d == null || !object.ReferenceEquals(d, AppDomain.CurrentDomain) || !d.IsDefaultAppDomain()) return 1;
            string exe = Assembly.GetEntryAssembly().Location;
            if (d.FriendlyName != "Domaine.exe" || !d.BaseDirectory.EndsWith("\\")) return 2;
            if (d.BaseDirectory != Path.GetDirectoryName(exe) + "\\" || d.SetupInformation.ApplicationBase != d.BaseDirectory) return 2;
            if ((string)d.GetData("APPBASE") != d.BaseDirectory || d.GetData("inconnu") != null) return 2;
            d.SetData("clé", 42);
            if ((int)d.GetData("clé") != 42) return 2;

            bool trouve = false;
            foreach (Assembly a in d.GetAssemblies()) if (a == typeof(Domaine).Assembly) trouve = true;
            if (!trouve) return 3;

            d.UnhandledException += (o, e) => { };
            // AssemblyResolve : consulté quand une assembly manque.
            string demande = null;
            d.AssemblyResolve += (o, e) => { demande = e.Name; return typeof(Domaine).Assembly; };
            if (Assembly.Load("Temoin.Absente, Version=1.0.0.0") != typeof(Domaine).Assembly || demande != "Temoin.Absente, Version=1.0.0.0") return 4;
            return 200;
        }
    }
}
