// Témoin d'un champ délégué dans Marshal.StructureToPtr (#2382).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE) passait sa table SERVICE_TABLE_ENTRY avec
// lpServiceProc = 0.
using System;
using System.Runtime.InteropServices;

namespace Temoins
{
    delegate int Compare(IntPtr a, IntPtr b);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct Entree
    {
        public string Nom;
        public Compare Proc;
    }

    static class Rappels
    {
        [DllImport("msvcrt.dll", CallingConvention = CallingConvention.Cdecl)]
        static extern void qsort(IntPtr tableau, UIntPtr nombre, UIntPtr taille, IntPtr comparer);

        static int appels;

        static int Decroissant(IntPtr a, IntPtr b)
        {
            appels++;
            return Marshal.ReadInt32(b) - Marshal.ReadInt32(a);
        }

        static int Main()
        {
            // 1. Le champ délégué devient un pointeur de fonction non nul.
            Entree e = new Entree();
            e.Nom = "CorsairService";
            e.Proc = Decroissant;
            IntPtr p = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(Entree)));
            Marshal.StructureToPtr(e, p, false);
            IntPtr f = Marshal.ReadIntPtr(p, 8);
            if (Marshal.SizeOf(typeof(Entree)) != 16 || f == IntPtr.Zero || Marshal.PtrToStringUni(Marshal.ReadIntPtr(p, 0)) != "CorsairService") return 1;

            // 2. Le code natif l'appelle : qsort trie avec ce pointeur.
            IntPtr t = Marshal.AllocHGlobal(16);
            int[] v = { 3, 9, 1, 7 };
            Marshal.Copy(v, 0, t, 4);
            qsort(t, new UIntPtr(4), new UIntPtr(4), f);
            Marshal.Copy(t, v, 0, 4);
            if (appels == 0 || v[0] != 9 || v[1] != 7 || v[2] != 3 || v[3] != 1) return 2;

            // 3. Retour vers le géré : le même délégué.
            Entree r = Marshal.PtrToStructure<Entree>(p);
            if (r.Proc == null || r.Nom != "CorsairService" || r.Proc(t, t) != 0) return 3;

            // 4. Délégué nul : pointeur nul.
            e.Proc = null;
            Marshal.StructureToPtr(e, p, true);
            if (Marshal.ReadIntPtr(p, 8) != IntPtr.Zero) return 4;
            Marshal.DestroyStructure(p, typeof(Entree));
            Marshal.FreeHGlobal(p);
            Marshal.FreeHGlobal(t);
            return 200;
        }
    }
}
