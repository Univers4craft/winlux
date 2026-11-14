// Témoin d'AppDomain.ProcessExit dans la mscorlib Winlux (#2058).
// Code de sortie attendu : 200.
// Main rend 1 ; seul le gestionnaire de ProcessExit, levé au retour de
// Main, fait sortir avec 200 (Environment.Exit ne le relève pas).
// Sinon : 1 (ProcessExit jamais levé) ou 2 (mauvais émetteur ou arguments).
using System;

namespace Temoins
{
    static class Sortie
    {
        static int fois;

        static void EnSortie(object emetteur, EventArgs e)
        {
            fois++;
            if (emetteur != AppDomain.CurrentDomain || e == null || fois != 1) Environment.Exit(2);
            Environment.Exit(200);
        }

        static int Main()
        {
            AppDomain.CurrentDomain.ProcessExit += EnSortie;
            return 1;
        }
    }
}
