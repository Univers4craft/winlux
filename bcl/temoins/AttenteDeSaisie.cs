// Témoin de Process.WaitForInputIdle dans la mscorlib Winlux (#3287).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// Avant #3287, notre mscorlib rendait toujours true sans rien regarder.
// MS Learn, « Process.WaitForInputIdle » : InvalidOperationException sans
// processus associé ou sans interface graphique (programme console) ;
// ArgumentOutOfRangeException pour un délai inférieur à -1.
// Le témoin se relance lui-même : c'est un programme console.
using System;
using System.Diagnostics;
using System.Threading.Tasks;

namespace Temoins
{
    static class AttenteDeSaisie
    {
        static int Main(string[] args)
        {
            if (args.Length == 1 && args[0] == "attente")
            {
                Task.Delay(10000).Wait();
                return 0;
            }

            // 1. Aucun processus associé.
            try { new Process().WaitForInputIdle(0); return 1; } catch (InvalidOperationException) { }

            // 2. Délai hors bornes.
            Process moi = Process.GetCurrentProcess();
            try { moi.WaitForInputIdle(-2); return 2; } catch (ArgumentOutOfRangeException) { }

            // 3. Un programme console n'attend jamais de saisie.
            ProcessStartInfo si = new ProcessStartInfo(moi.MainModule.FileName, "attente");
            si.UseShellExecute = false;
            si.CreateNoWindow = true;
            Process fils = Process.Start(si);
            bool leve = false;
            try { fils.WaitForInputIdle(2000); } catch (InvalidOperationException) { leve = true; }
            fils.Kill();
            fils.WaitForExit(5000);
            if (!leve) return 3;

            // 4. Le processus courant est prêt tout de suite.
            if (!moi.WaitForInputIdle(0)) return 4;
            return 200;
        }
    }
}
