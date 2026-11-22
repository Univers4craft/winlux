// Témoin de Collection<T> et KeyedCollection<TKey, TItem> dans la mscorlib
// Winlux (#2783).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 7) de la première vérification qui échoue.
// Le téléchargeur de Paint.NET mourait sur ICollection`1::get_Count d'une
// classe de Newtonsoft.Json dérivée de Collection<T>.
using System;
using System.Collections;
using System.Collections.Generic;
using System.Collections.ObjectModel;

namespace Temoins
{
    // Comme JPropertyKeyedCollection : redéfinit les points d'extension.
    class Suivie : Collection<string>
    {
        public int Inserts, Retraits, Remplacements, Vidages;
        protected override void InsertItem(int index, string item) { Inserts++; base.InsertItem(index, item); }
        protected override void RemoveItem(int index) { Retraits++; base.RemoveItem(index); }
        protected override void SetItem(int index, string item) { Remplacements++; base.SetItem(index, item); }
        protected override void ClearItems() { Vidages++; base.ClearItems(); }
        public IList<string> Interne { get { return Items; } }
    }

    class Prop { public string Nom; public int Valeur; public Prop(string n, int v) { Nom = n; Valeur = v; } }

    // Comme JsonPropertyCollection.
    class Props : KeyedCollection<string, Prop>
    {
        public Props() : base(EqualityComparer<string>.Default) { }
        public Props(int seuil) : base(null, seuil) { }
        protected override string GetKeyForItem(Prop p) { return p.Nom; }
        public bool AUnDictionnaire { get { return Dictionary != null; } }
        public void Renommer(Prop p, string nom) { ChangeItemKey(p, nom); p.Nom = nom; }
    }

    static class CollectionsModele
    {
        static int Compte<T>(ICollection<T> c) { return c.Count; }

        static int Main()
        {
            Suivie s = new Suivie();
            s.Add("a"); s.Add("c"); s.Insert(1, "b");

            // 1. Contenu et Count, y compris par les interfaces (le cas mesuré).
            if (s.Count != 3 || s[1] != "b" || Compte<string>(s) != 3 || ((ICollection)s).Count != 3
                || ((IReadOnlyCollection<string>)s).Count != 3 || s.IndexOf("c") != 2 || !s.Contains("a")) return 1;

            // 2. Toute modification passe par les points d'extension.
            s[0] = "A"; s.Remove("b"); s.RemoveAt(0); ((IList)s).Add("d"); ((ICollection<string>)s).Add("e");
            if (s.Inserts != 5 || s.Remplacements != 1 || s.Retraits != 2 || s.Count != 3 || s.Interne.Count != 3) return 2;
            s.Clear();
            if (s.Vidages != 1 || s.Count != 0 || s.Remove("x")) return 2;

            // 3. Une liste donnée est enveloppée, pas copiée ; lecture seule respectée.
            List<int> l = new List<int>(); l.Add(1);
            Collection<int> e = new Collection<int>(l);
            l.Add(2);
            if (e.Count != 2) return 3;
            Collection<int> ro = new Collection<int>(l.AsReadOnly());
            try { ro.Add(3); return 3; } catch (NotSupportedException) { }
            try { e.Insert(5, 1); return 3; } catch (ArgumentOutOfRangeException) { }

            // 4. Énumération et copie.
            int n = 0; foreach (int x in e) n += x;
            int[] t = new int[3]; e.CopyTo(t, 1);
            if (n != 3 || t[1] != 1 || t[2] != 2) return 4;

            // 5. KeyedCollection : accès par clé, Contains, Remove.
            Props p = new Props();
            Prop pa = new Prop("a", 1);
            p.Add(pa); p.Add(new Prop("b", 2));
            if (p["b"].Valeur != 2 || !p.Contains("a") || p.Contains("z") || p[0] != pa || !p.AUnDictionnaire) return 5;
            try { Prop z = p["z"]; return 5; } catch (KeyNotFoundException) { }
            if (!p.Remove("a") || p.Count != 1 || p.Contains("a")) return 5;

            // 6. Clé en double refusée, renommage suivi.
            try { p.Add(new Prop("b", 3)); return 6; } catch (ArgumentException) { }
            Prop pb = p[0];
            p.Renommer(pb, "c");
            if (p.Contains("b") || !p.Contains("c") || p["c"] != pb) return 6;

            // 7. Seuil : pas de dictionnaire avant de le dépasser, -1 jamais.
            Props q = new Props(2);
            q.Add(new Prop("a", 1)); q.Add(new Prop("b", 2));
            if (q.AUnDictionnaire || q["b"].Valeur != 2) return 7;
            q.Add(new Prop("c", 3));
            if (!q.AUnDictionnaire || q["a"].Valeur != 1 || q["c"].Valeur != 3) return 7;
            Props r = new Props(-1);
            r.Add(new Prop("a", 1));
            if (r.AUnDictionnaire || r["a"].Valeur != 1) return 7;
            r.Clear();
            if (r.Count != 0 || r.Contains("a")) return 7;

            return 200;
        }
    }
}
