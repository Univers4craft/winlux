// Témoin de callvirt sur une méthode non virtuelle (#2785).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 3) de la première vérification qui échoue.
// C# émet callvirt même pour une méthode non virtuelle ; le moteur prenait
// alors une méthode virtuelle de même nom d'une classe de base
// (Collection<T>.Contains à la place de KeyedCollection.Contains(TKey)).
using System;

namespace Temoins
{
    interface IContient { bool Contient(int x); }

    // Contient(int) implémente l'interface : virtuelle (final) dans l'IL.
    class Base : IContient
    {
        public bool Contient(int x) { return false; }
        public string this[int i] { get { return "entier"; } }
        public virtual string Nom() { return "base"; }
    }

    class Cles<K> : Base
    {
        public bool Contient(K cle) { return true; }
        public string this[K cle] { get { return "clé"; } }
    }

    class Mots : Cles<string>
    {
        public override string Nom() { return "mots"; }
    }

    static class AppelNonVirtuel
    {
        static int Main()
        {
            Mots m = new Mots();
            // 1. La méthode non virtuelle du type déclarant est appelée.
            if (!m.Contient("b") || m["b"] != "clé") return 1;
            // 2. La méthode de la base reste joignable par son propre type.
            if (m.Contient(1) || m[0] != "entier" || ((IContient)m).Contient(1)) return 2;
            // 3. Une vraie redéfinition est toujours choisie.
            Base b = m;
            if (b.Nom() != "mots") return 3;
            return 200;
        }
    }
}
