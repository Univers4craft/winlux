// Témoin de List<T>.AsReadOnly dans la mscorlib Winlux (#2306).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 4) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) mourait sur List`1::AsReadOnly.
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;

namespace Temoins
{
    static class LectureSeule
    {
        static int Main()
        {
            List<string> l = new List<string>();
            l.Add("a"); l.Add("b");
            ReadOnlyCollection<string> r = l.AsReadOnly();

            // 1. Même contenu.
            if (r == null || r.Count != 2 || r[0] != "a" || r[1] != "b" || !r.Contains("b") || r.IndexOf("b") != 1) return 1;

            // 2. Pas de copie : un ajout à la liste se voit dans l'enveloppe.
            l.Add("c");
            if (r.Count != 3 || r[2] != "c") return 2;
            int n = 0;
            foreach (string s in r) n++;
            if (n != 3) return 2;

            // 3. Toute modification est refusée.
            IList<string> il = r;
            try { il.Add("x"); return 3; } catch (NotSupportedException) { }
            try { il[0] = "x"; return 3; } catch (NotSupportedException) { }
            try { il.RemoveAt(0); return 3; } catch (NotSupportedException) { }
            if (!il.IsReadOnly || l.Count != 3) return 3;

            // 4. Interfaces en lecture.
            IReadOnlyList<string> ro = r;
            if (ro.Count != 3 || ro[1] != "b") return 4;
            return 200;
        }
    }
}
