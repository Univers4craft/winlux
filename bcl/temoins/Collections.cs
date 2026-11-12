// Témoin de List<T> et Dictionary<TKey, TValue> dans la mscorlib Winlux (#1896).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 14) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait List`1::Add.
using System;
using System.Collections.Generic;

namespace Temoins
{
    class Point
    {
        public int X;
        public Point(int x) { X = x; }
    }

    static class Collections
    {
        static int Somme(IEnumerable<int> valeurs)
        {
            int s = 0;
            foreach (int v in valeurs)
                s += v;
            return s;
        }

        static int Main()
        {
            List<int> l = new List<int>();
            for (int i = 1; i <= 10; i++)
                l.Add(i);
            if (l.Count != 10 || l[0] != 1 || l[9] != 10) return 1;
            int s = 0;
            foreach (int v in l)
                s += v;
            if (s != 55) return 2;
            if (Somme(l) != 55) return 3;
            l.Remove(5);
            l.Insert(0, 100);
            if (l.Count != 10 || l[0] != 100 || l.Contains(5) || l.IndexOf(6) != 5) return 4;
            l.RemoveAll(x => x % 2 == 0);
            if (l.Count != 4 || l[0] != 1) return 5;

            List<string> noms = new List<string> { "b", "a", "c" };
            noms.Sort((x, y) => x == y ? 0 : (x == "a" || (x == "b" && y == "c") ? -1 : 1));
            if (noms[0] != "a" || noms[2] != "c") return 6;
            List<Point> points = new List<Point>();
            points.Add(new Point(7));
            if (points.Find(p => p.X == 7) == null) return 7;

            Dictionary<string, int> d = new Dictionary<string, int>();
            for (int i = 0; i < 50; i++)
                d[string.Concat("k", i, "")] = i;
            if (d.Count != 50 || d["k42"] != 42) return 8;
            int valeur;
            if (!d.TryGetValue("k7", out valeur) || valeur != 7 || d.TryGetValue("zz", out valeur)) return 9;
            if (!d.Remove("k3") || d.ContainsKey("k3") || d.Count != 49) return 10;
            try
            {
                d.Add("k1", 0);
                return 11;
            }
            catch (ArgumentException) { }
            try
            {
                int x = d["absent"];
                return 12;
            }
            catch (KeyNotFoundException) { }

            Dictionary<int, string> e = new Dictionary<int, string>();
            e.Add(1, "un");
            e.Add(2, "deux");
            int cles = 0;
            foreach (KeyValuePair<int, string> kv in e)
                cles += kv.Key;
            foreach (int k in e.Keys)
                cles += k;
            if (cles != 6 || e[2] != "deux") return 13;
            IDictionary<int, string> id = e;
            if (!id.ContainsKey(1) || id.Count != 2) return 14;
            return 200;
        }
    }
}
