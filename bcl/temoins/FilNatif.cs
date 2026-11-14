// Témoin d'un rappel natif arrivé sur un fil créé par CreateThread (#2383).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 3) de la première vérification qui échoue.
// StartServiceCtrlDispatcher appelle ServiceMain sur un fil à lui ;
// Corsair.Service.exe (iCUE) en dépend.
using System;
using System.Runtime.InteropServices;

namespace Temoins
{
    delegate uint Debut(IntPtr parametre);

    static class FilNatif
    {
        [DllImport("kernel32.dll")]
        static extern IntPtr CreateThread(IntPtr attributs, UIntPtr pile, Debut debut, IntPtr parametre, uint options, out uint id);

        [DllImport("kernel32.dll")]
        static extern uint WaitForSingleObject(IntPtr h, uint ms);

        [DllImport("kernel32.dll")]
        static extern bool GetExitCodeThread(IntPtr h, out uint code);

        [DllImport("kernel32.dll")]
        static extern bool CloseHandle(IntPtr h);

        static int vu;
        static string texte;

        static uint Corps(IntPtr parametre)
        {
            vu = parametre.ToInt32();
            texte = "fil " + vu;
            return 77;
        }

        static int Main()
        {
            Debut d = Corps;
            uint id;
            IntPtr h = CreateThread(IntPtr.Zero, UIntPtr.Zero, d, new IntPtr(41), 0, out id);
            if (h == IntPtr.Zero) return 1;

            // 1. Le fil se termine.
            if (WaitForSingleObject(h, 10000) != 0) return 1;

            // 2. Le délégué a bien tourné, avec son paramètre, et a pu
            //    allouer (chaîne) sur ce fil.
            if (vu != 41 || texte != "fil 41") return 2;

            // 3. Sa valeur de retour est le code de sortie du fil.
            uint code;
            if (!GetExitCodeThread(h, out code) || code != 77) return 3;
            CloseHandle(h);
            GC.KeepAlive(d);
            return 200;
        }
    }
}
