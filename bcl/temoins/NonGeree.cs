// Témoin d'AppDomain.UnhandledException dans la mscorlib Winlux (#2058).
// Code de sortie attendu : 200.
// Main laisse échapper une exception ; le gestionnaire vérifie ses
// arguments puis sort avec 200. Sans lui, la sortie se ferait avec le code
// d'une exception non gérée.
// Sinon : 2 (arguments inattendus).
using System;

namespace Temoins
{
    static class NonGeree
    {
        static void EnPanne(object emetteur, UnhandledExceptionEventArgs e)
        {
            InvalidOperationException x = e.ExceptionObject as InvalidOperationException;
            if (emetteur != AppDomain.CurrentDomain || !e.IsTerminating || x == null || x.Message != "panne") Environment.Exit(2);
            Environment.Exit(200);
        }

        static int Main()
        {
            AppDomain.CurrentDomain.UnhandledException += EnPanne;
            throw new InvalidOperationException("panne");
        }
    }
}
