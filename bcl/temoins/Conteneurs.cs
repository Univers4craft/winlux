// Témoin de Nullable<T>, EventHandler, Queue<T>, Stack<T>, HashSet<T>,
// LinkedList<T> et ConcurrentDictionary dans la mscorlib Winlux (#1952).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 16) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Nullable`1::.ctor, Queue`1,
// HashSet`1 et ConcurrentDictionary`2.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;

namespace Temoins
{
    class Compteur
    {
        public int Total;
        public event EventHandler<int> Change;
        public void Ajoute(int n)
        {
            Total += n;
            EventHandler<int> h = Change;
            if (h != null)
                h(this, n);
        }
    }

    static class Conteneurs
    {
        static int? Moitie(int n)
        {
            if (n % 2 != 0)
                return null;
            return n / 2;
        }

        static int Main()
        {
            int? a = 5;
            int? b = null;
            if (!a.HasValue || a.Value != 5 || b.HasValue || b.GetValueOrDefault(9) != 9) return 1;
            object boite = a;
            object vide = b;
            if (vide != null || !(boite is int) || (int)boite != 5) return 2;
            int? retour = (int?)boite;
            int? retourVide = (int?)vide;
            if (retour != 5 || retourVide.HasValue) return 3;
            int? somme = a + Moitie(4);
            if (somme != 7 || (a + Moitie(3)).HasValue) return 4;

            Compteur c = new Compteur();
            int vu = 0;
            c.Change += (s, n) => vu += n;
            c.Ajoute(3);
            c.Ajoute(4);
            if (vu != 7 || c.Total != 7) return 5;

            Queue<string> q = new Queue<string>();
            for (int i = 0; i < 10; i++)
                q.Enqueue(string.Concat("e", i, ""));
            if (q.Count != 10 || q.Peek() != "e0" || q.Dequeue() != "e0" || q.Dequeue() != "e1") return 6;
            q.Enqueue("fin");
            int n = 0;
            string dernier = null;
            foreach (string s in q) { n++; dernier = s; }
            if (n != 9 || dernier != "fin" || !q.Contains("e5")) return 7;
            q.Clear();
            try { q.Dequeue(); return 8; } catch (InvalidOperationException) { }

            Stack<int> pile = new Stack<int>();
            pile.Push(1);
            pile.Push(2);
            pile.Push(3);
            if (pile.Peek() != 3 || pile.Pop() != 3 || pile.Count != 2 || pile.ToArray()[0] != 2) return 9;

            HashSet<string> ens = new HashSet<string>();
            if (!ens.Add("x") || ens.Add("x") || !ens.Add("y") || !ens.Add(null)) return 10;
            for (int i = 0; i < 30; i++)
                ens.Add(string.Concat("n", i, ""));
            if (ens.Count != 33 || !ens.Contains("n17") || !ens.Remove("y") || ens.Contains("y") || !ens.Contains(null)) return 11;
            HashSet<int> pairs = new HashSet<int> { 2, 4, 6 };
            pairs.UnionWith(new List<int> { 6, 8 });
            pairs.ExceptWith(new List<int> { 2 });
            if (pairs.Count != 3 || pairs.Contains(2) || !pairs.Contains(8)) return 12;

            LinkedList<int> liste = new LinkedList<int>();
            liste.AddLast(2);
            liste.AddFirst(1);
            LinkedListNode<int> trois = liste.AddLast(3);
            liste.AddBefore(trois, 25);
            liste.Remove(2);
            int cumul = 0;
            foreach (int v in liste) cumul = cumul * 100 + v;
            if (liste.Count != 3 || cumul != 12503 || liste.First.Value != 1 || liste.Last.Previous.Value != 25) return 13;

            ConcurrentDictionary<string, int> cd = new ConcurrentDictionary<string, int>();
            if (!cd.TryAdd("a", 1) || cd.TryAdd("a", 2) || cd["a"] != 1) return 14;
            cd.AddOrUpdate("a", 0, (k, v) => v + 10);
            cd.AddOrUpdate("b", 5, (k, v) => v + 10);
            if (cd["a"] != 11 || cd["b"] != 5 || cd.GetOrAdd("c", k => 42) != 42 || cd.GetOrAdd("c", 0) != 42) return 15;
            int retire;
            if (!cd.TryRemove("b", out retire) || retire != 5 || cd.Count != 2 || cd.ContainsKey("b")) return 16;
            int total = 0;
            foreach (KeyValuePair<string, int> kv in cd) total += kv.Value;
            if (total != 53) return 16;
            return 200;
        }
    }
}
