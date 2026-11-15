// Témoin de LINQ to Objects (#1975), des tableaux vus comme IEnumerable<T>/IList<T>
// (#1977), de « is »/« as » vers une interface (#1978)
// et de Tuple dans la mscorlib Winlux.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 14) de la première vérification qui échoue.
// Option de compilation : -optimize+.
// Corsair.Service.exe (iCUE 4.18) appelait Enumerable::Any, Select, ToList
// (22 fois chacun), Where, ToArray, FirstOrDefault, Cast et OfType.
using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;

namespace Temoins
{
    class Appareil
    {
        public string Nom;
        public int Leds;
        public string Marque;
    }

    static class Linq
    {
        static int Main()
        {
            int[] nombres = { 5, 3, 8, 1, 9, 2, 8 };
            IList<int> liste = nombres;
            if (liste.Count != 7 || liste[2] != 8 || !liste.Contains(9) || liste.IndexOf(8) != 2) return 1;
            int somme = 0;
            foreach (int n in (IEnumerable<int>)nombres) somme += n;
            IEnumerable brut = nombres;
            int compte = 0;
            foreach (object o in brut) if (o is int) compte++;
            if (somme != 36 || compte != 7) return 2;

            List<int> pairs = nombres.Where(n => n % 2 == 0).ToList();
            if (pairs.Count != 3 || pairs[0] != 8 || pairs[2] != 8) return 3;
            string[] textes = nombres.Select(n => "n" + n).ToArray();
            if (textes.Length != 7 || textes[6] != "n8") return 4;
            if (!nombres.Any() || !nombres.Any(n => n > 8) || nombres.All(n => n > 1) || new int[0].Any()) return 5;
            if (nombres.First() != 5 || nombres.FirstOrDefault(n => n > 100) != 0 || nombres.Last(n => n < 3) != 2 || nombres.Count(n => n == 8) != 2) return 6;
            if (nombres.Max() != 9 || nombres.Min() != 1 || nombres.Sum() != 36 || nombres.Distinct().Count() != 6) return 7;

            int[] tries = nombres.OrderBy(n => n).ToArray();
            int[] inverses = nombres.OrderByDescending(n => n).Take(2).ToArray();
            if (tries[0] != 1 || tries[6] != 9 || inverses[0] != 9 || inverses[1] != 8) return 8;
            if (!nombres.Skip(5).SequenceEqual(new[] { 2, 8 }) || nombres.Reverse().First() != 8 || Enumerable.Range(3, 4).Sum() != 18) return 9;

            List<Appareil> appareils = new List<Appareil>
            {
                new Appareil { Nom = "K70", Leds = 104, Marque = "Corsair" },
                new Appareil { Nom = "M65", Leds = 2, Marque = "Corsair" },
                new Appareil { Nom = "G502", Leds = 2, Marque = "Logitech" },
                new Appareil { Nom = "H150", Leds = 30, Marque = "Corsair" },
            };
            var parMarque = appareils.GroupBy(a => a.Marque).ToList();
            if (parMarque.Count != 2 || parMarque[0].Key != "Corsair" || parMarque[0].Count() != 3) return 10;
            string ordre = string.Join(",", appareils.OrderBy(a => a.Leds).ThenBy(a => a.Nom).Select(a => a.Nom).ToArray());
            if (ordre != "G502,M65,H150,K70") return 11;
            Dictionary<string, int> leds = appareils.ToDictionary(a => a.Nom, a => a.Leds);
            if (leds["H150"] != 30 || appareils.SingleOrDefault(a => a.Nom == "rien") != null) return 12;

            object[] melange = { 1, "deux", 3, "quatre", null };
            if (melange.OfType<string>().Count() != 2 || melange.Take(1).Cast<int>().Single() != 1) return 13;
            var paires = nombres.Take(3).SelectMany(n => Enumerable.Repeat(n, 2)).ToList();
            if (paires.Count != 6 || paires[1] != 5 || paires[5] != 8) return 13;
            try
            {
                new int[0].First();
                return 13;
            }
            catch (InvalidOperationException) { }

            Tuple<string, int> t = Tuple.Create("K70", 104);
            if (t.Item1 != "K70" || t.Item2 != 104 || !t.Equals(Tuple.Create("K70", 104)) || t.ToString() != "(K70, 104)") return 14;
            Func<int, int, int, int> trois = (a, b, c) => a * b + c;
            if (trois(3, 4, 5) != 17 || appareils.Aggregate(0, (s, a) => s + a.Leds) != 138) return 14;
            return 200;
        }
    }
}
