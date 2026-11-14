// Témoin d'Exception.Data dans la mscorlib Winlux (#2361).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) mourait sur Exception::get_Data.
using System;
using System.Collections;

namespace Temoins
{
    class MaPanne : Exception
    {
        public int Code;
        public MaPanne(string m) : base(m) { Code = 7; }
    }

    static class Donnees
    {
        static int Index() { return 5; }

        static int Main()
        {
            // 1. Jamais null, le même dictionnaire à chaque lecture.
            Exception e = new Exception("x", new InvalidOperationException("dedans"));
            IDictionary d = e.Data;
            if (d == null || d.Count != 0 || e.Data != d) return 1;

            // 2. Les paires se gardent ; message et exception interne intacts.
            e.Data["cle"] = "valeur";
            e.Data.Add(3, 4);
            if (e.Data.Count != 2 || (string)e.Data["cle"] != "valeur" || (int)e.Data[3] != 4) return 2;
            if (e.Message != "x" || e.InnerException == null || e.InnerException.Message != "dedans") return 2;

            // 3. Classe dérivée : ses champs ne se mêlent pas aux données.
            MaPanne p = new MaPanne("m");
            p.Data["a"] = 1;
            if (p.Code != 7 || p.Data.Count != 1 || p.Message != "m") return 3;

            // 4. Exception levée par le moteur, puis relancée avec ses données.
            try
            {
                try { int[] t = new int[1]; t[Index()] = 1; }
                catch (IndexOutOfRangeException n) { n.Data["ou"] = "ici"; throw; }
            }
            catch (IndexOutOfRangeException n2)
            {
                if ((string)n2.Data["ou"] != "ici") return 4;
                return 200;
            }
            return 4;
        }
    }
}
